//! Actual method boundaries consume the independently checked original provider
//! theorem. Concrete receiver images never prove generic conformance.
use super::*;
#[path = "world_invocation.rs"]
mod invocation;
fn recipe_type(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    recipe: &SemanticTypeRecipe,
) -> R<SemanticTypeValue> {
    crate::dyn_metadata::semantic_operand_snapshot(recipe, world.limits, world.left, 1)?;
    match super::super::materialize::actual_at(
        world.program,
        world.module,
        world.ledger,
        scope,
        &SemanticActualRecipe::Type(recipe.clone()),
        world.limits,
        world.left,
    )? {
        SemanticActual::Type(value) => Ok(value),
        _ => Err(bad("method original type materializes another actual kind")),
    }
}
fn call_result(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    site: &SemanticSite,
    recipe: &SemanticTypeRecipe,
    hint: &Type,
) -> R<SemanticTypeValue> {
    if matches!(recipe, SemanticTypeRecipe::Opaque { .. }) {
        super::super::materialize::call_result_at(
            world.program,
            world.module,
            world.ledger,
            instance.id,
            site.recipe,
            hint,
            world.limits,
            world.left,
        )
    } else {
        world.ty(instance.id, recipe, hint)
    }
}
fn parent_class(
    world: &mut World<'_, '_>,
    image: &SemanticClassExpr,
    actuals: &[SemanticActual],
) -> R<SemanticClassExpr> {
    crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    Ok(match image {
        SemanticClassExpr::Formal {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } => {
            let Some(SemanticActual::Life(actual)) = actuals.get(*ordinal as usize) else {
                return Err(bad("method parent substitution omits a Life actual"));
            };
            crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
            actual.clone()
        }
        SemanticClassExpr::IntrinsicClass {
            operand:
                SemanticOperand::TypeArgument {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                },
            path,
            class,
        } if path.is_empty() => {
            let Some(SemanticActual::Type(SemanticTypeValue::Resolved { classes, .. })) =
                actuals.get(*ordinal as usize)
            else {
                return Err(bad(
                    "method parent substitution omits a complete Type actual",
                ));
            };
            let actual = classes
                .get(*class as usize)
                .ok_or_else(|| bad("method parent substitution truncates whole-Type N"))?;
            crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
            actual.clone()
        }
        _ => {
            return Err(pending(
                "method parent class needs its complete lexical boundary substitution",
            ));
        }
    })
}
fn parent_value(
    world: &mut World<'_, '_>,
    value: &SemanticTypeValue,
    actuals: &[SemanticActual],
) -> R<SemanticTypeValue> {
    let mut result = world.copy(value)?;
    let SemanticTypeValue::Resolved { classes, .. } = &mut result else {
        return Err(bad("method parent retains an unresolved whole type"));
    };
    for image in classes {
        *image = parent_class(world, image, actuals)?;
    }
    Ok(result)
}
fn parent_actual(
    world: &mut World<'_, '_>,
    actual: &SemanticActual,
    actuals: &[SemanticActual],
) -> R<SemanticActual> {
    Ok(match actual {
        SemanticActual::Type(value) => SemanticActual::Type(parent_value(world, value, actuals)?),
        SemanticActual::Const { ty, value } => {
            crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
            SemanticActual::Const {
                ty: parent_value(world, ty, actuals)?,
                value: value.clone(),
            }
        }
        SemanticActual::Life(image) => SemanticActual::Life(parent_class(world, image, actuals)?),
        _ => return Err(bad("method parent retains a Core actual")),
    })
}
fn same_actual(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    left: &SemanticActual,
    right: &SemanticActual,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    match (left, right) {
        (SemanticActual::Type(a), SemanticActual::Type(b)) => world.same(scope, a, b, implicit),
        (SemanticActual::Life(a), SemanticActual::Life(b)) => {
            world.same_class(scope, a, b, implicit)
        }
        (
            SemanticActual::Const { ty: a, value: av },
            SemanticActual::Const { ty: b, value: bv },
        ) => {
            world.same(scope, a, b, implicit)?;
            crate::dyn_metadata::semantic_operand_snapshot(left, world.limits, world.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(right, world.limits, world.left, 1)?;
            if av != bv {
                return Err(bad("method parent changes a complete Const actual"));
            }
            Ok(())
        }
        _ => Err(bad("method parent changes an original mixed actual kind")),
    }
}
fn actual_specialization(
    world: &mut World<'_, '_>,
    actual: &SemanticActual,
    expected: &SemanticActual,
) -> R<()> {
    crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(expected, world.limits, world.left, 1)?;
    match (actual, expected) {
        (
            SemanticActual::Type(SemanticTypeValue::Resolved {
                ty: a,
                life: al,
                classes: ac,
            }),
            SemanticActual::Type(SemanticTypeValue::Resolved {
                ty: b,
                life: bl,
                classes: bc,
            }),
        ) if a == b && al == bl && ac.len() == bc.len() => Ok(()),
        (SemanticActual::Const { .. }, SemanticActual::Const { .. }) if actual == expected => {
            Ok(())
        }
        (SemanticActual::Life(_), SemanticActual::Life(_)) => Ok(()),
        _ => Err(bad(
            "method actual specialization changes its complete original type, carrier or kind",
        )),
    }
}
pub(super) fn evaluate(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    callee: &SemanticInstance,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    arguments: &[ValueId],
    values: &[Option<SemanticTypeValue>],
    locals: &[Option<SemanticTypeValue>],
    implicit: &mut BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<SemanticTypeValue>> {
    let mut selected = None;
    for site in world.point_sites(sites, block, index)? {
        let SemanticSiteAction::BodyCall { actuals } = &site.action else {
            continue;
        };
        let (method, signature, recipes, parent_recipes) = {
            let ledger = world.ledger;
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("method call lacks complete original Gamma"))?;
            let Some(method) = proof.method_calls.get(&(original.id, site.recipe)) else {
                continue;
            };
            if selected.is_some() {
                return Err(bad("actual method call repeats its original scheme"));
            }
            let signature = proof
                .provider_signature(
                    original.id,
                    site.recipe,
                    callee.definition,
                    world.limits,
                    world.left,
                )?
                .ok_or_else(|| {
                    bad("actual method provider has no quantified original conformance proof")
                })?;
            let recipes = proof
                .calls
                .get(&(original.id, site.recipe))
                .ok_or_else(|| bad("method call omits independently solved method actuals"))?;
            let target = world.definition(callee.definition)?;
            let parent = target
                .parent
                .ok_or_else(|| bad("actual method provider has no original lexical scope"))?;
            let parent_definition = world.definition(parent)?;
            let parent_recipes = match proof.scope_calls.get(&(original.id, site.recipe, parent)) {
                Some(recipes) => recipes.as_slice(),
                None if parent_definition.parameters.is_empty() => &[],
                None => {
                    return Err(pending(
                        "generic method provider parent has no independently solved original caller substitution",
                    ));
                }
            };
            crate::dyn_metadata::semantic_operand_snapshot(
                &method.self_type,
                world.limits,
                world.left,
                1,
            )?;
            if let Some(receiver) = &method.receiver_borrow {
                crate::dyn_metadata::semantic_operand_snapshot(
                    receiver,
                    world.limits,
                    world.left,
                    1,
                )?;
            }
            if let Some(place) = &method.receiver_place {
                crate::dyn_metadata::semantic_operand_snapshot(
                    &place.root,
                    world.limits,
                    world.left,
                    1,
                )?;
                world.pay(place.fields.len().saturating_add(1))?;
            }
            if let Some(application) = &method.application {
                crate::dyn_metadata::semantic_operand_snapshot(
                    application,
                    world.limits,
                    world.left,
                    1,
                )?;
            }
            world.pay(recipes.len().saturating_add(parent_recipes.len()))?;
            for recipe in recipes.iter().chain(parent_recipes) {
                crate::dyn_metadata::semantic_operand_snapshot(
                    recipe,
                    world.limits,
                    world.left,
                    1,
                )?;
            }
            (
                method.clone(),
                signature,
                recipes.clone(),
                parent_recipes.to_vec(),
            )
        };
        if !matches!(
            world.original_recipe(original, site.recipe)?.kind,
            SemanticSiteRecipeKind::OriginalCall
        ) {
            return Err(bad("method call changes its original operation kind"));
        }
        if recipes.len() != actuals.len() || actuals.len() != callee.actuals.len() {
            return Err(bad("method call omits original mixed method actuals"));
        }
        for ((recipe, actual), specialized) in recipes.iter().zip(actuals).zip(&callee.actuals) {
            let expected = super::super::materialize::actual_at(
                world.program,
                world.module,
                world.ledger,
                instance.id,
                recipe,
                world.limits,
                world.left,
            )?;
            same_actual(world, instance.id, actual, &expected, implicit)?;
            actual_specialization(world, specialized, &expected)?;
        }
        let target = world.definition(callee.definition)?;
        let parent_id = target
            .parent
            .ok_or_else(|| bad("method provider has no original scope"))?;
        let parent = world.definition(parent_id)?;
        let parent_instance = world.instance(
            callee
                .parent
                .ok_or_else(|| bad("method provider has no actual parent scope instance"))?,
        )?;
        if parent_instance.definition != parent_id
            || !matches!(parent_instance.target, SemanticInstanceTarget::Scope)
        {
            return Err(bad(
                "method provider actual parent differs from its real original scope",
            ));
        }
        if parent_recipes.len() != parent.parameters.len()
            || parent_instance.actuals.len() != parent_recipes.len()
        {
            return Err(bad("method parent omits its complete mixed actuals"));
        }
        let mut parent_actuals = Vec::new();
        world.pay(parent_recipes.len())?;
        for (recipe, actual) in parent_recipes.iter().zip(&parent_instance.actuals) {
            let expected = super::super::materialize::actual_at(
                world.program,
                world.module,
                world.ledger,
                instance.id,
                recipe,
                world.limits,
                world.left,
            )?;
            actual_specialization(world, actual, &expected)?;
            parent_actuals.push(expected);
        }
        let (actual_self, application, trait_scope) = match &parent.body {
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                self_type,
                trait_application,
                ..
            }) => (
                recipe_type(world, parent_instance.id, self_type)?,
                Some(trait_application),
                None,
            ),
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::InherentImpl {
                self_type, ..
            }) => (
                recipe_type(world, parent_instance.id, self_type)?,
                None,
                None,
            ),
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) => {
                let value = parent_instance.self_type.as_ref().ok_or_else(|| {
                    bad("default method actual trait scope omits its complete Self")
                })?;
                (world.copy(value)?, None, Some(*declaration))
            }
            _ => {
                return Err(bad(
                    "actual ordinary method provider has another lexical scope kind",
                ));
            }
        };
        if method.application.is_none()
            && (method.declaration != callee.definition || method.parent != parent_id)
        {
            return Err(bad("direct method call selects another original provider"));
        }
        let actual_self = parent_value(world, &actual_self, &parent_actuals)?;
        let expected_self = recipe_type(world, instance.id, &method.self_type)?;
        world.same(instance.id, &actual_self, &expected_self, implicit)?;
        match (&method.application, application, trait_scope) {
            (Some(expected), Some(actual), None)
                if expected.declaration == actual.declaration
                    && expected.arguments.len() == actual.arguments.len() =>
            {
                for (expected, actual) in expected.arguments.iter().zip(&actual.arguments) {
                    let expected = super::super::materialize::actual_at(
                        world.program,
                        world.module,
                        world.ledger,
                        instance.id,
                        expected,
                        world.limits,
                        world.left,
                    )?;
                    let actual = super::super::materialize::actual_at(
                        world.program,
                        world.module,
                        world.ledger,
                        parent_instance.id,
                        actual,
                        world.limits,
                        world.left,
                    )?;
                    let actual = parent_actual(world, &actual, &parent_actuals)?;
                    same_actual(world, instance.id, &actual, &expected, implicit)?;
                }
            }
            (Some(expected), None, Some(actual))
                if expected.declaration == actual
                    && expected.arguments.len() == parent_actuals.len() =>
            {
                for (expected, actual) in expected.arguments.iter().zip(&parent_actuals) {
                    let expected = super::super::materialize::actual_at(
                        world.program,
                        world.module,
                        world.ledger,
                        instance.id,
                        expected,
                        world.limits,
                        world.left,
                    )?;
                    same_actual(world, instance.id, actual, &expected, implicit)?;
                }
            }
            (None, _, _) => {}
            _ => {
                return Err(bad(
                    "method provider changes the complete original trait application",
                ));
            }
        }
        if arguments.len() != signature.parameters.len()
            || function.parameters.len() != arguments.len()
            || function.call_kind != signature.call_kind
            || function.result_mode != signature.result_mode
        {
            return Err(bad(
                "method actual entry changes its full original signature roles",
            ));
        }
        super::receiver::seal(
            world, instance, original, site, &method, arguments, values, locals, implicit,
        )?;
        for (ordinal, ((argument, parameter), local)) in arguments
            .iter()
            .zip(&signature.parameters)
            .zip(&function.parameters)
            .enumerate()
        {
            let ty = &function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("method entry parameter local is absent"))?
                .ty;
            let expected = world.ty(instance.id, &parameter.ty, ty)?;
            let actual = world.get(values, *argument)?;
            arguments::satisfy(
                world,
                instance,
                Some(callee),
                site,
                ordinal,
                *argument,
                &actual,
                &expected,
                locals,
                implicit,
            )?;
        }
        selected = Some(call_result(
            world,
            instance,
            site,
            &signature.result,
            &function.return_type,
        )?);
    }
    Ok(selected)
}

