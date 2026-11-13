//! Check original recipes against each actual operation. Type equality here is
//! the equality of a fully proved zero domain, never a way to erase a class.
use super::*;

struct Point<'a> {
    function: &'a TypedFunction,
    block: &'a BasicBlock,
    instruction: usize,
    edge: SemanticSiteEdge,
    initialization: &'a crate::Initialization,
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
    fn image_operands(&self, value: &SemanticTypeValue, point: &Point<'_>, result: Option<ValueId>, b: &mut Budget) -> CompileResult<()> {
        let SemanticTypeValue::Resolved { classes, .. } = value else { return Err(fail("class image retains an unresolved Core type")); };
        b.spend(classes.len(), WORK)?;
        for expression in classes {
            if let SemanticClassExpr::OuterOccurrence { operand, .. } | SemanticClassExpr::IntrinsicClass { operand, .. } = expression {
                match operand {
                    SemanticOperand::Value(value) if Some(*value) == result => {}
                    SemanticOperand::Value(value) => { point.value(*value, b)?; }
                    SemanticOperand::Local(local) => { point.local(*local, b)?; }
                    _ => return Err(unknown("capture class image needs its actual emitted value or initialized occupant")),
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

    fn result(
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
                self.image_operands(value, point, Some(*result), b)?;
                if self.captured_value(instance, value, &SemanticOperand::Value(*result), b)? != expected
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
                self.image_operands(expected, point, None, b)?;
                if expected_type != self.captured_value(instance, expected, subject, b)?
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
                self.image_operands(expected, point, None, b)?;
                if self.captured_value(instance, expected, &SemanticOperand::Local(*local), b)? != expected_type
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
        for function in &self.module.functions {
            let instance = self.function_instance(function.id, b)?;
            let definition = self.inventory.definition(instance.definition, b)?;
            let SemanticDefinitionBody::Function { sites: recipes, .. } = &definition.body else {
                return Err(fail("original function body is absent"));
            };
            let initialization = crate::initialize(function, b)?;
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
            for recipe in recipes {
                b.spend(self.program.sites.len(), WORK)?;
                if !self
                    .program
                    .sites
                    .iter()
                    .any(|site| site.instance == instance.id && site.recipe == recipe.id)
                {
                    return Err(fail("original emission recipe has no actual operation"));
                }
            }
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    b.spend(self.program.sites.len().saturating_add(1), WORK)?;
                    let requires = match instruction.kind {
                        InstructionKind::Call { .. } => Some(0),
                        InstructionKind::CallClosure { .. }
                        | InstructionKind::CallClosureOnce { .. }
                        | InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureMut { .. } => Some(1),
                        InstructionKind::MakeClosure { .. } => Some(2),
                        InstructionKind::SuspendCall { .. }
                        | InstructionKind::ForeignCall { .. }
                        | InstructionKind::CallIndirect { .. }
                        | InstructionKind::CallClosureBorrowed { .. } => {
                            return Err(unknown(
                                "actual semantic code operation has no completed scoped boundary proof",
                            ));
                        }
                        _ => None,
                    };
                    if let Some(kind) = requires {
                        let count = self.program.sites.iter().filter(|site| site.instance == instance.id
                            && matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, .. } if actual == block.id && instruction as usize == index)
                            && matches!((&site.action, kind), (SemanticSiteAction::Call { .. }, 0) | (SemanticSiteAction::CallableCall { .. }, 1) | (SemanticSiteAction::Closure { .. }, 2))).count();
                        if count != 1 {
                            return Err(fail(
                                "actual call or closure creation omits or duplicates its complete original recipe",
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
        for definition in &self.program.definitions {
            b.spend(self.program.instances.len(), WORK)?;
            if !self
                .program
                .instances
                .iter()
                .any(|instance| instance.definition == definition.id)
            {
                return Err(unknown(
                    "uninstantiated original definition requires its universal scoped proof",
                ));
            }
        }
        Ok(())
    }
}
