//! Actual erased receivers and slot boundaries. The original interface scheme
//! is solved before this module sees a Typed slot or a concrete provider.
use super::*;
use crate::semantics::original::{IntrinsicCall, IntrinsicKind};

pub(super) fn borrow(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    local: LocalId,
    mutable: bool,
    ty: &Type,
    image: SemanticClassExpr,
    source: &SemanticTypeValue,
) -> R<SemanticTypeValue> {
    let source = match World::type_of(source)? {
        Type::OwnedDyn { .. } => {
            if mutable
                && !function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| bad("dynamic owner local is absent"))?
                    .mutable
            {
                return Err(bad("exclusive dynamic borrow has an immutable owner local"));
            }
            world.copy(source)?
        }
        Type::Address {
            mutable: parent,
            inner,
        } if matches!(inner.as_ref(), Type::OwnedDyn { .. }) => {
            if mutable && !*parent {
                return Err(bad(
                    "exclusive dynamic borrow has only a shared parent address",
                ));
            }
            world.pointee(source)?
        }
        _ => {
            return Err(bad(
                "dynamic owner borrow reads another actual carrier kind",
            ));
        }
    };
    let SemanticTypeValue::Resolved {
        ty: Type::OwnedDyn { interface },
        life,
        classes,
    } = source
    else {
        return Err(bad("dynamic owner borrow has no complete retained domain"));
    };
    if !matches!(ty,Type::Address{mutable:actual,inner} if *actual==mutable && inner.as_ref()==&Type::Dyn{interface})
    {
        return Err(bad(
            "dynamic owner borrow changes its interface or receiver mode",
        ));
    }
    let inner = SemanticTypeValue::Resolved {
        ty: Type::Dyn { interface },
        life,
        classes,
    };
    world.address(ty, image, &inner)
}

