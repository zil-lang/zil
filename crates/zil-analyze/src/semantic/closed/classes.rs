//! Original class relations and actual header embeddings. These equations
//! never create a memory root, loan, borrow permission or cleanup capability.
use super::*;
use crate::semantic::value::{self, Argument, Class, Value};

impl Check<'_, '_> {
    pub(super) fn life(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticLifeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Class> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original lifetime relation exceeds its finite depth",
            ));
        }
        let expression = match recipe {
            SemanticLifeRecipe::Formal(reference) => SemanticClassExpr::Formal {
                scope: reference.scope,
                ordinal: reference.ordinal,
            },
            SemanticLifeRecipe::Implicit { scope, ordinal } => SemanticClassExpr::Implicit {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Environment { scope, ordinal } => SemanticClassExpr::Environment {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Invocation(ordinal) => {
                let declaration = self.inventory.definition(instance.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &declaration.body
                else {
                    return Err(fail("invocation lifetime has no original signature"));
                };
                if *ordinal >= signature.invocation_lifetimes {
                    return Err(fail("invocation lifetime exceeds its original frame"));
                }
                return Ok(Class::Invocation(instance.id, *ordinal));
            }
            SemanticLifeRecipe::IntrinsicClass {
                operand: SemanticRecipeOperand::Environment { scope, ordinal },
                path,
                class,
            } => {
                let context = self.ancestor(instance, *scope, b)?;
                return value::class(
                    context,
                    &SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::Environment(*ordinal),
                        path: path.clone(),
                        class: *class,
                    },
                    self.inventory,
                    b,
                    depth + 1,
                );
            }
            _ => {
                return Err(unknown(
                    "original derived or quantified lifetime needs its complete actual occurrence relation",
                ));
            }
        };
        value::class(instance, &expression, self.inventory, b, depth + 1)
    }
    pub(super) fn recipe_value(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "complete original type value exceeds its finite depth",
            ));
        }
        match recipe {
            SemanticTypeRecipe::OriginalType { scope, source } => self.original.source_value(
                instance,
                *scope,
                source,
                self.program,
                self.inventory,
                b,
            ),
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => Ok(value::scalar(ty.clone())),
            SemanticTypeRecipe::Slice(element) => {
                let element = self.recipe_value(instance, element, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "original Slice needs its complete element occurrence embedding",
                    ));
                }
                Ok(value::scalar(Type::Slice {
                    element: Box::new(element.ty),
                }))
            }
            SemanticTypeRecipe::Formal(reference) => {
                let context = self.ancestor(instance, reference.scope, b)?;
                if !matches!(
                    self.inventory
                        .definition(context.definition, b)?
                        .parameters
                        .get(reference.ordinal as usize),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail("original type value changes its full formal kind"));
                }
                let Some(SemanticActual::Type(actual)) =
                    context.actuals.get(reference.ordinal as usize)
                else {
                    return Err(fail("original type value omits a complete Type actual"));
                };
                value::resolved(context, actual, self.inventory, b, depth + 1)
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                let context = self.ancestor(instance, *scope, b)?;
                let declaration = self.inventory.definition(context.definition, b)?;
                if !matches!(
                    declaration
                        .environment
                        .get(*ordinal as usize)
                        .map(|formal| &formal.kind),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail("original environment type changes its declared kind"));
                }
                let parent = self.inventory.instance(
                    context
                        .parent
                        .ok_or_else(|| fail("original environment has no real creator context"))?,
                    b,
                )?;
                let Some(SemanticActual::Type(actual)) = context
                    .environment
                    .as_ref()
                    .and_then(|environment| environment.actuals.get(*ordinal as usize))
                else {
                    return Err(fail("original environment type omits a sealed actual"));
                };
                value::resolved(parent, actual, self.inventory, b, depth + 1)
            }
            SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let class = self.life(instance, life, b, depth + 1)?;
                let inner = self.recipe_value(instance, inner, b, depth + 1)?;
                value::address(*mutable, class, inner, b)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let declaration = self.inventory.definition(*definition, b)?;
                if declaration.parameters.len() != arguments.len() {
                    return Err(fail("nominal original type omits full mixed arguments"));
                }
                let mut actuals = Vec::new();
                for (formal, recipe) in declaration.parameters.iter().zip(arguments) {
                    let actual = self.argument_recipe(instance, formal, recipe, b, depth + 1)?;
                    self.push_value(&mut actuals, actual, b)?;
                }
                // The full original declaration was checked with rigid mixed
                // formals. Fields are instantiated through the same evaluator.
                self.nominal_value(*definition, &actuals, b, depth + 1)
            }
            SemanticTypeRecipe::Tuple(fields) => {
                if fields.is_empty() {
                    return Ok(value::scalar(Type::Unit));
                }
                let mut types = Vec::new();
                for field in fields {
                    let field = self.recipe_value(instance, field, b, depth + 1)?;
                    if !field.classes.is_empty() {
                        return Err(unknown(
                            "original tuple requires its complete retained class embedding",
                        ));
                    }
                    self.push_value(&mut types, field.ty, b)?;
                }
                Ok(value::scalar(Type::Tuple(types)))
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                if *definition != instance.definition {
                    return Err(unknown(
                        "opaque value needs its exact original definition specialization",
                    ));
                }
                let declaration = self.inventory.definition(*definition, b)?;
                if arguments.len() != declaration.parameters.len()
                    || arguments.len() != instance.actuals.len()
                {
                    return Err(fail(
                        "opaque original type omits mixed specialization arguments",
                    ));
                }
                for ((formal, recipe), actual) in declaration
                    .parameters
                    .iter()
                    .zip(arguments)
                    .zip(&instance.actuals)
                {
                    if self.argument_recipe(instance, formal, recipe, b, depth + 1)?
                        != self.actual_value(instance, actual, b, depth + 1)?
                    {
                        return Err(fail("opaque result changes its original specialization"));
                    }
                }
                self.recipe_value(instance, hidden, b, depth + 1)
            }
            _ => {
                // Existing closed scalar/closure recipes keep their complete
                // independent consumer. This does not turn None into zero.
                let ty = self.ty_closed(instance, recipe, b, depth + 1)?;
                if let Type::Closure(id) = ty {
                    let count = self
                        .module
                        .closures
                        .get(id.0 as usize)
                        .and_then(|definition| definition.storage_life.as_ref())
                        .ok_or_else(|| unknown("closure has no complete class inventory"))?
                        .fixed_binder_count;
                    if count != 0 {
                        return Err(unknown(
                            "classful opaque recipe needs its complete original environment value",
                        ));
                    }
                }
                Ok(value::scalar(ty))
            }
        }
    }
    pub(super) fn push_value<T>(
        &self,
        out: &mut Vec<T>,
        value: T,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(out.len().saturating_add(1), WORK)?;
        out.try_reserve(1)
            .map_err(|_| fail("complete original value allocation failed"))?;
        out.push(value);
        Ok(())
    }
    pub(super) fn actual_value(
        &self,
        instance: &SemanticInstance,
        actual: &SemanticActual,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Argument> {
        match actual {
            SemanticActual::Type(actual) => Ok(Argument::Type(value::resolved(
                instance,
                actual,
                self.inventory,
                b,
                depth + 1,
            )?)),
            SemanticActual::Life(actual) => Ok(Argument::Life(value::class(
                instance,
                actual,
                self.inventory,
                b,
                depth + 1,
            )?)),
            SemanticActual::Const {
                ty: carrier,
                value: constant,
            } => {
                let carrier = value::resolved(instance, carrier, self.inventory, b, depth + 1)?;
                if !carrier.classes.is_empty() || constant.ty() != carrier.ty {
                    return Err(fail("semantic constant has another complete carrier"));
                }
                Ok(Argument::Const {
                    carrier: carrier.ty,
                    value: constant.clone(),
                })
            }
            _ => Err(fail(
                "Core actual remains in an executable original context",
            )),
        }
    }
    fn argument_recipe(
        &self,
        instance: &SemanticInstance,
        formal: &SemanticFormal,
        recipe: &SemanticActualRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Argument> {
        match (formal, recipe) {
            (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(recipe)) => Ok(
                Argument::Type(self.recipe_value(instance, recipe, b, depth + 1)?),
            ),
            (SemanticFormal::Lifetime, SemanticActualRecipe::Life(recipe)) => {
                Ok(Argument::Life(self.life(instance, recipe, b, depth + 1)?))
            }
            (
                SemanticFormal::Const { .. },
                SemanticActualRecipe::Const {
                    carrier,
                    value: SemanticConstRecipe::Value(constant),
                },
            ) => {
                let carrier = self.recipe_value(instance, carrier, b, depth + 1)?;
                if !carrier.classes.is_empty() || carrier.ty != constant.ty() {
                    return Err(fail("original constant recipe has another actual carrier"));
                }
                Ok(Argument::Const {
                    carrier: carrier.ty,
                    value: constant.clone(),
                })
            }
            _ => Err(unknown(
                "original mixed actual needs its complete carrier-aware expression substitution",
            )),
        }
    }
    fn nominal_value(
        &self,
        definition: SemanticDefinitionId,
        arguments: &[Argument],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        let declaration = self.inventory.definition(definition, b)?;
        let SemanticDefinitionBody::Nominal { body, .. } = &declaration.body else {
            return Err(fail(
                "original nominal value selects another declaration kind",
            ));
        };
        let mut fields = Vec::new();
        match body {
            SemanticNominalBody::Product { fields: recipes } => {
                for field in recipes {
                    let value =
                        self.nominal_field(definition, arguments, &field.ty, b, depth + 1)?;
                    self.push_value(&mut fields, value, b)?;
                }
            }
            SemanticNominalBody::Enum { variants } => {
                for variant in variants {
                    for field in &variant.fields {
                        let value =
                            self.nominal_field(definition, arguments, field, b, depth + 1)?;
                        self.push_value(&mut fields, value, b)?;
                    }
                }
            }
        }
        value::nominal(definition, arguments, &fields, self.inventory, b, depth + 1)
    }
    fn nominal_field(
        &self,
        owner: SemanticDefinitionId,
        args: &[Argument],
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original nominal field value exceeds its complete depth",
            ));
        }
        match recipe {
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => Ok(value::scalar(ty.clone())),
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                let Some(Argument::Type(value)) = args.get(*ordinal as usize) else {
                    return Err(fail("original field changes its full Type formal kind"));
                };
                crate::dyn_metadata::ty(&value.ty, b)?;
                crate::dyn_metadata::shape(&value.life, b)?;
                Ok(value.clone())
            }
            SemanticTypeRecipe::Address {
                mutable,
                lifetime:
                    SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    })),
                inner,
            } => {
                let Some(Argument::Life(class)) = args.get(*ordinal as usize) else {
                    return Err(fail("original field changes its full Life formal kind"));
                };
                let inner = self.nominal_field(owner, args, inner, b, depth + 1)?;
                value::address(*mutable, class.clone(), inner, b)
            }
            _ => Err(unknown(
                "original nominal field needs its complete mixed type constructor substitution",
            )),
        }
    }
}

