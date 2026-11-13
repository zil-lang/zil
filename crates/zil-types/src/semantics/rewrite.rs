//! Resolve only embedded Type payloads. Lexical owners, original full formal
//! ordinals, class expressions, recipes and selector paths remain unchanged.
use crate::*;

pub(crate) trait Context {
    fn node(&mut self, depth: usize) -> R<()>;
    fn payload(&mut self, ty: &Type, depth: usize) -> R<Type>;
}

pub(crate) fn module(value: &mut SemanticModule, cx: &mut impl Context) -> R<()> {
    cx.node(0)?;
    for definition in &mut value.definitions {
        cx.node(1)?;
        for formal in &mut definition.parameters {
            formal_types(formal, cx, 2)?;
        }
        for requirement in &mut definition.requirements {
            requirement_types(requirement, cx, 2)?;
        }
        for environment in &mut definition.environment {
            cx.node(2)?;
            formal_types(&mut environment.kind, cx, 3)?;
            actual_recipe(&mut environment.source, cx, 3)?;
        }
        match &mut definition.body {
            SemanticDefinitionBody::Function { signature, sites } => {
                signature_types(signature, cx, 2)?;
                for site in sites {
                    cx.node(2)?;
                    match &mut site.kind {
                        SemanticSiteRecipeKind::CallableCall { callee: result }
                        | SemanticSiteRecipeKind::Construct { result }
                        | SemanticSiteRecipeKind::TypeUse { expected: result }
                        | SemanticSiteRecipeKind::DeclareLocal { expected: result } => {
                            ty(result, cx, 3)?
                        }
                        SemanticSiteRecipeKind::Call { actuals, .. } => {
                            actual_recipes(actuals, cx, 3)?
                        }
                        SemanticSiteRecipeKind::Closure { environment, .. } => {
                            actual_recipes(environment, cx, 3)?
                        }
                    }
                }
            }
            SemanticDefinitionBody::Nominal { body, .. } => match body {
                SemanticNominalBody::Product { fields } => {
                    for field in fields {
                        cx.node(2)?;
                        ty(&mut field.ty, cx, 3)?;
                    }
                }
                SemanticNominalBody::Enum { variants } => {
                    for variant in variants {
                        cx.node(2)?;
                        for field in &mut variant.fields {
                            ty(field, cx, 3)?;
                        }
                    }
                }
            },
            SemanticDefinitionBody::Scope(scope) => match scope {
                SemanticScopeRecipe::Trait { .. } => {}
                SemanticScopeRecipe::InherentImpl { self_type, .. }
                | SemanticScopeRecipe::BuiltinImpl { self_type, .. } => ty(self_type, cx, 2)?,
                SemanticScopeRecipe::TraitImpl {
                    self_type,
                    trait_application,
                    ..
                } => {
                    ty(self_type, cx, 2)?;
                    trait_types(trait_application, cx, 2)?;
                }
            },
        }
    }
    for instance in &mut value.instances {
        cx.node(1)?;
        actuals(&mut instance.actuals, cx, 2)?;
        if let Some(environment) = &mut instance.environment {
            cx.node(2)?;
            actuals(&mut environment.actuals, cx, 3)?;
        }
        if let Some(value) = &mut instance.self_type {
            type_value(value, cx, 2)?;
        }
    }
    for site in &mut value.sites {
        cx.node(1)?;
        match &mut site.action {
            SemanticSiteAction::CallableCall { .. } => {}
            SemanticSiteAction::Construct { value, .. }
            | SemanticSiteAction::TypeUse {
                expected: value, ..
            }
            | SemanticSiteAction::DeclareLocal {
                expected: value, ..
            } => type_value(value, cx, 2)?,
            SemanticSiteAction::Call {
                actuals: values, ..
            }
            | SemanticSiteAction::Closure {
                environment: values,
                ..
            } => actuals(values, cx, 2)?,
        }
    }
    Ok(())
}
fn type_value(value: &mut SemanticTypeValue, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    match value {
        SemanticTypeValue::Resolved { ty, .. } => *ty = cx.payload(ty, depth + 1)?,
        SemanticTypeValue::Core { source, .. } => match source {
            SemanticCoreTypeSource::OriginalRecipe(recipe)
            | SemanticCoreTypeSource::Expression { recipe, .. } => ty(recipe, cx, depth + 1)?,
        },
    }
    Ok(())
}

