//! Resolve a Range binding to the immutable body snapshot, independently of the
//! induction counter's name, LocalId and equal-typed neighboring inputs.
use super::*;
use zil_base::{Diagnostic, NodeId, Span};

#[derive(Clone, Copy)]
struct RangeGuard {
    head: BlockId,
    body: BlockId,
    after: BlockId,
    current: ValueId,
    limit: ValueId,
    counter: LocalId,
    snapshot: LocalId,
}
struct RangeEntries {
    pre: BlockId,
    initial_at: usize,
    initial: ValueId,
    latch: Option<BlockId>,
    observations: Vec<BlockId>,
}
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn range_binding_local(
        &self,
        world: &SemanticInstance,
        function: &TypedFunction,
        statement: NodeId,
        start: NodeId,
        end: NodeId,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<LocalId> {
        let key = (
            world.id,
            function.id,
            statement,
            b.partial_depth,
            b.helper_nodes,
            depth,
        );
        b.lookup(self.inventory.instances.len(), WORK)?;
        b.lookup(self.inventory.functions.len(), WORK)?;
        let held = self
            .inventory
            .instances
            .get(&world.id)
            .is_some_and(|known| std::ptr::eq(*known, world))
            && self.inventory.functions.get(&function.id)
                .is_some_and(|known| std::ptr::eq(*known, function));
        if held {
            b.lookup(self.range_locals.borrow().len(), WORK)?;
            if let Some(&local) = self.range_locals.borrow().get(&key) {
                return Ok(local);
            }
        }
        let original = self.inventory.original_index(world.definition, b)?;
        let span = original.get_stmt(statement, b)?.span;
        let invalid = |message: &str| {
            let mut errors = fail(message);
            errors[0].span = Some(span);
            errors[0].notes.push(format!("Range {:?} in {:?} requires endpoints {:?} and {:?} and a distinct guarded body snapshot in {:?}.",
                statement, world.definition, start, end, function.id));
            errors
        };
        let flow = crate::shared_initialization(function, b)?;
        let guard = self.range_guard(function, span, &invalid, b)?;
        let entries = self.range_entries(function, &guard, span, &invalid, b)?;
        let RangeEntries {
            pre,
            initial_at: index,
            initial,
            ..
        } = entries;
        let RangeGuard {
            limit, snapshot, ..
        } = guard;
        for value in [initial, limit] {
            operations::available_before(
                function,
                (pre, index, SemanticSiteEdge::Before),
                value,
                b,
            )?;
        }
        if !self.copy_expression_source(
            world,
            function,
            original.get_expr(start, b)?,
            initial,
            b,
            depth + 1,
        )? || !self.copy_expression_source(
            world,
            function,
            original.get_expr(end, b)?,
            limit,
            b,
            depth + 1,
        )? {
            return Err(invalid(
                "actual Range selects another original start or end expression",
            ));
        }
        self.check_range_edges(function, &flow, &guard, &entries, &invalid, b)?;
        if held {
            b.lookup(self.range_locals.borrow().len(), WORK)?;
            self.range_locals.borrow_mut().insert(key, snapshot);
        }
        Ok(snapshot)
    }
    fn range_guard(
        &self,
        function: &TypedFunction,
        span: Span,
        invalid: &impl Fn(&str) -> Vec<Diagnostic>,
        b: &mut Budget,
    ) -> CompileResult<RangeGuard> {
        let mut guard = None;
        for block in &function.blocks {
            b.spend(1, WORK)?;
            let Terminator::Branch {
                condition,
                then_block,
                else_block,
            } = block.terminator
            else {
                continue;
            };
            let (at, _, comparison) = self.defining(function, condition, b)?;
            if comparison.span != span {
                continue;
            }
            let InstructionKind::Binary {
                op: BinaryOp::Less,
                left,
                right,
            } = comparison.kind
            else {
                continue;
            };
            if at != block.id
                || then_block == else_block
                || [then_block, else_block].contains(&at)
                || guard
                    .replace((block.id, then_block, else_block, left, right))
                    .is_some()
            {
                return Err(invalid(
                    "actual Range has no unique guarded body/exit for its source occurrence",
                ));
            }
        }
        let (head, body, after, current, limit) = guard.ok_or_else(|| {
            invalid("actual Range lacks its original strict upper-bound comparison")
        })?;
        let (at, _, load) = self.defining(function, current, b)?;
        let InstructionKind::Load(counter) = load.kind else {
            return Err(invalid(
                "actual Range guard does not observe its induction counter",
            ));
        };
        let body_block = function
            .blocks
            .get(body.0 as usize)
            .filter(|block| block.id == body)
            .ok_or_else(|| invalid("actual Range body is absent"))?;
        let mut statements = body_block.instructions.iter();
        let snapshot = loop {
            b.spend(1, WORK)?;
            match statements.next() {
                Some(Instruction {
                    kind: InstructionKind::ScopeEnter { .. },
                    ..
                }) => continue,
                Some(Instruction {
                    kind: InstructionKind::Store { local, value },
                    span: source,
                    ..
                }) if *value == current && *source == span => break *local,
                _ => {
                    return Err(invalid(
                        "actual Range body entry does not bind its guarded counter value",
                    ));
                }
            }
        };
        let decl = function
            .locals
            .get(snapshot.0 as usize)
            .filter(|decl| decl.id == snapshot)
            .ok_or_else(|| invalid("actual Range snapshot storage is absent"))?;
        let accumulator = function
            .locals
            .get(counter.0 as usize)
            .filter(|decl| decl.id == counter)
            .ok_or_else(|| invalid("actual Range counter storage is absent"))?;
        if head != at
            || snapshot == counter
            || decl.mutable
            || !decl.ty.is_integer()
            || decl.ty != accumulator.ty
            || decl.span != span
            || accumulator.span != span
        {
            return Err(invalid(
                "actual Range changes the distinct counter or immutable integer snapshot storage",
            ));
        }
        Ok(RangeGuard {
            head,
            body,
            after,
            current,
            limit,
            counter,
            snapshot,
        })
    }

    fn range_entries(
        &self,
        function: &TypedFunction,
        guard: &RangeGuard,
        span: Span,
        invalid: &impl Fn(&str) -> Vec<Diagnostic>,
        b: &mut Budget,
    ) -> CompileResult<RangeEntries> {
        let RangeGuard {
            head,
            body,
            current,
            counter,
            snapshot,
            ..
        } = *guard;
        let decl = &function.locals[snapshot.0 as usize];
        let mut activation = [None; 2];
        let mut writes = Vec::new();
        let mut observations = Vec::new();
        let mut bindings = 0;
        for block in &function.blocks {
            for (at, ins) in block.instructions.iter().enumerate() {
                b.spend(1, WORK)?;
                for local in ins.kind.local_operands() {
                    b.spend(1, WORK)?;
                    if local != counter && local != snapshot {
                        continue;
                    }
                    match &ins.kind {
                        InstructionKind::StorageLive(_) if ins.span == span => {
                            if activation[usize::from(local == snapshot)]
                                .replace((block.id, at))
                                .is_some()
                            {
                                return Err(invalid(
                                    "actual Range storage has several activation sites",
                                ));
                            }
                        }
                        InstructionKind::Store { value, .. }
                            if local == counter && ins.span == span && writes.len() < 2 =>
                        {
                            writes.push((block.id, at, *value))
                        }
                        InstructionKind::Store { value, .. }
                            if local == snapshot && block.id == body && *value == current =>
                        {
                            bindings += 1
                        }
                        InstructionKind::Load(_) if local == snapshot => {
                            self.push_value(&mut observations, block.id, b)?
                        }
                        InstructionKind::BorrowLocal { .. }
                        | InstructionKind::BorrowPlaceLocal { mutable: false, .. }
                            if local == snapshot =>
                        {
                            self.push_value(&mut observations, block.id, b)?
                        }
                        InstructionKind::Load(_)
                            if local == counter && ins.result == Some(current) => {}
                        InstructionKind::StorageDead(_) => {}
                        _ => {
                            return Err(invalid(
                                "actual Range assigns or exposes one of its induction slots outside its declared role",
                            ));
                        }
                    }
                }
            }
        }
        let [
            Some((pre, counter_live)),
            Some((snapshot_pre, snapshot_live)),
        ] = activation
        else {
            return Err(invalid("actual Range lacks both storage activations"));
        };
        if pre != snapshot_pre
            || pre == head
            || bindings != 1
            || !matches!(function.blocks[pre.0 as usize].terminator, Terminator::Jump(to) if to == head)
        {
            return Err(invalid(
                "actual Range initialization and guarded snapshot do not have their original entry edges",
            ));
        }
        let mut initial = None;
        let mut latch = None;
        for (block, index, value) in writes {
            if block == pre {
                if index <= counter_live.max(snapshot_live)
                    || initial.replace((index, value)).is_some()
                {
                    return Err(invalid(
                        "actual Range reinitializes its counter in the same source preheader",
                    ));
                }
                continue;
            }
            let (add_block, add_at, step) = self.defining(function, value, b)?;
            let InstructionKind::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } = step.kind
            else {
                return Err(invalid(
                    "actual Range backedge omits its successor operation",
                ));
            };
            let (one_block, one_at, unit) = self.defining(function, right, b)?;
            let one = original::mixed::integer_constant("1", &decl.ty, b)?;
            if left != current
                || block != add_block
                || add_block != one_block
                || one_at >= add_at
                || add_at >= index
                || !matches!(&unit.kind, InstructionKind::Constant(constant) if *constant == one)
                || !matches!(function.blocks[block.0 as usize].terminator, Terminator::Jump(to) if to == head)
                || latch.replace(block).is_some()
            {
                return Err(invalid(
                    "actual Range backedge changes the current value, unit increment, or return to the guard",
                ));
            }
        }
        let (index, initial) =
            initial.ok_or_else(|| invalid("actual Range lacks its initial counter value"))?;
        Ok(RangeEntries {
            pre,
            initial_at: index,
            initial,
            latch,
            observations,
        })
    }

    fn check_range_edges(
        &self,
        function: &TypedFunction,
        flow: &crate::Initialization,
        guard: &RangeGuard,
        entries: &RangeEntries,
        invalid: &impl Fn(&str) -> Vec<Diagnostic>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let RangeGuard {
            head, body, after, ..
        } = *guard;
        let RangeEntries {
            pre,
            latch,
            ref observations,
            ..
        } = *entries;
        let pred = &flow.predecessors;
        b.spend(
            pred[head.0 as usize]
                .len()
                .saturating_add(pred[body.0 as usize].len()),
            WORK,
        )?;
        if pred[body.0 as usize].as_slice() != [head.0 as usize]
            || pred[head.0 as usize]
                .iter()
                .any(|&p| p != pre.0 as usize && Some(BlockId(p as u32)) != latch)
        {
            return Err(invalid(
                "actual Range enters its body or guard without the required counter definition",
            ));
        }
        b.spend(function.blocks.len(), WORK)?;
        let mut inside = vec![false; function.blocks.len()];
        let mut pending = vec![body];
        while let Some(next) = pending.pop() {
            b.spend(1, WORK)?;
            if next == after || Some(next) == latch {
                continue;
            }
            if next == head || next == pre {
                return Err(invalid(
                    "actual Range body bypasses its increment or reenters its own initialization",
                ));
            }
            if std::mem::replace(&mut inside[next.0 as usize], true) {
                continue;
            }
            match function.blocks[next.0 as usize].terminator {
                Terminator::Jump(to) => self.push_value(&mut pending, to, b)?,
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.push_value(&mut pending, then_block, b)?;
                    self.push_value(&mut pending, else_block, b)?;
                }
                Terminator::Return(_) | Terminator::Trap | Terminator::Unreachable => {}
            }
        }
        for (block, parents) in pred.iter().enumerate() {
            b.spend(1, WORK)?;
            if !inside[block] && Some(BlockId(block as u32)) != latch {
                continue;
            }
            b.spend(parents.len(), WORK)?;
            if parents
                .iter()
                .any(|&p| !inside[p] && !(p == head.0 as usize && block == body.0 as usize))
            {
                return Err(invalid(
                    "actual Range body admits an edge from outside the current iteration",
                ));
            }
        }
        b.spend(observations.len(), WORK)?;
        if observations.iter().any(|block| !inside[block.0 as usize]) {
            return Err(invalid(
                "actual Range snapshot is read outside its guarded iteration",
            ));
        }
        Ok(())
    }
}
