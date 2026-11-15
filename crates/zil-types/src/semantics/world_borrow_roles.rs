//! Directed contextual loans retain their real incoming source. The original
//! outlives theorem selects a role; every actual input/projection/alias/Store
//! operation is authenticated before that role can be observed by consumers.
use super::*;
use crate::semantics::original::{BorrowRole, ViewBorrowRoot, ViewBorrowStepKind};
use zil_base::NodeId;

#[derive(Clone, Copy)]
enum Cursor {
    Value(ValueId),
    Local(LocalId),
}
fn producer<'f>(
    world: &mut World<'_, '_>,
    function: &'f TypedFunction,
    value: ValueId,
) -> R<(&'f InstructionKind, BlockId, usize)> {
    world.pay(function.blocks.len())?;
    let mut result = None;
    for block in &function.blocks {
        world.pay(block.instructions.len())?;
        for (index, operation) in block.instructions.iter().enumerate() {
            if operation.result == Some(value)
                && result.replace((&operation.kind, block.id, index)).is_some()
            {
                return Err(bad("projected loan SSA has multiple actual producers"));
            }
        }
    }
    result.ok_or_else(|| bad("projected loan SSA has no actual producer"))
}
fn transparent(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    values: &[Option<SharedType>],
    mut cursor: Cursor,
) -> R<Cursor> {
    let mut seen = std::collections::BTreeSet::new();
    while let Cursor::Value(value) = cursor {
        world.pay(index::search(seen.len()).saturating_add(1))?;
        if seen.len() >= world.limits.max_call_depth || !seen.insert(value) {
            return Err(bad(
                "projected loan has a cyclic or excessive actual view chain",
            ));
        }
        world.get(values, value)?;
        match producer(world, function, value)?.0 {
            InstructionKind::ReborrowAddress { address, .. }
            | InstructionKind::DescribeAddress { address } => cursor = Cursor::Value(*address),
            _ => break,
        }
    }
    Ok(cursor)
}
fn binding_local(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    binding: NodeId,
) -> R<LocalId> {
    world.pay(world.program.sites.len())?;
    let mut found = None;
    for site in &world.program.sites {
        if site.instance != instance.id {
            continue;
        }
        let SemanticSiteAction::DeclareLocal {
            local: SemanticLocalTarget::Typed(local),
            ..
        } = site.action
        else {
            continue;
        };
        let recipe = world.original_recipe(original, site.recipe)?;
        if matches!(&recipe.kind, SemanticSiteRecipeKind::DeclareLocal {
            expected: SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Binding(node) }
        } if *node == binding)
        {
            if found.is_some_and(|old| old != local) {
                return Err(bad(
                    "projected loan alias has ambiguous actual declarations",
                ));
            }
            found = Some(local);
        }
    }
    found.ok_or_else(|| bad("projected loan alias lacks its exact original declaration"))
}
fn input_nominal(
    world: &mut World<'_, '_>,
    definition: SemanticDefinitionId,
    value: &SemanticTypeValue,
) -> R<()> {
    let declaration = world.definition(definition)?;
    let SemanticDefinitionBody::Nominal { declaration, .. } = declaration.body else {
        return Err(bad(
            "projected loan field does not select an original nominal",
        ));
    };
    let mut ty = World::type_of(value)?;
    while let Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) = ty {
        ty = inner;
    }
    world.pay(world.module.life_instances.len())?;
    let mut found = false;
    for row in &world.module.life_instances {
        if row.declaration != declaration {
            continue;
        }
        inference::charge_type_snapshot(ty, Span::default(), world.limits, world.left, 1)?;
        inference::charge_type_snapshot(&row.ty, Span::default(), world.limits, world.left, 1)?;
        if &row.ty == ty {
            found = true;
        }
    }
    if !found {
        return Err(bad(
            "actual projected loan changes its original nominal field owner",
        ));
    }
    Ok(())
}
fn trace(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    role: &BorrowRole,
    operand: ValueId,
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let mut cursor = Cursor::Value(operand);
    world.pay(role.steps.len())?;
    let mut reached_input = false;
    for step in role.steps.iter().rev() {
        cursor = transparent(world, function, values, cursor)?;
        match &step.kind {
            ViewBorrowStepKind::Field {
                definition,
                ordinal,
            } => {
                let Cursor::Value(value) = cursor else {
                    return Err(bad("projected loan field has no actual SSA projection"));
                };
                let operation = producer(world, function, value)?.0;
                let (next, index) = match operation {
                    InstructionKind::AddressField { address, index } => {
                        (Cursor::Value(*address), *index)
                    }
                    InstructionKind::ExtractField { aggregate, index } => {
                        (Cursor::Value(*aggregate), *index)
                    }
                    InstructionKind::ReadDesc { desc, index } => (Cursor::Value(*desc), *index),
                    InstructionKind::ReadPackageField { local, index }
                    | InstructionKind::DescribePackageField { local, index }
                    | InstructionKind::BorrowPackageField { local, index, .. } => {
                        (Cursor::Local(*local), *index)
                    }
                    _ => {
                        return Err(bad(
                            "projected loan field is not its actual field operation",
                        ));
                    }
                };
                if index != *ordinal {
                    return Err(bad("projected loan selects another original field ordinal"));
                }
                let parent = match next {
                    Cursor::Value(value) => world.get(values, value)?,
                    Cursor::Local(local) => world.local(locals, local)?,
                };
                input_nominal(world, *definition, &parent)?;
                cursor = next;
            }
            ViewBorrowStepKind::StoreBorrow { site, mutable } => {
                let Cursor::Value(value) = cursor else {
                    return Err(bad("projected Store loan has no actual result SSA"));
                };
                let (operation, block, index) = producer(world, function, value)?;
                let store = match operation {
                    InstructionKind::OwnedStoreBorrow { store, .. } if !*mutable => *store,
                    InstructionKind::OwnedStoreBorrowMut { store, .. } if *mutable => *store,
                    _ => {
                        return Err(bad(
                            "projected Store loan changes its real sealed operation or mode",
                        ));
                    }
                };
                world.pay(world.program.sites.len())?;
                if !world.program.sites.iter().any(|row| row.instance == instance.id && row.recipe == *site
                    && matches!(row.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After } if actual == block && instruction as usize == index)
                    && matches!(row.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. })) {
                    return Err(bad("projected Store loan changes its exact original call occurrence"));
                }
                cursor = Cursor::Value(store);
            }
            ViewBorrowStepKind::Alias { binding } => {
                let local = binding_local(world, instance, original, *binding)?;
                world.local(locals, local)?;
                match cursor {
                    Cursor::Value(value) if matches!(producer(world, function, value)?.0, InstructionKind::Load(actual) if *actual == local) =>
                        {}
                    Cursor::Local(actual) if actual == local => {}
                    _ => {
                        return Err(bad(
                            "projected loan alias does not read its real original local",
                        ));
                    }
                }
                let mut incoming = None;
                for block in &function.blocks {
                    world.pay(block.instructions.len())?;
                    for operation in &block.instructions {
                        if let InstructionKind::Store {
                            local: actual,
                            value,
                        } = operation.kind
                            && actual == local
                            && incoming.replace(value).is_some()
                        {
                            return Err(bad(
                                "projected input alias has more than one actual definition",
                            ));
                        }
                    }
                }
                cursor = Cursor::Value(
                    incoming.ok_or_else(|| bad("projected input alias has no real initializer"))?,
                );
            }
            ViewBorrowStepKind::Parameter => {
                let ViewBorrowRoot::Parameter(parameter) = role.root else {
                    return Err(bad("projected loan changes its original root kind"));
                };
                let local = *function
                    .parameters
                    .get(parameter as usize)
                    .ok_or_else(|| bad("projected loan original input is absent"))?;
                match cursor {
                    Cursor::Value(value) if matches!(producer(world, function, value)?.0, InstructionKind::Load(actual) if *actual == local) =>
                        {}
                    Cursor::Local(actual) if actual == local => {}
                    _ => {
                        return Err(bad(
                            "projected loan is not derived from its declared actual input",
                        ));
                    }
                }
                let actual = world.local(locals, local)?;
                let expected = world.parameter_value(
                    instance.id,
                    parameter as usize,
                    World::type_of(&actual)?,
                )?;
                world.same(instance.id, &actual, &expected, implicit)?;
                reached_input = true;
            }
            ViewBorrowStepKind::Local { binding } => {
                if role.root != ViewBorrowRoot::Local(*binding) {
                    return Err(bad("projected loan changes its original local root"));
                }
                let local = binding_local(world, instance, original, *binding)?;
                let actual = world.local(locals, local)?;
                match cursor {
                    Cursor::Local(found) if found == local => {}
                    Cursor::Value(value)
                        if matches!(producer(world, function, value)?.0,
                        InstructionKind::BorrowLocal { local: found } | InstructionKind::BorrowLocalMut { local: found }
                        if *found == local) => {}
                    _ => {
                        return Err(bad(
                            "projected local loan lacks its exact actual storage root",
                        ));
                    }
                }
                let expected = world.ty(
                    instance.id,
                    &SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Binding(*binding),
                    },
                    World::type_of(&actual)?,
                )?;
                world.same(instance.id, &actual, &expected, implicit)?;
                reached_input = true;
            }
            ViewBorrowStepKind::Borrow => {
                // A nested explicit borrow is a real address view. Its current
                // source must still be reached by the following exact steps.
                let Cursor::Value(value) = cursor else {
                    return Err(bad("nested projected borrow has no actual address"));
                };
                let actual = world.get(values, value)?;
                if !matches!(
                    World::type_of(actual.as_ref())?,
                    Type::Address { .. } | Type::Desc(_)
                ) {
                    return Err(bad("nested projected borrow changes its actual carrier"));
                }
            }
        }
    }
    if !reached_input {
        return Err(bad(
            "projected loan omits its complete original input chain",
        ));
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    site: &SemanticSite,
    operand: ValueId,
    actual: &SharedType,
    expected: &SharedType,
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(SharedType, roles::Source)>> {
    if !matches!(World::type_of(actual)?, Type::Address { .. }) {
        return Ok(None);
    }
    {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("projected borrow lacks its complete original Gamma"))?;
        world.pay(index::search(proof.borrow_roles.len()))?;
        if proof
            .borrow_roles
            .range((original.id, NodeId(0))..=(original.id, NodeId(u32::MAX)))
            .next()
            .is_none()
        {
            return Ok(None);
        }
    }
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Ok(None);
    };
    world.pay(site_nodes.len())?;
    let mut nodes = site_nodes.iter().filter(|row| row.site == site.recipe);
    let Some(node) = nodes.next() else {
        return Ok(None);
    };
    if nodes.next().is_some() {
        return Err(bad(
            "projected borrow role has ambiguous original node correspondence",
        ));
    }
    let SemanticOriginalNode::Expression(node) = node.node else {
        return Ok(None);
    };
    let role = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("projected borrow lacks its complete original Gamma"))?;
        world.pay(index::search(proof.borrow_roles.len()))?;
        let Some(role) = proof.borrow_roles.get(&(original.id, node)) else {
            return Ok(None);
        };
        crate::dyn_metadata::semantic_operand_snapshot(
            &role.source_lifetime,
            world.limits,
            world.left,
            1,
        )?;
        crate::dyn_metadata::semantic_operand_snapshot(
            &role.target_lifetime,
            world.limits,
            world.left,
            1,
        )?;
        crate::dyn_metadata::semantic_operand_snapshot(
            &role.occurrence_lifetime,
            world.limits,
            world.left,
            1,
        )?;
        crate::dyn_metadata::semantic_operand_snapshot(&role.inner, world.limits, world.left, 1)?;
        world.pay(role.steps.len().saturating_add(1))?;
        role.clone()
    };
    world.pay(1)?;
    if Rc::ptr_eq(actual, expected) {
        return Ok(None);
    }
    crate::dyn_metadata::semantic_operand_snapshot(actual.as_ref(), world.limits, world.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(expected.as_ref(), world.limits, world.left, 1)?;
    if actual == expected {
        return Ok(None);
    }
    if World::type_of(actual)? != World::type_of(expected)?
        || !matches!(World::type_of(actual)?, Type::Address { .. })
    {
        return Err(bad(
            "projected borrow role changes its complete actual type or access mode",
        ));
    }
    let inner = world.pointee(actual)?;
    let target_inner = world.pointee(expected)?;
    world.same(instance.id, &inner, &target_inner, implicit)?;
    trace(
        world, instance, original, function, &role, operand, values, locals, implicit,
    )?;
    let source = roles::outer(world, actual)?;
    let roots = arguments::roots(world, operand, actual)?;
    for root in &roots {
        world.pay(1)?;
        if world.stale_locals.contains(&root.local)
            || locals
                .get(root.local.0 as usize)
                .and_then(Option::as_ref)
                .is_none()
        {
            return Err(bad(
                "projected borrow role has an inactive or stale actual storage root",
            ));
        }
        world.pay(
            world
                .vacant
                .len()
                .saturating_mul(root.path.len().saturating_add(1)),
        )?;
        if world.vacant.iter().any(|(local, path)| {
            *local == root.local && (path.starts_with(&root.path) || root.path.starts_with(path))
        }) {
            return Err(bad(
                "projected borrow role uses a vacant actual parent projection",
            ));
        }
    }
    Ok(Some((world.share(expected)?, source)))
}
