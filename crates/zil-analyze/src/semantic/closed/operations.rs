//! Check original recipes against each actual operation. Scalar equality and
//! captured-head relations never erase intrinsic classes or grant memory access.
use super::*;

struct Point<'a> {
    function: &'a TypedFunction,
    block: &'a BasicBlock,
    instruction: usize,
    edge: SemanticSiteEdge,
    initialization: &'a crate::Initialization,
}

pub(super) fn available_before(
    function: &TypedFunction,
    at: (BlockId, usize, SemanticSiteEdge),
    value: ValueId,
    b: &mut Budget,
) -> CompileResult<()> {
    let initialization = crate::initialize(function, b)?;
    let block = function
        .blocks
        .get(at.0.0 as usize)
        .filter(|block| block.id == at.0)
        .ok_or_else(|| fail("implicit read has no actual emission boundary"))?;
    Point {
        function,
        block,
        instruction: at.1,
        edge: at.2,
        initialization: &initialization,
    }
    .value(value, b)?;
    Ok(())
}

impl Point<'_> {
    fn local(&self, local: LocalId, b: &mut Budget) -> CompileResult<&Type> {
        b.spend(self.function.locals.len().saturating_add(1), WORK)?;
        let declaration = self
            .function
            .locals
            .get(local.0 as usize)
            .filter(|declaration| declaration.id == local)
            .ok_or_else(|| fail("semantic operand selects an absent actual local"))?;
        let plan = self
            .initialization
            .blocks
            .get(self.block.id.0 as usize)
            .ok_or_else(|| fail("semantic local has no actual initialization state"))?;
        b.spend(plan.initialized_in.len(), WORK)?;
        let mut initialized = plan.initialized_in.contains(&local);
        let end = self
            .instruction
            .saturating_add(usize::from(self.edge == SemanticSiteEdge::After));
        for instruction in self.block.instructions.iter().take(end) {
            b.spend(1, WORK)?;
            match &instruction.kind {
                InstructionKind::UnpackVariant {
                    local: source,
                    fields,
                    ..
                }
                | InstructionKind::UnpackTuple {
                    local: source,
                    fields,
                } => {
                    b.spend(fields.len(), WORK)?;
                    if *source == local {
                        initialized = false;
                    }
                    if fields.contains(&local) {
                        initialized = true;
                    }
                }
                kind => match crate::incremental::Operation::of(kind) {
                    crate::incremental::Operation::Store(actual) if actual == local => {
                        initialized = true
                    }
                    crate::incremental::Operation::Take(actual)
                    | crate::incremental::Operation::Kill(actual)
                        if actual == local =>
                    {
                        initialized = false
                    }
                    _ => {}
                },
            }
        }
        if !initialized {
            return Err(fail(
                "semantic local operand has no initialized occupant at its actual site",
            ));
        }
        Ok(&declaration.ty)
    }

    fn value(&self, value: ValueId, b: &mut Budget) -> CompileResult<&Type> {
        let ty = self
            .function
            .values
            .get(value.0 as usize)
            .ok_or_else(|| fail("semantic operand selects an absent actual SSA value"))?;
        let mut definition = None;
        for block in &self.function.blocks {
            b.spend(block.instructions.len().saturating_add(1), WORK)?;
            for (index, instruction) in block.instructions.iter().enumerate() {
                if instruction.result == Some(value)
                    && definition.replace((block.id, index)).is_some()
                {
                    return Err(fail(
                        "semantic SSA operand has competing actual definitions",
                    ));
                }
            }
        }
        let (block, ordinal) =
            definition.ok_or_else(|| fail("semantic SSA operand has no actual definition"))?;
        if block == self.block.id {
            if ordinal < self.instruction
                || (ordinal == self.instruction && self.edge == SemanticSiteEdge::After)
            {
                return Ok(ty);
            }
            return Err(fail(
                "semantic SSA operand is read before its actual definition",
            ));
        }
        // An actual path to this site which avoids the definition disproves
        // dominance. Walk real CFG edges; an asserted type is not a def/use edge.
        b.spend(self.function.blocks.len().saturating_add(1), WORK)?;
        let mut seen = BTreeSet::new();
        let mut pending = vec![self.function.entry];
        while let Some(current) = pending.pop() {
            b.spend(seen.len().saturating_add(1), WORK)?;
            if current == block || !seen.insert(current) {
                continue;
            }
            if current == self.block.id {
                return Err(fail(
                    "semantic SSA operand does not dominate its actual use",
                ));
            }
            let actual = self
                .function
                .blocks
                .get(current.0 as usize)
                .filter(|actual| actual.id == current)
                .ok_or_else(|| fail("semantic SSA dominance encounters an invalid actual block"))?;
            match actual.terminator {
                Terminator::Jump(next) => {
                    b.spend(1, WORK)?;
                    pending.push(next);
                }
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    b.spend(2, WORK)?;
                    pending.push(then_block);
                    pending.push(else_block);
                }
                _ => {}
            }
        }
        if !self
            .initialization
            .reachable
            .get(block.0 as usize)
            .copied()
            .unwrap_or(false)
        {
            return Err(fail(
                "semantic SSA definition is not on an actual reachable path",
            ));
        }
        Ok(ty)
    }
}

impl Check<'_, '_> {
    fn inputs(
        &self,
        arguments: &[ValueId],
        point: &Point<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let before = Point {
            function: point.function,
            block: point.block,
            instruction: point.instruction,
            edge: SemanticSiteEdge::Before,
            initialization: point.initialization,
        };
        b.spend(arguments.len(), WORK)?;
        for value in arguments {
            before.value(*value, b)?;
        }
        Ok(())
    }