impl Check<'_, '_> {
    /// Classify the complete intrinsic nominal domain without choosing an
    /// external image or generating a memory root. Each local class receives
    /// its own token while reconstructing the original application.
    pub(super) fn nominal_inventory(&self, ty: &Type, b: &mut Budget) -> CompileResult<()> {
        let mut selected = None;
        for (index, instance) in self.module.life_instances.iter().enumerate() {
            crate::dyn_metadata::ty(&instance.ty, b)?;
            crate::dyn_metadata::ty(ty, b)?;
            if instance.ty == *ty && selected.replace((index, instance)).is_some() {
                return Err(fail("nominal type has duplicate exact inventory entries"));
            }
        }
        let (index, instance) =
            selected.ok_or_else(|| fail("nominal type has no complete actual inventory"))?;
        let index = u32::try_from(index).map_err(|_| fail("nominal inventory ordinal overflow"))?;
        let mut definition = None;
        for candidate in self.inventory.definitions.values() {
            b.spend(1, WORK)?;
            if matches!(candidate.body,SemanticDefinitionBody::Nominal {declaration,..} if declaration==instance.declaration)
                && definition.replace(candidate.id).is_some()
            {
                return Err(fail("nominal type has ambiguous original definitions"));
            }
        }
        let definition =
            definition.ok_or_else(|| fail("nominal type has no full original definition"))?;
        let life = match ty {
            Type::BorrowedStorage { life, .. } => life.clone(),
            _ => value::empty(),
        };
        let mut args = Vec::new();
        for (ordinal, argument) in instance.arguments.iter().enumerate() {
            let ordinal =
                u32::try_from(ordinal).map_err(|_| fail("nominal actual ordinal overflow"))?;
            let argument = match argument {
                LifeNominalActual::Lifetime(class) if *class < life.binder_count => {
                    Argument::Life(Class::Nominal(index, *class))
                }
                LifeNominalActual::Const(constant) => Argument::Const {
                    carrier: constant.ty(),
                    value: constant.clone(),
                },
                LifeNominalActual::Type(ty) | LifeNominalActual::QualifiedType { ty, .. } => {
                    let shape = match argument {
                        LifeNominalActual::QualifiedType { life, .. } => life.clone(),
                        _ => match ty {
                            Type::BorrowedStorage { life, .. } => life.clone(),
                            _ => value::empty(),
                        },
                    };
                    let mut classes = Vec::new();
                    for child in 0..shape.binder_count {
                        let mut mapped = None;
                        for row in &life.retained {
                            b.spend(row.path.len() + 1, WORK)?;
                            if row.path.as_slice() == [LifeTypeProjection::TypeArgument(ordinal)]
                                && row.class == child
                                && mapped.replace(row.binder).is_some()
                            {
                                return Err(fail(
                                    "nominal Type argument repeats a child occurrence mapping",
                                ));
                            }
                        }
                        let binder = mapped.ok_or_else(|| {
                            fail("nominal Type argument omits its child occurrence mapping")
                        })?;
                        if binder >= life.binder_count {
                            return Err(fail("nominal Type argument class is out of range"));
                        }
                        self.push_value(&mut classes, Class::Nominal(index, binder), b)?;
                    }
                    Argument::Type(Value {
                        ty: ty.clone(),
                        life: shape,
                        classes,
                    })
                }
                _ => return Err(fail("nominal actual has no complete local domain")),
            };
            self.push_value(&mut args, argument, b)?;
        }
        if self.nominal_value(definition, &args, b, 0)?.ty != *ty {
            return Err(fail(
                "nominal inventory differs from full original substitution",
            ));
        }
        Ok(())
    }
    pub(super) fn has_ghost_capture(&self, id: ClosureId, b: &mut Budget) -> CompileResult<bool> {
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        Ok(self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == id)
            .is_some_and(|definition| {
                definition
                    .captures
                    .iter()
                    .any(|ty| matches!(ty,Type::BorrowedStorage {life,..} if life.binder_count!=0))
            }))
    }
    pub(super) fn defining<'a>(
        &self,
        function: &'a TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<(BlockId, usize, &'a Instruction)> {
        let mut selected = None;
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                b.spend(1, WORK)?;
                if instruction.result == Some(value)
                    && selected.replace((block.id, index, instruction)).is_some()
                {
                    return Err(fail("semantic SSA value has multiple definitions"));
                }
            }
        }
        selected.ok_or_else(|| fail("semantic SSA value has no actual definition"))
    }
    fn capture_value_origin(
        &self,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<u32> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("capture value origin exceeds its actual SSA depth"));
        }
        let (block, index, instruction) = self.defining(function, value, b)?;
        match instruction.kind {
            InstructionKind::ReadCapture { index } => Ok(index),
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                self.capture_local_origin(function, local, (block, index), b, depth + 1)
            }
            _ => Err(unknown(
                "complete captured value needs its actual whole capture transport",
            )),
        }
    }
    fn capture_local_origin(
        &self,
        function: &TypedFunction,
        local: LocalId,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<u32> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("capture local origin exceeds its actual CFG depth"));
        }
        let block = function
            .blocks
            .get(at.0.0 as usize)
            .filter(|block| block.id == at.0)
            .ok_or_else(|| fail("capture local origin has no actual block"))?;
        for instruction in block.instructions.iter().take(at.1).rev() {
            b.spend(1, WORK)?;
            match instruction.kind {
                InstructionKind::Store {
                    local: target,
                    value,
                } if target == local => {
                    return self.capture_value_origin(function, value, b, depth + 1);
                }
                InstructionKind::StorageLive(target)
                | InstructionKind::StorageDead(target)
                | InstructionKind::TakeLocal { local: target }
                    if target == local =>
                {
                    return Err(fail("capture local origin crosses a vacant activation"));
                }
                _ => {}
            }
        }
        let initialized = crate::initialize(function, b)?;
        let mut selected = None;
        for predecessor in &function.blocks {
            b.spend(1, WORK)?;
            if !initialized.reachable[predecessor.id.0 as usize] {
                continue;
            }
            if matches!(predecessor.terminator,Terminator::Jump(next) if next==at.0)
                || matches!(predecessor.terminator,Terminator::Branch {then_block,else_block,..} if then_block==at.0||else_block==at.0)
            {
                let origin = self.capture_local_origin(
                    function,
                    local,
                    (predecessor.id, predecessor.instructions.len()),
                    b,
                    depth + 1,
                )?;
                if selected.is_some_and(|old| old != origin) {
                    return Err(unknown(
                        "capture local has several actual environment origins",
                    ));
                }
                selected = Some(origin);
            }
        }
        selected.ok_or_else(|| fail("capture local has no initialized actual origin"))
    }
    pub(super) fn ghost_capture_directory(
        &self,
        id: ClosureId,
        b: &mut Budget,
    ) -> CompileResult<Vec<u32>> {
        let closure = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == id)
            .ok_or_else(|| fail("ghost capture directory has another closure identity"))?;
        let instance = self.function_instance(closure.function, b)?;
        let definition = self.inventory.definition(instance.definition, b)?;
        let function = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(closure.function), b)?;
        let contract = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| unknown("ghost capture domain has no complete class contract"))?;
        if function.storage_life.as_ref() != Some(contract)
            || contract.invocation_binder_count != 0
            || !contract.bindings.is_empty()
            || !contract.static_bindings.is_empty()
            || !closure.lending_captures.is_empty()
        {
            return Err(unknown(
                "ghost capture needs its complete retained data and actual head relation",
            ));
        }
        let parent = self.inventory.instance(
            instance
                .parent
                .ok_or_else(|| fail("ghost capture has no actual creator instance"))?,
            b,
        )?;
        let environment = instance
            .environment
            .as_ref()
            .ok_or_else(|| fail("ghost capture has no sealed creator environment"))?;
        if environment.closure != SemanticClosureTarget::Typed(id)
            || definition.environment.len() != environment.actuals.len()
        {
            return Err(fail("ghost capture changes its original environment"));
        }
        let SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. } = &definition.body
        else {
            return Err(fail("ghost capture omits its complete original body"));
        };
        b.spend(contract.fixed_binder_count as usize, WORK)?;
        let mut directory = vec![None; contract.fixed_binder_count as usize];
        let mut mapped = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        for recipe in recipes {
            let SemanticSiteRecipeKind::TypeUse {
                expected:
                    SemanticTypeRecipe::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    },
            } = &recipe.kind
            else {
                continue;
            };
            let Some(SemanticActual::Type(actual)) = environment.actuals.get(*ordinal as usize)
            else {
                return Err(fail(
                    "ghost captured name changes its original environment kind",
                ));
            };
            let expected = value::resolved(parent, actual, self.inventory, b, 0)?;
            if !expected.life.slots.is_empty() || !expected.life.retained.is_empty() {
                return Err(unknown(
                    "captured borrowed data needs its complete actual retained source transport",
                ));
            }
            for site in &self.program.sites {
                b.spend(1, WORK)?;
                if site.instance != instance.id || site.recipe != recipe.id {
                    continue;
                }
                let SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } = site.at
                else {
                    return Err(fail("captured name retains a Core address"));
                };
                let SemanticSiteAction::TypeUse { subject, .. } = &site.action else {
                    return Err(fail("captured name has another actual operation"));
                };
                let capture = match subject {
                    SemanticOperand::Value(value) => {
                        self.capture_value_origin(function, *value, b, 0)?
                    }
                    SemanticOperand::Local(local) => self.capture_local_origin(
                        function,
                        *local,
                        (
                            block,
                            instruction as usize + usize::from(edge == SemanticSiteEdge::After),
                        ),
                        b,
                        0,
                    )?,
                    _ => {
                        return Err(unknown(
                            "captured name requires its actual whole value or initialized local",
                        ));
                    }
                };
                let ty = closure
                    .captures
                    .get(capture as usize)
                    .ok_or_else(|| fail("captured name selects an absent actual capture"))?;
                if *ty != expected.ty {
                    return Err(fail("captured name has another complete environment type"));
                }
                b.lookup(mapped.len(), WORK)?;
                mapped.insert(capture);
                let mut fixed = BTreeSet::new();
                for (child, class) in expected.classes.iter().enumerate() {
                    let mut selected = None;
                    for (row_index, row) in contract.classes.iter().enumerate() {
                        b.spend(row.position.path.len() + 1, WORK)?;
                        if row.position.base == StorageBase::Capture(capture)
                            && row.position.path.is_empty()
                            && row.position.class == child as u32
                        {
                            if selected.replace((row_index, row.binder)).is_some() {
                                return Err(fail(
                                    "capture class repeats its actual header occurrence",
                                ));
                            }
                        }
                    }
                    let (row, binder) = selected.ok_or_else(|| {
                        fail("capture class omits a full original child occurrence")
                    })?;
                    let StorageBinder::Fixed(index) = binder else {
                        return Err(fail("capture persistent class is not Fixed"));
                    };
                    b.lookup(fixed.len(), WORK)?;
                    if !fixed.insert(index) {
                        return Err(fail(
                            "capture collapses two intrinsic child classes into one Fixed class",
                        ));
                    }
                    let header = self.header_class(instance, binder, b)?;
                    if header != *class {
                        return Err(fail(
                            "captured class differs from the sealed original environment",
                        ));
                    }
                    let slot = directory
                        .get_mut(index as usize)
                        .ok_or_else(|| fail("capture Fixed class is out of range"))?;
                    if slot.is_some_and(|old| old != capture) {
                        return Err(unknown(
                            "several captures need their complete original intrinsic equality proof",
                        ));
                    }
                    *slot = Some(capture);
                    b.lookup(used_rows.len(), WORK)?;
                    used_rows.insert(row);
                }
            }
        }
        for (index, ty) in closure.captures.iter().enumerate() {
            b.lookup(mapped.len(), WORK)?;
            if !mapped.contains(&(index as u32)) {
                return Err(fail(
                    "actual capture has no exact original name/environment binding",
                ));
            }
            if !ty.is_b0_scalar()
                && !matches!(ty,Type::BorrowedStorage {life,..} if life.slots.is_empty())
            {
                return Err(unknown(
                    "actual capture requires its complete owning or borrowed-data proof",
                ));
            }
        }
        if used_rows.len() != contract.classes.len()
            || instance.header_embedding.len() != directory.len()
        {
            return Err(fail(
                "ghost capture adds or omits a complete class/header occurrence",
            ));
        }
        let mut result = Vec::new();
        for entry in directory {
            self.push_value(
                &mut result,
                entry.ok_or_else(|| {
                    fail("ghost capture Fixed class has no actual original occurrence")
                })?,
                b,
            )?;
        }
        Ok(result)
    }
    pub(super) fn header_class(
        &self,
        instance: &SemanticInstance,
        binder: StorageBinder,
        b: &mut Budget,
    ) -> CompileResult<Class> {
        let mut selected = None;
        for row in &instance.header_embedding {
            b.spend(1, WORK)?;
            if row.header == binder && selected.replace(&row.actual).is_some() {
                return Err(fail("header binder has duplicate semantic embeddings"));
            }
        }
        value::class(
            instance,
            selected
                .ok_or_else(|| fail("actual header binder has no complete original embedding"))?,
            self.inventory,
            b,
            0,
        )
    }
}

