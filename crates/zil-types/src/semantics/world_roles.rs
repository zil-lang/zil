//! Raw outer-role adaptation belongs to one initialized occupant. Its actual
//! incoming image stays separate from exact class normalization and whole N.
use super::*;
use crate::semantics::original::{LocalRole, LocalRoleKind};
#[path = "world_environment_classes.rs"]
mod environment_classes;
pub(super) fn environment_class(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    image: &SemanticClassExpr,
) -> R<SemanticClassExpr> {
    environment_classes::normalize(world, scope, image)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Static,
    Class(SemanticClassExpr),
}

/// A declaration-source trace checked at its real executed boundary. Identity
/// is transported inside the same incoming-source fact as the raw outer image.
/// Whole-state equality also retains roots, owners, vacancies and stale epochs;
/// equal source-node/SSA spellings alone never establish a reaching world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum OriginAnchor {
    /// The original trace selected the referent of this input, not its local
    /// pointer slot. Its lifetime belongs to the caller.
    Input(u32),
    /// This exact field in the current closure's environment.
    Capture(u32),
    /// No stable outside-frame root theorem was established by this trace.
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct OriginalLoan {
    pub node: zil_base::NodeId,
    pub mint: ValueId,
    pub anchors: Rc<Vec<OriginAnchor>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Incoming {
    pub source: Source,
    pub origin: Option<OriginalLoan>,
}
pub(super) fn snapshot_incoming(
    value: &Incoming, limits: &Limits, left: &mut usize,
) -> R<Incoming> {
    *left = left.checked_sub(1).ok_or_else(|| Diagnostic::limit(
        Phase::Types, "incoming source receipt snapshot exceeded", None))?;
    Ok(Incoming { source: snapshot_source(&value.source, limits, left)?, origin: value.origin.clone() })
}
pub(super) fn incoming(
    world: &mut World<'_, '_>, operand: ValueId, source: Source,
) -> R<Incoming> {
    world.pay(index::search(world.value_role_sources.len()).saturating_add(1))?;
    let origin = world.value_role_sources.get(&operand).and_then(|value| value.origin.clone());
    Ok(Incoming { source, origin })
}

pub(super) fn stable_origin(
    world: &mut World<'_, '_>, source: &Incoming, instance: SemanticInstanceId,
    vacant: &std::collections::BTreeSet<(SemanticInstanceId, u32)>,
) -> R<bool> {
    let Some(origin) = &source.origin else { return Ok(false); };
    world.pay(origin.anchors.len().saturating_add(1))?;
    if origin.anchors.is_empty() { return Ok(false); }
    for anchor in origin.anchors.iter() {
        match anchor {
            OriginAnchor::Input(_) => {}
            OriginAnchor::Capture(capture) => {
                world.pay(index::search(vacant.len()))?;
                if vacant.contains(&(instance, *capture)) { return Ok(false); }
            }
            OriginAnchor::Other => return Ok(false),
        }
    }
    Ok(true)
}

/// Writable operations retire the entire incoming fact (image and source
/// receipt together). Immutable SSA copies retain their own referent fact;
/// replacing a pointer cell never rewrites an already loaded pointer value.
pub(super) fn invalidate_roots(
    world: &mut World<'_, '_>, roots: &std::collections::BTreeSet<arguments::Root>,
) -> R<()> {
    if roots.is_empty() {
        world.pay(world.role_sources.len().saturating_add(1))?;
        world.role_sources.clear();
    } else {
        for root in roots {
            world.pay(root.path.len().saturating_add(index::search(world.role_sources.len())).saturating_add(1))?;
            // This container describes the local's OUTER pointer/description
            // occupant. A write to a proper pointee/aggregate subtree leaves
            // that pointer cell intact; sibling field writes cannot erase it.
            if root.path.is_empty() { world.role_sources.remove(&root.local); }
        }
    }
    Ok(())
}
pub(super) fn invalidate_address(
    world: &mut World<'_, '_>, address: ValueId, values: &[Option<SharedType>],
) -> R<()> {
    let value = world.get(values, address)?;
    let roots = arguments::roots(world, address, &value)?;
    invalidate_roots(world, &roots)
}
/// Actual argument shapes identify writable inputs; unknown environment/body
/// execution retires local source facts instead of pretending it is pure.
pub(super) fn invalidate_operation(
    world: &mut World<'_, '_>, operation: &InstructionKind, values: &[Option<SharedType>],
) -> R<()> {
    match operation {
        InstructionKind::WriteAddress { address, .. } => invalidate_address(world, *address, values),
        InstructionKind::WriteDesc { desc, .. } => invalidate_address(world, *desc, values),
        InstructionKind::StoreStoredBorrow { storage, .. } => invalidate_address(world, *storage, values),
        InstructionKind::Call { arguments, .. } | InstructionKind::SuspendCall { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. } | InstructionKind::CallIndirect { arguments, .. } => {
            world.pay(arguments.len())?;
            for argument in arguments {
                let value = world.get(values, *argument)?;
                if matches!(World::type_of(&value)?, Type::Address { mutable: true, .. } | Type::Desc(_)) {
                    let roots = arguments::roots(world, *argument, &value)?;
                    invalidate_roots(world, &roots)?;
                }
            }
            Ok(())
        }
        InstructionKind::CallClosure { .. } | InstructionKind::CallClosureOnce { .. }
        | InstructionKind::CallClosureShared { .. } | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosureBorrowed { .. } | InstructionKind::CallDynSlot { .. }
        | InstructionKind::WithForeignCallback { .. } | InstructionKind::WithScopedThread { .. }
        | InstructionKind::WithSyncCell { .. } | InstructionKind::WriteCapture { .. }
        | InstructionKind::TaskPoll { .. } | InstructionKind::TaskPollClose { .. }
        | InstructionKind::RegistrationStart { .. } | InstructionKind::RegistrationClose { .. }
        | InstructionKind::AsyncRegistrationStart { .. } | InstructionKind::AsyncRegistrationPollClose { .. } => {
            world.pay(world.role_sources.len().saturating_add(1))?;
            world.role_sources.clear();
            Ok(())
        }
        // All other operations remain subject to the original whole-type and
        // source trace checks. The all-writes local source traversal rejects
        // untracked writable opcodes before it can consume a provenance receipt.
        _ => Ok(()),
    }
}

/// An outer occurrence inside a sealed whole-Type environment is a projection
/// of that Type's own complete domain. Its binder is read from the exact port,
/// never from the environment ordinal or equality of outside images.
pub(super) fn environment_outer(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    ordinal: u32,
    path: &[LifeTypeProjection],
    mode: StoredBorrowMode,
) -> R<SemanticClassExpr> {
    let instance = world.instance(scope)?;
    let original = world.definition(instance.definition)?;
    world.pay(
        original
            .environment
            .len()
            .saturating_add(path.len())
            .saturating_add(1),
    )?;
    if !matches!(
        original
            .environment
            .get(ordinal as usize)
            .map(|row| &row.kind),
        Some(SemanticFormal::Type { .. })
    ) {
        return Err(bad(
            "outer environment occurrence does not select its original whole-Type slot",
        ));
    }
    let recipe = SemanticActualRecipe::Type(SemanticTypeRecipe::Environment {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    });
    let SemanticActual::Type(value) = super::super::materialize::actual_in_view(
        world.program,
        &world.view,
        world.ledger,
        scope,
        &recipe,
        world.limits,
        world.left,
    )?
    else {
        return Err(bad("outer environment occurrence changes its actual kind"));
    };
    if path.len() > world.limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "environment outer projection depth exceeded",
            None,
        ));
    }
    world.pay(1)?;
    let mut value = Rc::new(value);
    for step in path {
        value = match step {
            LifeTypeProjection::Place(selector) => projection::place(world, &value, *selector)?,
            LifeTypeProjection::Pointee => projection::pointee(world, &value)?,
            _ => {
                return Err(pending(
                    "environment outer occurrence requires its complete selected physical projection",
                ));
            }
        };
    }
    let SemanticTypeValue::Resolved { ty, life, classes } = world.owned(value)? else {
        return Err(bad("outer environment projection remains unresolved"));
    };
    if !matches!(
        (stored::physical(&ty), mode),
        (Type::Desc(_), StoredBorrowMode::WritableDescription)
            | (
                Type::Address { mutable: false, .. },
                StoredBorrowMode::SharedAddress
            )
            | (
                Type::Address { mutable: true, .. },
                StoredBorrowMode::ExclusiveAddress
            )
    ) {
        return Err(bad(
            "outer environment projection changes its original physical view mode",
        ));
    }
    world.pay(life.slots.len().saturating_add(classes.len()))?;
    let mut ports = life
        .slots
        .iter()
        .filter(|port| port.path.is_empty() && port.mode == mode);
    let port = ports
        .next()
        .ok_or_else(|| bad("outer environment projection has no exact variable port"))?;
    if ports.next().is_some() {
        return Err(bad(
            "outer environment projection has ambiguous variable ports",
        ));
    }
    let image = classes
        .get(port.binder as usize)
        .ok_or_else(|| bad("outer environment port leaves its complete intrinsic domain"))?;
    crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
    Ok(image.clone())
}
pub(super) fn snapshot_source(source: &Source, limits: &Limits, left: &mut usize) -> R<Source> {
    *left = left.checked_sub(1).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "local role source snapshot work exceeded",
            None,
        )
    })?;
    if let Source::Class(image) = source {
        crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
    }
    Ok(source.clone())
}
pub(super) fn copy_source(world: &mut World<'_, '_>, source: &Source) -> R<Source> {
    snapshot_source(source, world.limits, world.left)
}