/// Bind only the fresh outer receiver role. The Before observation is the
/// initialized owner; its complete Context domain is preserved by `borrow`.
pub(super) fn seal_borrow(
    world: &mut World<'_, '_>,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    local: LocalId,
    image: &SemanticClassExpr,
    implicit: &mut BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Err(bad("dynamic receiver borrow has no original body"));
    };
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("dynamic receiver borrow has no original Gamma"))?;
    let mut role = None;
    for site in world.point_sites(sites, block, index)? {
        if !matches!(
            site.at,
            SemanticSiteAddress::Typed {
                edge: SemanticSiteEdge::Before,
                ..
            }
        ) || !matches!(site.action, SemanticSiteAction::TypeUse { subject: SemanticOperand::Local(target), .. } if target == local)
        {
            continue;
        }
        let recipe = world.original_recipe(original, site.recipe)?;
        world.pay(site_nodes.len())?;
        let mut nodes = site_nodes.iter().filter(|row| row.site == site.recipe);
        let Some(row) = nodes.next() else {
            return Err(bad("dynamic receiver observation has no original node"));
        };
        if nodes.next().is_some() {
            return Err(bad(
                "dynamic receiver observation repeats its original node",
            ));
        }
        let SemanticOriginalNode::Expression(node) = row.node else {
            return Err(bad("dynamic receiver observation is not an expression"));
        };
        world.pay(proof.intrinsics.len())?;
        for ((owner, _), call) in &proof.intrinsics {
            if *owner != original.id
                || !matches!(call.kind, IntrinsicKind::DynCall { receiver_node, .. } if receiver_node == node)
            {
                continue;
            }
            let Some(SemanticTypeRecipe::Address {
                lifetime:
                    SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    }),
                ..
            }) = call.parameters.first()
            else {
                continue;
            };
            world.pay(original.implicit_lifetimes.len())?;
            if !original.implicit_lifetimes.iter().any(|entry| {
                entry.ordinal == *ordinal && entry.occurrence_path == recipe.occurrence_path
            }) {
                return Err(bad(
                    "dynamic receiver outer role differs from its original borrow occurrence",
                ));
            }
            if role.is_some_and(|previous| previous != *ordinal) {
                return Err(bad("dynamic receiver borrow has ambiguous original roles"));
            }
            role = Some(*ordinal);
        }
    }
    let ordinal = role
        .ok_or_else(|| bad("dynamic receiver borrow has no complete original occurrence seal"))?;
    world.pay(implicit.len().saturating_add(1))?;
    crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    if let Some(old) = implicit.insert(ordinal, image.clone()) {
        if old != *image {
            return Err(bad(
                "one dynamic receiver occurrence selects different actual heads",
            ));
        }
    }
    Ok(())
}
fn insert(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    map: &mut BTreeMap<StorageBinder, SemanticClassExpr>,
    binder: StorageBinder,
    image: &SemanticClassExpr,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    world.pay(map.len().saturating_add(1))?;
    if let Some(old) = map.get(&binder) {
        return world.same_class(scope, old, image, implicit);
    }
    crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    map.insert(binder, image.clone());
    Ok(())
}
fn mixed(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    expected: &SemanticActual,
    actual: &DynGenericActual,
    map: &BTreeMap<StorageBinder, SemanticClassExpr>,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    crate::dyn_metadata::semantic_operand_snapshot(expected, world.limits, world.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
    match (expected, actual) {
        (
            SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes }),
            DynGenericActual::Type(target),
        ) if ty == target && life.binder_count == 0 && classes.is_empty() => Ok(()),
        (
            SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes }),
            DynGenericActual::QualifiedType(target),
        ) if ty == &target.ty && life == &target.life && classes.len() == target.classes.len() => {
            world.pay(classes.len())?;
            for (image, binder) in classes.iter().zip(&target.classes) {
                let expected = map
                    .get(binder)
                    .ok_or_else(|| bad("dynamic Type actual uses an unbound full class role"))?;
                world.same_class(scope, image, expected, implicit)?;
            }
            Ok(())
        }
        (
            SemanticActual::Const {
                ty: SemanticTypeValue::Resolved { ty, life, classes },
                value,
            },
            DynGenericActual::Const(target),
        ) if ty == &target.ty()
            && life.binder_count == 0
            && classes.is_empty()
            && value == target =>
        {
            Ok(())
        }
        (SemanticActual::Life(image), DynGenericActual::Life(binder)) => {
            let target = map
                .get(binder)
                .ok_or_else(|| bad("dynamic Life actual uses an unbound original role"))?;
            world.same_class(scope, image, target, implicit)
        }
        _ => Err(bad(
            "dynamic slot changes its complete original mixed actual",
        )),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    call: &IntrinsicCall,
    site: &SemanticSite,
    instruction: &InstructionKind,
    parameters: &[SemanticTypeValue],
    result: &SemanticTypeValue,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let IntrinsicKind::DynCall {
        receiver_node: _,
        declaration,
        parent,
        member,
        receiver_mode,
        parameter_modes,
        result_mode,
        call_kind,
    } = &call.kind
    else {
        return Err(bad("dynamic bridge receives another original scheme"));
    };
    let InstructionKind::CallDynSlot {
        interface, slot, ..
    } = instruction
    else {
        return Err(bad("dynamic original call has another actual opcode"));
    };
    world.pay(world.module.dyn_interfaces.len())?;
    let mut interfaces = world
        .module
        .dyn_interfaces
        .iter()
        .filter(|row| row.id == *interface);
    let table = interfaces
        .next()
        .ok_or_else(|| bad("dynamic slot has no actual interface"))?;
    if interfaces.next().is_some() {
        return Err(bad("dynamic actual interface identity repeats"));
    }
    let target = table
        .slots
        .get(*slot as usize)
        .ok_or_else(|| bad("dynamic actual slot ordinal is absent"))?;
    crate::dyn_metadata::actual_slot_snapshot(target, world.limits, world.left, 1)?;
    super::super::materialize::dynamic::call_slot(
        world.program,
        world.module,
        world.ledger,
        *interface,
        *slot,
        *parent,
        *declaration,
        *member,
        world.limits,
        world.left,
    )?;
    if target.receiver.mode != *receiver_mode
        || target.parameter_modes != *parameter_modes
        || target.result_mode != *result_mode
        || target.call_kind != *call_kind
        || parameters.len() != target.parameters.len().saturating_add(1)
        || World::type_of(result)? != &target.result
    {
        return Err(bad(
            "dynamic actual slot changes its complete original ABI or modes",
        ));
    }
    let receiver = parameters
        .first()
        .ok_or_else(|| bad("dynamic scheme omits its receiver"))?;
    if !matches!(World::type_of(receiver)?,Type::Address{mutable,inner} if *mutable==target.receiver.mutable&&inner.as_ref()==&Type::Dyn{interface:*interface})
    {
        return Err(bad(
            "dynamic actual receiver changes its complete original interface or access",
        ));
    }
    for (parameter, ty) in parameters.iter().skip(1).zip(&target.parameters) {
        if World::type_of(parameter)? != ty {
            return Err(bad(
                "dynamic slot changes a complete original parameter type",
            ));
        }
    }
    let context = world.pointee(receiver)?;
    let SemanticTypeValue::Resolved {
        classes: context, ..
    } = &context
    else {
        return Err(bad("dynamic receiver Context remains unresolved"));
    };
    let header = target
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("dynamic slot omits its complete original lifetime header"))?;
    if header.fixed_binder_count as usize != context.len() {
        return Err(bad(
            "dynamic header truncates the entire receiver Context domain",
        ));
    }
    let mut bindings = Vec::new();
    let mut classes = Vec::new();
    let mut statics = Vec::new();
    world.pay(parameters.len().saturating_add(context.len()))?;
    for (ordinal, parameter) in parameters.iter().enumerate() {
        world.header_value(
            parameter,
            StorageBase::Parameter(ordinal as u32),
            &mut bindings,
            &mut classes,
            &mut statics,
        )?;
    }
    world.header_value(
        result,
        StorageBase::Result,
        &mut bindings,
        &mut classes,
        &mut statics,
    )?;
    if bindings.len() != header.bindings.len()
        || classes.len() != header.classes.len()
        || statics.len() != header.static_bindings.len()
    {
        return Err(bad(
            "dynamic slot header omits an original outer or intrinsic class occurrence",
        ));
    }
    let mut map = BTreeMap::new();
    for (ordinal, image) in context.iter().enumerate() {
        insert(
            world,
            instance.id,
            &mut map,
            StorageBinder::Fixed(ordinal as u32),
            image,
            implicit,
        )?;
    }
    let binding_paths = bindings.iter().fold(0usize, |n, (position, _)| {
        n.saturating_add(position.path.len().saturating_add(1))
    });
    let class_paths = classes.iter().fold(0usize, |n, (position, _)| {
        n.saturating_add(position.path.len().saturating_add(1))
    });
    let actual_binding_paths = header.bindings.iter().fold(0usize, |n, row| {
        n.saturating_add(row.position.path.len().saturating_add(1))
    });
    let actual_class_paths = header.classes.iter().fold(0usize, |n, row| {
        n.saturating_add(row.position.path.len().saturating_add(1))
    });
    world.pay(
        binding_paths
            .saturating_mul(header.bindings.len())
            .saturating_add(actual_binding_paths.saturating_mul(bindings.len()))
            .saturating_add(class_paths.saturating_mul(header.classes.len()))
            .saturating_add(actual_class_paths.saturating_mul(classes.len())),
    )?;
    // Input occurrences establish invocation images. Outputs may only consume
    // that mapping; an output claim never anchors its own class proof.
    for outputs in [false, true] {
        world.pay(
            header
                .bindings
                .len()
                .saturating_mul(bindings.len())
                .saturating_add(header.classes.len().saturating_mul(classes.len())),
        )?;
        for row in &header.bindings {
            if (row.position.base == StorageBase::Result) != outputs {
                continue;
            }
            if !matches!(
                row.position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                return Err(bad(
                    "dynamic header reads an invented capture or receiver scope",
                ));
            }
            let mut expected = bindings
                .iter()
                .filter(|(position, _)| position == &row.position);
            let (_, image) = expected
                .next()
                .ok_or_else(|| bad("dynamic header invents an original outer occurrence"))?;
            if expected.next().is_some() {
                return Err(bad("dynamic original outer occurrence is ambiguous"));
            }
            match row.binder {
                StorageBinder::Fixed(index) if index >= header.fixed_binder_count => {
                    return Err(bad("dynamic Fixed binder exceeds complete Context"));
                }
                StorageBinder::Invocation(index) if index >= header.invocation_binder_count => {
                    return Err(bad("dynamic Invocation binder exceeds its original scope"));
                }
                _ => {}
            }
            if outputs {
                let target = map.get(&row.binder).ok_or_else(|| {
                    pending(
                        "dynamic result class lacks a complete original input or Context source",
                    )
                })?;
                world.same_class(instance.id, image, target, implicit)?;
            } else {
                insert(world, instance.id, &mut map, row.binder, image, implicit)?;
            }
        }
        for row in &header.classes {
            if (row.position.base == StorageBase::Result) != outputs {
                continue;
            }
            if !matches!(
                row.position.base,
                StorageBase::Parameter(_) | StorageBase::Result
            ) {
                return Err(bad("dynamic intrinsic class reads an invented scope"));
            }
            let mut expected = classes
                .iter()
                .filter(|(position, _)| position == &row.position);
            let (_, image) = expected
                .next()
                .ok_or_else(|| bad("dynamic header invents a complete intrinsic occurrence"))?;
            if expected.next().is_some() {
                return Err(bad("dynamic intrinsic occurrence is ambiguous"));
            }
            match row.binder {
                StorageBinder::Fixed(index) if index >= header.fixed_binder_count => {
                    return Err(bad("dynamic intrinsic Fixed binder exceeds Context"));
                }
                StorageBinder::Invocation(index) if index >= header.invocation_binder_count => {
                    return Err(bad("dynamic intrinsic Invocation binder exceeds scope"));
                }
                _ => {}
            }
            if outputs {
                let target = map.get(&row.binder).ok_or_else(|| {
                    pending("dynamic result intrinsic class lacks its complete original source")
                })?;
                world.same_class(instance.id, image, target, implicit)?;
            } else {
                insert(world, instance.id, &mut map, row.binder, image, implicit)?;
            }
        }
    }
    world.pay(
        statics
            .iter()
            .fold(0usize, |n, position| {
                n.saturating_add(position.path.len().saturating_add(1))
            })
            .saturating_mul(header.static_bindings.len())
            .saturating_add(
                header
                    .static_bindings
                    .iter()
                    .fold(0usize, |n, position| {
                        n.saturating_add(position.path.len().saturating_add(1))
                    })
                    .saturating_mul(statics.len()),
            ),
    )?;
    for position in &statics {
        if header
            .static_bindings
            .iter()
            .filter(|candidate| *candidate == position)
            .count()
            != 1
        {
            return Err(bad(
                "dynamic slot changes a complete original static occurrence",
            ));
        }
    }
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("dynamic call loses its checked original mixed solution"))?;
    let recipes = proof
        .calls
        .get(&(instance.definition, site.recipe))
        .ok_or_else(|| bad("dynamic call has no original mixed solution"))?;
    if recipes.len() != target.actuals.len() {
        return Err(bad("dynamic slot omits full method actuals"));
    }
    world.pay(recipes.len())?;
    for (recipe, actual) in recipes.iter().zip(&target.actuals) {
        let expected = super::super::materialize::actual_at(
            world.program,
            world.module,
            world.ledger,
            instance.id,
            recipe,
            world.limits,
            world.left,
        )?;
        mixed(world, instance.id, &expected, actual, &map, implicit)?;
    }
    Ok(())
}