impl Check<'_, '_> {
    pub(super) fn ssa_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "semantic value transport exceeds its finite actual SSA depth",
            ));
        }
        let ty = function
            .values
            .get(id.0 as usize)
            .ok_or_else(|| fail("semantic value transport selects an absent SSA"))?;
        crate::dyn_metadata::ty(ty, b)?;
        if ty.is_b0_scalar() {
            return Ok(value::scalar(ty.clone()));
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        match &instruction.kind {
            InstructionKind::CallClosureBorrowed { .. } => self
                .abstract_operation(instance, function, block, index, b)?
                .ok_or_else(|| {
                    unknown("borrowed callable result has no complete original invocation role")
                }),
            InstructionKind::ReborrowAddress { address, mutable } => {
                if let Some(value) = self.source_borrow(instance, function, id, b, depth + 1)? {
                    return Ok(value);
                }
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                let parent = self.ssa_value(instance, function, *address, b, depth + 1)?;
                let Type::Address {
                    mutable: parent_mutable,
                    inner,
                } = &parent.ty
                else {
                    return Err(fail("actual reborrow has no complete parent address value"));
                };
                if *mutable && !*parent_mutable {
                    return Err(fail("actual reborrow strengthens a shared parent value"));
                }
                crate::dyn_metadata::ty(inner, b)?;
                let output = Type::Address {
                    mutable: *mutable,
                    inner: inner.clone(),
                };
                if output != *ty
                    || parent.life.slots.len() != 1
                    || !parent.life.slots[0].path.is_empty()
                {
                    return Err(fail(
                        "actual reborrow changes its whole pointee or outer occurrence",
                    ));
                }
                let child = value::pointee(&parent, self.inventory, b)?;
                let result = value::address(*mutable, Class::Reborrow(function.id, id), child, b)?;
                // This distinct outer occurrence may adapt to its actual
                // parent only within a proved call role; it is never Eq(parent).
                Ok(result)
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                self.local_value(instance, function, *local, (block, index), b, depth + 1)
            }
            InstructionKind::ReadCapture { index: capture } => {
                let closure = function
                    .environment
                    .and_then(|id| self.module.closures.get(id.0 as usize))
                    .ok_or_else(|| fail("capture transport has no real environment"))?;
                let contract = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("capture transport has no complete class relation"))?;
                let shape = match ty {
                    Type::BorrowedStorage { life, .. } => life.clone(),
                    _ => {
                        return Err(unknown(
                            "capture transport requires its whole intrinsic type value",
                        ));
                    }
                };
                let mut classes = Vec::new();
                for child in 0..shape.binder_count {
                    let mut selected = None;
                    for row in &contract.classes {
                        b.spend(row.position.path.len() + 1, WORK)?;
                        if row.position.base == StorageBase::Capture(*capture)
                            && row.position.path.is_empty()
                            && row.position.class == child
                            && selected.replace(row.binder).is_some()
                        {
                            return Err(fail("actual capture repeats a class occurrence"));
                        }
                    }
                    let class = self.header_class(
                        instance,
                        selected.ok_or_else(|| fail("actual capture omits an intrinsic class"))?,
                        b,
                    )?;
                    self.push_value(&mut classes, class, b)?;
                }
                Ok(Value {
                    ty: ty.clone(),
                    life: shape,
                    classes,
                })
            }
            InstructionKind::MakeAggregate { .. } => {
                let definition = self.inventory.definition(instance.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. } =
                    &definition.body
                else {
                    return Err(fail("actual constructor has no original body"));
                };
                let mut selected = None;
                for site in &self.program.sites {
                    b.spend(recipes.len() + 1, WORK)?;
                    if site.instance != instance.id
                        || !matches!(site.at,SemanticSiteAddress::Typed {block:actual,instruction:actual_index,edge:SemanticSiteEdge::After} if actual==block&&actual_index as usize==index)
                    {
                        continue;
                    }
                    let recipe = recipes
                        .iter()
                        .find(|recipe| recipe.id == site.recipe)
                        .ok_or_else(|| fail("actual constructor has no original recipe"))?;
                    if let SemanticSiteRecipeKind::Construct { result } = &recipe.kind {
                        let value = self.recipe_value(instance, result, b, depth + 1)?;
                        if selected.replace(value).is_some() {
                            return Err(fail(
                                "actual constructor has several original class values",
                            ));
                        }
                    }
                }
                let expected = selected.ok_or_else(|| {
                    fail("actual constructor has no exact original value occurrence")
                })?;
                if expected.ty != *ty
                    || !expected.life.slots.is_empty()
                    || !expected.life.retained.is_empty()
                {
                    return Err(unknown(
                        "actual aggregate requires its complete retained child/data class transport",
                    ));
                }
                Ok(expected)
            }
            InstructionKind::BorrowLocal { .. } | InstructionKind::BorrowLocalMut { .. } => {
                if let Some(value) = self.source_borrow(instance, function, id, b, depth + 1)? {
                    return Ok(value);
                }
                let mut selected = None;
                for site in &self.program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != instance.id {
                        continue;
                    }
                    if let SemanticSiteAction::TypeUse {
                        subject: SemanticOperand::Value(subject),
                        expected,
                    } = &site.action
                    {
                        if *subject == id {
                            let value =
                                value::resolved(instance, expected, self.inventory, b, depth + 1)?;
                            if selected.as_ref().is_some_and(|old| old != &value) {
                                return Err(fail(
                                    "borrow SSA has conflicting original class occurrences",
                                ));
                            }
                            selected = Some(value);
                        }
                    }
                }
                let expected = selected.ok_or_else(|| {
                    unknown("borrow SSA has no complete original occurrence value")
                })?;
                if expected.ty != *ty {
                    return Err(fail("borrow SSA loses its complete original type"));
                }
                Ok(expected)
            }
            InstructionKind::MakeClosure {
                closure: id,
                captures,
            } => {
                if !self.has_ghost_capture(*id, b)? {
                    return Err(unknown(
                        "closure result needs its complete actual borrowed-head transport",
                    ));
                }
                self.ghost_creator(instance, *id, captures, b)?;
                if *ty != Type::Closure(*id) {
                    return Err(fail(
                        "actual closure result changes its real constructed identity",
                    ));
                }
                let definition = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| fail("actual closure constructor has no definition"))?;
                let contract = definition
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| fail("actual closure constructor has no complete scope"))?;
                b.spend(contract.fixed_binder_count as usize, WORK)?;
                let mut classes = vec![None; contract.fixed_binder_count as usize];
                for row in &contract.classes {
                    let (StorageBase::Capture(capture), StorageBinder::Fixed(fixed)) =
                        (row.position.base, row.binder)
                    else {
                        return Err(unknown(
                            "closure constructor needs its complete parameter/result class relation",
                        ));
                    };
                    if !row.position.path.is_empty() {
                        return Err(unknown(
                            "closure constructor needs its complete projected captured class relation",
                        ));
                    }
                    let source = *captures
                        .get(capture as usize)
                        .ok_or_else(|| fail("closure constructor omits a captured operand"))?;
                    let source = self.ssa_value(instance, function, source, b, depth + 1)?;
                    let class = source
                        .classes
                        .get(row.position.class as usize)
                        .cloned()
                        .ok_or_else(|| {
                            fail("closure constructor omits an intrinsic captured class")
                        })?;
                    let slot = classes
                        .get_mut(fixed as usize)
                        .ok_or_else(|| fail("closure constructor Fixed class is out of range"))?;
                    if slot.as_ref().is_some_and(|old| old != &class) {
                        return Err(unknown(
                            "closure constructor requires its complete original equality relation",
                        ));
                    }
                    *slot = Some(class);
                }
                let mut complete = Vec::new();
                for class in classes {
                    self.push_value(
                        &mut complete,
                        class.ok_or_else(|| {
                            fail("closure constructor leaves an unproved Fixed class")
                        })?,
                        b,
                    )?;
                }
                Ok(Value {
                    ty: ty.clone(),
                    life: StorageLifeShape {
                        binder_count: contract.fixed_binder_count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    classes: complete,
                })
            }
            InstructionKind::Call {
                function: callee,
                arguments,
            } => {
                let target = self.function_instance(*callee, b)?;
                let expected_parameters =
                    self.static_call_values(instance, function, block, index, *callee, b)?;
                if expected_parameters.len() != arguments.len() {
                    return Err(fail(
                        "actual call changes its complete original input arity",
                    ));
                }
                let callee = self
                    .inventory
                    .function(&SemanticFunctionTarget::Typed(*callee), b)?;
                let contract = callee.storage_life.as_ref().ok_or_else(|| {
                    unknown("class-bearing call result has no complete callee header")
                })?;
                let count = match ty {
                    Type::Closure(id) => {
                        self.module
                            .closures
                            .get(id.0 as usize)
                            .and_then(|closure| closure.storage_life.as_ref())
                            .ok_or_else(|| {
                                unknown("call result closure has no complete intrinsic domain")
                            })?
                            .fixed_binder_count
                    }
                    Type::BorrowedStorage { life, .. } => life.binder_count,
                    _ => {
                        return Err(unknown(
                            "call result requires its complete outer/static class transport",
                        ));
                    }
                };
                let mut classes = Vec::new();
                for child in 0..count {
                    let mut result = None;
                    for row in &contract.classes {
                        b.spend(row.position.path.len() + 1, WORK)?;
                        if row.position.base == StorageBase::Result
                            && row.position.path.is_empty()
                            && row.position.class == child
                            && result.replace(row.binder).is_some()
                        {
                            return Err(fail(
                                "callee result repeats an intrinsic class occurrence",
                            ));
                        }
                    }
                    let binder = result
                        .ok_or_else(|| fail("callee result omits an intrinsic class occurrence"))?;
                    let mut source = None;
                    for row in &contract.bindings {
                        b.spend(row.position.path.len() + 1, WORK)?;
                        if row.binder != binder {
                            continue;
                        }
                        let StorageBase::Parameter(parameter) = row.position.base else {
                            continue;
                        };
                        if !row.position.path.is_empty() {
                            return Err(unknown(
                                "call result requires its complete projected input class transport",
                            ));
                        }
                        let actual = *arguments
                            .get(parameter as usize)
                            .ok_or_else(|| fail("callee class input omits an actual argument"))?;
                        let value = self.ssa_value(instance, function, actual, b, depth + 1)?;
                        let expected = expected_parameters
                            .get(parameter as usize)
                            .ok_or_else(|| fail("callee outer role omits an original input"))?;
                        if !matches!(expected.ty, Type::Address { .. })
                            || expected.life.slots.len() != 1
                            || !expected.life.slots[0].path.is_empty()
                            || !self.outer_role(
                                instance,
                                function,
                                &value,
                                expected,
                                b,
                                depth + 1,
                            )?
                        {
                            return Err(fail(
                                "actual input does not satisfy its original outer call role",
                            ));
                        }
                        let class = expected
                            .classes
                            .get(expected.life.slots[0].binder as usize)
                            .cloned()
                            .ok_or_else(|| fail("original call outer input omits a whole class"))?;
                        if source.as_ref().is_some_and(|old| old != &class) {
                            return Err(unknown(
                                "callee lifetime input needs its complete local role dependency union",
                            ));
                        }
                        source = Some(class);
                    }
                    if source.is_none() {
                        // Fixed/input-class sources require the same exact
                        // real header occurrence; no result class self-proof.
                        for row in &contract.classes {
                            b.spend(row.position.path.len() + 1, WORK)?;
                            if row.binder != binder {
                                continue;
                            }
                            let StorageBase::Parameter(parameter) = row.position.base else {
                                continue;
                            };
                            if !row.position.path.is_empty() {
                                return Err(unknown(
                                    "call result child requires its full projected input relation",
                                ));
                            }
                            let actual = *arguments.get(parameter as usize).ok_or_else(|| {
                                fail("callee class input omits an actual argument")
                            })?;
                            let input = self.ssa_value(instance, function, actual, b, depth + 1)?;
                            let expected = expected_parameters
                                .get(parameter as usize)
                                .ok_or_else(|| fail("callee class role omits an original input"))?;
                            if input != *expected {
                                return Err(fail(
                                    "actual intrinsic input changes its complete original class domain",
                                ));
                            }
                            let class = expected.classes.get(row.position.class as usize).cloned()
                                .ok_or_else(|| fail("callee class input is absent from its original whole value"))?;
                            if source.as_ref().is_some_and(|old| old != &class) {
                                return Err(unknown(
                                    "callee class input needs its complete role dependency union",
                                ));
                            }
                            source = Some(class);
                        }
                    }
                    let class=source.ok_or_else(||unknown("callee result class has no independently proved input/static/role source"))?;
                    self.push_value(&mut classes, class, b)?;
                }
                let shape = match ty {
                    Type::BorrowedStorage { life, .. } => life.clone(),
                    _ => StorageLifeShape {
                        binder_count: count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                };
                // Target context is checked separately; it cannot be replaced
                // by a same-shaped function or caller's lifetime coincidence.
                if target.definition != self.inventory.definition(target.definition, b)?.id {
                    return Err(fail(
                        "callee class transport changes its original definition",
                    ));
                }
                Ok(Value {
                    ty: ty.clone(),
                    life: shape,
                    classes,
                })
            }
            _ => Err(unknown(
                "semantic value requires its exact actual class/role transport",
            )),
        }
    }
    pub(super) fn local_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: LocalId,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "semantic local class transport exceeds its finite CFG depth",
            ));
        }
        let block = function
            .blocks
            .get(at.0.0 as usize)
            .filter(|block| block.id == at.0)
            .ok_or_else(|| fail("semantic local class has no actual block"))?;
        for instruction in block.instructions.iter().take(at.1).rev() {
            b.spend(1, WORK)?;
            match instruction.kind {
                InstructionKind::Store {
                    local: target,
                    value,
                } if target == local => {
                    return self.ssa_value(instance, function, value, b, depth + 1);
                }
                InstructionKind::StorageLive(target)
                | InstructionKind::StorageDead(target)
                | InstructionKind::TakeLocal { local: target }
                    if target == local =>
                {
                    return Err(fail("semantic local class crosses a vacant activation"));
                }
                _ => {}
            }
        }
        if at.0 == function.entry {
            if let Some(parameter) = function
                .parameters
                .iter()
                .position(|candidate| *candidate == local)
            {
                let definition = self.inventory.definition(instance.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    return Err(fail("semantic parameter has no complete original header"));
                };
                return self.recipe_value(
                    instance,
                    &signature
                        .parameters
                        .get(parameter)
                        .ok_or_else(|| fail("semantic original parameter is absent"))?
                        .ty,
                    b,
                    depth + 1,
                );
            }
        }
        let initialized = crate::initialize(function, b)?;
        let mut selected = None;
        for predecessor in &function.blocks {
            b.spend(1, WORK)?;
            if !initialized.reachable[predecessor.id.0 as usize] {
                continue;
            }
            if matches!(predecessor.terminator,Terminator::Jump(next) if next==at.0)
                || matches!(predecessor.terminator,Terminator::Branch {then_block,else_block,..} if then_block==at.0||else_block==at.0)
            {
                let value = self.local_value(
                    instance,
                    function,
                    local,
                    (predecessor.id, predecessor.instructions.len()),
                    b,
                    depth + 1,
                )?;
                if selected.as_ref().is_some_and(|old| old != &value) {
                    return Err(unknown(
                        "semantic local needs its complete joined role/source dependency relation",
                    ));
                }
                selected = Some(value);
            }
        }
        selected.ok_or_else(|| fail("semantic local has no initialized actual class origin"))
    }
}

