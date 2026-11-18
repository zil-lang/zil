//! Bind an original abstract-call subject to its exact actual receiver slot and
//! invocation. Type/role facts never grant its data or receiver-head loans.
use super::*;
use crate::semantic::value::Value;

impl Check<'_, '_> {
    fn abstract_copy_type(&self, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("abstract Copy receiver exceeds its finite capture directory"));
        }
        if let Type::Closure(id) = ty {
            let definition = self.module.closures.get(id.0 as usize).filter(|d| d.id == *id)
                .ok_or_else(|| fail("abstract Copy receiver has no exact environment"))?;
            if !matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut) { return Ok(false); }
            self.capture_directory(*id, b)?;
            for capture in &definition.captures {
                if !self.abstract_copy_type(capture, b, depth + 1)? { return Ok(false); }
            }
            return Ok(true);
        }
        original::nominal::predicate_actual(ty, original::Predicate::Copy, self.inventory, b, depth + 1)
    }

    /// A copied callable still comes from the selected initialized original
    /// place. Matching environment types cannot replace the actual Load or
    /// physical field read, and this proof never takes an environment address.
    #[allow(clippy::too_many_arguments)]
    fn abstract_copy_place(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        id: ValueId, source: &original::borrow_places::Source, path: &[u32],
        b: &mut Budget, depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("abstract copied receiver exceeds its finite read ancestry"));
        }
        if source.indirect || source.index.is_some() || source.expression.is_some() {
            return Err(unknown("abstract copied receiver needs its exact direct original place"));
        }
        let (block, index, producer) = self.defining(function, id, b)?;
        operations::available_before(function, (block, index + 1, SemanticSiteEdge::Before), id, b)?;
        match &producer.kind {
            InstructionKind::ExtractField { aggregate, index: field } => {
                if path.last() != Some(field) {
                    return Err(fail("abstract copied receiver selects another physical field"));
                }
                operations::available_before(function, (block, index, SemanticSiteEdge::Before), *aggregate, b)?;
                self.abstract_copy_place(instance, function, *aggregate, source, &path[..path.len()-1], b, depth + 1)?;
            }
            InstructionKind::Load(local) | InstructionKind::ReadPackageField { local, .. } => {
                let selected = match &producer.kind {
                    InstructionKind::Load(_) => path.is_empty(),
                    InstructionKind::ReadPackageField { index, .. } => path == [*index],
                    _ => false,
                };
                if !selected || self.anchored_local(instance, function,
                    source.anchor.ok_or_else(|| fail("abstract copied receiver has no original anchor"))?, b)? != *local {
                    return Err(fail("abstract copied receiver selects another original local"));
                }
                operations::initialized_local_before(function, (block, index), *local, b)?;
                let root = self.local_value(instance, function, *local, (block, index), b, depth + 1)?;
                if !self.capture_values_equal(&root, &source.root, b)? {
                    return Err(fail("abstract copied receiver changes its complete original root domain"));
                }
            }
            InstructionKind::ReadCapture { index } => {
                let Some(original::LocalAnchor::Environment(ordinal)) = source.anchor else {
                    return Err(fail("abstract copied receiver replaces an original local by a capture"));
                };
                if !path.is_empty() || self.environment_capture(instance, function, ordinal, b)? != *index {
                    return Err(fail("abstract copied receiver selects another original capture"));
                }
                let root = self.capture_whole_value(instance, function, *index, b, depth + 1)?;
                if !self.capture_values_equal(&root, &source.root, b)? {
                    return Err(fail("abstract copied receiver changes its complete captured root domain"));
                }
            }
            _ => return Err(unknown("abstract copied receiver needs its exact initialized read opcode")),
        }
        Ok(())
    }

    /// A lending call through a borrowed parameter produces a fresh view.
    /// Returning it under that parameter's bound keeps the real call loan
    /// (including its data parents) alive in the memory proof.
    pub(super) fn forwarded_receiver_return(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        exit: BlockId,
        returned: ValueId,
        actual: &Value,
        expected: &Value,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let (block, index, operation) = self.defining(function, returned, b)?;
        let InstructionKind::CallClosureBorrowed { callee, kind, .. } = operation.kind else {
            return Ok(false);
        };
        let Some(Type::Address { inner, .. }) = function.values.get(callee.0 as usize) else {
            return Ok(false);
        };
        let Type::Closure(closure) = inner.as_ref() else { return Ok(false) };
        b.lookup(b.semantic_receiver_views.len(), WORK)?;
        if b.semantic_receiver_views.get(&(function.id, returned, *closure)) != Some(&(block, index))
            || !matches!(expected.ty, Type::Address { mutable: false, .. })
            || !geometry::same_type(&actual.ty, &expected.ty, b)?
            || !self.capture_values_equal(&self.pointee(actual, b)?, &self.pointee(expected, b)?, b)?
        {
            return Ok(false);
        }
        let Some(header) = &function.storage_life else { return Ok(false) };
        b.spend(header.bindings.len(), WORK)?;
        let Some(result @ StorageBinder::Invocation(_)) =
            crate::signature_life::outer(header, StorageBase::Result) else { return Ok(false) };
        let [slot] = expected.life.slots.as_slice() else { return Ok(false) };
        if !slot.path.is_empty() || slot.mode != StoredBorrowMode::SharedAddress
            || expected.classes.get(slot.binder as usize) != Some(&self.header_class(instance, result, b)?)
        {
            return Ok(false);
        }

        for site in self.inventory.sites_at(self.program, instance.id, block, index, SemanticSiteEdge::After, b)? {
            if !matches!(site.action, SemanticSiteAction::BodyCall { .. }) { continue; }
            let Some(call) = self.original.abstract_boundary(instance, site.recipe, self.program, self.inventory, b)? else {
                continue;
            };
            let Some(parameter) = call.parameter else { continue; };
            if kind != call.kind || !original::receiver_result(&call.signature, kind)
                || !self.capture_values_equal(actual, &call.result, b)?
                || !self.anchored_value(instance, function, original::LocalAnchor::Parameter(parameter), callee, b, 0)?
            {
                continue;
            }
            b.spend(header.bindings.len(), WORK)?;
            if crate::signature_life::outer(header, StorageBase::Parameter(parameter)) != Some(result) {
                return Ok(false);
            }
            // The same borrowed receiver and argument roles have already been
            // established by abstract_operation for this exact invocation.
            let directory = self.inventory.original_index(instance.definition, b)?;
            for expression in directory.active_returns(b)?.into_iter().flatten() {
                if self.original_subject_value(instance, function, expression.id, b)? == Some(returned) {
                    b.lookup(b.semantic_receiving_returns.len(), WORK)?;
                    b.semantic_receiving_returns.insert((function.id, exit, returned, result));
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub(super) fn abstract_operation(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let mut selected = None;
        for site in self.inventory.sites_at(
            self.program, instance.id, block, index, SemanticSiteEdge::After, b,
        )? {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After } if actual == block && instruction as usize == index)
            {
                continue;
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                continue;
            };
            let Some(boundary) = self.original.abstract_boundary(
                instance,
                site.recipe,
                self.program,
                self.inventory,
                b,
            )?
            else {
                continue;
            };
            if !actuals.is_empty() {
                return Err(fail("abstract invocation invents mixed target actuals"));
            }
            if selected.is_some() {
                return Err(fail(
                    "abstract opcode has competing original invocation roles",
                ));
            }
            let instruction = function
                .blocks
                .get(block.0 as usize)
                .filter(|b| b.id == block)
                .and_then(|b| b.instructions.get(index))
                .ok_or_else(|| fail("abstract invocation has no actual instruction"))?;
            let (callee, local_callee, kind, arguments, copied) = match &instruction.kind {
                InstructionKind::CallClosureBorrowed {
                    callee,
                    kind,
                    arguments,
                } => (Some(*callee), None, *kind, arguments, false),
                InstructionKind::CallClosure { callee, arguments }
                    if boundary.place.is_some() => (Some(*callee), None, CallableKind::Fn, arguments, true),
                InstructionKind::CallClosureShared { callee, arguments }
                    if boundary.place.is_some() => (None, Some(*callee), CallableKind::Fn, arguments, false),
                InstructionKind::CallClosureMut { callee, arguments }
                    if boundary.place.is_some() => (None, Some(*callee), CallableKind::FnMut, arguments, false),
                InstructionKind::CallClosureOnce { callee, arguments }
                    if boundary.place.is_some() =>
                {
                    (Some(*callee), None, CallableKind::FnOnce, arguments, false)
                }
                _ => {
                    return Err(unknown(
                        "abstract invocation changes its actual receiver opcode",
                    ));
                }
            };
            if kind != boundary.kind
                || boundary.signature.call_kind == CallKind::Suspend
                    && !(kind == CallableKind::FnOnce && function.call_kind == CallKind::Suspend
                        && matches!(instruction.kind, InstructionKind::CallClosureOnce { .. })) {
                return Err(unknown(
                    "abstract callable needs its complete requested invocation ABI and receiver mode",
                ));
            }
            if let Some(callee) = callee {
                operations::available_before(function, (block, index, SemanticSiteEdge::Before), callee, b)?;
            }
            if let Some(source) = &boundary.place {
                if let Some(local) = local_callee {
                    if source.indirect || !source.path.is_empty() || source.index.is_some() || source.expression.is_some() {
                        return Err(fail("abstract local invocation changes its exact original place"));
                    }
                    let anchor = source.anchor
                        .ok_or_else(|| fail("abstract local invocation has no original anchor"))?;
                    let original = self.anchored_local(instance, function, anchor, b)?;
                    if kind != CallableKind::Fn && original != local {
                        return Err(fail("abstract exclusive invocation changes its exact original place"));
                    }
                    let source_point = self.prepared_owner_source(function, original, local, (block, index), b)?;
                    let declaration = function.locals.get(local.0 as usize).filter(|item| item.id == local)
                        .ok_or_else(|| fail("abstract local receiver has no exact local declaration"))?;
                    if declaration.ty != boundary.subject.ty || kind == CallableKind::FnMut && !declaration.mutable {
                        return Err(fail("abstract local invocation changes its complete receiver type or mode"));
                    }
                    let actual = self.local_value(instance, function, local, (block, index), b, 0)?;
                    if original != local {
                        let origin = self.local_value(instance, function, original, source_point, b, 0)?;
                        if !self.capture_values_equal(&actual, &origin, b)? {
                            return Err(fail("abstract local preparation changes its complete source domain"));
                        }
                    }
                    if !self.capture_values_equal(&actual, &source.root, b)? {
                        return Err(fail("abstract local invocation changes its full original root domain"));
                    }
                } else if copied {
                    let callee = callee.ok_or_else(|| fail("abstract copied receiver loses its actual SSA"))?;
                    self.abstract_copy_place(instance, function, callee, source, &source.path, b, 0)?;
                    let actual = self.ssa_value(instance, function, callee, b, 0)?;
                    if !self.capture_values_equal(&actual, &boundary.subject, b)?
                        || !self.abstract_copy_type(&boundary.subject.ty, b, 0)? {
                        return Err(fail("abstract copied invocation changes its full original receiver or Copy capability"));
                    }
                } else if kind == CallableKind::FnOnce {
                    let callee = callee.ok_or_else(|| fail("abstract owned receiver loses its actual SSA"))?;
                    self.consuming_receiver_place(instance, function, (block, index), callee, source, b)?;
                } else {
                    let callee = callee.ok_or_else(|| fail("abstract borrowed receiver loses its actual SSA"))?;
                    self.borrowed_invocation_place(instance, function, instruction, source, b)?;
                    let Some(Type::Address { mutable, inner }) =
                        function.values.get(callee.0 as usize)
                    else {
                        return Err(fail(
                            "abstract place receiver loses its environment address",
                        ));
                    };
                    if inner.as_ref() != &boundary.subject.ty
                        || kind == CallableKind::FnMut && !*mutable
                    {
                        return Err(fail(
                            "abstract place invocation changes its full type or mode",
                        ));
                    }
                }
                if !self.capture_values_equal(&source.inner, &boundary.subject, b)? {
                    return Err(fail(
                        "abstract place invocation changes its full original child domain",
                    ));
                }
            } else {
                let callee = callee.ok_or_else(|| fail("abstract borrowed parameter loses its actual SSA"))?;
                let parameter = self.anchored_local(
                    instance,
                    function,
                    original::LocalAnchor::Parameter(boundary.parameter.ok_or_else(|| {
                        fail("abstract receiver has neither an original parameter nor a checked place")
                    })?),
                    b,
                )?;
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    callee,
                    b,
                )?;
                if self.receiver_local(function, callee, b, 0)? != parameter {
                    return Err(fail(
                        "abstract call selects another original receiver parameter",
                    ));
                }
                let receiver = self.ssa_value(instance, function, callee, b, 0)?;
                let Type::Address { mutable, inner } = &receiver.ty else {
                    return Err(fail(
                        "abstract call receiver is not an actual environment address",
                    ));
                };
                let Type::Address {
                    mutable: expected_mutable,
                    ..
                } = &boundary.receiver.ty
                else {
                    return Err(fail(
                        "original abstract receiver lost its reference carrier",
                    ));
                };
                if (*mutable && !*expected_mutable)
                    || (kind == CallableKind::FnMut && !*mutable)
                    || inner.as_ref() != &boundary.subject.ty
                {
                    return Err(fail(
                        "abstract call changes its original complete receiver type or mode",
                    ));
                }
                let expected_mutable = *expected_mutable;
                let mut expected_receiver = boundary.receiver;
                if !*mutable && expected_mutable {
                    let geometry = expected_receiver.geometry_mut(b)?;
            let Type::Address { inner, .. } = &geometry.ty else {
                        unreachable!()
                    };
                    crate::dyn_metadata::ty(inner, b)?;
                    geometry.ty = Type::Address {
                        mutable: false,
                        inner: inner.clone(),
                    };
                    if geometry.life.slots.len() != 1 {
                        return Err(fail("original receiver omits its outer occurrence"));
                    }
                    geometry.life.slots[0].mode = StoredBorrowMode::SharedAddress;
                }
                if !self.outer_role(instance, function, &receiver, &expected_receiver, b, 0)? {
                    return Err(fail(
                        "abstract receiver does not satisfy its original head role",
                    ));
                }
            }
            let Type::Closure(id) = boundary.subject.ty else {
                return Err(unknown(
                    "abstract function pointer requires its complete pointer invocation proof",
                ));
            };
            b.spend(self.module.closures.len().saturating_add(1), WORK)?;
            let definition = self
                .module
                .closures
                .get(id.0 as usize)
                .filter(|d| d.id == id)
                .ok_or_else(|| {
                    fail("abstract receiver has no matching actual environment definition")
                })?;
            let allowed = match kind {
                CallableKind::Fn => definition.kind == CallableKind::Fn,
                CallableKind::FnMut => matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut),
                CallableKind::FnOnce => matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce),
                CallableKind::FunctionPointer => false,
            };
            if !allowed {
                return Err(fail("abstract opcode selects an environment with another actual callable mode"));
            }
            let mut context = instance;
            let mut found = false;
            for _ in 0..=b.partial_depth {
                b.spend(1, WORK)?;
                if context.definition == boundary.signature_owner {
                    found = true;
                    break;
                }
                context = self.inventory.instance(
                    context.parent.ok_or_else(|| {
                        fail("abstract role leaves its actual lexical parent chain")
                    })?,
                    b,
                )?;
            }
            if !found {
                return Err(unknown(
                    "abstract role lexical chain exceeds its finite depth",
                ));
            }
            self.capture_directory(id, b)?;
            self.quantified_callable_signature(
                context,
                &boundary.subject,
                definition,
                &boundary.signature,
                boundary.kind,
                &boundary.bounds,
                b,
            )?;
            if arguments.len() != boundary.parameters.len()
                || arguments.len() != definition.parameters.len()
            {
                return Err(fail(
                    "abstract actual invocation changes its complete argument arity",
                ));
            }
            for (ordinal, ((argument, expected), actual_type)) in arguments
                .iter()
                .zip(&boundary.parameters)
                .zip(&definition.parameters).enumerate()
            {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *argument,
                    b,
                )?;
                if function.values.get(argument.0 as usize) != Some(&expected.ty)
                    || actual_type != &expected.ty
                {
                    return Err(fail(
                        "abstract invocation changes an original argument whole type",
                    ));
                }
                if self.uses_full_value(&expected.ty, b)? {
                    let actual = self.ssa_value(instance, function, *argument, b, 0)?;
                    let valid = if matches!(expected.ty, Type::Address { .. } | Type::Desc(_)) {
                        self.outer_call_role(instance, function, (block, index), ordinal, *argument, &actual, expected, b, 0)?
                    } else {
                        actual == *expected
                    };
                    if !valid {
                        return Err(fail(
                            "abstract invocation changes a complete original argument role",
                        ));
                    }
                }
            }
            if definition.result != boundary.result.ty {
                return Err(fail(
                    "abstract invocation changes its promised whole result type",
                ));
            }
            match instruction.result {
                Some(value)
                    if function.values.get(value.0 as usize) == Some(&boundary.result.ty) => {}
                None if boundary.result.ty == Type::Unit && boundary.result.classes.is_empty() => {}
                _ => {
                    return Err(fail(
                        "abstract invocation has another actual result SSA type",
                    ));
                }
            }
            // This is a semantic result-role substitution after proving the
            // actual receiver/entry/full signature. Actual roots, retained
            // parents, loans, effects and cleanup are still independently
            // reconstructed by the memory/callable kernels before publication.
            if original::receiver_result(&boundary.signature, kind) {
                if !crate::stored_callables::receiver_access_head(definition) {
                    if !crate::stored_callables::constant_result(definition) {
                        return Err(fail("abstract Receiver role loses its actual invocation access head"));
                    }
                    self.static_byte_returns(self.inventory.function(
                        &SemanticFunctionTarget::Typed(definition.function), b)?, &definition.result, b)?;
                }
                let result = instruction.result.ok_or_else(|| fail("abstract Receiver call has no actual result SSA"))?;
                let key = (function.id, result, id);
                b.lookup(b.semantic_receiver_views.len(), WORK)?;
                if let Some(previous) = b.semantic_receiver_views.insert(key, (block, index)) {
                    if previous != (block, index) { return Err(fail("abstract Receiver changes its exact actual call identity")); }
                }
            }
            if kind == CallableKind::FnOnce {
                record_consuming_operation(function.id, block, index, id,
                    boundary.signature.call_kind, b)?;
            }
            selected = Some(boundary.result);
        }
        Ok(selected)
    }
}

