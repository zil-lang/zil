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
                if ty.is_b0_scalar() || matches!(ty, Type::ForeignHandle | Type::ByteBuffer) =>
            {
                Ok(value::scalar(ty.clone()))
            }
            SemanticTypeRecipe::Exact(exact)
                if (exact.ty == crate::generic_mutex::error_type()
                    || crate::dyn_dispatch::is_create_error(&exact.ty))
                    && empty(&exact.life)
                    && exact.classes.is_empty() =>
            {
                crate::dyn_metadata::ty(&exact.ty, b)?;
                Ok(value::scalar(exact.ty.clone()))
            }
            SemanticTypeRecipe::Slice(element) => {
                let element = self.recipe_value(instance, element, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "original Slice needs its complete element occurrence embedding",
                    ));
                }
                Ok(value::scalar(Type::Slice {
                    element: Box::new(element.into_type(b)?),
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
                if !child.classes.is_empty() {
                    return Err(unknown(
                        "semantic resource constructor needs complete child retained and phantom class transport",
                    ));
                }
                let ty = match recipe {
                    SemanticTypeRecipe::Managed(_) => Type::Managed(Box::new(child.into_type(b)?)),
                    SemanticTypeRecipe::OwnedStore(_) => Type::OwnedStore {
                        element: Box::new(child.into_type(b)?),
                    },
                    SemanticTypeRecipe::Mutex(_) => Type::Mutex {
                        payload: Box::new(child.into_type(b)?),
                    },
                    SemanticTypeRecipe::Registration(_) => Type::Registration {
                        environment: Box::new(child.into_type(b)?),
                    },
                    SemanticTypeRecipe::AsyncRegistration(_) => Type::AsyncRegistration {
                        environment: Box::new(child.into_type(b)?),
                    },
                    _ => {
                        return Err(fail(
                            "semantic resource constructor changed its original family",
                        ));
                    }
                };
                Ok(value::scalar(ty))
            }
            SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let child = self.recipe_value(instance, inner, b, depth + 1)?;
                let class = self.life(instance, life, b, depth + 1)?;
                let mut result = value::address(false, class, child, b)?;
                let geometry = result.geometry_mut(b)?;
                let Type::Address { inner, .. } = std::mem::replace(&mut geometry.ty, Type::Unit) else {
                    return Err(fail("semantic description loses its exact payload"));
                };
                geometry.ty = Type::Desc(inner);
                geometry
                    .life
                    .slots
                    .get_mut(0)
                    .ok_or_else(|| fail("semantic description loses its weak outer occurrence"))?
                    .mode = StoredBorrowMode::WritableDescription;
                Ok(result)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let error = self.recipe_value(instance, error, b, depth + 1)?;
                let result = self.recipe_value(instance, result, b, depth + 1)?;
                value::error_union(error, result, b)
            }
            SemanticTypeRecipe::Code { kind, signature } => {
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
                Ok(value::scalar(Type::Array {
                    element: Box::new(element.into_type(b)?),
                    length,
                }))
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
                    self.push_value(&mut types, field.into_type(b)?, b)?;
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
        let source_life = match ty { Type::BorrowedStorage { life, .. } => Some(life), _ => None };
        let geometry = self.inventory.geometry.type_value(ty, source_life, b)?;
        let index =
            crate::stored_flow::nominal_instance_index(self.module, &geometry.ty, self.inventory.limits, b)?;
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
                        _ => match ty { Type::BorrowedStorage { life, .. } => Some(life), _ => None },
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
        let definition = self.module.closures.get(id.0 as usize)
            .filter(|definition| definition.id == id)
            .ok_or_else(|| fail("capture transport has no actual closure definition"))?;
        b.spend(definition.captures.len(), WORK)?;
        if definition.captures.iter().any(|ty| !ty.is_b0_scalar()
            && !matches!(ty, Type::Address { mutable: false, inner } if inner.is_b0_scalar()))
        { return Ok(true); }
        // A moved reference has the complete class of the original lexical
        // value. It is not an extra implicit Address borrowing a scalar slot.
        // Metadata only selects the stronger all-creators proof; it cannot
        // establish either mode, the source class or a captured permission.
        for world in self.function_worlds(definition.function, b)? {
            b.spend(world.header_embedding.len(), WORK)?;
            if world.header_embedding.iter().any(|row| matches!(row.header, StorageBinder::Fixed(_))
                && !matches!(row.actual, SemanticClassExpr::Implicit { scope: SemanticScopeRef::Ancestor(0), .. }))
            { return Ok(true); }
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
            || contract.bindings.iter().any(|row| {
                !matches!(
                    row.position.base,
                    StorageBase::Parameter(_) | StorageBase::Result | StorageBase::Capture(_)
                )
            })
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
        let mut used_slots = BTreeSet::new();
        let creator_bindings = self.capture_creator_bindings(instance, closure, b)?;
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
            for row in &value.life.retained {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                if row.binder == class {
                    self.push_value(&mut children, (row.path.as_slice(), row.class), b)?;
                }
            }
        } else {
            self.push_value(&mut children, (&[][..], class), b)?;
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
        directory: &mut [Option<u32>],
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
        crate::dyn_metadata::ty(ty, b)?;
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
                return Ok(value::scalar(ty.clone()));
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
        ) && !self.dyn_sum_type(ty, b)? {
            // A complete independent zero-domain reconstruction needs no
            // predecessor SSA class equation. Actual initialization, source,
            // loans and cleanup remain obligations of each operation/kernel.
            // This is not an empty-shape shortcut: zero_data walks the full
            // nominal/opaque inventory and rejects nonzero or Unknown scopes.
            self.zero_data(ty, b, 0)?;
            return Ok(value::scalar(ty.clone()));
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
        match &instruction.kind {
            InstructionKind::CallDynSlot { .. } => self.trait_result(instance, function, block, index, b)?
                .ok_or_else(|| fail("dynamic result has no exact original member call receipt")),
            InstructionKind::MakeDynOwned { .. } => self.dyn_owned_result(instance, function, block, index, b),
            InstructionKind::BorrowDynOwned { local, mutable } => self.dyn_owned_borrow(
                instance, function, id, *local, *mutable, (block, index), b, depth + 1),
            InstructionKind::StaticUtf8 { bytes } => {
                b.spend(bytes.len().saturating_add(1), WORK)?;
                if !original::strings::is_byte_type(ty) {
                    return Err(fail(
                        "actual UTF8 literal changes its complete immutable byte-view type",
                    ));
                }
                Ok(value::scalar(ty.clone()))
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
                let receiver = self.ssa_value(instance, function, *callee, b, depth + 1)?;
                self.static_closure_result(instance, block, index, &receiver.ty, ty, b)
            }
            InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. } => {
                let receiver = self.local_value(instance, function, *callee,
                    (block, index), b, depth + 1)?;
                self.static_closure_result(instance, block, index, &receiver.ty, ty, b)
            }
            InstructionKind::CallClosureBorrowed { callee, .. } => {
                if let Some(result) = self.abstract_operation(instance, function, block, index, b)? {
                    return Ok(result);
                }
                let receiver = self.ssa_value(instance, function, *callee, b, depth + 1)?;
                let Type::Address { inner, .. } = &receiver.ty else {
                    return Err(fail("borrowed known callable result has no actual environment address"));
                };
                if !matches!(inner.as_ref(), Type::Closure(_)) {
                    return Err(fail("borrowed known callable result changes its actual closure identity"));
                }
                // BodyCall separately proves the exact original lexical place,
                // requested mode, fresh borrow and whole pointee class value.
                self.static_closure_result(instance, block, index, inner, ty, b)
            },
            InstructionKind::LoadStoredBorrow { storage, path, mode } => {
                operations::available_before(function, (block, index, SemanticSiteEdge::Before), *storage, b)?;
                let root = self.ssa_value(instance, function, *storage, b, depth + 1)?;
                let mut selected = if matches!(root.ty, Type::Address { .. }) {
                    self.pointee(&root, b)?
                } else { return Err(fail("stored reference read has no actual addressed aggregate source")); };
                for projection in path {
                    let PlaceProjection::Field(field) = projection else {
                        return Err(unknown("stored reference read needs its complete active Sum/array child source"));
                    };
                    selected = self.actual_nominal_field(&selected, *field, b, depth + 1)?;
                }
                let valid = match mode {
                    StoredBorrowLoad::CopyShared => matches!(selected.ty, Type::Address { mutable: false, .. }),
                    StoredBorrowLoad::CopyDescription => matches!(selected.ty, Type::Desc(_)),
                    StoredBorrowLoad::Take => matches!(selected.ty, Type::Address { .. } | Type::Desc(_)),
                    StoredBorrowLoad::Reborrow { .. } => false,
                };
                if !valid || !geometry::same_type(&selected.ty, ty, b)? {
                    return Err(unknown("stored reference read changes its full original type or requires its exact reborrow role"));
                }
                // Full actual stored-flow retains current cells, parent loans,
                // shared/exclusive mode and consuming state independently.
                Ok(selected)
            }
            InstructionKind::ReadVariant { value: parent, variant, index: field } => {
                self.projected_shared_value(instance, function, id, *parent,
                    PlaceProjection::Variant(zil_ir::VariantProjection { variant: *variant, index: *field }),
                    (block, index), b, depth + 1)
            }
            InstructionKind::ReadErrorUnion { value: parent, is_error }
            | InstructionKind::TakeErrorUnion { value: parent, is_error } => {
                if self.dyn_sum_type(function.values.get(parent.0 as usize)
                    .ok_or_else(|| fail("actual sum projection has no parent type"))?, b)? {
                    return self.dyn_owned_projection(instance, function, id, *parent, *is_error,
                        (block, index), b, depth + 1);
                }
                self.projected_shared_value(instance, function, id, *parent,
                    PlaceProjection::ErrorArm(*is_error), (block, index), b, depth + 1)
            }
            InstructionKind::ExtractField { aggregate, index: field } => {
                self.projected_shared_value(instance, function, id, *aggregate,
                    PlaceProjection::Field(*field), (block, index), b, depth + 1)
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
            InstructionKind::ReadCapture { index: capture } | InstructionKind::TakeCapture { index: capture } => {
                self.capture_whole_value(instance, function, *capture, b, depth + 1)
            }
            InstructionKind::MakeVariant { variant, fields } => {
                let mut selected = None;
                for site in &self.program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != instance.id
                        || !matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After }
                            if actual == block && instruction as usize == index)
                    { continue; }
                    let Some(constructor) = self.original.enum_constructor(instance, site.recipe,
                        self.program, self.inventory, b)? else { continue; };
                    if constructor.variant != *variant || constructor.payload.len() != fields.len()
                        || !geometry::same_type(&constructor.result.ty, ty, b)?
                    { return Err(fail("actual Sum construction changes its complete original variant or result domain")); }
                    for (operand, expected) in fields.iter().zip(&constructor.payload) {
                        operations::available_before(function, (block, index, SemanticSiteEdge::Before), *operand, b)?;
                        if !geometry::same_type(function.values.get(operand.0 as usize)
                            .ok_or_else(|| fail("actual Sum payload is absent"))?, &expected.ty, b)?
                            || !self.ssa_value(instance, function, *operand, b, depth + 1)?.equal_paid(expected, b)?
                        { return Err(fail("actual Sum payload changes its independently checked complete original class value")); }
                    }
                    if selected.as_ref().is_some_and(|old| old != &constructor.result) {
                        return Err(fail("actual Sum constructor has competing original domain occurrences"));
                    }
                    selected = Some(constructor.result);
                }
                // Inactive alternatives still retain the complete original
                // phantom/type-argument domain. This transports no payload
                // address, active tag, owner or permission for an empty arm.
                selected.ok_or_else(|| fail("class-bearing Sum constructor lacks its exact original occurrence"))
            }
            InstructionKind::MakeAggregate { fields } => {
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
                    return Err(fail("actual aggregate changes its complete original physical type"));
                }
                let storage = match &expected.ty {
                    Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                    ty => ty,
                };
                let (count, nominal) = match storage {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => (fields.len(), true),
                    Type::Tuple(fields) => (fields.len(), false),
                    Type::Array { length, .. } => (*length as usize, false),
                    _ => return Err(fail("actual aggregate has another complete constructor layout")),
                };
                if count != fields.len() {
                    return Err(fail("actual aggregate omits an original physical field"));
                }
                // The original whole value retains every mixed/phantom class.
                // Each physical child is proved separately; field equality is
                // never inferred from a shared image or the parent's head.
                for (ordinal, operand) in fields.iter().enumerate() {
                    operations::available_before(function,
                        (block, index, SemanticSiteEdge::Before), *operand, b)?;
                    let field = if nominal {
                        self.actual_nominal_field(&expected, u32::try_from(ordinal)
                            .map_err(|_| fail("actual aggregate field ordinal overflow"))?, b, depth + 1)?
                    } else {
                        let field = match storage {
                            Type::Tuple(fields) => &fields[ordinal],
                            Type::Array { element, .. } => element.as_ref(),
                            _ => unreachable!(),
                        };
                        // Class-bearing structural tuples/arrays require their
                        // own original embedding, rather than invented rows.
                        self.zero_data(field, b, depth + 1)?;
                        crate::dyn_metadata::ty(field, b)?;
                        value::scalar(field.clone())
                    };
                    let actual = self.ssa_value(instance, function, *operand, b, depth + 1)?;
                    if !actual.equal_paid(&field, b)?
                        && !self.outer_role(instance, function, &actual, &field, b, depth + 1)?
                    {
                        return Err(fail("actual aggregate child changes its complete original data or directed outer role"));
                    }
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
                    let child = self.local_value(instance, function, local, (block, index), b, depth + 1)?;
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
                if self.has_ghost_capture(*id, b)? {
                    self.ghost_creator(instance, *id, captures, b)?;
                } else {
                    self.creator_heads(instance, *id, captures, b)?;
                }
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
                    let source = self.ssa_value(instance, function, *source, b, depth + 1)?;
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
                Ok(Value::new(ty.clone(), StorageLifeShape {
                        binder_count: contract.fixed_binder_count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    }, complete))
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
                let entry = self.inventory.function(&SemanticFunctionTarget::Typed(*callee), b)?;
                if entry.call_kind != (if suspend { CallKind::Suspend } else { CallKind::Direct })
                    || entry.foreign.is_some()
                    || (suspend && function.call_kind != CallKind::Suspend)
                {
                    return Err(fail("actual call result changes its original execution convention"));
                }
                if let Some(value) = self.trait_result(instance, function, block, index, b)? {
                    return Ok(value);
                }
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
                let original_output = if matches!(ty, Type::Address { .. } | Type::Desc(_)
                    | Type::Closure(_) | Type::Dyn { .. } | Type::OwnedDyn { .. }) || self.dyn_sum_type(ty, b)? {
                    let output = self.static_call_result(instance, block, index, b)?;
                    if !geometry::same_type(&output.ty, ty, b)? { return Err(fail("call output changes its complete original head Type")); }
                    Some(output)
                } else { None };
                let count = match ty {
                    Type::Address { .. } | Type::Desc(_) | Type::Dyn { .. } | Type::OwnedDyn { .. }
                    | Type::ErrorUnion { .. } => u32::try_from(original_output.as_ref().ok_or_else(||
                        fail("call result omits its complete original output"))?.classes.len())
                        .map_err(|_| fail("complete call output domain exceeds its ordinal carrier"))?,
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
                    if let Some(output) = &original_output
                        && (matches!(ty, Type::Address { .. } | Type::Desc(_)) || self.dyn_sum_type(ty, b)?) {
                        for slot in &output.life.slots {
                            if slot.binder != child { continue; }
                            for row in &contract.bindings {
                                b.spend(row.position.path.len().saturating_add(slot.path.len() + 1), WORK)?;
                                if row.position.base == StorageBase::Result
                                    && row.position.path.len() == slot.path.len()
                                    && row.position.path.iter().zip(&slot.path).all(|(actual, expected)| matches!(actual, ContractProjection::Place(step) if step == expected))
                                {
                                    if result.is_some_and(|old| old != row.binder) { return Err(fail("call outer result has competing complete header binders")); }
                                    result = Some(row.binder);
                                }
                            }
                        }
                        for retained in &output.life.retained {
                            if retained.binder != child { continue; }
                            for row in &contract.classes {
                                b.spend(row.position.path.len().saturating_add(retained.path.len() + 1), WORK)?;
                                if row.position.base == StorageBase::Result && row.position.path == retained.path && row.position.class == retained.class {
                                    if result.is_some_and(|old| old != row.binder) { return Err(fail("call child result has competing complete header binders")); }
                                    result = Some(row.binder);
                                }
                            }
                        }
                    } else {
                        for row in &contract.classes {
                            b.spend(row.position.path.len() + 1, WORK)?;
                            if row.position.base == StorageBase::Result && row.position.path.is_empty()
                                && row.position.class == child && result.replace(row.binder).is_some()
                            { return Err(fail("callee result repeats an intrinsic class occurrence")); }
                        }
                    }
                    let binder = result
                        .ok_or_else(|| fail("callee result omits an intrinsic class occurrence"))?;
                    let input_binders = self.call_result_input_binders(callee, binder, b)?;
                    let mut source = None;
                    for row in &contract.bindings {
                        b.spend(row.position.path.len().saturating_add(input_binders.len() + 1), WORK)?;
                        if !input_binders.contains(&row.binder) {
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
                        if !matches!(expected.ty, Type::Address { .. } | Type::Desc(_))
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
                            b.spend(row.position.path.len().saturating_add(input_binders.len() + 1), WORK)?;
                            if !input_binders.contains(&row.binder) {
                                continue;
                            }
                            let StorageBase::Parameter(parameter) = row.position.base else {
                                continue;
                            };
                            let actual = *arguments.get(parameter as usize).ok_or_else(|| {
                                fail("callee class input omits an actual argument")
                            })?;
                            let mut input = self.ssa_value(instance, function, actual, b, depth + 1)?;
                            let expected = expected_parameters
                                .get(parameter as usize)
                                .ok_or_else(|| fail("callee class role omits an original input"))?;
                            let child;
                            let expected = if row.position.path.is_empty() {
                                expected
                            } else if row.position.path.as_slice() == [LifeTypeProjection::Pointee] {
                                if !self.outer_role(instance, function, &input, expected, b, depth + 1)? {
                                    return Err(fail("callee child class input changes its independently proved outer call role"));
                                }
                                input = self.pointee(&input, b)?;
                                child = self.pointee(expected, b)?;
                                &child
                            } else {
                                return Err(unknown("call result child requires its full projected input relation"));
                            };
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
                    if source.is_none() && let Some(output) = &original_output {
                        source = self.reference_call_result_class(instance, function, (block, index),
                            callee, child, output, b, depth + 1)?;
                    }
                    let class=source.ok_or_else(||unknown("callee result class has no independently proved input/static/role source"))?;
                    self.push_value(&mut classes, class, b)?;
                }
                if let Some(output) = original_output {
                    b.spend(output.classes.len().saturating_add(classes.len()), WORK)?;
                    if output.classes != classes { return Err(fail("call result header changes its independently proved original outer/child role")); }
                    return output.with_classes(classes, b);
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
                // static_call_values checked every actual target world against
                // this exact original call receipt. The result classes above
                // come only from the caller's original complete input roles;
                // no callee-world representative or class coincidence is used.
                Ok(Value::new(ty.clone(), shape, classes))
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
        if let Some(value) = self.reference_local_value(instance, function, local, at, b, depth + 1)? {
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
                crate::dyn_metadata::ty(ty, b)?;
                return Ok(value::scalar(ty.clone()));
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
        {
            // A zero-class fact does not depend on which loop iteration last
            // initialized the occupant. Actual initialization and resources
            // are still checked at every consuming operation.
            self.zero_data(ty, b, 0)?;
            crate::dyn_metadata::ty(ty, b)?;
            return Ok(value::scalar(ty.clone()));
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
        let mut selected = None;
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
                                pending.push((source, source_block, source_index));
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
                        return Err(fail("semantic local class crosses a vacant activation"));
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
                crate::dyn_metadata::ty(&value.ty, b)?;
                crate::dyn_metadata::shape(&value.life, b)?;
                b.spend(value.classes.len().saturating_add(1), WORK)?;
                if selected.as_ref().is_some_and(|old| old != &value) {
                    return Err(unknown(
                        "semantic local needs its complete joined role/source dependency relation",
                    ));
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
    fn static_closure_result(&self, instance: &SemanticInstance,
        block: BlockId, index: usize, receiver: &Type, ty: &Type,
        b: &mut Budget) -> CompileResult<Value> {
        let Type::Closure(id) = receiver else {
            return Err(fail("concrete closure result has another actual receiver type"));
        };
        let closure = self.module.closures.get(id.0 as usize).filter(|row| row.id == *id)
            .ok_or_else(|| fail("concrete closure result has no actual closure definition"))?;
        let output = self.static_call_result(instance, block, index, b)?;
        if !original::strings::is_byte_type(&output.ty) || !empty(&output.life)
            || !output.classes.is_empty() || !geometry::same_type(&output.ty, ty, b)?
            || !geometry::same_type(&closure.result, ty, b)?
        {
            return Err(unknown("concrete closure result needs its complete non-Static class boundary"));
        }
        let target = self.inventory.function(&SemanticFunctionTarget::Typed(closure.function), b)?;
        let position = StoragePosition { base: StorageBase::Result, path: Vec::new() };
        let life = target.storage_life.as_ref()
            .ok_or_else(|| fail("Static closure result has no complete actual header"))?;
        crate::signature_life::charge(life, b)?;
        if !life.static_bindings.contains(&position) {
            return Err(fail("Static closure result loses its exact actual header occurrence"));
        }
        let sources = target.retained_return_sources.as_ref()
            .ok_or_else(|| fail("Static closure result omits its actual source upper"))?;
        let mut found = false;
        for row in sources {
            crate::dyn_metadata::recipe(row, b)?;
            if row.position == position {
                if found || !matches!(row.sources.as_slice(), [StorageSource::Static { leaf, mode: StoredBorrowMode::SharedAddress }]
                    if leaf == ty) {
                    return Err(fail("Static closure result changes its complete actual source upper"));
                }
                found = true;
            }
        }
        if !found { return Err(fail("Static closure result lacks its exact actual source upper")); }
        // The exact BodyCall consumer checks every target world, input and ABI;
        // the retained flow checks each real return path against this source.
        crate::retained::check_sources(self.module, target, b)?;
        Ok(output)
    }

    pub(super) fn original_static_header(&self, parameters: &[Value], result: &Value,
        contract: &StorageLifeContract, b: &mut Budget) -> CompileResult<()> {
        // A class-free byte view is reconstructed here only from an
        // independently checked Static original type/body, never from N = 0
        // on the proposed executable header. Actual literal/return provenance
        // is checked separately by the instruction and retained-source flows.
        let mut static_positions = BTreeSet::new();
        for (base, value) in parameters.iter().enumerate()
            .map(|(index, value)| (StorageBase::Parameter(index as u32), value))
            .chain(std::iter::once((StorageBase::Result, result)))
        {
            if original::strings::is_byte_type(&value.ty) && empty(&value.life)
                && value.classes.is_empty()
            {
                b.lookup(static_positions.len(), WORK)?;
                static_positions.insert(StoragePosition { base, path: Vec::new() });
            }
        }
        if contract.static_bindings.len() != static_positions.len() {
            return Err(fail("actual header changes its complete original Static occurrences"));
        }
        for position in &contract.static_bindings {
            b.spend(position.path.len().saturating_add(1), WORK)?;
            b.lookup(static_positions.len(), WORK)?;
            if !static_positions.remove(position) {
                return Err(fail("actual header repeats or invents an original Static occurrence"));
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
        self.original_static_header(&parameters, &result, contract, b)?;
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
                    if index >= contract.fixed_binder_count { false }
                    else if captures { true }
                    else {
                        // A nongeneric invocation cannot invent a Fixed class.
                        // A complete lexical Type/life formal remains Fixed
                        // under its actual method/function specialization.
                        let class = self.header_class(instance, row.header, b)?;
                        let owner = match class {
                            Class::Formal(owner, _) | Class::TypeArgument(owner, _, _) => Some(owner),
                            _ => None,
                        };
                        let mut context = Some(instance);
                        let mut found = false;
                        for _ in 0..=b.partial_depth {
                            let Some(current) = context else { break; };
                            b.spend(1, WORK)?;
                            if Some(current.id) == owner { found = true; break; }
                            context = match current.parent { Some(parent) => Some(self.inventory.instance(parent, b)?), None => None };
                        }
                        found
                    }
                },
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
            if matches!(value.ty, Type::BorrowedStorage { .. } | Type::Closure(_)
                | Type::Dyn { .. } | Type::OwnedDyn { .. }) {
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
            if matches!(value.ty, Type::Address { .. } | Type::Desc(_)) || self.dyn_sum_type(&value.ty, b)? {
                for child in &value.life.retained {
                    if child.path.as_slice() != [LifeTypeProjection::Pointee]
                        && !(self.dyn_sum_type(&value.ty, b)? && child.path.as_slice() ==
                            [LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))]) {
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
        let expected = self.resolved(instance, expected, b, 0)?;
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
            return Err(fail(
                "actual whole type/class value differs from its original operand constraint",
            ));
        }
        Ok(actual)
    }
    pub(super) fn uses_full_value(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        match ty {
            Type::BorrowedStorage { life, .. } => Ok(life.binder_count != 0),
            Type::Address { .. } | Type::Desc(_) => Ok(true),
            Type::MutexGuard { .. } => Ok(true),
            Type::Dyn { .. } | Type::OwnedDyn { .. } => Ok(value::dyn_domain(ty, self.inventory, b)?.binder_count != 0),
            Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Dyn { .. } | Type::OwnedDyn { .. }) =>
                Ok(value::dyn_domain(result, self.inventory, b)?.binder_count != 0),
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
            for (class, actual_class) in actual.classes.iter().enumerate() {
                for (_, _, binder) in self.capture_class_headers(contract, index as u32, &actual, class as u32, b)? {
                if self.header_class(child, binder, b)? != *actual_class {
                    return Err(fail("ghost creator capture comes from another actual head or complete child class source"));
                }
                }
            }
        }
        Ok(())
    }
}