fn actuals(values: &mut [SemanticActual], cx: &mut impl Context, depth: usize) -> R<()> {
    for value in values {
        cx.node(depth)?;
        match value {
            SemanticActual::Type(value) | SemanticActual::Const { ty: value, .. } => {
                type_value(value, cx, depth + 1)?
            }
            SemanticActual::CoreRecipe { recipe, .. } => actual_recipe(recipe, cx, depth + 1)?,
            SemanticActual::Life(_) => {}
        }
    }
    Ok(())
}
fn actual_recipes(
    values: &mut [SemanticActualRecipe],
    cx: &mut impl Context,
    depth: usize,
) -> R<()> {
    for value in values {
        actual_recipe(value, cx, depth)?;
    }
    Ok(())
}
fn actual_recipe(value: &mut SemanticActualRecipe, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    match value {
        SemanticActualRecipe::Type(value) => ty(value, cx, depth + 1)?,
        SemanticActualRecipe::Const { carrier, value } => {
            ty(carrier, cx, depth + 1)?;
            constant(value, cx, depth + 1)?;
        }
        SemanticActualRecipe::Life(_) => {}
    }
    Ok(())
}
fn formal_types(value: &mut SemanticFormal, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    match value {
        SemanticFormal::Type { requirements } => {
            for value in requirements {
                requirement_types(value, cx, depth + 1)?;
            }
        }
        SemanticFormal::Const { carrier } => ty(carrier, cx, depth + 1)?,
        SemanticFormal::Lifetime => {}
    }
    Ok(())
}
fn requirement_types(
    value: &mut SemanticRequirementRecipe,
    cx: &mut impl Context,
    depth: usize,
) -> R<()> {
    cx.node(depth)?;
    match value {
        SemanticRequirementRecipe::Structural { ty: value, .. }
        | SemanticRequirementRecipe::Builtin { ty: value, .. } => ty(value, cx, depth + 1)?,
        SemanticRequirementRecipe::Trait {
            ty: value,
            application,
        } => {
            ty(value, cx, depth + 1)?;
            trait_types(application, cx, depth + 1)?;
        }
        SemanticRequirementRecipe::Callable {
            ty: value,
            signature,
            ..
        } => {
            ty(value, cx, depth + 1)?;
            signature_types(signature, cx, depth + 1)?;
        }
        SemanticRequirementRecipe::TypeEqual { left, right } => {
            ty(left, cx, depth + 1)?;
            ty(right, cx, depth + 1)?;
        }
        SemanticRequirementRecipe::ConstEqual {
            carrier,
            left,
            right,
        } => {
            ty(carrier, cx, depth + 1)?;
            constant(left, cx, depth + 1)?;
            constant(right, cx, depth + 1)?;
        }
        SemanticRequirementRecipe::ForAll { requirements, .. } => {
            for value in requirements {
                requirement_types(value, cx, depth + 1)?;
            }
        }
        SemanticRequirementRecipe::LifeEqual { .. }
        | SemanticRequirementRecipe::Outlives { .. } => {}
    }
    Ok(())
}
fn signature_types(
    value: &mut SemanticSignatureRecipe,
    cx: &mut impl Context,
    depth: usize,
) -> R<()> {
    cx.node(depth)?;
    for parameter in &mut value.parameters {
        ty(&mut parameter.ty, cx, depth + 1)?;
    }
    ty(&mut value.result, cx, depth + 1)
}
fn trait_types(value: &mut SemanticTraitRecipe, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    actual_recipes(&mut value.arguments, cx, depth + 1)
}
fn class_sequence(
    value: &mut SemanticClassSequenceRecipe,
    cx: &mut impl Context,
    depth: usize,
) -> R<()> {
    cx.node(depth)?;
    for part in &mut value.parts {
        cx.node(depth + 1)?;
        if let SemanticClassSequencePart::AllClasses { ty: value, .. } = part {
            ty(value, cx, depth + 2)?;
        }
    }
    Ok(())
}
fn retained(values: &mut [SemanticRetainedPart], cx: &mut impl Context, depth: usize) -> R<()> {
    for value in values {
        cx.node(depth)?;
        if let SemanticRetainedPart::All { parents, .. } = value {
            class_sequence(parents, cx, depth + 1)?;
        }
    }
    Ok(())
}
fn nominal(values: &mut [SemanticNominalPart], cx: &mut impl Context, depth: usize) -> R<()> {
    for value in values {
        cx.node(depth)?;
        if let SemanticNominalPart::FromType {
            ty: value, classes, ..
        } = value
        {
            ty(value, cx, depth + 1)?;
            class_sequence(classes, cx, depth + 1)?;
        }
    }
    Ok(())
}
fn storage(value: &mut SemanticStorageShapeRecipe, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    class_sequence(&mut value.domain, cx, depth + 1)?;
    retained(&mut value.retained, cx, depth + 1)?;
    nominal(&mut value.nominal, cx, depth + 1)
}
fn retained_shape(
    value: &mut SemanticRetainedShapeRecipe,
    cx: &mut impl Context,
    depth: usize,
) -> R<()> {
    cx.node(depth)?;
    class_sequence(&mut value.domain, cx, depth + 1)?;
    for port in &mut value.ports {
        cx.node(depth + 1)?;
        match port {
            SemanticPortPart::One { leaf, .. } => ty(leaf, cx, depth + 2)?,
            SemanticPortPart::FromType { ty: value, classes } => {
                ty(value, cx, depth + 2)?;
                class_sequence(classes, cx, depth + 2)?;
            }
        }
    }
    retained(&mut value.retained, cx, depth + 1)?;
    nominal(&mut value.nominal, cx, depth + 1)
}
fn dyn_interface(
    value: &mut SemanticDynInterfaceRecipe,
    cx: &mut impl Context,
    depth: usize,
) -> R<()> {
    cx.node(depth)?;
    trait_types(&mut value.application, cx, depth + 1)?;
    if let SemanticDynContextRecipe::Type(value) = &mut value.context {
        ty(value, cx, depth + 1)?;
    }
    for binding in &mut value.associated {
        cx.node(depth + 1)?;
        trait_types(&mut binding.application, cx, depth + 2)?;
        match &mut binding.value {
            SemanticAssociatedRecipe::Type(value) => ty(value, cx, depth + 2)?,
            SemanticAssociatedRecipe::Const { carrier, value } => {
                ty(carrier, cx, depth + 2)?;
                constant(value, cx, depth + 2)?;
            }
        }
    }
    Ok(())
}
pub(crate) fn ty(value: &mut SemanticTypeRecipe, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    match value {
        SemanticTypeRecipe::Atom(value) => *value = cx.payload(value, depth + 1)?,
        SemanticTypeRecipe::Exact(value) => value.ty = cx.payload(&value.ty, depth + 1)?,
        SemanticTypeRecipe::Nominal { arguments, .. } => actual_recipes(arguments, cx, depth + 1)?,
        SemanticTypeRecipe::AssociatedType {
            receiver,
            application,
            ..
        } => {
            ty(receiver, cx, depth + 1)?;
            trait_types(application, cx, depth + 1)?;
        }
        SemanticTypeRecipe::Tuple(values) | SemanticTypeRecipe::AnonymousSum(values) => {
            for value in values {
                ty(value, cx, depth + 1)?;
            }
        }
        SemanticTypeRecipe::Array { element, length } => {
            ty(element, cx, depth + 1)?;
            constant(length, cx, depth + 1)?;
        }
        SemanticTypeRecipe::Slice(value)
        | SemanticTypeRecipe::Managed(value)
        | SemanticTypeRecipe::OwnedStore(value)
        | SemanticTypeRecipe::Mutex(value)
        | SemanticTypeRecipe::Registration(value)
        | SemanticTypeRecipe::AsyncRegistration(value)
        | SemanticTypeRecipe::Address { inner: value, .. }
        | SemanticTypeRecipe::Desc { inner: value, .. } => ty(value, cx, depth + 1)?,
        SemanticTypeRecipe::MutexGuard { payload, retained } => {
            ty(payload, cx, depth + 1)?;
            retained_shape(retained, cx, depth + 1)?;
        }
        SemanticTypeRecipe::Task { result, retained } => {
            ty(result, cx, depth + 1)?;
            retained_shape(retained, cx, depth + 1)?;
        }
        SemanticTypeRecipe::ErrorUnion { error, result } => {
            ty(error, cx, depth + 1)?;
            ty(result, cx, depth + 1)?;
        }
        SemanticTypeRecipe::Qualified {
            storage: value,
            life,
        } => {
            ty(value, cx, depth + 1)?;
            storage(life, cx, depth + 1)?;
        }
        SemanticTypeRecipe::Code { signature, .. } => signature_types(signature, cx, depth + 1)?,
        SemanticTypeRecipe::Closure {
            arguments,
            environment,
            ..
        } => {
            actual_recipes(arguments, cx, depth + 1)?;
            actual_recipes(environment, cx, depth + 1)?;
        }
        SemanticTypeRecipe::Dyn { interface, .. } => dyn_interface(interface, cx, depth + 1)?,
        SemanticTypeRecipe::Opaque {
            arguments, hidden, ..
        } => {
            actual_recipes(arguments, cx, depth + 1)?;
            ty(hidden, cx, depth + 1)?;
        }
        SemanticTypeRecipe::Formal(_)
        | SemanticTypeRecipe::Environment { .. }
        | SemanticTypeRecipe::SelfType { .. }
        | SemanticTypeRecipe::UnresolvedNamed(_)
        | SemanticTypeRecipe::Unsupported(_) => {}
    }
    Ok(())
}
fn constant(value: &mut SemanticConstRecipe, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    match value {
        SemanticConstRecipe::Unary {
            operand_type,
            value,
            ..
        } => {
            ty(operand_type, cx, depth + 1)?;
            constant(value, cx, depth + 1)?;
        }
        SemanticConstRecipe::Binary {
            operand_type,
            left,
            right,
            ..
        } => {
            ty(operand_type, cx, depth + 1)?;
            constant(left, cx, depth + 1)?;
            constant(right, cx, depth + 1)?;
        }
        SemanticConstRecipe::Convert {
            source_type,
            target_type,
            value,
        } => {
            ty(source_type, cx, depth + 1)?;
            ty(target_type, cx, depth + 1)?;
            constant(value, cx, depth + 1)?;
        }
        SemanticConstRecipe::AssociatedConst {
            receiver,
            application,
            ..
        } => {
            ty(receiver, cx, depth + 1)?;
            trait_types(application, cx, depth + 1)?;
        }
        SemanticConstRecipe::Value(_)
        | SemanticConstRecipe::IntegerLiteral(_)
        | SemanticConstRecipe::FloatLiteral(_)
        | SemanticConstRecipe::Formal(_)
        | SemanticConstRecipe::Environment { .. } => {}
    }
    Ok(())
}
