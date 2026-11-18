//! The implicit class of an actual borrowed scalar capture. Its original
//! occurrence, active entry, physical capture and creator operand are all
//! checked; the class never supplies a target, parent loan or storage epoch.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Head {
    Local(FunctionId, LocalId),
    Capture(FunctionId, u32),
}
pub(super) type Heads = BTreeSet<Head>;
type ActiveHeads = BTreeSet<(FunctionId, ValueId, Vec<PlaceProjection>, Option<u32>, bool)>;

// An opaque child's Fixed class may have several original occurrences. Its
// class equality theorem is not an actual storage/head selection interface.
#[derive(PartialEq, Eq)]
pub(super) enum CaptureDirectory {
    Address(Vec<u32>),
    Ghost(Vec<BTreeSet<(u32, u32)>>),
}
impl CaptureDirectory {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Address(rows) => rows.len(),
            Self::Ghost(rows) => rows.len(),
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn charge_rows(&self, b: &mut Budget) -> CompileResult<()> {
        b.spend(self.len().saturating_add(1), WORK)?;
        if let Self::Ghost(rows) = self {
            for row in rows {
                b.spend(row.len().saturating_add(1), WORK)?;
            }
        }
        Ok(())
    }

    fn copy_paid(&self, b: &mut Budget) -> CompileResult<Self> {
        self.charge_rows(b)?;
        Ok(match self {
            Self::Address(rows) => Self::Address(rows.clone()),
            Self::Ghost(rows) => Self::Ghost(rows.clone()),
        })
    }
    fn address(self) -> CompileResult<Vec<u32>> {
        match self {
            Self::Address(rows) => Ok(rows),
            Self::Ghost(_) => Err(unknown(
                "opaque class needs its exact child/capture source selector, not an equal class image",
            )),
        }
    }
}

