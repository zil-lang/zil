//! Refill owned Sum captures in actual successful-initialization order.
//! This is private lowering order only; Analyze and Verify prove responsibility.
use super::*;

#[derive(Clone, Default, PartialEq, Eq)]
struct CaptureAge {
    definite: std::collections::BTreeSet<LocalId>,
    possible: std::collections::BTreeSet<LocalId>,
    before: std::collections::BTreeSet<(LocalId, LocalId)>,
}

impl Builder<'_, '_> {
    fn capture_cleanup_age(&mut self, span: Span) -> R<Vec<LocalId>> {
        let target = self.current.expect("live capture exit").0 as usize;
        let blocks = self.function.blocks.len();
        self.closures.snapshot(blocks.saturating_mul(5), span)?;
        let mut incoming = vec![Vec::new(); blocks];
        for (index, block) in self.function.blocks.iter().enumerate() {
            let successors = match block.terminator {
                Terminator::Jump(next) => vec![next],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block, else_block],
                _ => Vec::new(),
            };
            for next in successors {
                self.closures.snapshot(1, span)?;
                incoming[next.0 as usize].push(index);
            }
        }
        let mut can_reach = vec![false; blocks];
        let mut pending = std::collections::VecDeque::from([target]);
        can_reach[target] = true;
        while let Some(block) = pending.pop_front() {
            self.closures
                .snapshot(incoming[block].len().saturating_add(1), span)?;
            for &previous in &incoming[block] {
                if !can_reach[previous] {
                    can_reach[previous] = true;
                    pending.push_back(previous);
                }
            }
        }
        if !can_reach[self.function.entry.0 as usize] {
            return Err(unsupported(span, "owning Sum capture exit is unreachable"));
        }
        self.closures.snapshot(self.function.locals.len(), span)?;
        let mut owning = Vec::with_capacity(self.function.locals.len());
        for local in &self.function.locals {
            self.closures.snapshot_type(&local.ty, 1, 0, span)?;
            owning.push(
                !matches!(local.ty, Type::Address { .. } | Type::Desc(_))
                    && (matches!(local.ty, Type::Managed(_)) || self.affine_type(&local.ty)),
            );
        }
        let mut states: Vec<Option<CaptureAge>> = vec![None; blocks];
        states[self.function.entry.0 as usize] = Some(CaptureAge::default());
        let mut work = std::collections::VecDeque::from([self.function.entry.0 as usize]);
        let mut final_age = None;
        while let Some(block) = work.pop_front() {
            let source = states[block].as_ref().expect("queued CFG state");
            self.closures.snapshot(
                source.definite.len() + source.possible.len() + source.before.len(),
                span,
            )?;
            let mut age = source.clone();
            let instructions = &self.function.blocks[block].instructions;
            self.closures
                .snapshot(instructions.len().saturating_add(1), span)?;
            for instruction in instructions {
                let add = match &instruction.kind {
                    InstructionKind::Store { local, .. } if owning[local.0 as usize] => {
                        Some(*local)
                    }
                    _ => None,
                };
                let remove = match &instruction.kind {
                    InstructionKind::TakeLocal { local }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropTuple { local }
                    | InstructionKind::DropEnum { local }
                    | InstructionKind::DropErrorUnion { local }
                    | InstructionKind::DropOwnedArray { local }
                    | InstructionKind::DropClosure { local }
                    | InstructionKind::StorageDead(local) => Some(*local),
                    _ => add,
                };
                if let Some(local) = remove {
                    self.closures
                        .snapshot(age.before.len().saturating_add(2), span)?;
                    age.definite.remove(&local);
                    age.possible.remove(&local);
                    age.before.retain(|(a, b)| *a != local && *b != local);
                }
                if let Some(local) = add {
                    self.closures
                        .snapshot(age.possible.len().saturating_add(2), span)?;
                    for previous in &age.possible {
                        age.before.insert((*previous, local));
                    }
                    age.definite.insert(local);
                    age.possible.insert(local);
                }
            }
            if block == target {
                final_age = Some(age);
                continue;
            }
            let successors = match self.function.blocks[block].terminator {
                Terminator::Jump(next) => vec![next],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block, else_block],
                _ => Vec::new(),
            };
            for next in successors {
                let next = next.0 as usize;
                if !can_reach[next] {
                    continue;
                }
                self.closures.snapshot(
                    age.definite.len() + age.possible.len() + age.before.len() + 1,
                    span,
                )?;
                match &states[next] {
                    None => {
                        self.closures.snapshot(
                            age.definite.len() + age.possible.len() + age.before.len(),
                            span,
                        )?;
                        states[next] = Some(age.clone());
                        work.push_back(next);
                    }
                    Some(previous) => {
                        self.closures.snapshot(
                            previous.definite.len()
                                + previous.possible.len()
                                + previous.before.len()
                                + age.definite.len()
                                + age.possible.len()
                                + age.before.len(),
                            span,
                        )?;
                        let mut joined = previous.clone();
                        joined.definite.retain(|local| age.definite.contains(local));
                        joined.possible.extend(&age.possible);
                        joined.before.extend(&age.before);
                        if &joined != previous {
                            states[next] = Some(joined);
                            self.closures.snapshot(1, span)?;
                            work.push_back(next);
                        }
                    }
                }
            }
        }
        let final_age = final_age
            .ok_or_else(|| unsupported(span, "owning Sum capture exit has no actual path"))?;
        self.closures
            .snapshot(self.owned_sum_captures.len(), span)?;
        for local in self.owned_sum_captures.keys() {
            self.closures.snapshot(final_age.definite.len(), span)?;
            if !final_age.definite.contains(local) {
                return Err(error(
                    "E-UNINITIALIZED",
                    span,
                    "owning Sum capture is not refilled on its returning path",
                ));
            }
        }
        let mut remaining = final_age.possible;
        let mut before = final_age.before;
        let mut ordered = Vec::new();
        while !remaining.is_empty() {
            self.closures.snapshot(
                remaining
                    .len()
                    .saturating_mul(before.len().saturating_add(1))
                    .saturating_add(before.len())
                    .saturating_add(1),
                span,
            )?;
            let next = remaining
                .iter()
                .copied()
                .find(|candidate| !before.iter().any(|(_, later)| later == candidate))
                .ok_or_else(|| {
                    unsupported(
                        span,
                        "owning Sum capture exit needs path-specific cleanup after differing initialization orders",
                    )
                })?;
            ordered.push(next);
            remaining.remove(&next);
            before.retain(|(earlier, _)| *earlier != next);
        }
        Ok(ordered)
    }

    pub(super) fn end_owned_sum_capture_scopes(
        &mut self,
        depth: usize,
        kind: CleanupExitKind,
        span: Span,
    ) -> R<bool> {
        if self.owned_sum_captures.is_empty()
            || depth != 0
            || !matches!(kind, CleanupExitKind::Return | CleanupExitKind::Error)
        {
            return Ok(false);
        }
        let mut scope_of = BTreeMap::new();
        let mut ordinary = Vec::new();
        let mut owners = Vec::new();
        for index in (depth..self.scopes.len()).rev() {
            let mut locals = self.scopes[index].values().copied().collect::<Vec<_>>();
            self.closures
                .snapshot(locals.len().saturating_add(1), span)?;
            locals.sort_unstable_by(|a, b| b.cmp(a));
            for local in locals {
                scope_of.insert(local, index);
                let ty = &self.function.locals[local.0 as usize].ty;
                if !matches!(ty, Type::Address { .. } | Type::Desc(_))
                    && (matches!(ty, Type::Managed(_)) || self.affine_type(ty))
                {
                    owners.push(local);
                } else {
                    ordinary.push(local);
                }
            }
        }
        if self
            .defer_locals
            .iter()
            .any(|local| scope_of.contains_key(local))
        {
            // Defer effects retain their existing lexical order.
            return Ok(false);
        }
        let age = self.capture_cleanup_age(span)?;
        let mut order = ordinary;
        self.closures
            .snapshot(owners.len().saturating_add(order.len()), span)?;
        let owner_set: std::collections::BTreeSet<_> = owners.iter().copied().collect();
        let mut ordered: std::collections::BTreeSet<_> = order.iter().copied().collect();
        for local in age.into_iter().rev() {
            if scope_of.contains_key(&local) && owner_set.contains(&local) && ordered.insert(local)
            {
                self.closures
                    .snapshot(order.len().saturating_add(1), span)?;
                order.push(local);
            }
        }
        for local in owners {
            if ordered.insert(local) {
                self.closures
                    .snapshot(order.len().saturating_add(1), span)?;
                order.push(local);
            }
        }
        self.closures.snapshot(self.scopes.len(), span)?;
        let mut remaining = vec![0usize; self.scopes.len()];
        for &index in scope_of.values() {
            remaining[index] += 1;
        }
        let mut next_exit = self.scopes.len();
        for local in order {
            let index = scope_of[&local];
            if let Some(capture) = self.package_captures.get(&local).copied() {
                self.emit(
                    InstructionKind::EndPackageCapture {
                        local,
                        index: capture,
                    },
                    None,
                    span,
                )?;
            } else if let Some(capture) = self.owned_sum_captures.get(&local).copied() {
                let ty = self.function.locals[local.0 as usize].ty.clone();
                let value = self.value(InstructionKind::TakeLocal { local }, ty, span)?;
                self.emit(
                    InstructionKind::WriteCapture {
                        index: capture,
                        value,
                    },
                    None,
                    span,
                )?;
            } else if !self.temporary_locals.contains(&local)
                || self.resource_temps.contains(&local)
            {
                self.drop_owned_local(local, span)?;
            }
            if let Some(capture) = self.address_written_captures.get(&local).copied() {
                let ty = self.function.locals[local.0 as usize].ty.clone();
                let value = self.value(InstructionKind::Load(local), ty, span)?;
                self.emit(
                    InstructionKind::WriteCapture {
                        index: capture,
                        value,
                    },
                    None,
                    span,
                )?;
            }
            self.emit(InstructionKind::StorageDead(local), None, span)?;
            remaining[index] -= 1;
            while next_exit > depth && remaining[next_exit - 1] == 0 {
                next_exit -= 1;
                self.emit(
                    InstructionKind::ScopeExit {
                        scope: self.scope_ids[next_exit],
                        kind,
                    },
                    None,
                    span,
                )?;
            }
        }
        while next_exit > depth {
            next_exit -= 1;
            self.emit(
                InstructionKind::ScopeExit {
                    scope: self.scope_ids[next_exit],
                    kind,
                },
                None,
                span,
            )?;
        }
        Ok(true)
    }
}