impl Check<'_, '_> {
    pub(super) fn header_relation(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            let value = self.recipe_value(instance, &parameter.ty, b, 0)?;
            self.push_value(&mut parameters, value, b)?;
        }
        let result = self.recipe_value(instance, &signature.result, b, 0)?;
        let captures = if let Some(id) = function.environment {
            self.capture_directory(id, b)?;
            true
        } else {
            false
        };
        let Some(contract) = &function.storage_life else {
            if !instance.header_embedding.is_empty()
                || parameters.iter().any(|value| !value.classes.is_empty())
                || !result.classes.is_empty()
                || captures
            {
                return Err(fail(
                    "actual function omits a complete original header domain",
                ));
            }
            return Ok(());
        };
        crate::signature_life::charge(contract, b)?;
        if !contract.static_bindings.is_empty() {
            return Err(unknown(
                "original header static occurrences need their complete actual constant-source proof",
            ));
        }
        let total = (contract.fixed_binder_count as usize)
            .checked_add(contract.invocation_binder_count as usize)
            .ok_or_else(|| fail("actual header domain count overflow"))?;
        if instance.header_embedding.len() != total {
            return Err(fail(
                "actual header omits a complete original binder embedding",
            ));
        }
        let mut seen = BTreeSet::new();
        for row in &instance.header_embedding {
            b.lookup(seen.len(), WORK)?;
            let valid = match row.header {
                StorageBinder::Fixed(index) => captures && index < contract.fixed_binder_count,
                StorageBinder::Invocation(index) => index < contract.invocation_binder_count,
            };
            if !valid || !seen.insert(row.header) {
                return Err(fail(
                    "actual header binder is repeated or outside its real frame",
                ));
            }
        }
        let mut used_bindings = 0usize;
        let mut used_classes = 0usize;
        for (base, value) in parameters
            .iter()
            .enumerate()
            .map(|(index, value)| (StorageBase::Parameter(index as u32), value))
            .chain(std::iter::once((StorageBase::Result, &result)))
        {
            for slot in &value.life.slots {
                let expected = value
                    .classes
                    .get(slot.binder as usize)
                    .ok_or_else(|| fail("original header slot omits its whole class"))?;
                let mut selected = None;
                for row in &contract.bindings {
                    b.spend(row.position.path.len() + 1, WORK)?;
                    if row.position.base == base
                        && row.position.path.len()==slot.path.len()&&row.position.path.iter().zip(&slot.path).all(|(actual,expected)|matches!(actual,ContractProjection::Place(projection) if projection==expected))
                        && selected.replace(row.binder).is_some()
                    {
                        return Err(fail(
                            "actual header repeats an original outer borrow occurrence",
                        ));
                    }
                }
                let binder = selected.ok_or_else(|| {
                    fail("actual header omits an original outer borrow occurrence")
                })?;
                if self.header_class(instance, binder, b)? != *expected {
                    return Err(fail(
                        "actual outer header class differs from its original declaration domain",
                    ));
                }
                used_bindings += 1;
            }
            if matches!(value.ty, Type::BorrowedStorage { .. } | Type::Closure(_)) {
                for (child, expected) in value.classes.iter().enumerate() {
                    let mut selected = None;
                    for row in &contract.classes {
                        b.spend(row.position.path.len() + 1, WORK)?;
                        if row.position.base == base
                            && row.position.path.is_empty()
                            && row.position.class == child as u32
                            && selected.replace(row.binder).is_some()
                        {
                            return Err(fail(
                                "actual header repeats an intrinsic class occurrence",
                            ));
                        }
                    }
                    let binder = selected.ok_or_else(|| {
                        fail("actual header omits an original intrinsic class occurrence")
                    })?;
                    if self.header_class(instance, binder, b)? != *expected {
                        return Err(fail(
                            "actual intrinsic header class differs from its complete original occurrence",
                        ));
                    }
                    used_classes += 1;
                }
            }
            if matches!(value.ty, Type::Address { .. } | Type::Desc(_)) {
                for child in &value.life.retained {
                    if child.path.as_slice() != [LifeTypeProjection::Pointee] {
                        return Err(unknown(
                            "outer header requires its complete child occurrence path",
                        ));
                    }
                    let expected = value
                        .classes
                        .get(child.binder as usize)
                        .ok_or_else(|| fail("original Pointee header omits its whole class"))?;
                    let mut selected = None;
                    for row in &contract.classes {
                        b.spend(row.position.path.len().saturating_add(1), WORK)?;
                        if row.position.base == base
                            && row.position.path == child.path
                            && row.position.class == child.class
                            && selected.replace(row.binder).is_some()
                        {
                            return Err(fail(
                                "actual header repeats a Pointee intrinsic class occurrence",
                            ));
                        }
                    }
                    let binder = selected.ok_or_else(|| {
                        fail("actual header omits a complete Pointee class occurrence")
                    })?;
                    if self.header_class(instance, binder, b)? != *expected {
                        return Err(fail(
                            "actual Pointee header class differs from its complete original occurrence",
                        ));
                    }
                    used_classes += 1;
                }
            }
        }
        for row in &contract.bindings {
            if matches!(row.position.base, StorageBase::Capture(_)) {
                if !captures {
                    return Err(fail("ordinary function invents a capture header"));
                }
                used_bindings += 1;
            }
        }
        for row in &contract.classes {
            if matches!(row.position.base, StorageBase::Capture(_)) {
                if !captures {
                    return Err(fail("ordinary function invents a capture class"));
                }
                used_classes += 1;
            }
        }
        if used_bindings != contract.bindings.len() || used_classes != contract.classes.len() {
            return Err(fail(
                "actual header contains an unproved occurrence or class",
            ));
        }
        Ok(())
    }
}