impl Check<'_, '_> {
    pub(super) fn creator_heads(
        &self,
        instance: &SemanticInstance,
        closure: ClosureId,
        captures: &[ValueId],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if self.has_ghost_capture(closure, b)? {
            return self.ghost_creator(instance, closure, captures, b).map(|_| ());
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail(
                "implicit captured head has no actual creator function",
            ));
        };
        let function = self.inventory.function(target, b)?;
        for index in self.capture_directory(closure, b)?.address()? {
            let value = captures
                .get(index as usize)
                .ok_or_else(|| fail("actual creator omits its implicit captured head"))?;
            self.heads(function, *value, None, &mut BTreeSet::new(), 0, b)?;
        }
        Ok(())
    }

    fn instruction<'a>(
        &self,
        function: &'a TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<&'a InstructionKind> {
        let mut found = None;
        for block in &function.blocks {
            b.spend(block.instructions.len().saturating_add(1), WORK)?;
            for instruction in &block.instructions {
                if instruction.result == Some(value) && found.replace(&instruction.kind).is_some() {
                    return Err(fail(
                        "implicit capture operand has competing actual definitions",
                    ));
                }
            }
        }
        found.ok_or_else(|| fail("implicit capture operand has no actual definition"))
    }

    fn implicit_record_observation(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        recipe: &SemanticSiteRecipe,
        value: ValueId,
        covered_operands: &mut BTreeSet<zil_base::NodeId>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let SemanticSiteRecipeKind::TypeUse {
            expected: expected @ SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Expression(node),
            },
        } = &recipe.kind else {
            return Err(fail("implicit record observation needs its exact original expression"));
        };
        let original = self.inventory.original_index(instance.definition, b)?;
        let mut expression = original.get_expr(*node, b)?;
        let mut physical = value;
        loop {
            b.spend(1, WORK)?;
            if expression.id != *node {
                b.lookup(covered_operands.len(), WORK)?;
                covered_operands.insert(expression.id);
            }
            match &expression.kind {
                CoreExprKind::Group(child) => expression = child,
                CoreExprKind::Member { object, member, .. } => {
                    let at = self.definition_at(function, physical, b)?;
                    let InstructionKind::ExtractField { aggregate, index } =
                        self.instruction(function, physical, b)?
                    else {
                        return Err(fail("implicit record observation changes its exact field operation"));
                    };
                    operations::available_before(function, at, *aggregate, b)?;
                    let Some(Type::Record { fields, .. }) = function.values.get(aggregate.0 as usize)
                    else {
                        return Err(unknown("implicit record observation needs a complete inline record"));
                    };
                    let (name, _) = fields.get(*index as usize)
                        .ok_or_else(|| fail("implicit record observation has no actual field"))?;
                    b.spend(member.len().saturating_add(name.len()), WORK)?;
                    if name != member {
                        return Err(fail("implicit record observation changes its original field"));
                    }
                    expression = object;
                    physical = *aggregate;
                }
                CoreExprKind::Name(_) => break,
                _ => return Err(unknown("implicit record observation needs its exact lexical owner")),
            }
        }
        let anchor = self.original.local_read_anchor(instance.definition, expression.id, b)?
            .ok_or_else(|| unknown("implicit record observation needs its exact local binding"))?;
        let InstructionKind::Load(local) = self.instruction(function, physical, b)? else {
            return Err(fail("implicit record observation changes its original local read"));
        };
        if self.anchored_local(instance, function, anchor, b)? != *local {
            return Err(fail("implicit record observation selects another lexical owner"));
        }
        let expected = self.recipe_value(instance, expected, b, 0)?;
        let actual = self.ssa_value(instance, function, value, b, 0)?;
        if !expected.ty.is_b0_scalar() || !empty(&expected.life) || !expected.classes.is_empty()
            || !actual.equal_paid(&expected, b)?
        {
            return Err(unknown("implicit record observation needs its complete scalar selected value"));
        }
        Ok(())
    }

    fn captured_read(
        &self,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<u32> {
        let InstructionKind::ReadAddress { address } = self.instruction(function, value, b)? else {
            return Err(unknown(
                "implicit capture occurrence needs its exact actual captured-head read",
            ));
        };
        operations::available_before(
            function,
            self.definition_at(function, value, b)?,
            *address,
            b,
        )?;
        let actual = self.heads(function, *address, None, &mut BTreeSet::new(), 0, b)?;
        let mut selected = None;
        for head in actual {
            let Head::Capture(owner, index) = head else {
                return Err(fail(
                    "original implicit name does not read its actual captured head",
                ));
            };
            if owner != function.id || selected.is_some_and(|previous| previous != index) {
                return Err(fail(
                    "original implicit capture occurrence selects competing actual heads",
                ));
            }
            selected = Some(index);
        }
        selected.ok_or_else(|| fail("original implicit capture occurrence has no actual head"))
    }

    /// Complete Fixed occurrence inventory. Address rows select one actual
    /// head; opaque rows retain every original capture/child selector. Neither
    /// captured Type order nor equal lifetime images choose an owning root.
    pub(super) fn capture_directory(
        &self,
        id: ClosureId,
        b: &mut Budget,
    ) -> CompileResult<CaptureDirectory> {
        {
            let completed=self.capture_directories.borrow();
            b.lookup(completed.len(),WORK)?;
            if let Some(directory)=completed.get(&id) {return directory.copy_paid(b);}
        }
        {
            let mut active = self.scope_queries.borrow_mut();
            b.lookup(active.len(), WORK)?;
            if !active.insert(id) {
                return Err(unknown(
                    "captured directory retains an unresolved declaration dependency",
                ));
            }
        }
        let result = self.capture_directory_inner(id, b);
        self.scope_queries.borrow_mut().remove(&id);
        if let Ok(directory)=&result {
            let retained=directory.copy_paid(b)?;
            let mut completed=self.capture_directories.borrow_mut();
            b.lookup(completed.len(),WORK)?;
            completed.insert(id,retained);
        }
        result
    }
    pub(super) fn ghost_capture_directory(&self, id: ClosureId, b: &mut Budget)
        -> CompileResult<Vec<BTreeSet<(u32, u32)>>> {
        match self.capture_directory(id, b)? {
            CaptureDirectory::Ghost(rows) => Ok(rows),
            CaptureDirectory::Address(_) => Err(fail("opaque capture query selects an address-only directory")),
        }
    }
    fn capture_directory_inner(
        &self,
        id: ClosureId,
        b: &mut Budget,
    ) -> CompileResult<CaptureDirectory> {
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        let closure = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|closure| closure.id == id)
            .ok_or_else(|| fail("implicit head directory names an absent closure"))?;
        let ghost = self.has_ghost_capture(id, b)?;
        let mut directory = None;
        for world in self.function_worlds(closure.function, b)? {
            let rows = if ghost {
                CaptureDirectory::Ghost(self.form_ghost_capture_directory(closure, world, b)?)
            } else {
                self.address_capture_directory(closure, world, b)?
            };
            if let Some(previous) = &directory {
                rows.charge_rows(b)?;
                if &rows != previous {
                    return Err(fail("shared closure worlds disagree on their physical capture directory"));
                }
            } else {
                directory = Some(rows);
            }
        }
        directory.ok_or_else(|| fail("capture directory has no complete closure world"))
    }

    fn address_capture_directory(
        &self,
        closure: &ClosureDefinition,
        instance: &SemanticInstance,
        b: &mut Budget,
    ) -> CompileResult<CaptureDirectory> {
        let id = closure.id;
        let life = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("actual capture directory is Unknown"))?;
        let definition = self.inventory.definition(instance.definition, b)?;
        let function = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(closure.function), b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites: recipes,
            signature,
            ..
        } = &definition.body
        else {
            return Err(fail("borrowed capture has no original closure body"));
        };
        if definition.kind != SemanticDefinitionKind::Closure
            || function.environment != Some(id)
            || life.classes.iter().any(|row| {
                !matches!(
                    row.position.base,
                    StorageBase::Parameter(_) | StorageBase::Result
                )
            })
            || function.storage_life.as_ref() != Some(life)
            || !closure.lending_captures.is_empty()
        {
            return Err(unknown(
                "actual closure needs its full retained, lending or invocation context",
            ));
        }
        if !life.static_bindings.is_empty() {
            let mut parameters = Vec::new();
            for parameter in &signature.parameters {
                self.push_value(
                    &mut parameters,
                    self.recipe_value(instance, &parameter.ty, b, 0)?,
                    b,
                )?;
            }
            let result = self.recipe_value(instance, &signature.result, b, 0)?;
            self.original_static_header(instance, &parameters, &result, Some(id), life, b)?;
        }
        b.spend(
            (life.fixed_binder_count as usize).saturating_add(closure.captures.len()),
            WORK,
        )?;
        let mut directory = vec![None; life.fixed_binder_count as usize];
        // Potential implicit heads remain in the original syntax even when a
        // particular specialization captures a Copy value by value. Such a row
        // contributes no Fixed class, but its original emitted occurrence is
        // still checked instead of being dropped as an unused claim.
        let mut covered_operands = BTreeSet::new();
        let mut pending_operands = Vec::new();
        for occurrence in &definition.implicit_lifetimes {
            let mut emitted = false;
            let mut source_operand = None;
            for recipe in recipes {
                b.spend(
                    recipe
                        .occurrence_path
                        .len()
                        .saturating_add(occurrence.occurrence_path.len())
                        .saturating_add(1),
                    WORK,
                )?;
                if recipe.occurrence_path != occurrence.occurrence_path {
                    continue;
                }
                if let SemanticSiteRecipeKind::DeclareLocal {
                    expected: SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Binding(_),
                    },
                } = &recipe.kind
                {
                    // Potential declaration roles are checked by the exact
                    // DeclareLocal operation and its complete reference-role
                    // proof. A declaration never supplies a capture head.
                    for site in &self.program.sites {
                        b.spend(1, WORK)?;
                        if site.instance == instance.id && site.recipe == recipe.id {
                            if !matches!(site.action, SemanticSiteAction::DeclareLocal {
                                local: SemanticLocalTarget::Typed(_), ..
                            }) || emitted {
                                return Err(fail("implicit local role changes its exact declaration event"));
                            }
                            emitted = true;
                        }
                    }
                    continue;
                }
                if !matches!(recipe.kind, SemanticSiteRecipeKind::TypeUse { .. }) {
                    continue;
                }
                if let SemanticSiteRecipeKind::TypeUse {
                    expected: SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Expression(node),
                    },
                } = &recipe.kind
                {
                    if source_operand.replace(*node).is_some_and(|old| old != *node) {
                        return Err(fail("one implicit occurrence selects competing original expressions"));
                    }
                }
                for site in &self.program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != instance.id || site.recipe != recipe.id {
                        continue;
                    }
                    let SemanticSiteAction::TypeUse {
                        subject: SemanticOperand::Value(value),
                        ..
                    } = site.action
                    else {
                        return Err(fail("original implicit name has no actual emitted read"));
                    };
                    match self.instruction(function, value, b)? {
                        InstructionKind::StaticUtf8 { bytes } => {
                            let original = self.original.string_literal_at(instance, recipe.id, self.inventory, b)?
                                .ok_or_else(|| fail("literal observation has no exact original String occurrence"))?;
                            b.spend(bytes.len().saturating_add(original.len()), WORK)?;
                            let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
                                unreachable!()
                            };
                            let expected = self.recipe_value(instance, expected, b, 0)?;
                            if original != bytes.as_slice()
                                || !original::strings::is_byte_type(&expected.ty)
                                || !empty(&expected.life)
                                || !expected.classes.is_empty()
                                || function.values.get(value.0 as usize) != Some(&expected.ty)
                            {
                                return Err(fail(
                                    "literal observation changes its original bytes or complete Static type",
                                ));
                            }
                            // An original literal observation is not a capture
                            // and contributes no Fixed class or borrowed head.
                        }
                        InstructionKind::ReadCapture { index }
                            if closure
                                .captures
                                .get(*index as usize)
                                .is_some_and(Type::is_b0_scalar) => {}
                        InstructionKind::ReadAddress { .. } => {
                            self.captured_read(function, value, b)?;
                        }
                        InstructionKind::TakeOwnedArrayElement { local, index: 0 }
                        | InstructionKind::BorrowOwnedArrayElement {
                            local, index: 0, ..
                        } => {
                            let anchor = self
                                .original
                                .local_recipe_anchor(instance.definition, recipe, b)?
                                .ok_or_else(|| {
                                    fail("implicit element read lacks its exact original binding")
                                })?;
                            if matches!(anchor, original::LocalAnchor::Environment(_))
                                || self.anchored_local(instance, function, anchor, b)? != *local
                            {
                                return Err(fail(
                                    "implicit element read selects another original local",
                                ));
                            }
                            let slot = self.persistent_slot(instance, function, *local, b)?
                                .ok_or_else(|| fail("implicit element read lacks its original closure slot witness"))?;
                            let at = self.definition_at(function, value, b)?;
                            let actual = self.persistent_slot_value(
                                instance,
                                function,
                                &slot,
                                (at.0, at.1),
                                b,
                                0,
                            )?;
                            let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
                                return Err(fail(
                                    "implicit element read replaces its original Name operation",
                                ));
                            };
                            let expected = self.recipe_value(instance, expected, b, 0)?;
                            if !self.capture_values_equal(&actual, &expected, b)? {
                                return Err(fail(
                                    "implicit element read changes its complete original child value",
                                ));
                            }
                            let output =
                                function.values.get(value.0 as usize).ok_or_else(|| {
                                    fail("implicit element read lacks its actual result type")
                                })?;
                            match self.instruction(function, value, b)? {
                                InstructionKind::TakeOwnedArrayElement { .. }
                                    if output == &actual.ty => {}
                                InstructionKind::BorrowOwnedArrayElement { mutable, .. }
                                    if matches!(output, Type::Address { mutable: mode, inner }
                                        if mode == mutable && inner.as_ref() == &actual.ty) => {}
                                _ => {
                                    return Err(fail(
                                        "implicit element read changes its complete child or access mode",
                                    ));
                                }
                            }
                            operations::available_before(
                                function,
                                (at.0, at.1, SemanticSiteEdge::After),
                                value,
                                b,
                            )?;
                        }
                        InstructionKind::ExtractField { .. } => {
                            self.implicit_record_observation(
                                instance, function, recipe, value, &mut covered_operands, b,
                            )?;
                        }
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                            if let Some(anchor) =
                                self.original
                                    .local_recipe_anchor(instance.definition, recipe, b)?
                            {
                                if matches!(anchor, original::LocalAnchor::Environment(_))
                                    || self.anchored_local(instance, function, anchor, b)? != *local
                                {
                                    return Err(fail(
                                        "implicit name read substitutes another actual local for its original binder",
                                    ));
                                }
                            } else if let Some(ordinal) = self.original.capture_recipe_ordinal(
                                instance.definition,
                                recipe,
                                b,
                            )? {
                                // A moved Copy capture may be materialized in
                                // an activation local. Follow its real producer;
                                // matching names/types or slot order is not a
                                // captured origin or an additional head loan.
                                let capture =
                                    self.environment_capture(instance, function, ordinal, b)?;
                                if self.spilled_capture_name(function, *local, value, capture, b)? != capture {
                                    return Err(fail(
                                        "implicit spilled capture read changes its original environment source",
                                    ));
                                }
                            } else {
                                return Err(fail(
                                    "implicit name read lacks its exact original local or captured binder",
                                ));
                            }
                            let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind else {
                                return Err(fail(
                                    "implicit local read has another original operation",
                                ));
                            };
                            let expected = self.recipe_value(instance, expected, b, 0)?;
                            let actual_type = function
                                .values
                                .get(value.0 as usize)
                                .ok_or_else(|| fail("implicit local read has no actual output"))?;
                            let declaration = function
                                .locals
                                .get(local.0 as usize)
                                .filter(|declaration| declaration.id == *local)
                                .ok_or_else(|| {
                                    fail("implicit local read has no actual declaration")
                                })?;
                            if !geometry::same_type(actual_type, &declaration.ty, b)?
                                || !geometry::same_type(actual_type, &expected.ty, b)?
                                || !self
                                    .ssa_value(instance, function, value, b, 0)?
                                    .equal_paid(&expected, b)?
                            {
                                return Err(fail(
                                    "implicit local read changes its complete original value",
                                ));
                            }
                            // This read supplies no Fixed capture head. Its
                            // initialized occupant and consuming/Copy mode are
                            // checked independently by the actual CFG kernel.
                            let (block, index, _) = self.definition_at(function, value, b)?;
                            operations::available_before(
                                function,
                                (block, index, SemanticSiteEdge::After),
                                value,
                                b,
                            )?;
                        }
                        _ => {
                            return Err(unknown(
                                "original implicit occurrence needs its full actual capture-mode interpretation",
                            ));
                        }
                    }
                    emitted = true;
                }
            }
            if !emitted {
                let node = source_operand.ok_or_else(|| {
                    fail("original implicit occurrence has no matching actual name site")
                })?;
                self.push_value(&mut pending_operands, node, b)?;
            }
        }
        // A local may occur only as the selected operand of an actual
        // projection. The checked physical Load accounts for that child.
        for node in pending_operands {
            b.lookup(covered_operands.len(), WORK)?;
            if !covered_operands.contains(&node) {
                return Err(fail(
                    "original implicit occurrence has no matching actual name site",
                ));
            }
        }
        let mut seen_capture = BTreeSet::new();
        for binding in &life.bindings {
            if matches!(
                binding.position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                b.spend(binding.position.path.len().saturating_add(1), WORK)?;
                continue;
            }
            b.spend(
                directory
                    .len()
                    .saturating_add(seen_capture.len())
                    .saturating_add(1),
                WORK,
            )?;
            let (StorageBase::Capture(index), StorageBinder::Fixed(class)) =
                (binding.position.base, binding.binder)
            else {
                return Err(unknown(
                    "implicit scalar-capture directory contains another scope occurrence",
                ));
            };
            if !binding.position.path.is_empty()
                || !seen_capture.insert(index)
                || !matches!(closure.captures.get(index as usize), Some(Type::Address { mutable: false, inner }) if inner.is_b0_scalar())
            {
                return Err(fail(
                    "implicit scalar-capture binding loses its exact borrowed head or mode",
                ));
            }
            let slot = directory.get_mut(class as usize).ok_or_else(|| {
                fail("implicit capture Fixed ordinal is outside the complete directory")
            })?;
            if slot.replace(index).is_some() {
                return Err(unknown(
                    "several implicit captured heads need their independently proved joint relation",
                ));
            }
        }
        for (index, ty) in closure.captures.iter().enumerate() {
            b.spend(seen_capture.len().saturating_add(1), WORK)?;
            if ty.is_b0_scalar() {
                continue;
            }
            if !matches!(ty, Type::Address { mutable: false, inner } if inner.is_b0_scalar())
                || !seen_capture.contains(&(index as u32))
            {
                return Err(unknown(
                    "actual closure has an unproved external, owning or phantom capture domain",
                ));
            }
        }
        b.spend(instance.header_embedding.len(), WORK)?;
        if instance
            .header_embedding
            .iter()
            .filter(|row| matches!(row.header, StorageBinder::Fixed(_)))
            .count()
            != directory.len()
        {
            return Err(fail(
                "closure header omits its complete actual implicit head domain",
            ));
        }
        for (class, index) in directory.iter().enumerate() {
            let index = index.ok_or_else(|| {
                fail("actual capture directory contains an ungrounded Fixed class")
            })?;
            b.spend(instance.header_embedding.len(), WORK)?;
            let mut bindings = instance
                .header_embedding
                .iter()
                .filter(|row| row.header == StorageBinder::Fixed(class as u32));
            let binding = bindings.next().ok_or_else(|| {
                fail("actual capture Fixed class has no original occurrence embedding")
            })?;
            if bindings.next().is_some() {
                return Err(fail("actual capture Fixed embedding is repeated"));
            }
            let SemanticClassExpr::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } = binding.actual
            else {
                return Err(unknown(
                    "capture Fixed class is not bound to its exact original implicit occurrence",
                ));
            };
            b.spend(definition.implicit_lifetimes.len(), WORK)?;
            let mut implicit = definition
                .implicit_lifetimes
                .iter()
                .filter(|row| row.ordinal == ordinal);
            let occurrence = implicit
                .next()
                .ok_or_else(|| fail("capture head names an absent original implicit lifetime"))?;
            if implicit.next().is_some() {
                return Err(fail("original implicit capture ordinal is ambiguous"));
            }
            let mut found = false;
            for recipe in recipes {
                b.spend(
                    recipe
                        .occurrence_path
                        .len()
                        .saturating_add(occurrence.occurrence_path.len())
                        .saturating_add(1),
                    WORK,
                )?;
                if recipe.occurrence_path != occurrence.occurrence_path
                    || !matches!(recipe.kind, SemanticSiteRecipeKind::TypeUse { .. })
                {
                    continue;
                }
                for site in &self.program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != instance.id || site.recipe != recipe.id {
                        continue;
                    }
                    let SemanticSiteAction::TypeUse {
                        subject: SemanticOperand::Value(value),
                        ..
                    } = site.action
                    else {
                        return Err(fail(
                            "implicit capture occurrence has no actual scalar read",
                        ));
                    };
                    if self.captured_read(function, value, b)? != index {
                        return Err(fail(
                            "capture implicit occurrence is rebound to another actual captured head",
                        ));
                    }
                    found = true;
                }
            }
            if !found {
                return Err(fail(
                    "implicit captured head has no exact original name emission",
                ));
            }
        }
        b.spend(directory.len(), WORK)?;
        directory
            .into_iter()
            .map(|row| row.ok_or_else(|| fail("implicit head directory is incomplete")))
            .collect::<CompileResult<Vec<_>>>()
            .map(CaptureDirectory::Address)
    }

    #[allow(clippy::too_many_arguments)]
    fn local_heads(
        &self,
        function: &TypedFunction,
        local: LocalId,
        class: Option<u32>,
        at: (BlockId, usize, SemanticSiteEdge),
        active: &mut ActiveHeads,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        self.place_heads(
            function,
            local,
            None,
            &[],
            false,
            class,
            at,
            active,
            depth,
            b,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn place_heads(
        &self,
        function: &TypedFunction,
        local: LocalId,
        slot: Option<&persistent_slots::Slot>,
        projection: &[PlaceProjection],
        pointee: bool,
        class: Option<u32>,
        at: (BlockId, usize, SemanticSiteEdge),
        active: &mut ActiveHeads,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        if depth > b.partial_depth {
            return Err(unknown(
                "actual implicit local-head derivation exceeds its finite bound",
            ));
        }
        if slot.is_some() {
            operations::initialized_local_before(
                function,
                (
                    at.0,
                    at.1.saturating_add(usize::from(at.2 == SemanticSiteEdge::After)),
                ),
                local,
                b,
            )?;
        }
        let initialization = crate::shared_initialization(function, b)?;
        let mut out = Heads::new();
        let mut pending = vec![(
            at.0,
            at.1.saturating_add(usize::from(at.2 == SemanticSiteEdge::After)),
        )];
        let mut seen = BTreeSet::new();
        while let Some((block, end)) = pending.pop() {
            b.spend(seen.len().saturating_add(1), WORK)?;
            if !seen.insert((block, end)) {
                continue;
            }
            let actual = function
                .blocks
                .get(block.0 as usize)
                .filter(|actual| actual.id == block && end <= actual.instructions.len())
                .ok_or_else(|| fail("implicit local head has no actual CFG boundary"))?;
            let mut found = false;
            for (index, instruction) in actual.instructions.iter().enumerate().take(end).rev() {
                b.spend(1, WORK)?;
                if slot.is_some() || !projection.is_empty() {
                    self.head_projection_barrier(
                        function,
                        local,
                        projection,
                        &instruction.kind,
                        b,
                    )?;
                }
                match instruction.kind {
                    InstructionKind::Store {
                        local: destination,
                        value,
                    } if destination == local => {
                        operations::available_before(
                            function,
                            (block, index, SemanticSiteEdge::Before),
                            value,
                            b,
                        )?;
                        let source = if let Some(slot) = slot {
                            if slot.initial_store() != (block, index) || slot.array() != value {
                                return Err(fail(
                                    "implicit slot head crosses an unrelated shell Store",
                                ));
                            }
                            slot.initial()
                        } else {
                            value
                        };
                        let sources = if pointee {
                            self.pointee_heads(
                                function,
                                source,
                                class.ok_or_else(|| {
                                    fail("implicit Pointee query omits its child class")
                                })?,
                                active,
                                depth + 1,
                                b,
                            )?
                        } else {
                            self.projected_heads(
                                function,
                                source,
                                projection,
                                class,
                                active,
                                depth + 1,
                                b,
                            )?
                        };
                        b.spend(out.len().saturating_add(sources.len()), WORK)?;
                        out.extend(sources);
                        found = true;
                        break;
                    }
                    InstructionKind::StoreOwnedArrayElement {
                        local: destination,
                        index: element,
                        value,
                    } if destination == local && slot.is_some() => {
                        if element != 0 {
                            return Err(fail("implicit slot head selects another actual element"));
                        }
                        operations::available_before(
                            function,
                            (block, index, SemanticSiteEdge::Before),
                            value,
                            b,
                        )?;
                        let sources = if pointee {
                            self.pointee_heads(
                                function,
                                value,
                                class.ok_or_else(|| {
                                    fail("implicit Pointee query omits its child class")
                                })?,
                                active,
                                depth + 1,
                                b,
                            )?
                        } else {
                            self.projected_heads(
                                function,
                                value,
                                projection,
                                class,
                                active,
                                depth + 1,
                                b,
                            )?
                        };
                        b.spend(out.len().saturating_add(sources.len()), WORK)?;
                        out.extend(sources);
                        found = true;
                        break;
                    }
                    InstructionKind::TakeOwnedArrayElement {
                        local: destination, ..
                    }
                    | InstructionKind::DropOwnedArrayElement {
                        local: destination, ..
                    }
                    | InstructionKind::DropOwnedArray { local: destination }
                        if destination == local && slot.is_some() =>
                    {
                        return Err(fail(
                            "implicit slot head read crosses an actual vacant element",
                        ));
                    }
                    InstructionKind::StorageLive(destination)
                    | InstructionKind::StorageDead(destination)
                    | InstructionKind::TakeLocal { local: destination }
                    | InstructionKind::DropClosure { local: destination }
                        if destination == local =>
                    {
                        return Err(fail(
                            "implicit local-head read crosses an actual vacant activation",
                        ));
                    }
                    _ => {}
                }
            }
            if found {
                continue;
            }
            if block == function.entry {
                return Err(unknown(
                    "implicit local head has no actual initialized entry origin",
                ));
            }
            let mut incoming = false;
            for predecessor in &initialization.predecessors[block.0 as usize] {
                b.spend(1, WORK)?;
                if !initialization.reachable[*predecessor] {
                    continue;
                }
                let previous = &function.blocks[*predecessor];
                b.spend(pending.len().saturating_add(1), WORK)?;
                pending.push((previous.id, previous.instructions.len()));
                incoming = true;
            }
            if !incoming {
                return Err(fail(
                    "implicit head has no initialized reachable predecessor",
                ));
            }
        }
        if out.is_empty() && depth == 0 {
            return Err(fail(
                "implicit head has no actual reaching initialized local origin",
            ));
        }
        Ok(out)
    }

    pub(super) fn head_projection_barrier(
        &self,
        function: &TypedFunction,
        local: LocalId,
        path: &[PlaceProjection],
        instruction: &InstructionKind,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let overlaps =
            |changed: &[PlaceProjection]| path.starts_with(changed) || changed.starts_with(path);
        match instruction {
            InstructionKind::BorrowLocalMut { local: target } if *target == local => {
                return Err(unknown(
                    "implicit projected head crosses an unproved whole mutable alias",
                ));
            }
            InstructionKind::BorrowPlaceLocal {
                local: target,
                path: changed,
                mutable: true,
            } if *target == local => {
                b.spend(path.len().saturating_add(changed.len()), WORK)?;
                if overlaps(changed) {
                    return Err(unknown(
                        "implicit projected head crosses an unproved overlapping mutable alias",
                    ));
                }
            }
            InstructionKind::BorrowPackageField {
                local: target,
                index,
                mutable: true,
            } if *target == local => {
                b.spend(path.len().saturating_add(1), WORK)?;
                if overlaps(&[PlaceProjection::Field(*index)]) {
                    return Err(unknown(
                        "implicit projected head crosses an unproved mutable field alias",
                    ));
                }
            }
            InstructionKind::BorrowFieldLocal {
                local: target,
                path: changed,
                mutable: true,
            } if *target == local => {
                b.spend(path.len().saturating_add(changed.len()), WORK)?;
                if path
                    .iter()
                    .zip(changed)
                    .all(|(actual, field)| *actual == PlaceProjection::Field(*field))
                {
                    return Err(unknown(
                        "implicit projected head crosses an unproved mutable inline field alias",
                    ));
                }
            }
            InstructionKind::BorrowVariantLocal {
                local: target,
                path: changed,
                mutable: true,
            } if *target == local => {
                b.spend(path.len().saturating_add(changed.len()), WORK)?;
                if path
                    .iter()
                    .zip(changed)
                    .all(|(actual, field)| *actual == PlaceProjection::Variant(*field))
                {
                    return Err(unknown(
                        "implicit projected head crosses an unproved mutable variant alias",
                    ));
                }
            }
            InstructionKind::StorePackageField {
                local: target,
                index,
                ..
            }
            | InstructionKind::TakePackageField {
                local: target,
                index,
            }
            | InstructionKind::DropPackageField {
                local: target,
                index,
            } if *target == local => {
                b.spend(path.len().saturating_add(1), WORK)?;
                if overlaps(&[PlaceProjection::Field(*index)]) {
                    return Err(unknown(
                        "implicit projected head crosses a changed or vacant owning field",
                    ));
                }
            }
            InstructionKind::TakeTupleField {
                local: target,
                path: changed,
            }
            | InstructionKind::TakeVariantField {
                local: target,
                path: changed,
            } if *target == local => {
                b.spend(path.len().saturating_add(changed.len()), WORK)?;
                if overlaps(changed) {
                    return Err(unknown(
                        "implicit projected head crosses a transferred child place",
                    ));
                }
            }
            InstructionKind::DropPackage { local: target }
            | InstructionKind::DropTuple { local: target }
            | InstructionKind::DropEnum { local: target }
            | InstructionKind::DropErrorUnion { local: target }
            | InstructionKind::EndPackageCapture { local: target, .. }
            | InstructionKind::BindPackageCapture { local: target, .. }
                if *target == local =>
            {
                return Err(unknown(
                    "implicit projected head crosses a changed whole owner or capture activation",
                ));
            }
            InstructionKind::UnpackTuple {
                local: source,
                fields,
            }
            | InstructionKind::UnpackVariant {
                local: source,
                fields,
                ..
            } => {
                b.spend(fields.len().saturating_add(1), WORK)?;
                if *source == local || fields.contains(&local) {
                    return Err(unknown(
                        "implicit projected head requires its actual unpacked child source",
                    ));
                }
            }
            InstructionKind::CommitPlaceReplace { reservation, .. } => {
                let mut found = false;
                for block in &function.blocks {
                    for producer in &block.instructions {
                        b.spend(1, WORK)?;
                        match &producer.kind {
                            InstructionKind::ReservePlaceLocal {
                                reservation: actual,
                                local: target,
                                path: changed,
                            } if actual == reservation => {
                                if found {
                                    return Err(fail(
                                        "implicit head replacement has competing reservation definitions",
                                    ));
                                }
                                found = true;
                                b.spend(path.len().saturating_add(changed.len()), WORK)?;
                                if *target == local && overlaps(changed) {
                                    return Err(unknown(
                                        "implicit projected head crosses a committed overlapping replacement",
                                    ));
                                }
                            }
                            InstructionKind::ReservePlaceAddress {
                                reservation: actual,
                                ..
                            } if actual == reservation => {
                                return Err(unknown(
                                    "implicit projected head needs the committed address's complete source and effect",
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                if !found {
                    return Err(fail(
                        "implicit head replacement lacks its real reservation definition",
                    ));
                }
            }
            InstructionKind::WriteAddress { .. } | InstructionKind::StoreStoredBorrow { .. } => {
                return Err(unknown(
                    "implicit projected head needs a complete overlapping address-write effect",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn head_projection_type<'t>(mut ty: &'t Type, path: &[PlaceProjection]) -> Option<&'t Type> {
        for step in path {
            while let Type::BorrowedStorage { storage, .. } = ty {
                ty = storage;
            }
            ty = match (ty, step) {
                (
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                    PlaceProjection::Field(index),
                ) => &fields.get(*index as usize)?.1,
                (Type::Tuple(fields), PlaceProjection::Field(index)) => {
                    fields.get(*index as usize)?
                }
                (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error)) => {
                    if *is_error {
                        error
                    } else {
                        result
                    }
                }
                (Type::Enum { variants, .. }, PlaceProjection::Variant(field)) => variants
                    .get(field.variant as usize)?
                    .1
                    .get(field.index as usize)?,
                _ => return None,
            };
        }
        Some(ty)
    }

    #[allow(clippy::too_many_arguments)]
    fn projected_heads(
        &self,
        function: &TypedFunction,
        value: ValueId,
        path: &[PlaceProjection],
        class: Option<u32>,
        active: &mut ActiveHeads,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        if path.is_empty() {
            return self.heads(function, value, class, active, depth, b);
        }
        if depth > b.partial_depth || path.len() > b.partial_depth {
            return Err(unknown(
                "implicit head projection exceeds its finite ancestry",
            ));
        }
        let ty = function
            .values
            .get(value.0 as usize)
            .ok_or_else(|| fail("implicit head projection has no actual SSA type"))?;
        crate::dyn_metadata::ty(ty, b)?;
        b.spend(
            path.len()
                .saturating_mul(2)
                .saturating_add(active.len())
                .saturating_add(1),
            WORK,
        )?;
        Self::head_projection_type(ty, path)
            .ok_or_else(|| fail("implicit head projection leaves its complete actual layout"))?;
        let key = (function.id, value, path.to_vec(), class, false);
        if !active.insert(key.clone()) {
            return Ok(Heads::new());
        }
        let at = self.definition_at(function, value, b)?;
        let result = match self.instruction(function, value, b)? {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => self
                .place_heads(
                    function,
                    *local,
                    None,
                    path,
                    false,
                    class,
                    at,
                    active,
                    depth + 1,
                    b,
                ),
            InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields } => {
                let PlaceProjection::Field(field) = path[0] else {
                    return Err(fail(
                        "implicit aggregate head changes its actual field selector",
                    ));
                };
                let input = *fields
                    .get(field as usize)
                    .ok_or_else(|| fail("implicit aggregate head selects no prepared field"))?;
                operations::available_before(function, at, input, b)?;
                self.projected_heads(function, input, &path[1..], class, active, depth + 1, b)
            }
            InstructionKind::MakeVariant { variant, fields } => {
                let PlaceProjection::Variant(field) = path[0] else {
                    return Err(fail("implicit variant head changes its checked selector"));
                };
                if field.variant != *variant {
                    return Err(fail(
                        "implicit variant head selects another active construction",
                    ));
                }
                let input = *fields
                    .get(field.index as usize)
                    .ok_or_else(|| fail("implicit variant head selects no prepared field"))?;
                operations::available_before(function, at, input, b)?;
                self.projected_heads(function, input, &path[1..], class, active, depth + 1, b)
            }
            InstructionKind::MakeErrorUnion {
                is_error,
                value: input,
            } => {
                if path[0] != PlaceProjection::ErrorArm(*is_error) {
                    return Err(fail(
                        "implicit error head selects another active construction",
                    ));
                }
                operations::available_before(function, at, *input, b)?;
                self.projected_heads(function, *input, &path[1..], class, active, depth + 1, b)
            }
            InstructionKind::TaskCreateOwned { arguments, .. } => {
                // The opcode returns exactly these original inputs on failure.
                // Check the entire physical inputs tuple, not one equal field;
                // original scheme/tag/resource checks remain independent.
                let [
                    PlaceProjection::ErrorArm(true),
                    PlaceProjection::Field(1),
                    PlaceProjection::Field(input),
                    tail @ ..,
                ] = path
                else {
                    return Err(unknown(
                        "owned Task head requires its complete failure-input selector",
                    ));
                };
                let inputs = Self::head_projection_type(
                    ty,
                    &[PlaceProjection::ErrorArm(true), PlaceProjection::Field(1)],
                )
                .ok_or_else(|| fail("owned Task head loses its actual failure input tuple"))?;
                let Type::Tuple(fields) = inputs else {
                    return Err(fail(
                        "owned Task failure inputs have another physical constructor",
                    ));
                };
                if fields.len() != arguments.len() {
                    return Err(fail("owned Task failure omits an actual constructor input"));
                }
                for (field, argument) in fields.iter().zip(arguments) {
                    operations::available_before(function, at, *argument, b)?;
                    let actual = function
                        .values
                        .get(argument.0 as usize)
                        .ok_or_else(|| fail("owned Task head lacks an actual input type"))?;
                    if !geometry::same_type(field, actual, b)? {
                        return Err(fail(
                            "owned Task failure changes its complete actual input tuple",
                        ));
                    }
                }
                let input = *arguments
                    .get(*input as usize)
                    .ok_or_else(|| fail("owned Task head selects no actual constructor input"))?;
                self.projected_heads(function, input, tail, class, active, depth + 1, b)
            }
            _ => self.head_projection_operation(function, value, path, class, active, depth + 1, b),
        };
        active.remove(&key);
        result.and_then(|sources| {
            if depth == 0 && sources.is_empty() {
                Err(fail(
                    "implicit projected head graph has no initialized source",
                ))
            } else {
                Ok(sources)
            }
        })
    }

    // A projection transports a selector, never a representative class image.
    #[allow(clippy::too_many_arguments)]
    fn head_projection_operation(
        &self,
        function: &TypedFunction,
        value: ValueId,
        tail: &[PlaceProjection],
        class: Option<u32>,
        active: &mut ActiveHeads,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        let at = self.definition_at(function, value, b)?;
        let instruction = self.instruction(function, value, b)?;
        let (local, operand, prefix): (Option<LocalId>, Option<ValueId>, Vec<PlaceProjection>) =
            match instruction {
                InstructionKind::ExtractField { aggregate, index } => {
                    b.spend(1, WORK)?;
                    (None, Some(*aggregate), vec![PlaceProjection::Field(*index)])
                }
                InstructionKind::ReadPackageField { local, index }
                | InstructionKind::TakePackageField { local, index } => {
                    b.spend(1, WORK)?;
                    (Some(*local), None, vec![PlaceProjection::Field(*index)])
                }
                InstructionKind::TakeTupleField { local, path }
                | InstructionKind::TakeVariantField { local, path } => {
                    b.spend(path.len(), WORK)?;
                    (Some(*local), None, path.clone())
                }
                InstructionKind::ReadErrorUnion { value, is_error }
                | InstructionKind::TakeErrorUnion { value, is_error } => {
                    b.spend(1, WORK)?;
                    (
                        None,
                        Some(*value),
                        vec![PlaceProjection::ErrorArm(*is_error)],
                    )
                }
                _ => {
                    return Err(unknown(
                        "implicit selected head needs its complete actual projection source",
                    ));
                }
            };
        b.spend(
            prefix.len().saturating_add(tail.len()).saturating_add(1),
            WORK,
        )?;
        let mut path = prefix;
        path.extend_from_slice(tail);
        let source_ty = if let Some(local) = local {
            operations::initialized_local_before(function, (at.0, at.1), local, b)?;
            &function
                .locals
                .get(local.0 as usize)
                .filter(|entry| entry.id == local)
                .ok_or_else(|| fail("implicit selected head has no actual local"))?
                .ty
        } else {
            let operand =
                operand.ok_or_else(|| fail("implicit projection lacks an actual source"))?;
            operations::available_before(function, at, operand, b)?;
            function
                .values
                .get(operand.0 as usize)
                .ok_or_else(|| fail("implicit selected head has no actual SSA source type"))?
        };
        let output = function
            .values
            .get(value.0 as usize)
            .ok_or_else(|| fail("implicit selected head has no result type"))?;
        b.spend(
            path.len().saturating_add(tail.len()).saturating_add(1),
            WORK,
        )?;
        let selected = Self::head_projection_type(source_ty, &path)
            .ok_or_else(|| fail("implicit selected head changes its complete source path"))?;
        let expected = Self::head_projection_type(output, tail)
            .ok_or_else(|| fail("implicit selected head changes its complete result path"))?;
        if !geometry::same_type(selected, expected, b)? {
            return Err(fail(
                "implicit selected head changes its complete projected child type",
            ));
        }
        match (local, operand) {
            (Some(local), None) => self.place_heads(
                function, local, None, &path, false, class, at, active, depth, b,
            ),
            (None, Some(operand)) => {
                self.projected_heads(function, operand, &path, class, active, depth, b)
            }
            _ => Err(fail(
                "implicit projection changes its actual source category",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pointee_heads(
        &self,
        function: &TypedFunction,
        value: ValueId,
        class: u32,
        active: &mut ActiveHeads,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        if depth > b.partial_depth {
            return Err(unknown(
                "implicit Pointee head exceeds its finite source ancestry",
            ));
        }
        let Some(Type::Address {
            mutable: output_mutable,
            inner,
        }) = function.values.get(value.0 as usize)
        else {
            return Err(fail("implicit Pointee selector requires an actual Address"));
        };
        if !matches!(inner.as_ref(), Type::Closure(_)) {
            return Err(fail(
                "implicit Pointee selector changes its complete closure child",
            ));
        }
        b.spend(active.len().saturating_add(1), WORK)?;
        let key = (function.id, value, Vec::new(), Some(class), true);
        if !active.insert(key.clone()) {
            return Ok(Heads::new());
        }
        let at = self.definition_at(function, value, b)?;
        let result = match self.instruction(function, value, b)? {
            InstructionKind::BorrowOwnedArrayElement {
                local,
                index: 0,
                mutable,
            } => {
                let slot = self
                    .head_slot(function, *local, b)?
                    .ok_or_else(|| fail("implicit Pointee head lacks its exact original slot"))?;
                if output_mutable != mutable || inner.as_ref() != &slot.logical().ty {
                    return Err(fail(
                        "implicit Pointee head changes its complete child or access mode",
                    ));
                }
                self.place_heads(
                    function,
                    *local,
                    Some(&slot),
                    &[],
                    false,
                    Some(class),
                    at,
                    active,
                    depth + 1,
                    b,
                )
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => self
                .place_heads(
                    function,
                    *local,
                    None,
                    &[],
                    true,
                    Some(class),
                    at,
                    active,
                    depth + 1,
                    b,
                ),
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                let mutable = matches!(
                    self.instruction(function, value, b)?,
                    InstructionKind::BorrowLocalMut { .. }
                );
                if *output_mutable != mutable
                    || !function
                        .locals
                        .get(local.0 as usize)
                        .is_some_and(|entry| entry.id == *local && &entry.ty == inner.as_ref())
                {
                    return Err(fail(
                        "implicit Pointee local borrow changes its actual child or mode",
                    ));
                }
                self.place_heads(
                    function,
                    *local,
                    None,
                    &[],
                    false,
                    Some(class),
                    at,
                    active,
                    depth + 1,
                    b,
                )
            }
            InstructionKind::ReborrowAddress { address, mutable } => {
                operations::available_before(function, at, *address, b)?;
                if output_mutable != mutable
                    || !matches!(function.values.get(address.0 as usize),
                    Some(Type::Address { mutable: parent, inner: child })
                    if (!*mutable || *parent) && child == inner)
                {
                    return Err(fail(
                        "implicit Pointee reborrow changes its whole child or strengthens access",
                    ));
                }
                self.pointee_heads(function, *address, class, active, depth + 1, b)
            }
            _ => Err(unknown(
                "implicit Pointee head needs its complete actual pointer origin",
            )),
        };
        active.remove(&key);
        result.and_then(|sources| {
            if depth == 0 && sources.is_empty() {
                Err(fail("implicit Pointee graph has no initialized source"))
            } else {
                Ok(sources)
            }
        })
    }

    // heads() derives physical source identities without an original-world
    // argument. Establish the same exact representation in every real world
    // of this function; never select the first equal image as an authority.
    fn head_slot(
        &self,
        function: &TypedFunction,
        local: LocalId,
        b: &mut Budget,
    ) -> CompileResult<Option<persistent_slots::Slot>> {
        let mut selected: Option<persistent_slots::Slot> = None;
        let mut worlds = 0usize;
        for instance in &self.program.instances {
            b.spend(1, WORK)?;
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                continue;
            };
            if self.inventory.function(target, b)?.id != function.id {
                continue;
            }
            worlds += 1;
            let Some(slot) = self.persistent_slot(instance, function, local, b)? else {
                return Err(unknown(
                    "implicit array head has no exact original closure slot witness",
                ));
            };
            if let Some(old) = &selected {
                if old.local() != slot.local()
                    || old.declaration() != slot.declaration()
                    || old.initial_store() != slot.initial_store()
                    || old.array() != slot.array()
                    || old.initial() != slot.initial()
                {
                    return Err(fail(
                        "implicit slot worlds disagree on the actual representation",
                    ));
                }
            } else {
                selected = Some(slot);
            }
        }
        if worlds == 0 {
            return Err(fail(
                "implicit slot head has no actual original function world",
            ));
        }
        Ok(selected)
    }

    fn array_head_slot(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<Option<persistent_slots::Slot>> {
        if !matches!(function.values.get(value.0 as usize),
            Some(Type::Array { length: 1, element }) if matches!(element.as_ref(), Type::Closure(_)))
        {
            return Ok(None);
        }
        self.persistent_array_slot(instance, function, value, b)?
            .map(Some)
            .ok_or_else(|| {
                unknown("implicit array source lacks its complete original slot witness")
            })
    }

    fn definition_at(
        &self,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<(BlockId, usize, SemanticSiteEdge)> {
        let mut found = None;
        for block in &function.blocks {
            b.spend(block.instructions.len().saturating_add(1), WORK)?;
            for (index, instruction) in block.instructions.iter().enumerate() {
                if instruction.result == Some(value)
                    && found
                        .replace((block.id, index, SemanticSiteEdge::Before))
                        .is_some()
                {
                    return Err(fail("implicit operand has competing actual emission sites"));
                }
            }
        }
        found.ok_or_else(|| fail("implicit operand has no actual emission site"))
    }

    fn heads(
        &self,
        function: &TypedFunction,
        value: ValueId,
        class: Option<u32>,
        active: &mut ActiveHeads,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        if depth > b.partial_depth {
            return Err(unknown(
                "actual implicit-head derivation exceeds its finite bound",
            ));
        }
        b.spend(active.len().saturating_add(1), WORK)?;
        let key = (function.id, value, Vec::new(), class, false);
        if !active.insert(key.clone()) {
            // A backedge adds no source. The outer query must still reach a
            // real initialized seed; every non-cyclic predecessor is visited.
            return Ok(Heads::new());
        }
        let result = match (self.instruction(function, value, b)?, class) {
            (InstructionKind::BorrowLocal { local }, None) => {
                if !function
                    .locals
                    .get(local.0 as usize)
                    .is_some_and(|local| local.ty.is_b0_scalar())
                {
                    return Err(unknown(
                        "implicit capture borrow needs its complete actual local type",
                    ));
                }
                b.spend(1, WORK)?;
                Ok(Heads::from([Head::Local(function.id, *local)]))
            }
            (InstructionKind::ReadCapture { index }, None) => {
                let closure = function
                    .environment
                    .ok_or_else(|| fail("implicit captured address has no real environment"))?;
                let definition = self
                    .module
                    .closures
                    .get(closure.0 as usize)
                    .filter(|definition| {
                        definition.id == closure && definition.function == function.id
                    })
                    .ok_or_else(|| {
                        fail("implicit captured address has another environment entry")
                    })?;
                if !matches!(definition.captures.get(*index as usize), Some(Type::Address { mutable: false, inner }) if inner.is_b0_scalar())
                {
                    return Err(fail(
                        "implicit captured address is not in the complete head directory",
                    ));
                }
                b.spend(1, WORK)?;
                Ok(Heads::from([Head::Capture(function.id, *index)]))
            }
            (
                InstructionKind::ReborrowAddress {
                    address,
                    mutable: false,
                },
                None,
            ) => {
                operations::available_before(
                    function,
                    self.definition_at(function, value, b)?,
                    *address,
                    b,
                )?;
                self.heads(function, *address, None, active, depth + 1, b)
            }
            (InstructionKind::Load(local) | InstructionKind::TakeLocal { local }, class) => self
                .local_heads(
                    function,
                    *local,
                    class,
                    self.definition_at(function, value, b)?,
                    active,
                    depth + 1,
                    b,
                ),
            (
                InstructionKind::ExtractField { .. }
                | InstructionKind::ReadPackageField { .. }
                | InstructionKind::TakePackageField { .. }
                | InstructionKind::TakeTupleField { .. }
                | InstructionKind::TakeVariantField { .. }
                | InstructionKind::ReadErrorUnion { .. }
                | InstructionKind::TakeErrorUnion { .. },
                class,
            ) => self.head_projection_operation(function, value, &[], class, active, depth + 1, b),
            (InstructionKind::TakeOwnedArrayElement { local, index }, Some(class)) => {
                if *index != 0 {
                    return Err(fail(
                        "implicit closure head selects another persistent slot element",
                    ));
                }
                let slot = self.head_slot(function, *local, b)?.ok_or_else(|| {
                    unknown("implicit closure element has no complete original slot")
                })?;
                let output = function
                    .values
                    .get(value.0 as usize)
                    .ok_or_else(|| fail("implicit slot read omits its actual result type"))?;
                if !geometry::same_type(output, &slot.logical().ty, b)? {
                    return Err(fail("implicit slot take changes its complete child type"));
                }
                self.place_heads(
                    function,
                    *local,
                    Some(&slot),
                    &[],
                    false,
                    Some(class),
                    self.definition_at(function, value, b)?,
                    active,
                    depth + 1,
                    b,
                )
            }
            (InstructionKind::MakeClosure { closure, captures }, Some(class)) => {
                let directory = self.capture_directory(*closure, b)?.address()?;
                let index = directory.get(class as usize).ok_or_else(|| {
                    fail("implicit closure class is outside the actual complete directory")
                })?;
                let operand = captures
                    .get(*index as usize)
                    .ok_or_else(|| fail("actual creator omits the implicit capture operand"))?;
                self.heads(function, *operand, None, active, depth + 1, b)
            }
            _ => Err(unknown(
                "implicit captured-head origin needs its complete actual transport",
            )),
        };
        active.remove(&key);
        result.and_then(|sources| {
            if depth == 0 && sources.is_empty() {
                Err(fail("actual implicit-head graph has no initialized source"))
            } else {
                Ok(sources)
            }
        })
    }

    pub(super) fn image_heads(
        &self,
        instance: &SemanticInstance,
        value: &SemanticTypeValue,
        at: (BlockId, usize, SemanticSiteEdge),
        b: &mut Budget,
    ) -> CompileResult<Vec<Heads>> {
        let SemanticTypeValue::Resolved {
            ty: Type::Closure(id),
            life,
            classes,
        } = value
        else {
            self.value_type(value, b)?;
            return Ok(Vec::new());
        };
        if self.has_ghost_capture(*id, b)? {
            self.ghost_capture_directory(*id, b)?;
            self.resolved(instance, value, b, 0)?;
            // This lane compares the complete intrinsic domain, including an
            // original moved reference role. SSA images use the independently
            // reconstructed producer; they cannot certify their own claim.
            // Actual captured targets remain obligations of the source CFG.
            return Ok(Vec::new());
        }
        let directory = self.capture_directory(*id, b)?;
        if life.binder_count as usize != directory.len()
            || classes.len() != directory.len()
            || !life.slots.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Err(fail(
                "opaque closure value loses its actual complete intrinsic head domain",
            ));
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("capture value has no actual code context"));
        };
        let function = self.inventory.function(target, b)?;
        b.spend(classes.len(), WORK)?;
        let mut out = Vec::with_capacity(classes.len());
        for expression in classes {
            out.push(match expression {
                SemanticClassExpr::OuterOccurrence {
                    operand: SemanticOperand::Value(value),
                    path,
                    mode: StoredBorrowMode::SharedAddress,
                } if path.is_empty() => {
                    self.heads(function, *value, None, &mut BTreeSet::new(), 0, b)?
                }
                SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Value(value),
                    path,
                    class,
                } if path.is_empty() => {
                    self.heads(function, *value, Some(*class), &mut BTreeSet::new(), 0, b)?
                }
                SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Value(value),
                    path,
                    class,
                } if path.as_slice() == [LifeTypeProjection::Pointee] => {
                    self.pointee_heads(function, *value, *class, &mut BTreeSet::new(), 0, b)?
                }
                SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Local(local),
                    path,
                    class,
                } if path.is_empty() => {
                    let slot = self.persistent_slot(instance, function, *local, b)?;
                    self.place_heads(
                        function,
                        *local,
                        slot.as_ref(),
                        &[],
                        false,
                        Some(*class),
                        at,
                        &mut BTreeSet::new(),
                        0,
                        b,
                    )?
                }
                _ => {
                    return Err(unknown(
                        "implicit closure image requires its actual emitted capture-head selector",
                    ));
                }
            });
        }
        Ok(out)
    }

    pub(super) fn match_heads(
        &self,
        instance: &SemanticInstance,
        expected: &[Heads],
        operand: &SemanticOperand,
        at: (BlockId, usize, SemanticSiteEdge),
        b: &mut Budget,
    ) -> CompileResult<()> {
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("capture value has no actual code context"));
        };
        let function = self.inventory.function(target, b)?;
        for (class, expected) in expected.iter().enumerate() {
            let actual = match operand {
                SemanticOperand::Value(value) => {
                    if matches!(function.values.get(value.0 as usize),
                        Some(Type::Address { inner, .. }) if matches!(inner.as_ref(), Type::Closure(_)))
                    {
                        // Select the Closure Pointee explicitly, not the
                        // Address outer class of this original receiver.
                        self.pointee_heads(
                            function,
                            *value,
                            class as u32,
                            &mut BTreeSet::new(),
                            0,
                            b,
                        )?
                    } else {
                        let slot = self.array_head_slot(instance, function, *value, b)?;
                        let source = slot.as_ref().map_or(*value, |slot| slot.initial());
                        self.heads(
                            function,
                            source,
                            Some(class as u32),
                            &mut BTreeSet::new(),
                            0,
                            b,
                        )?
                    }
                }
                SemanticOperand::Local(local) => {
                    let slot = self.persistent_slot(instance, function, *local, b)?;
                    self.place_heads(
                        function,
                        *local,
                        slot.as_ref(),
                        &[],
                        false,
                        Some(class as u32),
                        at,
                        &mut BTreeSet::new(),
                        0,
                        b,
                    )?
                }
                _ => {
                    return Err(unknown(
                        "implicit closure domain needs its exact actual value or occupant",
                    ));
                }
            };
            b.spend(actual.len().saturating_add(expected.len()), WORK)?;
            if &actual != expected {
                return Err(fail(
                    "closure intrinsic image belongs to another actual borrowed head",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn captured_value(
        &self,
        instance: &SemanticInstance,
        value: &SemanticTypeValue,
        operand: &SemanticOperand,
        at: (BlockId, usize, SemanticSiteEdge),
        declaration: bool,
        b: &mut Budget,
    ) -> CompileResult<Type> {
        let SemanticTypeValue::Resolved { ty, .. } = value else {
            return Err(fail("executable whole value retains a Core type"));
        };
        if self.uses_full_value(ty, b)? {
            return Ok(self
                .full_value_at(instance, value, operand, at, declaration, b)?
                .into_type(b)?);
        }
        let SemanticTypeValue::Resolved {
            ty: Type::Closure(id),
            ..
        } = value
        else {
            return self.value_type(value, b);
        };
        let images = self.image_heads(instance, value, at, b)?;
        if !declaration {
            self.match_heads(instance, &images, operand, at, b)?;
        }
        Ok(Type::Closure(*id))
    }
}