/// A declaration-local anonymous input is universally instantiated only at
/// this real call boundary. Its Fixed identity inside the callee is unchanged;
/// captured Fixed and every whole-Type intrinsic class use separate mappings.
fn incoming_source(
    world: &mut World<'_, '_>,
    child: &SemanticInstance,
    binder: StorageBinder,
) -> R<Option<SemanticClassExpr>> {
    let source = world.binding(child, binder)?;
    if matches!(binder, StorageBinder::Invocation(_)) {
        return Ok(Some(source));
    }
    let SemanticClassExpr::Implicit {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    } = &source
    else {
        return Ok(None);
    };
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = child.target else {
        return Err(bad("call input class has no actual Typed entry"));
    };
    let entry = world.function(id)?;
    let definition = world.definition(child.definition)?;
    if entry.environment.is_some()
        || !matches!(
            definition.kind,
            SemanticDefinitionKind::Function
                | SemanticDefinitionKind::Method
                | SemanticDefinitionKind::TaskEntry
        )
    {
        return Ok(None);
    }
    world.pay(definition.implicit_lifetimes.len())?;
    if definition
        .implicit_lifetimes
        .iter()
        .filter(|row| row.ordinal == *ordinal)
        .count()
        != 1
    {
        return Err(bad(
            "call input anonymous class has no unique original declaration occurrence",
        ));
    }
    Ok(Some(source))
}
fn invocation_inputs(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    child: &SemanticInstance,
    header: &StorageLifeContract,
    parameters: &[SemanticTypeValue],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<invocation::Inputs> {
    let mut mapping = invocation::Inputs::default();
    world.pay(header.bindings.len().saturating_add(header.classes.len()))?;
    for row in &header.bindings {
        let StorageBase::Parameter(index) = row.position.base else {
            continue;
        };
        let Some(source) = incoming_source(world, child, row.binder)? else {
            continue;
        };
        let SemanticTypeValue::Resolved { life, classes, .. } = parameters
            .get(index as usize)
            .ok_or_else(|| bad("invocation header selects an absent argument role"))?
        else {
            return Err(bad("invocation argument retains an unresolved type"));
        };
        world.pay(
            life.slots
                .len()
                .saturating_mul(row.position.path.len().saturating_add(1)),
        )?;
        let mut slots = life.slots.iter().filter(|slot| {
            slot.path.len() == row.position.path.len()
                && slot
                    .path
                    .iter()
                    .zip(&row.position.path)
                    .all(|(a, b)| *b == ContractProjection::Place(*a))
        });
        let slot = if let Some(slot) = slots.next() {
            slot
        } else {
            if invocation::static_input(
                world,
                &mut mapping,
                &source,
                &parameters[index as usize],
                &row.position,
            )? {
                continue;
            }
            crate::dyn_metadata::semantic_operand_snapshot(
                &parameters[index as usize],
                world.limits,
                world.left,
                1,
            )?;
            world.pay(256)?;
            let mut error =
                pending("invocation outer role requires its complete actual argument occurrence");
            error.notes.push(format!("caller {:?}, callee {:?} definition {:?}, parameter {}, header position {:?}, source {:?}; argument slots {:?}, N={}",instance.id,child.id,child.definition,index,row.position,source,life.slots.iter().take(4).collect::<Vec<_>>(),classes.len()));
            return Err(error);
        };
        if slots.next().is_some() {
            return Err(bad("invocation argument repeats an outer occurrence"));
        }
        let actual = classes
            .get(slot.binder as usize)
            .ok_or_else(|| bad("invocation argument class exceeds its complete domain"))?;
        if mapping.statics.contains(&source) {
            return Err(bad("one anonymous input changes its static lifetime image"));
        }
        insert_invocation(
            world,
            instance,
            &mut mapping.classes,
            source,
            actual,
            implicit,
        )?;
    }
    for row in &header.classes {
        let StorageBase::Parameter(index) = row.position.base else {
            continue;
        };
        let Some(source) = incoming_source(world, child, row.binder)? else {
            continue;
        };
        let SemanticTypeValue::Resolved { ty, life, classes } = parameters
            .get(index as usize)
            .ok_or_else(|| bad("invocation class selects an absent argument role"))?
        else {
            return Err(bad("invocation argument retains an unresolved type"));
        };
        let actual = if row.position.path.is_empty()
            && matches!(
                ty,
                Type::BorrowedStorage { .. }
                    | Type::Closure(_)
                    | Type::MutexGuard { .. }
                    | Type::Task { .. }
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. }
            ) {
            classes
                .get(row.position.class as usize)
                .ok_or_else(|| bad("invocation truncates a whole argument class domain"))?
        } else {
            world.pay(
                life.retained
                    .len()
                    .saturating_mul(row.position.path.len().saturating_add(1)),
            )?;
            let mut paths = life
                .retained
                .iter()
                .filter(|part| part.path == row.position.path && part.class == row.position.class);
            let part = paths
                .next()
                .ok_or_else(|| bad("invocation omits a complete nested argument class"))?;
            if paths.next().is_some() {
                return Err(bad("invocation repeats a complete nested argument class"));
            }
            classes
                .get(part.binder as usize)
                .ok_or_else(|| bad("invocation nested class exceeds complete argument N"))?
        };
        if mapping.statics.contains(&source) {
            return Err(bad("static outer role is also an intrinsic class input"));
        }
        insert_invocation(
            world,
            instance,
            &mut mapping.classes,
            source,
            actual,
            implicit,
        )?;
    }
    Ok(mapping)
}
fn insert_invocation(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    mapping: &mut Vec<(SemanticClassExpr, SemanticClassExpr)>,
    source: SemanticClassExpr,
    actual: &SemanticClassExpr,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    world.pay(mapping.len())?;
    if let Some((_, old)) = mapping.iter().find(|(old, _)| old == &source) {
        return world.same_class(instance.id, old, actual, implicit);
    }
    crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
    mapping.push((source, actual.clone()));
    Ok(())
}
pub(super) fn callable(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    site: &SemanticSite,
    raw_value: &SemanticTypeValue,
    closure_value: &SemanticTypeValue,
    definition: &ClosureDefinition,
    child: &SemanticInstance,
    function: &TypedFunction,
    requested: CallableKind,
    actuals: &[SemanticActual],
    arguments: &[ValueId],
    values: &[Option<SemanticTypeValue>],
    locals: &[Option<SemanticTypeValue>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<SemanticTypeValue>> {
    let scheme = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("callable call lacks complete original Gamma"))?;
        let Some(scheme) = proof.callable_calls.get(&(original.id, site.recipe)) else {
            return Ok(None);
        };
        if !proof.complete.contains(&original.id) || !proof.complete.contains(&child.definition) {
            return Err(bad(
                "callable boundary has no complete original caller and actual entry proof",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(
            &scheme.callee,
            world.limits,
            world.left,
            1,
        )?;
        crate::dyn_metadata::semantic_operand_snapshot(
            &scheme.signature,
            world.limits,
            world.left,
            1,
        )?;
        scheme.clone()
    };
    if !actuals.is_empty() {
        return Err(bad("abstract callable call fabricates declaration actuals"));
    }
    let compatible = |actual, required| {
        matches!(
            (actual, required),
            (
                CallableKind::Fn,
                CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
            ) | (
                CallableKind::FnMut,
                CallableKind::FnMut | CallableKind::FnOnce
            ) | (CallableKind::FnOnce, CallableKind::FnOnce)
        )
    };
    if !compatible(scheme.kind, requested) || !compatible(definition.kind, requested) {
        return Err(bad(
            "actual callable opcode or entry lacks the original calling capability",
        ));
    }
    if matches!(
        World::type_of(raw_value)?,
        Type::Address { mutable: false, .. }
    ) && requested != CallableKind::Fn
        || matches!(World::type_of(raw_value)?, Type::Address { .. })
            && requested == CallableKind::FnOnce
    {
        return Err(bad(
            "actual borrowed callable changes its receiver permission or consumption",
        ));
    }
    let callee_query = SemanticTypeRecipe::OriginalType {
        scope: SemanticScopeRef::Ancestor(0),
        source: SemanticOriginalTypeSource::Expression(scheme.callee_node),
    };
    let expected = world.ty(instance.id, &callee_query, World::type_of(raw_value)?)?;
    world.same(instance.id, raw_value, &expected, implicit)?;
    let signature = &scheme.signature;
    if arguments.len() != signature.parameters.len()
        || function.parameters.len() != arguments.len()
        || function.result_mode != signature.result_mode
        || function.call_kind != signature.call_kind
    {
        return Err(bad(
            "actual callable entry changes the original complete signature roles",
        ));
    }
    let target = world.definition(child.definition)?;
    let (SemanticDefinitionBody::FunctionWithBody {
        signature: target_signature,
        ..
    }
    | SemanticDefinitionBody::Function {
        signature: target_signature,
        ..
    }) = &target.body
    else {
        return Err(bad("actual callable entry has another original body kind"));
    };
    let header = definition.storage_life.as_ref().ok_or_else(|| {
        pending("called environment has no complete Fixed and Invocation domains")
    })?;
    let mut expected_parameters = Vec::new();
    world.pay(signature.parameters.len())?;
    for (ordinal, ((argument, parameter), local)) in arguments
        .iter()
        .zip(&signature.parameters)
        .zip(&function.parameters)
        .enumerate()
    {
        let ty = &function
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("callable entry parameter local is absent"))?
            .ty;
        let expected = world.ty(instance.id, &parameter.ty, ty)?;
        let actual = world.get(values, *argument)?;
        arguments::satisfy(
            world,
            instance,
            Some(child),
            site,
            ordinal,
            *argument,
            &actual,
            &expected,
            locals,
            implicit,
        )?;
        expected_parameters.push(expected);
    }
    let invocation = invocation_inputs(
        world,
        instance,
        child,
        header,
        &expected_parameters,
        implicit,
    )?;
    if target_signature.parameters.len() != signature.parameters.len() {
        return Err(bad("actual callable entry omits original parameter roles"));
    }
    for (ordinal, (((parameter, expected), local), target_parameter)) in signature
        .parameters
        .iter()
        .zip(&expected_parameters)
        .zip(&function.parameters)
        .zip(&target_signature.parameters)
        .enumerate()
    {
        if parameter.mode != target_parameter.mode {
            return Err(bad(
                "callable entry changes an original parameter passing mode",
            ));
        }
        let ty = &function
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("callable entry parameter local is absent"))?
            .ty;
        let target = world.parameter_value(child.id, ordinal, ty)?;
        let target = invocation::map(
            world,
            &target,
            &[],
            Some((header, child, closure_value)),
            &invocation,
        )?;
        world.same(instance.id, &target, expected, implicit)?;
    }
    let expected = call_result(
        world,
        instance,
        site,
        &signature.result,
        &function.return_type,
    )?;
    let target = world.result_value(child.id, &function.return_type)?;
    let target = invocation::map(
        world,
        &target,
        &[],
        Some((header, child, closure_value)),
        &invocation,
    )?;
    world.same(instance.id, &target, &expected, implicit)?;
    Ok(Some(expected))
}

pub(super) fn direct(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    callee: &SemanticInstance,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    arguments: &[ValueId],
    values: &[Option<SemanticTypeValue>],
    locals: &[Option<SemanticTypeValue>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<SemanticTypeValue>> {
    let mut selected = None;
    for site in world.point_sites(sites, block, index)? {
        let SemanticSiteAction::BodyCall { actuals } = &site.action else {
            continue;
        };
        let signature = {
            let ledger = world.ledger;
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("direct call lacks complete original Gamma"))?;
            let Some(signature) = proof.direct_calls.get(&(original.id, site.recipe)) else {
                continue;
            };
            crate::dyn_metadata::semantic_operand_snapshot(signature, world.limits, world.left, 1)?;
            signature.clone()
        };
        if selected.is_some() {
            return Err(bad("actual direct call repeats its original solved scheme"));
        }
        world.body_call(instance, original, site, actuals, callee.definition)?;
        if actuals.len() != callee.actuals.len() {
            return Err(bad("actual direct call omits original mixed formals"));
        }
        for (actual, specialized) in actuals.iter().zip(&callee.actuals) {
            actual_specialization(world, specialized, actual)?;
        }
        if arguments.len() != signature.parameters.len()
            || function.parameters.len() != arguments.len()
            || function.result_mode != signature.result_mode
            || function.call_kind != signature.call_kind
        {
            return Err(bad(
                "actual direct entry changes its complete original signature roles",
            ));
        }
        let target = world.definition(callee.definition)?;
        let (SemanticDefinitionBody::FunctionWithBody {
            signature: target_signature,
            ..
        }
        | SemanticDefinitionBody::Function {
            signature: target_signature,
            ..
        }) = &target.body
        else {
            return Err(bad("direct entry has another original body kind"));
        };
        if target_signature.parameters.len() != signature.parameters.len() {
            return Err(bad("direct entry omits original value parameter roles"));
        }
        let mut parameters = Vec::new();
        world.pay(signature.parameters.len())?;
        for (ordinal, ((argument, parameter), local)) in arguments
            .iter()
            .zip(&signature.parameters)
            .zip(&function.parameters)
            .enumerate()
        {
            let ty = &function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("direct entry parameter local is absent"))?
                .ty;
            let expected = world.ty(instance.id, &parameter.ty, ty)?;
            let actual = world.get(values, *argument)?;
            arguments::satisfy(
                world,
                instance,
                Some(callee),
                site,
                ordinal,
                *argument,
                &actual,
                &expected,
                locals,
                implicit,
            )?;
            parameters.push(expected);
        }
        let invocation = match &function.storage_life {
            Some(header) => {
                invocation_inputs(world, instance, callee, header, &parameters, implicit)?
            }
            None => invocation::Inputs::default(),
        };
        for (ordinal, (((parameter, expected), target_parameter), local)) in signature
            .parameters
            .iter()
            .zip(&parameters)
            .zip(&target_signature.parameters)
            .zip(&function.parameters)
            .enumerate()
        {
            if parameter.mode != target_parameter.mode {
                return Err(bad("direct entry changes an original passing mode"));
            }
            let ty = &function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("direct entry parameter local is absent"))?
                .ty;
            let target = world.parameter_value(callee.id, ordinal, ty)?;
            let target = invocation::map(world, &target, actuals, None, &invocation)?;
            world.same(instance.id, &target, expected, implicit)?;
        }
        let expected = call_result(
            world,
            instance,
            site,
            &signature.result,
            &function.return_type,
        )?;
        let target = world.result_value(callee.id, &function.return_type)?;
        let target = invocation::map(world, &target, actuals, None, &invocation)?;
        world.same(instance.id, &target, &expected, implicit)?;
        selected = Some(expected);
    }
    Ok(selected)
}

