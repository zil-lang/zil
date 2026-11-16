//! Literal-result candidates from the actual original lexical body. This
//! navigation selects a Static obligation; it is never a storage proof and
//! never infers a receiver, input class, or callable ABI from a type spelling.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Candidate {
    Other,
    Text,
    TextClosure,
}
struct Flow {
    result: Option<Candidate>,
    fallthrough: bool,
}
struct Walk<'b, 'l> {
    budget: &'b mut Budget<'l>,
}
impl Walk<'_, '_> {
    fn bind<'a>(
        &mut self,
        env: &mut Vec<(&'a str, Candidate)>,
        name: &'a str,
        value: Candidate,
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
        env: &mut Vec<(&'a str, Candidate)>,
        value: Candidate,
        depth: usize,
    ) -> R<()> {
        self.budget.node(pattern.span, depth)?;
        match &pattern.kind {
            s::PatternKind::Name(name) => self.bind(env, name, value, pattern.span)?,
            s::PatternKind::Reference { pattern, .. }
            | s::PatternKind::ErrorArm { pattern, .. } => {
                self.shadow(pattern, env, Candidate::Other, depth + 1)?
            }
            s::PatternKind::Tuple(fields)
            | s::PatternKind::Constructor {
                arguments: fields, ..
            } => {
                for field in fields {
                    self.shadow(field, env, Candidate::Other, depth + 1)?;
                }
            }
            s::PatternKind::RecordConstructor { fields, .. } => {
                for field in fields {
                    self.shadow(&field.pattern, env, Candidate::Other, depth + 1)?;
                }
            }
            s::PatternKind::Wildcard | s::PatternKind::Literal(_) => {}
        }
        Ok(())
    }
    fn expression<'a>(
        &mut self,
        value: &'a s::Expr,
        env: &mut Vec<(&'a str, Candidate)>,
        depth: usize,
    ) -> R<Candidate> {
        self.budget.node(value.span, depth)?;
        Ok(match &value.kind {
            s::ExprKind::Literal(s::Literal::String(_)) => Candidate::Text,
            s::ExprKind::Name(name) => {
                let mut selected = Candidate::Other;
                for (binding, candidate) in env.iter().rev() {
                    self.budget.node(value.span, depth + 1)?;
                    self.budget.text(name, value.span)?;
                    self.budget.text(binding, value.span)?;
                    if *binding == name.as_str() {
                        selected = *candidate;
                        break;
                    }
                }
                selected
            }
            s::ExprKind::Group(inner) | s::ExprKind::Ascribed { value: inner, .. } => {
                self.expression(inner, env, depth + 1)?
            }
            s::ExprKind::Slice { object, .. } => {
                if self.expression(object, env, depth + 1)? == Candidate::Text {
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
                if left == right {
                    left
                } else {
                    Candidate::Other
                }
            }
            s::ExprKind::Closure { params, body, .. } => {
                let saved = env.len();
                for parameter in params {
                    self.bind(env, &parameter.name, Candidate::Other, parameter.span)?;
                }
                let result = self.arrow(body, env, depth + 1)?;
                env.truncate(saved);
                if result == Some(Candidate::Text) {
                    Candidate::TextClosure
                } else {
                    Candidate::Other
                }
            }
            _ => Candidate::Other,
        })
    }
    fn merge(result: &mut Option<Candidate>, next: Option<Candidate>) {
        if let Some(next) = next {
            *result = Some(match *result {
                None => next,
                Some(old) if old == next => old,
                _ => Candidate::Other,
            });
        }
    }
    fn arrow<'a>(
        &mut self,
        body: &'a s::ArrowBody,
        env: &mut Vec<(&'a str, Candidate)>,
        depth: usize,
    ) -> R<Option<Candidate>> {
        match body {
            s::ArrowBody::Expression(value) => Ok(Some(self.expression(value, env, depth + 1)?)),
            s::ArrowBody::Block(body) => Ok(self.block(body, env, depth + 1)?.result),
        }
    }
    fn block<'a>(
        &mut self,
        body: &'a s::Block,
        env: &mut Vec<(&'a str, Candidate)>,
        depth: usize,
    ) -> R<Flow> {
        self.budget.node(body.span, depth)?;
        let saved = env.len();
        let mut result = None;
        let mut fallthrough = true;
        for statement in &body.statements {
            if !fallthrough {
                break;
            }
            let next = self.statement(statement, env, depth + 1)?;
            Self::merge(&mut result, next.result);
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
        env: &mut Vec<(&'a str, Candidate)>,
        depth: usize,
    ) -> R<()> {
        if let s::Condition::Let { pattern, .. } = condition {
            self.shadow(pattern, env, Candidate::Other, depth + 1)?;
        }
        Ok(())
    }
    fn statement<'a>(
        &mut self,
        statement: &'a s::Stmt,
        env: &mut Vec<(&'a str, Candidate)>,
        depth: usize,
    ) -> R<Flow> {
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
                Self::merge(&mut result, left.result);
                env.truncate(saved);
                if let Some(branch) = else_body {
                    let right = match branch {
                        s::ElseBranch::Block(body) => self.block(body, env, depth + 1)?,
                        s::ElseBranch::If(statement) => {
                            self.statement(statement, env, depth + 1)?
                        }
                    };
                    Self::merge(&mut result, right.result);
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
            s::StmtKind::Match { arms, .. } => {
                fallthrough = arms.is_empty();
                for arm in arms {
                    let saved = env.len();
                    self.shadow(&arm.pattern, env, Candidate::Other, depth + 1)?;
                    // Expression arms are statement expressions, not returns.
                    if let s::ArrowBody::Block(body) = &arm.body {
                        let inner = self.block(body, env, depth + 1)?;
                        Self::merge(&mut result, inner.result);
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
    Ok(result == Some(Candidate::TextClosure))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn candidate_tracks_real_literals_all_returns_and_lexical_shadowing() {
        for (text, expected) in [
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
