//! Bounded source-only receiver classification for owning Package captures.
//! This is not a permission proof: Analyze and Verify reconstruct the actual
//! typed control flow independently, including every returning error edge.
use super::*;
use std::collections::BTreeSet;

const LIVE: u8 = 1;
const EMPTY: u8 = 2;
type State = Vec<u8>;

struct Package {
    capture: usize,
    name: String,
    whole: usize,
    end: usize,
    // Child Packages move by ordinary value use; Managed fields project Desc
    // until an explicit give. Both occupy one complete direct field debt.
    fields: BTreeMap<String, (usize, bool)>,
    copy_fields: BTreeSet<String>,
    owned_use: bool,
    automatic_owned_use: bool,
    has_child: bool,
}

#[derive(Default)]
struct Flow {
    normal: Option<State>,
    breaks: Option<State>,
    continues: Option<State>,
}

pub(super) fn classify(
    body: &CoreBlock,
    parameters: &[CoreClosureParameter],
    captures: &mut [Capture],
    nested_moves: &BTreeMap<zil_base::NodeId, BTreeSet<String>>,
    limits: &Limits,
    work: &mut usize,
) -> R<()> {
    *work = work
        .checked_sub(captures.len())
        .ok_or_else(|| limit("Package capture classification budget exceeded", body.span))?;
    if !captures
        .iter()
        .any(|capture| matches!(capture.ty, Type::Package { .. }))
    {
        return Ok(());
    }
    let mut packages = Vec::new();
    let mut slots = 0usize;
    let mut has_automatic_owned_use = false;
    for (capture, item) in captures.iter().enumerate() {
        let Type::Package { fields, .. } = &item.ty else {
            continue;
        };
        *work = work
            .checked_sub(
                fields
                    .len()
                    .saturating_add(item.name.len())
                    .saturating_add(1),
            )
            .ok_or_else(|| limit("Package capture slot budget exceeded", item.span))?;
        let whole = slots;
        slots = slots
            .checked_add(1)
            .ok_or_else(|| limit("Package slot overflow", item.span))?;
        let mut owned = BTreeMap::new();
        let mut copy_fields = BTreeSet::new();
        let mut automatic_owned_use = false;
        let mut has_child = false;
        for (name, ty) in fields {
            *work = work
                .checked_sub(name.len())
                .ok_or_else(|| limit("Package field name budget exceeded", item.span))?;
            if matches!(ty, Type::Managed(_) | Type::Package { .. }) {
                let child = matches!(ty, Type::Package { .. });
                has_child |= child;
                automatic_owned_use |= dynamic_store::is_carrier(ty) || child;
                owned.insert(name.clone(), (slots, child));
                slots = slots
                    .checked_add(1)
                    .ok_or_else(|| limit("Package slot overflow", item.span))?;
            } else {
                copy_fields.insert(name.clone());
            }
        }
        packages.push(Package {
            capture,
            name: item.name.clone(),
            whole,
            end: slots,
            fields: owned,
            copy_fields,
            owned_use: false,
            automatic_owned_use,
            has_child,
        });
        has_automatic_owned_use |= automatic_owned_use;
    }
    let mut scan = Scan {
        writes: vec![false; packages.len()],
        partial_exit: vec![false; packages.len()],
        packages,
        scopes: vec![BTreeSet::new()],
        limits,
        work,
        nested_moves,
        has_automatic_owned_use,
        borrow_depth: 0,
    };
    for parameter in parameters {
        scan.bind(&parameter.name, parameter.span)?;
    }
    scan.spend(slots, body.span)?;
    let flow = scan.block(vec![LIVE; slots], body, 0)?;
    if let Some(state) = flow.normal {
        scan.exit(&state, body.span)?;
    }
    for (index, package) in scan.packages.iter().enumerate() {
        captures[package.capture].written |= scan.writes[index];
        captures[package.capture].consumed |= scan.partial_exit[index];
        captures[package.capture].moved |= package.owned_use;
    }
    Ok(())
}

struct Scan<'a> {
    packages: Vec<Package>,
    writes: Vec<bool>,
    partial_exit: Vec<bool>,
    scopes: Vec<BTreeSet<String>>,
    limits: &'a Limits,
    work: &'a mut usize,
    nested_moves: &'a BTreeMap<zil_base::NodeId, BTreeSet<String>>,
    has_automatic_owned_use: bool,
    borrow_depth: usize,
}

