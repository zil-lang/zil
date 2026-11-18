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
        let class = value::class(instance, &expression, self.inventory, b, depth + 1)?;
        self.capture_class_image(class, b)
    }
    pub(super) fn recipe_value(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        let value = self.recipe_value_raw(instance, recipe, b, depth)?;
        self.capture_class_value(value, b)
    }
    fn recipe_value_raw(
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
            SemanticTypeRecipe::Dyn { owned, interface } => self.original.dyn_recipe(
                instance,
                *owned,
                interface,
                self.program,
                self.inventory,
                b,
                depth + 1,
            ),
            SemanticTypeRecipe::Task { result, retained } => self.original.task_recipe(
                instance,
                result,
                retained,
                self.program,
                self.inventory,
                b,
                depth + 1,
            ),
            SemanticTypeRecipe::SelfType { scope } => {
                let context = self.ancestor(instance, *scope, b)?;
                self.original_scope_self(context, b, depth + 1)
            }
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                member,
            } => {
                let receiver = self.recipe_value(instance, receiver, b, depth + 1)?;
                let arguments = self.trait_actuals(instance, application, b, depth + 1)?;
                self.associated_actual(
                    &receiver,
                    application.declaration,
                    &arguments,
                    *member,
                    b,
                    depth + 1,
                )
            }
            SemanticTypeRecipe::OriginalType { scope, source } => self.original.source_value(
                instance,
                *scope,
                source,
                self.program,
                self.inventory,
                b,
            ),
            SemanticTypeRecipe::Atom(ty)
                if ty.is_b0_scalar()
                    || matches!(ty, Type::ForeignHandle | Type::CWideString | Type::ByteBuffer) =>
            {
                value::borrowed_scalar(ty, self.inventory, b)
            }
            SemanticTypeRecipe::Exact(exact)
                if (exact.ty == crate::generic_mutex::error_type()
                    || crate::dyn_dispatch::is_create_error(&exact.ty))
                    && empty(&exact.life)
                    && exact.classes.is_empty() =>
            {
                value::borrowed_scalar(&exact.ty, self.inventory, b)
            }
            SemanticTypeRecipe::Slice(element) => {
                let element = self.recipe_value(instance, element, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "original Slice needs its complete element occurrence embedding",
                    ));
                }
                value::unary(geometry::Unary::Slice, element, b)
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
                self.resolved(context, actual, b, depth + 1)
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
                self.resolved(parent, actual, b, depth + 1)
            }
            SemanticTypeRecipe::Address {
                mutable: false,
                lifetime: SemanticOuterLifeRecipe::Static,
                inner,
            } => {
                let child = self.recipe_value(instance, inner, b, depth + 1)?;
                if !empty(&child.life)
                    || !child.classes.is_empty()
                    || !matches!(&child.ty,Type::Slice{element} if element.as_ref()==&Type::U8)
                {
                    return Err(unknown(
                        "original static address needs its complete inner occurrence proof",
                    ));
                }
                Ok(value::scalar(original::strings::byte_type()))
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
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => {
                let child = self.recipe_value(instance, inner, b, depth + 1)?;
                let kind = match recipe {
                    SemanticTypeRecipe::Managed(_) => geometry::Unary::Managed,
                    SemanticTypeRecipe::OwnedStore(_) => geometry::Unary::OwnedStore,
                    SemanticTypeRecipe::Mutex(_) => geometry::Unary::Mutex,
                    SemanticTypeRecipe::Registration(_) => geometry::Unary::Registration,
                    SemanticTypeRecipe::AsyncRegistration(_) => geometry::Unary::AsyncRegistration,
                    _ => {
                        return Err(fail(
                            "semantic resource constructor changed its original family",
                        ));
                    }
                };
                self.resource_constructor(kind, child, b)
            }
            SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let child = self.recipe_value(instance, inner, b, depth + 1)?;
                let class = self.life(instance, life, b, depth + 1)?;
                value::description(class, child, b)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let error = self.recipe_value(instance, error, b, depth + 1)?;
                let result = self.recipe_value(instance, result, b, depth + 1)?;
                value::error_union(error, result, b)
            }
            SemanticTypeRecipe::Code { kind, signature } => {
                if matches!(kind, SemanticCodeKind::FunctionPointer | SemanticCodeKind::BorrowedFunctionPointer)
                    && signature.invocation_lifetimes != 0
                {
                    return self.invocation_code_recipe(instance, signature, b, depth + 1);
                }
                if signature.bound_lifetimes != 0
                    || signature.invocation_lifetimes != 0
                    || signature.call_kind != CallKind::Direct
                    || !matches!(
                        signature.result_mode,
                        PassingMode::Infer | PassingMode::Copy
                    )
                {
                    return Err(unknown(
                        "original Code requires its complete per-call lifetime and passing relation",
                    ));
                }
                let mut parameters = Vec::new();
                for parameter in &signature.parameters {
                    if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                        return Err(unknown(
                            "original Code parameter requires its full resource mode",
                        ));
                    }
                    let parameter = self.recipe_value(instance, &parameter.ty, b, depth + 1)?;
                    if !parameter.classes.is_empty() {
                        return Err(unknown(
                            "original Code parameter requires complete scoped class transport",
                        ));
                    }
                    self.push_value(&mut parameters, parameter.into_type(b)?, b)?;
                }
                let result = self.recipe_value(instance, &signature.result, b, depth + 1)?;
                if !result.classes.is_empty() {
                    return Err(unknown(
                        "original Code result requires complete scoped class transport",
                    ));
                }
                Ok(value::scalar(match kind {
                    SemanticCodeKind::FunctionPointer => Type::FunctionPointer {
                        parameters,
                        result: Box::new(result.into_type(b)?),
                    },
                    SemanticCodeKind::ForeignCallback => Type::ForeignCallback {
                        parameters,
                        result: Box::new(result.into_type(b)?),
                    },
                    SemanticCodeKind::BorrowedFunctionPointer => {
                        return Err(unknown(
                            "original borrowed code requires complete actual invocation proof",
                        ));
                    }
                }))
            }
            SemanticTypeRecipe::Array { element, length } => {
                let element = self.recipe_value(instance, element, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "original array needs complete element class occurrence transport",
                    ));
                }
                let length = self.constant_recipe(instance, length, &Type::Usize, b, depth + 1)?;
                let Constant::Usize(length) = length else {
                    return Err(fail("original array length has another constant carrier"));
                };
                let length = u32::try_from(length).map_err(|_| {
                    original::resource("original array length exceeds its physical representation")
                })?;
                value::unary(geometry::Unary::Array(length), element, b)
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
                    self.push_value(&mut types, field, b)?;
                }
                value::tuple(types, b)
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
    #[track_caller]
    pub(super) fn push_value<T>(
        &self,
        out: &mut Vec<T>,
        value: T,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let relocation = if out.len() == out.capacity() {
            out.len()
        } else {
            0
        };
        b.spend(relocation.saturating_add(1), WORK)?;
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
            SemanticActual::Type(actual) => Ok(Argument::Type(self.resolved(
                instance,
                actual,
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
                let carrier = self.resolved(instance, carrier, b, depth + 1)?;
                if !carrier.classes.is_empty() || constant.ty() != carrier.ty {
                    return Err(fail("semantic constant has another complete carrier"));
                }
                Ok(Argument::Const {
                    carrier: carrier.into_type(b)?,
                    value: constant.clone(),
                })
            }
            _ => Err(fail(
                "Core actual remains in an executable original context",
            )),
        }
    }
    fn constant_recipe(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticConstRecipe,
        carrier: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Constant> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original constant instantiation exceeds its bounded depth",
            ));
        }
        match recipe {
            SemanticConstRecipe::Value(value) if value.ty() == *carrier => {
                b.spend(1, WORK)?;
                Ok(value.clone())
            }
            SemanticConstRecipe::IntegerLiteral(text) => {
                original::mixed::integer_constant(text, carrier, b)
            }
            SemanticConstRecipe::Formal(reference) => {
                let context = self.ancestor(instance, reference.scope, b)?;
                let declaration = self.inventory.definition(context.definition, b)?;
                let Some(SemanticFormal::Const {
                    carrier: original_carrier,
                }) = declaration.parameters.get(reference.ordinal as usize)
                else {
                    return Err(fail("original constant selects another formal kind"));
                };
                let original_carrier =
                    self.recipe_value(context, original_carrier, b, depth + 1)?;
                let Some(SemanticActual::Const { ty, value }) =
                    context.actuals.get(reference.ordinal as usize)
                else {
                    return Err(fail(
                        "original constant has no exact actual at its full ordinal",
                    ));
                };
                let actual = self.resolved(context, ty, b, depth + 1)?;
                if actual != original_carrier
                    || !actual.classes.is_empty()
                    || actual.ty != *carrier
                    || value.ty() != *carrier
                {
                    return Err(fail("original constant loses its full declared carrier"));
                }
                Ok(value.clone())
            }
            _ => Err(unknown(
                "original constant expression requires its complete scoped evaluation",
            )),
        }
    }
    pub(super) fn argument_recipe(
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
                    carrier: carrier.into_type(b)?,
                    value: constant.clone(),
                })
            }
            _ => Err(unknown(
                "original mixed actual needs its complete carrier-aware expression substitution",
            )),
        }
    }
    pub(super) fn nominal_value(
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
        let closed = arguments.is_empty() && declaration.parameters.is_empty()
            && declaration.environment.is_empty() && declaration.requirements.is_empty()
            && self.trait_queries.borrow().is_empty();
        let key = (definition, depth);
        if closed {
            let entries = self.closed_nominals.borrow();
            b.lookup(entries.len(), "closed nominal declaration lookup")?;
            if let Some(value) = entries.get(&key) {
                return value.copy(b);
            }
        }
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
        let value = value::nominal(definition, arguments, &fields, self.program, self.original,
            self.inventory, b, depth + 1)?;
        if closed && value.classes.is_empty() && value.life == value::empty() {
            let mut entries = self.closed_nominals.borrow_mut();
            b.lookup(entries.len(), "closed nominal declaration publication")?;
            entries.insert(key, value.copy(b)?);
        }
        Ok(value)
    }
    fn nominal_field(
        &self,
        owner: SemanticDefinitionId,
        args: &[Argument],
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        self.nominal_field_value(owner, args, recipe, b, depth)
    }
}

