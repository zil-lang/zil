//! Source borrow occurrences and compiler reborrows transport semantic classes
//! through their real local/SSA chain. Loans, parents and epochs remain actual
//! memory-kernel obligations; no class here creates a permission or target.
use super::*;
use crate::semantic::value::{self, Value};

impl Check<'_, '_> {
    pub(super) fn checked_view_role(
        &self,
        root: &Value,
        output: &Value,
        role: &(value::Class, Option<value::Class>, value::Class, Option<original::LocalAnchor>),
        anchor: Option<original::LocalAnchor>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(4, WORK)?;
        let [slot] = output.life.slots.as_slice() else {
            return Err(fail("actual view role loses its exact outer occurrence"));
        };
        if !slot.path.is_empty() || output.classes.get(slot.binder as usize) != Some(&role.2) {
            return Err(fail("actual view changes its independently selected original lifetime role"));
        }
        match &role.1 {
            None if role.0 != role.2 => {
                if !matches!(role.3, Some(original::LocalAnchor::Binding(_)))
                    || role.3 != anchor
                    || !matches!((&role.0, &role.2), (value::Class::Implicit(a, _), value::Class::Implicit(z, _)) if a == z)
                {
                    return Err(fail("actual contextual view lacks its exact original local region or projected input upper bound"));
                }
                // The following physical-place check must independently bind
                // this exact original local to its initialized actual root.
                // A common region grants neither target Eq nor a parent loan.
            }
            Some(upper) => {
                if !matches!(root.ty, Type::Address { .. }) {
                    return Err(fail("actual view upper has no genuine original Address source"));
                }
                let [head] = root.life.slots.as_slice() else {
                    return Err(fail("actual projected source loses its complete outer head"));
                };
                if !head.path.is_empty() || root.classes.get(head.binder as usize) != Some(upper) {
                    return Err(fail("actual view upper changes its exact original source head class"));
                }
            }
            None => {}
        }
        // This proves only the original role relation. The caller still
        // checks this actual operation's complete physical source and loans.
        Ok(())
    }
    pub(super) fn anchored_local(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        anchor: original::LocalAnchor,
        b: &mut Budget,
    ) -> CompileResult<LocalId> {
        self.anchored_local_at(instance, function, anchor, b, 0)
    }