impl Check<'_, '_> {
    pub(super) fn full_value_at(
        &self,
        instance: &SemanticInstance,
        expected: &SemanticTypeValue,
        operand: &SemanticOperand,
        at: (BlockId, usize, SemanticSiteEdge),
        declaration: bool,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let expected = value::resolved(instance, expected, self.inventory, b, 0)?;
        if declaration {
            return Ok(expected);
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("semantic whole value has no real function context"));
        };
        let function = self.inventory.function(target, b)?;
        let actual = match operand {
            SemanticOperand::Value(value) => self.ssa_value(instance, function, *value, b, 0)?,
            SemanticOperand::Local(local) => self.local_value(
                instance,
                function,
                *local,
                (at.0, at.1 + usize::from(at.2 == SemanticSiteEdge::After)),
                b,
                0,
            )?,
            SemanticOperand::Parameter(index) => {
                let local = *function
                    .parameters
                    .get(*index as usize)
                    .ok_or_else(|| fail("semantic whole value selects an absent parameter"))?;
                self.local_value(
                    instance,
                    function,
                    local,
                    (at.0, at.1 + usize::from(at.2 == SemanticSiteEdge::After)),
                    b,
                    0,
                )?
            }
            _ => {
                return Err(unknown(
                    "semantic whole value requires its complete actual operand transport",
                ));
            }
        };
        if actual != expected {
            return Err(fail(
                "actual whole type/class value differs from its original operand constraint",
            ));
        }
        Ok(actual)
    }
    pub(super) fn uses_full_value(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        match ty {
            Type::BorrowedStorage { life, .. } => Ok(life.binder_count != 0),
            Type::Address { .. } => Ok(true),
            Type::Closure(id) => self.has_ghost_capture(*id, b),
            _ => Ok(false),
        }
    }
    pub(super) fn ghost_creator(
        &self,
        instance: &SemanticInstance,
        id: ClosureId,
        captures: &[ValueId],
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.ghost_capture_directory(id, b)?;
        let definition = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == id)
            .ok_or_else(|| fail("ghost creator names another actual closure"))?;
        let child = self.function_instance(definition.function, b)?;
        let function = match &instance.target {
            SemanticInstanceTarget::Function(target) => self.inventory.function(target, b)?,
            _ => return Err(fail("ghost creator has no real caller function")),
        };
        let contract = definition
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("ghost creator has no complete class contract"))?;
        if captures.len() != definition.captures.len() {
            return Err(fail("ghost creator omits an actual capture"));
        }
        for (index, operand) in captures.iter().enumerate() {
            let actual = self.ssa_value(instance, function, *operand, b, 0)?;
            if actual.ty != definition.captures[index] {
                return Err(fail(
                    "ghost creator has another complete actual capture type",
                ));
            }
            let mut count = 0usize;
            for row in &contract.classes {
                b.spend(row.position.path.len() + 1, WORK)?;
                if row.position.base != StorageBase::Capture(index as u32) {
                    continue;
                }
                if !row.position.path.is_empty() {
                    return Err(unknown(
                        "ghost creator needs its complete projected child transport",
                    ));
                }
                let class = actual
                    .classes
                    .get(row.position.class as usize)
                    .ok_or_else(|| fail("ghost creator omits a full captured class"))?;
                if self.header_class(child, row.binder, b)? != *class {
                    return Err(fail(
                        "ghost creator capture comes from another actual semantic source",
                    ));
                }
                count += 1;
            }
            if count != actual.classes.len() {
                return Err(fail(
                    "ghost creator omits or adds captured class occurrences",
                ));
            }
        }
        Ok(())
    }
}
