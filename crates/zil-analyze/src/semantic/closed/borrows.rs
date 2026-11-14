//! Source borrow occurrences and compiler reborrows transport semantic classes
//! through their real local/SSA chain. Loans, parents and epochs remain actual
//! memory-kernel obligations; no class here creates a permission or target.
use super::*;
use crate::semantic::value::{self, Value};

impl Check<'_, '_> {
    pub(super) fn anchored_local(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        anchor: original::LocalAnchor,
        b: &mut Budget,
    ) -> CompileResult<LocalId> {
        match anchor {
            original::LocalAnchor::Parameter(index) => function
                .parameters
                .get(index as usize)
                .copied()
                .ok_or_else(|| fail("original parameter anchor has no actual local")),
            original::LocalAnchor::Binding(statement) => {
                let definition = self.inventory.definition(instance.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    site_nodes, sites, ..
                } = &definition.body
                else {
                    return Err(fail("original local anchor has no complete body"));
                };
                let mut selected = None;
                for link in site_nodes {
                    b.spend(sites.len().saturating_add(1), WORK)?;
                    if link.node != SemanticOriginalNode::Statement(statement) {
                        continue;
                    }
                    if !sites.iter().any(|site| {
                        site.id == link.site
                            && matches!(site.kind, SemanticSiteRecipeKind::DeclareLocal { .. })
                    }) {
                        continue;
                    }
                    for site in &self.program.sites {
                        b.spend(1, WORK)?;
                        if site.instance != instance.id || site.recipe != link.site {
                            continue;
                        }
                        let SemanticSiteAction::DeclareLocal {
                            local: SemanticLocalTarget::Typed(local),
                            ..
                        } = site.action
                        else {
                            return Err(fail(
                                "original local anchor changes its actual declaration operation",
                            ));
                        };
                        if selected.replace(local).is_some() {
                            return Err(fail("original local anchor has competing actual slots"));
                        }
                    }
                }
                selected.ok_or_else(|| fail("original local anchor has no actual declared slot"))
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
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => Ok(local),
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
        for site in &self.program.sites {
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
            if self.function_instance(callee, b)?.definition != target {
                return Err(fail(
                    "actual call role changes its original target definition",
                ));
            }
            if selected.replace(values).is_some() {
                return Err(fail("actual call has competing original input roles"));
            }
        }
        b.spend(function.blocks.len(), WORK)?;
        selected.ok_or_else(|| unknown("actual call has no complete original input role anchor"))
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
        if !matches!(actual.ty, Type::Address { .. }) {
            return Ok(actual == expected);
        }
        if value::pointee(actual, self.inventory, b)?
            != value::pointee(expected, self.inventory, b)?
        {
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
        if actual_class == expected_class {
            return Ok(true);
        }
        let value::Class::Reborrow(owner, value) = actual_class else {
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
            let Type::Address { inner, .. } = &parent.ty else {
                unreachable!()
            };
            crate::dyn_metadata::ty(inner, b)?;
            parent.ty = Type::Address {
                mutable: false,
                inner: inner.clone(),
            };
            if parent.life.slots.len() != 1 {
                return Ok(false);
            }
            parent.life.slots[0].mode = StoredBorrowMode::SharedAddress;
        }
        self.outer_role(instance, function, &parent, expected, b, depth + 1)
    }

    fn borrow_root(
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
        for site in &self.program.sites {
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
            let Some((anchor, inner, slice)) = self.original.borrowed_source(
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
            let Type::Address { mutable, .. } = expected.ty else {
                return Err(fail("original borrow does not produce a reference"));
            };
            if let Some(slice) = slice {
                let value = self.source_slice_borrow(
                    instance,
                    function,
                    id,
                    anchor,
                    &inner,
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
            let local = self.anchored_local(instance, function, anchor, b)?;
            let (block, index, actual, actual_mutable) =
                self.borrow_root(function, id, b, depth + 1)?;
            if actual != local || mutable != actual_mutable {
                return Err(fail(
                    "actual borrow selects another original source slot or mode",
                ));
            }
            let occupant =
                self.local_value(instance, function, local, (block, index), b, depth + 1)?;
            if occupant != inner {
                return Err(fail(
                    "actual borrow changes its original pointee classes or type",
                ));
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
