//! Exact original cleanup occurrences, mapped after all ordinary worlds/sites
//! have been checked. Original Gamma premises and actual frame proofs stay
//! separate: no concrete caller supplies an abstract SyncDrop predicate.
use super::*;
type ReplacementSource = (ValueId, Vec<PlaceProjection>);

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
                if let original::CleanupEvent::CaptureExit(operation) = pending.event
                    && let original::CleanupSource::Local(original::LocalAnchor::Environment(ordinal)) = pending.source {
                    let capture = self.environment_capture(instance, function, ordinal, b)?;
                    let id = function.environment.ok_or_else(|| fail("original capture cleanup loses its actual entry"))?;
                    let closure = self.module.closures.get(id.0 as usize).filter(|definition| definition.id == id
                        && definition.function == function.id && matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce))
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
                    b.semantic_capture_cleanups.insert((id, capture, pending.path.clone(), operation));
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
                let producer = if let original::CleanupSource::Expression(node) = pending.source {
                    self.original_subject_value(instance, function, node, b)?
                } else { None };
                let declaration = function.locals.get(local.0 as usize).filter(|declaration| declaration.id == local)
                    .ok_or_else(|| fail("original cleanup occurrence loses its exact actual local"))?;
                let slot = self.persistent_slot(instance, function, local, b)?;
                if let Some(slot) = &slot {
                    if !slot.logical().equal_paid(&expected, b)? {
                        return Err(fail("original slot cleanup changes its complete logical subject"));
                    }
                } else if !geometry::same_type(&expected.ty, &declaration.ty, b)? {
                    return Err(fail("original cleanup occurrence changes its complete instantiated subject type"));
                }
                // A Suspend declaration may dispose of its actual closed
                // environment through its frame. This does not establish
                // SyncDrop for the original formal or grant a Direct drop.
                let synchronous = function.call_kind == CallKind::Suspend
                    && self.structural_actual(&expected.ty, original::Predicate::SyncDrop, b, 0)?;
                let replacements = self.cleanup_replacement_values(instance, function,
                    pending.source, &pending.path, pending.event, b)?;
                let closure = match &expected.ty {
                    Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) => None,
                    Type::Closure(id) => Some(*id),
                    _ => return Err(unknown("original cleanup occurrence needs its complete actual owning Task frame proof")),
                };
                let mut sites = 0usize;
                for block in &function.blocks {
                    for (position, instruction) in block.instructions.iter().enumerate() {
                        b.spend(1, WORK)?;
                        let matches = if slot.is_some() {
                            matches!(instruction.kind,
                                InstructionKind::DropOwnedArrayElement { local: selected, index: 0 }
                                | InstructionKind::DropOwnedArray { local: selected } if selected == local)
                        } else { match closure {
                            None => matches!(instruction.kind, InstructionKind::DropManaged { local: selected } if selected == local),
                            Some(_) => matches!(instruction.kind, InstructionKind::DropClosure { local: selected } if selected == local),
                        }};
                        if matches {
                            if !self.cleanup_event_matches(function, local, block.id, position,
                                pending.event, &replacements, b)? { continue; }
                            if let Some(slot) = &slot {
                                // A conditional element drop and the final shell
                                // drop can be vacant. Prove every live source;
                                // the converged resource ledger decides debts.
                                self.cleanup_slot_subject(instance, function, slot,
                                    (block.id, position), &expected, b)?;
                            } else {
                                self.cleanup_local_subject(instance, function, local,
                                    (block.id, position), &expected, producer, b)?;
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

    // Reconstruct the destination of an indirect write from actual SSA.
    // Distinct stack locals are disjoint, and an incoming address predates
    // the current activation. Neither proof relies on a candidate Hold row.
    fn cleanup_write_disjoint(&self, function: &TypedFunction, owner: LocalId,
        mut at: (BlockId, usize), mut address: ValueId, b: &mut Budget) -> CompileResult<bool> {
        let mut seen = BTreeSet::new();
        loop {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert(address) { return Ok(false); }
            operations::available_before(function, (at.0, at.1, SemanticSiteEdge::Before), address, b)?;
            let (block, index, source) = self.defining(function, address, b)?;
            let Some(ty @ Type::Address { mutable: true, inner }) = function.values.get(address.0 as usize) else {
                return Ok(false);
            };
            at = (block, index);
            match source.kind {
                InstructionKind::Load(local) => {
                    operations::initialized_local_before(function, at, local, b)?;
                    let declaration = function.locals.get(local.0 as usize).filter(|row| row.id == local)
                        .ok_or_else(|| fail("conditional cleanup write has no address slot"))?;
                    if !geometry::same_type(ty, &declaration.ty, b)? { return Ok(false); }
                    b.spend(function.parameters.len(), WORK)?;
                    let entry = function.parameters.contains(&local);
                    let mut stored = None;
                    for body in &function.blocks {
                        for (position, op) in body.instructions.iter().enumerate() {
                            b.spend(1, WORK)?;
                            match op.kind {
                                InstructionKind::Load(_) | InstructionKind::StorageDead(_) => continue,
                                InstructionKind::StorageLive(_) if !entry => continue,
                                InstructionKind::Store { local: target, value } if target == local && !entry => {
                                    if stored.replace((body.id, position, value)).is_some() { return Ok(false); }
                                    continue;
                                }
                                _ => {}
                            }
                            // Include reservations, borrowed slot aliases and
                            // unpack destinations among competing definitions.
                            for operand in op.kind.local_operands() {
                                b.spend(1, WORK)?;
                                if operand == local { return Ok(false); }
                            }
                        }
                    }
                    if entry { return Ok(true); }
                    let Some((block, index, value)) = stored else { return Ok(false); };
                    at = (block, index);
                    address = value;
                }
                InstructionKind::BorrowLocalMut { local } => {
                    operations::initialized_local_before(function, at, local, b)?;
                    let declaration = function.locals.get(local.0 as usize).filter(|row| row.id == local)
                        .ok_or_else(|| fail("conditional cleanup write loses its actual local root"))?;
                    return Ok(local != owner && geometry::same_type(inner, &declaration.ty, b)?);
                }
                InstructionKind::AddressField { address: parent, index: field } => {
                    let Some(Type::Address { mutable: true, inner: aggregate }) = function.values.get(parent.0 as usize) else {
                        return Ok(false);
                    };
                    let Some(child) = Self::pattern_child_type(aggregate, PlaceProjection::Field(field)) else {
                        return Ok(false);
                    };
                    if !geometry::same_type(inner, child, b)? { return Ok(false); }
                    address = parent;
                }
                InstructionKind::ReborrowAddress { address: parent, mutable: true } => {
                    if function.values.get(parent.0 as usize).is_none_or(|ty| !matches!(ty, Type::Address { mutable: true, .. })) {
                        return Ok(false);
                    }
                    if !geometry::same_type(&function.values[parent.0 as usize], ty, b)? { return Ok(false); }
                    address = parent;
                }
                _ => return Ok(false),
            }
        }
    }

    // A conditional owning drop is not a read. Validate every live reaching
    // source independently; an actual removal supplies only a vacant debt.
    fn cleanup_local_subject(&self, instance: &SemanticInstance, function: &TypedFunction,
        local: LocalId, at: (BlockId, usize), expected: &value::Value,
        producer: Option<ValueId>, b: &mut Budget) -> CompileResult<()> {
        let initialization = crate::shared_initialization(function, b)?;
        let mut pending = Vec::new();
        self.push_value(&mut pending, at, b)?;
        let mut seen = BTreeSet::new();
        let mut grounded = false;
        while let Some((block, end)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((block, end)) { continue; }
            let body = function.blocks.get(block.0 as usize)
                .filter(|body| body.id == block && end <= body.instructions.len())
                .ok_or_else(|| fail("conditional cleanup loses its actual CFG boundary"))?;
            let mut stopped = false;
            for (index, instruction) in body.instructions.iter().enumerate().take(end).rev() {
                b.spend(1, WORK)?;
                match instruction.kind {
                    InstructionKind::Store { local: target, value } if target == local => {
                        operations::available_before(function, (block, index, SemanticSiteEdge::Before), value, b)?;
                        if let Some(producer) = producer {
                            self.original_producer_value(function, value, producer, b)?;
                        }
                        let actual = self.ssa_value(instance, function, value, b, 0)?;
                        if !actual.equal_paid(expected, b)? {
                            return Err(fail("conditional cleanup changes a complete live original subject"));
                        }
                        grounded = true;
                        stopped = true;
                        break;
                    }
                    InstructionKind::TakeLocal { local: target }
                    | InstructionKind::DropManaged { local: target }
                    | InstructionKind::DropClosure { local: target }
                    | InstructionKind::StorageLive(target) | InstructionKind::StorageDead(target)
                        if target == local => {
                        // The actual resource/epoch kernel must still accept
                        // this removal and the final conditional drop itself.
                        grounded = true;
                        stopped = true;
                        break;
                    }
                    InstructionKind::WriteAddress { address, .. }
                        if self.cleanup_write_disjoint(function, local, (block, index), address, b)? => {}
                    InstructionKind::CommitPlaceReplace { .. } | InstructionKind::WriteAddress { .. }
                    | InstructionKind::StoreStoredBorrow { .. } => {
                        return Err(unknown("conditional cleanup needs its complete indirect replacement source"));
                    }
                    _ => {}
                }
            }
            if stopped { continue; }
            if block == function.entry {
                b.spend(function.parameters.len(), WORK)?;
                if !function.parameters.contains(&local) {
                    return Err(fail("conditional cleanup has no initialized source or actual vacancy"));
                }
                let actual = self.local_value(instance, function, local, (function.entry, 0), b, 0)?;
                if !actual.equal_paid(expected, b)? {
                    return Err(fail("conditional cleanup changes its complete original input subject"));
                }
                grounded = true;
                continue;
            }
            let mut incoming = false;
            for predecessor in &initialization.predecessors[block.0 as usize] {
                b.spend(1, WORK)?;
                if initialization.reachable[*predecessor] {
                    let previous = &function.blocks[*predecessor];
                    self.push_value(&mut pending, (previous.id, previous.instructions.len()), b)?;
                    incoming = true;
                }
            }
            if !incoming { return Err(fail("conditional cleanup has no complete predecessor source")); }
        }
        if !grounded { return Err(unknown("conditional cleanup has an ungrounded source cycle")); }
        Ok(())
    }

    // Cleanup is conditional on element debt. Unlike a read, a genuine
    // preceding take/drop proves vacancy; it never supplies a logical value.
    fn cleanup_slot_subject(&self, instance: &SemanticInstance, function: &TypedFunction,
        slot: &persistent_slots::Slot, at: (BlockId, usize), expected: &value::Value,
        b: &mut Budget) -> CompileResult<()> {
        operations::initialized_local_before(function, at, slot.local(), b)?;
        let initialization = crate::shared_initialization(function, b)?;
        let mut pending = Vec::new();
        self.push_value(&mut pending, at, b)?;
        let mut seen = BTreeSet::new();
        let mut grounded = false;
        while let Some((block, end)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((block, end)) { continue; }
            let body = function.blocks.get(block.0 as usize)
                .filter(|body| body.id == block && end <= body.instructions.len())
                .ok_or_else(|| fail("slot cleanup loses its exact actual CFG boundary"))?;
            let mut stopped = false;
            for (index, instruction) in body.instructions.iter().enumerate().take(end).rev() {
                b.spend(1, WORK)?;
                self.head_projection_barrier(function, slot.local(), &[], &instruction.kind, b)?;
                let source = match instruction.kind {
                    InstructionKind::StoreOwnedArrayElement { local, index: 0, value }
                        if local == slot.local() => Some(value),
                    InstructionKind::Store { local, value } if local == slot.local() => {
                        if (block, index) != slot.initial_store() || value != slot.array() {
                            return Err(fail("slot cleanup crosses an unrelated shell replacement"));
                        }
                        Some(slot.initial())
                    }
                    InstructionKind::TakeOwnedArrayElement { local, index: 0 }
                    | InstructionKind::DropOwnedArrayElement { local, index: 0 } if local == slot.local() => {
                        grounded = true;
                        stopped = true;
                        break;
                    }
                    InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local)
                    | InstructionKind::DropOwnedArray { local } | InstructionKind::TakeLocal { local }
                        if local == slot.local() => {
                        return Err(fail("slot cleanup crosses a vacant shell activation"));
                    }
                    _ => None,
                };
                if let Some(source) = source {
                    operations::available_before(function, (block, index, SemanticSiteEdge::Before), source, b)?;
                    let actual = self.ssa_value(instance, function, source, b, 0)?;
                    if !actual.equal_paid(expected, b)? {
                        return Err(fail("slot cleanup changes a complete live original child source"));
                    }
                    grounded = true;
                    stopped = true;
                    break;
                }
            }
            if stopped { continue; }
            if block == function.entry {
                return Err(fail("slot cleanup has no initialized original child or actual removal"));
            }
            let mut incoming = false;
            for predecessor in &initialization.predecessors[block.0 as usize] {
                b.spend(1, WORK)?;
                if initialization.reachable[*predecessor] {
                    let previous = &function.blocks[*predecessor];
                    self.push_value(&mut pending, (previous.id, previous.instructions.len()), b)?;
                    incoming = true;
                }
            }
            if !incoming { return Err(fail("slot cleanup has no complete predecessor activation")); }
        }
        if !grounded { return Err(unknown("slot cleanup has an ungrounded occupancy cycle")); }
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
        event: original::CleanupEvent, b: &mut Budget) -> CompileResult<BTreeSet<ReplacementSource>> {
        let node = match event {
            original::CleanupEvent::Replace(node) | original::CleanupEvent::Backedge(node) => node,
            _ => return Ok(BTreeSet::new()),
        };
        let index = self.inventory.original_index(instance.definition, b)?;
        let mut result = BTreeSet::new();
        for statement in index.active_writes_under(node, b)? {
            let Some(write) = self.original.cleanup_write(instance.definition, statement.id, b)? else { continue; };
            b.spend(path.len().saturating_add(write.path.len()).saturating_add(1), WORK)?;
            if !matches!(source, original::CleanupSource::Local(anchor) if anchor == write.anchor)
                || !path.starts_with(&write.path) { continue; }
            if let Some(value) = self.original_subject_value(instance, function, write.value, b)? {
                let mut selected = Vec::new();
                for field in &path[write.path.len()..] {
                    self.push_value(&mut selected, PlaceProjection::Field(*field), b)?;
                }
                b.lookup(result.len(), WORK)?;
                b.spend(selected.len().saturating_add(1), WORK)?;
                result.insert((value, selected));
            } else {
                return Err(unknown("original cleanup replacement needs its complete actual RHS emission"));
            }
        }
        if result.is_empty() {
            return Err(unknown("original cleanup replacement/loop has no exact actual write values"));
        }
        Ok(result)
    }

    fn cleanup_event_matches(&self, function: &TypedFunction, local: LocalId,
        block: BlockId, position: usize, event: original::CleanupEvent,
        replacements: &BTreeSet<ReplacementSource>, b: &mut Budget) -> CompileResult<bool> {
        match event {
            original::CleanupEvent::Scope | original::CleanupEvent::Discard(_) =>
                self.cleanup_scope_boundary(function, local, block, position + 1, b),
            original::CleanupEvent::Replace(_) | original::CleanupEvent::Backedge(_) => {
                let root = (block, position + 1);
                let mut pending = vec![root];
                let mut seen = BTreeSet::new();
                let mut graph = BTreeMap::new();
                let mut settled = BTreeSet::new();
                while let Some((next, start)) = pending.pop() {
                    b.lookup(seen.len(), WORK)?;
                    if !seen.insert((next, start)) { continue; }
                    let body = function.blocks.get(next.0 as usize).filter(|body| body.id == next)
                        .ok_or_else(|| fail("original cleanup replacement has no actual CFG block"))?;
                    let mut wrote = false;
                    for (offset, instruction) in body.instructions.iter().enumerate().skip(start) {
                        b.spend(1, WORK)?;
                        let stored = match instruction.kind {
                            InstructionKind::Store { local: target, value } if target == local => Some(value),
                            InstructionKind::StoreOwnedArrayElement { local: target, index: 0, value } if target == local => Some(value),
                            InstructionKind::StorageLive(target) | InstructionKind::StorageDead(target)
                            | InstructionKind::DropManaged { local: target } | InstructionKind::DropClosure { local: target }
                                if target == local => return Ok(false),
                            _ => None,
                        };
                        let Some(value) = stored else { continue; };
                        let mut matched = false;
                        for (producer, path) in replacements {
                            b.spend(path.len().saturating_add(1), WORK)?;
                            if !operations::value_available_before(function,
                                (block, position, SemanticSiteEdge::Before), *producer, b)? { continue; }
                            let same = if path.is_empty() {
                                self.operation_operand_matches(function, value, *producer, b)?
                            } else {
                                let origins = self.call_operand_origins(function, value, b)?;
                                let mut complete = true;
                                for origin in origins {
                                    complete &= self.pattern_ssa_path(function, origin, *producer, path, b, 0)?;
                                }
                                complete
                            };
                            if same {
                                // Preparation precedes destruction. A later load of
                                // its owning temporary is still this same RHS.
                                matched = true;
                            }
                        }
                        if !matched { return Ok(false); }
                        if matches!(event, original::CleanupEvent::Backedge(_))
                            && !self.cleanup_cyclic_write(function, local, block, position, next, offset, b)? {
                            return Ok(false);
                        }
                        b.lookup(settled.len(), WORK)?;
                        settled.insert((next, start));
                        wrote = true;
                        break;
                    }
                    if wrote { continue; }
                    let successors = match body.terminator {
                        Terminator::Jump(target) => [Some((target, 0)), None],
                        Terminator::Branch { then_block, else_block, .. } => [Some((then_block, 0)), Some((else_block, 0))],
                        _ => return Ok(false),
                    };
                    let mut edges = Vec::new();
                    for successor in successors.into_iter().flatten() {
                        self.push_value(&mut edges, successor, b)?;
                        self.push_value(&mut pending, successor, b)?;
                    }
                    b.lookup(graph.len(), WORK)?;
                    graph.insert((next, start), edges);
                }
                // Every continuation must reach its matching replacement;
                // an ungrounded loop is not evidence of a completed event.
                loop {
                    let mut progress = false;
                    for (node, edges) in &graph {
                        b.lookup(settled.len(), WORK)?;
                        if settled.contains(node) { continue; }
                        let mut complete = true;
                        for edge in edges {
                            b.lookup(settled.len(), WORK)?;
                            complete &= settled.contains(edge);
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
            original::CleanupEvent::CaptureExit(_) => Err(fail("remaining capture cleanup lost its exact environment source")),
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

    #[allow(clippy::too_many_arguments)]
    fn cleanup_cyclic_write(&self, function: &TypedFunction, local: LocalId,
        block: BlockId, drop_position: usize, store_block: BlockId, store_position: usize, b: &mut Budget) -> CompileResult<bool> {
        let mut pending = vec![(store_block, store_position + 1)];
        let mut seen = BTreeSet::new();
        while let Some((next, start)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((next, start)) { continue; }
            let body = function.blocks.get(next.0 as usize).filter(|body| body.id == next)
                .ok_or_else(|| fail("original cleanup backedge has no actual CFG block"))?;
            let mut ended = false;
            for (position, instruction) in body.instructions.iter().enumerate().skip(start) {
                b.spend(1, WORK)?;
                if next == block && position == drop_position { return Ok(true); }
                ended = match instruction.kind {
                    InstructionKind::StorageDead(target) | InstructionKind::StorageLive(target)
                    | InstructionKind::Store { local: target, .. } | InstructionKind::TakeLocal { local: target }
                    | InstructionKind::TakeOwnedArrayElement { local: target, .. }
                    | InstructionKind::StoreOwnedArrayElement { local: target, .. }
                    | InstructionKind::DropOwnedArrayElement { local: target, .. }
                    | InstructionKind::DropOwnedArray { local: target }
                    | InstructionKind::DropManaged { local: target } | InstructionKind::DropClosure { local: target }
                        => target == local,
                    // A break/error/return leaves this original loop, even if
                    // an outer loop later reaches a numerically identical block.
                    InstructionKind::ScopeExit { kind: CleanupExitKind::Break | CleanupExitKind::Return
                        | CleanupExitKind::Error, .. } => true,
                    _ => false,
                };
                if ended { break; }
            }
            if ended { continue; }
            let targets = match body.terminator {
                Terminator::Jump(next) => [Some(next), None],
                Terminator::Branch { then_block, else_block, .. } => [Some(then_block), Some(else_block)],
                _ => [None, None],
            };
            for target in targets.into_iter().flatten() {
                self.push_value(&mut pending, (target, 0), b)?;
            }
        }
        Ok(false)
    }

}
