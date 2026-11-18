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
    let initialization = crate::shared_initialization(function, b)?;
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

/// Candidate occurrence selection uses the same paid dominance relation as
/// mandatory operands. An unavailable candidate is a mismatch; malformed CFG
/// identities and resource exhaustion remain errors.
pub(super) fn value_available_before(function: &TypedFunction,
    at: (BlockId, usize, SemanticSiteEdge), value: ValueId, b: &mut Budget)
    -> CompileResult<bool> {
    let initialization = crate::shared_initialization(function, b)?;
    let block = function.blocks.get(at.0.0 as usize)
        .filter(|block| block.id == at.0 && at.1 <= block.instructions.len())
        .ok_or_else(|| fail("candidate occurrence has no actual emission boundary"))?;
    Ok(Point { function, block, instruction: at.1, edge: at.2,
        initialization: &initialization }.unavailable(value, b)?.is_none())
}

pub(super) fn initialized_local_before(
    function: &TypedFunction,
    at: (BlockId, usize),
    local: LocalId,
    b: &mut Budget,
) -> CompileResult<()> {
    let initialization = crate::shared_initialization(function, b)?;
    let block = function.blocks.get(at.0.0 as usize)
        .filter(|block| block.id == at.0 && at.1 <= block.instructions.len())
        .ok_or_else(|| fail("resource source has no exact initialized local boundary"))?;
    Point { function, block, instruction: at.1, edge: SemanticSiteEdge::Before,
        initialization: &initialization }.local(local, b)?;
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
        let ty = self.function.values.get(value.0 as usize)
            .ok_or_else(|| fail("semantic operand selects an absent actual SSA value"))?;
        if let Some(reason) = self.unavailable(value, b)? { return Err(fail(reason)); }
        Ok(ty)
    }
    fn unavailable(&self, value: ValueId, b: &mut Budget) -> CompileResult<Option<&'static str>> {
        if self.function.values.get(value.0 as usize).is_none() {
            return Err(fail("semantic operand selects an absent actual SSA value"));
        }
        let definition = crate::shared_definition(self.function, value, b)?;
        let (block, ordinal) = (definition.block, definition.instruction);
        if block == self.block.id {
            if ordinal < self.instruction
                || (ordinal == self.instruction && self.edge == SemanticSiteEdge::After)
            {
                return Ok(None);
            }
            return Ok(Some("semantic SSA operand is read before its actual definition"));
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
                return Ok(Some("semantic SSA operand does not dominate its actual use"));
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
            return Ok(Some("semantic SSA definition is not on an actual reachable path"));
        }
        Ok(None)
    }
}