/// A sealed task/thread operation carries a separately solved original worker
/// scheme. Its real target is checked with the same complete boundary mapping
/// as an ordinary static call; the builtin itself never becomes a fake Def.
pub(super) fn sealed_worker(
    world: &mut World<'_, '_>,
    caller: &SemanticInstance,
    callee: &SemanticInstance,
    function: &TypedFunction,
    signature: &SemanticSignatureRecipe,
    recipes: &[SemanticActualRecipe],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    if function.call_kind != signature.call_kind
        || function.result_mode != signature.result_mode
        || function.parameters.len() != signature.parameters.len()
        || callee.actuals.len() != recipes.len()
    {
        return Err(bad(
            "sealed worker changes its full original ABI or mixed arity",
        ));
    }
    world.pay(recipes.len().saturating_add(signature.parameters.len()))?;
    let mut actuals = Vec::new();
    for (recipe, target) in recipes.iter().zip(&callee.actuals) {
        let actual = super::super::materialize::actual_at(
            world.program,
            world.module,
            world.ledger,
            caller.id,
            recipe,
            world.limits,
            world.left,
        )?;
        actual_specialization(world, target, &actual)?;
        actuals.push(actual);
    }
    let original = world.definition(callee.definition)?;
    let SemanticDefinitionBody::FunctionWithBody { signature: raw, .. } = &original.body else {
        return Err(bad("sealed worker lacks its complete original declaration"));
    };
    if raw.parameters.len() != signature.parameters.len() {
        return Err(bad("sealed worker changes its original parameter count"));
    }
    let mut parameters = Vec::new();
    for ((parameter, raw), local) in signature
        .parameters
        .iter()
        .zip(&raw.parameters)
        .zip(&function.parameters)
    {
        if parameter.mode != raw.mode {
            return Err(bad("sealed worker changes an original passing mode"));
        }
        let ty = &function
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("sealed worker parameter local is absent"))?
            .ty;
        parameters.push(world.ty(caller.id, &parameter.ty, ty)?);
    }
    let invocation = match &function.storage_life {
        Some(header) => invocation_inputs(world, caller, callee, header, &parameters, implicit)?,
        None => invocation::Inputs::default(),
    };
    for (ordinal, (expected, local)) in parameters.iter().zip(&function.parameters).enumerate() {
        let ty = &function
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("sealed worker parameter local is absent"))?
            .ty;
        let target = world.parameter_value(callee.id, ordinal, ty)?;
        let target = invocation::map(world, &target, &actuals, None, &invocation)?;
        world.same(caller.id, &target, expected, implicit)?;
    }
    let expected = world.ty(caller.id, &signature.result, &function.return_type)?;
    let target = world.result_value(callee.id, &function.return_type)?;
    let target = invocation::map(world, &target, &actuals, None, &invocation)?;
    world.same(caller.id, &target, &expected, implicit)
}