impl Scan<'_> {
    fn spend(&mut self, amount: usize, span: Span) -> R<()> {
        *self.work = self
            .work
            .checked_sub(amount)
            .ok_or_else(|| limit("Package capture flow work budget exceeded", span))?;
        Ok(())
    }
    fn visit(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(limit("Package capture flow depth budget exceeded", span));
        }
        self.spend(1, span)
    }
    fn bind(&mut self, name: &str, span: Span) -> R<()> {
        let size = self.scopes.last().expect("scope").len();
        self.spend(
            name.len()
                .saturating_add(1)
                .saturating_mul(size.checked_ilog2().map_or(1, |n| n as usize + 2)),
            span,
        )?;
        self.scopes.last_mut().expect("scope").insert(name.into());
        Ok(())
    }
    fn pattern(&mut self, pattern: &CorePattern, depth: usize) -> R<()> {
        self.visit(depth, pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => self.bind(name, pattern.span)?,
            CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => {
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn package(&mut self, name: &str, span: Span) -> R<Option<usize>> {
        let mut cost = self.packages.len().saturating_add(1);
        for scope in &self.scopes {
            cost = cost.saturating_add(scope.len().checked_ilog2().map_or(1, |n| n as usize + 2));
        }
        self.spend(cost.saturating_mul(name.len().saturating_add(1)), span)?;
        if self.scopes.iter().rev().any(|scope| scope.contains(name)) {
            return Ok(None);
        }
        Ok(self
            .packages
            .iter()
            .position(|package| package.name == name))
    }
    fn named(&mut self, expression: &CoreExpr) -> R<Option<usize>> {
        if let CoreExprKind::Name(name) = &expression.kind {
            self.package(name, expression.span)
        } else {
            Ok(None)
        }
    }
    fn copy(&mut self, state: &State, span: Span) -> R<State> {
        self.spend(state.len(), span)?;
        Ok(state.clone())
    }
    fn join(&mut self, a: Option<State>, b: Option<State>, span: Span) -> R<Option<State>> {
        match (a, b) {
            (Some(mut a), Some(b)) => {
                self.spend(a.len().saturating_add(b.len()), span)?;
                for (a, b) in a.iter_mut().zip(b) {
                    *a |= b;
                }
                Ok(Some(a))
            }
            (a, b) => Ok(a.or(b)),
        }
    }
    fn merge(&mut self, a: Flow, b: Flow, span: Span) -> R<Flow> {
        Ok(Flow {
            normal: self.join(a.normal, b.normal, span)?,
            breaks: self.join(a.breaks, b.breaks, span)?,
            continues: self.join(a.continues, b.continues, span)?,
        })
    }
    fn exit(&mut self, state: &State, span: Span) -> R<()> {
        self.spend(state.len().saturating_add(self.packages.len()), span)?;
        for (index, package) in self.packages.iter().enumerate() {
            self.partial_exit[index] |= state[package.whole..package.end]
                .iter()
                .any(|slot| *slot != LIVE);
        }
        Ok(())
    }
    fn whole(&mut self, state: &mut State, package: usize, value: u8, span: Span) -> R<()> {
        let range = self.packages[package].whole..self.packages[package].end;
        self.spend(range.len(), span)?;
        state[range].fill(value);
        self.writes[package] = true;
        self.packages[package].owned_use |= value == EMPTY && self.borrow_depth == 0;
        Ok(())
    }
    fn field(&mut self, package: usize, name: &str, span: Span) -> R<Option<(usize, bool)>> {
        self.spend(
            name.len().saturating_add(1).saturating_mul(
                self.packages[package]
                    .fields
                    .len()
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 2),
            ),
            span,
        )?;
        Ok(self.packages[package].fields.get(name).copied())
    }
    fn place(&mut self, state: &mut State, expression: &CoreExpr, depth: usize) -> R<()> {
        self.visit(depth, expression.span)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                if let Some(package) = self.package(name, expression.span)? {
                    self.writes[package] = true;
                }
            }
            CoreExprKind::Member { object, .. } => self.place(state, object, depth + 1)?,
            CoreExprKind::Index { object, index } => {
                self.place(state, object, depth + 1)?;
                self.expr(state, index, depth + 1)?;
            }
            CoreExprKind::Unary { operand, .. } | CoreExprKind::Ascribed { value: operand, .. } => {
                self.expr(state, operand, depth + 1)?;
            }
            _ => self.expr(state, expression, depth + 1)?,
        }
        Ok(())
    }
    fn assign(
        &mut self,
        state: &mut State,
        target: &CoreExpr,
        value: &CoreExpr,
        depth: usize,
    ) -> R<()> {
        self.place(state, target, depth + 1)?;
        self.expr(state, value, depth + 1)?;
        if let Some(package) = self.named(target)? {
            self.whole(state, package, LIVE, target.span)?;
        } else if let CoreExprKind::Member { object, member } = &target.kind
            && let Some(package) = self.named(object)?
            && let Some((slot, _)) = self.field(package, member, target.span)?
        {
            state[slot] = LIVE;
        }
        Ok(())
    }
    fn expr(&mut self, state: &mut State, expression: &CoreExpr, depth: usize) -> R<()> {
        self.visit(depth, expression.span)?;
        let span = expression.span;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                if let Some(package) = self.package(name, span)? {
                    self.whole(state, package, EMPTY, span)?;
                }
            }
            CoreExprKind::Member { object, member } => {
                // Direct Package projections name its inline slot. More
                // complex Package expressions already produce whole values.
                if let Some(package) = self.named(object)? {
                    if self.packages[package].has_child
                        && self.borrow_depth == 0
                        && let Some((slot, true)) = self.field(package, member, span)?
                    {
                        state[slot] = EMPTY;
                        self.writes[package] = true;
                        self.packages[package].owned_use = true;
                    }
                } else {
                    self.expr(state, object, depth + 1)?;
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => {
                if let CoreExprKind::Member { object, member } = &operand.kind
                    && let Some(package) = self.named(object)?
                    && let Some((slot, _)) = self.field(package, member, span)?
                {
                    state[slot] = EMPTY;
                    self.writes[package] = true;
                    self.packages[package].owned_use = true;
                } else {
                    self.expr(state, operand, depth + 1)?;
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                // The old receiver flow remains conservative, but a read of a
                // Package place must not infer the new creation-time transfer.
                self.borrow_depth += 1;
                self.expr(state, operand, depth + 1)?;
                self.borrow_depth -= 1;
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::Ascribed { value: operand, .. }
            | CoreExprKind::IntegerConvert { value: operand, .. }
            | CoreExprKind::TypeApply {
                callee: operand, ..
            }
            | CoreExprKind::CompileTime(operand) => self.expr(state, operand, depth + 1)?,
            CoreExprKind::Try(operand) => {
                self.expr(state, operand, depth + 1)?;
                self.exit(state, span)?;
            }
            CoreExprKind::Binary { op, left, right } => {
                self.expr(state, left, depth + 1)?;
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let prior = self.copy(state, span)?;
                    self.expr(state, right, depth + 1)?;
                    let current = self.copy(state, span)?;
                    *state = self
                        .join(Some(prior), Some(current), span)?
                        .expect("branch");
                } else {
                    self.expr(state, right, depth + 1)?;
                }
            }
            CoreExprKind::Call { callee, arguments } => {
                self.expr(state, callee, depth + 1)?;
                if let CoreExprKind::Member { object, member } = &callee.kind
                    && let Some(package) = self.named(object)?
                {
                    let size = self.packages[package].copy_fields.len();
                    self.spend(
                        member
                            .len()
                            .saturating_add(1)
                            .saturating_mul(size.checked_ilog2().map_or(1, |n| n as usize + 2)),
                        callee.span,
                    )?;
                    if self.field(package, member, callee.span)?.is_none()
                        && !self.packages[package].copy_fields.contains(member)
                    {
                        // A valid Package method in this finite profile has a
                        // by-value receiver; head addresses remain unsupported.
                        // The transfer precedes ordinary arguments. Later full
                        // refills can restore repeatability on every exit.
                        self.whole(state, package, EMPTY, object.span)?;
                    }
                }
                for argument in arguments {
                    self.expr(state, argument, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(items) | CoreExprKind::Array(items) => {
                for item in items {
                    self.expr(state, item, depth + 1)?;
                }
            }
            CoreExprKind::Index { object, index } => {
                self.expr(state, object, depth + 1)?;
                self.expr(state, index, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(state, object, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.expr(state, endpoint, depth + 1)?;
                }
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.expr(state, value, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expr(state, base, depth + 1)?;
                for (_, value) in fields {
                    self.expr(state, value, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.scopes.push(BTreeSet::new());
                for (name, value) in bindings {
                    self.expr(state, value, depth + 1)?;
                    self.bind(name, value.span)?;
                }
                self.expr(state, value, depth + 1)?;
                self.scopes.pop();
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(state, condition, depth + 1)?;
                let mut other = self.copy(state, span)?;
                self.expr(state, then_value, depth + 1)?;
                self.expr(&mut other, else_value, depth + 1)?;
                let current = self.copy(state, span)?;
                *state = self
                    .join(Some(current), Some(other), span)?
                    .expect("branches");
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                // The body executes later, but its owned captures transfer at
                // creation. Only these source-inferred transfers enter this
                // flow; Analyze/Verify independently check the actual IR.
                if let Some(names) = self.nested_moves.get(&expression.id) {
                    for name in names {
                        if let Some(package) = self.package(name, span)? {
                            self.whole(state, package, EMPTY, span)?;
                        }
                    }
                }
                if self.has_automatic_owned_use {
                    self.nested_owned_uses(state, parameters, body, depth + 1)?;
                }
            }
            CoreExprKind::Integer(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::Literal(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Unit
            | CoreExprKind::ReceiverMember(_)
            | CoreExprKind::Unsupported(_) => {}
        }
        Ok(())
    }
    fn nested_owned_uses(
        &mut self,
        state: &mut State,
        parameters: &[CoreClosureParameter],
        body: &CoreBlock,
        depth: usize,
    ) -> R<()> {
        // Only Store-bearing or Product Package captures enter this extra
        // source walk. A nested body's real value/give use transfers its complete
        // Package when that child is created, regardless of a `move` keyword.
        let count = self.packages.len();
        self.spend(
            count
                .saturating_mul(6)
                .saturating_add(state.len())
                .saturating_add(parameters.len())
                .saturating_add(1),
            body.span,
        )?;
        let writes = std::mem::replace(&mut self.writes, vec![false; count]);
        let exits = std::mem::replace(&mut self.partial_exit, vec![false; count]);
        let owned = self
            .packages
            .iter_mut()
            .map(|package| std::mem::replace(&mut package.owned_use, false))
            .collect::<Vec<_>>();
        let borrow_depth = std::mem::replace(&mut self.borrow_depth, 0);
        self.scopes.push(BTreeSet::new());
        for parameter in parameters {
            self.bind(&parameter.name, parameter.span)?;
        }
        self.block(vec![LIVE; state.len()], body, depth)?;
        self.scopes.pop();
        self.borrow_depth = borrow_depth;
        self.writes = writes;
        self.partial_exit = exits;
        for (index, prior) in owned.into_iter().enumerate() {
            let moved = self.packages[index].automatic_owned_use && self.packages[index].owned_use;
            self.packages[index].owned_use = prior;
            if moved {
                self.whole(state, index, EMPTY, body.span)?;
            }
        }
        Ok(())
    }
    fn block(&mut self, state: State, block: &CoreBlock, depth: usize) -> R<Flow> {
        self.visit(depth, block.span)?;
        self.scopes.push(BTreeSet::new());
        let mut flow = Flow {
            normal: Some(state),
            ..Flow::default()
        };
        for statement in &block.statements {
            let Some(state) = flow.normal.take() else {
                break;
            };
            let next = self.statement(state, statement, depth + 1)?;
            flow.normal = next.normal;
            flow.breaks = self.join(flow.breaks, next.breaks, statement.span)?;
            flow.continues = self.join(flow.continues, next.continues, statement.span)?;
        }
        self.scopes.pop();
        Ok(flow)
    }
    fn patterned(
        &mut self,
        state: State,
        pattern: &CorePattern,
        body: &CoreBlock,
        depth: usize,
    ) -> R<Flow> {
        self.scopes.push(BTreeSet::new());
        self.pattern(pattern, depth + 1)?;
        let flow = self.block(state, body, depth + 1)?;
        self.scopes.pop();
        Ok(flow)
    }
    fn looping(
        &mut self,
        start: State,
        condition: Option<&CoreExpr>,
        pattern: Option<&CorePattern>,
        body: &CoreBlock,
        depth: usize,
    ) -> R<Flow> {
        let mut head = self.copy(&start, body.span)?;
        let mut exits = None;
        let unconditional = condition
            .is_some_and(|value| matches!(value.kind, CoreExprKind::Bool(true)))
            && pattern.is_none();
        loop {
            self.visit(depth, body.span)?;
            let mut current = self.copy(&head, body.span)?;
            if let Some(condition) = condition {
                self.expr(&mut current, condition, depth + 1)?;
            }
            if !unconditional {
                let stopped = self.copy(&current, body.span)?;
                exits = self.join(exits, Some(stopped), body.span)?;
            }
            let body_flow = if let Some(pattern) = pattern {
                self.patterned(current, pattern, body, depth + 1)?
            } else {
                self.block(current, body, depth + 1)?
            };
            exits = self.join(exits, body_flow.breaks, body.span)?;
            let back = self.join(body_flow.normal, body_flow.continues, body.span)?;
            let initial = self.copy(&start, body.span)?;
            let next = self
                .join(Some(initial), back, body.span)?
                .expect("loop entry");
            self.spend(next.len().saturating_add(head.len()), body.span)?;
            if next == head {
                break;
            }
            head = next;
        }
        Ok(Flow {
            normal: exits,
            ..Flow::default()
        })
    }
    fn statement(&mut self, mut state: State, statement: &CoreStatement, depth: usize) -> R<Flow> {
        self.visit(depth, statement.span)?;
        let span = statement.span;
        match &statement.kind {
            CoreStatementKind::Binding {
                name, initializer, ..
            } => {
                if let Some(value) = initializer {
                    self.expr(&mut state, value, depth + 1)?;
                }
                self.bind(name, span)?;
            }
            CoreStatementKind::Assign { target, value } => {
                self.assign(&mut state, target, value, depth + 1)?
            }
            CoreStatementKind::Expression(value) => self.expr(&mut state, value, depth + 1)?,
            CoreStatementKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(&mut state, value, depth + 1)?;
                }
                self.exit(&state, span)?;
                return Ok(Flow::default());
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expr(&mut state, condition, depth + 1)?;
                let other = self.copy(&state, span)?;
                let a = self.block(state, then_body, depth + 1)?;
                let b = if let Some(body) = else_body {
                    self.block(other, body, depth + 1)?
                } else {
                    Flow {
                        normal: Some(other),
                        ..Flow::default()
                    }
                };
                return self.merge(a, b, span);
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.expr(&mut state, value, depth + 1)?;
                let other = self.copy(&state, span)?;
                let a = self.patterned(state, pattern, then_body, depth + 1)?;
                let b = if let Some(body) = else_body {
                    self.block(other, body, depth + 1)?
                } else {
                    Flow {
                        normal: Some(other),
                        ..Flow::default()
                    }
                };
                return self.merge(a, b, span);
            }
            CoreStatementKind::While { condition, body } => {
                return self.looping(state, Some(condition), None, body, depth + 1);
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => return self.looping(state, Some(value), Some(pattern), body, depth + 1),
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expr(&mut state, start, depth + 1)?;
                self.expr(&mut state, end, depth + 1)?;
                self.scopes.push(BTreeSet::new());
                self.bind(name, span)?;
                let result = self.looping(state, None, None, body, depth + 1)?;
                self.scopes.pop();
                return Ok(result);
            }
            CoreStatementKind::Match { value, arms } => {
                self.expr(&mut state, value, depth + 1)?;
                let mut flow = Flow::default();
                for arm in arms {
                    let incoming = self.copy(&state, span)?;
                    let next = self.patterned(incoming, &arm.pattern, &arm.body, depth + 1)?;
                    flow = self.merge(flow, next, span)?;
                }
                return Ok(flow);
            }
            CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                return self.block(state, body, depth + 1);
            }
            CoreStatementKind::Break => {
                return Ok(Flow {
                    breaks: Some(state),
                    ..Flow::default()
                });
            }
            CoreStatementKind::Continue => {
                return Ok(Flow {
                    continues: Some(state),
                    ..Flow::default()
                });
            }
            CoreStatementKind::Unsupported(_) => {}
        }
        Ok(Flow {
            normal: Some(state),
            ..Flow::default()
        })
    }
}