impl Check<'_, '_> {
    /// Classify the complete intrinsic nominal domain without choosing an
    /// external image or generating a memory root. Each local class receives
    /// its own token while reconstructing the original application.
    pub(super) fn nominal_inventory(&self, ty: &Type, b: &mut Budget) -> CompileResult<()> {
        self.nominal_inventory_source(ty, b).map(|_| ())
    }
    pub(super) fn nominal_inventory_source(
        &self,
        ty: &Type,
        b: &mut Budget,
    ) -> CompileResult<(SemanticDefinitionId, Vec<Argument>)> {
        self.nominal_inventory_source_at(ty, b, 0)
    }
    pub(super) fn nominal_inventory_source_at(
        &self,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(SemanticDefinitionId, Vec<Argument>)> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual nominal exceeds its complete substitution depth",
            ));
        }
        let source_life = match ty {
            Type::BorrowedStorage { life, .. } => Some(life),
            _ => None,
        };
        let geometry = self.inventory.geometry.type_value(ty, source_life, b)?;
        let index = crate::stored_flow::nominal_instance_index(
            self.module,
            &geometry.ty,
            self.inventory.limits,
            b,
        )?;
        let instance = &self.module.life_instances[index];
        let index = u32::try_from(index).map_err(|_| fail("nominal inventory ordinal overflow"))?;
        let cached = {
            let cache = self.nominal_definitions.borrow();
            b.lookup(cache.len(), WORK)?;
            cache.get(&instance.declaration).copied()
        };
        let definition = if let Some(definition) = cached {
            definition
        } else {
            let mut selected = None;
            for candidate in self.inventory.definitions.values() {
                b.spend(1, WORK)?;
                if matches!(candidate.body,SemanticDefinitionBody::Nominal {declaration,..} if declaration==instance.declaration)
                    && selected.replace(candidate.id).is_some()
                {
                    return Err(fail("nominal type has ambiguous original definitions"));
                }
            }
            let definition =
                selected.ok_or_else(|| fail("nominal type has no full original definition"))?;
            let mut cache = self.nominal_definitions.borrow_mut();
            b.lookup(cache.len(), WORK)?;
            cache.insert(instance.declaration, definition);
            definition
        };
        // Only identity lookup was reused. Every current scope, complete
        // argument image and intrinsic N/kappa is reconstructed below.
        if !matches!(self.inventory.definition(definition, b)?.body, SemanticDefinitionBody::Nominal { declaration, .. } if declaration == instance.declaration)
        {
            return Err(fail(
                "nominal inventory changes its exact original declaration",
            ));
        }
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
                    let source_life = match argument {
                        LifeNominalActual::QualifiedType { life, .. } => Some(life),
                        _ => match ty {
                            Type::BorrowedStorage { life, .. } => Some(life),
                            _ => None,
                        },
                    };
                    let geometry = self.inventory.geometry.type_value(ty, source_life, b)?;
                    let shape = &geometry.life;
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
                    Argument::Type(Value::from_geometry(geometry, classes))
                }
                _ => return Err(fail("nominal actual has no complete local domain")),
            };
            self.push_value(&mut args, argument, b)?;
        }
        if self.nominal_value(definition, &args, b, depth)?.ty != *ty {
            return Err(fail(
                "nominal inventory differs from full original substitution",
            ));
        }
        Ok((definition, args))
    }
    pub(super) fn has_ghost_capture(&self, id: ClosureId, b: &mut Budget) -> CompileResult<bool> {
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        let definition = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == id)
            .ok_or_else(|| fail("capture transport has no actual closure definition"))?;
        b.spend(definition.captures.len(), WORK)?;
        // The explicit head/result relation schedules the stronger original
        // all-creators/all-returns theorem below; it supplies no source fact.
        if crate::stored_callables::receiver_access_head(definition) {
            return Ok(true);
        }
        if definition.captures.iter().any(|ty| {
            !ty.is_b0_scalar()
                && !matches!(ty, Type::Address { mutable: false, inner } if inner.is_b0_scalar())
        }) {
            return Ok(true);
        }
        // A moved reference has the complete class of the original lexical
        // value. It is not an extra implicit Address borrowing a scalar slot.
        // Metadata only selects the stronger all-creators proof; it cannot
        // establish either mode, the source class or a captured permission.
        for world in self.function_worlds(definition.function, b)? {
            b.spend(world.header_embedding.len(), WORK)?;
            if world.header_embedding.iter().any(|row| {
                matches!(row.header, StorageBinder::Fixed(_))
                    && !matches!(
                        row.actual,
                        SemanticClassExpr::Implicit {
                            scope: SemanticScopeRef::Ancestor(0),
                            ..
                        }
                    )
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn defining<'a>(
        &self,
        function: &'a TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<(BlockId, usize, &'a Instruction)> {
        let definition = crate::shared_definition(function, value, b)?;
        let instruction = function
            .blocks
            .get(definition.block.0 as usize)
            .filter(|block| block.id == definition.block)
            .and_then(|block| block.instructions.get(definition.instruction))
            .filter(|instruction| instruction.result == Some(value))
            .ok_or_else(|| fail("semantic SSA definition differs from its actual instruction"))?;
        Ok((definition.block, definition.instruction, instruction))
    }
    pub(super) fn capture_value_origin(
        &self,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<u32> {
        self.capture_origin_value(function, value, b, depth)
    }
    pub(super) fn capture_local_origin(
        &self,
        function: &TypedFunction,
        local: LocalId,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<u32> {
        self.capture_origin_local(function, local, at, b, depth)
    }
    pub(super) fn form_ghost_capture_directory(
        &self,
        closure: &ClosureDefinition,
        instance: &SemanticInstance,
        b: &mut Budget,
    ) -> CompileResult<Vec<BTreeSet<(u32, u32)>>> {
        let id = closure.id;
        let definition = self.inventory.definition(instance.definition, b)?;
        let function = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(closure.function), b)?;
        let contract = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| unknown("ghost capture domain has no complete class contract"))?;
        b.spend(contract.classes.len(), WORK)?;
        if contract
            .classes
            .iter()
            .any(|row| row.position.base == StorageBase::Receiver)
        {
            return Err(fail(
                "Receiver cannot contain captured or intrinsic semantic classes",
            ));
        }
        if function.storage_life.as_ref() != Some(contract)
            || contract.bindings.iter().any(|row| {
                !matches!(
                    row.position.base,
                    StorageBase::Parameter(_)
                        | StorageBase::Result
                        | StorageBase::Capture(_)
                        | StorageBase::Receiver
                )
            })
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
        let SemanticDefinitionBody::FunctionWithBody {
            sites: recipes,
            signature,
            ..
        } = &definition.body
        else {
            return Err(fail("ghost capture omits its complete original body"));
        };
        b.spend(contract.fixed_binder_count as usize, WORK)?;
        let mut directory = vec![BTreeSet::new(); contract.fixed_binder_count as usize];
        let mut mapped = BTreeSet::new();
        let mut used_rows = BTreeSet::new();
        let mut used_slots = BTreeSet::new();
        let creator_bindings = self.capture_creator_bindings(instance, closure, b)?;
        let mut static_captures = BTreeSet::new();
        let mut static_result = false;
        for position in &contract.static_bindings {
            b.spend(position.path.len().saturating_add(1), WORK)?;
            if position.base == StorageBase::Result {
                if !position.path.is_empty() || static_result {
                    return Err(fail(
                        "Static closure result repeats or projects its whole occurrence",
                    ));
                }
                let expected = self.recipe_value(instance, &signature.result, b, 0)?;
                if !original::strings::is_byte_type(&expected.ty)
                    || !empty(&expected.life)
                    || !expected.classes.is_empty()
                    || closure.result != expected.ty
                {
                    return Err(fail(
                        "Static closure result changes its complete original type and domain",
                    ));
                }
                self.static_byte_returns(function, &expected.ty, b)?;
                static_result = true;
                continue;
            }
            let StorageBase::Capture(capture) = position.base else {
                return Err(unknown(
                    "Static closure header needs its complete noncapture original occurrence",
                ));
            };
            b.lookup(static_captures.len(), WORK)?;
            if !static_captures.insert(position.clone()) {
                return Err(fail("Static capture repeats an exact physical occurrence"));
            }
            let mut stored = None;
            for binding in creator_bindings.values() {
                b.spend(1, WORK)?;
                if binding.capture == capture && stored.replace(&binding.stored).is_some() {
                    return Err(fail(
                        "Static capture has competing original creator bindings",
                    ));
                }
            }
            let stored = stored.ok_or_else(|| {
                fail("Static capture lacks its independently proved original creator")
            })?;
            if closure.captures.get(capture as usize) != Some(&stored.ty)
                || !self.has_original_static_byte_path(stored, &position.path, b)?
            {
                return Err(fail(
                    "Static capture changes its complete creator-proved physical leaf",
                ));
            }
        }
        for binding in creator_bindings.values() {
            for path in self.original_static_byte_paths(&binding.stored, b)? {
                b.lookup(static_captures.len(), WORK)?;
                if !static_captures.contains(&StoragePosition {
                    base: StorageBase::Capture(binding.capture),
                    path,
                }) {
                    return Err(fail(
                        "literal capture omits a complete physical Static occurrence",
                    ));
                }
            }
        }
        let receiver_result = crate::stored_callables::receiver_access_head(closure);
        if receiver_result {
            if !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                || function.call_kind != CallKind::Direct
                || !matches!(&closure.result, Type::Address { mutable: false, .. })
                || !closure
                    .lending_captures
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
            {
                return Err(fail(
                    "Receiver result changes its actual reusable shared return mode",
                ));
            }
            let original = self
                .original
                .receiver_sources(instance.definition, self.inventory, b)?
                .ok_or_else(|| {
                    fail("Receiver header has no independent original captured return proof")
                })?;
            let mut physical = BTreeSet::new();
            for ordinal in original {
                b.lookup(creator_bindings.len(), WORK)?;
                let binding = creator_bindings.get(&ordinal).ok_or_else(|| {
                    fail("Receiver source has no exact creator-to-capture binding")
                })?;
                b.lookup(physical.len(), WORK)?;
                physical.insert(binding.capture);
            }
            b.spend(
                physical
                    .len()
                    .saturating_add(closure.lending_captures.len()),
                WORK,
            )?;
            if physical
                .iter()
                .copied()
                .ne(closure.lending_captures.iter().copied())
            {
                return Err(fail(
                    "Receiver lending directory changes the complete original capture sources",
                ));
            }
        }
        let strong_lending = !receiver_result
            && !closure.lending_captures.is_empty()
            && matches!(
                crate::signature_life::outer(contract, StorageBase::Result),
                Some(StorageBinder::Fixed(_))
            )
            && matches!(closure.result, Type::Address { mutable: false, .. });
        if strong_lending {
            let original = self
                .original
                .receiver_sources(instance.definition, self.inventory, b)?
                .ok_or_else(|| {
                    fail("Fixed lending header has no complete original captured return proof")
                })?;
            let mut physical = BTreeSet::new();
            for ordinal in original {
                b.lookup(creator_bindings.len(), WORK)?;
                let binding = creator_bindings
                    .get(&ordinal)
                    .ok_or_else(|| fail("Fixed lending source has no exact creator capture"))?;
                b.lookup(physical.len(), WORK)?;
                physical.insert(binding.capture);
            }
            b.spend(
                physical
                    .len()
                    .saturating_add(closure.lending_captures.len()),
                WORK,
            )?;
            if physical
                .iter()
                .copied()
                .ne(closure.lending_captures.iter().copied())
            {
                return Err(fail(
                    "Fixed lending directory changes its original captured return sources",
                ));
            }
        }
        if !closure.lending_captures.is_empty() && !receiver_result && !strong_lending {
            if !static_result
                || !closure
                    .lending_captures
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
            {
                return Err(unknown(
                    "consuming Static result needs its whole original capture and return relation",
                ));
            }
            for capture in &closure.lending_captures {
                b.spend(creator_bindings.len(), WORK)?;
                let binding = creator_bindings
                    .values()
                    .find(|binding| binding.capture == *capture)
                    .ok_or_else(|| {
                        fail("lending capture omits its independently bound original source")
                    })?;
                let paths = self.original_static_byte_paths(&binding.stored, b)?;
                if paths.is_empty()
                    || !empty(&binding.stored.life)
                    || !binding.stored.classes.is_empty()
                {
                    return Err(unknown(
                        "consuming result retains a non-Static captured head",
                    ));
                }
                for path in paths {
                    b.lookup(static_captures.len(), WORK)?;
                    if !static_captures.contains(&StoragePosition {
                        base: StorageBase::Capture(*capture),
                        path,
                    }) {
                        return Err(fail(
                            "lending capture omits a proved Static field occurrence",
                        ));
                    }
                }
            }
            // Every listed capture is creator-grounded literal storage and
            // every actual Return is independently Static. No owned head or
            // environment-relative loan is lent through this exception.
        }
        for binding in creator_bindings.values() {
            self.ghost_capture_rows(
                instance,
                closure,
                contract,
                binding.capture,
                &binding.stored,
                &mut directory,
                &mut mapped,
                &mut used_rows,
                &mut used_slots,
                b,
            )?;
        }
        for recipe in recipes {
            let Some(ordinal) =
                self.original
                    .capture_recipe_ordinal(instance.definition, recipe, b)?
            else {
                continue;
            };
            let Some(SemanticActual::Type(actual)) = environment.actuals.get(ordinal as usize)
            else {
                return Err(fail(
                    "ghost captured name changes its original environment kind",
                ));
            };
            let expected = self.resolved(parent, actual, b, 0)?;
            b.lookup(creator_bindings.len(), WORK)?;
            if let Some(binding) = creator_bindings.get(&ordinal) {
                if binding.lexical != expected {
                    return Err(fail(
                        "captured name changes its creator-sealed complete environment value",
                    ));
                }
                // The all-creators proof above fixes the physical binding.
                // A mutable scalar's current value may be a Binary/Call result;
                // it is not required to remain the initial ReadCapture SSA.
                // Actual name uses, writes, data/loans and class transport are
                // still checked at their individual operation boundaries.
                continue;
            }
            let weak_mutex = matches!(&expected.ty,Type::Desc(inner) if matches!(inner.as_ref(),Type::Mutex {payload} if **payload==Type::I32));
            if (!expected.life.slots.is_empty() && !weak_mutex)
                || !expected.life.retained.is_empty()
            {
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
                    SemanticOperand::Capture(index) => *index,
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
                if let Some(binding) = creator_bindings.get(&ordinal)
                    && binding.capture != capture
                {
                    return Err(fail(
                        "actual captured read selects another original lexical binding",
                    ));
                }
                self.ghost_capture_rows(
                    instance,
                    closure,
                    contract,
                    capture,
                    &expected,
                    &mut directory,
                    &mut mapped,
                    &mut used_rows,
                    &mut used_slots,
                    b,
                )?;
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
                && !matches!(ty, Type::Address { .. } | Type::Desc(_))
            {
                // Complete capture rows above already retain nonzero child
                // domains. Classify that exact child without reinterpreting
                // an opaque Closure/Package as a zero-domain environment.
                self.ordinary_type(ty, b)?;
            }
        }
        if used_slots.len()
            != contract
                .bindings
                .iter()
                .filter(|row| matches!(row.position.base, StorageBase::Capture(_)))
                .count()
            || used_rows.len()
                != contract
                    .classes
                    .iter()
                    .filter(|row| matches!(row.position.base, StorageBase::Capture(_)))
                    .count()
            || instance
                .header_embedding
                .iter()
                .filter(|row| matches!(row.header, StorageBinder::Fixed(_)))
                .count()
                != directory.len()
        {
            return Err(fail(
                "ghost capture adds or omits a complete class/header occurrence",
            ));
        }
        let mut result = Vec::new();
        for entry in directory {
            if entry.is_empty() {
                return Err(fail(
                    "ghost capture Fixed class has no actual original occurrence",
                ));
            }
            self.push_value(&mut result, entry, b)?;
        }
        Ok(result)
    }

    pub(super) fn capture_class_headers(
        &self,
        contract: &StorageLifeContract,
        capture: u32,
        value: &Value,
        class: u32,
        b: &mut Budget,
    ) -> CompileResult<Vec<(usize, bool, StorageBinder)>> {
        let outer = matches!(value.ty, Type::Address { .. } | Type::Desc(_));
        let mut selected = Vec::new();
        let mut children = Vec::new();
        if outer {
            let [slot] = value.life.slots.as_slice() else {
                return Err(fail("captured head loses its complete outer occurrence"));
            };
            if !slot.path.is_empty() {
                return Err(fail("captured head has a projected outer occurrence"));
            }
            if slot.binder == class {
                let mut head = None;
                for (index, row) in contract.bindings.iter().enumerate() {
                    b.spend(row.position.path.len().saturating_add(1), WORK)?;
                    if row.position.base == StorageBase::Capture(capture)
                        && row.position.path.is_empty()
                        && head.replace((index, true, row.binder)).is_some()
                    {
                        return Err(fail(
                            "captured outer head repeats its actual header occurrence",
                        ));
                    }
                }
                self.push_value(
                    &mut selected,
                    head.ok_or_else(|| {
                        fail("captured outer head omits its actual header occurrence")
                    })?,
                    b,
                )?;
            }
            for row in self.signature_retained_classes(value, b)? {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                if row.binder == class {
                    self.push_value(&mut children, (row.path.as_slice(), row.class), b)?;
                }
            }
        } else if self.task_domain(&value.ty, b)?.is_some() && !value.life.retained.is_empty() {
            for row in self.signature_retained_classes(value, b)? {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                if row.binder == class {
                    self.push_value(&mut children, (row.path.as_slice(), row.class), b)?;
                }
            }
        } else {
            self.push_value(&mut children, (&[][..], class), b)?;
        }
        for (path, binder) in self.task_ports(value, b)? {
            if binder != class {
                continue;
            }
            let mut selected_port = None;
            for (index, row) in contract.bindings.iter().enumerate() {
                b.spend(row.position.path.len().saturating_add(path.len() + 1), WORK)?;
                if row.position.base == StorageBase::Capture(capture)
                    && row.position.path == path
                    && selected_port.replace((index, true, row.binder)).is_some()
                {
                    return Err(fail("captured Task repeats its physical retained port"));
                }
            }
            self.push_value(
                &mut selected,
                selected_port
                    .ok_or_else(|| fail("captured Task omits its actual physical retained port"))?,
                b,
            )?;
        }
        // Keep every explicit child-kappa selector even when several images
        // coincide. Header/domain indices are never interned by their image.
        for (path, child) in children {
            let mut row = None;
            for (index, candidate) in contract.classes.iter().enumerate() {
                b.spend(
                    candidate.position.path.len().saturating_add(path.len() + 1),
                    WORK,
                )?;
                if candidate.position.base == StorageBase::Capture(capture)
                    && candidate.position.path == path
                    && candidate.position.class == child
                    && row.replace((index, false, candidate.binder)).is_some()
                {
                    return Err(fail(
                        "captured intrinsic child repeats its actual header occurrence",
                    ));
                }
            }
            self.push_value(
                &mut selected,
                row.ok_or_else(|| {
                    fail("captured head omits a complete original child class occurrence")
                })?,
                b,
            )?;
        }
        if selected.is_empty() {
            return Err(fail(
                "captured head omits its complete intrinsic child embedding",
            ));
        }
        Ok(selected)
    }
    #[allow(clippy::too_many_arguments)]
    fn ghost_capture_rows(
        &self,
        instance: &SemanticInstance,
        closure: &ClosureDefinition,
        contract: &StorageLifeContract,
        capture: u32,
        expected: &Value,
        directory: &mut [BTreeSet<(u32, u32)>],
        mapped: &mut BTreeSet<u32>,
        used_rows: &mut BTreeSet<usize>,
        used_slots: &mut BTreeSet<usize>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let ty = closure
            .captures
            .get(capture as usize)
            .ok_or_else(|| fail("captured name selects an absent actual capture"))?;
        if *ty != expected.ty {
            return Err(fail("captured name has another complete environment type"));
        }
        b.lookup(mapped.len(), WORK)?;
        mapped.insert(capture);
        let mut fixed = BTreeMap::new();
        for (child, class) in expected.classes.iter().enumerate() {
            for (row, physical, binder) in
                self.capture_class_headers(contract, capture, expected, child as u32, b)?
            {
                let StorageBinder::Fixed(index) = binder else {
                    return Err(fail("capture persistent class is not Fixed"));
                };
                b.lookup(fixed.len(), WORK)?;
                if fixed.insert(index, child).is_some_and(|old| old != child) {
                    return Err(fail(
                        "capture collapses two intrinsic child classes into one Fixed class",
                    ));
                }
                let header = self.header_class(instance, binder, b)?;
                if header != *class && header != self.capture_class_image(class.clone(), b)? {
                    b.spend(192, WORK)?;
                    let mut errors =
                        fail("captured class differs from the sealed original environment");
                    errors[0].notes.push(format!(
                        "capture world {}:{} definition {}:{} entry {} capture {} child {} binder {:?}: original {:?}, actual header {:?}",
                        instance.id.module.0, instance.id.index, instance.definition.module.0,
                        instance.definition.index, closure.function.0, capture, child, binder, class, header));
                    return Err(errors);
                }
                let slot = directory
                    .get_mut(index as usize)
                    .ok_or_else(|| fail("capture Fixed class is out of range"))?;
                // Each occurrence has independently matched the exact original
                // class above. Keep all capture/child selectors; a common
                // lifetime image never equates their real owner/source roots.
                b.lookup(slot.len(), WORK)?;
                slot.insert((capture, child as u32));
                if physical {
                    b.lookup(used_slots.len(), WORK)?;
                    used_slots.insert(row);
                } else {
                    b.lookup(used_rows.len(), WORK)?;
                    used_rows.insert(row);
                }
            }
        }
        Ok(())
    }
    pub(super) fn header_class(
        &self,
        instance: &SemanticInstance,
        binder: StorageBinder,
        b: &mut Budget,
    ) -> CompileResult<Class> {
        let class = self.header_class_raw(instance, binder, b)?;
        self.capture_class_image(class, b)
    }
    fn header_class_raw(
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
        let expression = selected
            .ok_or_else(|| fail("actual header binder has no complete original embedding"))?;
        if matches!(expression, SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Receiver, path, mode: StoredBorrowMode::SharedAddress,
        } if path.is_empty())
        {
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                return Err(fail("Receiver header class has no executing function"));
            };
            let function = self.inventory.function(target, b)?;
            let closure = function
                .environment
                .and_then(|id| self.module.closures.get(id.0 as usize))
                .filter(|row| row.function == function.id)
                .ok_or_else(|| fail("Receiver header class is outside a real closure entry"))?;
            let header = function
                .storage_life
                .as_ref()
                .ok_or_else(|| fail("Receiver class omits its actual entry header"))?;
            let StorageBinder::Invocation(index) = binder else {
                return Err(fail(
                    "Receiver class cannot replace a captured Fixed identity",
                ));
            };
            if function.call_kind != CallKind::Direct
                || !crate::stored_callables::receiver_access_head(closure)
                || crate::signature_life::outer(header, StorageBase::Receiver) != Some(binder)
                || index >= header.invocation_binder_count
            {
                return Err(fail(
                    "Receiver class changes its explicit actual head/result relation",
                ));
            }
            self.original
                .receiver_sources(instance.definition, self.inventory, b)?
                .ok_or_else(|| fail("Receiver class lacks its original captured return proof"))?;
            return Ok(Class::Invocation(instance.id, index));
        }
        if let SemanticClassExpr::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = expression
            && let Some(class) = self.captured_implicit_head(instance, binder, *ordinal, b)?
        {
            return Ok(class);
        }
        if let SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Parameter(parameter),
            path,
            mode,
        } = expression
        {
            return self.parameter_header_class(
                instance,
                binder,
                *parameter,
                path,
                Some(*mode),
                None,
                b,
            );
        }
        if let SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::Parameter(parameter),
            path,
            class,
        } = expression
        {
            return self.parameter_header_class(
                instance,
                binder,
                *parameter,
                path,
                None,
                Some(*class),
                b,
            );
        }
        value::class(instance, expression, self.inventory, b, 0)
    }
}

