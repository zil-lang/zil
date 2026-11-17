//! Literal-result candidates from the actual original lexical body. This
//! navigation selects a Static obligation; it is never a storage proof and
//! never infers a receiver, input class, or callable ABI from a type spelling.
use super::*;

enum Candidate<'a> {
    Other,
    Text,
    TextClosure,
    Tuple(Vec<Candidate<'a>>),
    Record(Vec<(&'a str, Candidate<'a>)>),
}
struct Flow<'a> {
    result: Option<Candidate<'a>>,
    fallthrough: bool,
}
struct Walk<'b, 'l> {
    budget: &'b mut Budget<'l>,
}
impl Walk<'_, '_> {
    fn reserve<T>(&mut self, values: &mut Vec<T>, additional: usize, span: Span) -> R<()> {
        let needed = values
            .len()
            .checked_add(additional)
            .ok_or_else(|| error(span, "static aggregate length overflow"))?;
        if needed > values.capacity() {
            let bytes = (needed - values.capacity())
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| error(span, "static aggregate allocation overflow"))?;
            self.budget.lineage_bytes(bytes, span)?;
            values
                .try_reserve_exact(additional)
                .map_err(|_| error(span, "static aggregate allocation failed"))?;
        }
        Ok(())
    }
    fn vector<T>(&mut self, count: usize, span: Span) -> R<Vec<T>> {
        let mut result = Vec::new();
        self.reserve(&mut result, count, span)?;
        Ok(result)
    }
    fn copy<'a>(&mut self, value: &Candidate<'a>, span: Span, depth: usize) -> R<Candidate<'a>> {
        self.budget.node(span, depth)?;
        Ok(match value {
            Candidate::Other => Candidate::Other,
            Candidate::Text => Candidate::Text,
            Candidate::TextClosure => Candidate::TextClosure,
            Candidate::Tuple(fields) => {
                let mut values = self.vector(fields.len(), span)?;
                for field in fields {
                    values.push(self.copy(field, span, depth + 1)?);
                }
                Candidate::Tuple(values)
            }
            Candidate::Record(fields) => {
                let mut values = self.vector(fields.len(), span)?;
                for (name, field) in fields {
                    values.push((*name, self.copy(field, span, depth + 1)?));
                }
                Candidate::Record(values)
            }
        })
    }
    fn lookup<'a>(
        &mut self,
        env: &[(&'a str, Candidate<'a>)],
        name: &str,
        span: Span,
        depth: usize,
    ) -> R<Candidate<'a>> {
        for (binding, candidate) in env.iter().rev() {
            self.budget.node(span, depth)?;
            self.budget.text(name, span)?;
            self.budget.text(binding, span)?;
            if *binding == name {
                return self.copy(candidate, span, depth + 1);
            }
        }
        Ok(Candidate::Other)
    }
    fn named<'a>(
        &mut self,
        value: &Candidate<'a>,
        name: &str,
        span: Span,
        depth: usize,
    ) -> R<Candidate<'a>> {
        self.budget.node(span, depth)?;
        if let Candidate::Record(fields) = value {
            for (field, value) in fields {
                self.budget.text(field, span)?;
                self.budget.text(name, span)?;
                if *field == name {
                    return self.copy(value, span, depth + 1);
                }
            }
        }
        Ok(Candidate::Other)
    }
    /// Join origins, not types or classes. Each projected member must have
    /// a literal origin on both paths, independently of its siblings.
    fn join<'a>(
        &mut self,
        left: Candidate<'a>,
        right: Candidate<'a>,
        span: Span,
        depth: usize,
    ) -> R<Candidate<'a>> {
        self.budget.node(span, depth)?;
        Ok(match (left, right) {
            (Candidate::Text, Candidate::Text) => Candidate::Text,
            (Candidate::TextClosure, Candidate::TextClosure) => Candidate::TextClosure,
            (Candidate::Tuple(mut left), Candidate::Tuple(right)) if left.len() == right.len() => {
                for (left, right) in left.iter_mut().zip(right) {
                    *left = self.join(
                        std::mem::replace(left, Candidate::Other),
                        right,
                        span,
                        depth + 1,
                    )?;
                }
                Candidate::Tuple(left)
            }
            (Candidate::Record(mut left), Candidate::Record(mut right))
                if left.len() == right.len() =>
            {
                for (name, left) in &mut left {
                    let mut selected = None;
                    for (other, right) in &mut right {
                        self.budget.text(name, span)?;
                        self.budget.text(other, span)?;
                        if *name == *other {
                            selected = Some(std::mem::replace(right, Candidate::Other));
                            break;
                        }
                    }
                    let Some(right) = selected else {
                        return Ok(Candidate::Other);
                    };
                    *left = self.join(
                        std::mem::replace(left, Candidate::Other),
                        right,
                        span,
                        depth + 1,
                    )?;
                }
                Candidate::Record(left)
            }
            _ => Candidate::Other,
        })
    }
    fn bind<'a>(
        &mut self,
        env: &mut Vec<(&'a str, Candidate<'a>)>,
        name: &'a str,
        value: Candidate<'a>,
        span: Span,
    ) -> R<()> {
        self.budget.text(name, span)?;
        if env.len() == env.capacity() {
            let growth = env.capacity().max(4);
            self.budget.lineage_bytes(
                growth
                    .checked_mul(std::mem::size_of::<(&str, Candidate)>())
                    .ok_or_else(|| error(span, "static result lexical directory overflow"))?,
                span,
            )?;
            env.try_reserve_exact(growth)
                .map_err(|_| error(span, "static result lexical allocation failed"))?;
        }
        env.push((name, value));
        Ok(())
    }
    fn shadow<'a>(
        &mut self,
        pattern: &'a s::Pattern,
        env: &mut Vec<(&'a str, Candidate<'a>)>,
        value: Candidate<'a>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(pattern.span, depth)?;
        match &pattern.kind {
            s::PatternKind::Name(name) => self.bind(env, name, value, pattern.span)?,
            s::PatternKind::Reference { pattern, .. }
            | s::PatternKind::ErrorArm { pattern, .. } => {
                self.shadow(pattern, env, Candidate::Other, depth + 1)?
            }
            s::PatternKind::Tuple(fields) => {
                let mut values = match value {
                    Candidate::Tuple(values) if values.len() == fields.len() => {
                        Some(values.into_iter())
                    }
                    _ => None,
                };
                for field in fields {
                    let child = values
                        .as_mut()
                        .and_then(Iterator::next)
                        .unwrap_or(Candidate::Other);
                    self.shadow(field, env, child, depth + 1)?;
                }
            }
            s::PatternKind::Constructor {
                arguments: fields, ..
            } => {
                for field in fields {
                    self.shadow(field, env, Candidate::Other, depth + 1)?;
                }
            }
            s::PatternKind::RecordConstructor { fields, .. } => {
                for field in fields {
                    let child = self.named(&value, &field.name, field.span, depth + 1)?;
                    self.shadow(&field.pattern, env, child, depth + 1)?;
                }
            }
            s::PatternKind::Wildcard | s::PatternKind::Literal(_) => {}
        }
        Ok(())
    }
    fn expression<'a>(
        &mut self,
        value: &'a s::Expr,
        env: &mut Vec<(&'a str, Candidate<'a>)>,
        depth: usize,
    ) -> R<Candidate<'a>> {
        self.budget.node(value.span, depth)?;
        Ok(match &value.kind {
            s::ExprKind::Literal(s::Literal::String(_)) => Candidate::Text,
            s::ExprKind::Name(name) => self.lookup(env, name, value.span, depth + 1)?,
            s::ExprKind::Tuple(fields) => {
                let mut children = self.vector(fields.len(), value.span)?;
                for field in fields {
                    children.push(self.expression(field, env, depth + 1)?);
                }
                Candidate::Tuple(children)
            }
            s::ExprKind::Record { base, fields, .. } => {
                let mut children = match base {
                    Some(base) => match self.expression(base, env, depth + 1)? {
                        Candidate::Record(children) => children,
                        _ => return Ok(Candidate::Other),
                    },
                    None => Vec::new(),
                };
                self.reserve(&mut children, fields.len(), value.span)?;
                for (ordinal, field) in fields.iter().enumerate() {
                    // Duplicate field syntax is never used to select a source.
                    for previous in &fields[..ordinal] {
                        self.budget.text(&previous.name, field.span)?;
                        self.budget.text(&field.name, field.span)?;
                        if previous.name == field.name {
                            return Ok(Candidate::Other);
                        }
                    }
                    let child = match &field.value {
                        Some(value) => self.expression(value, env, depth + 1)?,
                        None => self.lookup(env, &field.name, field.span, depth + 1)?,
                    };
                    let mut replaced = false;
                    for (name, previous) in &mut children {
                        self.budget.text(name, field.span)?;
                        self.budget.text(&field.name, field.span)?;
                        if *name == field.name.as_str() {
                            *previous = self.copy(&child, field.span, depth + 1)?;
                            replaced = true;
                            break;
                        }
                    }
                    if !replaced {
                        children.push((field.name.as_str(), child));
                    }
                }
                Candidate::Record(children)
            }
            s::ExprKind::Member { object, member } => {
                let object = self.expression(object, env, depth + 1)?;
                match member {
                    s::MemberName::TupleIndex(index) => match object {
                        Candidate::Tuple(mut fields) if (*index as usize) < fields.len() => {
                            std::mem::replace(&mut fields[*index as usize], Candidate::Other)
                        }
                        _ => Candidate::Other,
                    },
                    s::MemberName::Name(name) | s::MemberName::Constrained { name, .. } => {
                        self.named(&object, name, value.span, depth + 1)?
                    }
                }
            }
            s::ExprKind::Group(inner) | s::ExprKind::Ascribed { value: inner, .. } => {
                self.expression(inner, env, depth + 1)?
            }
            s::ExprKind::Slice { object, .. } => {
                if matches!(self.expression(object, env, depth + 1)?, Candidate::Text) {
                    Candidate::Text
                } else {
                    Candidate::Other
                }
            }
            s::ExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                let left = self.expression(then_value, env, depth + 1)?;
                let right = self.expression(else_value, env, depth + 1)?;
                self.join(left, right, value.span, depth + 1)?
            }
            s::ExprKind::Closure { params, body, .. } => {
                let saved = env.len();
                for parameter in params {
                    self.bind(env, &parameter.name, Candidate::Other, parameter.span)?;
                }
                let result = self.arrow(body, env, depth + 1)?;
                env.truncate(saved);
                if matches!(result, Some(Candidate::Text)) {
                    Candidate::TextClosure
                } else {
                    Candidate::Other
                }
            }
            _ => Candidate::Other,
        })
    }
    fn merge<'a>(
        &mut self,
        result: &mut Option<Candidate<'a>>,
        next: Option<Candidate<'a>>,
        span: Span,
        depth: usize,
    ) -> R<()> {
        if let Some(next) = next {
            *result = Some(match result.take() {
                None => next,
                Some(old) => self.join(old, next, span, depth + 1)?,
            });
        }
        Ok(())
    }
    fn arrow<'a>(
        &mut self,
        body: &'a s::ArrowBody,
        env: &mut Vec<(&'a str, Candidate<'a>)>,
        depth: usize,
    ) -> R<Option<Candidate<'a>>> {
        match body {
            s::ArrowBody::Expression(value) => Ok(Some(self.expression(value, env, depth + 1)?)),
            s::ArrowBody::Block(body) => Ok(self.block(body, env, depth + 1)?.result),
        }
    }
    fn block<'a>(
        &mut self,
        body: &'a s::Block,
        env: &mut Vec<(&'a str, Candidate<'a>)>,
        depth: usize,
    ) -> R<Flow<'a>> {
        self.budget.node(body.span, depth)?;
        let saved = env.len();
        let mut result = None;
        let mut fallthrough = true;
        for statement in &body.statements {
            if !fallthrough {
                break;
            }
            let next = self.statement(statement, env, depth + 1)?;
            self.merge(&mut result, next.result, statement.span, depth + 1)?;
            fallthrough = next.fallthrough;
        }
        env.truncate(saved);
        Ok(Flow {
            result,
            fallthrough,
        })
    }
    fn condition<'a>(
        &mut self,
        condition: &'a s::Condition,
        env: &mut Vec<(&'a str, Candidate<'a>)>,
        depth: usize,
    ) -> R<()> {
        if let s::Condition::Let { pattern, value } = condition {
            let actual = self.expression(value, env, depth + 1)?;
            self.shadow(pattern, env, actual, depth + 1)?;
        }
        Ok(())
    }
    fn statement<'a>(
        &mut self,
        statement: &'a s::Stmt,
        env: &mut Vec<(&'a str, Candidate<'a>)>,
        depth: usize,
    ) -> R<Flow<'a>> {
        self.budget.node(statement.span, depth)?;
        let mut result = None;
        let mut fallthrough = true;
        match &statement.kind {
            s::StmtKind::Binding {
                mutable,
                pattern,
                value,
                ..
            } => {
                let actual = self.expression(value, env, depth + 1)?;
                self.shadow(
                    pattern,
                    env,
                    if *mutable { Candidate::Other } else { actual },
                    depth + 1,
                )?;
            }
            s::StmtKind::Return(value) => {
                result = Some(match value {
                    Some(value) => self.expression(value, env, depth + 1)?,
                    None => Candidate::Other,
                });
                fallthrough = false;
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let saved = env.len();
                self.condition(condition, env, depth + 1)?;
                let left = self.block(then_body, env, depth + 1)?;
                self.merge(&mut result, left.result, statement.span, depth + 1)?;
                env.truncate(saved);
                if let Some(branch) = else_body {
                    let right = match branch {
                        s::ElseBranch::Block(body) => self.block(body, env, depth + 1)?,
                        s::ElseBranch::If(statement) => {
                            self.statement(statement, env, depth + 1)?
                        }
                    };
                    self.merge(&mut result, right.result, statement.span, depth + 1)?;
                    fallthrough = left.fallthrough || right.fallthrough;
                }
            }
            s::StmtKind::While { condition, body } => {
                let saved = env.len();
                self.condition(condition, env, depth + 1)?;
                result = self.block(body, env, depth + 1)?.result;
                env.truncate(saved);
            }
            s::StmtKind::For { pattern, body, .. } => {
                let saved = env.len();
                self.shadow(pattern, env, Candidate::Other, depth + 1)?;
                result = self.block(body, env, depth + 1)?.result;
                env.truncate(saved);
            }
            s::StmtKind::With { name, body, .. } => {
                let saved = env.len();
                self.bind(env, name, Candidate::Other, statement.span)?;
                let inner = self.block(body, env, depth + 1)?;
                result = inner.result;
                fallthrough = inner.fallthrough;
                env.truncate(saved);
            }
            s::StmtKind::Loop(body) => {
                // A break may continue after the loop. Keep this conservative
                // without evaluating loop conditions or inventing reachability.
                result = self.block(body, env, depth + 1)?.result;
            }
            s::StmtKind::Unsafe(body) => {
                let inner = self.block(body, env, depth + 1)?;
                result = inner.result;
                fallthrough = inner.fallthrough;
            }
            s::StmtKind::Match { value, arms } => {
                let actual = self.expression(value, env, depth + 1)?;
                fallthrough = arms.is_empty();
                for arm in arms {
                    let saved = env.len();
                    let selected = self.copy(&actual, statement.span, depth + 1)?;
                    self.shadow(&arm.pattern, env, selected, depth + 1)?;
                    // Expression arms are statement expressions, not returns.
                    if let s::ArrowBody::Block(body) = &arm.body {
                        let inner = self.block(body, env, depth + 1)?;
                        self.merge(&mut result, inner.result, statement.span, depth + 1)?;
                        fallthrough |= inner.fallthrough;
                    } else {
                        fallthrough = true;
                    }
                    env.truncate(saved);
                }
            }
            s::StmtKind::Meta(_) => result = Some(Candidate::Other),
            s::StmtKind::Break | s::StmtKind::Continue => fallthrough = false,
            s::StmtKind::Assign { .. }
            | s::StmtKind::Expression(_)
            | s::StmtKind::Yield(_)
            | s::StmtKind::Defer(_) => {}
        }
        Ok(Flow {
            result,
            fallthrough,
        })
    }
}

