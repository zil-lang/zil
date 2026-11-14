//! Original method storage roots and real receiver borrows meet only at this
//! call boundary. Storage queries retain their original complete value type.
use super::*;
use crate::semantics::original::MethodCall;

pub(super) struct Adaptation {
    pub caller: SemanticInstanceId,
    pub site: SemanticSiteId,
    pub operand: ValueId,
    pub source: roles::Source,
    pub target: SemanticClassExpr,
}
pub(super) fn permits(
    world: &mut World<'_, '_>,
    caller: SemanticInstanceId,
    site: SemanticSiteId,
    operand: ValueId,
    source: &roles::Source,
    target: &roles::Source,
) -> R<bool> {
    let roles::Source::Class(target) = target else {
        return Ok(false);
    };
    world.pay(world.receiver_adaptations.len())?;
    for proof in &world.receiver_adaptations {
        if proof.caller != caller || proof.site != site || proof.operand != operand {
            continue;
        }
        if let roles::Source::Class(image) = &proof.source {
            crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&proof.target, world.limits, world.left, 1)?;
        if &proof.source == source && &proof.target == target {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn seal(
    world: &mut World<'_, '_>,
    caller: &SemanticInstance,
    original: &SemanticDefinition,
    site: &SemanticSite,
    method: &MethodCall,
    arguments: &[ValueId],
    values: &[Option<SemanticTypeValue>],
    locals: &[Option<SemanticTypeValue>],
    implicit: &mut BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let Some(borrow) = &method.receiver_borrow else {
        return Ok(());
    };
    let parameter = method
        .receiver_parameter
        .ok_or_else(|| bad("method borrow has no original receiver parameter"))?;
    let node = method
        .receiver_node
        .ok_or_else(|| bad("method borrow has no original receiver occurrence"))?;
    let place = method
        .receiver_place
        .as_ref()
        .ok_or_else(|| pending("method borrow lacks its complete original storage root"))?;
    if place.owner != original.id {
        return Err(pending(
            "method capture borrow requires its complete creator storage relocation",
        ));
    }
    let function = match caller.target {
        SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) => {
            world.function(function)?
        }
        _ => return Err(bad("method borrow has no real caller function")),
    };
    let local = match &place.root {
        SemanticOriginalTypeSource::Parameter(ordinal) => *function
            .parameters
            .get(*ordinal as usize)
            .ok_or_else(|| bad("method borrow parameter root is absent"))?,
        source @ (SemanticOriginalTypeSource::Binding(_)
        | SemanticOriginalTypeSource::BindingProjection { .. }) => {
            world.pay(world.program.sites.len())?;
            let mut selected = None;
            let program = world.program;
            for declaration in &program.sites {
                if declaration.instance != caller.id {
                    continue;
                }
                let SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    ..
                } = declaration.action
                else {
                    continue;
                };
                let recipe = world.original_recipe(original, declaration.recipe)?;
                if matches!(&recipe.kind,SemanticSiteRecipeKind::DeclareLocal{expected:SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:actual}} if actual==source)
                {
                    if selected.is_some_and(|previous| previous != local) {
                        return Err(bad("method original binding has ambiguous actual locals"));
                    }
                    selected = Some(local);
                }
            }
            selected.ok_or_else(|| {
                pending("method borrow has no exact initialized original binding declaration")
            })?
        }
        _ => {
            return Err(pending(
                "method borrow root requires its complete original place correspondence",
            ));
        }
    };
    if world.stale_locals.contains(&local)
        || locals
            .get(local.0 as usize)
            .and_then(Option::as_ref)
            .is_none()
    {
        return Err(bad(
            "method borrow has no initialized current original storage shell",
        ));
    }
    crate::dyn_metadata::semantic_operand_snapshot(&place.root, world.limits, world.left, 1)?;
    let root_recipe = world.symbolic(
        caller.id,
        &SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: place.root.clone(),
        },
    )?;
    let mut expected = if matches!(
        &root_recipe,
        SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
    ) && !place.fields.is_empty()
    {
        match world.local_roots.get(&local) {
            Some(roots) => {
                let left = &mut *world.left;
                *left = left
                    .checked_sub(roots.len().saturating_add(1))
                    .ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Types,
                            "method parent root snapshot work exceeded",
                            None,
                        )
                    })?;
                for root in roots {
                    *left = left.checked_sub(root.path.len()).ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Types,
                            "method parent root path work exceeded",
                            None,
                        )
                    })?;
                }
                roots.clone()
            }
            None => {
                return Err(pending(
                    "method address root has no reaching original storage correspondence",
                ));
            }
        }
    } else {
        world.pay(1)?;
        std::collections::BTreeSet::from([arguments::Root {
            local,
            path: Vec::new(),
        }])
    };
    world.pay(
        place
            .fields
            .len()
            .saturating_mul(expected.len().saturating_add(1)),
    )?;
    for (declaration, index) in &place.fields {
        let original_field = world.definition(*declaration)?;
        if !matches!(&original_field.body,SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..} if (*index as usize)<fields.len())
        {
            return Err(bad(
                "method field path leaves its original complete nominal declaration",
            ));
        }
        let mut next = std::collections::BTreeSet::new();
        for mut root in expected {
            world.pay(root.path.len().saturating_add(1))?;
            if root.path.len() >= world.limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "method storage path depth exceeded",
                    None,
                ));
            }
            root.path.push(PlaceProjection::Field(*index));
            next.insert(root);
        }
        expected = next;
    }
    let operand = *arguments
        .get(parameter as usize)
        .ok_or_else(|| bad("method actual receiver argument is absent"))?;
    let actual = world.get(values, operand)?;
    let SemanticTypeRecipe::Address {
        mutable, lifetime, ..
    } = borrow
    else {
        return Err(bad(
            "method borrow receipt has another complete outer constructor",
        ));
    };
    places::borrow_mode(Some(World::type_of(&actual)?), *mutable)?;
    let roots = arguments::roots(world, operand, &actual)?;
    world.pay(expected.len().saturating_mul(roots.len().saturating_add(1)))?;
    if roots.is_empty() || roots != expected {
        return Err(bad(
            "method receiver borrow is not the exact original storage place",
        ));
    }
    for root in &roots {
        world.pay(
            world
                .vacant
                .len()
                .saturating_mul(root.path.len().saturating_add(1)),
        )?;
        if locals
            .get(root.local.0 as usize)
            .and_then(Option::as_ref)
            .is_none()
            || world.vacant.iter().any(|(owner, path)| {
                *owner == root.local
                    && (path.starts_with(&root.path) || root.path.starts_with(path))
            })
        {
            return Err(bad(
                "method receiver borrow has an inactive or vacant actual parent root",
            ));
        }
    }
    let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    }) = lifetime
    else {
        return Ok(());
    };
    let SemanticDefinitionBody::FunctionWithBody {
        sites, site_nodes, ..
    } = &original.body
    else {
        return Err(bad(
            "method borrow has no complete original occurrence directory",
        ));
    };
    world.pay(
        site_nodes.len().saturating_mul(
            sites
                .len()
                .saturating_add(original.implicit_lifetimes.len())
                .saturating_add(1),
        ),
    )?;
    let fresh = site_nodes
        .iter()
        .filter(|row| row.node == SemanticOriginalNode::Expression(node))
        .any(|row| {
            sites.iter().any(|site| {
                site.id == row.site
                    && matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. })
                    && original.implicit_lifetimes.iter().any(|head| {
                        head.ordinal == *ordinal && head.occurrence_path == site.occurrence_path
                    })
            })
        });
    if !fresh {
        return Ok(());
    } // Existing address-parameter roles are never rebound.
    let source = roles::outer(world, &actual)?;
    let roles::Source::Class(actual_image) = &source else {
        return Err(bad(
            "fresh method receiver role has no actual variable borrow occurrence",
        ));
    };
    let image = if matches!(
        actual_image,
        SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(_),
            ..
        }
    ) {
        crate::dyn_metadata::semantic_operand_snapshot(actual_image, world.limits, world.left, 1)?;
        actual_image.clone()
    } else {
        world.pay(function.blocks.len())?;
        let mut producer = None;
        for block in &function.blocks {
            world.pay(block.instructions.len())?;
            for instruction in &block.instructions {
                if instruction.result == Some(operand) {
                    if producer.replace(&instruction.kind).is_some() {
                        return Err(bad("method receiver SSA has multiple actual producers"));
                    }
                }
            }
        }
        if !matches!(
            producer,
            Some(
                InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::BorrowFieldLocal { .. }
                    | InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::BorrowVariantLocal { .. }
                    | InstructionKind::BorrowPlaceAddress { .. }
                    | InstructionKind::BorrowVariantAddress { .. }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::AddressIndex { .. }
                    | InstructionKind::ReborrowAddress { .. }
            )
        ) {
            return Err(bad(
                "fresh method receiver role is not produced by its real actual borrow",
            ));
        }
        let image = world.occurrence(function, operand)?;
        world.pay(world.receiver_adaptations.len().saturating_add(1))?;
        let source = roles::snapshot_source(&source, world.limits, world.left)?;
        crate::dyn_metadata::semantic_operand_snapshot(&image, world.limits, world.left, 1)?;
        world.receiver_adaptations.push(Adaptation {
            caller: caller.id,
            site: site.recipe,
            operand,
            source,
            target: image.clone(),
        });
        image
    };
    world.pay(implicit.len().saturating_add(1))?;
    crate::dyn_metadata::semantic_operand_snapshot(&image, world.limits, world.left, 1)?;
    if let Some(previous) = implicit.insert(*ordinal, image.clone()) {
        if previous != image {
            return Err(bad(
                "one original method borrow selects different actual occurrences",
            ));
        }
    }
    Ok(())
}