    fn image_operands(
        &self,
        instance: &SemanticInstance,
        value: &SemanticTypeValue,
        point: &Point<'_>,
        result: Option<ValueId>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let SemanticTypeValue::Resolved { classes, .. } = value else {
            return Err(fail("class image retains an unresolved Core type"));
        };
        b.spend(classes.len(), WORK)?;
        for expression in classes {
            if let SemanticClassExpr::OuterOccurrence { operand, .. }
            | SemanticClassExpr::IntrinsicClass { operand, .. } = expression
            {
                match operand {
                    SemanticOperand::Value(value) if Some(*value) == result => {}
                    SemanticOperand::Value(value) => {
                        point.value(*value, b)?;
                    }
                    SemanticOperand::Local(local) => {
                        point.local(*local, b)?;
                    }
                    SemanticOperand::Environment(_) | SemanticOperand::TypeArgument { .. }
                        if matches!(expression, SemanticClassExpr::IntrinsicClass { .. }) =>
                    {
                        // This selector reads a complete checked logical type
                        // in the real creator/formal scope; it is not a head
                        // permission. The subject below is still matched to
                        // its actual ReadCapture/SSA/initialized occupant, and
                        // every creator separately proves its real captures.
                        crate::semantic::value::class(instance, expression, self.inventory, b, 0)?;
                    }
                    _ => {
                        return Err(unknown(
                            "capture class image needs its actual emitted value or initialized occupant",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn operand(
        &self,
        instance: &SemanticInstance,
        operand: &SemanticOperand,
        point: &Point<'_>,
        b: &mut Budget,
    ) -> CompileResult<Type> {
        b.spend(1, WORK)?;
        let ty = match operand {
            SemanticOperand::Value(value) => point.value(*value, b)?,
            SemanticOperand::Local(local) => point.local(*local, b)?,
            SemanticOperand::Parameter(index) => {
                let local = point
                    .function
                    .parameters
                    .get(*index as usize)
                    .ok_or_else(|| {
                        fail("semantic parameter operand is outside its original header")
                    })?;
                point.local(*local, b)?
            }
            SemanticOperand::Capture(index) => {
                let closure = point
                    .function
                    .environment
                    .ok_or_else(|| fail("semantic capture operand has no actual environment"))?;
                self.module
                    .closures
                    .get(closure.0 as usize)
                    .filter(|definition| definition.id == closure)
                    .and_then(|definition| definition.captures.get(*index as usize))
                    .ok_or_else(|| {
                        fail("semantic capture operand is outside its actual environment")
                    })?
            }
            SemanticOperand::TypeArgument { scope, ordinal } => {
                let context = self.ancestor(instance, *scope, b)?;
                return match context.actuals.get(*ordinal as usize) {
                    Some(SemanticActual::Type(value)) => self.value_type(value, b),
                    _ => Err(fail(
                        "semantic type argument operand changes its original mixed kind",
                    )),
                };
            }
            SemanticOperand::Environment(ordinal) => {
                return match instance
                    .environment
                    .as_ref()
                    .and_then(|environment| environment.actuals.get(*ordinal as usize))
                {
                    Some(SemanticActual::Type(value)) => self.value_type(value, b),
                    _ => Err(fail("semantic environment operand has no sealed type slot")),
                };
            }
            _ => {
                return Err(unknown(
                    "semantic receiver or unresolved operand needs its complete actual context",
                ));
            }
        };
        self.ordinary_type(ty, b)?;
        Ok(ty.clone())
    }

    pub(super) fn result(
        &self,
        instruction: &Instruction,
        function: &TypedFunction,
        expected: &Type,
    ) -> CompileResult<()> {
        match (instruction.result, expected) {
            (None, Type::Unit) => Ok(()),
            (Some(value), _) if function.values.get(value.0 as usize) == Some(expected) => Ok(()),
            _ => Err(fail(
                "actual call or constructor result differs from its original instantiated type",
            )),
        }
    }

    fn arguments(
        &self,
        function: &TypedFunction,
        arguments: &[ValueId],
        expected: &[Type],
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(arguments.len().saturating_add(expected.len()), WORK)?;
        if arguments.len() != expected.len() {
            return Err(fail("actual call changes its original argument arity"));
        }
        for (actual, expected) in arguments.iter().zip(expected) {
            if function.values.get(actual.0 as usize) != Some(expected) {
                return Err(fail(
                    "actual call argument differs from the complete instantiated parameter",
                ));
            }
        }
        Ok(())
    }

    fn operation(
        &self,
        instance: &SemanticInstance,
        site: &SemanticSite,
        recipe: &SemanticSiteRecipe,
        point: &Point<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let instruction = point.block.instructions.get(point.instruction);
        match (&recipe.kind, &site.action) {
            (
                SemanticSiteRecipeKind::Construct { result: recipe },
                SemanticSiteAction::Construct {
                    result: SemanticOperand::Value(result),
                    value,
                },
            ) => {
                let expected = self.ty(instance, recipe, b, 0)?;
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                if self.uses_full_value(&original_value.ty, b)?
                    && self.resolved(instance, value, b, 0)? != original_value
                {
                    return Err(fail(
                        "actual semantic claim changes its complete original class value",
                    ));
                }

                self.image_operands(instance, value, point, Some(*result), b)?;
                if self.captured_value(
                    instance,
                    value,
                    &SemanticOperand::Value(*result),
                    (point.block.id, point.instruction, SemanticSiteEdge::After),
                    false,
                    b,
                )? != expected
                    || instruction.and_then(|actual| actual.result) != Some(*result)
                {
                    return Err(fail(
                        "actual construction changes its original whole type or result",
                    ));
                }
                self.result(
                    instruction.ok_or_else(|| fail("actual construction instruction is absent"))?,
                    point.function,
                    &expected,
                )?;
            }
            (
                SemanticSiteRecipeKind::TypeUse { expected: recipe },
                SemanticSiteAction::TypeUse { subject, expected },
            ) => {
                let expected_type = self.ty(instance, recipe, b, 0)?;
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                if self.uses_full_value(&original_value.ty, b)?
                    && self.resolved(instance, expected, b, 0)? != original_value
                {
                    return Err(fail(
                        "actual semantic claim changes its complete original class value",
                    ));
                }

                self.image_operands(instance, expected, point, None, b)?;
                self.function_reference(
                    instance,
                    site.recipe,
                    point.function,
                    subject,
                    &expected_type,
                    b,
                )?;
                if let Some(constructor) = self.original.enum_constructor(
                    instance,
                    site.recipe,
                    self.program,
                    self.inventory,
                    b,
                )? && constructor.value_member
                {
                    let SemanticOperand::Value(value) = subject else {
                        return Err(fail("original enum member has no emitted SSA"));
                    };
                    let (_, _, actual) = self.defining(point.function, *value, b)?;
                    if !constructor.payload.is_empty()
                        || constructor.result.ty != expected_type
                        || !matches!(&actual.kind, InstructionKind::MakeVariant { variant, fields }
                            if *variant == constructor.variant && fields.is_empty())
                    {
                        return Err(fail(
                            "actual enum member changes its original declaration, variant or payload",
                        ));
                    }
                }
                if let Some(bytes) =
                    self.original
                        .string_literal_at(instance, site.recipe, self.inventory, b)?
                {
                    let SemanticOperand::Value(value) = subject else {
                        return Err(fail("original string literal has no emitted SSA"));
                    };
                    let (_, _, literal) = self.defining(point.function, *value, b)?;
                    let InstructionKind::StaticUtf8 { bytes: actual } = &literal.kind else {
                        return Err(fail(
                            "original string literal is replaced by another actual operation",
                        ));
                    };
                    b.spend(bytes.len().saturating_add(actual.len() + 1), WORK)?;
                    if actual.as_slice() != bytes {
                        return Err(fail(
                            "actual UTF8 bytes differ from the complete original literal",
                        ));
                    }
                }
                if expected_type
                    != self.captured_value(
                        instance,
                        expected,
                        subject,
                        (point.block.id, point.instruction, point.edge),
                        false,
                        b,
                    )?
                    || self.operand(instance, subject, point, b)? != expected_type
                {
                    return Err(fail(
                        "actual type-use occupant differs from its complete original ascription",
                    ));
                }
            }
            (
                SemanticSiteRecipeKind::DeclareLocal { expected: recipe },
                SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                },
            ) => {
                let expected_type = self.ty(instance, recipe, b, 0)?;
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                if self.uses_full_value(&original_value.ty, b)?
                    && self.resolved(instance, expected, b, 0)? != original_value
                {
                    return Err(fail(
                        "actual semantic claim changes its complete original class value",
                    ));
                }

                self.image_operands(instance, expected, point, None, b)?;
                if self.captured_value(
                    instance,
                    expected,
                    &SemanticOperand::Local(*local),
                    (point.block.id, point.instruction, point.edge),
                    true,
                    b,
                )? != expected_type
                    || point
                        .function
                        .locals
                        .get(local.0 as usize)
                        .filter(|actual| actual.id == *local)
                        .map(|actual| &actual.ty)
                        != Some(&expected_type)
                    || !matches!(instruction.map(|actual| &actual.kind), Some(InstructionKind::StorageLive(actual)) if actual == local)
                {
                    return Err(fail(
                        "actual local declaration changes its complete original constraint",
                    ));
                }
            }
            (SemanticSiteRecipeKind::OriginalCall, SemanticSiteAction::BodyCall { actuals }) => {
                if self
                    .code_operation(
                        instance,
                        site.recipe,
                        point.function,
                        point.block.id,
                        point.instruction,
                        actuals,
                        b,
                    )?
                    .is_some()
                {
                    return Ok(());
                }
                if self
                    .trait_operation(
                        instance,
                        site.recipe,
                        point.function,
                        point.block.id,
                        point.instruction,
                        actuals,
                        b,
                    )?
                    .is_some()
                {
                    return Ok(());
                }
                if self
                    .abstract_operation(
                        instance,
                        point.function,
                        point.block.id,
                        point.instruction,
                        b,
                    )?
                    .is_some()
                {
                    return Ok(());
                }
                if let Some(constructor) = self.original.enum_constructor(
                    instance,
                    site.recipe,
                    self.program,
                    self.inventory,
                    b,
                )? {
                    let instruction = instruction.ok_or_else(|| {
                        fail("original enum construction has no actual instruction")
                    })?;
                    let InstructionKind::MakeVariant { variant, fields } = &instruction.kind else {
                        return Err(fail(
                            "original enum constructor is replaced by another opcode",
                        ));
                    };
                    if *variant != constructor.variant
                        || fields.len() != constructor.payload.len()
                        || actuals.len() != constructor.arguments.len()
                    {
                        return Err(fail(
                            "actual enum constructor changes its original member or full arity",
                        ));
                    }
                    self.inputs(fields, point, b)?;
                    for (actual, expected) in fields.iter().zip(&constructor.payload) {
                        b.spend(1, WORK)?;
                        if point.function.values.get(actual.0 as usize) != Some(&expected.ty) {
                            return Err(fail(
                                "actual enum payload differs from its complete original member type",
                            ));
                        }
                        if self.uses_full_value(&expected.ty, b)? {
                            let value = self.ssa_value(instance, point.function, *actual, b, 0)?;
                            if value != *expected {
                                return Err(fail(
                                    "actual enum payload changes its original intrinsic class occurrence",
                                ));
                            }
                        }
                    }
                    for (actual, expected) in actuals.iter().zip(&constructor.arguments) {
                        if self.actual_value(instance, actual, b, 0)? != *expected {
                            return Err(fail(
                                "actual enum construction changes its ordered original mixed arguments",
                            ));
                        }
                    }
                    self.result(instruction, point.function, &constructor.result.ty)?;
                    return Ok(());
                }
                if let Some(intrinsic) =
                    self.original
                        .intrinsic(instance, site.recipe, self.inventory, b)?
                {
                    match intrinsic {
                        original::sealed::Intrinsic::CCallback => {
                            let outputs = self.original.intrinsic_inputs(
                                instance,
                                site.recipe,
                                self.program,
                                self.inventory,
                                b,
                            )?;
                            let [output] = outputs.as_slice() else {
                                return Err(fail(
                                    "original callback conversion loses its complete output scheme",
                                ));
                            };
                            if !actuals.is_empty()
                                || !matches!(output.ty, Type::ForeignCallback { .. })
                                || !empty(&output.life)
                                || !output.classes.is_empty()
                            {
                                return Err(fail(
                                    "actual callback conversion changes its original fixed ABI scheme",
                                ));
                            }
                            let instruction = instruction.ok_or_else(|| {
                                fail("original callback conversion has no actual instruction")
                            })?;
                            let result = instruction.result.ok_or_else(|| {
                                fail("actual callback conversion has no code SSA")
                            })?;
                            self.function_reference(
                                instance,
                                site.recipe,
                                point.function,
                                &SemanticOperand::Value(result),
                                &output.ty,
                                b,
                            )?;
                            self.result(instruction, point.function, &output.ty)?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::StoreNew
                        | original::sealed::Intrinsic::StoreReserve
                        | original::sealed::Intrinsic::StorePush
                        | original::sealed::Intrinsic::StorePop
                        | original::sealed::Intrinsic::StoreLen
                        | original::sealed::Intrinsic::StoreCapacity
                        | original::sealed::Intrinsic::StoreAt
                        | original::sealed::Intrinsic::StoreAtMut => {
                            self.store_operation(
                                instance,
                                site.recipe,
                                intrinsic,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::TaskYield => {
                            let instruction = instruction.ok_or_else(|| {
                                fail("original task_yield has no actual instruction")
                            })?;
                            if !actuals.is_empty()
                                || point.function.call_kind != CallKind::Suspend
                                || !matches!(instruction.kind, InstructionKind::TaskYield)
                            {
                                return Err(fail(
                                    "actual operation changes the original task_yield scheme or Suspend ABI",
                                ));
                            }
                            self.result(instruction, point.function, &Type::Unit)?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::TaskCreate
                        | original::sealed::Intrinsic::TaskPoll
                        | original::sealed::Intrinsic::TaskCancel
                        | original::sealed::Intrinsic::TaskPollClose
                        | original::sealed::Intrinsic::TaskFinish) => {
                            let output = self.task_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual Task operation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::MutexCreate
                        | original::sealed::Intrinsic::MutexLock
                        | original::sealed::Intrinsic::MutexValue
                        | original::sealed::Intrinsic::ThreadScope) => {
                            let output = self.mutex_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual synchronization operation has no instruction")
                                })?,
                                point.function,
                                &output.ty,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::BytesNew
                        | original::sealed::Intrinsic::BytesReserve
                        | original::sealed::Intrinsic::BytesPush
                        | original::sealed::Intrinsic::BytesTruncate
                        | original::sealed::Intrinsic::BytesLen
                        | original::sealed::Intrinsic::BytesCapacity) => {
                            let output = self.buffer_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual ByteBuffer operation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::Register
                        | original::sealed::Intrinsic::RegisterAsync) => {
                            let output = self.registration_start_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual registration installation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        id @ (original::sealed::Intrinsic::AsyncRequest
                        | original::sealed::Intrinsic::AsyncPoll
                        | original::sealed::Intrinsic::AsyncFinish
                        | original::sealed::Intrinsic::AsyncFinishChecked
                        | original::sealed::Intrinsic::AsyncValid
                        | original::sealed::Intrinsic::RegistrationValid
                        | original::sealed::Intrinsic::RegistrationClose) => {
                            let output = self.registration_operation(
                                instance,
                                site.recipe,
                                id,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual registration operation has no instruction")
                                })?,
                                point.function,
                                &output,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::SliceLength => {
                            self.slice_length_operation(
                                instance,
                                site.recipe,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            return Ok(());
                        }
                        original::sealed::Intrinsic::NumericConvert(number) => {
                            let instruction = instruction.ok_or_else(|| {
                                fail("original conversion has no actual instruction")
                            })?;
                            let (value, target, integer) = match &instruction.kind {
                                InstructionKind::ConvertNumber { value, target } => {
                                    (value, target, false)
                                }
                                InstructionKind::ConvertInteger { value, target } => {
                                    (value, target, true)
                                }
                                _ => {
                                    return Err(fail(
                                        "original numeric conversion is replaced by another actual opcode",
                                    ));
                                }
                            };
                            if !actuals.is_empty() || *target != number.ty() {
                                return Err(fail(
                                    "actual numeric conversion changes its closed target or generic arity",
                                ));
                            }
                            self.inputs(&[*value], point, b)?;
                            let expected = self.original.intrinsic_inputs(
                                instance,
                                site.recipe,
                                self.program,
                                self.inventory,
                                b,
                            )?;
                            if expected.len() != 1
                                || point.function.values.get(value.0 as usize)
                                    != Some(&expected[0].ty)
                            {
                                return Err(fail(
                                    "actual numeric input differs from its checked original numeric carrier",
                                ));
                            }
                            let input =
                                point.function.values.get(value.0 as usize).ok_or_else(|| {
                                    fail("actual numeric conversion input is absent")
                                })?;
                            if !matches!(
                                input,
                                Type::I8
                                    | Type::I16
                                    | Type::I32
                                    | Type::I64
                                    | Type::Isize
                                    | Type::U8
                                    | Type::U16
                                    | Type::U32
                                    | Type::U64
                                    | Type::Usize
                                    | Type::F32
                                    | Type::F64
                            ) {
                                return Err(fail(
                                    "actual numeric conversion input violates its original numeric premise",
                                ));
                            }
                            if integer
                                && (matches!(input, Type::F32 | Type::F64)
                                    || matches!(target, Type::F32 | Type::F64))
                            {
                                return Err(fail(
                                    "actual integer conversion changes the original mathematical carrier family",
                                ));
                            }
                            self.result(instruction, point.function, &number.ty())?;
                            return Ok(());
                        }
                        _ => {
                            return Err(unknown(
                                "actual core operation lacks its complete original scheme proof",
                            ));
                        }
                    }
                }
                let (original, closure, expected_actuals) =
                    self.original
                        .call(instance, site.recipe, self.program, self.inventory, b)?;
                let instruction = instruction
                    .ok_or_else(|| fail("original BodyCall has no actual instruction"))?;
                let (callee, arguments, parameters, result) = match &instruction.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    }
                    | InstructionKind::SuspendCall {
                        function,
                        arguments,
                    }
                    | InstructionKind::ForeignCall {
                        function,
                        arguments,
                        ..
                    } if !closure => {
                        let callee = self
                            .module
                            .functions
                            .get(function.0 as usize)
                            .filter(|callee| callee.id == *function)
                            .ok_or_else(|| fail("actual original callee function is absent"))?;
                        let declaration = self.inventory.definition(original, b)?;
                        let SemanticDefinitionBody::FunctionWithBody {
                            signature, body, ..
                        } = &declaration.body
                        else {
                            return Err(fail(
                                "actual static call has no independently checked original scheme",
                            ));
                        };
                        let valid = match (&instruction.kind, body) {
                            (
                                InstructionKind::ForeignCall { .. },
                                SemanticOriginalFunctionBody::External,
                            ) => {
                                signature.call_kind == CallKind::Direct
                                    && callee.call_kind == CallKind::Direct
                                    && callee.foreign.is_some()
                            }
                            (
                                InstructionKind::SuspendCall { .. },
                                SemanticOriginalFunctionBody::Defined(_),
                            ) => {
                                signature.call_kind == CallKind::Suspend
                                    && callee.call_kind == CallKind::Suspend
                                    && callee.foreign.is_none()
                                    && point.function.call_kind == CallKind::Suspend
                            }
                            (
                                InstructionKind::Call { .. },
                                SemanticOriginalFunctionBody::Defined(_),
                            ) => {
                                signature.call_kind == CallKind::Direct
                                    && callee.call_kind == CallKind::Direct
                                    && callee.foreign.is_none()
                            }
                            _ => false,
                        };
                        if !valid {
                            return Err(fail(
                                "actual static call changes its original foreign or Suspend ABI role",
                            ));
                        }
                        let mut parameters = Vec::new();
                        for parameter in &callee.parameters {
                            b.spend(callee.locals.len().saturating_add(1), WORK)?;
                            let ty = &callee
                                .locals
                                .get(parameter.0 as usize)
                                .filter(|local| local.id == *parameter)
                                .ok_or_else(|| fail("actual original callee parameter is absent"))?
                                .ty;
                            crate::dyn_metadata::ty(ty, b)?;
                            parameters.push(ty.clone());
                        }
                        (*function, arguments, parameters, &callee.return_type)
                    }
                    InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureOnce { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                        if closure =>
                    {
                        let (operand, arguments) = match &instruction.kind {
                            InstructionKind::CallClosure { callee, arguments }
                            | InstructionKind::CallClosureOnce { callee, arguments } => {
                                (SemanticOperand::Value(*callee), arguments)
                            }
                            InstructionKind::CallClosureShared { callee, arguments }
                            | InstructionKind::CallClosureMut { callee, arguments } => {
                                (SemanticOperand::Local(*callee), arguments)
                            }
                            _ => {
                                return Err(fail(
                                    "actual closure call changed during original resolution",
                                ));
                            }
                        };
                        let Type::Closure(id) = self.operand(instance, &operand, point, b)? else {
                            return Err(fail(
                                "original closure callee has another complete actual type",
                            ));
                        };
                        let callee = self
                            .module
                            .closures
                            .get(id.0 as usize)
                            .filter(|callee| callee.id == id)
                            .ok_or_else(|| fail("original closure call has no actual entry"))?;
                        b.spend(callee.parameters.len(), WORK)?;
                        for parameter in &callee.parameters {
                            crate::dyn_metadata::ty(parameter, b)?;
                        }
                        (
                            callee.function,
                            arguments,
                            callee.parameters.clone(),
                            &callee.result,
                        )
                    }
                    _ => {
                        return Err(unknown(
                            "original body call needs its full independent dynamic, foreign or suspend boundary",
                        ));
                    }
                };
                let target = self.function_instance(callee, b)?;
                if target.definition != original
                    || actuals.len() != expected_actuals.len()
                    || target.actuals.len() != expected_actuals.len()
                {
                    return Err(fail(
                        "actual BodyCall changes its independently inferred original target or mixed arity",
                    ));
                }
                for (ordinal, ((proposed, callee), expected)) in actuals
                    .iter()
                    .zip(&target.actuals)
                    .zip(&expected_actuals)
                    .enumerate()
                {
                    let proposed = self.actual_value(instance, proposed, b, 0)?;
                    if proposed != *expected {
                        return Err(fail(
                            "actual BodyCall differs from original caller-domain mixed inference",
                        ));
                    }
                    let callee = self.actual_value(target, callee, b, 0)?;
                    let valid = match (expected, callee) {
                        (
                            crate::semantic::value::Argument::Life(_),
                            crate::semantic::value::Argument::Life(
                                crate::semantic::value::Class::Formal(owner, index),
                            ),
                        ) => owner == target.id && index as usize == ordinal,
                        (
                            crate::semantic::value::Argument::Type(expected),
                            crate::semantic::value::Argument::Type(actual),
                        ) => actual.ty == expected.ty && actual.life == expected.life,
                        (
                            crate::semantic::value::Argument::Const {
                                carrier: expected,
                                value,
                            },
                            crate::semantic::value::Argument::Const {
                                carrier: actual,
                                value: actual_value,
                            },
                        ) => *expected == actual && *value == actual_value,
                        _ => false,
                    };
                    if !valid {
                        return Err(fail(
                            "actual BodyCall target changes its independent original rigid mixed context",
                        ));
                    }
                }
                self.arguments(point.function, arguments, &parameters, b)?;
                self.inputs(arguments, point, b)?;
                let (input_target, expected_inputs) = self.original.call_inputs(
                    instance,
                    site.recipe,
                    self.program,
                    self.inventory,
                    b,
                )?;
                if input_target != original || expected_inputs.len() != arguments.len() {
                    return Err(fail(
                        "actual call loses its independently inferred original input roles",
                    ));
                }
                for (actual, expected) in arguments.iter().zip(&expected_inputs) {
                    if self.uses_full_value(&expected.ty, b)? {
                        let actual = self.ssa_value(instance, point.function, *actual, b, 0)?;
                        let valid = if matches!(expected.ty, Type::Address { .. }) {
                            self.outer_role(instance, point.function, &actual, expected, b, 0)?
                        } else {
                            actual == *expected
                        };
                        if !valid {
                            return Err(fail(
                                "actual call input differs from its complete original role",
                            ));
                        }
                    }
                }
                self.result(instruction, point.function, result)?;
            }
            (
                SemanticSiteRecipeKind::Call {
                    target,
                    actuals: recipes,
                },
                SemanticSiteAction::Call {
                    target: actual_target,
                    actuals,
                },
            ) => {
                let callee = self.inventory.function(actual_target, b)?;
                let actual_instance = self.function_instance(callee.id, b)?;
                if actual_instance.definition != *target
                    || callee.environment.is_some()
                    || !self.actual_vectors(instance, recipes, actuals, b, 0)?
                    || !self.actual_vectors(instance, recipes, &actual_instance.actuals, b, 0)?
                {
                    return Err(fail(
                        "actual call changes its original definition or full mixed substitution",
                    ));
                }
                let instruction =
                    instruction.ok_or_else(|| fail("actual call instruction is absent"))?;
                let InstructionKind::Call {
                    function,
                    arguments,
                } = &instruction.kind
                else {
                    return Err(unknown(
                        "suspend and foreign semantic calls require their full independent boundary contracts",
                    ));
                };
                if *function != callee.id {
                    return Err(fail("original call does not select the actual callee"));
                }
                b.spend(callee.parameters.len(), WORK)?;
                let mut expected = Vec::with_capacity(callee.parameters.len());
                for local in &callee.parameters {
                    expected.push(
                        callee
                            .locals
                            .get(local.0 as usize)
                            .ok_or_else(|| fail("actual callee parameter is absent"))?
                            .ty
                            .clone(),
                    );
                }
                self.arguments(point.function, arguments, &expected, b)?;
                self.inputs(arguments, point, b)?;
                self.result(instruction, point.function, &callee.return_type)?;
            }
            (
                SemanticSiteRecipeKind::CallableCall { callee: recipe },
                SemanticSiteAction::CallableCall { callee },
            ) => {
                let expected = self.ty(instance, recipe, b, 0)?;
                if expected != self.operand(instance, callee, point, b)? {
                    return Err(fail(
                        "actual callable operand changes its original whole type",
                    ));
                }
                let Type::Closure(id) = expected else {
                    return Err(unknown(
                        "semantic code-pointer calls need the complete scoped signature proof",
                    ));
                };
                let definition = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|definition| definition.id == id)
                    .ok_or_else(|| fail("actual callable has no exact environment definition"))?;
                let instruction =
                    instruction.ok_or_else(|| fail("actual callable instruction is absent"))?;
                let (operand, arguments) = match &instruction.kind {
                    InstructionKind::CallClosure { callee, arguments }
                    | InstructionKind::CallClosureOnce { callee, arguments } => {
                        (SemanticOperand::Value(*callee), arguments)
                    }
                    InstructionKind::CallClosureShared { callee, arguments }
                    | InstructionKind::CallClosureMut { callee, arguments } => {
                        (SemanticOperand::Local(*callee), arguments)
                    }
                    _ => {
                        return Err(unknown(
                            "semantic borrowed callable needs its complete actual head relation",
                        ));
                    }
                };
                if *callee != operand {
                    return Err(fail("callable recipe selects another actual operand"));
                }
                self.arguments(point.function, arguments, &definition.parameters, b)?;
                self.inputs(arguments, point, b)?;
                self.result(instruction, point.function, &definition.result)?;
            }
            (
                SemanticSiteRecipeKind::Closure {
                    definition,
                    environment: recipes,
                },
                SemanticSiteAction::Closure {
                    result: SemanticOperand::Value(result),
                    closure: SemanticClosureTarget::Typed(closure),
                    environment,
                },
            ) => {
                let actual = self
                    .module
                    .closures
                    .get(closure.0 as usize)
                    .filter(|actual| actual.id == *closure)
                    .ok_or_else(|| fail("actual closure construction has no definition"))?;
                let child = self.function_instance(actual.function, b)?;
                let closed = child
                    .environment
                    .as_ref()
                    .ok_or_else(|| fail("actual closure has no creator-sealed environment"))?;
                if child.definition != *definition
                    || child.parent != Some(instance.id)
                    || closed.closure != SemanticClosureTarget::Typed(*closure)
                    || !child.actuals.is_empty()
                    || !self.actual_vectors(instance, recipes, environment, b, 0)?
                    || !self.actual_vectors(instance, recipes, &closed.actuals, b, 0)?
                {
                    return Err(fail(
                        "closure construction changes its original definition, creator or environment",
                    ));
                }
                let instruction =
                    instruction.ok_or_else(|| fail("actual closure constructor is absent"))?;
                let InstructionKind::MakeClosure {
                    closure: id,
                    captures,
                } = &instruction.kind
                else {
                    return Err(fail("closure ghost site is not an actual MakeClosure"));
                };
                if id != closure || instruction.result != Some(*result) {
                    return Err(fail(
                        "closure ghost site changes its actual constructed environment",
                    ));
                }
                self.arguments(point.function, captures, &actual.captures, b)?;
                self.inputs(captures, point, b)?;
                self.creator_heads(instance, *closure, captures, b)?;
                self.result(instruction, point.function, &Type::Closure(*closure))?;
            }
            _ => {
                return Err(fail(
                    "actual semantic action changes its original operation recipe",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn sites(&self, b: &mut Budget) -> CompileResult<()> {
        for instance in &self.program.instances {
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                continue;
            };
            let function = self.inventory.function(target, b)?;
            let definition = self.inventory.definition(instance.definition, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                sites: recipes,
                body,
                foreign,
                ..
            } = &definition.body
            else {
                return Err(fail("original function body is absent"));
            };
            if let Some(actual) = &function.foreign {
                b.spend(
                    recipes.len().saturating_add(self.program.sites.len() + 1),
                    WORK,
                )?;
                if !matches!(body, SemanticOriginalFunctionBody::External)
                    || foreign.as_ref() != Some(actual)
                    || function.call_kind != CallKind::Direct
                    || function.environment.is_some()
                    || !function.blocks.is_empty()
                    || !recipes.is_empty()
                    || self
                        .program
                        .sites
                        .iter()
                        .any(|site| site.instance == instance.id)
                {
                    return Err(fail(
                        "foreign declaration changes its original external body or emits runtime sites",
                    ));
                }
                // instances() has already checked the exact original ABI,
                // promises, full P/R and class header. External declarations
                // have no executable CFG to initialize here.
                continue;
            }
            let initialization = crate::initialize(function, b)?;
            let mut declared = BTreeMap::new();
            let mut declared_values = BTreeMap::new();
            for site in &self.program.sites {
                b.spend(1, WORK)?;
                if site.instance != instance.id {
                    continue;
                }
                let SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                } = &site.action
                else {
                    continue;
                };
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } = site.at
                else {
                    return Err(fail("declaration snapshot retains a Core emission site"));
                };
                let images =
                    self.image_heads(instance, expected, (block, instruction as usize, edge), b)?;
                if let SemanticTypeValue::Resolved { ty, .. } = expected {
                    if self.uses_full_value(ty, b)? {
                        let value = self.resolved(instance, expected, b, 0)?;
                        b.lookup(declared_values.len(), WORK)?;
                        if declared_values.insert(local, value).is_some() {
                            return Err(unknown(
                                "repeated semantic local activation needs its full class partition",
                            ));
                        }
                    }
                }
                b.spend(declared.len().saturating_add(1), WORK)?;
                if declared.insert(*local, images).is_some() {
                    return Err(unknown(
                        "repeated local declarations need their full original activation partition",
                    ));
                }
            }
            for site in &self.program.sites {
                b.spend(1, WORK)?;
                if site.instance != instance.id {
                    continue;
                }
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } = site.at
                else {
                    return Err(fail("actual semantic site retains a Core address"));
                };
                let block = function
                    .blocks
                    .get(block.0 as usize)
                    .filter(|actual| actual.id == block)
                    .ok_or_else(|| fail("semantic operation has no actual block"))?;
                if !initialization.reachable[block.id.0 as usize] {
                    return Err(fail("semantic operation has no reachable actual site"));
                }
                b.spend(recipes.len(), WORK)?;
                let recipe = recipes
                    .iter()
                    .find(|recipe| recipe.id == site.recipe)
                    .ok_or_else(|| fail("actual semantic action has no original recipe"))?;
                self.operation(
                    instance,
                    site,
                    recipe,
                    &Point {
                        function,
                        block,
                        instruction: instruction as usize,
                        edge,
                        initialization: &initialization,
                    },
                    b,
                )?;
            }
            let original_nodes = super::super::original_index::index(definition, b)?;
            for recipe in recipes {
                b.spend(self.program.sites.len(), WORK)?;
                if !self
                    .program
                    .sites
                    .iter()
                    .any(|site| site.instance == instance.id && site.recipe == recipe.id)
                {
                    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } =
                        &definition.body
                    else {
                        return Err(fail("emission coverage loses its original body"));
                    };
                    let mut selected = None;
                    for link in site_nodes {
                        b.spend(1, WORK)?;
                        if link.site == recipe.id {
                            let node = match link.node {
                                SemanticOriginalNode::Expression(node)
                                | SemanticOriginalNode::Statement(node) => node,
                            };
                            if selected.replace(node).is_some() {
                                return Err(fail(
                                    "emission coverage has duplicate original node links",
                                ));
                            }
                        }
                    }
                    let node = selected
                        .ok_or_else(|| fail("emission recipe has no original operation node"))?;
                    // A raw source stage is proved universally as well. Its
                    // selected CT execution is checked in the frozen context;
                    // it is not an extra executable operation in this runtime.
                    if original_nodes.erased(node, b)? {
                        continue;
                    }
                    if let SemanticSiteRecipeKind::TypeUse { expected } = &recipe.kind
                        && original_nodes.navigation(node, b)?
                    {
                        let value = self.recipe_value(instance, expected, b, 0)?;
                        self.actual_type(&value.ty, b)?;
                        continue;
                    }
                    let mut errors = fail("original emission recipe has no actual operation");
                    errors[0].notes.push(format!(
                        "original module {} definition {} recipe {} node {}",
                        definition.id.module.0, definition.id.index, recipe.id.0, node.0
                    ));
                    return Err(errors);
                }
            }
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    b.spend(self.program.sites.len().saturating_add(1), WORK)?;
                    if let InstructionKind::Store { local, value } = instruction.kind {
                        if !function
                            .locals
                            .get(local.0 as usize)
                            .is_some_and(|declaration| {
                                declaration.id == local
                                    && function.values.get(value.0 as usize)
                                        == Some(&declaration.ty)
                            })
                        {
                            return Err(fail(
                                "actual stored occupant changes its complete declared type",
                            ));
                        }
                        b.lookup(declared_values.len(), WORK)?;
                        if let Some(expected) = declared_values.get(&local) {
                            let actual = self.ssa_value(instance, function, value, b, 0)?;
                            if actual != *expected {
                                return Err(fail(
                                    "actual Store changes its complete original local class constraint",
                                ));
                            }
                        }
                        b.spend(declared.len().saturating_add(1), WORK)?;
                        if let Some(images) = declared.get(&local) {
                            self.match_heads(
                                instance,
                                images,
                                &SemanticOperand::Value(value),
                                (block.id, index, SemanticSiteEdge::Before),
                                b,
                            )?;
                        }
                    }
                    let requires = match instruction.kind {
                        InstructionKind::OwnedStoreNew { .. }
                        | InstructionKind::OwnedStoreReserve { .. }
                        | InstructionKind::OwnedStorePush { .. }
                        | InstructionKind::OwnedStorePop { .. }
                        | InstructionKind::OwnedStoreLen { .. }
                        | InstructionKind::OwnedStoreCapacity { .. }
                        | InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. } => Some(3),
                        InstructionKind::Call { .. }
                        | InstructionKind::SuspendCall { .. }
                        | InstructionKind::ForeignCall { .. } => Some(0),
                        InstructionKind::TaskYield
                        | InstructionKind::TaskCreate { .. }
                        | InstructionKind::TaskCreateOwned { .. }
                        | InstructionKind::TaskPoll { .. }
                        | InstructionKind::TaskRequestCancel { .. }
                        | InstructionKind::TaskPollClose { .. }
                        | InstructionKind::TaskFinishDestroy { .. }
                        | InstructionKind::MutexCreate { .. }
                        | InstructionKind::MutexLock { .. }
                        | InstructionKind::MutexValue { .. }
                        | InstructionKind::WithScopedThread { .. }
                        | InstructionKind::AsyncRegistrationRequestClose { .. }
                        | InstructionKind::AsyncRegistrationPollClose { .. }
                        | InstructionKind::AsyncRegistrationFinishDestroy { .. }
                        | InstructionKind::AsyncRegistrationFinishDestroyChecked { .. } => Some(3),
                        InstructionKind::CallClosure { .. }
                        | InstructionKind::CallClosureOnce { .. }
                        | InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureBorrowed { .. } => Some(1),
                        InstructionKind::MakeClosure { .. } => Some(2),
                        InstructionKind::CallIndirect { .. } => Some(3),
                        _ => None,
                    };
                    if let Some(kind) = requires {
                        let count = self.program.sites.iter().filter(|site| site.instance == instance.id
                            && matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, .. } if actual == block.id && instruction as usize == index)
                            && matches!((&site.action, kind), (SemanticSiteAction::Call { .. }, 0) | (SemanticSiteAction::CallableCall { .. }, 1) | (SemanticSiteAction::Closure { .. }, 2) | (SemanticSiteAction::BodyCall { .. }, 0 | 1 | 3))).count();
                        if count != 1 {
                            return Err(fail(
                                "actual call or closure creation omits or duplicates its complete original recipe",
                            ));
                        }
                    }
                }
                if let Terminator::Return(value) = block.terminator {
                    b.spend(1, WORK)?;
                    if self.uses_full_value(&function.return_type, b)? {
                        let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                            &definition.body
                        else {
                            return Err(fail(
                                "actual result has no complete original function signature",
                            ));
                        };
                        let expected = self.recipe_value(instance, &signature.result, b, 0)?;
                        let Some(value) = value else {
                            return Err(fail("class-bearing actual Return omits its result SSA"));
                        };
                        let actual = self.ssa_value(instance, function, value, b, 0)?;
                        if actual != expected {
                            return Err(fail(
                                "actual Return changes its complete original result class/source role",
                            ));
                        }
                    }
                    match value {
                        Some(value)
                            if function.values.get(value.0 as usize)
                                == Some(&function.return_type) =>
                        {
                            Point {
                                function,
                                block,
                                instruction: block.instructions.len(),
                                edge: SemanticSiteEdge::Before,
                                initialization: &initialization,
                            }
                            .value(value, b)?;
                        }
                        None if function.return_type == Type::Unit => {}
                        _ => {
                            return Err(fail(
                                "actual Return does not preserve the complete original result type",
                            ));
                        }
                    }
                }
            }
        }
        // A hidden environment cannot authenticate itself. Every concrete
        // entry is sealed at its actual creator's MakeClosure, even if unused.
        for closure in &self.module.closures {
            b.spend(self.program.sites.len(), WORK)?;
            if !self.program.sites.iter().any(|site| matches!(site.action, SemanticSiteAction::Closure { closure: SemanticClosureTarget::Typed(id), .. } if id == closure.id)) {
                return Err(fail("actual closure environment has no checked original creation site"));
            }
        }
        Ok(())
    }
}
