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

impl Check<'_, '_> {
    pub(super) fn creator_heads(
        &self,
        instance: &SemanticInstance,
        closure: ClosureId,
        captures: &[ValueId],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if self.has_ghost_capture(closure, b)? {
            return self.ghost_creator(instance, closure, captures, b);
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail(
                "implicit captured head has no actual creator function",
            ));
        };
        let function = self.inventory.function(target, b)?;
        for index in self.capture_directory(closure, b)? {
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

    /// Fixed ordinal -> actual physical capture ordinal. Every row is derived
    /// from the original free-name occurrence and its real ReadCapture, never
    /// from the order or count of captured Types.
    pub(super) fn capture_directory(
        &self,
        id: ClosureId,
        b: &mut Budget,
    ) -> CompileResult<Vec<u32>> {
        if self.has_ghost_capture(id, b)? {
            return self.ghost_capture_directory(id, b);
        }
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        let closure = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|closure| closure.id == id)
            .ok_or_else(|| fail("implicit head directory names an absent closure"))?;
        let life = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("actual capture directory is Unknown"))?;
        let instance = self.function_instance(closure.function, b)?;
        let definition = self.inventory.definition(instance.definition, b)?;
        let function = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(closure.function), b)?;
        let SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. } = &definition.body
        else {
            return Err(fail("borrowed capture has no original closure body"));
        };
        if definition.kind != SemanticDefinitionKind::Closure
            || function.environment != Some(id)
            || life.invocation_binder_count != 0
            || !life.classes.is_empty()
            || !life.static_bindings.is_empty()
            || function.storage_life.as_ref() != Some(life)
            || !closure.lending_captures.is_empty()
        {
            return Err(unknown(
                "actual closure needs its full retained, lending or invocation context",
            ));
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
        for occurrence in &definition.implicit_lifetimes {
            let mut emitted = false;
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
                        return Err(fail("original implicit name has no actual emitted read"));
                    };
                    match self.instruction(function, value, b)? {
                        InstructionKind::ReadCapture { index }
                            if closure
                                .captures
                                .get(*index as usize)
                                .is_some_and(Type::is_b0_scalar) => {}
                        InstructionKind::ReadAddress { .. } => {
                            self.captured_read(function, value, b)?;
                        }
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local }
                            if function
                                .values
                                .get(value.0 as usize)
                                .is_some_and(Type::is_b0_scalar)
                                && function.locals.get(local.0 as usize).is_some_and(
                                    |declaration| {
                                        declaration.id == *local
                                            && function.values.get(value.0 as usize)
                                                == Some(&declaration.ty)
                                    },
                                ) =>
                        {
                            // A real scalar parameter/local read activates no
                            // persistent capture class. Fixed rows below still
                            // require their own exact original captured-head
                            // read, so this case cannot supply a head loan.
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
                return Err(fail(
                    "original implicit occurrence has no matching actual name site",
                ));
            }
        }
        let mut seen_capture = BTreeSet::new();
        for binding in &life.bindings {
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
        if instance.header_embedding.len() != directory.len() {
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
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn local_heads(
        &self,
        function: &TypedFunction,
        local: LocalId,
        class: Option<u32>,
        at: (BlockId, usize, SemanticSiteEdge),
        active: &mut BTreeSet<(FunctionId, ValueId, Option<u32>)>,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        if depth > b.partial_depth {
            return Err(unknown(
                "actual implicit local-head derivation exceeds its finite bound",
            ));
        }
        let initialization = crate::initialize(function, b)?;
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
                .filter(|actual| actual.id == block)
                .ok_or_else(|| fail("implicit local head has no actual CFG boundary"))?;
            let mut found = false;
            for (index, instruction) in actual.instructions.iter().enumerate().take(end).rev() {
                b.spend(1, WORK)?;
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
                        let sources = self.heads(function, value, class, active, depth + 1, b)?;
                        b.spend(out.len().saturating_add(sources.len()), WORK)?;
                        out.extend(sources);
                        found = true;
                        break;
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
            for predecessor in &function.blocks {
                b.spend(1, WORK)?;
                if !initialization.reachable[predecessor.id.0 as usize] {
                    continue;
                }
                if matches!(predecessor.terminator, Terminator::Jump(next) if next == block)
                    || matches!(predecessor.terminator, Terminator::Branch { then_block, else_block, .. } if then_block == block || else_block == block)
                {
                    b.spend(1, WORK)?;
                    pending.push((predecessor.id, predecessor.instructions.len()));
                }
            }
        }
        if out.is_empty() {
            return Err(fail(
                "implicit head has no actual reaching initialized local origin",
            ));
        }
        Ok(out)
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
        active: &mut BTreeSet<(FunctionId, ValueId, Option<u32>)>,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<Heads> {
        if depth > b.partial_depth {
            return Err(unknown(
                "actual implicit-head derivation exceeds its finite bound",
            ));
        }
        b.spend(active.len().saturating_add(1), WORK)?;
        let key = (function.id, value, class);
        if !active.insert(key) {
            return Err(fail("actual implicit-head source has an ungrounded cycle"));
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
            (InstructionKind::MakeClosure { closure, captures }, Some(class)) => {
                let directory = self.capture_directory(*closure, b)?;
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
        result
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
            crate::semantic::value::resolved(instance, value, self.inventory, b, 0)?;
            // No physical borrowed head exists in this independently checked
            // data-free directory. Its nonzero ghost domain is kept separately.
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
                    operand: SemanticOperand::Local(local),
                    path,
                    class,
                } if path.is_empty() => self.local_heads(
                    function,
                    *local,
                    Some(*class),
                    at,
                    &mut BTreeSet::new(),
                    0,
                    b,
                )?,
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
                SemanticOperand::Value(value) => self.heads(
                    function,
                    *value,
                    Some(class as u32),
                    &mut BTreeSet::new(),
                    0,
                    b,
                )?,
                SemanticOperand::Local(local) => self.local_heads(
                    function,
                    *local,
                    Some(class as u32),
                    at,
                    &mut BTreeSet::new(),
                    0,
                    b,
                )?,
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
                .ty);
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
