//! Result candidates from the original lexical body. A candidate selects a
//! header obligation. The executable borrow proofs still establish its roots.
use super::*;

#[derive(PartialEq)]
enum Candidate<'a> {
    Other,
    Text,
    TextClosure,
    TakenInput(usize),
    TransferClosure(usize),
    Input(usize),
    Inputs(Vec<usize>),
    Tuple(Vec<Candidate<'a>>),
    Record(Vec<(&'a str, Candidate<'a>)>),
}
type Bindings<'a> = Vec<(&'a str, Candidate<'a>, bool)>;
type Saved<'a> = Vec<(usize, Candidate<'a>)>;
struct LoopExits<'a> {
    prefix: usize,
    breaks: Option<Saved<'a>>,
    backedges: Option<Saved<'a>>,
}
struct Flow<'a> {
    result: Option<Candidate<'a>>,
    fallthrough: bool,
}
struct Walk<'b, 'l> {
    budget: &'b mut Budget<'l>,
    inputs: bool,
    rebindings: bool,
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
            Candidate::TakenInput(i) => Candidate::TakenInput(*i),
            Candidate::TransferClosure(i) => Candidate::TransferClosure(*i),
            Candidate::Input(index) => Candidate::Input(*index),
            Candidate::Inputs(inputs) => {
                let mut copy = self.vector(inputs.len(), span)?;
                self.budget.identity_work(inputs.len(), span)?;
                copy.extend_from_slice(inputs);
                Candidate::Inputs(copy)
            }
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
        env: &[(&'a str, Candidate<'a>, bool)],
        name: &str,
        span: Span,
        depth: usize,
    ) -> R<Candidate<'a>> {
        for (binding, candidate, _) in env.iter().rev() {
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
            (Candidate::Input(left), Candidate::Input(right)) if left == right => Candidate::Input(left),
            (Candidate::Input(index), Candidate::Text) | (Candidate::Text, Candidate::Input(index)) => {
                let mut sources = self.vector(1, span)?;
                sources.push(index);
                Candidate::Inputs(sources)
            }
            (union @ Candidate::Inputs(_), Candidate::Text) | (Candidate::Text, union @ Candidate::Inputs(_)) => union,
            (left @ (Candidate::Input(_) | Candidate::Inputs(_)),
             right @ (Candidate::Input(_) | Candidate::Inputs(_))) => {
                let mut inputs = Vec::new();
                for source in [left, right] {
                    let row = match &source {
                        Candidate::Input(index) => std::slice::from_ref(index),
                        Candidate::Inputs(row) => row.as_slice(),
                        _ => unreachable!(),
                    };
                    for index in row {
                        self.budget.identity_work(inputs.len().saturating_add(1), span)?;
                        if !inputs.contains(index) {
                            self.reserve(&mut inputs, 1, span)?;
                            inputs.push(*index);
                        }
                    }
                }
                Candidate::Inputs(inputs)
            }
            (Candidate::Text, Candidate::Text) => Candidate::Text,
            (Candidate::TextClosure, Candidate::TextClosure) => Candidate::TextClosure,
            (Candidate::TakenInput(a), Candidate::TakenInput(b)) if a == b => Candidate::TakenInput(a),
            (Candidate::TransferClosure(a), Candidate::TransferClosure(b)) if a == b => Candidate::TransferClosure(a),
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
        env: &mut Bindings<'a>,
        name: &'a str,
        value: Candidate<'a>,
        span: Span,
    ) -> R<()> {
        self.budget.text(name, span)?;
        if env.len() == env.capacity() {
            let growth = env.capacity().max(4);
            self.budget.lineage_bytes(
                growth
                    .checked_mul(std::mem::size_of::<(&str, Candidate, bool)>())
                    .ok_or_else(|| error(span, "static result lexical directory overflow"))?,
                span,
            )?;
            env.try_reserve_exact(growth)
                .map_err(|_| error(span, "static result lexical allocation failed"))?;
        }
        env.push((name, value, false));
        Ok(())
    }
    fn saved_bindings<'a>(
        &mut self,
        env: &Bindings<'a>,
        span: Span,
        depth: usize,
    ) -> R<Vec<(usize, Candidate<'a>)>> {
        let mut saved = Vec::new();
        if self.rebindings {
            self.budget.identity_work(env.len(), span)?;
            for (index, (_, value, mutable)) in env.iter().enumerate() {
                if *mutable {
                    self.reserve(&mut saved, 1, span)?;
                    saved.push((index, self.copy(value, span, depth + 1)?));
                }
            }
        }
        Ok(saved)
    }
    fn invalidate_mutable(&mut self, env: &mut Bindings<'_>, span: Span) -> R<()> {
        if self.rebindings {
            self.budget.identity_work(env.len(), span)?;
            for (_, source, mutable) in env.iter_mut() {
                if *mutable {
                    *source = Candidate::Other;
                }
            }
        }
        Ok(())
    }
    fn shadow<'a>(
        &mut self,
        pattern: &'a s::Pattern,
        env: &mut Bindings<'a>,
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
        env: &mut Bindings<'a>,
        depth: usize,
    ) -> R<Candidate<'a>> {
        self.budget.node(value.span, depth)?;
        Ok(match &value.kind {
            s::ExprKind::Literal(s::Literal::String(_)) => Candidate::Text,
            s::ExprKind::Literal(_) => Candidate::Other,
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
            s::ExprKind::Slice { object, start, end } => {
                let source = self.expression(object, env, depth + 1)?;
                if self.rebindings {
                    for bound in [start, end].into_iter().flatten() {
                        self.expression(bound, env, depth + 1)?;
                    }
                }
                match source {
                    source @ (Candidate::Input(_) | Candidate::Inputs(_) | Candidate::Text) => source,
                    _ => Candidate::Other,
                }
            }
            s::ExprKind::Unary { op: s::UnaryOp::Give, operand } if self.inputs => {
                match self.expression(operand, env, depth + 1)? {
                    Candidate::Input(i) => Candidate::TakenInput(i),
                    _ => Candidate::Other,
                }
            }
            s::ExprKind::Unary { op: s::UnaryOp::Borrow | s::UnaryOp::BorrowMut, operand } if self.inputs => {
                // A reborrow of an actual pointee follows the incoming loan.
                // Borrowing a parameter's own slot has a local lifetime.
                let mut place = operand.as_ref();
                let mut projected = false;
                loop {
                    self.budget.node(place.span, depth + 1)?;
                    match &place.kind {
                        s::ExprKind::Group(inner) => place = inner,
                        s::ExprKind::Index { object, index } => {
                            if self.rebindings { self.expression(index, env, depth + 1)?; }
                            projected = true;
                            place = object;
                        }
                        s::ExprKind::Member {object,..} => {
                            projected = true;
                            place = object;
                        }
                        s::ExprKind::Unary {op:s::UnaryOp::Dereference,operand} => {
                            projected = true;
                            place = operand;
                            break;
                        }
                        _ => break,
                    }
                }
                if projected {
                    match self.expression(place, env, depth + 1)? {
                        source @ (Candidate::Input(_) | Candidate::Inputs(_)) => source,
                        _ => Candidate::Other,
                    }
                } else {
                    if matches!(value.kind, s::ExprKind::Unary { op: s::UnaryOp::BorrowMut, .. }) {
                        self.invalidate_mutable(env, value.span)?;
                    }
                    Candidate::Other
                }
            }
            s::ExprKind::Call { callee, arguments } if self.inputs => {
                // These bare primitives are reserved by the source namespace.
                // Their view depends on the first actual manager; the element
                // index contributes no storage origin.
                let s::ExprKind::Name(name) = &callee.kind else {
                    self.invalidate_mutable(env, value.span)?;
                    return Ok(Candidate::Other);
                };
                self.budget.text(name, callee.span)?;
                let manager = match (name.as_str(), arguments.as_slice()) {
                    ("bytes_view" | "bytes_view_mut", [manager]) if !manager.spread => manager,
                    ("store_at" | "store_at_mut", [manager, index]) if !manager.spread && !index.spread => manager,
                    _ => {
                        self.invalidate_mutable(env, value.span)?;
                        return Ok(Candidate::Other);
                    }
                };
                for (binding, _, _) in env.iter().rev() {
                    self.budget.text(binding, callee.span)?;
                    self.budget.text(name, callee.span)?;
                    if *binding == name {
                        self.invalidate_mutable(env, value.span)?;
                        return Ok(Candidate::Other);
                    }
                }
                let source = self.expression(&manager.value, env, depth + 1)?;
                if self.rebindings {
                    if let Some(index) = arguments.get(1) { self.expression(&index.value, env, depth + 1)?; }
                }
                match source {
                    source @ (Candidate::Input(_) | Candidate::Inputs(_)) => source,
                    _ => Candidate::Other,
                }
            }
            s::ExprKind::If {
                then_value,
                else_value,
                condition,
            } => {
                if self.rebindings { self.expression(condition, env, depth + 1)?; }
                let left = self.expression(then_value, env, depth + 1)?;
                let right = self.expression(else_value, env, depth + 1)?;
                self.join(left, right, value.span, depth + 1)?
            }
            s::ExprKind::Closure { move_capture, params, body, .. } => {
                self.invalidate_mutable(env,value.span)?;
                let saved = env.len();
                for parameter in params {
                    self.bind(env, &parameter.name, Candidate::Other, parameter.span)?;
                }
                let result = self.arrow(body, env, depth + 1)?;
                env.truncate(saved);
                self.invalidate_mutable(env,value.span)?;
                match result {
                    Some(Candidate::Text) => Candidate::TextClosure,
                    Some(Candidate::TakenInput(i)) if *move_capture => Candidate::TransferClosure(i),
                    _ => Candidate::Other,
                }
            }
            s::ExprKind::Unary {op:s::UnaryOp::Dereference,operand} if self.rebindings => {
                self.expression(operand,env,depth+1)?;
                Candidate::Other
            }
            s::ExprKind::Binary { left, right, .. } if self.rebindings => {
                self.expression(left, env, depth + 1)?;
                self.expression(right, env, depth + 1)?;
                Candidate::Other
            }
            _ => {
                self.invalidate_mutable(env, value.span)?;
                Candidate::Other
            }
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
        env: &mut Bindings<'a>,
        depth: usize,
    ) -> R<Option<Candidate<'a>>> {
        match body {
            s::ArrowBody::Expression(value) => Ok(Some(self.expression(value, env, depth + 1)?)),
            s::ArrowBody::Block(body) => Ok(self.block(body, env, depth + 1, None)?.result),
        }
    }
    fn block<'a>(
        &mut self,
        body: &'a s::Block,
        env: &mut Bindings<'a>,
        depth: usize,
        mut jumps: Option<&mut LoopExits<'a>>,
    ) -> R<Flow<'a>> {
        self.budget.node(body.span, depth)?;
        let saved = env.len();
        let mut result = None;
        let mut fallthrough = true;
        for statement in &body.statements {
            if !fallthrough {
                break;
            }
            let next = self.statement(statement, env, depth + 1, jumps.as_deref_mut())?;
            self.merge(&mut result, next.result, statement.span, depth + 1)?;
            fallthrough = next.fallthrough;
        }
        env.truncate(saved);
        Ok(Flow {
            result,
            fallthrough,
        })
    }
    fn merge_loop_state<'a>(
        &mut self,
        prior: &mut Option<Saved<'a>>,
        incoming: Saved<'a>,
        span: Span,
        depth: usize,
    ) -> R<()> {
        let Some(rows) = prior else {
            *prior = Some(incoming);
            return Ok(());
        };
        if rows.len() != incoming.len() {
            return Err(error(
                span,
                "loop exit changes the enclosing lexical bindings",
            ));
        }
        for ((index, old), (other, next)) in rows.iter_mut().zip(incoming) {
            if *index != other {
                return Err(error(
                    span,
                    "loop edge changes an enclosing mutable binding",
                ));
            }
            *old = self.join(
                std::mem::replace(old, Candidate::Other),
                next,
                span,
                depth + 1,
            )?;
        }
        Ok(())
    }

    fn same_loop_origins(
        &mut self,
        left: &Candidate<'_>,
        right: &Candidate<'_>,
        span: Span,
        depth: usize,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        match (left, right) {
            (Candidate::Inputs(left), Candidate::Inputs(right)) => {
                self.budget
                    .identity_work(left.len().min(right.len()), span)?;
                Ok(left == right)
            }
            (Candidate::Tuple(left), Candidate::Tuple(right)) if left.len() == right.len() => {
                for (a, z) in left.iter().zip(right) {
                    if !self.same_loop_origins(a, z, span, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Candidate::Record(left), Candidate::Record(right)) if left.len() == right.len() => {
                for ((name, a), (other, z)) in left.iter().zip(right) {
                    self.budget.text(name, span)?;
                    self.budget.text(other, span)?;
                    if name != other || !self.same_loop_origins(a, z, span, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            // Differing shapes and leaves compare in constant work.
            _ => Ok(left == right),
        }
    }

    fn loop_flow<'a>(
        &mut self,
        condition: Option<&'a s::Condition>,
        pattern: Option<&'a s::Pattern>,
        body: &'a s::Block,
        env: &mut Bindings<'a>,
        span: Span,
        depth: usize,
    ) -> R<Flow<'a>> {
        let prefix = env.len();
        let mut header = self.saved_bindings(env, span, depth)?;
        let mut result = None;
        let mut exits = None;
        let may_exit = pattern.is_some()
            || condition.is_some_and(|c| {
                !matches!(
                    c,
                    s::Condition::Expression(s::Expr {
                        kind: s::ExprKind::Literal(s::Literal::Bool(true)),
                        ..
                    })
                )
            });
        loop {
            self.budget.node(span, depth)?;
            for (index, value) in &header {
                env[*index].1 = self.copy(value, span, depth + 1)?;
            }
            if let Some(condition) = condition {
                self.condition(condition, env, depth + 1)?;
            }
            if may_exit {
                let mut state = self.saved_bindings(env, span, depth)?;
                state.retain(|(index, _)| *index < prefix);
                self.merge_loop_state(&mut exits, state, span, depth + 1)?;
            }
            if let Some(pattern) = pattern {
                self.shadow(pattern, env, Candidate::Other, depth + 1)?;
            }
            let mut jumps = LoopExits {
                prefix,
                breaks: None,
                backedges: None,
            };
            let step = self.block(body, env, depth + 1, Some(&mut jumps))?;
            env.truncate(prefix);
            self.merge(&mut result, step.result, span, depth + 1)?;
            if let Some(state) = jumps.breaks {
                self.merge_loop_state(&mut exits, state, span, depth + 1)?;
            }
            if step.fallthrough {
                let state = self.saved_bindings(env, span, depth)?;
                self.merge_loop_state(&mut jumps.backedges, state, span, depth + 1)?;
            }
            let Some(backedge) = jumps.backedges else {
                break;
            };
            let mut stable = true;
            for ((index, old), (other, edge)) in header.iter_mut().zip(backedge) {
                if *index != other {
                    return Err(error(span, "loop backedge changes an enclosing binding"));
                }
                let next = self.copy(old, span, depth + 1)?;
                let next = self.join(next, edge, span, depth + 1)?;
                stable &= self.same_loop_origins(old, &next, span, depth + 1)?;
                *old = next;
            }
            if stable {
                break;
            }
        }
        let fallthrough = exits.is_some();
        if let Some(exit) = exits {
            for (index, value) in exit {
                env[index].1 = value;
            }
        }
        Ok(Flow {
            result,
            fallthrough,
        })
    }

    fn condition<'a>(
        &mut self,
        condition: &'a s::Condition,
        env: &mut Bindings<'a>,
        depth: usize,
    ) -> R<()> {
        match condition {
            s::Condition::Let { pattern, value } => {
                let actual = self.expression(value, env, depth + 1)?;
                self.shadow(pattern, env, actual, depth + 1)?;
            }
            s::Condition::Expression(value) if self.rebindings => {
                self.expression(value, env, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn statement<'a>(
        &mut self,
        statement: &'a s::Stmt,
        env: &mut Bindings<'a>,
        depth: usize,
        mut jumps: Option<&mut LoopExits<'a>>,
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
                let before = env.len();
                self.shadow(pattern, env, if *mutable && !self.rebindings { Candidate::Other } else { actual }, depth + 1)?;
                if *mutable && self.rebindings {
                    self.budget.identity_work(env.len() - before,statement.span)?;
                    for row in &mut env[before..] { row.2 = true; }
                }
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
                let before = self.saved_bindings(env, statement.span, depth)?;
                let left = self.block(then_body, env, depth + 1, jumps.as_deref_mut())?;
                self.merge(&mut result, left.result, statement.span, depth + 1)?;
                env.truncate(saved);
                let mut left_bindings = before;
                for (index,source) in &mut left_bindings {
                    std::mem::swap(source,&mut env[*index].1);
                }
                let right = match else_body {
                    Some(s::ElseBranch::Block(body)) => self.block(body, env, depth + 1, jumps.as_deref_mut())?,
                    Some(s::ElseBranch::If(statement)) => self.statement(statement, env, depth + 1, jumps.as_deref_mut())?,
                    None => Flow { result: None, fallthrough: true },
                };
                self.merge(&mut result, right.result, statement.span, depth + 1)?;
                fallthrough = left.fallthrough || right.fallthrough;
                if left.fallthrough {
                    for (index,left) in left_bindings {
                        let right_value = &mut env[index].1;
                        *right_value = if right.fallthrough {
                            self.join(left,std::mem::replace(right_value,Candidate::Other),statement.span,depth+1)?
                        } else { left };
                    }
                }
            }
            s::StmtKind::While {condition,body} =>
                return self.loop_flow(Some(condition),None,body,env,statement.span,depth+1),
            s::StmtKind::For {pattern,iterable,body} => {
                if self.rebindings {self.expression(iterable,env,depth+1)?;}
                return self.loop_flow(None,Some(pattern),body,env,statement.span,depth+1);
            }
            s::StmtKind::With { name, body, .. } => {
                let saved = env.len();
                self.bind(env, name, Candidate::Other, statement.span)?;
                let inner = self.block(body, env, depth + 1, jumps.as_deref_mut())?;
                result = inner.result;
                fallthrough = inner.fallthrough;
                env.truncate(saved);
            }
            s::StmtKind::Loop(body) =>
                return self.loop_flow(None,None,body,env,statement.span,depth+1),
            s::StmtKind::Unsafe(body) => {
                let inner = self.block(body, env, depth + 1, jumps.as_deref_mut())?;
                result = inner.result;
                fallthrough = inner.fallthrough;
            }
            s::StmtKind::Match { value, arms } => {
                self.invalidate_mutable(env,statement.span)?;
                let actual = self.expression(value, env, depth + 1)?;
                fallthrough = arms.is_empty();
                for arm in arms {
                    let saved = env.len();
                    let selected = self.copy(&actual, statement.span, depth + 1)?;
                    self.shadow(&arm.pattern, env, selected, depth + 1)?;
                    // Expression arms are statement expressions, not returns.
                    if let s::ArrowBody::Block(body) = &arm.body {
                        let inner = self.block(body, env, depth + 1, jumps.as_deref_mut())?;
                        self.merge(&mut result, inner.result, statement.span, depth + 1)?;
                        fallthrough |= inner.fallthrough;
                    } else {
                        fallthrough = true;
                    }
                    env.truncate(saved);
                }
            }
            s::StmtKind::Meta(_) => result = Some(Candidate::Other),
            s::StmtKind::Break | s::StmtKind::Continue => {
                if let Some(jumps)=jumps {
                    let mut state=self.saved_bindings(env,statement.span,depth)?;
                    state.retain(|(index,_)|*index<jumps.prefix);
                    let edges=if matches!(statement.kind,s::StmtKind::Break){&mut jumps.breaks}else{&mut jumps.backedges};
                    self.merge_loop_state(edges,state,statement.span,depth+1)?;
                }
                fallthrough=false;
            }
            s::StmtKind::Assign {target,op,value} if self.rebindings => {
                let s::ExprKind::Name(name) = &target.kind else {
                    self.invalidate_mutable(env,statement.span)?;
                    return Ok(Flow {result,fallthrough});
                };
                let rhs = if *op == s::AssignOp::Set {self.expression(value,env,depth+1)?} else {Candidate::Other};
                for (binding,source,mutable) in env.iter_mut().rev() {
                    self.budget.text(binding,target.span)?;
                    self.budget.text(name,target.span)?;
                    if *binding == name {
                        if *mutable { *source = rhs; }
                        break;
                    }
                }
            }
            s::StmtKind::Expression(value) | s::StmtKind::Yield(value) if self.rebindings => {
                self.expression(value, env, depth + 1)?;
            }
            s::StmtKind::Defer(_) => self.invalidate_mutable(env, statement.span)?,
            s::StmtKind::Expression(_) => {}
            s::StmtKind::Assign { .. }
            | s::StmtKind::Yield(_) => {}
        }
        if matches!(statement.kind,s::StmtKind::Match{..}) {
            self.invalidate_mutable(env,statement.span)?;
        }
        Ok(Flow {
            result,
            fallthrough,
        })
    }
}

/// Only an omitted outer callable-result life needs the literal-origin walk.
/// This is a query filter, not the qualification of that result as Static.
pub(super) fn elided_result(bound: &s::TypeRef, budget: &mut Budget<'_>, depth: usize) -> R<Option<bool>> {
    budget.node(bound.span, depth)?;
    match &bound.kind {
        s::TypeKind::Group(inner) | s::TypeKind::ForLifetime { inner, .. } => {
            elided_result(inner, budget, depth + 1)
        }
        s::TypeKind::Named { path, args } if matches!(path.as_slice(), [name] if matches!(name.as_str(), "Fn" | "FnMut" | "FnOnce")) =>
        {
            let [s::GenericArg::Type(_), s::GenericArg::Type(result)] = args.as_slice() else {
                return Ok(None);
            };
            let mut result = result;
            while let s::TypeKind::Group(inner) = &result.kind {
                budget.node(result.span, depth + 1)?;
                result = inner;
            }
            Ok(match result.kind {
                s::TypeKind::Reference { mutable, lifetime: None, .. } => Some(mutable),
                _ => None,
            })
        }
        _ => Ok(None),
    }
}

fn original_result<'a>(
    function: &'a s::Function,
    budget: &mut Budget<'_>,
    inputs: bool, rebindings:bool,
) -> R<Option<Candidate<'a>>> {
    let mut walk = Walk { budget, inputs, rebindings };
    let mut env = Vec::new();
    for (index, input) in function.params.iter().enumerate() {
        let source = if inputs { Candidate::Input(index) } else { Candidate::Other };
        walk.bind(&mut env, &input.name, source, input.span)?;
    }
    match &function.body {
        s::FunctionBody::Block(body) => Ok(walk.block(body, &mut env, 0, None)?.result),
        s::FunctionBody::Expression(value) => Ok(Some(walk.expression(value, &mut env, 0)?)),
        s::FunctionBody::Declaration => Ok(None),
    }
}