impl Check<'_, '_> {
    fn contains_nonzero_closure(
        &self,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.contains_nonzero_child(ty, false, b, depth)
    }

    fn contains_nonzero_child(
        &self,
        ty: &Type,
        include_nominal: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "retained child inventory exceeds its finite type depth",
            ));
        }
        match ty {
            Type::Closure(id) => {
                let scope = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|entry| entry.id == *id)
                    .and_then(|entry| entry.storage_life.as_ref())
                    .ok_or_else(|| unknown("closure child has no complete Fixed inventory"))?;
                Ok(scope.fixed_binder_count != 0)
            }
            Type::BorrowedStorage { life, .. } if include_nominal && life.binder_count != 0 => Ok(true),
            Type::BorrowedStorage { storage, .. }
            | Type::Managed(storage)
            | Type::Array {
                element: storage, ..
            }
            | Type::Slice { element: storage }
            | Type::Address { inner: storage, .. } => {
                self.contains_nonzero_child(storage, include_nominal, b, depth + 1)
            }
            Type::Tuple(fields) => {
                for field in fields {
                    if self.contains_nonzero_child(field, include_nominal, b, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, field) in fields {
                    if self.contains_nonzero_child(field, include_nominal, b, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for field in fields {
                        if self.contains_nonzero_child(field, include_nominal, b, depth + 1)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            Type::ErrorUnion { error, result } => {
                Ok(self.contains_nonzero_child(error, include_nominal, b, depth + 1)?
                    || self.contains_nonzero_child(result, include_nominal, b, depth + 1)?)
            }
            _ => Ok(false),
        }
    }

    /// Select from an independently reconstructed whole value, preserving its
    /// complete child directory. Equal class images never choose a child row.
    fn check_child_embedding(
        &self,
        parent: &Value,
        selector: PlaceProjection,
        child: &Value,
        direct: &BTreeMap<u32, u32>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let mut storage = &parent.ty;
        while let Type::BorrowedStorage { storage: inner, .. } = storage {
            storage = inner;
        }
        let prefix = if matches!(storage, Type::Array { .. }) {
            LifeTypeProjection::Element
        } else {
            LifeTypeProjection::Place(selector)
        };
        let binder = |index: u32, b: &mut Budget| {
            b.lookup(direct.len(), WORK)?;
            direct
                .get(&index)
                .copied()
                .ok_or_else(|| fail("child embedding omits an original intrinsic class"))
        };
        let mut expected = value::empty();
        let mut actual = value::empty();
        let mut width = 1usize;
        for row in &child.life.slots {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            let mut path = vec![selector];
            path.extend_from_slice(&row.path);
            width = width.max(path.len().saturating_add(1));
            self.push_value(
                &mut expected.slots,
                StorageLifeSlot {
                    path,
                    binder: binder(row.binder, b)?,
                    mode: row.mode,
                },
                b,
            )?;
        }
        for row in &child.life.retained {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            let mut path = vec![prefix];
            path.extend_from_slice(&row.path);
            width = width.max(path.len().saturating_add(1));
            self.push_value(
                &mut expected.retained,
                StorageRetainedBinding {
                    path,
                    class: row.class,
                    binder: binder(row.binder, b)?,
                },
                b,
            )?;
        }
        for row in &child.life.nominal_arguments {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            let mut path = vec![prefix];
            path.extend_from_slice(&row.path);
            width = width.max(path.len().saturating_add(1));
            self.push_value(
                &mut expected.nominal_arguments,
                NominalLifeArgument {
                    path,
                    declaration: row.declaration,
                    argument: row.argument,
                    binder: binder(row.binder, b)?,
                },
                b,
            )?;
        }
        for row in &parent.life.slots {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.first() == Some(&selector) {
                width = width.max(row.path.len().saturating_add(1));
                self.push_value(&mut actual.slots, row.clone(), b)?;
            }
        }
        for row in &parent.life.retained {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.first() == Some(&prefix) && row.path.len() > 1 {
                width = width.max(row.path.len().saturating_add(1));
                self.push_value(&mut actual.retained, row.clone(), b)?;
            }
        }
        for row in &parent.life.nominal_arguments {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.first() == Some(&prefix) {
                width = width.max(row.path.len().saturating_add(1));
                self.push_value(&mut actual.nominal_arguments, row.clone(), b)?;
            }
        }
        fn rows_equal<T: PartialEq>(
            mut actual: Vec<T>,
            expected: &[T],
            width: usize,
            b: &mut Budget,
        ) -> CompileResult<bool> {
            if actual.len() != expected.len() {
                return Ok(false);
            }
            for row in expected {
                b.spend(actual.len().saturating_mul(width).saturating_add(1), WORK)?;
                let Some(index) = actual.iter().position(|candidate| candidate == row) else {
                    return Ok(false);
                };
                actual.swap_remove(index);
            }
            Ok(actual.is_empty())
        }
        if !rows_equal(actual.slots, &expected.slots, width, b)?
            || !rows_equal(actual.retained, &expected.retained, width, b)?
            || !rows_equal(
                actual.nominal_arguments,
                &expected.nominal_arguments,
                width,
                b,
            )?
        {
            return Err(fail(
                "child projection changes its complete retained, borrow or phantom forest",
            ));
        }
        Ok(())
    }

    fn closure_child_value(
        &self,
        parent: &Value,
        selector: PlaceProjection,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let mut storage = &parent.ty;
        while let Type::BorrowedStorage { storage: inner, .. } = storage {
            storage = inner;
        }
        if let (Type::Record { .. } | Type::Package { .. }, PlaceProjection::Field(field)) =
            (storage, selector)
        {
            return self.actual_nominal_field(parent, field, b, 0);
        }
        if let (Type::Enum { variants, .. }, PlaceProjection::Variant(place)) = (storage, selector)
        {
            let expected = variants
                .get(place.variant as usize)
                .and_then(|(_, fields)| fields.get(place.index as usize))
                .ok_or_else(|| {
                    fail("nominal variant projection selects an absent physical field")
                })?;
            let (definition, mut arguments) = self.nominal_inventory_source(&parent.ty, b)?;
            for argument in &mut arguments {
                let mut image = |class: &mut Class| -> CompileResult<()> {
                    b.spend(1, WORK)?;
                    let Class::Nominal(_, binder) = class else {
                        return Err(fail("variant field has another original coordinate domain"));
                    };
                    *class = parent
                        .classes
                        .get(*binder as usize)
                        .cloned()
                        .ok_or_else(|| {
                            fail("variant field leaves its whole original class domain")
                        })?;
                    Ok(())
                };
                match argument {
                    Argument::Life(class) => image(class)?,
                    Argument::Type(value) => {
                        for class in &mut value.classes {
                            image(class)?;
                        }
                    }
                    Argument::Const { .. } => {}
                }
            }
            let SemanticDefinitionBody::Nominal {
                body: SemanticNominalBody::Enum { variants },
                ..
            } = &self.inventory.definition(definition, b)?.body
            else {
                return Err(fail(
                    "variant projection has another original declaration kind",
                ));
            };
            let recipe = variants
                .get(place.variant as usize)
                .and_then(|variant| variant.fields.get(place.index as usize))
                .ok_or_else(|| fail("variant projection has no original full field ordinal"))?;
            let selected = self.nominal_field_value(definition, &arguments, recipe, b, 1)?;
            if !geometry::same_type(&selected.ty, expected, b)? {
                return Err(fail(
                    "variant field changes its complete original substitution",
                ));
            }
            return Ok(selected);
        }
        let child = match (storage, selector) {
            (Type::Tuple(fields), PlaceProjection::Field(field)) => fields.get(field as usize),
            (Type::Array { element, length }, PlaceProjection::Field(index)) if index < *length => {
                Some(element.as_ref())
            }
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error)) => {
                Some(if is_error {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            _ => None,
        }
        .ok_or_else(|| {
            unknown("closure child projection needs its complete original structural source")
        })?;
        if let PlaceProjection::Field(field) = selector
            && matches!(storage, Type::Tuple(_))
            && empty(&parent.life)
            && parent.classes.is_empty()
            && !self.physical_byte_paths(child, b)?.is_empty()
        {
            return parent.empty_tuple_child(field, b);
        }
        let prefix = if matches!(storage, Type::Array { .. }) {
            LifeTypeProjection::Element
        } else {
            LifeTypeProjection::Place(selector)
        };
        let template = match child {
            Type::Record { .. } | Type::Package { .. } | Type::BorrowedStorage { .. } => {
                let (definition, arguments) = self.nominal_inventory_source(child, b)?;
                let template = self.nominal_value(definition, &arguments, b, 0)?;
                if !geometry::same_type(&template.ty, child, b)? {
                    return Err(fail(
                        "projected nominal child changes its complete original application",
                    ));
                }
                Some(template)
            }
            _ => None,
        };
        let mut direct = BTreeMap::new();
        for row in &parent.life.retained {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.as_slice() == [prefix] {
                b.lookup(direct.len(), WORK)?;
                if direct.insert(row.class, row.binder).is_some() {
                    return Err(fail(
                        "closure child repeats a complete intrinsic occurrence",
                    ));
                }
            }
        }
        let count = match child {
            Type::Closure(id) => self
                .module
                .closures
                .get(id.0 as usize)
                .filter(|entry| entry.id == *id)
                .and_then(|entry| entry.storage_life.as_ref())
                .map(|scope| scope.fixed_binder_count)
                .ok_or_else(|| unknown("projected closure lacks its complete Fixed inventory"))?,
            Type::Tuple(_)
            | Type::Address { .. }
            | Type::Desc(_)
            | Type::Array { .. }
            | Type::ErrorUnion { .. } => direct
                .keys()
                .next_back()
                .map(|last| {
                    last.checked_add(1)
                        .ok_or_else(|| fail("closure tuple class count overflow"))
                })
                .transpose()?
                .unwrap_or(0),
            _ if template.is_some() => template.as_ref().unwrap().life.binder_count,
            _ => {
                return Err(unknown(
                    "projected owning child requires its complete original domain constructor",
                ));
            }
        };
        if count as usize != direct.len() {
            return Err(fail(
                "closure child omits a complete intrinsic or phantom class",
            ));
        }
        if count == 0 {
            self.zero_data(child, b, 0)?;
        }
        let mut classes = Vec::new();
        for class in 0..count {
            b.lookup(direct.len(), WORK)?;
            let binder = *direct
                .get(&class)
                .ok_or_else(|| fail("closure child class directory has a gap"))?;
            self.push_value(
                &mut classes,
                parent
                    .classes
                    .get(binder as usize)
                    .ok_or_else(|| fail("closure child class leaves its complete parent domain"))?
                    .clone(),
                b,
            )?;
        }
        if let Some(template) = template {
            self.check_child_embedding(parent, selector, &template, &direct, b)?;
            return value::borrowed_full(child, &template.life, classes, self.inventory, b);
        }
        let child_binder = |binder: u32, b: &mut Budget| -> CompileResult<u32> {
            b.spend(direct.len().saturating_add(1), WORK)?;
            let mut matches = direct.iter().filter(|(_, parent)| **parent == binder);
            let (child, _) = matches
                .next()
                .ok_or_else(|| fail("nested closure child leaves its whole directory"))?;
            if matches.next().is_some() {
                return Err(unknown(
                    "nested closure child needs its exact noninjective original embedding",
                ));
            }
            Ok(*child)
        };
        let mut life = value::empty();
        life.binder_count = count;
        for row in &parent.life.slots {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.first() == Some(&selector) {
                let binder = child_binder(row.binder, b)?;
                self.push_value(
                    &mut life.slots,
                    StorageLifeSlot {
                        path: row.path[1..].to_vec(),
                        binder,
                        mode: row.mode,
                    },
                    b,
                )?;
            }
        }
        for row in &parent.life.retained {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.first() == Some(&prefix) && row.path.len() > 1 {
                let binder = child_binder(row.binder, b)?;
                self.push_value(
                    &mut life.retained,
                    StorageRetainedBinding {
                        path: row.path[1..].to_vec(),
                        class: row.class,
                        binder,
                    },
                    b,
                )?;
            }
        }
        for row in &parent.life.nominal_arguments {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            if row.path.first() == Some(&prefix) {
                let binder = child_binder(row.binder, b)?;
                self.push_value(
                    &mut life.nominal_arguments,
                    NominalLifeArgument {
                        path: row.path[1..].to_vec(),
                        declaration: row.declaration,
                        argument: row.argument,
                        binder,
                    },
                    b,
                )?;
            }
        }
        value::borrowed_full(child, &life, classes, self.inventory, b)
    }

    #[allow(clippy::too_many_arguments)]
    fn rebuilt_package(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        (block, index): (BlockId, usize),
        ty: &Type,
        fields: &[ValueId],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Value>> {
        let Type::Package { fields: declaration, .. } = crate::resources::package_storage(ty)
        else { return Ok(None); };
        let Some(start) = index.checked_sub(fields.len()) else { return Ok(None); };
        if fields.is_empty() || fields.len() != declaration.len() {
            return Ok(None);
        }
        let body = function.blocks.get(block.0 as usize)
            .filter(|body| body.id == block)
            .ok_or_else(|| fail("rebuilt package loses its exact actual block"))?;
        let mut local = None;
        for (ordinal, ((instruction, operand), (_, field))) in body.instructions[start..index]
            .iter().zip(fields).zip(declaration).enumerate()
        {
            b.type_work(field, false, WORK)?;
            let (source, selected, take) = match instruction.kind {
                InstructionKind::TakePackageField { local, index } => (local, index, true),
                InstructionKind::ReadPackageField { local, index } => (local, index, false),
                _ => return Ok(None),
            };
            if instruction.result != Some(*operand)
                || selected as usize != ordinal
                || take && field.is_copy_value()
                || !take && !field.is_copy_value()
                    && !crate::callable_fields::copy_value(self.module, field, b, 0)?
                || local.is_some_and(|local| local != source)
            {
                return Ok(None);
            }
            local = Some(source);
        }
        let local = local.expect("checked nonempty reconstruction");
        b.spend(self.program.sites.len(), WORK)?;
        if self.program.sites.iter().any(|site| {
            site.instance == instance.id
                && matches!(site.action, SemanticSiteAction::Construct { .. })
                && matches!(site.at, SemanticSiteAddress::Typed {
                    block: owner, instruction: point, edge: SemanticSiteEdge::After,
                } if owner == block && point as usize == index)
        }) {
            return Ok(None);
        }
        operations::initialized_local_before(function, (block, start), local, b)?;
        let parent = self.local_value(instance, function, local, (block, start), b, depth + 1)?;
        if !geometry::same_type(&parent.ty, ty, b)? {
            return Err(fail("rebuilt package changes its complete original occupant"));
        }
        for (ordinal, operand) in fields.iter().enumerate() {
            operations::available_before(function, (block, index, SemanticSiteEdge::Before), *operand, b)?;
            let expected = self.actual_nominal_field(&parent, ordinal as u32, b, depth + 1)?;
            let actual = self.ssa_value(instance, function, *operand, b, depth + 1)?;
            if !actual.equal_paid(&expected, b)? {
                return Err(fail("rebuilt package changes an original field value"));
            }
        }
        Ok(Some(parent))
    }

    pub(super) fn ssa_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        let ty = function
            .values
            .get(id.0 as usize)
            .ok_or_else(|| fail("semantic value transport selects an absent SSA"))?;
        self.inventory.geometry.check_type(ty, b)?;
        if let Some(slot) = self.persistent_array_slot(instance, function, id, b)? {
            let child = self.ssa_value(instance, function, slot.initial(), b, depth + 1)?;
            return self.persistent_slot_array_value(instance, function, &slot, &child, b);
        }
        if ty.is_b0_scalar() {
            return Ok(value::scalar(ty.clone()));
        }
        if let Type::Closure(id) = ty {
            let closure = self
                .module
                .closures
                .get(id.0 as usize)
                .filter(|closure| closure.id == *id)
                .ok_or_else(|| fail("actual closure SSA has no exact definition"))?;
            if closure
                .storage_life
                .as_ref()
                .is_some_and(|life| life.fixed_binder_count == 0)
            {
                // The count selects a proof obligation, never proves it:
                // reconstruct every real capture and original environment.
                self.zero_data(ty, b, 0)?;
                return value::borrowed_scalar(ty, self.inventory, b);
            }
        }
        if matches!(
            ty,
            Type::FunctionPointer { .. }
                | Type::ForeignCallback { .. }
                | Type::Managed(_)
                | Type::Tuple(_)
                | Type::Array { .. }
                | Type::Record { .. }
                | Type::Package { .. }
                | Type::Enum { .. }
                | Type::ErrorUnion { .. }
        ) && !self.dyn_sum_type(ty, b)?
            && self
                .task_domain(ty, b)?
                .is_none_or(|life| life.binder_count == 0)
            && !self.contains_nonzero_child(ty, true, b, 0)?
            && self.physical_byte_paths(ty, b)?.is_empty()
        {
            // A complete independent zero-domain reconstruction needs no
            // predecessor SSA class equation. Actual initialization, source,
            // loans and cleanup remain obligations of each operation/kernel.
            // This is not an empty-shape shortcut: zero_data walks the full
            // nominal/opaque inventory and rejects nonzero or Unknown scopes.
            self.zero_data(ty, b, 0)?;
            return value::borrowed_scalar(ty, self.inventory, b);
        }
        if depth > b.partial_depth {
            let mut errors =
                unknown("semantic value transport exceeds its finite actual SSA depth");
            errors[0].notes.push(format!(
                "actual function {} SSA {} original world {}:{}",
                function.id.0, id.0, instance.id.module.0, instance.id.index
            ));
            return Err(errors);
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        if matches!(ty, Type::Desc(_))
            && let Some(value) = self.source_description_at(instance, function, id, b, depth + 1)?
        {
            return Ok(value);
        }
        if let InstructionKind::MakePackage { fields } = &instruction.kind
            && let Some(value) = self.rebuilt_package(instance, function, (block, index), ty, fields, b, depth + 1)?
        {
            return Ok(value);
        }
        match &instruction.kind {
            InstructionKind::MakeErrorUnion {
                is_error,
                value: payload,
            } => {
                let definition = self.inventory.definition(instance.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    signature,
                    sites: recipes,
                    ..
                } = &definition.body
                else {
                    return Err(fail(
                        "actual error carrier has no complete original function body",
                    ));
                };
                let mut expected: Option<Value> = None;
                for site in &self.program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != instance.id {
                        continue;
                    }
                    let SemanticSiteAction::TypeUse {
                        subject: SemanticOperand::Value(actual),
                        ..
                    } = &site.action
                    else {
                        continue;
                    };
                    if *actual != id {
                        continue;
                    }
                    b.spend(recipes.len(), WORK)?;
                    let recipe = recipes
                        .iter()
                        .find(|recipe| recipe.id == site.recipe)
                        .ok_or_else(|| {
                            fail("actual error carrier omits its original type-use recipe")
                        })?;
                    let SemanticSiteRecipeKind::TypeUse { expected: source } = &recipe.kind else {
                        return Err(fail(
                            "actual error carrier substitutes another original operation",
                        ));
                    };
                    let original = self.recipe_value(instance, source, b, depth + 1)?;
                    if let Some(old) = &expected {
                        if !old.equal_paid(&original, b)? {
                            return Err(fail(
                                "actual error carrier has competing original whole values",
                            ));
                        }
                    }
                    expected = Some(original);
                }
                if expected.is_none() {
                    // Compiler-generated error forwarding has no new source
                    // expression. Its exact Return uses the original ABI's
                    // whole result, including the inactive success domain.
                    let mut returned = false;
                    for actual in &function.blocks {
                        b.spend(1, WORK)?;
                        if matches!(actual.terminator, Terminator::Return(Some(value)) if value == id)
                        {
                            returned = true;
                        }
                    }
                    if !returned {
                        return Err(unknown(
                            "actual error carrier needs its exact original construction or return boundary",
                        ));
                    }
                    expected =
                        Some(self.recipe_value(instance, &signature.result, b, depth + 1)?);
                }
                let expected = expected
                    .ok_or_else(|| fail("actual error carrier lost its original whole result"))?;
                if !geometry::same_type(&expected.ty, ty, b)? {
                    return Err(fail(
                        "actual error carrier changes its complete original result type",
                    ));
                }
                let Type::ErrorUnion { error, result } = &expected.ty else {
                    return Err(fail(
                        "actual error carrier replaces its original sum constructor",
                    ));
                };
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *payload,
                    b,
                )?;
                let selected = if *is_error {
                    error.as_ref()
                } else {
                    result.as_ref()
                };
                let actual = self.ssa_value(instance, function, *payload, b, depth + 1)?;
                if !geometry::same_type(&actual.ty, selected, b)? {
                    return Err(fail("actual error carrier changes its active payload type"));
                }
                if *is_error {
                    // Task errors have an independently reconstructed zero
                    // domain. They cannot provide or erase success classes.
                    self.zero_data(error, b, depth + 1)?;
                    if !actual.classes.is_empty() || !empty(&actual.life) {
                        return Err(unknown(
                            "error carrier needs its complete nonzero error-payload embedding",
                        ));
                    }
                } else if let Some(life) = self.task_domain(selected, b)? {
                    let child = self.task_projection(
                        &expected,
                        selected,
                        PlaceProjection::ErrorArm(false),
                        b,
                    )?;
                    if child.life != life || !self.capture_values_equal(&actual, &child, b)? {
                        return Err(fail(
                            "actual success carrier changes its complete original child classes",
                        ));
                    }
                } else if self.contains_nonzero_closure(selected, b, depth + 1)? {
                    let child =
                        self.closure_child_value(&expected, PlaceProjection::ErrorArm(false), b)?;
                    if !self.capture_values_equal(&actual, &child, b)? {
                        return Err(fail(
                            "actual success carrier changes its complete original closure domain",
                        ));
                    }
                } else {
                    return Err(unknown(
                        "success carrier needs its complete active child-domain embedding",
                    ));
                }
                Ok(expected)
            }
            InstructionKind::ReadPackageField {
                local,
                index: field,
            }
            | InstructionKind::TakePackageField {
                local,
                index: field,
            } if self.contains_nonzero_closure(ty, b, 0)?
                || !self.physical_byte_paths(ty, b)?.is_empty() =>
            {
                let parent =
                    self.local_value(instance, function, *local, (block, index), b, depth + 1)?;
                let selected =
                    self.closure_child_value(&parent, PlaceProjection::Field(*field), b)?;
                if !geometry::same_type(&selected.ty, ty, b)? {
                    return Err(fail(
                        "actual closure field projection changes its complete child type",
                    ));
                }
                self.check_static_value_sources(&selected, function, id, b)?;
                Ok(selected)
            }
            InstructionKind::TakeTupleField { local, path }
            | InstructionKind::TakeVariantField { local, path }
                if self.contains_nonzero_closure(ty, b, 0)?
                    || !self.physical_byte_paths(ty, b)?.is_empty() =>
            {
                let mut selected =
                    self.local_value(instance, function, *local, (block, index), b, depth + 1)?;
                for selector in path {
                    b.spend(1, WORK)?;
                    selected = self.closure_child_value(&selected, *selector, b)?;
                }
                if !geometry::same_type(&selected.ty, ty, b)? {
                    return Err(fail(
                        "actual owning closure projection changes its whole selected child",
                    ));
                }
                self.check_static_value_sources(&selected, function, id, b)?;
                Ok(selected)
            }
            InstructionKind::TakeOwnedArrayElement {
                local,
                index: element,
            }
            | InstructionKind::BorrowOwnedArrayElement {
                local,
                index: element,
                ..
            } => {
                if *element != 0 {
                    return Err(fail(
                        "persistent closure read selects another actual element",
                    ));
                }
                let slot = self
                    .persistent_slot(instance, function, *local, b)?
                    .ok_or_else(|| {
                        unknown("closure element read lacks its complete original slot witness")
                    })?;
                let child = self.persistent_slot_value(
                    instance,
                    function,
                    &slot,
                    (block, index),
                    b,
                    depth + 1,
                )?;
                let result = match &instruction.kind {
                    InstructionKind::TakeOwnedArrayElement { .. } => child,
                    InstructionKind::BorrowOwnedArrayElement { mutable, .. } => {
                        value::address(*mutable, Class::Reborrow(function.id, id), child, b)?
                    }
                    _ => unreachable!(),
                };
                if !geometry::same_type(&result.ty, ty, b)? {
                    return Err(fail(
                        "persistent closure read changes its complete actual result",
                    ));
                }
                Ok(result)
            }
            InstructionKind::TaskCreate { .. } | InstructionKind::TaskCreateOwned { .. }
            | InstructionKind::TaskPoll { .. } | InstructionKind::TaskPollClose { .. } => {
                self.task_result(instance, function, block, index, b)
            }
            InstructionKind::CallDynSlot { .. } => self
                .trait_result(instance, function, block, index, b)?
                .ok_or_else(|| fail("dynamic result has no exact original member call receipt")),
            InstructionKind::MakeDynOwned { .. } => {
                self.dyn_owned_result(instance, function, block, index, b)
            }
            InstructionKind::BorrowDynOwned { local, mutable } => self.dyn_owned_borrow(
                instance,
                function,
                id,
                *local,
                *mutable,
                (block, index),
                b,
                depth + 1,
            ),
            InstructionKind::StaticUtf8 { bytes } => {
                b.spend(bytes.len().saturating_add(1), WORK)?;
                if !original::strings::is_byte_type(ty) {
                    return Err(fail(
                        "actual UTF8 literal changes its complete immutable byte-view type",
                    ));
                }
                value::borrowed_scalar(ty, self.inventory, b)
            }
            InstructionKind::OwnedStoreNew { .. }
            | InstructionKind::OwnedStoreReserve { .. }
            | InstructionKind::OwnedStorePush { .. }
            | InstructionKind::OwnedStorePop { .. }
            | InstructionKind::OwnedStoreLen { .. }
            | InstructionKind::OwnedStoreCapacity { .. }
            | InstructionKind::OwnedStoreBorrow { .. }
            | InstructionKind::OwnedStoreBorrowMut { .. } => {
                self.store_result(instance, function, block, index, b)
            }
            InstructionKind::ByteBufferBorrow { .. } => {
                self.buffer_result(instance, function, block, index, b)
            }
            InstructionKind::DescribeLocal { .. }
            | InstructionKind::DescribeAddress { .. }
            | InstructionKind::DescribeCapture { .. }
            | InstructionKind::DescribePackageField { .. } => {
                self.source_description(instance, function, id, b, depth + 1)
            }
            InstructionKind::MutexCreate { .. }
            | InstructionKind::MutexLock { .. }
            | InstructionKind::MutexValue { .. }
            | InstructionKind::WithScopedThread { .. } => {
                self.synchronization_result(instance, function, block, index, b)
            }
            InstructionKind::CallClosure { callee, .. }
            | InstructionKind::CallClosureOnce { callee, .. } => {
                if let Some(result) =
                    self.abstract_operation(instance, function, block, index, b)?
                {
                    return Ok(result);
                }
                let receiver = self.ssa_value(instance, function, *callee, b, depth + 1)?;
                self.concrete_call_output(instance, block, index, &receiver, ty, b)
            }
            InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. } => {
                if let Some(result) =
                    self.abstract_operation(instance, function, block, index, b)?
                {
                    return Ok(result);
                }
                let receiver =
                    self.local_value(instance, function, *callee, (block, index), b, depth + 1)?;
                self.concrete_call_output(instance, block, index, &receiver, ty, b)
            }
            InstructionKind::CallClosureBorrowed { callee, .. } => {
                if let Some(result) =
                    self.abstract_operation(instance, function, block, index, b)?
                {
                    return Ok(result);
                }
                let receiver = self.ssa_value(instance, function, *callee, b, depth + 1)?;
                let Type::Address { inner, .. } = &receiver.ty else {
                    return Err(fail(
                        "borrowed known callable result has no actual environment address",
                    ));
                };
                if !matches!(inner.as_ref(), Type::Closure(_)) {
                    return Err(fail(
                        "borrowed known callable result changes its actual closure identity",
                    ));
                }
                // BodyCall separately proves the exact original lexical place,
                // requested mode, fresh borrow and whole pointee class value.
                let whole = self.pointee(&receiver, b)?;
                self.concrete_call_output(instance, block, index, &whole, ty, b)
            }
            InstructionKind::ReadAddress { address } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *address,
                    b,
                )?;
                let parent = self.ssa_value(instance, function, *address, b, depth + 1)?;
                let child = self.pointee(&parent, b)?;
                if !geometry::same_type(&child.ty, ty, b)? {
                    return Err(fail(
                        "actual pointer-slot read changes its complete original child Type",
                    ));
                }
                // This is the stored pointer's whole domain, not a new view of
                // the slot. Actual pointer provenance and parents remain in
                // the memory/loan pass for this exact ReadAddress operation.
                Ok(child)
            }
            InstructionKind::LoadStoredBorrow {
                storage,
                path,
                mode,
            } => {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *storage,
                    b,
                )?;
                let root = self.ssa_value(instance, function, *storage, b, depth + 1)?;
                let mut selected = if matches!(root.ty, Type::Address { .. }) {
                    self.pointee(&root, b)?
                } else {
                    return Err(fail(
                        "stored reference read has no actual addressed aggregate source",
                    ));
                };
                for projection in path {
                    let PlaceProjection::Field(field) = projection else {
                        return Err(unknown(
                            "stored reference read needs its complete active Sum/array child source",
                        ));
                    };
                    selected = self.actual_nominal_field(&selected, *field, b, depth + 1)?;
                }
                let valid = match mode {
                    StoredBorrowLoad::CopyShared => {
                        matches!(selected.ty, Type::Address { mutable: false, .. })
                    }
                    StoredBorrowLoad::CopyDescription => matches!(selected.ty, Type::Desc(_)),
                    StoredBorrowLoad::Take => {
                        matches!(selected.ty, Type::Address { .. } | Type::Desc(_))
                    }
                    StoredBorrowLoad::Reborrow { .. } => false,
                };
                if !valid || !geometry::same_type(&selected.ty, ty, b)? {
                    return Err(unknown(
                        "stored reference read changes its full original type or requires its exact reborrow role",
                    ));
                }
                // Full actual stored-flow retains current cells, parent loans,
                // shared/exclusive mode and consuming state independently.
                Ok(selected)
            }
            InstructionKind::TakeVariantField { local, path }
                if self.dyn_sum_type(
                    &function
                        .locals
                        .get(local.0 as usize)
                        .ok_or_else(|| fail("Dyn arm extraction has no actual local"))?
                        .ty,
                    b,
                )? =>
            {
                self.dyn_owned_local_projection(
                    instance,
                    function,
                    id,
                    *local,
                    path,
                    (block, index),
                    b,
                    depth + 1,
                )
            }
            InstructionKind::TakeVariantField { local, path }
                if path.len() == 1 && (self.task_domain(ty, b)?.is_some()
                    || matches!(ty, Type::Address { mutable: false, .. })) =>
            {
                operations::initialized_local_before(function, (block, index), *local, b)?;
                let parent = self.local_value(instance, function, *local, (block, index), b, depth + 1)?;
                self.projected_child(function, id, &parent, path[0], b, depth + 1)
            }
            InstructionKind::ReadVariant {
                value: parent,
                variant,
                index: field,
            } => self.projected_shared_value(
                instance,
                function,
                id,
                *parent,
                PlaceProjection::Variant(zil_ir::VariantProjection {
                    variant: *variant,
                    index: *field,
                }),
                (block, index),
                b,
                depth + 1,
            ),
            InstructionKind::ReadErrorUnion {
                value: parent,
                is_error,
            }
            | InstructionKind::TakeErrorUnion {
                value: parent,
                is_error,
            } => {
                if self.contains_nonzero_child(ty, true, b, 0)? {
                    operations::available_before(
                        function,
                        (block, index, SemanticSiteEdge::Before),
                        *parent,
                        b,
                    )?;
                    let parent = self.ssa_value(instance, function, *parent, b, depth + 1)?;
                    let selected =
                        self.closure_child_value(&parent, PlaceProjection::ErrorArm(*is_error), b)?;
                    if !geometry::same_type(&selected.ty, ty, b)? {
                        return Err(fail(
                            "actual error-arm projection changes its complete retained child",
                        ));
                    }
                    return Ok(selected);
                }
                if self.dyn_sum_type(
                    function
                        .values
                        .get(parent.0 as usize)
                        .ok_or_else(|| fail("actual sum projection has no parent type"))?,
                    b,
                )? {
                    return self.dyn_owned_projection(
                        instance,
                        function,
                        id,
                        *parent,
                        *is_error,
                        (block, index),
                        b,
                        depth + 1,
                    );
                }
                self.projected_shared_value(
                    instance,
                    function,
                    id,
                    *parent,
                    PlaceProjection::ErrorArm(*is_error),
                    (block, index),
                    b,
                    depth + 1,
                )
            }
            InstructionKind::UpdateField {
                aggregate,
                index: field,
                value: replacement,
            } if matches!(ty, Type::Record { .. }) => {
                let boundary = (block, index, SemanticSiteEdge::Before);
                operations::available_before(function, boundary, *aggregate, b)?;
                operations::available_before(function, boundary, *replacement, b)?;
                let parent = self.ssa_value(instance, function, *aggregate, b, depth + 1)?;
                if !geometry::same_type(&parent.ty, ty, b)? {
                    return Err(fail("actual record update changes its original product type"));
                }
                let field = self.actual_nominal_field(&parent, *field, b, depth + 1)?;
                let actual = self.ssa_value(instance, function, *replacement, b, depth + 1)?;
                self.check_static_value_sources(&field, function, *replacement, b)?;
                if !actual.equal_paid(&field, b)? {
                    return Err(fail("actual record update changes its complete field value"));
                }
                Ok(parent)
            }
            InstructionKind::ExtractField {
                aggregate,
                index: field,
            } => {
                if !self.physical_byte_paths(ty, b)?.is_empty() {
                    operations::available_before(
                        function,
                        (block, index, SemanticSiteEdge::Before),
                        *aggregate,
                        b,
                    )?;
                    let parent = self.ssa_value(instance, function, *aggregate, b, depth + 1)?;
                    if empty(&parent.life)
                        && parent.classes.is_empty()
                        && matches!(parent.ty, Type::Tuple(_))
                    {
                        let selected = self.static_structural_child(&parent, *field, b)?;
                        if !geometry::same_type(&selected.ty, ty, b)? {
                            return Err(fail(
                                "Static Tuple extraction changes its exact child type",
                            ));
                        }
                        self.check_static_value_sources(&selected, function, id, b)?;
                        return Ok(selected);
                    }
                }
                if self.contains_nonzero_closure(ty, b, 0)? {
                    operations::available_before(
                        function,
                        (block, index, SemanticSiteEdge::Before),
                        *aggregate,
                        b,
                    )?;
                    let parent = self.ssa_value(instance, function, *aggregate, b, depth + 1)?;
                    let selected =
                        self.closure_child_value(&parent, PlaceProjection::Field(*field), b)?;
                    if !geometry::same_type(&selected.ty, ty, b)? {
                        return Err(fail(
                            "actual tuple extraction changes its complete closure child",
                        ));
                    }
                    return Ok(selected);
                }
                self.projected_shared_value(
                    instance,
                    function,
                    id,
                    *aggregate,
                    PlaceProjection::Field(*field),
                    (block, index),
                    b,
                    depth + 1,
                )
            }
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
                // The empty domain here is obtained from the actual literal
                // or its exact transport above. An immutable byte literal
                // remains immortal when the emitter inserts a shared reborrow.
                if !*mutable
                    && original::strings::is_byte_type(&parent.ty)
                    && parent.ty == *ty
                    && empty(&parent.life)
                    && parent.classes.is_empty()
                {
                    return Ok(parent);
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
                    let mut errors =
                        fail("actual reborrow changes its whole pointee or outer occurrence");
                    errors[0].notes.push(format!(
                        "function {} SSA {} parent {}: {:?}; required {:?}",
                        function.id.0, id.0, address.0, parent, ty
                    ));
                    return Err(errors);
                }
                let child = self.pointee(&parent, b)?;
                let result = value::address(*mutable, Class::Reborrow(function.id, id), child, b)?;
                // This distinct outer occurrence may adapt to its actual
                // parent only within a proved call role; it is never Eq(parent).
                Ok(result)
            }
            InstructionKind::AddressSlice { address, .. } => {
                if let Some(value) = self.source_borrow(instance, function, id, b, depth + 1)? {
                    return Ok(value);
                }
                self.range_projection_value(instance, function, id, *address, b, depth + 1)
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                self.local_value(instance, function, *local, (block, index), b, depth + 1)
            }
            InstructionKind::BorrowCapture {
                index: capture,
                mutable,
            } => {
                if let Some(value) = self.source_borrow(instance, function, id, b, depth + 1)? {
                    return Ok(value);
                }
                let closure = function
                    .environment
                    .and_then(|id| self.module.closures.get(id.0 as usize))
                    .filter(|entry| entry.function == function.id)
                    .ok_or_else(|| {
                        fail("actual capture borrow has another complete environment entry")
                    })?;
                if *mutable && closure.kind == CallableKind::Fn {
                    return Err(fail(
                        "actual shared environment cannot mutably borrow a capture",
                    ));
                }
                let whole = self.capture_whole_value(instance, function, *capture, b, depth + 1)?;
                if !matches!(ty, Type::Address { mutable: output, inner }
                    if output == mutable && inner.as_ref() == &whole.ty)
                {
                    return Err(fail(
                        "actual capture borrow changes its complete child type or mode",
                    ));
                }
                // A genuine per-operation head is separate from the captured
                // child's full Fixed domain and grants no access/cleanup fact.
                value::address(*mutable, Class::Reborrow(function.id, id), whole, b)
            }
            InstructionKind::ReadCapture { index: capture }
            | InstructionKind::TakeCapture { index: capture } => {
                self.capture_whole_value(instance, function, *capture, b, depth + 1)
            }
            InstructionKind::MakeVariant { variant, fields } => {
                let mut selected = None;
                for site in &self.program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != instance.id
                        || !matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After }
                            if actual == block && instruction as usize == index)
                    {
                        continue;
                    }
                    let Some(constructor) = self.original.enum_constructor(
                        instance,
                        site.recipe,
                        self.program,
                        self.inventory,
                        b,
                    )?
                    else {
                        continue;
                    };
                    if constructor.variant != *variant
                        || constructor.payload.len() != fields.len()
                        || !geometry::same_type(&constructor.result.ty, ty, b)?
                    {
                        return Err(fail(
                            "actual Sum construction changes its complete original variant or result domain",
                        ));
                    }
                    for (operand, expected) in fields.iter().zip(&constructor.payload) {
                        operations::available_before(
                            function,
                            (block, index, SemanticSiteEdge::Before),
                            *operand,
                            b,
                        )?;
                        if !geometry::same_type(
                            function
                                .values
                                .get(operand.0 as usize)
                                .ok_or_else(|| fail("actual Sum payload is absent"))?,
                            &expected.ty,
                            b,
                        )? || !self
                            .ssa_value(instance, function, *operand, b, depth + 1)?
                            .equal_paid(expected, b)?
                        {
                            return Err(fail(
                                "actual Sum payload changes its independently checked complete original class value",
                            ));
                        }
                    }
                    if selected
                        .as_ref()
                        .is_some_and(|old| old != &constructor.result)
                    {
                        return Err(fail(
                            "actual Sum constructor has competing original domain occurrences",
                        ));
                    }
                    selected = Some(constructor.result);
                }
                // Inactive alternatives still retain the complete original
                // phantom/type-argument domain. This transports no payload
                // address, active tag, owner or permission for an empty arm.
                selected.ok_or_else(|| {
                    fail("class-bearing Sum constructor lacks its exact original occurrence")
                })
            }
            InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields } => {
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
                if !geometry::same_type(&expected.ty, ty, b)? {
                    return Err(fail(
                        "actual aggregate changes its complete original physical type",
                    ));
                }
                let storage = match &expected.ty {
                    Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                    ty => ty,
                };
                let (count, nominal) = match storage {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        (fields.len(), true)
                    }
                    Type::Tuple(fields) => (fields.len(), false),
                    Type::Array { length, .. } => (*length as usize, false),
                    _ => {
                        return Err(fail(
                            "actual aggregate has another complete constructor layout",
                        ));
                    }
                };
                if count != fields.len() {
                    return Err(fail("actual aggregate omits an original physical field"));
                }
                // The original whole value retains every mixed/phantom class.
                // Each physical child is proved separately; field equality is
                // never inferred from a shared image or the parent's head.
                for (ordinal, operand) in fields.iter().enumerate() {
                    operations::available_before(
                        function,
                        (block, index, SemanticSiteEdge::Before),
                        *operand,
                        b,
                    )?;
                    let field = if nominal {
                        self.actual_nominal_field(
                            &expected,
                            u32::try_from(ordinal)
                                .map_err(|_| fail("actual aggregate field ordinal overflow"))?,
                            b,
                            depth + 1,
                        )?
                    } else {
                        let field = match storage {
                            Type::Tuple(fields) => &fields[ordinal],
                            Type::Array { element, .. } => element.as_ref(),
                            _ => unreachable!(),
                        };
                        if matches!(storage, Type::Tuple(_))
                            && !self.physical_byte_paths(field, b)?.is_empty()
                        {
                            self.static_structural_child(
                                &expected,
                                u32::try_from(ordinal)
                                    .map_err(|_| fail("Static field ordinal overflow"))?,
                                b,
                            )?
                        } else {
                            // Class-bearing structural tuples/arrays require
                            // their own original embedding, never invented rows.
                            self.zero_data(field, b, depth + 1)?;
                            value::borrowed_scalar(field, self.inventory, b)?
                        }
                    };
                    let actual = self.ssa_value(instance, function, *operand, b, depth + 1)?;
                    self.check_static_value_sources(&field, function, *operand, b)?;
                    if !actual.equal_paid(&field, b)? {
                        if !self.outer_call_role(
                            instance,
                            function,
                            (block, index),
                            ordinal,
                            *operand,
                            &actual,
                            &field,
                            b,
                            depth + 1,
                        )? {
                            b.spend(
                                actual
                                    .classes
                                    .len()
                                    .saturating_add(field.classes.len())
                                    .saturating_mul(128)
                                    .saturating_add(256),
                                WORK,
                            )?;
                            let mut errors = fail(
                                "actual aggregate child changes its complete original data or directed outer role",
                            );
                            errors[0].notes.push(format!("actual function {} aggregate SSA {} field {} operand SSA {}: {:?}; independently reconstructed original field {:?}", function.id.0, id.0, ordinal, operand.0, actual, field));
                            return Err(errors);
                        }
                        let ordinal = u32::try_from(ordinal)
                            .map_err(|_| fail("actual constructor field ordinal overflows"))?;
                        b.lookup(b.semantic_constructor_views.len(), WORK)?;
                        b.semantic_constructor_views
                            .insert((function.id, id, ordinal));
                    }
                }
                Ok(expected)
            }
            InstructionKind::BorrowPackageField {
                local,
                index: field,
                mutable,
            } => {
                if let Some(value) = self.source_borrow(instance, function, id, b, depth + 1)? {
                    return Ok(value);
                }
                operations::initialized_local_before(function, (block, index), *local, b)?;
                let parent =
                    self.local_value(instance, function, *local, (block, index), b, depth + 1)?;
                let child = self.closure_child_value(&parent, PlaceProjection::Field(*field), b)?;
                let output = value::address(*mutable, Class::Reborrow(function.id, id), child, b)?;
                if !geometry::same_type(&output.ty, ty, b)? {
                    return Err(fail(
                        "borrowed field changes its complete original child type",
                    ));
                }
                Ok(output)
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
                            let value = self.resolved(instance, expected, b, depth + 1)?;
                            if selected.as_ref().is_some_and(|old| old != &value) {
                                return Err(fail(
                                    "borrow SSA has conflicting original class occurrences",
                                ));
                            }
                            selected = Some(value);
                        }
                    }
                }
                let expected = if let Some(expected) = selected {
                    expected
                } else {
                    // Emitter-created capture heads need no invented original
                    // TypeUse. Their fresh outer occurrence belongs to this
                    // actual borrow producer; the complete child still comes
                    // from the real local's reaching value at this point.
                    let (local, mutable) = match instruction.kind {
                        InstructionKind::BorrowLocal { local } => (local, false),
                        InstructionKind::BorrowLocalMut { local } => (local, true),
                        _ => unreachable!(),
                    };
                    let child =
                        self.local_value(instance, function, local, (block, index), b, depth + 1)?;
                    value::address(mutable, Class::Reborrow(function.id, id), child, b)?
                };
                if expected.ty != *ty {
                    return Err(fail("borrow SSA loses its complete original type"));
                }
                Ok(expected)
            }
            InstructionKind::MakeClosure {
                closure: id,
                captures,
            } => {
                let stored_bounds = if self.has_ghost_capture(*id, b)? {
                    self.ghost_creator(instance, *id, captures, b)?
                } else {
                    self.creator_heads(instance, *id, captures, b)?;
                    BTreeMap::new()
                };
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
                // Physical heads live in bindings; intrinsic and Pointee
                // classes live in classes. Use the same complete occurrence
                // projector as the independent creator/header checks.
                for (capture, source) in captures.iter().enumerate() {
                    b.lookup(stored_bounds.len(), WORK)?;
                    let source = if let Some(bound) = stored_bounds.get(&capture) {
                        bound.copy(b)?
                    } else {
                        self.ssa_value(instance, function, *source, b, depth + 1)?
                    };
                    for (child, class) in source.classes.iter().enumerate() {
                        for (_, _, binder) in self.capture_class_headers(
                            contract,
                            capture as u32,
                            &source,
                            child as u32,
                            b,
                        )? {
                            let StorageBinder::Fixed(fixed) = binder else {
                                return Err(fail(
                                    "closure constructor capture has a non-Fixed class",
                                ));
                            };
                            let slot = classes.get_mut(fixed as usize).ok_or_else(|| {
                                fail("closure constructor Fixed class is out of range")
                            })?;
                            if slot.as_ref().is_some_and(|old| old != class) {
                                return Err(unknown(
                                    "closure constructor requires its complete original equality relation",
                                ));
                            }
                            *slot = Some(class.clone());
                        }
                    }
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
                value::borrowed_full(
                    ty,
                    &StorageLifeShape {
                        binder_count: contract.fixed_binder_count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    complete,
                    self.inventory,
                    b,
                )
            }
            InstructionKind::Call {
                function: callee,
                arguments,
            }
            | InstructionKind::SuspendCall {
                function: callee,
                arguments,
            } => {
                let suspend = matches!(instruction.kind, InstructionKind::SuspendCall { .. });
                let entry = self
                    .inventory
                    .function(&SemanticFunctionTarget::Typed(*callee), b)?;
                if entry.call_kind
                    != (if suspend {
                        CallKind::Suspend
                    } else {
                        CallKind::Direct
                    })
                    || entry.foreign.is_some()
                    || (suspend && function.call_kind != CallKind::Suspend)
                {
                    return Err(fail(
                        "actual call result changes its original execution convention",
                    ));
                }
                if let Some(value) = self.trait_result(instance, function, block, index, b)? {
                    return Ok(value);
                }
                let expected_parameters =
                    self.original_call_inputs(instance, function, block, index, *callee, b)?;
                if expected_parameters.len() != arguments.len() {
                    return Err(fail(
                        "actual call changes its complete original input arity",
                    ));
                }
                let callee = self
                    .inventory
                    .function(&SemanticFunctionTarget::Typed(*callee), b)?;
                // Every class-bearing call transports its complete original
                // result. A list of selected physical constructors omitted
                // Task/ErrorUnion results and lost their child domains.
                let output = self.original_call_output(instance, block, index, b)?;
                if !geometry::same_type(&output.ty, ty, b)? {
                    return Err(fail(
                        "call output changes its complete original result Type",
                    ));
                }
                let original_output = output;
                self.call_output_from_entry(
                    instance,
                    function,
                    (block, index),
                    callee,
                    arguments,
                    &expected_parameters,
                    ty,
                    original_output,
                    None,
                    b,
                    depth,
                )
            }
            _ => {
                let mut errors =
                    unknown("semantic value requires its exact actual class/role transport");
                errors[0].notes.push(format!(
                    "actual function {} SSA {} original world {}:{}",
                    function.id.0, id.0, instance.id.module.0, instance.id.index
                ));
                Err(errors)
            }
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
        let ty = &function
            .locals
            .get(local.0 as usize)
            .filter(|item| item.id == local)
            .ok_or_else(|| fail("semantic class query selects no actual local type"))?
            .ty;
        if let Some(slot) = self.persistent_slot(instance, function, local, b)? {
            let child = self.persistent_slot_value(instance, function, &slot, at, b, depth + 1)?;
            return self.persistent_slot_array_value(instance, function, &slot, &child, b);
        }
        if let Some(value) =
            self.reference_local_value(instance, function, local, at, b, depth + 1)?
        {
            return Ok(value);
        }
        if let Type::Closure(id) = ty {
            let closure = self
                .module
                .closures
                .get(id.0 as usize)
                .filter(|closure| closure.id == *id)
                .ok_or_else(|| fail("actual closure local has no exact definition"))?;
            if closure
                .storage_life
                .as_ref()
                .is_some_and(|life| life.fixed_binder_count == 0)
            {
                self.zero_data(ty, b, 0)?;
                return value::borrowed_scalar(ty, self.inventory, b);
            }
        }
        if ty.is_b0_scalar()
            || matches!(
                ty,
                Type::Managed(_)
                    | Type::Record { .. }
                    | Type::Package { .. }
                    | Type::Tuple(_)
                    | Type::Array { .. }
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
                    | Type::FunctionPointer { .. }
                    | Type::ForeignCallback { .. }
            ) && !self.dyn_sum_type(ty, b)?
                && self
                    .task_domain(ty, b)?
                    .is_none_or(|life| life.binder_count == 0)
                && !self.contains_nonzero_closure(ty, b, 0)?
                && self.physical_byte_paths(ty, b)?.is_empty()
        {
            // A zero-class fact does not depend on which loop iteration last
            // initialized the occupant. Actual initialization and resources
            // are still checked at every consuming operation.
            self.zero_data(ty, b, 0)?;
            if ty.is_b0_scalar() {
                crate::dyn_metadata::ty(ty, b)?;
                return Ok(value::scalar(ty.clone()));
            }
            return value::borrowed_scalar(ty, self.inventory, b);
        }
        if depth > b.partial_depth {
            return Err(unknown(
                "semantic local class transport exceeds its finite CFG depth",
            ));
        }
        // Solve the finite set of reaching local definitions. A CFG backedge
        // does not add type depth or mint a new class; every concrete incoming
        // origin must still produce the same complete value in this world.
        let initialized = crate::shared_initialization(function, b)?;
        let mut pending = vec![(local, at.0, at.1)];
        let mut seen = BTreeSet::new();
        let mut selected: Option<Value> = None;
        while let Some((local, at_block, end)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((local, at_block, end)) {
                continue;
            }
            let block = function
                .blocks
                .get(at_block.0 as usize)
                .filter(|block| block.id == at_block)
                .ok_or_else(|| fail("semantic local class has no actual block"))?;
            let mut origin = None;
            let mut assigned = false;
            for (index, instruction) in block.instructions.iter().enumerate().take(end).rev() {
                b.spend(1, WORK)?;
                match instruction.kind {
                    InstructionKind::BindPackageCapture {
                        local: target,
                        index,
                    } if target == local => {
                        origin = Some(self.capture_whole_value(
                            instance,
                            function,
                            index,
                            b,
                            depth + 1,
                        )?);
                        assigned = true;
                        break;
                    }
                    InstructionKind::EndPackageCapture { local: target, .. } if target == local => {
                        return Err(fail(
                            "semantic Package class read crosses an ended capture view",
                        ));
                    }
                    InstructionKind::Store {
                        local: target,
                        value,
                    } if target == local => {
                        operations::available_before(
                            function,
                            (at_block, index, SemanticSiteEdge::Before),
                            value,
                            b,
                        )?;
                        let (source_block, source_index, source) =
                            self.defining(function, value, b)?;
                        match source.kind {
                            InstructionKind::Load(source)
                            | InstructionKind::TakeLocal { local: source } => {
                                b.spend(1, WORK)?;
                                // The source local may have a distinct, fully
                                // checked mutable-reference declaration role.
                                // Copying its saved pointer keeps that sigma;
                                // following only its initializer would replace
                                // the role by an earlier RHS's lifetime image.
                                if let Some(value) = self.reference_local_value(
                                    instance,
                                    function,
                                    source,
                                    (source_block, source_index),
                                    b,
                                    depth + 1,
                                )? {
                                    origin = Some(value);
                                } else {
                                    pending.push((source, source_block, source_index));
                                }
                            }
                            _ => {
                                origin =
                                    Some(self.ssa_value(instance, function, value, b, depth + 1)?)
                            }
                        }
                        assigned = true;
                        break;
                    }
                    InstructionKind::StorageLive(target)
                    | InstructionKind::StorageDead(target)
                    | InstructionKind::TakeLocal { local: target }
                        if target == local =>
                    {
                        b.spend(512, WORK)?;
                        let mut errors = fail("semantic local class crosses a vacant activation");
                        errors[0].notes.push(format!("actual function {} local {} requested at block {} before instruction {}, reached {:?} at block {} instruction {} in original world {:?}",
                            function.id.0, local.0, at.0.0, at.1, instruction.kind, at_block.0, index, instance.id));
                        return Err(errors);
                    }
                    _ => {}
                }
            }
            if !assigned && at_block == function.entry {
                let parameter = function
                    .parameters
                    .iter()
                    .position(|candidate| *candidate == local)
                    .ok_or_else(|| {
                        fail("semantic local entry has no initialized parameter origin")
                    })?;
                let definition = self.inventory.definition(instance.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    return Err(fail("semantic parameter has no complete original header"));
                };
                origin = Some(
                    self.recipe_value(
                        instance,
                        &signature
                            .parameters
                            .get(parameter)
                            .ok_or_else(|| fail("semantic original parameter is absent"))?
                            .ty,
                        b,
                        depth + 1,
                    )?,
                );
            }
            if let Some(value) = origin {
                value.check_geometry(b)?;
                b.spend(value.classes.len().saturating_add(1), WORK)?;
                if let Some(old) = &selected {
                    if !old.equal_paid(&value, b)? {
                        return Err(unknown(
                            "semantic local needs its complete joined role/source dependency relation",
                        ));
                    }
                }
                selected = Some(value);
            }
            if assigned {
                continue;
            }
            let mut incoming = at_block == function.entry;
            for predecessor in &initialized.predecessors[at_block.0 as usize] {
                b.spend(1, WORK)?;
                if initialized.reachable[*predecessor] {
                    incoming = true;
                    let previous = &function.blocks[*predecessor];
                    pending.push((local, previous.id, previous.instructions.len()));
                }
            }
            if !incoming {
                return Err(fail(
                    "semantic local has no initialized actual class origin",
                ));
            }
        }
        selected.ok_or_else(|| fail("semantic local cycle has no initialized actual class origin"))
    }
}

