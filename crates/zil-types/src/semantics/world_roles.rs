//! Raw outer-role adaptation belongs to one initialized occupant. Its actual
//! incoming image stays separate from exact class normalization and whole N.
use super::*;
use crate::semantics::original::{LocalRole, LocalRoleKind};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Static,
    Class(SemanticClassExpr),
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

fn outer(world: &mut World<'_, '_>, value: &SemanticTypeValue) -> R<Source> {
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
) -> R<Option<(SemanticTypeValue, Source)>> {
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
    let expected = world.ty(instance.id, &role.ty, World::type_of(value)?)?;
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
                let image = super::super::materialize::class_at(
                    world.program,
                    world.module,
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
        snapshot_source(source, world.limits, world.left)?
    } else {
        copy_source(world, &actual_outer)?
    };
    Ok(Some((expected, source)))
}