pub(crate) enum BorrowedResult {
    Input(usize),
    Inputs(Vec<usize>),
    Static,
}

pub(crate) fn input_result(function: &s::Function, budget: &mut Budget<'_>) -> R<Option<BorrowedResult>> {
    Ok(match original_result(function, budget, true, true)? {
        Some(Candidate::Input(index)) => Some(BorrowedResult::Input(index)),
        Some(Candidate::Inputs(indices)) => Some(BorrowedResult::Inputs(indices)),
        Some(Candidate::Text) => Some(BorrowedResult::Static),
        _ => None,
    })
}

pub(super) fn transferred_input(function: &s::Function, budget: &mut Budget<'_>) -> R<Option<usize>> {
    Ok(match original_result(function, budget, true, false)? {
        Some(Candidate::TransferClosure(input)) => Some(input),
        _ => None,
    })
}

pub(super) fn opaque_result(function: &s::Function, budget: &mut Budget<'_>) -> R<bool> {
    Ok(matches!(original_result(function, budget, false, false)?, Some(Candidate::TextClosure)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn original_returns_join_input_roots_and_static_literals() {
        for (body, expected) in [
            ("return a", (vec![0], false)),
            ("return if f {a} else {b}", (vec![0, 1], false)),
            ("var q=a; if f {q=b;} return q", (vec![1,0], false)),
            ("var q=a; q=b; return q", (vec![1], false)),
            ("var q=a; while f {q=b;break;} return q", (vec![0,1], false)),
            ("var q=a; loop {q=b;break;} return q", (vec![1], false)),
            ("var q=a; while f {q=b;continue;} return q", (vec![0,1], false)),
            ("var q=a; if f {let q=b;let seen=*q;} return q", (vec![0], false)),
            ("if f {return b;} return a", (vec![1, 0], false)),
            ("return if f {a} else {a}", (vec![0], false)),
            ("return if f {b} else {\"ABC\"}", (vec![1], true)),
            ("return \"ABC\"", (vec![], true)),
        ] {
            let source = SourceUnit::new(FileId(0), ModuleId(0), "origins.zil",
                format!("fn choose(a:&[u8],b:&[u8],f:bool)->&[u8]{{{body}}}"));
            let module = zil_syntax::parse(&source).into_result().unwrap();
            let s::ItemKind::Function(function) = &module.items[0].kind else { panic!(); };
            let limits = Limits::default();
            let mut budget = Budget::new(&limits);
            let result = input_result(function, &mut budget).unwrap();
            match result {
                Some(BorrowedResult::Input(index)) => assert_eq!((vec![index], false), expected),
                Some(BorrowedResult::Inputs(indices)) => {
                    assert_eq!(&indices, &expected.0);
                    // A join with a literal still keeps its own result occurrence.
                    if indices.len() == 1 { assert!(expected.1); }
                }
                Some(BorrowedResult::Static) => assert_eq!((vec![], true), expected),
                None => panic!("missing return origin for {body}"),
            }
        }
    }

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
            ("fn make(input:&[u8])->impl Fn()->&[u8]{let pair=Pair{text:\"A\",tag:7};return move()=>{let Pair{text:text,tag:_}=pair;return text;};}", true),
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
            let module = zil_syntax::parse(&source).into_result().unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
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