impl Check<'_, '_> {
    pub(super) fn receiver_return_role(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        value: ValueId,
        actual: &Value,
        expected: &Value,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let Some(closure) = function
            .environment
            .and_then(|id| self.module.closures.get(id.0 as usize))
            .filter(|closure| closure.function == function.id)
        else {
            return Ok(false);
        };
        if function.call_kind != CallKind::Direct
            || !crate::stored_callables::result_is_receiver(closure)
            || !matches!(actual.ty, Type::Address { mutable: false, .. })
            || !geometry::same_type(&actual.ty, &expected.ty, b)?
        {
            return Ok(false);
        }
        if self
            .original
            .receiver_sources(instance.definition, self.inventory, b)?
            .is_none()
            || !self.capture_values_equal(
                &self.pointee(actual, b)?,
                &self.pointee(expected, b)?,
                b,
            )?
        {
            return Ok(false);
        }
        let directory = self.inventory.original_index(instance.definition, b)?;
        let returns = directory.active_returns(b)?;
        let mut selected = false;
        for original in returns {
            let Some(original) = original else {
                return Ok(false);
            };
            if let Some(producer) =
                self.original_subject_value(instance, function, original.id, b)?
            {
                if producer == value {
                    selected = true;
                }
            }
        }
        if !selected {
            return Ok(false);
        }
        let header = function
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("Receiver actual return omits its entry header"))?;
        let binder = crate::signature_life::outer(header, StorageBase::Receiver)
            .ok_or_else(|| fail("Receiver actual return has no access head"))?;
        let [outer] = expected.life.slots.as_slice() else {
            return Ok(false);
        };
        if !outer.path.is_empty()
            || outer.mode != StoredBorrowMode::SharedAddress
            || expected.classes.get(outer.binder as usize)
                != Some(&self.header_class(instance, binder, b)?)
        {
            return Ok(false);
        }
        let actual_block = function
            .blocks
            .get(block.0 as usize)
            .filter(|row| row.id == block)
            .ok_or_else(|| fail("Receiver return has no actual exit block"))?;
        if !matches!(actual_block.terminator, Terminator::Return(Some(returned)) if returned == value)
        {
            return Err(fail("Receiver result adapter selects another actual exit"));
        }
        b.lookup(b.semantic_receiving_returns.len(), WORK)?;
        b.semantic_receiving_returns
            .insert((function.id, block, value, binder));
        // This accepts only the directed outer result view. Every actual
        // source operation, invariant child and final memory ancestry remain
        // separate mandatory checks; no σ/Fixed equality is published.
        Ok(true)
    }

    fn concrete_call_output(
        &self,
        instance: &SemanticInstance,
        block: BlockId,
        index: usize,
        receiver: &Value,
        ty: &Type,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        let Type::Closure(id) = receiver.ty else {
            return Err(fail(
                "concrete invocation has another complete receiver type",
            ));
        };
        let closure = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|row| row.id == id)
            .ok_or_else(|| fail("concrete invocation has no actual closure identity"))?;
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("concrete invocation has no actual caller function"));
        };
        let function = self.inventory.function(target, b)?;
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|row| row.id == block)
            .and_then(|row| row.instructions.get(index))
            .ok_or_else(|| fail("concrete invocation has no actual instruction"))?;
        let arguments = match &instruction.kind {
            InstructionKind::CallClosure { arguments, .. }
            | InstructionKind::CallClosureOnce { arguments, .. }
            | InstructionKind::CallClosureShared { arguments, .. }
            | InstructionKind::CallClosureMut { arguments, .. }
            | InstructionKind::CallClosureBorrowed { arguments, .. } => arguments,
            _ => return Err(fail("concrete invocation changes its actual operation")),
        };
        let entry = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(closure.function), b)?;
        let output = self.original_call_output(instance, block, index, b)?;
        if !geometry::same_type(&output.ty, ty, b)?
            || !geometry::same_type(&entry.return_type, ty, b)?
        {
            return Err(fail(
                "concrete invocation changes its whole original result",
            ));
        }
        let parameters =
            self.original_call_inputs(instance, function, block, index, entry.id, b)?;
        let mut receiving = None;
        let mut subject = None;
        for site in self.inventory.sites_at(
            self.program,
            instance.id,
            block,
            index,
            SemanticSiteEdge::After,
            b,
        )? {
            if !matches!(site.action, SemanticSiteAction::BodyCall { .. }) {
                continue;
            }
            if subject.replace(site).is_some() {
                return Err(fail("concrete invocation repeats its original subject"));
            }
        }
        let site = subject
            .ok_or_else(|| fail("concrete invocation lacks its original operation receipt"))?;
        let source = self.original.borrowed_closure_receiver(
            instance,
            site.recipe,
            self.program,
            self.inventory,
            b,
        )?;
        self.known_call_place(instance, function, (block, index), instruction, &source, b)?;
        if self
            .original
            .call_receiver_view(instance, site.recipe, self.inventory, b)?
        {
            if entry.call_kind != CallKind::Direct
                || !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                || !matches!(output.ty, Type::Address { mutable: false, .. })
            {
                return Err(fail(
                    "Receiver view changes its original reusable Direct shared mode",
                ));
            }
            self.original
                .receiver_sources(
                    self.function_definition(entry.id, b)?,
                    self.inventory,
                    b,
                )?
                .ok_or_else(|| {
                    fail("Receiver result has no complete original all-return source proof")
                })?;
            self.known_receiver_place(instance, function, (block, index), instruction, &source, b)?;
            if !crate::stored_callables::receiver_access_head(closure) {
                if !crate::stored_callables::constant_result(closure) {
                    return Err(fail(
                        "Receiver view has no actual access head or independent Static source",
                    ));
                }
                self.static_byte_returns(entry, ty, b)?;
            }
            let [outer] = output.life.slots.as_slice() else {
                return Err(fail(
                    "Receiver output loses its exact full shared outer occurrence",
                ));
            };
            if !outer.path.is_empty() || outer.mode != StoredBorrowMode::SharedAddress {
                return Err(fail(
                    "Receiver output changes its complete outer projection",
                ));
            }
            self.pointee(&output, b)?;
            receiving = Some(outer.binder);
            let result = instruction
                .result
                .ok_or_else(|| fail("Receiver invocation loses its actual output SSA"))?;
            let key = (function.id, result, id);
            b.lookup(b.semantic_receiver_views.len(), WORK)?;
            if b.semantic_receiver_views
                .insert(key, (block, index))
                .is_some_and(|old| old != (block, index))
            {
                return Err(fail(
                    "Receiver result changes its original operation identity",
                ));
            }
        }
        self.capture_directory(id, b)?;
        crate::stored_callables::check_role_relation(
            self.module,
            closure,
            entry
                .storage_life
                .as_ref()
                .ok_or_else(|| fail("call entry has no complete role directory"))?,
            b,
        )?;
        self.transport_operation_output(
            instance,
            function,
            (block, index),
            entry,
            arguments,
            &parameters,
            output,
            Some(&receiver.classes),
            receiving,
            None,
            b,
            0,
        )
    }

    /// Physical byte leaves only: a closure or owning head does not expose its
    /// hidden environment/retained children as fields of this value.
    pub(super) fn physical_byte_paths(
        &self,
        ty: &Type,
        b: &mut Budget,
    ) -> CompileResult<BTreeSet<Vec<ContractProjection>>> {
        fn visit(
            ty: &Type,
            path: &mut Vec<ContractProjection>,
            out: &mut BTreeSet<Vec<ContractProjection>>,
            b: &mut Budget,
            depth: usize,
        ) -> CompileResult<()> {
            b.spend(path.len().saturating_add(1), WORK)?;
            if depth > b.partial_depth {
                return Err(unknown(
                    "physical Static occurrence traversal exceeds its finite depth",
                ));
            }
            if original::strings::is_byte_type(ty) {
                b.product(
                    out.len().saturating_add(1),
                    path.len().saturating_add(1),
                    1,
                    WORK,
                )?;
                b.spend(path.len().saturating_add(1), WORK)?;
                out.insert(path.clone());
                return Ok(());
            }
            match ty {
                Type::Tuple(fields) => {
                    for (index, child) in fields.iter().enumerate() {
                        b.spend(1, WORK)?;
                        path.push(ContractProjection::Place(PlaceProjection::Field(
                            u32::try_from(index)
                                .map_err(|_| fail("Static field ordinal overflow"))?,
                        )));
                        visit(child, path, out, b, depth + 1)?;
                        path.pop();
                    }
                }
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    for (index, (_, child)) in fields.iter().enumerate() {
                        b.spend(1, WORK)?;
                        path.push(ContractProjection::Place(PlaceProjection::Field(
                            u32::try_from(index)
                                .map_err(|_| fail("Static field ordinal overflow"))?,
                        )));
                        visit(child, path, out, b, depth + 1)?;
                        path.pop();
                    }
                }
                Type::BorrowedStorage { storage, .. } => visit(storage, path, out, b, depth + 1)?,
                _ => {}
            }
            Ok(())
        }
        let mut out = BTreeSet::new();
        visit(ty, &mut Vec::new(), &mut out, b, 0)?;
        Ok(out)
    }

    /// The input is a complete independently reconstructed original/creator
    /// value. An omitted variable slot selects a Static obligation, never a
    /// literal fact; each actual operand/return still proves that obligation.
    pub(super) fn original_static_byte_paths(
        &self,
        value: &Value,
        b: &mut Budget,
    ) -> CompileResult<BTreeSet<Vec<ContractProjection>>> {
        let mut out = BTreeSet::new();
        for path in self.physical_byte_paths(&value.ty, b)? {
            let mut variable = false;
            for slot in &value.life.slots {
                b.spend(slot.path.len().saturating_add(path.len() + 1), WORK)?;
                if slot.path.len() == path.len()
                    && slot
                        .path
                        .iter()
                        .zip(&path)
                        .all(|(selector, actual)| *actual == ContractProjection::Place(*selector))
                {
                    variable = true;
                }
            }
            if !variable {
                b.product(
                    out.len().saturating_add(1),
                    path.len().saturating_add(1),
                    1,
                    WORK,
                )?;
                out.insert(path);
            }
        }
        Ok(out)
    }

    pub(super) fn has_original_static_byte_path(
        &self,
        value: &Value,
        path: &[ContractProjection],
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let paths = self.original_static_byte_paths(value, b)?;
        b.product(
            paths.len().saturating_add(1),
            path.len().saturating_add(1),
            1,
            WORK,
        )?;
        Ok(paths.contains(path))
    }

    pub(super) fn check_static_value_sources(
        &self,
        value: &Value,
        function: &TypedFunction,
        actual: ValueId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        for path in self.original_static_byte_paths(value, b)? {
            crate::retained::check_dyn_static_return(self.module, function, actual, &path, b)?;
        }
        Ok(())
    }

    fn static_structural_child(
        &self,
        parent: &Value,
        field: u32,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        parent.empty_tuple_child(field, b)
    }

    pub(super) fn static_byte_returns(
        &self,
        target: &TypedFunction,
        ty: &Type,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !original::strings::is_byte_type(ty) || target.return_type != *ty {
            return Err(fail(
                "Static byte producer changes its complete actual returned type",
            ));
        }
        let position = StoragePosition {
            base: StorageBase::Result,
            path: Vec::new(),
        };
        let life = target
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("Static closure result has no complete actual header"))?;
        crate::signature_life::charge(life, b)?;
        if !life.static_bindings.contains(&position) {
            return Err(fail(
                "Static closure result loses its exact actual header occurrence",
            ));
        }
        let sources = target
            .retained_return_sources
            .as_ref()
            .ok_or_else(|| fail("Static closure result omits its actual source upper"))?;
        let mut found = false;
        for row in sources {
            crate::dyn_metadata::recipe(row, b)?;
            if row.position == position {
                if found
                    || !matches!(row.sources.as_slice(), [StorageSource::Static { leaf, mode: StoredBorrowMode::SharedAddress }]
                    if leaf == ty)
                {
                    return Err(fail(
                        "Static closure result changes its complete actual source upper",
                    ));
                }
                found = true;
            }
        }
        if !found {
            return Err(fail(
                "Static closure result lacks its exact actual source upper",
            ));
        }
        // This is a byte-view producer, not a callable factory. Its actual
        // reachable returns must derive from literal storage; a factory's
        // capture_sources contract cannot stand in for this result relation.
        let initialization = crate::shared_initialization(target, b)?;
        let mut returned = false;
        for (block, reachable) in target.blocks.iter().zip(&initialization.reachable) {
            b.spend(1, WORK)?;
            if !*reachable {
                continue;
            }
            match block.terminator {
                Terminator::Return(Some(value)) => {
                    crate::retained::check_dyn_static_return(self.module, target, value, &[], b)?;
                    returned = true;
                }
                Terminator::Return(None) => {
                    return Err(fail("Static byte producer omits its actual returned SSA"));
                }
                _ => {}
            }
        }
        if !returned {
            return Err(fail("Static byte producer has no grounded actual return"));
        }
        Ok(())
    }

    pub(super) fn original_static_header(
        &self,
        instance: &SemanticInstance,
        parameters: &[Value],
        result: &Value,
        captures: Option<ClosureId>,
        contract: &StorageLifeContract,
        b: &mut Budget,
    ) -> CompileResult<()> {
        // A class-free byte view is reconstructed here only from an
        // independently checked Static original type/body, never from N = 0
        // on the proposed executable header. Actual literal/return provenance
        // is checked separately by the instruction and retained-source flows.
        let mut static_positions = BTreeSet::new();
        for (base, value) in parameters
            .iter()
            .enumerate()
            .map(|(index, value)| (StorageBase::Parameter(index as u32), value))
            .chain(std::iter::once((StorageBase::Result, result)))
        {
            for path in self.original_static_byte_paths(value, b)? {
                b.lookup(static_positions.len(), WORK)?;
                static_positions.insert(StoragePosition { base, path });
            }
        }
        if let Some(id) = captures {
            let closure = self
                .module
                .closures
                .get(id.0 as usize)
                .filter(|closure| closure.id == id)
                .ok_or_else(|| fail("Static capture header has no actual closure"))?;
            // These complete values come from the original creator binding
            // and every real MakeClosure operand, including literal-source
            // proof. A factory's returned envelope is a different header.
            b.spend(closure.captures.len(), WORK)?;
            let mut physical_bytes = false;
            for ty in &closure.captures {
                physical_bytes |= !self.physical_byte_paths(ty, b)?.is_empty();
            }
            if physical_bytes {
                for binding in self
                    .capture_creator_bindings(instance, closure, b)?
                    .values()
                {
                    for path in self.original_static_byte_paths(&binding.stored, b)? {
                        let position = StoragePosition {
                            base: StorageBase::Capture(binding.capture),
                            path,
                        };
                        b.lookup(static_positions.len(), WORK)?;
                        if !static_positions.insert(position) {
                            return Err(fail(
                                "Static capture header repeats an original physical creator occurrence",
                            ));
                        }
                    }
                }
            }
        }
        if contract.static_bindings.len() != static_positions.len() {
            let mut errors = fail("actual header changes its complete original Static occurrences");
            errors[0].notes.push(format!(
                "original instance {:?}, proved Static occurrences {:?}; actual {:?}",
                instance.id, static_positions, contract.static_bindings
            ));
            return Err(errors);
        }
        for position in &contract.static_bindings {
            b.spend(position.path.len().saturating_add(1), WORK)?;
            b.lookup(static_positions.len(), WORK)?;
            if !static_positions.remove(position) {
                return Err(fail(
                    "actual header repeats or invents an original Static occurrence",
                ));
            }
        }
        Ok(())
    }

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
        self.original_static_header(
            instance,
            &parameters,
            &result,
            function.environment,
            contract,
            b,
        )?;
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
                StorageBinder::Fixed(index) => {
                    if index >= contract.fixed_binder_count {
                        false
                    } else if captures {
                        true
                    } else {
                        // A nongeneric invocation cannot invent a Fixed class.
                        // A complete lexical Type/life formal remains Fixed
                        // under its actual method/function specialization.
                        let class = self.header_class(instance, row.header, b)?;
                        let owner = match class {
                            Class::Formal(owner, _) | Class::TypeArgument(owner, _, _) => {
                                Some(owner)
                            }
                            _ => None,
                        };
                        let mut context = Some(instance);
                        let mut found = false;
                        for _ in 0..=b.partial_depth {
                            let Some(current) = context else {
                                break;
                            };
                            b.spend(1, WORK)?;
                            if Some(current.id) == owner {
                                found = true;
                                break;
                            }
                            context = match current.parent {
                                Some(parent) => Some(self.inventory.instance(parent, b)?),
                                None => None,
                            };
                        }
                        found
                    }
                }
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
                    let receiver_view = base == StorageBase::Result
                        && slot.path.is_empty()
                        && slot.mode == StoredBorrowMode::SharedAddress
                        && function
                            .environment
                            .and_then(|id| self.module.closures.get(id.0 as usize))
                            .is_some_and(crate::stored_callables::result_is_receiver)
                        && crate::signature_life::outer(contract, StorageBase::Receiver)
                            == Some(binder);
                    if !receiver_view
                        || self
                            .original
                            .receiver_sources(instance.definition, self.inventory, b)?
                            .is_none()
                    {
                        return Err(fail(
                            "actual outer header class differs from its original declaration domain",
                        ));
                    }
                    // Only this returned view is shortened. Its original
                    // occurrence and retained/inner classes are not equated.
                }
                used_bindings += 1;
            }
            if matches!(
                value.ty,
                Type::BorrowedStorage { .. }
                    | Type::Closure(_)
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. }
            ) {
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
            for (path, class) in self.task_ports(value, b)? {
                let expected = value
                    .classes
                    .get(class as usize)
                    .ok_or_else(|| fail("Task header port exceeds its complete class domain"))?;
                let mut selected = None;
                for row in &contract.bindings {
                    b.spend(row.position.path.len().saturating_add(path.len() + 1), WORK)?;
                    if row.position.base == base
                        && row.position.path == path
                        && selected.replace(row.binder).is_some()
                    {
                        return Err(fail(
                            "actual Task header repeats a retained port occurrence",
                        ));
                    }
                }
                let binder = selected
                    .ok_or_else(|| fail("actual Task header omits a retained port occurrence"))?;
                if self.header_class(instance, binder, b)? != *expected {
                    return Err(fail(
                        "actual Task port changes its independently checked original class",
                    ));
                }
                used_bindings += 1;
            }
            let qualified_sum = matches!(value.ty, Type::ErrorUnion { .. })
                && crate::stored_flow::enum_address::signature(&value.ty);
            if matches!(value.ty, Type::Address { .. } | Type::Desc(_)) || qualified_sum
                || self.dyn_sum_type(&value.ty, b)?
                || self.task_domain(&value.ty, b)?.is_some()
            {
                for child in self.signature_retained_classes(value, b)? {
                    if child.path.first() != Some(&LifeTypeProjection::Pointee)
                        && !(qualified_sum && matches!(child.path.first(),
                            Some(LifeTypeProjection::Place(PlaceProjection::ErrorArm(_)))))
                        && !(self.dyn_sum_type(&value.ty, b)?
                            && child.path.as_slice()
                                == [LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))])
                        && !(self.task_domain(&value.ty, b)?.is_some()
                            && child.path.first()
                                == Some(&LifeTypeProjection::Place(PlaceProjection::ErrorArm(
                                    false,
                                ))))
                    {
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
            if row.position.base == StorageBase::Receiver {
                let closure = function
                    .environment
                    .and_then(|id| self.module.closures.get(id.0 as usize))
                    .ok_or_else(|| fail("ordinary header invents a callable Receiver"))?;
                if !row.position.path.is_empty()
                    || !crate::stored_callables::receiver_access_head(closure)
                    || !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                    || !matches!(row.binder, StorageBinder::Invocation(_))
                {
                    return Err(fail(
                        "Receiver header has no exact reusable head/result relation",
                    ));
                }
                self.original
                    .receiver_sources(instance.definition, self.inventory, b)?
                    .ok_or_else(|| {
                        fail("Receiver header lacks its original captured return theorem")
                    })?;
                used_bindings += 1;
            }
            if matches!(row.position.base, StorageBase::Capture(_)) {
                if !captures {
                    return Err(fail("ordinary function invents a capture header"));
                }
                used_bindings += 1;
            }
        }
        for row in &contract.classes {
            if row.position.base == StorageBase::Receiver {
                return Err(fail(
                    "Receiver is an invocation access head, not a semantic class directory",
                ));
            }
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

    // A whole Managed<Task> has its child's full domain but is not another
    // signature class boundary. Keep its complete transport forest, and omit
    // only the wrapper occurrence whose exact child map is independently
    // present. The real Task occurrence, including phantom coordinates, stays.
    pub(super) fn signature_retained_classes<'v>(
        &self,
        value: &'v Value,
        b: &mut Budget,
    ) -> CompileResult<Vec<&'v StorageRetainedBinding>> {
        let mut selected = Vec::new();
        if value::qualified_reference_header(value, self.inventory, b)? {
            for row in &value.life.retained {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                if row.path.as_slice() == [LifeTypeProjection::Pointee] {
                    self.push_value(&mut selected, row, b)?;
                }
            }
            return Ok(selected);
        }
        for row in &value.life.retained {
            b.spend(row.path.len().saturating_add(1), WORK)?;
            let mut child = Some(&value.ty);
            let mut below_qualified = false;
            for step in &row.path {
                if matches!(child, Some(Type::BorrowedStorage { .. })) {
                    below_qualified = true;
                    break;
                }
                child = match (child, step) {
                    (
                        Some(
                            Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner),
                        ),
                        LifeTypeProjection::Pointee,
                    ) => Some(inner),
                    (
                        Some(Type::ErrorUnion { error, result }),
                        LifeTypeProjection::Place(PlaceProjection::ErrorArm(is_error)),
                    ) => Some(if *is_error { error } else { result }),
                    (
                        Some(Type::Tuple(fields)),
                        LifeTypeProjection::Place(PlaceProjection::Field(index)),
                    ) => fields.get(*index as usize),
                    (
                        Some(Type::Record { fields, .. } | Type::Package { fields, .. }),
                        LifeTypeProjection::Place(PlaceProjection::Field(index)),
                    ) => fields.get(*index as usize).map(|(_, ty)| ty),
                    (
                        Some(Type::Enum { variants, .. }),
                        LifeTypeProjection::Place(PlaceProjection::Variant(field)),
                    ) => variants
                        .get(field.variant as usize)
                        .and_then(|(_, fields)| fields.get(field.index as usize)),
                    (
                        Some(
                            Type::Array { element, .. }
                            | Type::Slice { element }
                            | Type::OwnedStore { element },
                        ),
                        LifeTypeProjection::Element,
                    ) => Some(element),
                    _ => None,
                };
            }
            if below_qualified { continue; }
            let Some(Type::Managed(payload)) = child else {
                self.push_value(&mut selected, row, b)?;
                continue;
            };
            let Type::Task { retained_life, .. } = payload.as_ref() else {
                self.push_value(&mut selected, row, b)?;
                continue;
            };
            if row.class >= retained_life.binder_count || row.binder >= value.life.binder_count {
                return Err(fail(
                    "Task wrapper transport leaves its complete class domain",
                ));
            }
            let mut actual = None;
            let mut wrapper = false;
            for candidate in &value.life.retained {
                b.spend(candidate.path.len().saturating_add(1), WORK)?;
                if candidate.class == row.class && candidate.path == row.path {
                    if wrapper {
                        return Err(fail("Task wrapper repeats its whole class transport"));
                    }
                    wrapper = true;
                }
                if candidate.class == row.class
                    && candidate.path.len() == row.path.len() + 1
                    && candidate.path[..row.path.len()] == row.path
                    && candidate.path.last() == Some(&LifeTypeProjection::Pointee)
                    && actual.replace(candidate.binder).is_some()
                {
                    return Err(fail(
                        "Task wrapper repeats its complete child class transport",
                    ));
                }
            }
            if actual != Some(row.binder) {
                return Err(fail(
                    "Task wrapper changes its exact child to whole class transport",
                ));
            }
        }
        Ok(selected)
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
        let expected = self.resolved(instance, expected, b, 0)?;
        if declaration {
            return Ok(expected);
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("semantic whole value has no real function context"));
        };
        let function = self.inventory.function(target, b)?;
        let actual = match operand {
            SemanticOperand::Value(value) => {
                let actual = self.ssa_value(instance, function, *value, b, 0)?;
                if matches!(expected.ty, Type::Closure(_))
                    && matches!(
                        self.defining(function, *value, b)?.2.kind,
                        InstructionKind::BorrowOwnedArrayElement { .. }
                    )
                {
                    self.pointee(&actual, b)?
                } else {
                    actual
                }
            }
            SemanticOperand::Local(local) => {
                let point = (at.0, at.1 + usize::from(at.2 == SemanticSiteEdge::After));
                if matches!(expected.ty, Type::Closure(_))
                    && let Some(slot) = self.persistent_slot(instance, function, *local, b)?
                {
                    self.persistent_slot_value(instance, function, &slot, point, b, 0)?
                } else {
                    self.local_value(instance, function, *local, point, b, 0)?
                }
            }
            SemanticOperand::Capture(index) => {
                self.capture_whole_value(instance, function, *index, b, 0)?
            }
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
            // A genuine reborrow can carry a fresh physical loan while this
            // receiving use retains the original parent region. The complete
            // inner value and actual SSA ancestry are checked by outer_role;
            // the original source expression's sigma is never rewritten.
            if self.outer_role(instance, function, &actual, &expected, b, 0)? {
                return Ok(expected);
            }
            return Err(fail(
                "actual whole type/class value differs from its original operand constraint",
            ));
        }
        Ok(actual)
    }
    pub(super) fn uses_full_value(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        if !self.physical_byte_paths(ty, b)?.is_empty() {
            return Ok(true);
        }
        match ty {
            Type::BorrowedStorage { life, .. } => Ok(life.binder_count != 0),
            Type::Address { .. } | Type::Desc(_) => Ok(true),
            Type::MutexGuard { .. } => Ok(true),
            Type::Managed(_) | Type::ErrorUnion { .. }
                if self
                    .task_domain(ty, b)?
                    .is_some_and(|life| life.binder_count != 0) =>
            {
                Ok(true)
            }
            Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                Ok(value::dyn_domain(ty, self.inventory, b)?.binder_count != 0)
            }
            Type::ErrorUnion { result, .. }
                if matches!(result.as_ref(), Type::Dyn { .. } | Type::OwnedDyn { .. }) =>
            {
                Ok(value::dyn_domain(result, self.inventory, b)?.binder_count != 0)
            }
            Type::ErrorUnion { .. } => self.contains_nonzero_child(ty, true, b, 0),
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
    ) -> CompileResult<BTreeMap<usize, Value>> {
        self.ghost_capture_directory(id, b)?;
        let definition = self
            .module
            .closures
            .get(id.0 as usize)
            .filter(|definition| definition.id == id)
            .ok_or_else(|| fail("ghost creator names another actual closure"))?;
        let child = self.closure_creator_world(definition.function, instance.id, id, b)?;
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
        let mut bindings = None;
        let mut stored_bounds = BTreeMap::new();
        for (index, operand) in captures.iter().enumerate() {
            let actual = self.ssa_value(instance, function, *operand, b, 0)?;
            if actual.ty != definition.captures[index] {
                return Err(fail(
                    "ghost creator has another complete actual capture type",
                ));
            }
            let capture = u32::try_from(index)
                .map_err(|_| fail("physical capture ordinal overflow"))?;
            if self.ghost_creator_capture(child, contract, capture, &actual, b)? {
                continue;
            }
            // Only the all-creators proof can connect this exact physical
            // operand to a different lexical upper at its stored header.
            if bindings.is_none() {
                bindings = Some(self.capture_creator_bindings(child, definition, b)?);
            }
            let mut proved = None;
            if let Some(bindings) = &bindings {
                for binding in bindings.values() {
                    b.spend(1, WORK)?;
                    if binding.capture != capture {
                        continue;
                    }
                    b.lookup(binding.bound_operands.len(), WORK)?;
                    if binding.bound_operands.contains(operand)
                        && proved.replace(&binding.stored).is_some()
                    {
                        return Err(fail("ghost creator capture has competing lexical bounds"));
                    }
                }
            }
            let Some(proved) = proved else {
                return Err(fail(
                    "ghost creator capture comes from another actual head or complete child class source",
                ));
            };
            if !self.ghost_creator_capture(child, contract, capture, proved, b)? {
                return Err(fail("ghost creator capture changes its proved stored classes"));
            }
            b.lookup(stored_bounds.len(), WORK)?;
            stored_bounds.insert(index, proved.copy(b)?);
        }
        Ok(stored_bounds)
    }

    fn ghost_creator_capture(
        &self,
        child: &SemanticInstance,
        contract: &StorageLifeContract,
        capture: u32,
        value: &Value,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        for (class, actual_class) in value.classes.iter().enumerate() {
            for (_, _, binder) in
                self.capture_class_headers(contract, capture, value, class as u32, b)?
            {
                let header = self.header_class(child, binder, b)?;
                if header != *actual_class
                    && header != self.capture_class_image(actual_class.clone(), b)?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

impl Check<'_, '_> {
    /// This is a coordinate interpretation of already checked original field
    /// recipes. Local atoms cannot escape as semantic identities or proofs.
    pub(super) fn environment_parameter_binders(
        &self,
        b: &mut Budget,
    ) -> CompileResult<BTreeMap<(FunctionId, u32, Vec<PlaceProjection>), StorageBinder>> {
        let mut completed = BTreeMap::new();
        for function in &self.module.functions {
            for (ordinal, local) in function.parameters.iter().enumerate() {
                let parameter = u32::try_from(ordinal)
                    .map_err(|_| fail("environment parameter ordinal overflows"))?;
                let leaves = crate::stored_flow::environment_parameter_leaves(
                    self.module,
                    &function.locals[local.0 as usize].ty,
                    b,
                )?;
                if leaves.is_empty() {
                    continue;
                }
                let header = function
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| fail("environment parameter omits its complete header"))?;
                let worlds = self.function_worlds(function.id, b)?;
                if worlds.is_empty() {
                    return Err(fail(
                        "environment parameter has no independently checked original world",
                    ));
                }
                for world in worlds {
                    let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                        &self.inventory.definition(world.definition, b)?.body
                    else {
                        return Err(fail(
                            "environment parameter has another original definition kind",
                        ));
                    };
                    let recipe = &signature
                        .parameters
                        .get(ordinal)
                        .ok_or_else(|| fail("environment parameter has no original full ordinal"))?
                        .ty;
                    let source = self.recipe_value(world, recipe, b, 0)?;
                    if !geometry::same_type(&source.ty, &function.locals[local.0 as usize].ty, b)? {
                        return Err(fail(
                            "environment parameter changes its original whole Type",
                        ));
                    }
                    let mut coordinates = BTreeMap::new();
                    let mut record = |whole: u32,
                                      binder: StorageBinder,
                                      b: &mut Budget|
                     -> CompileResult<()> {
                        if whole >= source.life.binder_count {
                            return Err(fail(
                                "environment coordinate leaves the complete original domain",
                            ));
                        }
                        b.lookup(coordinates.len(), WORK)?;
                        if let Some(previous) = coordinates.insert(whole, binder)
                            && previous != binder
                        {
                            return Err(fail(
                                "environment coordinate selects distinct actual header binders",
                            ));
                        }
                        if source.classes.get(whole as usize)
                            != Some(&self.header_class(world, binder, b)?)
                        {
                            return Err(fail(
                                "environment header changes its selected original coordinate image",
                            ));
                        }
                        Ok(())
                    };
                    for row in &header.classes {
                        b.spend(row.position.path.len().saturating_add(1), WORK)?;
                        if row.position.base != StorageBase::Parameter(parameter) {
                            continue;
                        }
                        let whole = if row.position.path.is_empty() {
                            row.position.class
                        } else {
                            let mut selected = None;
                            for part in &source.life.retained {
                                b.spend(
                                    part.path
                                        .len()
                                        .saturating_add(row.position.path.len())
                                        .saturating_add(1),
                                    WORK,
                                )?;
                                if part.path == row.position.path
                                    && part.class == row.position.class
                                    && selected.replace(part.binder).is_some()
                                {
                                    return Err(fail(
                                        "environment header repeats an original child coordinate",
                                    ));
                                }
                            }
                            selected.ok_or_else(|| {
                                fail("environment header omits its original retained coordinate")
                            })?
                        };
                        record(whole, row.binder, b)?;
                    }
                    for row in &header.bindings {
                        b.spend(row.position.path.len().saturating_add(1), WORK)?;
                        if row.position.base != StorageBase::Parameter(parameter) {
                            continue;
                        }
                        for part in &source.life.slots {
                            b.spend(
                                part.path
                                    .len()
                                    .saturating_add(row.position.path.len())
                                    .saturating_add(1),
                                WORK,
                            )?;
                            if row.position.path.len() == part.path.len()
                                && row
                                    .position
                                    .path
                                    .iter()
                                    .zip(&part.path)
                                    .all(|(a, z)| matches!(a,ContractProjection::Place(a) if a==z))
                            {
                                record(part.binder, row.binder, b)?;
                            }
                        }
                    }
                    drop(record);
                    let mut atoms = Vec::new();
                    for class in 0..source.life.binder_count {
                        self.push_value(&mut atoms, Class::Local(class), b)?;
                    }
                    let coordinate_value = source.with_classes(atoms, b)?;
                    for (prefix, id, leaf) in &leaves {
                        b.spend(prefix.len().saturating_add(leaf.len()), WORK)?;
                        let mut path = prefix.clone();
                        path.extend(leaf);
                        let root = if matches!(coordinate_value.ty, Type::Address { .. }) {
                            self.pointee(&coordinate_value, b)?
                        } else {
                            coordinate_value.copy(b)?
                        };
                        let whole = self.environment_leaf_coordinate(&root, &path, b, 0)?;
                        b.lookup(coordinates.len(), WORK)?;
                        let binder = *coordinates.get(&whole).ok_or_else(|| {
                            fail("environment field omits its selected original header coordinate")
                        })?;
                        b.spend(
                            prefix.len().saturating_add(leaf.len()).saturating_add(1),
                            WORK,
                        )?;
                        let _ = id; // actual ClosureId already selected by the original path walk
                        let key = (function.id, parameter, path);
                        b.lookup(completed.len(), WORK)?;
                        if let Some(previous) = completed.insert(key, binder)
                            && previous != binder
                        {
                            return Err(fail(
                                "environment binder differs across complete original worlds",
                            ));
                        }
                    }
                }
            }
        }
        Ok(completed)
    }
}

impl Check<'_, '_> {
    fn environment_capture_coordinates(
        &self,
        parent: &Value,
        capture: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        if depth > b.partial_depth {
            return Err(unknown("environment coordinate path exceeds finite depth"));
        }
        let Type::Closure(id) = parent.ty else {
            return Err(fail("environment capture has no actual Closure identity"));
        };
        let definition = &self.module.closures[id.0 as usize];
        let header = definition
            .storage_life
            .as_ref()
            .ok_or_else(|| fail("environment capture has no complete Fixed header"))?;
        let function = &self.module.functions[definition.function.0 as usize];
        let worlds = self.function_worlds(function.id, b)?;
        if worlds.is_empty() {
            return Err(fail("environment capture has no original completed world"));
        }
        let mut selected: Option<Value> = None;
        for world in worlds {
            let source = self.capture_whole_value(world, function, capture, b, 0)?;
            let mut classes = Vec::new();
            for class in 0..source.life.binder_count {
                let rows = self.capture_class_headers(header, capture, &source, class, b)?;
                let mut coordinate = None;
                for (_, _, binder) in rows {
                    let StorageBinder::Fixed(index) = binder else {
                        return Err(fail(
                            "environment capture coordinate is not persistent Fixed",
                        ));
                    };
                    if coordinate
                        .replace(index)
                        .is_some_and(|previous| previous != index)
                    {
                        return Err(fail(
                            "environment capture repeats one coordinate with distinct Fixed axes",
                        ));
                    }
                }
                let index = coordinate.ok_or_else(|| {
                    fail("environment capture omits a complete intrinsic or phantom coordinate")
                })?;
                self.push_value(
                    &mut classes,
                    parent
                        .classes
                        .get(index as usize)
                        .ok_or_else(|| {
                            fail("environment capture leaves its actual complete Fixed domain")
                        })?
                        .clone(),
                    b,
                )?;
            }
            let value = source.with_classes(classes, b)?;
            if let Some(previous) = &selected {
                if !previous.equal_paid(&value, b)? {
                    return Err(fail(
                        "environment coordinate projection differs across original capture worlds",
                    ));
                }
            } else {
                selected = Some(value);
            }
        }
        selected.ok_or_else(|| fail("environment capture coordinate has no source"))
    }
    fn environment_leaf_coordinate(
        &self,
        parent: &Value,
        path: &[PlaceProjection],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<u32> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "environment leaf coordinate exceeds finite path depth",
            ));
        }
        if path.is_empty() {
            let [head] = parent.life.slots.as_slice() else {
                return Err(fail(
                    "environment physical leaf omits its exact outer occurrence",
                ));
            };
            if !head.path.is_empty() {
                return Err(fail(
                    "environment physical leaf has a projected outer occurrence",
                ));
            }
            let Some(Class::Local(coordinate)) = parent.classes.get(head.binder as usize) else {
                return Err(fail(
                    "environment leaf has no original directory coordinate",
                ));
            };
            return Ok(*coordinate);
        }
        if matches!(parent.ty, Type::Address { .. }) {
            let child = self.pointee(parent, b)?;
            return self.environment_leaf_coordinate(&child, path, b, depth + 1);
        }
        let (step, tail) = path.split_first().unwrap();
        let child = if matches!(parent.ty, Type::Closure(_)) {
            let PlaceProjection::Field(capture) = step else {
                return Err(fail(
                    "environment capture selector has another physical kind",
                ));
            };
            self.environment_capture_coordinates(parent, *capture, b, depth + 1)?
        } else {
            self.closure_child_value(parent, *step, b)?
        };
        self.environment_leaf_coordinate(&child, tail, b, depth + 1)
    }
}
