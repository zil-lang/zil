//! Mutable Managed bindings use a private, single-field owning slot. This is
//! untrusted lowering to existing Package operations, never a verifier escape.
use super::*;
use std::collections::BTreeSet;
const FIELD: &str = "$managed155";
impl Builder<'_, '_> {
    pub(super) fn managed_slot(
        &mut self,
        name: &str,
        ty: Type,
        value: ValueId,
        span: Span,
    ) -> R<()> {
        let package = Type::Package {
            name: format!(
                "$managed155${:?}${}",
                self.function.id,
                self.function.locals.len()
            ),
            fields: vec![(FIELD.into(), ty)],
        };
        // Completed SSA carrier is transferred once into a lexical owning slot.
        let value = self.value(
            InstructionKind::MakePackage {
                fields: vec![value],
            },
            package.clone(),
            span,
        )?;
        let local = self.local(name, package, true, span)?;
        self.store(local, value, span)?;
        self.managed_slots.insert(local);
        Ok(())
    }
    pub(super) fn rewrite_managed_statement(
        &mut self,
        source: &CoreStatement,
    ) -> R<Option<CoreStatement>> {
        if self.managed_slots.is_empty() {
            return Ok(None);
        }
        let names: BTreeSet<_> = self
            .scopes
            .iter()
            .flat_map(|s| s.iter())
            .filter(|(name, id)| self.managed_slots.contains(id) && self.lookup(name) == Some(**id))
            .map(|(name, _)| name.clone())
            .collect();
        if names.is_empty() {
            return Ok(None);
        }
        // Source has already passed bounded shape validation. Each visited node
        // below additionally charges the per-function lowering work allowance.
        let mut statement = source.clone();
        Rewriter {
            names: &names,
            work: &mut self.managed_slot_work,
            depth_limit: self.limits.max_call_depth,
        }
        .statement(&mut statement, &BTreeSet::new(), 0, false)?;
        Ok(Some(statement))
    }
}
struct Rewriter<'a> {
    names: &'a BTreeSet<String>,
    work: &'a mut usize,
    depth_limit: usize,
}
impl Rewriter<'_> {
    fn charge(&mut self, depth: usize, span: Span) -> R<()> {
        *self.work = self.work.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "managed slot lowering work exceeded",
                Some(span),
            )
        })?;
        if depth > self.depth_limit {
            return Err(Diagnostic::limit(
                Phase::Types,
                "managed slot lowering depth exceeded",
                Some(span),
            ));
        }
        Ok(())
    }
    fn pattern(&mut self, p: &CorePattern, names: &mut BTreeSet<String>, depth: usize) -> R<()> {
        self.charge(depth, p.span)?;
        match &p.kind {
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => {
                self.pattern(pattern, names, depth + 1)?
            }
            CorePatternKind::Name(n) => {
                names.insert(n.clone());
            }
            CorePatternKind::Tuple(xs) | CorePatternKind::Variant { fields: xs, .. } => {
                for x in xs {
                    self.pattern(x, names, depth + 1)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn block(&mut self, b: &mut CoreBlock, shadow: &BTreeSet<String>, depth: usize) -> R<()> {
        self.charge(depth, b.span)?;
        let mut shadow = shadow.clone();
        for s in &mut b.statements {
            self.statement(s, &shadow, depth + 1, true)?;
            if let CoreStatementKind::TupleBinding { pattern, .. } = &s.kind {
                self.pattern(pattern, &mut shadow, depth + 1)?;
            }
            if let CoreStatementKind::Binding { name, .. } = &s.kind {
                shadow.insert(name.clone());
            }
        }
        Ok(())
    }
    fn statement(
        &mut self,
        s: &mut CoreStatement,
        shadow: &BTreeSet<String>,
        depth: usize,
        nested: bool,
    ) -> R<()> {
        self.charge(depth, s.span)?;
        match &mut s.kind {
            CoreStatementKind::TupleBinding { initializer, .. } => {
                self.expr(initializer, shadow, depth + 1)?;
            }
            CoreStatementKind::Binding { initializer, .. } => {
                if let Some(e) = initializer {
                    self.expr(e, shadow, depth + 1)?
                }
            }
            CoreStatementKind::Assign { target, value } => {
                self.expr(target, shadow, depth + 1)?;
                self.expr(value, shadow, depth + 1)?;
            }
            CoreStatementKind::Expression(e) | CoreStatementKind::Return(Some(e)) => {
                self.expr(e, shadow, depth + 1)?
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expr(condition, shadow, depth + 1)?;
                if nested {
                    self.block(then_body, shadow, depth + 1)?;
                    if let Some(b) = else_body {
                        self.block(b, shadow, depth + 1)?
                    }
                }
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.expr(value, shadow, depth + 1)?;
                if nested {
                    let mut local = shadow.clone();
                    self.pattern(pattern, &mut local, depth + 1)?;
                    self.block(then_body, &local, depth + 1)?;
                    if let Some(b) = else_body {
                        self.block(b, shadow, depth + 1)?
                    }
                }
            }
            CoreStatementKind::While { condition, body } => {
                self.expr(condition, shadow, depth + 1)?;
                if nested {
                    self.block(body, shadow, depth + 1)?
                }
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => {
                self.expr(value, shadow, depth + 1)?;
                if nested {
                    let mut local = shadow.clone();
                    self.pattern(pattern, &mut local, depth + 1)?;
                    self.block(body, &local, depth + 1)?
                }
            }
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expr(start, shadow, depth + 1)?;
                self.expr(end, shadow, depth + 1)?;
                if nested {
                    let mut local = shadow.clone();
                    local.insert(name.clone());
                    self.block(body, &local, depth + 1)?
                }
            }
            CoreStatementKind::Match { value, arms } => {
                self.expr(value, shadow, depth + 1)?;
                if nested {
                    for arm in arms {
                        let mut local = shadow.clone();
                        self.pattern(&arm.pattern, &mut local, depth + 1)?;
                        self.block(&mut arm.body, &local, depth + 1)?
                    }
                }
            }
            CoreStatementKind::Block(b)
            | CoreStatementKind::Unsafe(b)
            | CoreStatementKind::Defer(b) => {
                if nested {
                    self.block(b, shadow, depth + 1)?
                }
            }
            CoreStatementKind::Break
            | CoreStatementKind::Continue
            | CoreStatementKind::Return(None)
            | CoreStatementKind::Unsupported(_) => {}
        }
        Ok(())
    }
    fn expr(&mut self, e: &mut CoreExpr, shadow: &BTreeSet<String>, depth: usize) -> R<()> {
        self.charge(depth, e.span)?;
        if let CoreExprKind::Name(name) = &e.kind
            && self.names.contains(name)
            && !shadow.contains(name)
        {
            e.kind = CoreExprKind::Member {
                object: Box::new(e.clone()),
                member: FIELD.into(),
                owner: None,
            };
            return Ok(());
        }
        match &mut e.kind {
            // Internal projection was rewritten before entering this nested scope.
            CoreExprKind::Member { member, .. } if member == FIELD => {}
            CoreExprKind::MakeDynBorrow { source: value, .. }
            | CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::NumericConvert { value, .. }
            | CoreExprKind::TypeApply { callee: value, .. }
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Try(value)
            | CoreExprKind::Unary { operand: value, .. } => self.expr(value, shadow, depth + 1)?,
            CoreExprKind::Binary { left, right, .. } => {
                self.expr(left, shadow, depth + 1)?;
                self.expr(right, shadow, depth + 1)?
            }
            CoreExprKind::Call { callee, arguments }
            | CoreExprKind::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expr(callee, shadow, depth + 1)?;
                for a in arguments {
                    self.expr(a, shadow, depth + 1)?
                }
            }
            CoreExprKind::Tuple(xs) | CoreExprKind::Array(xs) => {
                for x in xs {
                    self.expr(x, shadow, depth + 1)?
                }
            }
            CoreExprKind::Index { object, index } => {
                self.expr(object, shadow, depth + 1)?;
                self.expr(index, shadow, depth + 1)?
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, shadow, depth + 1)?;
                for x in [start, end].into_iter().flatten() {
                    self.expr(x, shadow, depth + 1)?
                }
            }
            CoreExprKind::Member { object, .. } => self.expr(object, shadow, depth + 1)?,
            CoreExprKind::Record { fields, .. } => {
                for (_, x) in fields {
                    self.expr(x, shadow, depth + 1)?
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expr(base, shadow, depth + 1)?;
                for (_, x) in fields {
                    self.expr(x, shadow, depth + 1)?
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                let mut local = shadow.clone();
                for (name, x) in bindings {
                    self.expr(x, &local, depth + 1)?;
                    local.insert(name.clone());
                }
                self.expr(value, &local, depth + 1)?
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                let mut local = shadow.clone();
                for p in parameters {
                    local.insert(p.name.clone());
                }
                self.block(body, &local, depth + 1)?
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, shadow, depth + 1)?;
                self.expr(then_value, shadow, depth + 1)?;
                self.expr(else_value, shadow, depth + 1)?
            }
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_)
            | CoreExprKind::ReceiverMember(_)
            | CoreExprKind::Unsupported(_) => {}
        }
        Ok(())
    }
}

pub(super) type BindingSite = (zil_base::NodeId, Span, String);
/// Only promote bindings which actually use the newly admitted replacement.
/// Existing mutable resources with ordinary borrowed captures keep old IR.
pub(super) fn reassigned_bindings(body: &CoreBlock, limits: &Limits) -> R<BTreeSet<BindingSite>> {
    let mut scan = BindingScan {
        scopes: vec![BTreeMap::new()],
        marked: BTreeSet::new(),
        work: limits.max_analysis_steps,
        depth: limits.max_call_depth,
    };
    scan.block(body, 0)?;
    Ok(scan.marked)
}
struct BindingScan {
    scopes: Vec<BTreeMap<String, Option<BindingSite>>>,
    marked: BTreeSet<BindingSite>,
    work: usize,
    depth: usize,
}
impl BindingScan {
    fn charge(&mut self, n: usize, d: usize, s: Span) -> R<()> {
        self.work = self.work.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "managed binding scan work exceeded", Some(s))
        })?;
        if d > self.depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "managed binding scan depth exceeded",
                Some(s),
            ));
        }
        Ok(())
    }
    fn pattern(&mut self, p: &CorePattern, d: usize) -> R<()> {
        self.charge(1, d, p.span)?;
        match &p.kind {
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => self.pattern(pattern, d + 1)?,
            CorePatternKind::Name(n) => {
                self.scopes.last_mut().unwrap().insert(n.clone(), None);
            }
            CorePatternKind::Tuple(xs) | CorePatternKind::Variant { fields: xs, .. } => {
                for x in xs {
                    self.pattern(x, d + 1)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn block(&mut self, b: &CoreBlock, d: usize) -> R<()> {
        self.charge(1, d, b.span)?;
        self.scopes.push(BTreeMap::new());
        for s in &b.statements {
            self.statement(s, d + 1)?
        }
        self.scopes.pop();
        Ok(())
    }
    fn statement(&mut self, s: &CoreStatement, d: usize) -> R<()> {
        self.charge(1, d, s.span)?;
        match &s.kind {
            CoreStatementKind::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                self.expr(initializer, d + 1)?;
                self.pattern(pattern, d + 1)?;
            }
            CoreStatementKind::Binding {
                name,
                mutable,
                initializer,
                ..
            } => {
                if let Some(e) = initializer {
                    self.expr(e, d + 1)?
                };
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), mutable.then(|| (s.id, s.span, name.clone())));
            }
            CoreStatementKind::Assign { target, value } => {
                if let CoreExprKind::Name(n) = &target.kind
                    && let Some(Some(site)) = self.scopes.iter().rev().find_map(|m| m.get(n))
                {
                    self.marked.insert(site.clone());
                }
                self.expr(target, d + 1)?;
                self.expr(value, d + 1)?;
            }
            CoreStatementKind::Expression(e) | CoreStatementKind::Return(Some(e)) => {
                self.expr(e, d + 1)?
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expr(condition, d + 1)?;
                self.block(then_body, d + 1)?;
                if let Some(b) = else_body {
                    self.block(b, d + 1)?
                }
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.expr(value, d + 1)?;
                self.scopes.push(BTreeMap::new());
                self.pattern(pattern, d + 1)?;
                self.block(then_body, d + 1)?;
                self.scopes.pop();
                if let Some(b) = else_body {
                    self.block(b, d + 1)?
                }
            }
            CoreStatementKind::While { condition, body } => {
                self.expr(condition, d + 1)?;
                self.block(body, d + 1)?
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => {
                self.expr(value, d + 1)?;
                self.scopes.push(BTreeMap::new());
                self.pattern(pattern, d + 1)?;
                self.block(body, d + 1)?;
                self.scopes.pop();
            }
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expr(start, d + 1)?;
                self.expr(end, d + 1)?;
                self.scopes.push(BTreeMap::from([(name.clone(), None)]));
                self.block(body, d + 1)?;
                self.scopes.pop();
            }
            CoreStatementKind::Match { value, arms } => {
                self.expr(value, d + 1)?;
                for a in arms {
                    self.scopes.push(BTreeMap::new());
                    self.pattern(&a.pattern, d + 1)?;
                    self.block(&a.body, d + 1)?;
                    self.scopes.pop();
                }
            }
            CoreStatementKind::Block(b)
            | CoreStatementKind::Unsafe(b)
            | CoreStatementKind::Defer(b) => self.block(b, d + 1)?,
            CoreStatementKind::Break
            | CoreStatementKind::Continue
            | CoreStatementKind::Return(None)
            | CoreStatementKind::Unsupported(_) => {}
        }
        Ok(())
    }
    fn expr(&mut self, e: &CoreExpr, d: usize) -> R<()> {
        self.charge(1, d, e.span)?;
        match &e.kind {
            CoreExprKind::MakeDynBorrow { source: value, .. }
            | CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::NumericConvert { value, .. }
            | CoreExprKind::TypeApply { callee: value, .. }
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Try(value)
            | CoreExprKind::Unary { operand: value, .. } => self.expr(value, d + 1),
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => self.expr_pair(left, right, d + 1),
            CoreExprKind::Call { .. } | CoreExprKind::CallDynSlot { .. } => self.expr_call(e, d),
            CoreExprKind::Tuple(xs) | CoreExprKind::Array(xs) => self.expr_list(xs, d + 1),
            CoreExprKind::Slice { .. } => self.expr_slice(e, d),
            CoreExprKind::Member { object, .. } => self.expr(object, d + 1),
            CoreExprKind::Record { fields, .. } => self.expr_fields(fields, d + 1),
            CoreExprKind::RecordUpdate { .. } => self.expr_record_update(e, d),
            CoreExprKind::Sequence { .. } => self.expr_sequence(e, d),
            CoreExprKind::Closure { .. } => self.expr_closure(e, d),
            CoreExprKind::If { .. } => self.expr_if(e, d),
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_)
            | CoreExprKind::ReceiverMember(_)
            | CoreExprKind::Unsupported(_) => Ok(()),
        }
    }

    #[inline(never)]
    fn expr_pair(&mut self, first: &CoreExpr, second: &CoreExpr, d: usize) -> R<()> {
        self.expr(first, d)?;
        self.expr(second, d)
    }

    #[inline(never)]
    fn expr_list(&mut self, values: &[CoreExpr], d: usize) -> R<()> {
        for value in values {
            self.expr(value, d)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_fields(&mut self, fields: &[(String, CoreExpr)], d: usize) -> R<()> {
        for (_, value) in fields {
            self.expr(value, d)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_call(&mut self, e: &CoreExpr, d: usize) -> R<()> {
        let (CoreExprKind::Call { callee, arguments }
        | CoreExprKind::CallDynSlot {
            callee, arguments, ..
        }) = &e.kind
        else {
            unreachable!("binding call dispatcher")
        };
        self.expr(callee, d + 1)?;
        self.expr_list(arguments, d + 1)
    }

    #[inline(never)]
    fn expr_slice(&mut self, e: &CoreExpr, d: usize) -> R<()> {
        let CoreExprKind::Slice { object, start, end } = &e.kind else {
            unreachable!("binding slice dispatcher")
        };
        self.expr(object, d + 1)?;
        for x in [start, end].into_iter().flatten() {
            self.expr(x, d + 1)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_record_update(&mut self, e: &CoreExpr, d: usize) -> R<()> {
        let CoreExprKind::RecordUpdate { base, fields, .. } = &e.kind else {
            unreachable!("binding record dispatcher")
        };
        self.expr(base, d + 1)?;
        self.expr_fields(fields, d + 1)
    }

    #[inline(never)]
    fn expr_sequence(&mut self, e: &CoreExpr, d: usize) -> R<()> {
        let CoreExprKind::Sequence { bindings, value } = &e.kind else {
            unreachable!("binding sequence dispatcher")
        };
        self.scopes.push(BTreeMap::new());
        for (n, x) in bindings {
            self.expr(x, d + 1)?;
            self.scopes.last_mut().unwrap().insert(n.clone(), None);
        }
        self.expr(value, d + 1)?;
        self.scopes.pop();
        Ok(())
    }

    #[inline(never)]
    fn expr_closure(&mut self, e: &CoreExpr, d: usize) -> R<()> {
        let CoreExprKind::Closure {
            parameters, body, ..
        } = &e.kind
        else {
            unreachable!("binding closure dispatcher")
        };
        self.scopes.push(BTreeMap::new());
        for p in parameters {
            self.scopes.last_mut().unwrap().insert(p.name.clone(), None);
        }
        self.block(body, d + 1)?;
        self.scopes.pop();
        Ok(())
    }

    #[inline(never)]
    fn expr_if(&mut self, e: &CoreExpr, d: usize) -> R<()> {
        let CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } = &e.kind
        else {
            unreachable!("binding if dispatcher")
        };
        self.expr(condition, d + 1)?;
        self.expr(then_value, d + 1)?;
        self.expr(else_value, d + 1)
    }
}
