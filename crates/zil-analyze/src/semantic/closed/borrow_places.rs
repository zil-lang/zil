//! Actual projected borrows follow physical field operations to the original
//! initialized root. Class claims neither select a root nor cross an owner.
use super::*;

enum Root {
    Local(LocalId, BlockId, usize, bool), // bool: loaded an existing address
    Slot(LocalId, BlockId, usize),
    Capture(u32, bool),
    Expression(ValueId),
}
struct Origin {
    root: Root,
    path: Vec<u32>,
    index: Option<ValueId>,
    fresh: bool,
}

impl Check<'_, '_> {
    pub(super) fn checked_owned_place(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        source: &original::borrow_places::Source,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if source.indirect || source.index.is_some() || source.expression.is_some() {
            return Err(fail(
                "owning callable cannot move through an indirect original place",
            ));
        }
        let local = self.anchored_local(
            instance,
            function,
            source
                .anchor
                .ok_or_else(|| fail("owning callable has no original anchor"))?,
            b,
        )?;
        self.owned_place_value(
            instance,
            function,
            id,
            local,
            source,
            &mut BTreeSet::new(),
            b,
            0,
        )
    }

    // Callee lowering can move the selected field into a temporary and then
    // consume that temporary. Follow actual consuming transfers; Load, an
    // intervening access, a vacant activation or a different root cannot stand
    // in for the original move. Every incoming CFG path must prove the origin.
    #[allow(clippy::too_many_arguments)]
    fn owned_place_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        local: LocalId,
        source: &original::borrow_places::Source,
        active: &mut BTreeSet<(LocalId, BlockId, usize)>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "owning callable transfer exceeds its finite ancestry",
            ));
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        if function.values.get(id.0 as usize) != Some(&source.inner.ty) {
            return Err(fail(
                "owning callable transfer changes its complete child type",
            ));
        }
        b.spend(source.path.len().saturating_add(1), WORK)?;
        if let InstructionKind::TakeOwnedArrayElement {
            local: actual,
            index: 0,
        } = instruction.kind
            && actual == local
            && source.path.is_empty()
        {
            let slot = self
                .persistent_slot(instance, function, local, b)?
                .ok_or_else(|| {
                    fail("owning callback element lacks its exact original slot witness")
                })?;
            let root = self.persistent_slot_value(
                instance,
                function,
                &slot,
                (block, index),
                b,
                depth + 1,
            )?;
            if !self.capture_values_equal(&root, &source.root, b)?
                || !self.capture_values_equal(&root, &source.inner, b)?
            {
                return Err(fail(
                    "owning callback element changes its complete original source value",
                ));
            }
            return Ok(());
        }
        let matches = match &instruction.kind {
            InstructionKind::TakeLocal { local: actual } => {
                *actual == local && source.path.is_empty()
            }
            InstructionKind::TakePackageField {
                local: actual,
                index: field,
            } => *actual == local && source.path.as_slice() == [*field],
            InstructionKind::TakeTupleField {
                local: actual,
                path,
            } => {
                *actual == local
                    && path.iter().copied().eq(source
                        .path
                        .iter()
                        .copied()
                        .map(PlaceProjection::Field))
            }
            _ => false,
        };
        if matches {
            let root = self.local_value(instance, function, local, (block, index), b, 0)?;
            if !self.capture_values_equal(&root, &source.root, b)?
                || self.borrow_field_type(&root.ty, &source.path, b)? != &source.inner.ty
            {
                return Err(fail(
                    "owning callable loses its original root and complete child domain",
                ));
            }
            return Ok(());
        }
        let InstructionKind::TakeLocal { local: temporary } = instruction.kind else {
            return Err(fail(
                "owning callable takes another original place, field or full type",
            ));
        };
        self.owned_place_local(
            instance,
            function,
            temporary,
            (block, index),
            local,
            source,
            active,
            b,
            depth + 1,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn owned_place_local(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        temporary: LocalId,
        at: (BlockId, usize),
        local: LocalId,
        source: &original::borrow_places::Source,
        active: &mut BTreeSet<(LocalId, BlockId, usize)>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "owning callable temporary exceeds its finite ancestry",
            ));
        }
        let declaration = function
            .locals
            .get(temporary.0 as usize)
            .filter(|item| item.id == temporary)
            .ok_or_else(|| fail("owning callable temporary has no exact local declaration"))?;
        if declaration.ty != source.inner.ty {
            return Err(fail(
                "owning callable temporary changes its complete stored type",
            ));
        }
        let key = (temporary, at.0, at.1);
        b.lookup(active.len(), WORK)?;
        if !active.insert(key) {
            return Err(unknown(
                "owning callable transfer has an ungrounded CFG cycle",
            ));
        }
        let block = function
            .blocks
            .get(at.0.0 as usize)
            .filter(|block| block.id == at.0)
            .ok_or_else(|| fail("owning callable temporary has no actual block"))?;
        for (index, instruction) in block.instructions.iter().enumerate().take(at.1).rev() {
            b.spend(1, WORK)?;
            if let InstructionKind::Store {
                local: target,
                value,
            } = instruction.kind
                && target == temporary
            {
                operations::available_before(
                    function,
                    (block.id, index, SemanticSiteEdge::Before),
                    value,
                    b,
                )?;
                self.owned_place_value(
                    instance,
                    function,
                    value,
                    local,
                    source,
                    active,
                    b,
                    depth + 1,
                )?;
                b.lookup(active.len(), WORK)?;
                active.remove(&key);
                return Ok(());
            }
            let interferes = match crate::incremental::Operation::of(&instruction.kind) {
                crate::incremental::Operation::Read(target)
                | crate::incremental::Operation::Store(target)
                | crate::incremental::Operation::Take(target)
                | crate::incremental::Operation::Kill(target) => target == temporary,
                crate::incremental::Operation::Unpack { source, fields } => {
                    b.spend(fields.len(), WORK)?;
                    source == temporary || fields.contains(&temporary)
                }
                crate::incremental::Operation::Other => false,
            };
            if interferes {
                return Err(fail(
                    "owning callable temporary crosses an access or vacant activation",
                ));
            }
        }
        if block.id == function.entry {
            return Err(fail(
                "owning callable temporary has no actual consuming store origin",
            ));
        }
        let initialized = crate::shared_initialization(function, b)?;
        let mut incoming = false;
        for predecessor in &initialized.predecessors[block.id.0 as usize] {
            b.spend(1, WORK)?;
            if !initialized.reachable[*predecessor] {
                continue;
            }
            incoming = true;
            let previous = &function.blocks[*predecessor];
            self.owned_place_local(
                instance,
                function,
                temporary,
                (previous.id, previous.instructions.len()),
                local,
                source,
                active,
                b,
                depth + 1,
            )?;
        }
        if !incoming {
            return Err(fail("owning callable temporary has no reachable source"));
        }
        b.lookup(active.len(), WORK)?;
        active.remove(&key);
        Ok(())
    }

    pub(super) fn borrow_field_type<'a>(
        &self,
        mut ty: &'a Type,
        path: &[u32],
        b: &mut Budget,
    ) -> CompileResult<&'a Type> {
        for index in path {
            b.spend(1, WORK)?;
            if let Type::BorrowedStorage { storage, .. } = ty {
                ty = storage;
            }
            ty = match ty {
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    &fields
                        .get(*index as usize)
                        .ok_or_else(|| {
                            fail("actual borrowed field exceeds complete nominal arity")
                        })?
                        .1
                }
                Type::Tuple(fields) => fields
                    .get(*index as usize)
                    .ok_or_else(|| fail("actual borrowed field exceeds complete tuple arity"))?,
                _ => {
                    return Err(fail(
                        "actual borrowed field crosses a pointer, owner or opaque boundary",
                    ));
                }
            };
        }
        Ok(ty)
    }

    fn projected_borrow_origin(
        &self,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Origin> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual projected borrow exceeds its finite ancestry",
            ));
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        let Some(Type::Address {
            mutable: output_mutable,
            inner: output_inner,
        }) = function.values.get(id.0 as usize)
        else {
            return Err(fail(
                "actual projected borrow has no complete address result",
            ));
        };
        match &instruction.kind {
            InstructionKind::BorrowOwnedArrayElement {
                local,
                index: 0,
                mutable,
            } => {
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .filter(|entry| entry.id == *local)
                    .ok_or_else(|| fail("actual callback borrow lacks its exact element owner"))?;
                if *mutable != *output_mutable
                    || !matches!(&actual.ty, Type::Array { length: 1, element }
                        if element == output_inner && matches!(element.as_ref(), Type::Closure(_)))
                {
                    return Err(fail(
                        "actual callback element borrow changes its complete type or mode",
                    ));
                }
                Ok(Origin {
                    root: Root::Slot(*local, block, index),
                    path: Vec::new(),
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                let mutable = matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. });
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| fail("actual borrow root local is absent"))?;
                if mutable != *output_mutable || output_inner.as_ref() != &actual.ty {
                    return Err(fail("actual borrowed root changes its full type or mode"));
                }
                Ok(Origin {
                    root: Root::Local(*local, block, index, false),
                    path: Vec::new(),
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::BorrowPackageField {
                local,
                index: field,
                mutable,
            } => {
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .filter(|declaration| declaration.id == *local)
                    .ok_or_else(|| fail("actual package field borrow has no local declaration"))?;
                if !matches!(actual.ty, Type::Package { .. })
                    || *mutable != *output_mutable
                    || self.borrow_field_type(&actual.ty, &[*field], b)? != output_inner.as_ref()
                {
                    return Err(fail(
                        "actual package field borrow changes its complete leaf or mode",
                    ));
                }
                b.spend(1, WORK)?;
                Ok(Origin {
                    root: Root::Local(*local, block, index, false),
                    path: vec![*field],
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::BorrowPlaceLocal {
                local,
                path,
                mutable,
            } => {
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| fail("actual projected root local is absent"))?;
                let mut fields = Vec::new();
                for step in path {
                    let PlaceProjection::Field(field) = step else {
                        return Err(unknown(
                            "original field borrow requires its exact active variant projection",
                        ));
                    };
                    self.push_value(&mut fields, *field, b)?;
                }
                if *mutable != *output_mutable
                    || self.borrow_field_type(&actual.ty, &fields, b)? != output_inner.as_ref()
                {
                    return Err(fail(
                        "actual projected root changes its full leaf type or mode",
                    ));
                }
                Ok(Origin {
                    root: Root::Local(*local, block, index, false),
                    path: fields,
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                let actual = function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| fail("actual borrowed address local is absent"))?;
                if function.values.get(id.0 as usize) != Some(&actual.ty) {
                    return Err(fail("actual borrowed address read changes its full type"));
                }
                Ok(Origin {
                    root: Root::Local(*local, block, index, true),
                    path: Vec::new(),
                    index: None,
                    fresh: false,
                })
            }
            InstructionKind::BorrowCapture { index, mutable } => {
                if *mutable != *output_mutable {
                    return Err(fail("actual capture borrow changes its mode"));
                }
                Ok(Origin {
                    root: Root::Capture(*index, false),
                    path: Vec::new(),
                    index: None,
                    fresh: true,
                })
            }
            InstructionKind::ReadCapture { index } => Ok(Origin {
                root: Root::Capture(*index, true),
                path: Vec::new(),
                index: None,
                fresh: false,
            }),
            InstructionKind::ReborrowAddress { address, mutable } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                let Some(Type::Address {
                    mutable: parent,
                    inner,
                }) = function.values.get(address.0 as usize)
                else {
                    return Err(fail("actual projected reborrow has no parent address"));
                };
                if *mutable != *output_mutable || *mutable && !*parent || inner != output_inner {
                    return Err(fail(
                        "actual projected reborrow strengthens a mode or changes its complete pointee",
                    ));
                }
                let mut origin = self.projected_borrow_origin(function, *address, b, depth + 1)?;
                origin.fresh = true;
                Ok(origin)
            }
            InstructionKind::AddressIndex {
                address,
                index: offset,
            } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *offset,
                    b,
                )?;
                if !function
                    .values
                    .get(offset.0 as usize)
                    .is_some_and(Type::is_integer)
                {
                    let mut errors =
                        fail("actual indexed borrow has another checked index carrier");
                    errors[0].notes.push(format!(
                        "function {} SSA {} index {}: {:?}",
                        function.id.0,
                        id.0,
                        offset.0,
                        function.values.get(offset.0 as usize)
                    ));
                    return Err(errors);
                }
                let Some(Type::Address { mutable, inner }) =
                    function.values.get(address.0 as usize)
                else {
                    return Err(fail("actual indexed borrow has no backing reference"));
                };
                let element = match inner.as_ref() {
                    Type::Array { element, .. } | Type::Slice { element } => element,
                    _ => {
                        return Err(fail(
                            "actual indexed borrow crosses another backing category",
                        ));
                    }
                };
                if mutable != output_mutable || element != output_inner {
                    return Err(fail(
                        "actual indexed borrow changes its element type or access mode",
                    ));
                }
                let mut origin = self.projected_borrow_origin(function, *address, b, depth + 1)?;
                if origin.index.replace(*offset).is_some() {
                    return Err(unknown(
                        "actual indexed borrow needs its complete multidimensional source",
                    ));
                }
                origin.fresh = true;
                Ok(origin)
            }
            InstructionKind::AddressField {
                address,
                index: field,
            } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                let Some(Type::Address { mutable, inner }) =
                    function.values.get(address.0 as usize)
                else {
                    return Err(fail("actual field borrow has no complete parent address"));
                };
                if mutable != output_mutable
                    || self.borrow_field_type(inner, &[*field], b)? != output_inner.as_ref()
                {
                    return Err(fail(
                        "actual field borrow changes its exact leaf type or mode",
                    ));
                }
                let mut origin = self.projected_borrow_origin(function, *address, b, depth + 1)?;
                if origin.index.is_some() {
                    return Err(unknown(
                        "actual field borrow needs its complete post-index source mapping",
                    ));
                }
                self.push_value(&mut origin.path, *field, b)?;
                origin.fresh = true;
                Ok(origin)
            }
            _ => Ok(Origin {
                root: Root::Expression(id),
                path: Vec::new(),
                index: None,
                fresh: false,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn checked_borrow_place(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        source: &original::borrow_places::Source,
        address_input: bool,
        outer_fresh: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        let mut origin = self.projected_borrow_origin(function, id, b, depth + 1)?;
        if let Some(anchor) = source.anchor
            && !matches!(anchor, original::LocalAnchor::Environment(_))
            && !source.indirect
            && source.path.is_empty()
        {
            let local = self.anchored_local(instance, function, anchor, b)?;
            if let Some(slot) = self.persistent_slot(instance, function, local, b)? {
                if !self.slot_operand_origin(instance, function, id, &slot, true, b)? {
                    return Err(fail(
                        "actual callback borrow selects another original element source",
                    ));
                }
                // The real borrow/temporary graph is proved above. The
                // independently checked class invariant holds at this exact
                // consuming point; physical loans and epochs remain actual.
                let (block, index, _) = self.defining(function, id, b)?;
                origin = Origin {
                    root: Root::Slot(local, block, index),
                    path: Vec::new(),
                    index: None,
                    fresh: true,
                };
            }
        }
        if let Root::Local(local, block, index, true) = &origin.root
            && matches!(source.anchor, Some(original::LocalAnchor::Environment(_)))
            && !source.indirect
        {
            // A borrowed callable capture is spilled as Address<whole F>.
            // Loading this compiler slot preserves the actual BorrowCapture;
            // it is not a read of a pointer stored inside lexical F. The
            // finite capture-origin proof checks every initialized CFG source.
            let capture =
                self.capture_local_origin(function, *local, (*block, *index), b, depth + 1)?;
            let closure = function
                .environment
                .and_then(|id| self.module.closures.get(id.0 as usize))
                .filter(|closure| closure.function == function.id)
                .ok_or_else(|| fail("borrowed capture spill has another actual environment"))?;
            let captured = closure.captures.get(capture as usize).ok_or_else(|| {
                fail("borrowed capture spill leaves its actual capture directory")
            })?;
            let local_type = &function
                .locals
                .get(local.0 as usize)
                .filter(|item| item.id == *local)
                .ok_or_else(|| fail("borrowed capture spill has no actual local"))?
                .ty;
            if !geometry::same_type(captured, &source.root.ty, b)?
                || !matches!(local_type, Type::Address { inner, .. }
                    if inner.as_ref() == captured)
            {
                return Err(fail(
                    "borrowed capture spill changes its complete lexical value or physical pointer boundary",
                ));
            }
            // Normalize only the canonical capture-borrow spill. A copied
            // value subsequently borrowed from another local is another
            // physical root even when its capture-origin image is the same.
            let mut stores = 0usize;
            for actual_block in &function.blocks {
                for item in &actual_block.instructions {
                    b.spend(1, WORK)?;
                    match &item.kind {
                        InstructionKind::Store {
                            local: target,
                            value,
                        } if target == local => {
                            stores = stores.saturating_add(1);
                            let (_, _, producer) = self.defining(function, *value, b)?;
                            if actual_block.id != function.entry
                                || !matches!(producer.kind, InstructionKind::BorrowCapture { index, .. }
                                    if index == capture)
                            {
                                return Err(fail(
                                    "borrowed capture spill is not its genuine entry capture borrow",
                                ));
                            }
                        }
                        kind if matches!(crate::incremental::Operation::of(kind),
                            crate::incremental::Operation::Store(target) if target == *local) =>
                        {
                            return Err(fail(
                                "borrowed capture spill has another actual defining write",
                            ));
                        }
                        _ => {}
                    }
                }
            }
            if stores != 1 {
                return Err(fail(
                    "borrowed capture spill has no unique genuine capture borrow",
                ));
            }
            origin.root = Root::Capture(capture, false);
            origin.fresh = true;
        }
        b.spend(origin.path.len().saturating_add(source.path.len()), WORK)?;
        if origin.path != source.path || !(origin.fresh || outer_fresh) {
            return Err(fail(
                "actual borrow changes its original physical field path or omits its fresh borrow",
            ));
        }
        let (root, indirect) = match origin.root {
            Root::Slot(local, block, index) => {
                let anchor = source.anchor.ok_or_else(|| {
                    fail("actual callback element replaces an original lexical place")
                })?;
                if self.anchored_local(instance, function, anchor, b)? != local {
                    return Err(fail(
                        "actual callback element has another original local anchor",
                    ));
                }
                let slot = self
                    .persistent_slot(instance, function, local, b)?
                    .ok_or_else(|| {
                        fail("actual callback element lacks its exact original representation")
                    })?;
                (
                    self.persistent_slot_value(
                        instance,
                        function,
                        &slot,
                        (block, index),
                        b,
                        depth + 1,
                    )?,
                    false,
                )
            }
            Root::Local(local, block, index, indirect) => {
                if self.anchored_local(
                    instance,
                    function,
                    source.anchor.ok_or_else(|| {
                        fail("actual local borrow replaces an original emitted reference")
                    })?,
                    b,
                )? != local
                {
                    return Err(fail("actual field borrow selects another original root"));
                }
                (
                    self.local_value(instance, function, local, (block, index), b, depth + 1)?,
                    indirect,
                )
            }
            Root::Capture(capture, indirect) => {
                let Some(original::LocalAnchor::Environment(ordinal)) = source.anchor else {
                    return Err(fail(
                        "actual field borrow replaces a lexical root by a capture",
                    ));
                };
                if self.environment_capture(instance, function, ordinal, b)? != capture {
                    return Err(fail("actual field borrow selects another captured root"));
                }
                (
                    self.capture_whole_value(instance, function, capture, b, depth + 1)?,
                    indirect,
                )
            }
            Root::Expression(actual) => {
                let node = source.expression.ok_or_else(|| {
                    fail("actual returned reference replaces an original lexical place")
                })?;
                if self.original_subject_value(instance, function, node, b)? != Some(actual) {
                    return Err(fail(
                        "actual indexed borrow selects another original returned reference",
                    ));
                }
                (
                    self.ssa_value(instance, function, actual, b, depth + 1)?,
                    true,
                )
            }
        };
        if !self.capture_values_equal(&root, &source.root, b)?
            || indirect != (source.indirect || address_input)
        {
            return Err(fail(
                "actual borrowed root changes its original whole classes or pointer boundary",
            ));
        }
        let whole = if indirect {
            let Type::Address { inner, .. } = &root.ty else {
                return Err(fail(
                    "actual borrowed root loses its original reference head",
                ));
            };
            inner.as_ref()
        } else {
            &root.ty
        };
        let mut leaf = self.borrow_field_type(whole, &origin.path, b)?;
        match (source.index, origin.index) {
            (None, None) => {}
            (Some(node), Some(actual)) => {
                let observed = self.original_subject_value(instance, function, node, b)?;
                if observed != Some(actual) {
                    let definition = self.inventory.definition(instance.definition, b)?;
                    let original = self.inventory.original_index(definition.id, b)?;
                    let expression = original.get_expr(node, b)?;
                    let (_, _, operation) = self.defining(function, actual, b)?;
                    // A folded integer literal need not have a separate
                    // runtime observation. Prove its exact original spelling,
                    // carrier and value against the actual Constant instead.
                    let literal = match (&expression.kind, &operation.kind) {
                        (CoreExprKind::Integer(text), InstructionKind::Constant(value))
                            if observed.is_none() && value.ty().is_integer() =>
                        {
                            let expected = self.recipe_value(
                                instance,
                                &SemanticTypeRecipe::OriginalType {
                                    scope: SemanticScopeRef::Ancestor(0),
                                    source: SemanticOriginalTypeSource::Expression(node),
                                },
                                b,
                                0,
                            )?;
                            expected.ty == value.ty()
                                && original::mixed::integer_constant(text, &expected.ty, b)?
                                    == *value
                        }
                        _ => false,
                    };
                    if !literal {
                        let mut errors = unknown(
                            "actual indexed borrow lacks its exact original emitted index operand",
                        );
                        errors[0].notes.push(format!("original index node {}, actual SSA {}, observed {:?}, actual operation {:?}", node.0, actual.0, observed, operation.kind));
                        return Err(errors);
                    }
                }
                leaf = match leaf {
                    Type::Array { element, .. } | Type::Slice { element } => element.as_ref(),
                    _ => {
                        return Err(fail(
                            "original indexed borrow loses its complete backing element",
                        ));
                    }
                };
            }
            _ => {
                return Err(fail(
                    "actual indexed borrow adds or omits an original index projection",
                ));
            }
        }
        let required = if address_input {
            let Type::Address { inner, .. } = &source.inner.ty else {
                return Err(fail(
                    "original range backing loses its complete reference carrier",
                ));
            };
            inner.as_ref()
        } else {
            &source.inner.ty
        };
        let Some(Type::Address { inner, .. }) = function.values.get(id.0 as usize) else {
            return Err(fail("actual borrowed place loses its result reference"));
        };
        if leaf != required || inner.as_ref() != required {
            return Err(fail(
                "actual borrowed place changes its complete original field type",
            ));
        }
        Ok(())
    }
}