    pub(super) fn anchored_local_at(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        anchor: original::LocalAnchor,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<LocalId> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual pattern head ancestry exceeds its finite depth",
            ));
        }
        match anchor {
            original::LocalAnchor::Pattern(index) => {
                self.pattern_local(instance, function, index, b, depth + 1)
            }
            original::LocalAnchor::Parameter(index) => function
                .parameters
                .get(index as usize)
                .copied()
                .ok_or_else(|| fail("original parameter anchor has no actual local")),
            original::LocalAnchor::Environment(ordinal) => {
                let capture = self.environment_capture(instance, function, ordinal, b)?;
                let mut selected = None;
                for block in &function.blocks {
                    for instruction in &block.instructions {
                        b.spend(1, WORK)?;
                        if let InstructionKind::BindPackageCapture { local, index } =
                            instruction.kind
                            && index == capture
                            && selected.replace(local).is_some()
                        {
                            return Err(fail(
                                "original captured Package has several actual bound lexical heads",
                            ));
                        }
                    }
                }
                selected.ok_or_else(|| {
                    unknown("original environment place needs its actual bound owning head")
                })
            }
            original::LocalAnchor::Binding(statement) => {
                let index = self.inventory.original_index(instance.definition, b)?;
                let recipe = index.declaration_site(statement, b)?;
                let site = self.inventory.emitted_site(self.program, instance.id, recipe, b)?;
                let SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local), ..
                } = site.action else {
                    return Err(fail("original local anchor changes its actual declaration operation"));
                };
                Ok(local)
            }
        }
    }

    pub(super) fn receiver_local(
        &self,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<LocalId> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("actual receiver chain exceeds its finite depth"));
        }
        let (_, _, instruction) = self.defining(function, id, b)?;
        match instruction.kind {
            InstructionKind::Load(local)
            | InstructionKind::TakeLocal { local }
            | InstructionKind::BorrowLocal { local }
            | InstructionKind::BorrowLocalMut { local } => Ok(local),
            InstructionKind::ReborrowAddress { address, .. } => {
                self.receiver_local(function, address, b, depth + 1)
            }
            _ => Err(unknown(
                "actual abstract receiver has no exact original local read chain",
            )),
        }
    }

    pub(super) fn static_call_values(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        callee: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<Vec<Value>> {
        let mut selected = None;
        for site in self.inventory.sites_at(
            self.program, instance.id, block, index, SemanticSiteEdge::After, b,
        )? {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After } if actual == block && instruction as usize == index)
                || !matches!(
                    site.action,
                    SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. }
                )
            {
                continue;
            }
            let (target, values) = self.original.call_inputs(
                instance,
                site.recipe,
                self.program,
                self.inventory,
                b,
            )?;
            let worlds = self.function_worlds(callee, b)?;
            if worlds.is_empty() { return Err(fail("actual call role has no complete target world")); }
            for world in worlds {
                if world.definition != target {
                    return Err(fail("actual call role changes its original target definition"));
                }
            }
            if selected.replace(values).is_some() {
                return Err(fail("actual call has competing original input roles"));
            }
        }
        b.spend(function.blocks.len(), WORK)?;
        selected.ok_or_else(|| unknown("actual call has no complete original input role anchor"))
    }

    pub(super) fn static_call_result(&self, instance: &SemanticInstance, block: BlockId, index: usize,
        b: &mut Budget) -> CompileResult<Value> {
        let mut selected = None;
        for site in self.inventory.sites_at(
            self.program, instance.id, block, index, SemanticSiteEdge::After, b,
        )? {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After }
                    if actual == block && instruction as usize == index)
                || !matches!(site.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. })
            { continue; }
            let value = self.original.call_output(instance, site.recipe, self.program, self.inventory, b)?;
            if selected.replace(value).is_some() { return Err(fail("call output has competing original receipts")); }
        }
        selected.ok_or_else(|| fail("call output has no exact original receipt"))
    }

    // Independent physical header slots may denote the same declared
    // original lifetime. Prove that equation in every actual world, rather
    // than requiring equal slot numbers or equating their runtime loans.
    pub(super) fn call_result_input_binders(&self, callee: &TypedFunction,
        result: StorageBinder, b: &mut Budget) -> CompileResult<Vec<StorageBinder>> {
        let contract = callee.storage_life.as_ref()
            .ok_or_else(|| fail("call class source has no complete actual header"))?;
        let mut candidates = Vec::new();
        for (base, binder) in contract.bindings.iter().map(|row| (row.position.base, row.binder))
            .chain(contract.classes.iter().map(|row| (row.position.base, row.binder)))
        {
            b.spend(candidates.len().saturating_add(1), WORK)?;
            if matches!(base, StorageBase::Parameter(_)) && !candidates.contains(&binder) {
                self.push_value(&mut candidates, binder, b)?;
            }
        }
        b.spend(candidates.len(), WORK)?;
        if candidates.contains(&result) {
            let mut exact = Vec::new();
            self.push_value(&mut exact, result, b)?;
            return Ok(exact);
        }
        if candidates.is_empty() { return Ok(Vec::new()); }
        let worlds = self.function_worlds(callee.id, b)?;
        if worlds.is_empty() { return Err(fail("call result class has no original callee world")); }
        for world in worlds {
            let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                &self.inventory.definition(world.definition, b)?.body else {
                    return Err(fail("call class equation omits its complete original declaration"));
                };
            // This checks all source parameters, full result domain and exact
            // embeddings against the independently checked original recipes.
            self.header_relation(world, callee, signature, b)?;
            let original = self.header_class(world, result, b)?;
            let mut proven = Vec::new();
            for candidate in candidates {
                b.spend(1, WORK)?;
                if self.header_class(world, candidate, b)? == original {
                    self.push_value(&mut proven, candidate, b)?;
                }
            }
            candidates = proven;
        }
        Ok(candidates)
    }

    pub(super) fn outer_role(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        actual: &Value,
        expected: &Value,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("outer call role ancestry exceeds its finite depth"));
        }
        if actual.ty != expected.ty {
            return Ok(false);
        }
        crate::dyn_metadata::shape(&actual.life, b)?;
        crate::dyn_metadata::shape(&expected.life, b)?;
        b.spend(
            actual.classes.len().saturating_add(expected.classes.len()),
            WORK,
        )?;
        if self.capture_values_equal(actual, expected, b)? {
            return Ok(true);
        }
        if !matches!(actual.ty, Type::Address { .. }) {
            return Ok(false);
        }
        if original::strings::is_byte_type(&actual.ty)
            && empty(&actual.life)
            && actual.classes.is_empty()
            && expected.life.binder_count == 1
            && expected.life.retained.is_empty()
            && expected.life.nominal_arguments.is_empty()
            && matches!(expected.life.slots.as_slice(),[slot] if slot.path.is_empty() && slot.binder==0 && slot.mode==StoredBorrowMode::SharedAddress)
        {
            return Ok(true);
        }
        if !self.capture_values_equal(
            &value::pointee(actual, self.inventory, b)?,
            &value::pointee(expected, self.inventory, b)?,
            b,
        )? {
            return Ok(false);
        }
        if actual.life.slots.len() != 1
            || expected.life.slots.len() != 1
            || !actual.life.slots[0].path.is_empty()
            || !expected.life.slots[0].path.is_empty()
            || actual.life.slots[0].mode != expected.life.slots[0].mode
        {
            return Ok(false);
        }
        let actual_class = actual
            .classes
            .get(actual.life.slots[0].binder as usize)
            .ok_or_else(|| fail("actual outer role has no whole class"))?;
        let expected_class = expected
            .classes
            .get(expected.life.slots[0].binder as usize)
            .ok_or_else(|| fail("required outer role has no whole class"))?;
        let actual_class = self.capture_class_image(actual_class.clone(), b)?;
        let expected_class = self.capture_class_image(expected_class.clone(), b)?;
        if actual_class == expected_class {
            return Ok(true);
        }
        let value::Class::Reborrow(owner, value) = &actual_class else {
            return Ok(false);
        };
        if *owner != function.id {
            return Ok(false);
        }
        let (block, index, instruction) = self.defining(function, *value, b)?;
        let InstructionKind::ReborrowAddress { address, .. } = instruction.kind else {
            return Err(fail("outer role child does not name a real reborrow"));
        };
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            address,
            b,
        )?;
        let mut parent = self.ssa_value(instance, function, address, b, depth + 1)?;
        // Shared adaptation changes only the requested outer mode. It never
        // retags the child's classes or modifies an embedded intrinsic domain.
        if let (Type::Address { mutable: false, .. }, Type::Address { mutable: true, .. }) =
            (&expected.ty, &parent.ty)
        {
            let geometry = parent.geometry_mut(b)?;
            let Type::Address { inner, .. } = &geometry.ty else {
                unreachable!()
            };
            crate::dyn_metadata::ty(inner, b)?;
            geometry.ty = Type::Address {
                mutable: false,
                inner: inner.clone(),
            };
            if geometry.life.slots.len() != 1 {
                return Ok(false);
            }
            geometry.life.slots[0].mode = StoredBorrowMode::SharedAddress;
        }
        let proven = self.outer_role(instance, function, &parent, expected, b, depth + 1)?;
        if proven {
            b.lookup(b.semantic_reborrow_regions.len(), WORK)?;
            b.semantic_reborrow_regions.insert((function.id, *value));
        }
        Ok(proven)
    }

    pub(super) fn borrow_root(
        &self,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(BlockId, usize, LocalId, bool)> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("actual borrow chain exceeds its finite depth"));
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        match instruction.kind {
            InstructionKind::BorrowLocal { local } => Ok((block, index, local, false)),
            InstructionKind::BorrowLocalMut { local } => Ok((block, index, local, true)),
            InstructionKind::ReborrowAddress { address, mutable } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    address,
                    b,
                )?;
                let Some(Type::Address {
                    mutable: parent,
                    inner,
                }) = function.values.get(address.0 as usize)
                else {
                    return Err(fail("actual reborrow has no complete parent address"));
                };
                if mutable && !parent {
                    return Err(fail("actual reborrow strengthens a shared parent"));
                }
                crate::dyn_metadata::ty(inner, b)?;
                if function.values.get(id.0 as usize)
                    != Some(&Type::Address {
                        mutable,
                        inner: inner.clone(),
                    })
                {
                    return Err(fail("actual reborrow changes its complete pointee type"));
                }
                crate::dyn_metadata::ty(inner, b)?;
                self.borrow_root(function, address, b, depth + 1)
            }
            _ => Err(unknown(
                "original borrow needs its exact actual local-borrow root",
            )),
        }
    }

    pub(super) fn source_borrow(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Value>> {
        let definition = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { sites, .. } = &definition.body else {
            return Err(fail("actual borrow omits its complete original definition"));
        };
        let mut selected = None;
        for site in self.inventory.sites_for_value(self.program, instance.id, id, b)? {
            b.spend(1, WORK)?;
            if site.instance != instance.id {
                continue;
            }
            let SemanticSiteAction::TypeUse {
                subject: SemanticOperand::Value(value),
                expected: proposed,
            } = &site.action
            else {
                continue;
            };
            if *value != id {
                continue;
            }
            let Some(source) = self.original.borrowed_source(
                instance,
                site.recipe,
                self.program,
                self.inventory,
                b,
            )?
            else {
                continue;
            };
            b.spend(sites.len(), WORK)?;
            let recipe = sites
                .iter()
                .find(|recipe| recipe.id == site.recipe)
                .ok_or_else(|| fail("actual borrow has no original recipe"))?;
            let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
                return Err(fail("actual borrow changes its original role"));
            };
            let expected = self.recipe_value(instance, expected, b, depth + 1)?;
            if value::resolved(instance, proposed, self.inventory, b, depth + 1)? != expected
                || function.values.get(id.0 as usize) != Some(&expected.ty)
            {
                return Err(fail(
                    "actual borrow changes its complete original occurrence value",
                ));
            }
            let role = source.role.as_ref().ok_or_else(|| fail("actual source borrow omits its independently checked occurrence role"))?;
            self.checked_view_role(&source.root, &expected, role, source.anchor, b)?;
            let Type::Address { mutable, .. } = expected.ty else {
                return Err(fail("original borrow does not produce a reference"));
            };
            if let Some(slice) = source.slice {
                let value = self.source_slice_borrow(
                    instance,
                    function,
                    id,
                    &source,
                    slice,
                    &expected,
                    b,
                    depth + 1,
                )?;
                if selected.as_ref().is_some_and(|old| old != &value) {
                    return Err(fail(
                        "actual range borrow has competing original occurrence values",
                    ));
                }
                selected = Some(value);
                continue;
            }
            self.checked_borrow_place(instance, function, id, &source, false, false, b, depth + 1)?;
            let Some(Type::Address {
                mutable: actual_mutable,
                ..
            }) = function.values.get(id.0 as usize)
            else {
                return Err(fail("actual borrowed place has no full result reference"));
            };
            if mutable != *actual_mutable {
                return Err(fail("actual borrow changes its original result mode"));
            }
            if selected.as_ref().is_some_and(|old| old != &expected) {
                return Err(fail(
                    "actual borrow has competing source occurrence relations",
                ));
            }
            selected = Some(expected);
        }
        Ok(selected)
    }
}
