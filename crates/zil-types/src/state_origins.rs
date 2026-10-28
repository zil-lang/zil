//! Private source declarations for the finite two-binding owning State profile.
//! A declaration restricts a use; it never supplies a runtime value or loan.
use super::*;
use std::borrow::Cow;
use std::rc::Rc;
use zil_base::NodeId;

type Position = (u32, Option<u32>);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AliasKey {
    Local(u32),
    Expression(NodeId),
    Prepared(u64),
}
#[derive(Clone)]
struct Alias {
    parent: Position,
    mutable: bool,
    borrowed_at: u64,
    ancestors: Vec<u64>,
    invalidated_at: Option<u64>,
}
impl Alias {
    fn work(&self) -> usize {
        self.ancestors.len().saturating_add(1)
    }
}
#[derive(Clone)]
struct MoveFlow {
    live: bool,
    moved: std::collections::BTreeSet<Position>,
    aliases: BTreeMap<AliasKey, Vec<Alias>>,
    alias_entries: usize,
}
impl MoveFlow {
    fn work(&self) -> usize {
        self.moved
            .len()
            .saturating_add(self.aliases.len())
            .saturating_add(self.alias_entries)
            .saturating_add(1)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Move,
    Write,
}

/// This is the symbolic declaration's position ledger. A field take transfers
/// its value to a fresh local; its source position stays unavailable. No kind
/// (including Fn) supplies a Copy promise for the declaration's parameter.
struct MoveWalk<'a, 's> {
    uses: &'a BTreeMap<NodeId, Shape>,
    bounds: &'a [CoreCallableParameter],
    scopes: Vec<BTreeMap<String, u32>>,
    next: u32,
    generation: u64,
    flow: MoveFlow,
    loops: Vec<(Option<MoveFlow>, Option<MoveFlow>)>,
    loop_alias_uses: Vec<std::collections::BTreeSet<u64>>,
    state: &'s mut closures::State,
    limits: &'a Limits,
}
impl MoveWalk<'_, '_> {
    fn node(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "State source position depth exceeded",
                Some(span),
            ));
        }
        self.state.snapshot(1, span)
    }
    fn clone_flow(&mut self, flow: &MoveFlow, span: Span) -> R<MoveFlow> {
        self.state.snapshot(flow.work(), span)?;
        Ok(flow.clone())
    }
    fn snapshot_flow(&mut self, span: Span) -> R<MoveFlow> {
        self.state.snapshot(self.flow.work(), span)?;
        Ok(self.flow.clone())
    }
    fn join(&mut self, mut a: MoveFlow, b: MoveFlow, span: Span) -> R<MoveFlow> {
        self.state
            .snapshot(a.work().saturating_add(b.work()), span)?;
        if !a.live {
            return Ok(b);
        }
        if b.live {
            a.moved.extend(b.moved);
            for (key, aliases) in b.aliases {
                self.state
                    .snapshot(helpers::plan::tree_work(a.aliases.len()), span)?;
                let entry = a.aliases.entry(key).or_default();
                for alias in aliases {
                    self.state.snapshot(entry.len().saturating_add(1), span)?;
                    if let Some(previous) = entry.iter_mut().find(|previous| {
                        previous.parent == alias.parent && previous.borrowed_at == alias.borrowed_at
                    }) {
                        previous.invalidated_at = previous.invalidated_at.or(alias.invalidated_at);
                    } else {
                        a.alias_entries = a.alias_entries.saturating_add(alias.work());
                        entry.push(alias);
                    }
                }
            }
        }
        Ok(a)
    }
    fn aliases(&mut self, key: AliasKey, span: Span) -> R<Vec<Alias>> {
        self.state
            .snapshot(helpers::plan::tree_work(self.flow.aliases.len()), span)?;
        let values = self.flow.aliases.get(&key).map_or(&[][..], Vec::as_slice);
        self.state.snapshot(values.len(), span)?;
        self.state.snapshot(
            values
                .iter()
                .fold(0usize, |n, a| n.saturating_add(a.work())),
            span,
        )?;
        Ok(values.to_vec())
    }
    fn set_aliases(&mut self, key: AliasKey, values: Vec<Alias>, span: Span) -> R<()> {
        self.state.snapshot(
            helpers::plan::tree_work(self.flow.aliases.len()).saturating_add(values.len()),
            span,
        )?;
        self.flow.alias_entries = self.flow.alias_entries.saturating_add(
            values
                .iter()
                .fold(0usize, |n, a| n.saturating_add(a.work())),
        );
        let old = if values.is_empty() {
            self.flow.aliases.remove(&key)
        } else {
            self.flow.aliases.insert(key, values)
        };
        if let Some(old) = old {
            self.state.snapshot(old.len(), span)?;
            self.flow.alias_entries -= old.iter().fold(0usize, |n, a| n.saturating_add(a.work()));
        }
        Ok(())
    }
    fn copy_aliases(&mut self, from: AliasKey, to: AliasKey, span: Span) -> R<()> {
        let values = self.aliases(from, span)?;
        self.set_aliases(to, values, span)
    }
    fn check_aliases(&mut self, key: AliasKey, span: Span) -> R<()> {
        if !self.flow.live {
            return Ok(());
        }
        self.state
            .snapshot(helpers::plan::tree_work(self.flow.aliases.len()), span)?;
        if let Some(values) = self.flow.aliases.get(&key) {
            self.state.snapshot(values.len(), span)?;
            if values.iter().any(|alias| alias.invalidated_at.is_some()) {
                return Err(error(
                    "E-STATE-ALIAS",
                    span,
                    "State field alias outlived a conflicting access to its original parent value",
                ));
            }
            for uses in &mut self.loop_alias_uses {
                self.state.snapshot(
                    values.len().saturating_mul(helpers::plan::tree_work(
                        uses.len().saturating_add(values.len()),
                    )),
                    span,
                )?;
                uses.extend(values.iter().map(|alias| alias.borrowed_at));
            }
        }
        Ok(())
    }
    fn next_generation(&mut self, span: Span) -> R<u64> {
        self.state.snapshot(1, span)?;
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "State alias generation exceeded", Some(span))
        })?;
        Ok(self.generation)
    }
    fn overlapping(a: Position, b: Position) -> bool {
        a.0 == b.0 && (a.1.is_none() || b.1.is_none() || a.1 == b.1)
    }
    fn alias_access(&mut self, active: &[Alias], mutable: bool, span: Span) -> R<()> {
        if !self.flow.live || active.is_empty() {
            return Ok(());
        }
        self.state.snapshot(active.len(), span)?;
        let work = active
            .iter()
            .fold(0usize, |n, a| n.saturating_add(a.work()));
        self.state.snapshot(
            self.flow.work().saturating_mul(work.saturating_add(1)),
            span,
        )?;
        let generation = self.next_generation(span)?;
        for values in self.flow.aliases.values_mut() {
            for alias in values {
                if active.iter().any(|current| {
                    Self::overlapping(current.parent, alias.parent)
                        && (mutable || alias.mutable)
                        && current.borrowed_at != alias.borrowed_at
                        && !current.ancestors.contains(&alias.borrowed_at)
                }) {
                    alias.invalidated_at.get_or_insert(generation);
                }
            }
        }
        Ok(())
    }
    fn fresh_alias(&mut self, parent: Position, mutable: bool, span: Span) -> R<Vec<Alias>> {
        let borrowed_at = self.next_generation(span)?;
        let values = vec![Alias {
            parent,
            mutable,
            borrowed_at,
            ancestors: Vec::new(),
            invalidated_at: None,
        }];
        self.alias_access(&values, mutable, span)?;
        Ok(values)
    }
    fn derive_aliases(
        &mut self,
        mut values: Vec<Alias>,
        mutable: bool,
        activate: bool,
        span: Span,
    ) -> R<Vec<Alias>> {
        self.state.snapshot(values.len(), span)?;
        for alias in &values {
            if alias.invalidated_at.is_some() || (mutable && !alias.mutable) {
                return Err(error(
                    "E-STATE-ALIAS",
                    span,
                    "State field reborrow lacks a live compatible parent",
                ));
            }
            if alias.ancestors.len() >= self.limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "State alias parent depth exceeded",
                    Some(span),
                ));
            }
        }
        if activate {
            self.alias_access(&values, mutable, span)?;
        }
        for alias in &mut values {
            self.state
                .snapshot(alias.ancestors.len().saturating_add(1), span)?;
            alias.ancestors.push(alias.borrowed_at);
            alias.borrowed_at = self.next_generation(span)?;
            alias.mutable = mutable;
        }
        Ok(values)
    }
    fn invalidate_aliases(&mut self, position: Position, span: Span) -> R<()> {
        if !self.flow.live || self.flow.aliases.is_empty() {
            return Ok(());
        }
        self.state.snapshot(
            self.flow
                .aliases
                .len()
                .saturating_add(self.flow.alias_entries)
                .saturating_add(1),
            span,
        )?;
        let generation = self.next_generation(span)?;
        for values in self.flow.aliases.values_mut() {
            for alias in values {
                let same_parent = Self::overlapping(alias.parent, position);
                if same_parent && alias.invalidated_at.is_none() {
                    alias.invalidated_at = Some(generation);
                }
            }
        }
        Ok(())
    }
    fn remember_alias(&mut self, expression: &CoreExpr, shape: &Shape, depth: usize) -> R<()> {
        let address = matches!(shape, Shape::Address(_))
            || matches!(shape, Shape::Union { result, .. } if matches!(result.as_ref(), Shape::Address(_)));
        let projection = if let CoreExprKind::Member { object, .. } = &expression.kind {
            matches!(self.shape(object)?, Shape::Address(_))
        } else {
            false
        };
        if !shape.has_origin() || (!address && !projection) {
            return Ok(());
        }
        let values = match &expression.kind {
            CoreExprKind::Name(_) => {
                if let Some((root, None)) = self.position(expression, depth + 1)? {
                    self.aliases(AliasKey::Local(root), expression.span)?
                } else {
                    Vec::new()
                }
            }
            CoreExprKind::Member { object, member } if projection => {
                let Shape::Address(base) = self.shape(object)? else {
                    unreachable!()
                };
                let Shape::Package(fields) = base.as_ref() else {
                    return Err(unsupported(
                        expression.span,
                        "State field address has no actual Package parent",
                    ));
                };
                self.state.snapshot(
                    fields.len().saturating_mul(member.len().saturating_add(1)),
                    expression.span,
                )?;
                let field = fields
                    .iter()
                    .position(|(name, _)| name == member)
                    .ok_or_else(|| unsupported(expression.span, "State alias field is absent"))?
                    as u32;
                let source = self.aliases(AliasKey::Expression(object.id), expression.span)?;
                let mut result = Vec::new();
                for mut alias in source {
                    if alias.parent.1.is_none() {
                        alias.parent.1 = Some(field);
                    }
                    let mutable = alias.mutable;
                    self.state
                        .snapshot(result.len().saturating_add(2), expression.span)?;
                    result.extend(self.derive_aliases(
                        vec![alias],
                        mutable,
                        false,
                        expression.span,
                    )?);
                }
                result
            }
            CoreExprKind::Unary {
                op: op @ (UnaryOp::Borrow | UnaryOp::BorrowMut),
                operand,
            } => {
                let inherited = self.aliases(AliasKey::Expression(operand.id), expression.span)?;
                let mutable = *op == UnaryOp::BorrowMut;
                if inherited.is_empty() {
                    if let Some(parent) = self.position(operand, depth + 1)? {
                        self.fresh_alias(parent, mutable, expression.span)?
                    } else {
                        Vec::new()
                    }
                } else {
                    self.derive_aliases(inherited, mutable, true, expression.span)?
                }
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Unary { operand: value, .. }
            | CoreExprKind::Sequence { value, .. }
            | CoreExprKind::Try(value) => {
                self.aliases(AliasKey::Expression(value.id), expression.span)?
            }
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                let mut values =
                    self.aliases(AliasKey::Expression(then_value.id), expression.span)?;
                let other = self.aliases(AliasKey::Expression(else_value.id), expression.span)?;
                self.state
                    .snapshot(values.len().saturating_add(other.len()), expression.span)?;
                values.extend(other);
                values
            }
            CoreExprKind::Call { callee, arguments }
                if matches!(
                    dynamic_store::primitive(callee),
                    Some("store_at" | "store_at_mut")
                ) =>
            {
                if let Some(argument) = arguments.first()
                    && let Some(parent) = self.position(argument, depth + 1)?
                {
                    self.fresh_alias(
                        parent,
                        dynamic_store::primitive(callee) == Some("store_at_mut"),
                        expression.span,
                    )?
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        if values.is_empty() && self.flow.live {
            return Err(unsupported(
                expression.span,
                "State field address lost its actual parent position",
            ));
        }
        self.set_aliases(AliasKey::Expression(expression.id), values, expression.span)
    }
    fn bind(&mut self, name: &str, span: Span) -> R<()> {
        if self.next as usize >= self.limits.max_locals {
            return Err(Diagnostic::limit(
                Phase::Types,
                "State source position count exceeded",
                Some(span),
            ));
        }
        let scope = self.scopes.last_mut().expect("State move scope");
        self.state.snapshot(
            name.len()
                .saturating_mul(helpers::plan::tree_work(scope.len()))
                .saturating_add(name.len())
                .saturating_add(1),
            span,
        )?;
        scope.insert(name.to_owned(), self.next);
        self.next += 1;
        Ok(())
    }
    fn shape(&mut self, expression: &CoreExpr) -> R<Shape> {
        self.state
            .snapshot(helpers::plan::tree_work(self.uses.len()), expression.span)?;
        let shape = self.uses.get(&expression.id).unwrap_or(&Shape::None);
        self.state.snapshot(shape.work(), expression.span)?;
        Ok(shape.clone())
    }
    fn position(&mut self, expression: &CoreExpr, depth: usize) -> R<Option<Position>> {
        self.node(depth, expression.span)?;
        Ok(match &expression.kind {
            CoreExprKind::Name(name) => {
                let mut found = None;
                for scope in self.scopes.iter().rev() {
                    self.state.snapshot(
                        name.len()
                            .saturating_add(1)
                            .saturating_mul(helpers::plan::tree_work(scope.len())),
                        expression.span,
                    )?;
                    if let Some(id) = scope.get(name) {
                        found = Some((*id, None));
                        break;
                    }
                }
                found
            }
            CoreExprKind::Member { object, member } => {
                let base = self.position(object, depth + 1)?;
                let shape = self.shape(object)?;
                let shape = if let Shape::Address(inner) = &shape {
                    inner.as_ref()
                } else {
                    &shape
                };
                if let (Some((root, None)), Shape::Package(fields)) = (base, shape) {
                    self.state.snapshot(
                        fields.len().saturating_mul(member.len().saturating_add(1)),
                        expression.span,
                    )?;
                    fields
                        .iter()
                        .position(|(name, _)| name == member)
                        .map(|field| (root, Some(field as u32)))
                } else {
                    None
                }
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::Unary {
                operand: value,
                op: UnaryOp::Give | UnaryOp::Dereference,
            } => self.position(value, depth + 1)?,
            _ => None,
        })
    }
    fn access(
        &mut self,
        expression: &CoreExpr,
        shape: &Shape,
        access: Access,
        force_store: bool,
        depth: usize,
    ) -> R<()> {
        if !self.flow.live || !shape.has_origin() {
            return Ok(());
        }
        let affine = matches!(
            shape,
            Shape::Callable(_) | Shape::Package(_) | Shape::Union { .. }
        ) || force_store;
        if access == Access::Move
            && affine
            && let CoreExprKind::Member { object, .. } = &expression.kind
            && matches!(self.shape(object)?, Shape::Address(_))
        {
            return Err(error(
                "E-STATE-MOVE",
                expression.span,
                "a borrowed State position cannot transfer an owned callback",
            ));
        }
        let Some(position @ (root, field)) = self.position(expression, depth + 1)? else {
            return Ok(());
        };
        if access != Access::Write {
            self.check_aliases(AliasKey::Local(root), expression.span)?;
        }
        self.state
            .snapshot(self.flow.moved.len().saturating_add(2), expression.span)?;
        if access != Access::Write
            && (self.flow.moved.contains(&(root, None))
                || self.flow.moved.contains(&position)
                || (access == Access::Move
                    && field.is_none()
                    && self.flow.moved.iter().any(|(owner, _)| *owner == root)))
        {
            return Err(error(
                "E-STATE-MOVE",
                expression.span,
                "State source position was already transferred on a reachable path",
            ));
        }
        if access == Access::Move && affine {
            self.invalidate_aliases(position, expression.span)?;
            self.flow.moved.insert(position);
        }
        Ok(())
    }
    fn expr(&mut self, expression: &CoreExpr, access: Access, depth: usize) -> R<()> {
        self.node(depth, expression.span)?;
        let shape = self.shape(expression)?;
        match &expression.kind {
            CoreExprKind::Name(_) => self.access(expression, &shape, access, false, depth)?,
            CoreExprKind::Member { object, .. } => {
                self.expr(object, Access::Read, depth + 1)?;
                self.access(expression, &shape, access, false, depth)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                let target = if let CoreExprKind::TypeApply { callee, .. } = &callee.kind {
                    callee.as_ref()
                } else {
                    callee.as_ref()
                };
                let target_shape = self.shape(target)?;
                self.state.snapshot(self.bounds.len(), target.span)?;
                let kind = target_shape
                    .slot()
                    .and_then(|slot| self.bounds.iter().find(|bound| bound.generic_index == slot))
                    .map(|bound| bound.kind);
                let consume = kind == Some(CallableKind::FnOnce);
                if consume && matches!(target_shape, Shape::Address(_)) {
                    return Err(error(
                        "E-STATE-ALIAS",
                        target.span,
                        "FnOnce requires an owning State callback position",
                    ));
                }
                self.expr(
                    target,
                    if consume { Access::Move } else { Access::Read },
                    depth + 1,
                )?;
                let prepared = if matches!(kind, Some(CallableKind::Fn | CallableKind::FnMut)) {
                    let mutable = kind == Some(CallableKind::FnMut);
                    let aliases = self.aliases(AliasKey::Expression(target.id), target.span)?;
                    let values = if aliases.is_empty() {
                        if let Some(parent) = self.position(target, depth + 1)? {
                            self.fresh_alias(parent, mutable, target.span)?
                        } else {
                            return Err(unsupported(
                                target.span,
                                "State callee has no preserved parent position",
                            ));
                        }
                    } else {
                        self.derive_aliases(aliases, mutable, true, target.span)?
                    };
                    let key = AliasKey::Prepared(self.next_generation(target.span)?);
                    self.set_aliases(key, values, target.span)?;
                    Some(key)
                } else {
                    None
                };
                for argument in arguments {
                    self.expr(argument, Access::Move, depth + 1)?;
                }
                // Callee preparation precedes its arguments, but an argument
                // cannot make the prepared borrowed environment valid again.
                if let Some(key) = prepared {
                    self.check_aliases(key, target.span)?;
                    self.set_aliases(key, Vec::new(), target.span)?;
                }
                if !consume {
                    self.access(target, &target_shape, Access::Read, false, depth + 1)?;
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => self.expr(operand, Access::Read, depth + 1)?,
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => {
                self.expr(operand, Access::Read, depth + 1)?;
                let shape = self.shape(operand)?;
                self.access(operand, &shape, Access::Move, true, depth + 1)?;
            }
            CoreExprKind::Ascribed { value, .. } | CoreExprKind::CompileTime(value) => {
                self.expr(value, access, depth + 1)?
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::Try(operand)
            | CoreExprKind::IntegerConvert { value: operand, .. } => {
                self.expr(operand, Access::Move, depth + 1)?
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                self.expr(left, Access::Move, depth + 1)?;
                self.expr(right, Access::Move, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, Access::Read, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expr(value, Access::Move, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    self.expr(value, Access::Move, depth + 1)?;
                }
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.expr(value, Access::Move, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expr(base, Access::Move, depth + 1)?;
                for (_, value) in fields {
                    self.expr(value, Access::Move, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.state.snapshot(1, expression.span)?;
                self.scopes.push(BTreeMap::new());
                for (name, value) in bindings {
                    self.expr(value, Access::Move, depth + 1)?;
                    self.bind(name, value.span)?;
                    self.copy_aliases(
                        AliasKey::Expression(value.id),
                        AliasKey::Local(self.next - 1),
                        value.span,
                    )?;
                }
                self.expr(value, access, depth + 1)?;
                self.pop_scope(expression.span)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, Access::Move, depth + 1)?;
                let before = self.snapshot_flow(expression.span)?;
                self.expr(then_value, access, depth + 1)?;
                let after = self.snapshot_flow(expression.span)?;
                self.flow = before;
                self.expr(else_value, access, depth + 1)?;
                let other = self.snapshot_flow(expression.span)?;
                self.flow = self.join(after, other, expression.span)?;
            }
            // The declaration-origin pass already rejects captures of these
            // sources; an independent closure has no source position to move.
            CoreExprKind::Closure { .. } => {}
            _ => {}
        }
        self.remember_alias(expression, &shape, depth + 1)?;
        Ok(())
    }
    fn pattern(&mut self, pattern: &CorePattern, depth: usize) -> R<()> {
        self.node(depth, pattern.span)?;
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
    fn pop_scope(&mut self, span: Span) -> R<()> {
        let scope = self.scopes.pop().expect("State move scope");
        self.state.snapshot(scope.len(), span)?;
        for root in scope.into_values() {
            self.invalidate_aliases((root, None), span)?;
            self.state.snapshot(self.flow.moved.len(), span)?;
            self.flow.moved.retain(|(owner, _)| *owner != root);
            self.set_aliases(AliasKey::Local(root), Vec::new(), span)?;
        }
        Ok(())
    }
    fn loop_body(
        &mut self,
        body: &CoreBlock,
        before: MoveFlow,
        exit: MoveFlow,
        depth: usize,
    ) -> R<()> {
        self.state.snapshot(2, body.span)?;
        self.loops.push((None, None));
        self.loop_alias_uses.push(Default::default());
        self.block(body, depth + 1)?;
        let (breaks, continues) = self.loops.pop().expect("State move loop");
        let used = self.loop_alias_uses.pop().expect("State alias loop");
        let mut back = self.snapshot_flow(body.span)?;
        if let Some(continues) = continues {
            back = self.join(back, continues, body.span)?;
        }
        self.state.snapshot(
            before.moved.len().saturating_add(back.moved.len()),
            body.span,
        )?;
        if back.live && !back.moved.is_subset(&before.moved) {
            return Err(error(
                "E-STATE-MOVE",
                body.span,
                "a repeated loop path cannot transfer a declared State position",
            ));
        }
        if back.live {
            self.state.snapshot(
                before
                    .alias_entries
                    .saturating_mul(helpers::plan::tree_work(used.len()))
                    .saturating_add(before.aliases.len()),
                body.span,
            )?;
            self.state.snapshot(
                before
                    .alias_entries
                    .saturating_mul(helpers::plan::tree_work(before.alias_entries)),
                body.span,
            )?;
            let previous = before
                .aliases
                .values()
                .flatten()
                .filter(|alias| used.contains(&alias.borrowed_at))
                .map(|alias| alias.borrowed_at)
                .collect::<std::collections::BTreeSet<_>>();
            self.state.snapshot(
                back.alias_entries
                    .saturating_mul(helpers::plan::tree_work(previous.len()))
                    .saturating_add(back.aliases.len()),
                body.span,
            )?;
            if back.aliases.values().flatten().any(|alias| {
                alias.invalidated_at.is_some() && previous.contains(&alias.borrowed_at)
            }) {
                return Err(error(
                    "E-STATE-ALIAS",
                    body.span,
                    "a repeated loop path would reuse an invalidated State field alias",
                ));
            }
        }
        // A loop can replace a field and therefore restore its owning
        // position while permanently invalidating an older borrowed alias.
        let exit = self.join(exit, back, body.span)?;
        self.flow = if let Some(breaks) = breaks {
            self.join(exit, breaks, body.span)?
        } else {
            exit
        };
        Ok(())
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.node(depth, block.span)?;
        self.state.snapshot(1, block.span)?;
        self.scopes.push(BTreeMap::new());
        for statement in &block.statements {
            self.node(depth + 1, statement.span)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name, initializer, ..
                } => {
                    if let Some(value) = initializer {
                        self.expr(value, Access::Move, depth + 1)?;
                    }
                    self.bind(name, statement.span)?;
                    if let Some(value) = initializer {
                        self.copy_aliases(
                            AliasKey::Expression(value.id),
                            AliasKey::Local(self.next - 1),
                            statement.span,
                        )?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expr(target, Access::Write, depth + 1)?;
                    self.expr(value, Access::Move, depth + 1)?;
                    if let Some((owner, field)) = self.position(target, depth + 1)? {
                        self.invalidate_aliases((owner, field), statement.span)?;
                        self.state.snapshot(self.flow.moved.len(), statement.span)?;
                        self.flow.moved.retain(|(root, actual)| {
                            *root != owner || (field.is_some() && *actual != field)
                        });
                        if field.is_none() {
                            self.copy_aliases(
                                AliasKey::Expression(value.id),
                                AliasKey::Local(owner),
                                statement.span,
                            )?;
                        }
                    }
                }
                CoreStatementKind::Expression(value) => {
                    self.expr(value, Access::Move, depth + 1)?
                }
                CoreStatementKind::Return(value) => {
                    if let Some(value) = value {
                        self.expr(value, Access::Move, depth + 1)?;
                    }
                    self.flow.live = false;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, Access::Move, depth + 1)?;
                    let before = self.snapshot_flow(statement.span)?;
                    self.block(then_body, depth + 1)?;
                    let after = self.snapshot_flow(statement.span)?;
                    self.flow = before;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                    let other = self.snapshot_flow(statement.span)?;
                    self.flow = self.join(after, other, statement.span)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    self.expr(value, Access::Move, depth + 1)?;
                    let before = self.snapshot_flow(statement.span)?;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    self.pop_scope(statement.span)?;
                    let after = self.snapshot_flow(statement.span)?;
                    self.flow = before;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                    let other = self.snapshot_flow(statement.span)?;
                    self.flow = self.join(after, other, statement.span)?;
                }
                CoreStatementKind::While { condition, body } => {
                    let before = self.snapshot_flow(statement.span)?;
                    self.expr(condition, Access::Move, depth + 1)?;
                    let exit = self.snapshot_flow(statement.span)?;
                    self.loop_body(body, before, exit, depth + 1)?;
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let before = self.snapshot_flow(statement.span)?;
                    self.expr(value, Access::Move, depth + 1)?;
                    let exit = self.snapshot_flow(statement.span)?;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, depth + 1)?;
                    self.loop_body(body, before, exit, depth + 1)?;
                    self.pop_scope(statement.span)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    self.expr(start, Access::Move, depth + 1)?;
                    self.expr(end, Access::Move, depth + 1)?;
                    let before = self.snapshot_flow(statement.span)?;
                    let exit = self.clone_flow(&before, statement.span)?;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.bind(name, statement.span)?;
                    self.loop_body(body, before, exit, depth + 1)?;
                    self.pop_scope(statement.span)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expr(value, Access::Move, depth + 1)?;
                    let before = self.snapshot_flow(statement.span)?;
                    let mut joined = MoveFlow {
                        live: false,
                        moved: Default::default(),
                        aliases: Default::default(),
                        alias_entries: 0,
                    };
                    for arm in arms {
                        self.flow = self.clone_flow(&before, statement.span)?;
                        self.state.snapshot(1, statement.span)?;
                        self.scopes.push(BTreeMap::new());
                        self.pattern(&arm.pattern, depth + 1)?;
                        self.block(&arm.body, depth + 1)?;
                        self.pop_scope(statement.span)?;
                        let after = self.snapshot_flow(statement.span)?;
                        joined = self.join(joined, after, statement.span)?;
                    }
                    self.flow = joined;
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body, depth + 1)?
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {
                    let flow = self.snapshot_flow(statement.span)?;
                    let Some(mut frame) = self.loops.pop() else {
                        return Err(error(
                            "E-CONTROL",
                            statement.span,
                            "State position loop exit is outside a loop",
                        ));
                    };
                    let target = if matches!(statement.kind, CoreStatementKind::Break) {
                        &mut frame.0
                    } else {
                        &mut frame.1
                    };
                    *target = Some(if let Some(previous) = target.take() {
                        self.join(previous, flow, statement.span)?
                    } else {
                        flow
                    });
                    self.loops.push(frame);
                    self.flow.live = false;
                }
                _ => {}
            }
        }
        self.pop_scope(block.span)?;
        Ok(())
    }
}

fn check_moves(
    function: &CoreFunction,
    uses: &BTreeMap<NodeId, Shape>,
    bounds: &[CoreCallableParameter],
    state: &mut closures::State,
    limits: &Limits,
) -> R<()> {
    state.snapshot(1, function.span)?;
    let mut walk = MoveWalk {
        uses,
        bounds,
        scopes: vec![BTreeMap::new()],
        next: 0,
        generation: 0,
        flow: MoveFlow {
            live: true,
            moved: Default::default(),
            aliases: Default::default(),
            alias_entries: 0,
        },
        loops: Vec::new(),
        loop_alias_uses: Vec::new(),
        state,
        limits,
    };
    for parameter in &function.parameters {
        walk.bind(&parameter.name, parameter.span)?;
    }
    walk.block(&function.body, 0)
}

pub(super) fn bounds(function: &CoreFunction) -> Cow<'_, [CoreCallableParameter]> {
    let Some(state) = function.state_parameters.first() else {
        return Cow::Borrowed(&function.callable_parameters);
    };
    Cow::Owned(
        state
            .bindings
            .iter()
            .map(|binding| CoreCallableParameter {
                index: state.index,
                generic_index: binding.generic_index,
                generic_name: binding.generic_name.clone(),
                kind: binding.kind,
                parameters: binding.parameters.clone(),
                result: binding.result.clone(),
            })
            .collect(),
    )
}

pub(super) fn validate_header(function: &CoreFunction) -> R<()> {
    if function.state_parameters.is_empty() {
        return Ok(());
    }
    let reject = || {
        error(
            "E-STATE-PARAMETERS",
            function.span,
            "State declarations require one owned parameter with two distinct ordered callable bindings",
        )
    };
    let [state] = function.state_parameters.as_slice() else {
        return Err(reject());
    };
    if !function.callable_parameters.is_empty()
        || !function.type_parameters.is_empty()
        || function.callable_return.is_some()
        || function.foreign.is_some()
        || function.receiver.is_some()
        || state.bindings.len() != 2
        || state.index as usize >= function.parameters.len()
        || function.parameters[state.index as usize].mode != PassingMode::Infer
    {
        return Err(reject());
    }
    for (index, binding) in state.bindings.iter().enumerate() {
        if binding.generic_index != index as u32
            || binding.generic_name.is_empty()
            || binding.kind == CallableKind::FunctionPointer
            || !binding.result.is_copy_value()
            || !closures::supported_signature(&binding.parameters, &binding.result)
            || binding.fields.len() != 1
        {
            return Err(reject());
        }
    }
    if state.bindings[0].generic_name == state.bindings[1].generic_name
        || state.bindings[0].fields == state.bindings[1].fields
    {
        return Err(reject());
    }
    Ok(())
}

/// Called only after aggregate resolution and the input's bounded metadata walk.
pub(super) fn validate_shape(function: &CoreFunction) -> R<()> {
    validate_header(function)?;
    let Some(state) = function.state_parameters.first() else {
        return Ok(());
    };
    let reject = || {
        error(
            "E-STATE-PARAMETERS",
            function.span,
            "State selectors must cover exactly the two direct fields of their declared binders",
        )
    };
    let Type::Package { fields, .. } = &function.parameters[state.index as usize].ty else {
        return Err(reject());
    };
    let mut covered = 0;
    for (index, (_, ty)) in fields.iter().enumerate() {
        if let Some(binding) = state
            .bindings
            .iter()
            .find(|binding| matches!(ty, Type::Named(name) if name == &binding.generic_name))
        {
            if binding.fields.as_slice() != [index as u32] {
                return Err(reject());
            }
            covered += 1;
        } else if !(ty.is_copy_value()
            || matches!(ty, Type::Managed(_) if resources::supported(ty)))
        {
            return Err(reject());
        }
    }
    if covered != 2 {
        return Err(reject());
    }
    Ok(())
}

pub(super) fn match_type(
    pattern: &Type,
    actual: &Type,
    bounds: &[CoreCallableParameter],
    span: Span,
) -> R<helpers::Bindings> {
    let reject = || {
        error(
            "E-TYPE-MISMATCH",
            span,
            "State requires the complete nominal shape and both concrete callback bindings",
        )
    };
    let (
        Type::Package {
            name: a,
            fields: af,
        },
        Type::Package {
            name: b,
            fields: bf,
        },
    ) = (pattern, actual)
    else {
        return Err(reject());
    };
    if callable_fields::nominal_base(a) != callable_fields::nominal_base(b) || af.len() != bf.len()
    {
        return Err(reject());
    }
    let mut bindings = helpers::Bindings::default();
    for ((an, wanted), (bn, got)) in af.iter().zip(bf) {
        if an != bn {
            return Err(reject());
        }
        if let Some(bound) = bounds
            .iter()
            .find(|bound| matches!(wanted, Type::Named(name) if name == &bound.generic_name))
        {
            let Type::Closure(id) = got else {
                return Err(reject());
            };
            bindings.bind(bound.generic_index, *id, span)?;
        } else if wanted != got {
            return Err(reject());
        }
    }
    for bound in bounds {
        bindings.get(bound.generic_index, span)?;
    }
    Ok(bindings)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Shape {
    None,
    Callable(u32),
    Package(Vec<(String, Shape)>),
    Address(Box<Shape>),
    Store(Box<Shape>),
    Union {
        error: Box<Shape>,
        result: Box<Shape>,
    },
}
impl Shape {
    fn work(&self) -> usize {
        match self {
            Self::Package(fields) => fields.iter().fold(1usize, |n, (name, ty)| {
                n.saturating_add(name.len()).saturating_add(ty.work())
            }),
            Self::Address(inner) | Self::Store(inner) => 1usize.saturating_add(inner.work()),
            Self::Union { error, result } => 1usize
                .saturating_add(error.work())
                .saturating_add(result.work()),
            _ => 1,
        }
    }
    fn has_origin(&self) -> bool {
        match self {
            Self::None => false,
            Self::Callable(_) => true,
            Self::Package(fields) => fields.iter().any(|(_, field)| field.has_origin()),
            Self::Address(inner) | Self::Store(inner) => inner.has_origin(),
            Self::Union { error, result } => error.has_origin() || result.has_origin(),
        }
    }
    fn slot(&self) -> Option<u32> {
        match self {
            Self::Callable(slot) => Some(*slot),
            Self::Address(inner) => inner.slot(),
            _ => None,
        }
    }
    fn field(&self, index: u32) -> Option<&Shape> {
        match self {
            Self::Package(fields) => fields.get(index as usize).map(|(_, shape)| shape),
            Self::Address(inner) => inner.field(index),
            _ => None,
        }
    }
}

pub(super) struct Uses {
    bindings: helpers::Bindings,
    declarations: Vec<CoreCallableParameter>,
    expressions: BTreeMap<NodeId, Shape>,
}
impl Uses {
    pub(super) fn lookup_work(&self) -> usize {
        helpers::plan::tree_work(self.expressions.len()).saturating_add(self.declarations.len())
    }
    pub(super) fn declaration(
        &self,
        expression: &CoreExpr,
        field: Option<u32>,
    ) -> Option<(ClosureId, CallableKind)> {
        let mut shape = self.expressions.get(&expression.id)?;
        if let Some(field) = field {
            shape = shape.field(field)?;
        }
        let slot = shape.slot()?;
        let bound = self
            .declarations
            .iter()
            .find(|bound| bound.generic_index == slot)?;
        Some((self.bindings.0[slot as usize]?, bound.kind))
    }
}

impl Builder<'_, '_> {
    pub(super) fn declared_state_kind(
        &mut self,
        expression: &CoreExpr,
        field: Option<u32>,
        id: ClosureId,
    ) -> R<Option<CallableKind>> {
        let Some(uses) = &self.state_uses else {
            return Ok(None);
        };
        self.closures
            .snapshot(uses.lookup_work(), expression.span)?;
        let Some((expected, kind)) = uses.declaration(expression, field) else {
            return Ok(None);
        };
        if expected != id {
            return Err(error(
                "E-STATE-ORIGIN",
                expression.span,
                "State declaration does not match the actual callable type at this use",
            ));
        }
        let pending =
            self.closures.pending.get(id.0 as usize).ok_or_else(|| {
                unsupported(expression.span, "State callable definition is absent")
            })?;
        self.closures
            .snapshot(pending.captures.len().saturating_add(1), expression.span)?;
        if !helpers::compatible(self.closures.pending[id.0 as usize].callable_kind(), kind) {
            return Err(error(
                "E-CLOSURE-MODE",
                expression.span,
                "actual State field does not implement its declaration",
            ));
        }
        Ok(Some(kind))
    }
    pub(super) fn require_state_kind(
        &mut self,
        expression: &CoreExpr,
        field: Option<u32>,
        id: ClosureId,
        requested: CallableKind,
    ) -> R<()> {
        if self
            .declared_state_kind(expression, field, id)?
            .is_some_and(|kind| !helpers::compatible(kind, requested))
        {
            return Err(error(
                "E-CLOSURE-MODE",
                expression.span,
                "State declaration does not provide the requested helper capability",
            ));
        }
        Ok(())
    }
}

/// The side table belongs to this source instance, not to a ClosureId or a
/// globally specialized Package. Ordinary T instances remain shared.
pub(super) fn collect(
    function: &CoreFunction,
    bindings: helpers::Bindings,
    context: &helpers::Context,
    state: &mut closures::State,
    limits: &Limits,
) -> R<Option<Uses>> {
    if function.state_parameters.is_empty()
        && !callable_fields::dual_constructor(
            &function.callable_parameters,
            &function.return_type,
            |index| function.parameters.get(index).map(|p| &p.ty),
        )
    {
        return Ok(None);
    }
    let span = function.span;
    let (work, _) = inference::helper_snapshot_work(function, limits)?;
    state.snapshot(work, span)?;
    let bounds = bounds(function).into_owned();
    let tables = state.helper_tables.clone();
    let mut walk = Walk {
        bounds: &bounds,
        context,
        tables,
        scopes: vec![BTreeMap::new()],
        expressions: BTreeMap::new(),
        capture: false,
        live: 1,
        loops: Vec::new(),
        state,
        limits,
    };
    for parameter in &function.parameters {
        let shape = walk.ty(&parameter.ty, &[], true, 0, parameter.span)?;
        walk.bind(&parameter.name, shape, parameter.span)?;
    }
    walk.block(&function.body, 0)?;
    check_moves(function, &walk.expressions, &bounds, walk.state, limits)?;
    let expressions = walk.expressions;
    Ok(Some(Uses {
        bindings,
        declarations: bounds,
        expressions,
    }))
}

struct Walk<'a, 's> {
    bounds: &'a [CoreCallableParameter],
    context: &'a helpers::Context,
    tables: Rc<helpers::Tables>,
    scopes: Vec<BTreeMap<String, Shape>>,
    expressions: BTreeMap<NodeId, Shape>,
    capture: bool,
    // Four possible consumed-slot masks for this declaration's two distinct
    // sources. This is source capability checking, not a concrete loan proof.
    live: u8,
    loops: Vec<(u8, u8)>,
    state: &'s mut closures::State,
    limits: &'a Limits,
}
impl Walk<'_, '_> {
    fn node(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "State declaration source depth exceeded",
                Some(span),
            ));
        }
        self.state.snapshot(1, span)
    }
    fn copy(&mut self, shape: &Shape, span: Span) -> R<Shape> {
        self.state.snapshot(shape.work(), span)?;
        Ok(shape.clone())
    }
    fn bind(&mut self, name: &str, shape: Shape, span: Span) -> R<()> {
        let scope = self.scopes.last_mut().expect("State source scope");
        self.state.snapshot(
            name.len()
                .saturating_mul(helpers::plan::tree_work(scope.len()))
                .saturating_add(name.len())
                .saturating_add(1),
            span,
        )?;
        scope.insert(name.to_owned(), shape);
        Ok(())
    }
    fn lookup(&mut self, name: &str, span: Span) -> R<Option<Shape>> {
        for scope in self.scopes.iter().rev() {
            self.state.snapshot(
                (name.len().saturating_add(1))
                    .saturating_mul(helpers::plan::tree_work(scope.len())),
                span,
            )?;
            if let Some(shape) = scope.get(name) {
                self.state.snapshot(shape.work(), span)?;
                return Ok(Some(shape.clone()));
            }
        }
        Ok(None)
    }
    fn ty(
        &mut self,
        ty: &Type,
        substitutions: &[(String, Shape)],
        caller: bool,
        depth: usize,
        span: Span,
    ) -> R<Shape> {
        self.node(depth, span)?;
        Ok(match ty {
            Type::Named(name) => {
                self.state.snapshot(
                    (name.len().saturating_add(1)).saturating_mul(
                        substitutions
                            .len()
                            .saturating_add(self.bounds.len())
                            .saturating_add(1),
                    ),
                    span,
                )?;
                if let Some((_, shape)) = substitutions.iter().find(|(n, _)| n == name) {
                    self.copy(shape, span)?
                } else if caller
                    && let Some(bound) = self.bounds.iter().find(|b| &b.generic_name == name)
                {
                    Shape::Callable(bound.generic_index)
                } else {
                    Shape::None
                }
            }
            Type::Package { fields, .. } => {
                self.state.snapshot(fields.len(), span)?;
                let mut result = Vec::new();
                for (name, ty) in fields {
                    self.state.snapshot(name.len(), span)?;
                    result.push((
                        name.clone(),
                        self.ty(ty, substitutions, caller, depth + 1, span)?,
                    ));
                }
                Shape::Package(result)
            }
            Type::Address { inner, .. } => Shape::Address(Box::new(self.ty(
                inner,
                substitutions,
                caller,
                depth + 1,
                span,
            )?)),
            Type::Managed(inner) | Type::Desc(inner) => {
                if let Type::OwnedStore { element } = inner.as_ref() {
                    Shape::Store(Box::new(self.ty(
                        element,
                        substitutions,
                        caller,
                        depth + 1,
                        span,
                    )?))
                } else {
                    Shape::None
                }
            }
            Type::ErrorUnion { error, result } => Shape::Union {
                error: Box::new(self.ty(error, substitutions, caller, depth + 1, span)?),
                result: Box::new(self.ty(result, substitutions, caller, depth + 1, span)?),
            },
            _ => Shape::None,
        })
    }
    fn signature(&mut self, target: &CoreExpr) -> R<Option<Signature>> {
        let table = &self.tables[&self.context.module];
        let found = match &target.kind {
            CoreExprKind::Name(name) => {
                self.state.snapshot(
                    (name.len().saturating_add(1))
                        .saturating_mul(helpers::plan::tree_work(table.len())),
                    target.span,
                )?;
                if self
                    .scopes
                    .iter()
                    .rev()
                    .any(|scope| scope.contains_key(name))
                {
                    None
                } else {
                    table.get(name)
                }
            }
            CoreExprKind::Member { object, member } => {
                if let CoreExprKind::Name(alias) = &object.kind {
                    self.state.snapshot(
                        alias
                            .len()
                            .saturating_add(member.len())
                            .saturating_add(1)
                            .saturating_mul(helpers::plan::tree_work(table.len()).saturating_add(
                                helpers::plan::tree_work(self.context.imports.len()),
                            )),
                        target.span,
                    )?;
                    if self
                        .scopes
                        .iter()
                        .rev()
                        .any(|scope| scope.contains_key(alias))
                    {
                        None
                    } else {
                        table.get(&format!("{alias}.{member}")).or_else(|| {
                            self.context
                                .imports
                                .get(alias)
                                .and_then(|module| self.tables.get(module))
                                .and_then(|table| table.get(member))
                        })
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(signature) = found {
            let work = inference::signature_snapshot_work("", signature, target.span, self.limits)?;
            self.state.snapshot(work, target.span)?;
            Ok(Some(signature.clone()))
        } else {
            Ok(None)
        }
    }
    fn require(&mut self, shape: &Shape, requested: CallableKind, span: Span) -> R<()> {
        if let Some(slot) = shape.slot() {
            self.state.snapshot(self.bounds.len(), span)?;
            let declared = self
                .bounds
                .iter()
                .find(|b| b.generic_index == slot)
                .ok_or_else(|| unsupported(span, "State declaration source is absent"))?;
            if !helpers::compatible(declared.kind, requested) {
                return Err(error(
                    "E-CLOSURE-MODE",
                    span,
                    "State declaration does not supply the helper's requested callable capability",
                ));
            }
        }
        Ok(())
    }
    fn infer_value_shape(
        &mut self,
        pattern: &Type,
        name: &str,
        actual: &Shape,
        inferred: &mut Option<Shape>,
        span: Span,
    ) -> R<()> {
        let work = inference::type_snapshot_work(pattern, span, self.limits)?;
        self.state
            .snapshot(work.saturating_add(actual.work()), span)?;
        if matches!(pattern, Type::Named(n) if n == name) {
            if let Some(previous) = inferred {
                self.same_origin(previous, actual, span)?;
            } else {
                *inferred = Some(self.copy(actual, span)?);
            }
        } else if let Some(element) = dynamic_store::element(pattern) {
            let Shape::Store(value) = actual else {
                if actual.has_origin() {
                    return Err(error(
                        "E-STATE-ORIGIN",
                        span,
                        "inferred Store source shape differs",
                    ));
                }
                return Ok(());
            };
            self.infer_value_shape(element, name, value, inferred, span)?;
        } else if let Type::ErrorUnion {
            error: error_type,
            result,
        } = pattern
        {
            if let Shape::Union {
                error: failure,
                result: success,
            } = actual
            {
                self.infer_value_shape(error_type, name, failure, inferred, span)?;
                self.infer_value_shape(result, name, success, inferred, span)?;
            } else if actual.has_origin() {
                return Err(error(
                    "E-STATE-ORIGIN",
                    span,
                    "inferred error-union source shape differs",
                ));
            }
        }
        Ok(())
    }
    fn consume_once(&mut self, shape: &Shape, span: Span) -> R<()> {
        let Some(slot) = shape.slot() else {
            return Ok(());
        };
        self.state
            .snapshot(self.bounds.len().saturating_add(4), span)?;
        if self
            .bounds
            .iter()
            .find(|bound| bound.generic_index == slot)
            .is_none_or(|bound| bound.kind != CallableKind::FnOnce)
        {
            return Ok(());
        }
        let consumed = 1u8 << slot;
        let mut next = 0;
        for mask in 0u8..4 {
            if self.live & (1 << mask) == 0 {
                continue;
            }
            if mask & consumed != 0 {
                return Err(error(
                    "E-STATE-ONCE",
                    span,
                    "declared FnOnce State source was already consumed on a reachable path",
                ));
            }
            next |= 1 << (mask | consumed);
        }
        self.live = next;
        Ok(())
    }
    fn loop_body(&mut self, body: &CoreBlock, before: u8, depth: usize, span: Span) -> R<()> {
        self.state.snapshot(8, span)?;
        self.loops.push((0, 0));
        self.block(body, depth + 1)?;
        let (breaks, continues) = self.loops.pop().expect("State source loop");
        if (self.live | continues) & !before != 0 {
            return Err(error(
                "E-STATE-ONCE",
                span,
                "a repeated loop path cannot consume a declared FnOnce State source",
            ));
        }
        self.live = before | breaks;
        Ok(())
    }
    fn expr(&mut self, expression: &CoreExpr, depth: usize) -> R<Shape> {
        self.node(depth, expression.span)?;
        let span = expression.span;
        let shape = match &expression.kind {
            CoreExprKind::Name(name) => {
                let shape = self.lookup(name, span)?.unwrap_or(Shape::None);
                if self.capture && shape.has_origin() {
                    return Err(unsupported(
                        span,
                        "capturing a new State declaration source requires an explicit preserved capability contract",
                    ));
                }
                shape
            }
            CoreExprKind::Member { object, member } => {
                let base = self.expr(object, depth + 1)?;
                let base = if let Shape::Address(inner) = &base {
                    inner.as_ref()
                } else {
                    &base
                };
                if let Shape::Package(fields) = base {
                    self.state.snapshot(
                        fields.len().saturating_mul(member.len().saturating_add(1)),
                        span,
                    )?;
                    if let Some((_, field)) = fields.iter().find(|(name, _)| name == member) {
                        self.copy(field, span)?
                    } else {
                        Shape::None
                    }
                } else {
                    Shape::None
                }
            }
            CoreExprKind::Call { callee, arguments } => {
                let target = if let CoreExprKind::TypeApply { callee, .. } = &callee.kind {
                    callee.as_ref()
                } else {
                    callee.as_ref()
                };
                let target_shape = self.expr(target, depth + 1)?;
                self.consume_once(&target_shape, target.span)?;
                self.state.snapshot(arguments.len(), span)?;
                let mut shapes = Vec::new();
                for argument in arguments {
                    shapes.push(self.expr(argument, depth + 1)?);
                }
                if let Some(primitive) = dynamic_store::primitive(target) {
                    if let Some(Shape::Store(element)) = shapes.first() {
                        match primitive {
                            "store_at" | "store_at_mut" => {
                                Shape::Address(Box::new(self.copy(element, span)?))
                            }
                            "store_pop" => Shape::Union {
                                error: Box::new(Shape::None),
                                result: Box::new(self.copy(element, span)?),
                            },
                            "store_push" => Shape::Union {
                                error: Box::new(self.copy(element, span)?),
                                result: Box::new(Shape::None),
                            },
                            _ => Shape::None,
                        }
                    } else {
                        Shape::None
                    }
                } else if let Some(signature) = self.signature(target)? {
                    let mut substitutions = Vec::new();
                    if let CoreExprKind::TypeApply { type_arguments, .. } = &callee.kind {
                        let (Some(bound), [actual]) =
                            (&signature.type_parameter, type_arguments.as_slice())
                        else {
                            return Err(error(
                                "E-TYPE-APPLICATION",
                                span,
                                "ordinary State source forwarding needs one complete type argument",
                            ));
                        };
                        let shape = self.ty(actual, &[], true, depth + 1, span)?;
                        self.state
                            .snapshot(bound.name.len().saturating_add(1), span)?;
                        substitutions.push((bound.name.clone(), shape));
                    } else if let Some(bound) = &signature.type_parameter {
                        let mut inferred = None;
                        for (pattern, shape) in signature.parameters.iter().zip(&shapes) {
                            self.infer_value_shape(
                                pattern,
                                &bound.name,
                                shape,
                                &mut inferred,
                                span,
                            )?;
                        }
                        let source = inferred.unwrap_or(Shape::None);
                        self.state
                            .snapshot(bound.name.len().saturating_add(1), span)?;
                        substitutions.push((bound.name.clone(), source));
                    } else {
                        for bound in &signature.callable {
                            let argument = shapes.get(bound.index as usize).ok_or_else(|| {
                                error("E-CALL-ARITY", span, "State source argument count differs")
                            })?;
                            let source = if let Some(state) = &signature.state_parameter {
                                let binding = state
                                    .bindings
                                    .iter()
                                    .find(|b| b.generic_index == bound.generic_index)
                                    .ok_or_else(|| {
                                        unsupported(span, "State declaration selector is absent")
                                    })?;
                                argument.field(binding.fields[0]).ok_or_else(|| {
                                    unsupported(span, "State field declaration source was erased")
                                })?
                            } else {
                                argument
                            };
                            self.require(source, bound.kind, span)?;
                            let source = if let Shape::Address(inner) = source {
                                inner.as_ref()
                            } else {
                                source
                            };
                            let copied = self.copy(source, span)?;
                            self.state
                                .snapshot(bound.generic_name.len().saturating_add(1), span)?;
                            substitutions.push((bound.generic_name.clone(), copied));
                        }
                        if signature.callable.is_empty() && shapes.iter().any(Shape::has_origin) {
                            return Err(unsupported(
                                span,
                                "State declaration sources cannot cross an erased ordinary signature",
                            ));
                        }
                    }
                    let result =
                        self.ty(&signature.result, &substitutions, false, depth + 1, span)?;
                    if !result.has_origin() {
                        for (_, source) in &substitutions {
                            self.consume_once(source, span)?;
                        }
                    }
                    result
                } else {
                    if foreign::callback_constructor(target) && shapes.iter().any(Shape::has_origin)
                    {
                        return Err(unsupported(
                            span,
                            "State declaration sources cannot become function pointers",
                        ));
                    }
                    Shape::None
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => Shape::Address(Box::new(self.expr(operand, depth + 1)?)),
            CoreExprKind::Unary { operand, .. } | CoreExprKind::CompileTime(operand) => {
                self.expr(operand, depth + 1)?
            }
            CoreExprKind::Try(value) => match self.expr(value, depth + 1)? {
                Shape::Union { result, .. } => *result,
                shape if !shape.has_origin() => Shape::None,
                _ => {
                    return Err(unsupported(
                        span,
                        "State success source requires an explicit error-union shape",
                    ));
                }
            },
            CoreExprKind::Ascribed { ty, value } => {
                let actual = self.expr(value, depth + 1)?;
                let annotated = self.ty(ty, &[], true, depth + 1, span)?;
                self.same_origin(&actual, &annotated, span)?;
                annotated
            }
            CoreExprKind::Record { ty, fields } => {
                for (_, value) in fields {
                    self.expr(value, depth + 1)?;
                }
                self.ty(ty, &[], true, depth + 1, span)?
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                let base = self.expr(base, depth + 1)?;
                for (_, value) in fields {
                    self.expr(value, depth + 1)?;
                }
                if let Some(ty) = ty {
                    self.ty(ty, &[], true, depth + 1, span)?
                } else {
                    base
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.state.snapshot(1, span)?;
                self.scopes.push(BTreeMap::new());
                for (name, value) in bindings {
                    let shape = self.expr(value, depth + 1)?;
                    self.bind(name, shape, span)?;
                }
                let shape = self.expr(value, depth + 1)?;
                self.scopes.pop();
                shape
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1)?;
                self.state.snapshot(4, span)?;
                let before = self.live;
                let a = self.expr(then_value, depth + 1)?;
                let after_then = self.live;
                self.live = before;
                let b = self.expr(else_value, depth + 1)?;
                self.live |= after_then;
                self.same_origin(&a, &b, span)?;
                a
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                let previous = self.capture;
                self.capture = true;
                self.state.snapshot(1, span)?;
                self.scopes.push(BTreeMap::new());
                for parameter in parameters {
                    self.bind(&parameter.name, Shape::None, parameter.span)?;
                }
                self.block(body, depth + 1)?;
                self.scopes.pop();
                self.capture = previous;
                Shape::None
            }
            CoreExprKind::IntegerConvert { value, .. } => {
                self.expr(value, depth + 1)?;
                Shape::None
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                let a = self.expr(left, depth + 1)?;
                let b = self.expr(right, depth + 1)?;
                if a.has_origin() || b.has_origin() {
                    return Err(unsupported(
                        span,
                        "this operation does not preserve a State declaration source",
                    ));
                }
                Shape::None
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    if self.expr(value, depth + 1)?.has_origin() {
                        return Err(unsupported(
                            span,
                            "nested State declaration source containers require an explicit source contract",
                        ));
                    }
                }
                Shape::None
            }
            CoreExprKind::Slice { object, start, end } => {
                if self.expr(object, depth + 1)?.has_origin() {
                    return Err(unsupported(
                        span,
                        "State declaration source slicing is not implemented",
                    ));
                }
                for value in [start, end].into_iter().flatten() {
                    self.expr(value, depth + 1)?;
                }
                Shape::None
            }
            _ => Shape::None,
        };
        self.state.snapshot(
            shape
                .work()
                .saturating_add(helpers::plan::tree_work(self.expressions.len())),
            span,
        )?;
        if self.expressions.len() >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "State declaration use table exceeded",
                Some(span),
            ));
        }
        if let Some(previous) = self.expressions.get(&expression.id) {
            self.state.snapshot(previous.work(), span)?;
            if previous != &shape {
                return Err(error(
                    "E-STATE-ORIGIN",
                    span,
                    "one source node identifies conflicting State declaration origins",
                ));
            }
        } else {
            self.expressions.insert(expression.id, shape.clone());
        }
        Ok(shape)
    }
    fn same_origin(&mut self, a: &Shape, b: &Shape, span: Span) -> R<()> {
        self.state
            .snapshot(a.work().saturating_add(b.work()), span)?;
        if (a.has_origin() || b.has_origin()) && a != b {
            return Err(error(
                "E-STATE-ORIGIN",
                span,
                "State declaration source cannot be erased or changed by an annotation, assignment or branch",
            ));
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &CorePattern, shape: &Shape, depth: usize) -> R<()> {
        self.node(depth, pattern.span)?;
        if let CorePatternKind::Name(name) = &pattern.kind {
            let shape = self.copy(shape, pattern.span)?;
            self.bind(name, shape, pattern.span)?;
        } else if shape.has_origin() {
            return Err(unsupported(
                pattern.span,
                "this pattern does not preserve a State declaration source",
            ));
        }
        Ok(())
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.node(depth, block.span)?;
        self.state.snapshot(1, block.span)?;
        self.scopes.push(BTreeMap::new());
        for statement in &block.statements {
            self.node(depth + 1, statement.span)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name,
                    ty,
                    initializer,
                    ..
                } => {
                    let inferred = if let Some(value) = initializer {
                        self.expr(value, depth + 1)?
                    } else {
                        Shape::None
                    };
                    let shape = if let Some(ty) = ty {
                        let shape = self.ty(ty, &[], true, depth + 1, statement.span)?;
                        // store_new has no value arguments; its declared full
                        // element type is the source of the fresh Store shape.
                        if initializer.as_ref().is_none_or(|value| !matches!(&value.kind, CoreExprKind::Call { callee, .. } if dynamic_store::primitive(callee) == Some("store_new"))) {
                            self.same_origin(&inferred, &shape, statement.span)?;
                        }
                        shape
                    } else {
                        inferred
                    };
                    self.bind(name, shape, statement.span)?;
                }
                CoreStatementKind::Assign { target, value } => {
                    let a = self.expr(target, depth + 1)?;
                    let b = self.expr(value, depth + 1)?;
                    self.same_origin(&a, &b, statement.span)?;
                }
                CoreStatementKind::Expression(value) => {
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::Return(value) => {
                    if let Some(value) = value {
                        self.expr(value, depth + 1)?;
                    }
                    self.live = 0;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, depth + 1)?;
                    self.state.snapshot(4, statement.span)?;
                    let before = self.live;
                    self.block(then_body, depth + 1)?;
                    let after_then = self.live;
                    self.live = before;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                    self.live |= after_then;
                }
                CoreStatementKind::While { condition, body } => {
                    let before = self.live;
                    self.expr(condition, depth + 1)?;
                    let after_condition = self.live;
                    self.loop_body(body, before, depth, statement.span)?;
                    self.live |= after_condition;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let shape = self.expr(value, depth + 1)?;
                    self.state.snapshot(4, statement.span)?;
                    let before = self.live;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, &shape, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    self.scopes.pop();
                    let after_then = self.live;
                    self.live = before;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                    self.live |= after_then;
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let before = self.live;
                    let shape = self.expr(value, depth + 1)?;
                    let after_condition = self.live;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, &shape, depth + 1)?;
                    self.loop_body(body, before, depth, statement.span)?;
                    self.scopes.pop();
                    self.live |= after_condition;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                    ..
                } => {
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    let before = self.live;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.bind(name, Shape::None, statement.span)?;
                    self.loop_body(body, before, depth, statement.span)?;
                    self.scopes.pop();
                }
                CoreStatementKind::Match { value, arms } => {
                    let shape = self.expr(value, depth + 1)?;
                    self.state
                        .snapshot(arms.len().saturating_mul(4), statement.span)?;
                    let before = self.live;
                    let mut after = 0;
                    for arm in arms {
                        self.live = before;
                        self.state.snapshot(1, statement.span)?;
                        self.scopes.push(BTreeMap::new());
                        self.pattern(&arm.pattern, &shape, depth + 1)?;
                        self.block(&arm.body, depth + 1)?;
                        self.scopes.pop();
                        after |= self.live;
                    }
                    self.live = after;
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body, depth + 1)?
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {
                    self.state.snapshot(1, statement.span)?;
                    let Some((breaks, continues)) = self.loops.last_mut() else {
                        return Err(error(
                            "E-CONTROL",
                            statement.span,
                            "State source loop exit is outside a loop",
                        ));
                    };
                    if matches!(statement.kind, CoreStatementKind::Break) {
                        *breaks |= self.live;
                    } else {
                        *continues |= self.live;
                    }
                    self.live = 0;
                }
                _ => {}
            }
        }
        self.scopes.pop();
        Ok(())
    }
}