impl Check<'_, '_> {
    /// A zero-class callable still has an exact current owner. The original
    /// temporary's emitted SSA and the final initialization of its actual
    /// receiver local must agree; equal closure types do not choose an owner.
    pub(super) fn known_consuming_receiver(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        at: (BlockId, usize), callee: ValueId, site: SemanticSiteId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        operations::available_before(function, (at.0, at.1, SemanticSiteEdge::Before), callee, b)?;
        let source = self.original.borrowed_closure_receiver(instance, site,
            self.program, self.inventory, b)?;
        self.consuming_receiver_place(instance, function, at, callee, &source, b)
    }

    fn consuming_receiver_place(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        at: (BlockId, usize), callee: ValueId, source: &original::borrow_places::Source,
        b: &mut Budget,
    ) -> CompileResult<()> {
        operations::available_before(function, (at.0, at.1, SemanticSiteEdge::Before), callee, b)?;
        if let Some(node) = source.expression {
            if source.anchor.is_some() || source.indirect || !source.path.is_empty()
                || source.index.is_some() || source.slice.is_some() {
                return Err(fail("temporary consuming receiver changes its original whole expression"));
            }
            let expected = self.original_subject_value(instance, function, node, b)?
                .ok_or_else(|| fail("temporary consuming receiver has no exact emitted original value"))?;
            self.original_producer_value(function, callee, expected, b)?;
            let actual = self.ssa_value(instance, function, callee, b, 0)?;
            if !self.capture_values_equal(&actual, &source.root, b)? {
                return Err(fail("temporary consuming receiver changes its complete original value"));
            }
            Ok(())
        } else {
            self.checked_owned_place(instance, function, callee, source, b)
        }
    }

