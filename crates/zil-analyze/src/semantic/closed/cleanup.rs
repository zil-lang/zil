//! Exact original cleanup occurrences, mapped after all ordinary worlds/sites
//! have been checked. Original Gamma premises and actual frame proofs stay
//! separate: no concrete caller supplies an abstract SyncDrop predicate.
use super::*;

impl Check<'_, '_> {
    pub(super) fn cleanup_occurrences(&self, b: &mut Budget) -> CompileResult<()> {
        for (ordinal, pending) in self.original.pending_cleanups().iter().enumerate() {
            b.spend(1, WORK)?;
            let mut instances = 0usize;
            for instance in &self.program.instances {
                b.spend(1, WORK)?;
                if instance.definition != pending.definition { continue; }
                let SemanticInstanceTarget::Function(target) = &instance.target else {
                    return Err(fail("original cleanup occurrence has no complete actual function instance"));
                };
                let function = self.inventory.function(target, b)?;
                let expected = self.original.cleanup_value(ordinal, instance, self.program, self.inventory, b)?;
                if matches!(pending.event, original::CleanupEvent::CaptureExit)
                    && let original::CleanupSource::Local(original::LocalAnchor::Environment(ordinal)) = pending.source {
                    let capture = self.environment_capture(instance, function, ordinal, b)?;
                    let id = function.environment.ok_or_else(|| fail("original capture cleanup loses its actual entry"))?;
                    let closure = self.module.closures.get(id.0 as usize).filter(|definition| definition.id == id
                        && definition.function == function.id && definition.kind == CallableKind::FnOnce)
                        .ok_or_else(|| fail("original capture cleanup changes its consuming entry identity"))?;
                    let mut ty = closure.captures.get(capture as usize)
                        .ok_or_else(|| fail("original capture cleanup selects no actual captured field"))?;
                    for field in &pending.path {
                        b.spend(1, WORK)?;
                        ty = Self::pattern_child_type(ty, PlaceProjection::Field(*field))
                            .ok_or_else(|| fail("original capture cleanup loses its full selected child"))?;
                    }
                    if !geometry::same_type(ty, &expected.ty, b)? {
                        return Err(fail("original capture cleanup changes its complete selected subject"));
                    }
                    self.capture_directory(id, b)?;
                    b.spend(pending.path.len(), WORK)?;
                    b.lookup(b.semantic_capture_cleanups.len(), WORK)?;
                    b.semantic_capture_cleanups.insert((id, capture, pending.path.clone()));
                    instances += 1;
                    continue;
                }
                let local = match pending.source {
                    original::CleanupSource::Local(anchor) => {
                        let root = self.anchored_local_at(instance, function, anchor, b, 0)?;
                        self.cleanup_child_local(function, root, &pending.path, b)?
                    }
                    original::CleanupSource::Expression(node) => {
                        let value = self.original_subject_value(instance, function, node, b)?
                            .ok_or_else(|| fail("original discarded cleanup has no exact emitted value"))?;
                        if !pending.path.is_empty() {
                            return Err(unknown("original indirect cleanup needs its complete selected actual source"));
                        }
                        self.stored_pattern_value(function, value, b)?
                            .ok_or_else(|| fail("original discarded cleanup has no actual owning temporary"))?
                    }
                    original::CleanupSource::Unknown => return Err(fail("original cleanup has no source occurrence")),
                };
                let declaration = function.locals.get(local.0 as usize).filter(|declaration| declaration.id == local)
                    .ok_or_else(|| fail("original cleanup occurrence loses its exact actual local"))?;
                if !geometry::same_type(&expected.ty, &declaration.ty, b)? {
                    return Err(fail("original cleanup occurrence changes its complete instantiated subject type"));
                }
                // A Suspend declaration may dispose of its actual closed
                // environment through its frame. This does not establish
                // SyncDrop for the original formal or grant a Direct drop.
                let synchronous = function.call_kind == CallKind::Suspend
                    && self.structural_actual(&expected.ty, original::Predicate::SyncDrop, b, 0)?;
                let replacements = self.cleanup_replacement_values(instance, function,
                    pending.source, &pending.path, pending.event, b)?;
                let closure = match &declaration.ty {
                    Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) => None,
                    Type::Closure(id) => Some(*id),
                    _ => return Err(unknown("original cleanup occurrence needs its complete actual owning Task frame proof")),
                };
                let mut sites = 0usize;
                for block in &function.blocks {
                    for (position, instruction) in block.instructions.iter().enumerate() {
                        b.spend(1, WORK)?;
                        let matches = match closure {
                            None => matches!(instruction.kind, InstructionKind::DropManaged { local: selected } if selected == local),
                            Some(_) => matches!(instruction.kind, InstructionKind::DropClosure { local: selected } if selected == local),
                        };
                        if matches {
                            if !self.cleanup_event_matches(function, local, block.id, position,
                                pending.event, &replacements, b)? { continue; }
                            let actual = self.local_value(instance, function, local, (block.id, position), b, 0)?;
                            if actual != expected {
                                return Err(fail("original cleanup occurrence changes its complete actual child/class value"));
                            }
                            if !synchronous {
                                b.lookup(b.semantic_cleanup_sites.len(), WORK)?;
                                b.semantic_cleanup_sites.insert((function.id, block.id, position, local, closure));
                            }
                            sites += 1;
                        }
                    }
                }
                if sites == 0 {
                    return Err(fail("original cleanup occurrence has no exact actual cleanup site"));
                }
                if synchronous { crate::tasks::cleanup_frame_profile(self.module, function, b)?; }
                instances += 1;
            }
            if instances == 0 {
                return Err(fail("original cleanup occurrence has no complete actual instances"));
            }
        }
        Ok(())
    }

    fn cleanup_child_local(&self, function: &TypedFunction, root: LocalId,
        path: &[u32], b: &mut Budget) -> CompileResult<LocalId> {
        if path.is_empty() { return Ok(root); }
        let mut selected = None;
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(path.len().saturating_add(1), WORK)?;
                let exact = match &instruction.kind {
                    InstructionKind::TakePackageField { local, index } => *local == root && path == [*index],
                    InstructionKind::TakeTupleField { local, path: actual }
                    | InstructionKind::TakeVariantField { local, path: actual } => *local == root
                        && actual.iter().copied().eq(path.iter().copied().map(PlaceProjection::Field)),
                    _ => false,
                };
                if !exact { continue; }
                let value = instruction.result.ok_or_else(|| fail("original cleanup extraction has no actual SSA"))?;
                let local = self.stored_pattern_value(function, value, b)?
                    .ok_or_else(|| fail("original cleanup extraction has no actual owning temporary"))?;
                if selected.is_some_and(|old| old != local) {
                    return Err(unknown("original cleanup child has several distinct actual destinations"));
                }
                selected = Some(local);
            }
        }
        selected.ok_or_else(|| unknown("original cleanup child needs its complete actual selected drop operation"))
    }

    fn cleanup_replacement_values(&self, instance: &SemanticInstance, function: &TypedFunction,
        source: original::CleanupSource, path: &[u32],
        event: original::CleanupEvent, b: &mut Budget) -> CompileResult<BTreeSet<ValueId>> {
        use crate::semantic::original_index::Node;
        let node = match event {
            original::CleanupEvent::Replace(node) | original::CleanupEvent::Backedge(node) => node,
            _ => return Ok(BTreeSet::new()),
        };
        let index = self.inventory.original_index(instance.definition, b)?;
        let Node::Statement(statement) = index.node(node, b)? else {
            return Err(fail("original cleanup replacement/loop loses its exact statement"));
        };
        let mut pending = vec![statement];
        let mut result = BTreeSet::new();
        while let Some(statement) = pending.pop() {
            b.spend(1, WORK)?;
            match &statement.kind {
                CoreStatementKind::Assign { .. } => {
                    let Some(write) = self.original.cleanup_write(instance.definition, statement.id, b)? else { continue; };
                    b.spend(path.len().saturating_add(write.path.len() + 1), WORK)?;
                    if !matches!(source, original::CleanupSource::Local(anchor) if anchor == write.anchor)
                        || !path.starts_with(&write.path) { continue; }
                    if let Some(value) = self.original_subject_value(instance, function, write.value, b)? {
                        b.lookup(result.len(), WORK)?;
                        result.insert(value);
                    } else {
                        return Err(unknown("original cleanup replacement needs its complete actual RHS emission"));
                    }
                }
                CoreStatementKind::If { then_body, else_body, .. }
                | CoreStatementKind::IfLet { then_body, else_body, .. } => {
                    for body in std::iter::once(then_body).chain(else_body.iter()) {
                        for statement in &body.statements { self.push_value(&mut pending, statement, b)?; }
                    }
                }
                CoreStatementKind::Loop(body) | CoreStatementKind::Block(body)
                | CoreStatementKind::Unsafe(body) | CoreStatementKind::While { body, .. }
                | CoreStatementKind::WhileLet { body, .. } | CoreStatementKind::ForRange { body, .. }
                | CoreStatementKind::ForEach { body, .. } | CoreStatementKind::With { body, .. } => {
                    for statement in &body.statements { self.push_value(&mut pending, statement, b)?; }
                }
                CoreStatementKind::Match { arms, .. } => {
                    for arm in arms { for statement in &arm.body.statements {
                        self.push_value(&mut pending, statement, b)?;
                    }}
                }
                _ => {}
            }
        }
        if result.is_empty() {
            return Err(unknown("original cleanup replacement/loop has no exact actual write values"));
        }
        Ok(result)
    }

    fn cleanup_event_matches(&self, function: &TypedFunction, local: LocalId,
        block: BlockId, position: usize, event: original::CleanupEvent,
        replacements: &BTreeSet<ValueId>, b: &mut Budget) -> CompileResult<bool> {
        match event {
            original::CleanupEvent::Scope | original::CleanupEvent::Discard(_) =>
                self.cleanup_scope_boundary(function, local, block, position + 1, b),
            original::CleanupEvent::Replace(_) | original::CleanupEvent::Backedge(_) => {
                let body = function.blocks.get(block.0 as usize).filter(|body| body.id == block)
                    .ok_or_else(|| fail("original cleanup replacement has no actual block"))?;
                for (offset, instruction) in body.instructions.iter().enumerate().skip(position + 1) {
                    b.spend(1, WORK)?;
                    let stored = match instruction.kind {
                        InstructionKind::Store { local: target, value } if target == local => Some(value),
                        InstructionKind::StoreOwnedArrayElement { local: target, index: 0, value } if target == local => Some(value),
                        InstructionKind::StorageLive(target) | InstructionKind::StorageDead(target) if target == local => return Ok(false),
                        InstructionKind::DropManaged { local: target } | InstructionKind::DropClosure { local: target }
                            if target == local => return Ok(false),
                        _ => None,
                    };
                    if let Some(value) = stored {
                        b.lookup(replacements.len(), WORK)?;
                        if !replacements.contains(&value) { return Ok(false); }
                        operations::available_before(function, (block, position, SemanticSiteEdge::Before), value, b)?;
                        // A backedge obligation is tied to a real cyclic write,
                        // not another same-local replacement elsewhere.
                        if matches!(event, original::CleanupEvent::Backedge(_))
                            && !self.cleanup_cyclic_write(function, block, offset, b)? { return Ok(false); }
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            original::CleanupEvent::CaptureExit => Err(fail("remaining capture cleanup lost its exact environment source")),
        }
    }

    fn cleanup_scope_boundary(&self, function: &TypedFunction, local: LocalId,
        block: BlockId, start: usize, b: &mut Budget) -> CompileResult<bool> {
        let root = (block, start);
        let mut pending = vec![root];
        let mut seen = BTreeSet::new();
        let mut graph = BTreeMap::new();
        let mut settled = BTreeSet::new();
        while let Some((block, start)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((block, start)) { continue; }
            let body = function.blocks.get(block.0 as usize).filter(|body| body.id == block)
                .ok_or_else(|| fail("original cleanup exit has no actual CFG block"))?;
            let mut ended = false;
            for instruction in body.instructions.iter().skip(start) {
                b.spend(1, WORK)?;
                match instruction.kind {
                    InstructionKind::StorageDead(target) if target == local => { ended = true; break; }
                    InstructionKind::StorageLive(target) | InstructionKind::Store { local: target, .. }
                    | InstructionKind::StoreOwnedArrayElement { local: target, .. } if target == local => return Ok(false),
                    _ => {}
                }
            }
            if ended || matches!(body.terminator, Terminator::Return(_)) {
                b.lookup(settled.len(), WORK)?;
                settled.insert((block, start));
                continue;
            }
            let successors = match body.terminator {
                Terminator::Jump(next) => [Some((next, 0)), None],
                Terminator::Branch { then_block, else_block, .. } => [Some((then_block, 0)), Some((else_block, 0))],
                _ => return Ok(false),
            };
            let mut edges = Vec::new();
            for next in successors.into_iter().flatten() {
                self.push_value(&mut edges, next, b)?;
                self.push_value(&mut pending, next, b)?;
            }
            b.lookup(graph.len(), WORK)?;
            graph.insert((block, start), edges);
        }
        // Shared successors are ordinary joins, not cycles. Only paths which
        // reach an actual lifetime boundary on every successor discharge Scope.
        loop {
            let mut progress = false;
            for (node, edges) in &graph {
                b.lookup(settled.len(), WORK)?;
                if settled.contains(node) { continue; }
                let mut complete = true;
                for next in edges {
                    b.lookup(settled.len(), WORK)?;
                    complete &= settled.contains(next);
                }
                if complete {
                    b.lookup(settled.len(), WORK)?;
                    settled.insert(*node);
                    progress = true;
                }
            }
            b.lookup(settled.len(), WORK)?;
            if settled.contains(&root) { return Ok(true); }
            if !progress { return Ok(false); }
        }
    }

    fn cleanup_cyclic_write(&self, function: &TypedFunction, block: BlockId,
        _position: usize, b: &mut Budget) -> CompileResult<bool> {
        let mut pending = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = Some(block);
        while let Some(next) = current.take().or_else(|| pending.pop()) {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert(next) { continue; }
            let body = function.blocks.get(next.0 as usize).filter(|body| body.id == next)
                .ok_or_else(|| fail("original cleanup backedge has no actual CFG block"))?;
            let targets = match body.terminator {
                Terminator::Jump(next) => [Some(next), None],
                Terminator::Branch { then_block, else_block, .. } => [Some(then_block), Some(else_block)],
                _ => [None, None],
            };
            for target in targets.into_iter().flatten() {
                b.spend(1, WORK)?;
                if target == block { return Ok(true); }
                pending.push(target);
            }
        }
        Ok(false)
    }
}