/// Only an omitted outer callable-result life needs the literal-origin walk.
/// This is a query filter, not the qualification of that result as Static.
pub(super) fn elided_result(bound: &s::TypeRef, budget: &mut Budget<'_>, depth: usize) -> R<bool> {
    budget.node(bound.span, depth)?;
    match &bound.kind {
        s::TypeKind::Group(inner) | s::TypeKind::ForLifetime { inner, .. } => {
            elided_result(inner, budget, depth + 1)
        }
        s::TypeKind::Named { path, args } if matches!(path.as_slice(), [name] if matches!(name.as_str(), "Fn" | "FnMut" | "FnOnce")) =>
        {
            let [s::GenericArg::Type(_), s::GenericArg::Type(result)] = args.as_slice() else {
                return Ok(false);
            };
            let mut result = result;
            while let s::TypeKind::Group(inner) = &result.kind {
                budget.node(result.span, depth + 1)?;
                result = inner;
            }
            Ok(matches!(
                result.kind,
                s::TypeKind::Reference {
                    mutable: false,
                    lifetime: None,
                    ..
                }
            ))
        }
        _ => Ok(false),
    }
}

pub(super) fn opaque_result(function: &s::Function, budget: &mut Budget<'_>) -> R<bool> {
    let mut walk = Walk { budget };
    let mut env = Vec::new();
    for parameter in &function.params {
        walk.bind(&mut env, &parameter.name, Candidate::Other, parameter.span)?;
    }
    let result = match &function.body {
        s::FunctionBody::Block(body) => walk.block(body, &mut env, 0)?.result,
        s::FunctionBody::Expression(value) => Some(walk.expression(value, &mut env, 0)?),
        s::FunctionBody::Declaration => None,
    };
    Ok(matches!(result, Some(Candidate::TextClosure)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn candidate_tracks_real_literals_all_returns_and_lexical_shadowing() {
        for (text, expected) in [
            ("fn make()->impl FnOnce()->&[u8]{let pair=(\"A\",7i32);return move()=>{return pair.0};}", true),
            ("fn make()->impl FnOnce()->&[u8]{let pair=((7,\"A\"),9);let alias=pair;return move()=>{let (inner,_)=alias;return inner.1};}", true),
            ("fn make(flag:bool,input:&[u8])->impl Fn()->&[u8]{let pair=if flag{(\"A\",input)}else{(\"B\",input)};return move()=>pair.0;}", true),
            ("fn make(flag:bool,input:&[u8])->impl Fn()->&[u8]{let pair=if flag{(\"A\",input)}else{(input,\"B\")};return move()=>pair.0;}", false),
            ("fn make(input:&[u8])->impl Fn()->&[u8]{let pair=(\"A\",input);return move()=>pair.1;}", false),
            ("fn make()->impl Fn()->&[u8]{var pair=(\"A\",7);return move()=>pair.0;}", false),
            ("fn make(input:&[u8])->impl Fn()->&[u8]{let pair=(\"A\",7);return move()=>{let pair=(input,7);return pair.0};}", false),
            ("fn make()->impl Fn()->&[u8]{let pair=(\"A\",7);return move()=>pair.missing;}", false),
            ("fn make()->impl Fn()->&[u8]{let pair=(\"A\",7);return move()=>pair.2;}", false),
            ("fn make()->impl Fn()->&[u8]{let pair=(unknown(),7);return move()=>pair.0;}", false),
            ("fn make()->impl Fn()->&[u8]{let pair=(move()=>\"A\",7);return pair.0;}", true),
            ("fn make(flag:bool,input:&[u8])->impl Fn()->&[u8]{let pair=(\"A\",7);return move()=>{if flag{return pair.0;}else{return \"B\";}return input;};}", true),
            ("fn make(input:&[u8])->impl Fn()->&[u8]{let pair=(\"A\",7);return move()=>{return pair.0;return input;};}", true),
            ("fn make()->impl Fn()->&[u8]{let text=\"A\";let pair=Pair{text,tag:7};let alias=pair;return move()=>alias.text;}", true),
            ("fn make(input:&[u8])->impl Fn()->&[u8]{let pair=Pair{text:input,tag:7};return move()=>pair.text;}", false),
            ("fn make(input:&[u8])->impl Fn()->&[u8]{let pair=Pair{text:\"A\",tag:7};return move()=>{let Pair{text,tag:_}=pair;return text;};}", true),
            ("fn make()->impl Fn()->&[u8]{let pair=(\"A\",7);return move()=>{match pair{(text,_)=>{return text;}}};}", true),
            (
                "fn make(input:&[u8])->impl Fn()->&[u8]{return move()=>{return \"abc\";return input;};}",
                true,
            ),
            (
                "fn make(flag:bool,input:&[u8])->impl Fn()->&[u8]{return move()=>{if flag{return \"abc\";}else{return \"def\";}return input;};}",
                true,
            ),
            (
                "fn make()->impl Fn()->&[u8]{let text=\"abc\";let alias=text;return move()=>alias;}",
                true,
            ),
            (
                "fn make(flag:bool,input:&[u8])->impl Fn()->&[u8]{let text=\"abc\";return move()=>{if flag{return text;}else{return input;}};}",
                false,
            ),
            (
                "fn make(input:&[u8])->impl Fn()->&[u8]{let text=\"abc\";return move()=>{let text=input;return text;};}",
                false,
            ),
            (
                "fn make()->impl Fn()->&[u8]{var text=\"abc\";return move()=>text;}",
                false,
            ),
            (
                include_str!("../../../tests/fixtures/returned-owning-holders/forward-lending.zil"),
                false,
            ),
            (
                include_str!(
                    "../../../tests/fixtures/returned-owning-holders/fnonce-exclusive-capture-take-inferred-result.zil"
                ),
                false,
            ),
        ] {
            let source = SourceUnit::new(FileId(0), ModuleId(0), "literal-origin.zil", text);
            let module = zil_syntax::parse(&source).into_result().unwrap();
            let function = module
                .items
                .iter()
                .find_map(|item| match &item.kind {
                    s::ItemKind::Function(function) if function.name == "make" => Some(function),
                    _ => None,
                })
                .unwrap();
            let limits = Limits::default();
            let mut budget = Budget::new(&limits);
            assert_eq!(
                opaque_result(function, &mut budget).unwrap(),
                expected,
                "{text}"
            );
        }
    }
}