impl Check<'_, '_> {
    fn ffi_literal_occurrence(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        subject: &SemanticOperand,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let Some(original) = self.original.ffi_literal_at(instance, site, self.inventory, b)? else {
            return Ok(());
        };
        let SemanticOperand::Value(output) = subject else {
            return Err(fail("original foreign constant has no emitted SSA"));
        };
        let (_, _, instruction) = self.defining(function, *output, b)?;
        let InstructionKind::Constant(actual) = &instruction.kind else {
            return Err(fail(
                "original foreign constant is replaced by another actual operation",
            ));
        };
        if let (Constant::WideString(original), Constant::WideString(actual)) = (original, actual) {
            b.spend(original.len().saturating_add(actual.len()), WORK)?;
        }
        if original != actual {
            return Err(fail(
                "actual foreign constant changes the original literal payload",
            ));
        }
        Ok(())
    }

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
                    SemanticOperand::Parameter(parameter) => {
                        let local = point.function.parameters.get(*parameter as usize)
                            .ok_or_else(|| fail("parameter class image exceeds the actual input table"))?;
                        point.local(*local, b)?;
                        // Reuse the original signature/actual selector proof;
                        // an initialized input is not its own class image.
                        if self.parameter_address_claim(instance, value, b, 0)?.is_none() {
                            return Err(unknown("parameter class image needs its complete whole-value selector transport"));
                        }
                    }
                    SemanticOperand::Environment(_) | SemanticOperand::TypeArgument { .. } => {
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

    pub(super) fn body_call_actual(
        &self,
        target: &SemanticInstance,
        ordinal: usize,
        formal: &SemanticFormal,
        expected: &crate::semantic::value::Argument,
        actual: &crate::semantic::value::Argument,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        use crate::semantic::value::{Argument, Class};
        b.spend(1, WORK)?;
        let ordinal =
            u32::try_from(ordinal).map_err(|_| fail("original call mixed ordinal overflows"))?;
        Ok(match (formal, expected, actual) {
            (
                SemanticFormal::Lifetime,
                Argument::Life(_),
                Argument::Life(Class::Formal(owner, index)),
            ) => *owner == target.id && *index == ordinal,
            (SemanticFormal::Type { .. }, Argument::Type(expected), Argument::Type(actual)) => {
                // The target is checked in its own rigid declaration frame.
                // Reconstruct every intrinsic coordinate, including phantom
                // classes; caller images never become target premises.
                b.spend(
                    expected.classes.len().saturating_add(actual.classes.len()),
                    WORK,
                )?;
                if expected.classes.len() != expected.life.binder_count as usize
                    || actual.classes.len() != expected.classes.len()
                {
                    return Ok(false);
                }
                let mut classes = Vec::new();
                for child in 0..expected.life.binder_count {
                    self.push_value(
                        &mut classes,
                        Class::TypeArgument(target.id, ordinal, child),
                        b,
                    )?;
                }
                let rigid = expected.with_classes(classes, b)?;
                actual.equal_paid(&rigid, b)?
            }
            (SemanticFormal::Const { .. }, Argument::Const { .. }, Argument::Const { .. }) => {
                self.capture_arguments_equal(expected, actual, b)?
            }
            _ => false,
        })
    }

    fn original_claim(
        &self,
        instance: &SemanticInstance,
        claim: &SemanticTypeValue,
        original: &crate::semantic::value::Value,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let actual = self.resolved(instance, claim, b, 0)?;
        if self.capture_values_equal(&actual, original, b)? {
            return Ok(());
        }
        crate::dyn_metadata::recipe(claim, b)?;
        crate::dyn_metadata::ty(&original.ty, b)?;
        crate::dyn_metadata::shape(&original.life, b)?;
        b.spend(
            original
                .classes
                .len()
                .saturating_add(actual.classes.len())
                .saturating_mul(128)
                .saturating_add(256),
            WORK,
        )?;
        let mut errors = fail("actual semantic claim changes its complete original class value");
        errors[0].notes.push(format!("independent original value {original:?}; independently resolved actual claim {actual:?}; input claim {claim:?}"));
        Err(errors)
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
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                let expected = &original_value.ty;
                if self.uses_full_value(&original_value.ty, b)? {
                    self.original_claim(instance, value, &original_value, b)?;
                }

                self.image_operands(instance, value, point, Some(*result), b)?;
                if &self.captured_value(
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
                    expected,
                )?;
            }
            (
                SemanticSiteRecipeKind::TypeUse { expected: recipe },
                SemanticSiteAction::TypeUse { subject, expected },
            ) => {
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                let expected_type = &original_value.ty;
                // A logical local use crosses the private shell only through
                // this original binding's real declaration/initializer witness.
                let actual_operand = if let SemanticOperand::Local(local) = subject
                    && matches!(expected_type, Type::Closure(_))
                    && let Some(slot) = self.persistent_slot(instance, point.function, *local, b)? {
                    self.persistent_slot_value(instance, point.function, &slot,
                        (point.block.id, point.instruction + usize::from(point.edge == SemanticSiteEdge::After)), b, 0)?.into_type(b)?
                } else { self.operand(instance, subject, point, b)? };
                if &actual_operand != expected_type
                    && let Some((whole, error)) = self.original.return_injection(
                        instance,
                        site.recipe,
                        self.program,
                        self.inventory,
                        b,
                    )?
                {
                    let SemanticOperand::Value(output) = subject else {
                        return Err(fail("implicit return injection has no actual result SSA"));
                    };
                    let (block, index, producer) = self.defining(point.function, *output, b)?;
                    let InstructionKind::MakeErrorUnion { is_error, value } = producer.kind else {
                        return Err(fail(
                            "implicit return injection is replaced by another actual opcode",
                        ));
                    };
                    if is_error != error
                        || actual_operand != whole.ty
                        || !self.capture_values_equal(
                            &self.resolved(instance, expected, b, 0)?, &whole, b,
                        )?
                        || point.function.values.get(value.0 as usize) != Some(&original_value.ty)
                    {
                        return Err(fail(
                            "implicit return injection changes its original union, arm or complete payload",
                        ));
                    }
                    operations::available_before(
                        point.function,
                        (block, index, SemanticSiteEdge::Before),
                        value,
                        b,
                    )?;
                    if self.uses_full_value(&original_value.ty, b)?
                        && !self.capture_values_equal(
                            &self.ssa_value(instance, point.function, value, b, 0)?,
                            &original_value, b,
                        )?
                    {
                        return Err(fail(
                            "implicit return injection changes its original payload classes",
                        ));
                    }
                    self.image_operands(instance, expected, point, None, b)?;
                    self.function_reference(
                        instance,
                        site.recipe,
                        point.function,
                        &SemanticOperand::Value(value),
                        &original_value.ty,
                        b,
                    )?;
                    if let Some(bytes) =
                        self.original
                            .string_literal_at(instance, site.recipe, self.inventory, b)?
                    {
                        let (_, _, inner) = self.defining(point.function, value, b)?;
                        let InstructionKind::StaticUtf8 { bytes: actual } = &inner.kind else {
                            return Err(fail(
                                "implicit return injection replaces its original literal payload",
                            ));
                        };
                        b.spend(bytes.len().saturating_add(actual.len()), WORK)?;
                        if actual.as_slice() != bytes {
                            return Err(fail(
                                "implicit return injection changes original literal bytes",
                            ));
                        }
                    }
                    self.ffi_literal_occurrence(
                        instance,
                        site.recipe,
                        point.function,
                        &SemanticOperand::Value(value),
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
                        let (_, _, inner) = self.defining(point.function, value, b)?;
                        if !constructor.payload.is_empty()
                            || constructor.result.ty != original_value.ty
                            || !matches!(&inner.kind, InstructionKind::MakeVariant { variant, fields } if *variant == constructor.variant && fields.is_empty())
                        {
                            return Err(fail(
                                "implicit return injection replaces its original variant payload",
                            ));
                        }
                    }
                    return Ok(());
                }
                if self.uses_full_value(&original_value.ty, b)? {
                    self.original_claim(instance, expected, &original_value, b)?;
                }

                self.image_operands(instance, expected, point, None, b)?;
                self.function_reference(
                    instance,
                    site.recipe,
                    point.function,
                    subject,
                    expected_type,
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
                        || &constructor.result.ty != expected_type
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
                self.ffi_literal_occurrence(instance, site.recipe, point.function, subject, b)?;
                let captured = self.captured_value(
                    instance,
                    expected,
                    subject,
                    (point.block.id, point.instruction, point.edge),
                    false,
                    b,
                )?;
                if expected_type != &captured || &actual_operand != expected_type {
                    crate::dyn_metadata::ty(&actual_operand, b)?;
                    crate::dyn_metadata::ty(&captured, b)?;
                    crate::dyn_metadata::recipe(expected, b)?;
                    b.spend(
                        original_value
                            .classes
                            .len()
                            .saturating_mul(128)
                            .saturating_add(256),
                        WORK,
                    )?;
                    let mut errors = fail(
                        "actual type-use occupant differs from its complete original ascription",
                    );
                    errors[0].notes.push(format!("original whole value {original_value:?}; emitted claim {expected:?}; captured operand type {captured:?}; actual operand type {actual_operand:?}"));
                    return Err(errors);
                }
            }
            (
                SemanticSiteRecipeKind::DeclareLocal { expected: recipe },
                SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                },
            ) => {
                let original_value = self.recipe_value(instance, recipe, b, 0)?;
                let expected_type = &original_value.ty;
                if self.uses_full_value(&original_value.ty, b)? {
                    self.original_claim(instance, expected, &original_value, b)?;
                }

                if let Some(slot) = self.persistent_slot(instance, point.function, *local, b)? {
                    if slot.declaration() != (point.block.id, point.instruction)
                        || point.edge != SemanticSiteEdge::After
                        || !slot.logical().equal_paid(&original_value, b)? {
                        return Err(fail("persistent declaration changes its exact original boundary or whole value"));
                    }
                    let actual = self.ssa_value(instance, point.function, slot.initial(), b, 0)?;
                    if !actual.equal_paid(&original_value, b)? {
                        return Err(fail("persistent declaration initializer changes its complete original child"));
                    }
                    // The original declaration is logical; its private
                    // shell is initialized by the witnessed immediate Store.
                    let images = self.image_heads(instance, expected,
                        (slot.initial_store().0, slot.initial_store().1, SemanticSiteEdge::After), b)?;
                    self.match_heads(instance, &images, &SemanticOperand::Value(slot.initial()),
                        (slot.initial_store().0, slot.initial_store().1, SemanticSiteEdge::Before), b)?;
                    return Ok(());
                }
                self.image_operands(instance, expected, point, None, b)?;
                if &self.captured_value(
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
                        != Some(expected_type)
                    || !matches!(instruction.map(|actual| &actual.kind), Some(InstructionKind::StorageLive(actual)) if actual == local)
                {
                    return Err(fail(
                        "actual local declaration changes its complete original constraint",
                    ));
                }
                self.check_reference_declaration(instance, point.function, *local, b)?;
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
                            if !self.capture_values_equal(&value, expected, b)? {
                                return Err(fail(
                                    "actual enum payload changes its original intrinsic class occurrence",
                                ));
                            }
                        }
                    }
                    for (actual, expected) in actuals.iter().zip(&constructor.arguments) {
                        if !self.capture_arguments_equal(
                            &self.actual_value(instance, actual, b, 0)?, expected, b,
                        )? {
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
                        original::sealed::Intrinsic::OwnedDyn => {
                            let output = self.dyn_owned_operation(
                                instance,
                                site.recipe,
                                point.function,
                                point.block.id,
                                point.instruction,
                                actuals,
                                b,
                            )?;
                            self.result(
                                instruction.ok_or_else(|| {
                                    fail("actual owning Dyn construction has no instruction")
                                })?,
                                point.function,
                                &output.ty,
                            )?;
                            return Ok(());
                        }
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
                        | original::sealed::Intrinsic::TaskCreateOwned
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
                        | original::sealed::Intrinsic::BytesView
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
                        | original::sealed::Intrinsic::TaskValid
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
                let (original, closure, expected_actuals, operation) =
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
                    | InstructionKind::CallClosureBorrowed { .. }
                        if closure =>
                    {
                        let (operand, arguments) = match &instruction.kind {
                            InstructionKind::CallClosure { callee, arguments }
                            | InstructionKind::CallClosureOnce { callee, arguments }
                            | InstructionKind::CallClosureBorrowed { callee, arguments, .. } => {
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
                        let receiver = self.operand(instance, &operand, point, b)?;
                        let id = if let InstructionKind::CallClosureBorrowed { callee, kind, .. } = instruction.kind {
                            let Type::Address { mutable, inner } = &receiver else {
                                return Err(fail("borrowed known closure has no actual environment address"));
                            };
                            let Type::Closure(id) = inner.as_ref() else {
                                return Err(fail("borrowed known closure changes its complete pointee type"));
                            };
                            if !matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                                || kind == CallableKind::FnMut && !*mutable {
                                return Err(fail("borrowed known closure changes its original invocation mode"));
                            }
                            let original = self.original.borrowed_closure_receiver(instance, site.recipe,
                                self.program, self.inventory, b)?;
                            self.borrowed_invocation_place(instance, point.function, instruction, &original, b)?;
                            let whole = self.ssa_value(instance, point.function, callee, b, 0)?;
                            if !self.pointee(&whole, b)?.equal_paid(&original.inner, b)? {
                                return Err(fail("borrowed known closure changes its complete original environment value"));
                            }
                            *id
                        } else {
                            let Type::Closure(id) = receiver else {
                                return Err(fail("original closure callee has another complete actual type"));
                            };
                            id
                        };
                        let callee = self
                            .module
                            .closures
                            .get(id.0 as usize)
                            .filter(|callee| callee.id == id)
                            .ok_or_else(|| fail("original closure call has no actual entry"))?;
                        if let InstructionKind::CallClosureBorrowed { kind, .. } = instruction.kind
                            && !matches!((callee.kind, kind),
                                (CallableKind::Fn, CallableKind::Fn)
                                | (CallableKind::Fn | CallableKind::FnMut, CallableKind::FnMut))
                        {
                            return Err(fail("borrowed known closure entry has another complete callable role"));
                        }
                        let original_receiver = self.original.borrowed_closure_receiver(
                            instance, site.recipe, self.program, self.inventory, b)?;
                        self.known_call_place(instance, point.function,
                            (point.block.id, point.instruction), instruction, &original_receiver, b)?;
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
                // A machine body may have several independently checked
                // invocation worlds. Check this boundary against every one;
                // no global representative identifies their local classes.
                let worlds = self.function_worlds(callee, b)?;
                let formals = &self.inventory.definition(original, b)?.parameters;
                if worlds.is_empty() {
                    return Err(fail(
                        "actual BodyCall has no complete original target world",
                    ));
                }
                for target in worlds {
                    if target.definition != original
                        || actuals.len() != expected_actuals.len()
                        || target.actuals.len() != expected_actuals.len()
                        || formals.len() != expected_actuals.len()
                    {
                        return Err(fail(
                            "actual BodyCall changes its independently inferred original target or mixed arity",
                        ));
                    }
                    for (ordinal, (((proposed, callee), expected), formal)) in actuals
                        .iter()
                        .zip(&target.actuals)
                        .zip(&expected_actuals)
                        .zip(formals)
                        .enumerate()
                    {
                        let proposed = self.actual_value(instance, proposed, b, 0)?;
                        if !self.capture_arguments_equal(&proposed, expected, b)? {
                            return Err(fail(
                                "actual BodyCall differs from original caller-domain mixed inference",
                            ));
                        }
                        let callee = self.actual_value(target, callee, b, 0)?;
                        let valid =
                            self.body_call_actual(target, ordinal, formal, expected, &callee, b)?;
                        if !valid {
                            return Err(fail(
                                "actual BodyCall target changes its independent original rigid mixed context",
                            ));
                        }
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
                for (ordinal, (actual, expected)) in arguments.iter().zip(&expected_inputs).enumerate() {
                    if self.uses_full_value(&expected.ty, b)? {
                        let operand = *actual;
                        let actual = self.ssa_value(instance, point.function, operand, b, 0)?;
                        let valid = if matches!(expected.ty, Type::Address { .. } | Type::Desc(_)) {
                            self.outer_call_role(instance, point.function, (point.block.id, point.instruction), ordinal, operand, &actual, expected, b, 0)?
                        } else {
                            self.capture_values_equal(&actual, expected, b)?
                        };
                        if !valid {
                            return Err(fail(
                                "actual call input differs from its complete original role",
                            ));
                        }
                    }
                }
                self.result(instruction, point.function, result)?;
                if closure && let InstructionKind::CallClosureOnce { callee, .. } = instruction.kind {
                    let Some(Type::Closure(id)) = point.function.values.get(callee.0 as usize) else {
                        return Err(fail("known consuming operation loses its complete receiver"));
                    };
                    self.known_consuming_receiver(instance, point.function,
                        (point.block.id, point.instruction), callee, site.recipe, b)?;
                    let upper = operation.ok_or_else(|| fail("known consuming operation loses its original execution upper"))?;
                    if upper == CallKind::Suspend && point.function.call_kind != CallKind::Suspend {
                        return Err(fail("known Suspend consuming operation has no original Suspend caller"));
                    }
                    record_consuming_operation(point.function.id, point.block.id,
                        point.instruction, *id, upper, b)?;
                }
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
                if callee.environment.is_some()
                    || !self.actual_vectors(instance, recipes, actuals, b, 0)?
                {
                    return Err(fail(
                        "actual call changes its original definition or full mixed substitution",
                    ));
                }
                let mut caller_actuals = Vec::new();
                for actual in actuals {
                    let actual = self.actual_value(instance, actual, b, 0)?;
                    self.push_value(&mut caller_actuals, actual, b)?;
                }
                let worlds = self.function_worlds(callee.id, b)?;
                if worlds.is_empty() {
                    return Err(fail("actual call has no complete original target world"));
                }
                for world in worlds {
                    if world.definition != *target {
                        return Err(fail("actual call changes its original definition"));
                    }
                    self.operation_mixed_arguments(world, &caller_actuals, b)?;
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
                let child = self.closure_creator_world(actual.function, instance.id, *closure, b)?;
                let closed = child
                    .environment
                    .as_ref()
                    .ok_or_else(|| fail("actual closure has no creator-sealed environment"))?;
                if child.definition != *definition
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
        // These directories contain only references to the complete immutable
        // event table. They answer presence/location queries; every original
        // recipe, actual event, duplicate and state obligation is still checked.
        let mut emitted_recipes = BTreeSet::new();
        let mut operation_sites: BTreeMap<
            (SemanticInstanceId, BlockId, usize), Vec<&SemanticSite>,
        > = BTreeMap::new();
        self.inventory.profile_step("event directory", b);
        for site in &self.program.sites {
            self.inventory.profile_site(site, b);
            b.spend(1, WORK)?;
            b.lookup(emitted_recipes.len(), WORK)?;
            emitted_recipes.insert((site.instance, site.recipe));
            if let SemanticSiteAddress::Typed { block, instruction, .. } = site.at {
                b.lookup(operation_sites.len(), WORK)?;
                let rows = operation_sites.entry((site.instance, block, instruction as usize)).or_default();
                let relocation = if rows.len() == rows.capacity() { rows.len() } else { 0 };
                b.spend(relocation.saturating_add(1), WORK)?;
                rows.try_reserve(1).map_err(|_| fail("actual semantic event directory allocation failed"))?;
                rows.push(site);
            }
        }
        for instance in &self.program.instances {
            self.inventory.profile_step("world entry", b);
            self.inventory.profile_instance(instance, b);
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
            let initialization = crate::shared_initialization(function, b)?;
            let mut declared = BTreeMap::new();
            let mut declared_values = BTreeMap::new();
            self.inventory.profile_step("local declarations", b);
            for site in &self.program.sites {
                self.inventory.profile_site(site, b);
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
                let image_at = if let Some(slot) = self.persistent_slot(instance, function, *local, b)? {
                    (slot.initial_store().0, slot.initial_store().1, SemanticSiteEdge::After)
                } else { (block, instruction as usize, edge) };
                let images = self.image_heads(instance, expected, image_at, b)?;
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
            self.inventory.profile_step("original operations", b);
            for site in &self.program.sites {
                self.inventory.profile_site(site, b);
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
                let checked = self.operation(
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
                );
                if let Err(mut errors) = checked {
                    if errors.iter().any(|error| error.category == zil_base::ResultCategory::ResourceLimit) {
                        // Optional contextual formatting must not replace the
                        // actual exhausted proof's category, caller or ledger.
                        return Err(errors);
                    }
                    b.spend(192, WORK)?;
                    for error in &mut errors {
                        error.notes.push(format!("actual semantic instance {}:{} original definition {}:{} function {} recipe {} at block {} instruction {} {:?}",
                            instance.id.module.0,instance.id.index,definition.id.module.0,definition.id.index,function.id.0,site.recipe.0,block.id.0,instruction,edge));
                    }
                    return Err(errors);
                }
            }
            let original_nodes = self.inventory.original_index(definition.id, b)?;
            self.inventory.profile_step("recipe coverage", b);
            for recipe in recipes {
                self.inventory.profile_recipe(recipe.id, b);
                b.lookup(emitted_recipes.len(), WORK)?;
                if !emitted_recipes.contains(&(instance.id, recipe.id)) {
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
            // Primitive BorrowLocal/ReadAddress and call-adapter SSA may have
            // no TypeUse of their own. Reuse the actual original whole-value
            // reconstruction; inferring geometry from equal erased Types or
            // through a Reborrow edge would lose outer/child alias structure.
            let nested = |ty: &Type| matches!(ty, Type::Address { inner, .. }
                if matches!(inner.as_ref(), Type::Address { .. } | Type::Desc(_)));
            for (ordinal, ty) in function.values.iter().enumerate() {
                b.spend(1, WORK)?;
                if !nested(ty) { continue; }
                let id = ValueId(u32::try_from(ordinal).map_err(|_| fail("raw geometry SSA ordinal overflow"))?);
                let value = self.ssa_value(instance, function, id, b, 0).map_err(|mut errors| {
                    for error in &mut errors {
                        error.notes.push(format!("complete raw geometry producer function {} SSA {} original instance {:?}, Type {:?}", function.id.0, id.0, instance.id, ty));
                    }
                    errors
                })?;
                if !geometry::same_type(&value.ty, ty, b)? {
                    return Err(fail("raw geometry reconstruction changes its complete actual Type"));
                }
                crate::dyn_metadata::shape(&value.life, b)?;
                b.lookup(b.semantic_raw_value_shapes.len(), WORK)?;
                if let Some(old) = b.semantic_raw_value_shapes.get(&(function.id, id)).cloned() {
                    crate::dyn_metadata::shape(&old, b)?;
                    if old != value.life { return Err(fail("raw SSA has different complete original geometries in its actual worlds")); }
                } else { b.semantic_raw_value_shapes.insert((function.id, id), value.life.clone()); }
            }
            for (local, value) in &declared_values {
                b.spend(1, WORK)?;
                if !nested(&value.ty) { continue; }
                crate::dyn_metadata::shape(&value.life, b)?;
                b.lookup(b.semantic_raw_local_shapes.len(), WORK)?;
                if let Some(old) = b.semantic_raw_local_shapes.get(&(function.id, **local)).cloned() {
                    crate::dyn_metadata::shape(&old, b)?;
                    if old != value.life { return Err(fail("raw local has different complete declared geometries in its actual worlds")); }
                } else { b.semantic_raw_local_shapes.insert((function.id, **local), value.life.clone()); }
            }
            self.inventory.profile_step("actual CFG coverage", b);
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    self.inventory.profile_operation(block.id, index, b);
                    b.spend(1, WORK)?;
                    if let InstructionKind::WriteAddress { address, value } = instruction.kind
                        && function.values.get(value.0 as usize).is_some_and(|ty|
                            matches!(ty, Type::Address { .. }) || crate::stored_flow::option_address::candidate(ty)) {
                        let destination = self.ssa_value(instance, function, address, b, 0)?;
                        if !matches!(destination.ty, Type::Address { mutable: true, .. }) {
                            return Err(fail("actual raw pointer replacement has no exclusive storage head"));
                        }
                        let expected = self.pointee(&destination, b)?;
                        let incoming = self.ssa_value(instance, function, value, b, 0)?;
                        if !incoming.equal_paid(&expected, b)?
                            && !self.outer_role(instance, function, &incoming, &expected, b, 0)? {
                            return Err(fail("raw pointer replacement changes its complete original child or directed receiving role"));
                        }
                        b.lookup(b.semantic_raw_writes.len(), WORK)?;
                        b.semantic_raw_writes.insert((function.id, block.id, index));
                    }
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
                            let actual =
                                match self.reference_store_value(instance, function, local, b)? {
                                    Some(role) => role,
                                    None => self.ssa_value(instance, function, value, b, 0)?,
                                };
                            let shell = self.persistent_slot(instance, function, local, b)?;
                            let physical_expected = if let Some(slot) = &shell {
                                if slot.initial_store() != (block.id, index) || slot.array() != value {
                                    return Err(fail("persistent Store replaces another original shell activation"));
                                }
                                self.persistent_slot_array_value(instance, function, slot, expected, b)?
                            } else { expected.copy(b)? };
                            if !actual.equal_paid(&physical_expected, b)?
                                && !self.outer_role(instance, function, &actual, &physical_expected, b, 0)? {
                                return Err(fail(
                                    "actual Store changes its complete original local class constraint",
                                ));
                            }
                        }
                        b.spend(declared.len().saturating_add(1), WORK)?;
                        if let Some(images) = declared.get(&local) {
                            let subject = if let Some(slot) = self.persistent_slot(instance, function, local, b)? {
                                if slot.initial_store() != (block.id, index) || slot.array() != value {
                                    return Err(fail("persistent head Store changes its exact initializer"));
                                }
                                slot.initial()
                            } else { value };
                            self.match_heads(
                                instance,
                                images,
                                &SemanticOperand::Value(subject),
                                (block.id, index, SemanticSiteEdge::Before),
                                b,
                            )?;
                        }
                    }
                    if let InstructionKind::StoreOwnedArrayElement { local, index: element, value } = instruction.kind
                        && let Some(slot) = self.persistent_slot(instance, function, local, b)? {
                        if element != 0 { return Err(fail("persistent refill changes its original element selector")); }
                        available_before(function, (block.id, index, SemanticSiteEdge::Before), value, b)?;
                        let incoming = self.ssa_value(instance, function, value, b, 0)?;
                        if !incoming.equal_paid(slot.logical(), b)? {
                            return Err(fail("persistent refill changes its complete original logical value"));
                        }
                        if let Some(images) = declared.get(&local) {
                            self.match_heads(instance, images, &SemanticOperand::Value(value),
                                (block.id, index, SemanticSiteEdge::Before), b)?;
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
                        b.lookup(operation_sites.len(), WORK)?;
                        let rows = operation_sites.get(&(instance.id, block.id, index)).map(Vec::as_slice).unwrap_or(&[]);
                        b.spend(rows.len(), WORK)?;
                        let count = rows.iter().copied().filter(|site| site.instance == instance.id
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
                    self.inventory.profile_operation(block.id, block.instructions.len(), b);
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
                        if !self.capture_values_equal(&actual, &expected, b)?
                            && !self.outer_role(instance, function, &actual, &expected, b, 0)?
                            && !self.receiver_return_role(instance, function, block.id, value, &actual, &expected, b)?
                            && !self.receiving_field_return(instance, function, block.id, value, &actual, &expected, b)?
                            && !self.forwarded_receiver_return(instance, function, block.id, value, &actual, &expected, b)? {
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
        self.inventory.profile_stage("closure creation coverage", b);
        for closure in &self.module.closures {
            b.spend(self.program.sites.len(), WORK)?;
            if !self.program.sites.iter().any(|site| matches!(site.action, SemanticSiteAction::Closure { closure: SemanticClosureTarget::Typed(id), .. } if id == closure.id)) {
                return Err(fail("actual closure environment has no checked original creation site"));
            }
        }
        Ok(())
    }
}