pub(super) fn outer(world: &mut World<'_, '_>, value: &SemanticTypeValue) -> R<Source> {
    let SemanticTypeValue::Resolved { ty, life, classes } = value else {
        return Err(bad("local role retains an unresolved actual type"));
    };
    if !matches!(ty, Type::Address { .. } | Type::Desc(_)) {
        return Err(bad("local role changes its complete raw outer constructor"));
    }
    world.pay(life.slots.len())?;
    let mut rows = life.slots.iter().filter(|slot| slot.path.is_empty());
    if let Some(slot) = rows.next() {
        if rows.next().is_some() {
            return Err(bad("local role has several actual outer occurrences"));
        }
        let image = classes
            .get(slot.binder as usize)
            .ok_or_else(|| bad("local role outer occurrence exceeds complete domain"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
        Ok(Source::Class(image.clone()))
    } else if matches!(ty, Type::Address { mutable: false, .. }) {
        Ok(Source::Static)
    } else {
        Err(bad(
            "mutable outer role has no actual initialized occurrence",
        ))
    }
}
fn receipt(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    local: LocalId,
) -> R<Option<LocalRole>> {
    world.pay(sites.len())?;
    let mut selected = None;
    for site in sites {
        if !matches!(site.action,SemanticSiteAction::DeclareLocal{local:SemanticLocalTarget::Typed(id),..}if id==local)
        {
            continue;
        }
        let recipe = world.original_recipe(original, site.recipe)?;
        let SemanticSiteRecipeKind::DeclareLocal {
            expected:
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Binding(binding),
                },
        } = &recipe.kind
        else {
            continue;
        };
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("local role lacks complete original Gamma"))?;
        let Some(role) = proof.local_roles.get(&(original.id, *binding)) else {
            continue;
        };
        if site.instance != instance.id || role.binding != *binding || selected.is_some() {
            return Err(bad(
                "typed local role repeats or changes its original declaration",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(&role.ty, world.limits, world.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(
            &role.lifetime,
            world.limits,
            world.left,
            1,
        )?;
        world.pay(role.sources.len())?;
        for source in &role.sources {
            crate::dyn_metadata::semantic_operand_snapshot(
                &source.lifetime,
                world.limits,
                world.left,
                1,
            )?;
        }
        selected = Some(role.clone());
    }
    Ok(selected)
}
pub(super) fn store(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    local: LocalId,
    operand: ValueId,
    value: &SemanticTypeValue,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(SharedType, Source)>> {
    let Some(role) = receipt(world, instance, original, sites, local)? else {
        return Ok(None);
    };
    let valid = matches!((role.kind,World::type_of(value)?),(LocalRoleKind::Address{mutable:a},Type::Address{mutable:b,..})if a==*b)
        || matches!(
            (role.kind, World::type_of(value)?),
            (LocalRoleKind::Desc, Type::Desc(_))
        );
    if !valid {
        return Err(bad(
            "local outer role changes its actual access permission mode",
        ));
    }
    let expected = world.physical_type(instance.id, &role.ty, World::type_of(value)?)?;
    let actual_inner = world.pointee(value)?;
    let expected_inner = world.pointee(&expected)?;
    world.same(instance.id, &actual_inner, &expected_inner, implicit)?;
    let actual_outer = outer(world, value)?;
    let expected_outer = outer(world, &expected)?;
    let mut allowed =
        actual_outer == expected_outer && world.value_role_sources.contains_key(&operand);
    for source in &role.sources {
        world.pay(1)?;
        let candidate = match &source.lifetime {
            SemanticOuterLifeRecipe::Static => Source::Static,
            SemanticOuterLifeRecipe::Class(recipe) => {
                let image = super::super::materialize::class_in_view(
                    world.program,
                    &world.view,
                    world.ledger,
                    instance.id,
                    recipe,
                    world.limits,
                    world.left,
                )?;
                Source::Class(world.class(instance.id, &image, implicit)?)
            }
        };
        if candidate == actual_outer {
            allowed = true;
        }
    }
    if !allowed {
        return Err(bad(
            "actual local role assignment has no independently proved original source bound",
        ));
    }
    let source = if let Some(source) = world.value_role_sources.get(&operand) {
        snapshot_source(&source.source, world.limits, world.left)?
    } else {
        copy_source(world, &actual_outer)?
    };
    Ok(Some((expected, source)))
}

/// Recheck the exact incoming image of the pointer moved into a closure.
pub(super) fn moved_capture(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    operand: ValueId,
    actual: &SemanticTypeValue,
    expected: &SemanticTypeValue,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<bool> {
    if !matches!(World::type_of(expected)?, Type::Address { .. } | Type::Desc(_))
        || World::type_of(actual)? != World::type_of(expected)?
    {
        return Ok(false);
    }
    world.pay(index::search(world.value_role_sources.len()))?;
    let Some(Incoming { source: Source::Class(incoming), .. }) = world.value_role_sources.get(&operand) else {
        return Ok(false);
    };
    crate::dyn_metadata::semantic_operand_snapshot(incoming, world.limits, world.left, 1)?;
    let incoming = incoming.clone();
    let Source::Class(target) = outer(world, expected)? else {
        return Ok(false);
    };
    if world.class(scope, &incoming, implicit)? != world.class(scope, &target, implicit)? {
        return Ok(false);
    }
    let pointee = world.pointee(actual)?;
    let transported = world.address(World::type_of(actual)?, incoming, &pointee)?;
    world.same(scope, &transported, expected, implicit)?;
    Ok(true)
}

/// Source bounds remain attached to a live class even after an outer role has
/// entered a nominal whole value. An unproved local origin cannot become a
/// stable loop identity merely because the raw outer class was adapted.
pub(super) fn loop_sources(
    world: &mut World<'_, '_>,
    instance: SemanticInstanceId,
    ordinal: u32,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(bool, Vec<SemanticClassExpr>)>> {
    let owner = world.instance(instance)?.definition;
    let binding = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("loop role bounds lack complete original Gamma"))?;
        world.pay(proof.local_roles.len())?;
        proof.local_roles.iter().find_map(|((scope,binding),role)|(*scope==owner&&matches!(role.lifetime,SemanticLifeRecipe::Implicit{scope:SemanticScopeRef::Ancestor(0),ordinal:found}if found==ordinal)).then_some(*binding))
    };
    let Some(binding) = binding else {
        return Ok(None);
    };
    let mut work = vec![(owner, binding, false, 0usize)];
    let mut visited = std::collections::BTreeSet::new();
    let mut images = Vec::new();
    let mut unknown = false;
    while let Some((scope, binding, region, depth)) = work.pop() {
        world.pay(1)?;
        if depth > world.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "loop role source depth exceeded",
                None,
            ));
        }
        if !visited.insert((scope, binding, region)) {
            continue;
        }
        if scope != owner {
            unknown = true;
            continue;
        }
        let sources = {
            let ledger = world.ledger;
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("loop role source proof disappeared"))?;
            let sources = if region {
                &proof
                    .call_regions
                    .get(&(scope, binding.0))
                    .ok_or_else(|| bad("loop source references an absent original call region"))?
                    .sources
            } else {
                &proof
                    .local_roles
                    .get(&(scope, binding))
                    .ok_or_else(|| bad("loop role source references an absent original role"))?
                    .sources
            };
            world.pay(sources.len())?;
            for source in sources {
                crate::dyn_metadata::semantic_operand_snapshot(
                    &source.lifetime,
                    world.limits,
                    world.left,
                    1,
                )?;
            }
            sources.clone()
        };
        for source in sources {
            match source.escape {
                crate::semantics::original::OriginalRoleEscape::Input { .. }
                | crate::semantics::original::OriginalRoleEscape::Static => {}
                crate::semantics::original::OriginalRoleEscape::Role { owner, binding } => {
                    world.pay(1)?;
                    work.push((owner, binding, false, depth + 1));
                }
                crate::semantics::original::OriginalRoleEscape::CallRegion { owner, ordinal } => {
                    world.pay(1)?;
                    work.push((owner, zil_base::NodeId(ordinal), true, depth + 1));
                }
                crate::semantics::original::OriginalRoleEscape::LocalPlace { .. } => {
                    match source.lifetime {
                        SemanticOuterLifeRecipe::Static => {}
                        SemanticOuterLifeRecipe::Class(recipe) => {
                            let image = super::super::materialize::class_in_view(
                                world.program,
                                &world.view,
                                world.ledger,
                                instance,
                                &recipe,
                                world.limits,
                                world.left,
                            )?;
                            let image = world.class(instance, &image, implicit)?;
                            if matches!(image, SemanticClassExpr::Implicit { .. }) {
                                unknown = true;
                            } else {
                                world.pay(1)?;
                                images.push(image);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(Some((unknown, images)))
}