    pub(super) fn borrowed_invocation_place(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        invocation: &Instruction,
        source: &original::borrow_places::Source,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let InstructionKind::CallClosureBorrowed { callee, kind, .. } = invocation.kind else {
            return Err(fail("borrowed receiver lacks its actual invocation"));
        };
        let (_, _, producer) = self.defining(function, callee, b)?;
        // Calling through an existing &F reads its original pointer. The
        // invocation creates the access window; that earlier Load does not
        // manufacture a second borrow of the variable holding the pointer.
        let from_pointer = source.indirect && source.path.is_empty()
            && source.index.is_none() && source.slice.is_none()
            && matches!(producer.kind, InstructionKind::Load(_) | InstructionKind::TakeLocal { .. })
            && matches!(&source.root.ty, Type::Address { inner, mutable }
                if matches!(inner.as_ref(), Type::Closure(_))
                    && matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                    && (kind != CallableKind::FnMut || *mutable));
        self.checked_borrow_place(instance, function, callee, source, false, from_pointer, b, 0)
    }

    pub(super) fn known_receiver_place(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        at: (BlockId, usize), instruction: &Instruction,
        source: &original::borrow_places::Source, b: &mut Budget,
    ) -> CompileResult<()> {
        let local = match instruction.kind {
            InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. } => callee,
            InstructionKind::CallClosureBorrowed { callee, .. } => {
                if source.expression.is_none() {
                    return self.borrowed_invocation_place(instance, function, instruction, source, b);
                }
                self.receiver_local(function, callee, b, 0)?
            }
            _ => return Err(fail("Receiver view cannot escape a copied or consuming environment")),
        };
        operations::initialized_local_before(function, at, local, b)?;
        let actual = function.locals.get(local.0 as usize).filter(|row| row.id == local)
            .ok_or_else(|| fail("Receiver place has no actual local declaration"))?;
        if !geometry::same_type(&actual.ty, &source.root.ty, b)? || !source.path.is_empty() || source.indirect
            || source.index.is_some() {
            return Err(fail("Receiver place changes its complete original owning subject"));
        }
        if let Some(anchor) = source.anchor {
            if self.anchored_local(instance, function, anchor, b)? != local {
                return Err(fail("Receiver call selects another original lexical owner"));
            }
        } else {
            let node = source.expression.ok_or_else(|| fail("temporary Receiver omits its original expression"))?;
            let expected = self.original_subject_value(instance, function, node, b)?
                .ok_or_else(|| fail("temporary Receiver has no exact emitted original SSA"))?;
            self.original_producer_local(function, local, at, expected, b)?;
        }
        let current = self.local_value(instance, function, local, at, b, 0)?;
        if !self.capture_values_equal(&current, &source.root, b)? {
            return Err(fail("Receiver call changes its complete original subject classes"));
        }
        Ok(())
    }

    /// Authenticate every concrete invocation, including scalar-result calls
    /// that never request the shorter Receiver result view.
    pub(super) fn known_call_place(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        at: (BlockId, usize), instruction: &Instruction,
        source: &original::borrow_places::Source, b: &mut Budget,
    ) -> CompileResult<()> {
        match instruction.kind {
            InstructionKind::CallClosureOnce { callee, .. } =>
                self.consuming_receiver_place(instance, function, at, callee, source, b),
            InstructionKind::CallClosure { callee, .. } => {
                operations::available_before(function, (at.0, at.1, SemanticSiteEdge::Before), callee, b)?;
                if source.expression.is_some() {
                    self.consuming_receiver_place(instance, function, at, callee, source, b)?;
                } else {
                    self.abstract_copy_place(instance, function, callee, source, &source.path, b, 0)?;
                    if !self.abstract_copy_type(&source.inner.ty, b, 0)? {
                        return Err(fail("copied concrete receiver lacks its complete Copy environment proof"));
                    }
                }
                let actual = self.ssa_value(instance, function, callee, b, 0)?;
                if !self.capture_values_equal(&actual, &source.inner, b)? {
                    return Err(fail("copied concrete invocation changes its original full receiver domain"));
                }
                Ok(())
            }
            InstructionKind::CallClosureShared { .. }
            | InstructionKind::CallClosureMut { .. }
            | InstructionKind::CallClosureBorrowed { .. } =>
                self.known_receiver_place(instance, function, at, instruction, source, b),
            _ => Err(fail("concrete invocation loses its exact callable operation")),
        }
    }
}
