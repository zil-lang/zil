//! Directed contextual loans retain their real incoming source. The original
//! outlives theorem selects a role; every actual input/projection/alias/Store
//! operation is authenticated before that role can be observed by consumers.
use super::*;
use crate::semantics::original::{BorrowRole, Query, ViewBorrowRoot, ViewBorrowStepKind};
use zil_base::NodeId;

#[derive(Clone, Copy)]
enum Cursor {
    Value(ValueId),
    Local(LocalId),
    FieldPath { value: ValueId, remaining: usize },
}
pub(super) fn producer<'f>(
    world: &mut World<'_, '_>,
    function: &'f TypedFunction,
    value: ValueId,
) -> R<(&'f InstructionKind, BlockId, usize)> {
    let shape = flow::Shape::for_function(world, function)?;
    let (block, ordinal, operation) = shape.producer(world, function, value)?;
    Ok((&operation.kind, block, ordinal))
}
fn transparent(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    values: &ssa::Values,
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ActivationSource {
    Value(ValueId),
    Capture(u32),
}
/// All real definitions reaching this Load, stopped at the nearest write on
/// each CFG predecessor path. A loop backedge is not an arbitrary first store.
fn reaching_values(
    world: &mut World<'_, '_>, function: &TypedFunction,
    local: LocalId, at: (BlockId, usize),
) -> R<Vec<ActivationSource>> {
    reaching_values_inner(world, function, local, at, false)
}
/// Only direct whole-slot stores can replace a local whose storage was never
/// exposed. Loading its pointer value exposes the referent, not the slot.
fn private_pointer_slot(world: &mut World<'_, '_>, function: &TypedFunction, local: LocalId) -> R<bool> {
    if !matches!(function.locals.get(local.0 as usize).map(|row| &row.ty), Some(Type::Address { .. })) {
        return Ok(false);
    }
    for block in &function.blocks {
        world.pay(block.instructions.len().saturating_add(1))?;
        for operation in &block.instructions {
            if matches!(operation.kind, InstructionKind::Load(_) | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_)
                | InstructionKind::TakeLocal { .. }) { continue; }
            for touched in operation.kind.local_operands() {
                world.pay(1)?;
                if touched == local { return Ok(false); }
            }
        }
    }
    Ok(true)
}
fn reaching_values_inner(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    local: LocalId,
    at: (BlockId, usize),
    require_unaliased_writes: bool,
) -> R<Vec<ActivationSource>> {
    let shape = flow::Shape::for_function(world, function)?;
    let mut private = None;
    let mut pending = vec![at];
    let mut seen = std::collections::BTreeSet::new();
    let mut values = std::collections::BTreeSet::new();
    while let Some((block, before)) = pending.pop() {
        world.pay(index::search(seen.len()).saturating_add(1))?;
        if !seen.insert((block, before)) {
            continue;
        }
        if seen.len() > world.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "capture reaching-state node limit exceeded",
                None,
            ));
        }
        let (body, predecessors) = shape.block_and_predecessors(world, function, block)?;
        if before > body.instructions.len() {
            return Err(bad(
                "capture read leaves its exact actual instruction boundary",
            ));
        }
        let mut assigned = None;
        for operation in body.instructions[..before].iter().rev() {
            world.pay(1)?;
            match operation.kind {
                InstructionKind::Store {
                    local: target,
                    value,
                } if target == local => {
                    assigned = Some(ActivationSource::Value(value));
                    break;
                }
                InstructionKind::BindPackageCapture {
                    local: target,
                    index,
                } if target == local => {
                    assigned = Some(ActivationSource::Capture(index));
                    break;
                }
                InstructionKind::StorageLive(target)
                | InstructionKind::StorageDead(target)
                | InstructionKind::TakeLocal { local: target }
                    if target == local =>
                {
                    return Err(bad("capture alias has no initialized reaching occupant"));
                }
                InstructionKind::Constant(_) | InstructionKind::StaticUtf8 { .. }
                | InstructionKind::Load(_) | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
                | InstructionKind::BorrowLocal { .. } | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::ReadCapture { .. } | InstructionKind::ReadAddress { .. }
                | InstructionKind::ReadDesc { .. } | InstructionKind::AddressField { .. }
                | InstructionKind::AddressIndex { .. } | InstructionKind::AddressSlice { .. }
                | InstructionKind::SliceLength { .. } | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. } | InstructionKind::ConvertInteger { .. }
                | InstructionKind::ConvertNumber { .. } | InstructionKind::ExtractField { .. }
                | InstructionKind::ReadPackageField { .. } => {}
                _ if require_unaliased_writes => {
                    if private.is_none() { private = Some(private_pointer_slot(world, function, local)?); }
                    if private != Some(true) {
                        return Err(bad("local returned source crosses an untracked writable operation"));
                    }
                }
                _ => {}
            }
        }
        if let Some(value) = assigned {
            values.insert(value);
            continue;
        }
        world.pay(predecessors.len())?;
        for &predecessor in predecessors {
            let parent = &function.blocks[predecessor];
            pending.push((parent.id, parent.instructions.len()));
        }
        if predecessors.is_empty() {
            return Err(bad(
                "capture alias reaches entry without its actual initialization",
            ));
        }
    }
    if values.is_empty() {
        return Err(bad("capture alias has no real reaching source"));
    }
    Ok(values.into_iter().collect())
}

/// Check the actual affine call preparation from its nearest initialized store.
pub(super) fn prepared_argument_matches(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    argument: ValueId,
    source: ValueId,
) -> R<bool> {
    let (instruction, block, index) = producer(world, function, argument)?;
    let InstructionKind::TakeLocal { local } = instruction else {
        return Ok(false);
    };
    let reaching = reaching_values(world, function, *local, (block, index))?;
    Ok(reaching.as_slice() == [ActivationSource::Value(source)])
}

/// Reconstruct the complete captured value through the actual activation
/// prologue and its exact reaching copies. This is geometry/source transport;
/// it neither resurrects a transferred field nor grants current permission.
#[allow(clippy::too_many_arguments)]
fn capture_source_value(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    value: ValueId,
    capture_index: u32,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
    active: &mut std::collections::BTreeSet<ValueId>,
    depth: usize,
) -> R<SharedType> {
    world.pay(index::search(active.len()).saturating_add(1))?;
    if depth > world.limits.max_call_depth || !active.insert(value) {
        return Err(bad(
            "capture read has a cyclic or excessive actual alias path",
        ));
    }
    let (operation, block, instruction) = producer(world, function, value)?;
    let output = function
        .values
        .get(value.0 as usize)
        .ok_or_else(|| bad("capture alias SSA has no complete actual type"))?;
    let result = match operation {
        InstructionKind::Load(local) => {
            let mut result: Option<SharedType> = None;
            for source in reaching_values(world, function, *local, (block, instruction))? {
                let source = match source {
                    ActivationSource::Value(source) => capture_source_value(
                        world,
                        instance,
                        original,
                        function,
                        source,
                        capture_index,
                        implicit,
                        active,
                        depth + 1,
                    )?,
                    ActivationSource::Capture(index) if index == capture_index => {
                        world.capture_initial(instance, index)?
                    }
                    ActivationSource::Capture(_) => {
                        return Err(bad("activation local selects another actual capture slot"));
                    }
                };
                if let Some(previous) = &result {
                    world.same(instance.id, previous, &source, implicit)?;
                } else {
                    result = Some(source);
                }
            }
            result.ok_or_else(|| bad("capture alias has no complete reaching value"))?
        }
        InstructionKind::ReadCapture { index } if *index == capture_index => {
            let captured = world.capture_initial(instance, *index)?;
            match (World::type_of(&captured)?, output) {
                (source, target)
                    if source == target
                        && (stored::physical(source).is_copy_value()
                            || matches!(
                                stored::physical(source),
                                Type::Address { .. } | Type::Desc(_)
                            )) =>
                {
                    captured
                }
                (
                    Type::Address {
                        mutable: true,
                        inner,
                    },
                    Type::Address {
                        mutable: false,
                        inner: target,
                    },
                ) if inner == target => {
                    let child = world.pointee(&captured)?;
                    world.address_projection(output, &captured, &child)?
                }
                _ => {
                    return Err(bad(
                        "capture alias changes its full Copy/view mode or moves an owning field",
                    ));
                }
            }
        }
        InstructionKind::TakeCapture { index } if *index == capture_index => {
            let closure = function
                .environment
                .ok_or_else(|| bad("capture transfer alias has no actual environment"))?;
            let (definition, child) = world.child_at(closure, instance.id)?;
            if child.id != instance.id
                || !matches!(definition.kind, CallableKind::FnMut | CallableKind::FnOnce)
            {
                return Err(bad(
                    "capture transfer alias lacks an exclusive actual entry",
                ));
            }
            world.capture_initial(instance, *index)?
        }
        InstructionKind::BorrowCapture { index, mutable } if *index == capture_index => {
            let closure = function
                .environment
                .ok_or_else(|| bad("capture borrow alias has no actual environment"))?;
            let (definition, child) = world.child_at(closure, instance.id)?;
            if child.id != instance.id || *mutable && definition.kind == CallableKind::Fn {
                return Err(bad(
                    "capture borrow alias strengthens the actual receiver mode",
                ));
            }
            if !matches!(output, Type::Address { mutable: actual, .. } if actual == mutable) {
                return Err(bad("capture borrow alias changes its actual outer access"));
            }
            let captured = world.capture_initial(instance, *index)?;
            let image = world.occurrence(function, value)?;
            world.address(output, image, &captured)?
        }
        InstructionKind::DescribeCapture { index } if *index == capture_index => {
            let captured = world.capture_initial(instance, *index)?;
            if !matches!(World::type_of(&captured)?, Type::Managed(_)) {
                return Err(bad(
                    "capture description alias has no original owning field",
                ));
            }
            let payload = projection::pointee(world, &captured)?;
            let image = world.occurrence(function, value)?;
            world.address(output, image, &payload)?
        }
        InstructionKind::ReadAddress { address } => {
            let source = capture_source_value(
                world,
                instance,
                original,
                function,
                *address,
                capture_index,
                implicit,
                active,
                depth + 1,
            )?;
            world.pointee(&source)?
        }
        InstructionKind::DescribeAddress { address } => {
            let source = capture_source_value(
                world,
                instance,
                original,
                function,
                *address,
                capture_index,
                implicit,
                active,
                depth + 1,
            )?;
            places::address_mutable(world, &source, false)?;
            let owner = world.pointee(&source)?;
            if !matches!(World::type_of(&owner)?, Type::Managed(_)) {
                return Err(bad(
                    "capture description alias has no reached owning carrier",
                ));
            }
            let payload = projection::pointee(world, &owner)?;
            let image = world.occurrence(function, value)?;
            world.address(output, image, &payload)?
        }
        InstructionKind::ReborrowAddress { address, mutable } => {
            let source = capture_source_value(
                world,
                instance,
                original,
                function,
                *address,
                capture_index,
                implicit,
                active,
                depth + 1,
            )?;
            places::address_mutable(world, &source, *mutable)?;
            let payload = world.pointee(&source)?;
            let image = world.occurrence(function, value)?;
            world.address(output, image, &payload)?
        }
        _ => {
            return Err(bad(
                "capture loan is not its exact actual environment operation",
            ));
        }
    };
    inference::charge_type_snapshot(output, Span::default(), world.limits, world.left, 1)?;
    inference::charge_type_snapshot(
        World::type_of(&result)?,
        Span::default(),
        world.limits,
        world.left,
        1,
    )?;
    if World::type_of(&result)? != output {
        return Err(bad(
            "capture alias changes its complete actual physical type",
        ));
    }
    active.remove(&value);
    Ok(result)
}

fn binding_local(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    binding: NodeId,
    observed: NodeId,
) -> R<LocalId> {
    binding_source_local(
        world,
        instance,
        original,
        &SemanticOriginalTypeSource::Binding(binding),
        observed,
    )
}
fn binding_source_local(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    source: &SemanticOriginalTypeSource,
    observed: NodeId,
) -> R<LocalId> {
    let sites = world.instance_sites(instance.id)?;
    let mut found = None;
    for &site in &sites {
        let SemanticSiteAction::DeclareLocal {
            local: SemanticLocalTarget::Typed(local),
            ..
        } = site.action
        else {
            continue;
        };
        let recipe = world.original_recipe(original, site.recipe)?;
        if matches!(&recipe.kind, SemanticSiteRecipeKind::DeclareLocal {
            expected: SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: actual }
        } if actual == source)
        {
            if found.is_some_and(|old| old != local) {
                return Err(bad(
                    "projected loan alias has ambiguous actual declarations",
                ));
            }
            found = Some(local);
        }
    }
    if let Some(local) = found {
        return Ok(local);
    }
    // Pattern payloads do not have an ordinary Binding statement mint. Use
    // this exact original Name observation and its emitted local selector;
    // never search for another local of an equal type.
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("loan binding lost its original lexical proof"))?;
    world.pay(index::search(proof.borrow_bindings.len()))?;
    if proof.borrow_bindings.get(&(original.id, observed)) != Some(source) {
        return Err(bad(
            "loan observation does not name its exact original declaration",
        ));
    }
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = instance.target
    else {
        return Err(bad("loan binding has no actual function"));
    };
    let function = world.function(function)?;
    for site in sites {
        if occurrences::node(world, original, site)? != Some(observed) {
            continue;
        }
        let recipe = world.original_recipe(original, site.recipe)?;
        if !matches!(&recipe.kind, SemanticSiteRecipeKind::TypeUse { expected: SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Expression(actual) } } if *actual == observed)
        {
            continue;
        }
        let SemanticSiteAction::TypeUse { subject, .. } = &site.action else {
            continue;
        };
        let local = match subject {
            SemanticOperand::Local(local) => Some(*local),
            SemanticOperand::Value(value) => match producer(world, function, *value)?.0 {
                InstructionKind::Load(local)
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::DescribeLocal { local } => Some(*local),
                _ => None,
            },
            _ => None,
        };
        if let Some(local) = local {
            if found.is_some_and(|old| old != local) {
                return Err(bad(
                    "one original binding observation selects several actual locals",
                ));
            }
            found = Some(local);
        }
    }
    found.ok_or_else(|| {
        bad("projected loan alias lacks its exact original declaration or emitted binding read")
            .with_note(format!("original definition {:?}, observation {:?}, source {:?}, actual function {:?}", original.id, observed, source, function.id))
    })
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
#[allow(clippy::too_many_arguments)]
fn checked_call_source(
    world: &mut World<'_, '_>, instance: &SemanticInstance, original: &SemanticDefinition,
    node: NodeId, site: SemanticSiteId, block: BlockId, index: usize,
) -> R<()> {
    let recipe = world.original_recipe(original, site)?;
    if !matches!(recipe.kind, SemanticSiteRecipeKind::OriginalCall) {
        return Err(bad("returned source is not its original Call occurrence"));
    }
    let mut candidates = world.instance_sites(instance.id)?.into_iter().filter(|row| row.recipe == site
        && matches!(row.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After }
            if actual == block && instruction as usize == index)
        && matches!(row.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. }
            | SemanticSiteAction::CallableCall { .. }));
    let selected = candidates.next().ok_or_else(|| bad("returned source has no exact actual original Call"))?;
    if candidates.next().is_some() {
        return Err(bad("returned source repeats its exact actual original Call"));
    }
    if occurrences::node(world, original, selected)? != Some(node)
        || occurrences::site_value(world, instance, original, selected)?.is_none()
    { return Err(bad("returned source changes its exact actual expression occurrence")); }
    Ok(())

}
/// A non-address original callee (for example a returned temporary) has no
/// projected-pointer view theorem. Its existing value/prepared-place receipts
/// still identify the exact live whole environment used by this call.
#[allow(clippy::too_many_arguments)]
fn whole_receiver_source(
    world: &mut World<'_, '_>, instance: &SemanticInstance, original: &SemanticDefinition,
    function: &TypedFunction, source: NodeId, site: SemanticSiteId,
    at: (BlockId, usize), receiver: Cursor, values: &ssa::Values,
    locals: &[Option<SharedType>], implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    if let Cursor::Value(address) = receiver {
        let mut calls = world.instance_sites(instance.id)?.into_iter().filter(|row| row.recipe == site
            && matches!(row.at, SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::After }
                if block == at.0 && instruction as usize == at.1));
        let call = calls.next().ok_or_else(|| bad("whole receiver has no exact original call occurrence"))?;
        if calls.next().is_some() { return Err(bad("whole receiver repeats its actual call occurrence")); }
        if occurrences::prepared_callee(world, instance, original, call, source)? == Some(address) {
            let raw = world.get(values, address)?;
            let value = world.pointee(&raw)?;
            let expected = world.physical_type(instance.id, &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Expression(source),
            }, World::type_of(&value)?)?;
            return world.same(instance.id, &value, &expected, implicit);
        }
    }
    let source_value = occurrences::expression_value(world, instance, original, source)?
        .ok_or_else(|| bad("whole temporary receiver lost its real original value occurrence"))?;
    let expected = world.get(values, source_value)?;
    if !matches!(World::type_of(&expected)?, Type::Closure(_)) {
        return Err(bad("whole receiver source is not a complete callable environment"));
    }
    let Cursor::Local(local) = receiver else {
        return Err(bad("whole temporary receiver has no exact staged local place"));
    };
    let actual = world.local(locals, local)?;
    for incoming in reaching_values(world, function, local, at)? {
        if incoming != ActivationSource::Value(source_value) {
            return Err(bad("temporary receiver place selects another original whole value"));
        }
    }
    world.same(instance.id, &actual, &expected, implicit)
}

fn trace(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    role: &BorrowRole,
    operand: ValueId,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
    return_boundary: bool,
) -> R<()> {
    trace_steps(world, instance, original, function, role.root, &role.steps,
        Cursor::Value(operand), values, locals, implicit, return_boundary, 0)
}
#[allow(clippy::too_many_arguments)]
fn trace_steps(
    world: &mut World<'_, '_>, instance: &SemanticInstance,
    original: &SemanticDefinition, function: &TypedFunction,
    root: ViewBorrowRoot, steps: &[crate::semantics::original::ViewBorrowStep],
    mut cursor: Cursor, values: &ssa::Values, locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>, return_boundary: bool, depth: usize,
) -> R<()> {
    if depth > world.limits.max_call_depth {
        return Err(Diagnostic::limit(Phase::Types, "actual returned source depth exceeded", None));
    }
    world.pay(1)?;
    if return_boundary {
        world.pay(steps.len())?;
    }
    let input_referent = return_boundary
        && matches!(root, ViewBorrowRoot::Parameter(_))
        && steps.iter()
            .any(|step| matches!(step.kind, ViewBorrowStepKind::Parameter));
    world.pay(steps.len())?;
    let mut reached_input = false;
    let mut through_store = false;
    for step in steps.iter().rev() {
        cursor = transparent(world, function, values, cursor)?;
        match &step.kind {
            ViewBorrowStepKind::Dereference => {
                if let Cursor::Value(value) = cursor {
                    if let InstructionKind::ReadAddress { address } =
                        producer(world, function, value)?.0
                    {
                        cursor = Cursor::Value(*address);
                    }
                }
            }
            ViewBorrowStepKind::CallInputs { site, inputs } => {
                let Cursor::Value(value) = cursor else { return Err(bad("returned inputs have no actual Call result SSA")); };
                if root != ViewBorrowRoot::Result(step.node) { return Err(bad("returned inputs change their original call root")); }
                let (operation, block, index) = producer(world, function, value)?;
                checked_call_source(world, instance, original, step.node, *site, block, index)?;
                let (callee, arguments) = match operation {
                    InstructionKind::Call { function, arguments }
                    | InstructionKind::SuspendCall { function, arguments } => (*function, arguments),
                    _ => return Err(bad("returned input theorem is not its actual ordinary call")),
                };
                let ledger = world.ledger;
                let proof = ledger.original.borrow();
                let proof = proof.as_ref().ok_or_else(|| bad("returned input lacks complete original Gamma"))?;
                world.pay(index::search(proof.call_targets.len()).saturating_add(world.program.instances.len()))?;
                let target = proof.call_targets.get(&(original.id, *site))
                    .ok_or_else(|| bad("returned input call has no original target"))?;
                if !world.program.instances.iter().any(|row| row.definition == *target
                    && row.target == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(callee)))
                { return Err(bad("returned input call changes its original actual callee")); }
                if inputs.is_empty() { return Err(bad("returned input theorem has an empty source domain")); }
                world.pay(inputs.len().saturating_mul(2))?;
                let mut selected = std::collections::BTreeSet::new();
                for (parameter, node) in inputs {
                    world.pay(index::search(selected.len()).saturating_add(1))?;
                    if !selected.insert(*parameter) { return Err(bad("returned input theorem repeats a parameter occurrence")); }
                    let value = *arguments.get(*parameter as usize)
                        .ok_or_else(|| bad("returned input leaves its actual complete argument vector"))?;
                    world.get(values, value)?;
                    let (source, steps) = proof.view_source_steps(original.id, *node, world.limits, world.left)?
                        .ok_or_else(|| bad("returned argument omits its original source chain"))?;
                    trace_steps(world, instance, original, function, source, &steps, Cursor::Value(value),
                        values, locals, implicit, return_boundary, depth + 1)?;
                }
                reached_input = true;
            }
            ViewBorrowStepKind::CallableReceiver { site, source } => {
                let Cursor::Value(value) = cursor else { return Err(bad("receiver result has no actual Call SSA")); };
                if root != ViewBorrowRoot::Result(step.node) { return Err(bad("receiver result changes its original call root")); }
                let (operation, block, index) = producer(world, function, value)?;
                checked_call_source(world, instance, original, step.node, *site, block, index)?;
                let receiver = match operation {
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => Cursor::Local(*callee),
                    InstructionKind::CallClosureBorrowed { callee, kind, .. }
                        if matches!(kind, CallableKind::Fn | CallableKind::FnMut) => Cursor::Value(*callee),
                    _ => return Err(bad("receiver return theorem changes its actual non-consuming operation")),
                };
                let ledger = world.ledger;
                let proof = ledger.original.borrow();
                let proof = proof.as_ref().ok_or_else(|| bad("returned receiver lacks complete original Gamma"))?;
                world.pay(index::search(proof.callable_calls.len()))?;
                let call = proof.callable_calls.get(&(original.id, *site))
                    .ok_or_else(|| bad("returned receiver has no original callable operation"))?;
                if call.callee_node != *source || !matches!(call.kind, CallableKind::Fn | CallableKind::FnMut)
                    || call.signature.call_kind != CallKind::Direct
                { return Err(bad("returned receiver changes its exact original operand or mode")); }
                if let Some((root, steps)) = proof.view_source_steps(original.id, *source, world.limits, world.left)? {
                    trace_steps(world, instance, original, function, root, &steps, receiver,
                        values, locals, implicit, return_boundary, depth + 1)?;
                } else {
                    whole_receiver_source(world, instance, original, function, *source, *site,
                        (block, index), receiver, values, locals, implicit)?;
                }
                reached_input = true;
            }
            ViewBorrowStepKind::BytesBorrow { site, mutable } => {
                let Cursor::Value(value) = cursor else { return Err(bad("byte view has no actual produced SSA")); };
                let (operation, block, index) = producer(world, function, value)?;
                checked_call_source(world, instance, original, step.node, *site, block, index)?;
                let InstructionKind::ByteBufferBorrow { buffer, mutable: actual } = operation else {
                    return Err(bad("byte view changes its actual buffer-borrow operation"));
                };
                if actual != mutable { return Err(bad("byte view changes its actual access permission")); }
                let ledger = world.ledger;
                let proof = ledger.original.borrow();
                let proof = proof.as_ref().ok_or_else(|| bad("byte view lacks complete original Gamma"))?;
                world.pay(index::search(proof.intrinsics.len()))?;
                if !matches!(proof.intrinsics.get(&(original.id, *site)), Some(call)
                    if matches!(call.kind, super::super::original::IntrinsicKind::BytesBorrow { mutable: requested } if requested == *mutable))
                { return Err(bad("byte view is not its exact checked original intrinsic")); }
                world.get(values, *buffer)?;
                cursor = Cursor::Value(*buffer);
            }
            ViewBorrowStepKind::CallResult { site } => {
                let Cursor::Value(value) = cursor else {
                    return Err(bad("call result loan has no real produced SSA"));
                };
                if root != ViewBorrowRoot::Result(step.node) {
                    return Err(bad("call result loan changes its original root expression"));
                }
                let (_, block, index) = producer(world, function, value)?;
                checked_call_source(world, instance, original, step.node, *site, block, index)?;
                // This value was produced by the independently checked actual
                // invocation. Never select one input as an alias for its result.
                world.get(values, value)?;
                reached_input = true;
            }
            ViewBorrowStepKind::Capture { name } => {
                if !matches!(root, ViewBorrowRoot::Capture(_)) {
                    return Err(bad("capture loan changes its original source kind"));
                }
                let environment = instance
                    .environment
                    .as_ref()
                    .ok_or_else(|| bad("capture loan has no actual environment"))?;
                let SemanticClosureTarget::Typed(closure) = environment.closure else {
                    return Err(bad("capture loan retains a Core environment"));
                };
                let ledger = world.ledger;
                let receipts = ledger
                    .closure_captures
                    .get(&closure)
                    .ok_or_else(|| bad("capture loan has no actual creator inputs"))?;
                let mut selected = None;
                for (index, (candidate, _)) in receipts.iter().enumerate() {
                    world.pay(name.len().saturating_add(candidate.len()).saturating_add(1))?;
                    if candidate == name && selected.replace(index as u32).is_some() {
                        return Err(bad("capture loan repeats its exact creator name"));
                    }
                }
                let index = selected
                    .ok_or_else(|| bad("capture loan has no exact original creator source"))?;
                match cursor {
                    Cursor::FieldPath { .. } => {
                        return Err(bad("capture loan omits part of its stored field path"));
                    }
                    Cursor::Value(value) => {
                        let reconstructed = capture_source_value(
                            world,
                            instance,
                            original,
                            function,
                            value,
                            index,
                            implicit,
                            &mut std::collections::BTreeSet::new(),
                            0,
                        )?;
                        let actual = world.get(values, value)?;
                        world.same(instance.id, &actual, &reconstructed, implicit)?;
                    }
                    Cursor::Local(local) => {
                        let Some((_, current_function, block, instruction, _)) =
                            world.current_point
                        else {
                            return Err(bad("capture local has no exact current CFG point"));
                        };
                        if current_function != function.id {
                            return Err(bad("capture local is queried in another actual function"));
                        }
                        let captured = world.capture_initial(instance, index)?;
                        let actual = world.local(locals, local)?;
                        world.same(instance.id, &actual, &captured, implicit)?;
                        for source in reaching_values(world, function, local, (block, instruction))?
                        {
                            match source {
                                ActivationSource::Capture(actual) if actual == index => {}
                                ActivationSource::Value(value) => {
                                    let source = capture_source_value(
                                        world,
                                        instance,
                                        original,
                                        function,
                                        value,
                                        index,
                                        implicit,
                                        &mut std::collections::BTreeSet::new(),
                                        0,
                                    )?;
                                    world.same(instance.id, &source, &captured, implicit)?;
                                }
                                _ => {
                                    return Err(bad(
                                        "capture local is not bound to its exact actual environment field",
                                    ));
                                }
                            }
                        }
                    }
                }
                reached_input = true;
            }
            ViewBorrowStepKind::Slice | ViewBorrowStepKind::Index { .. } => {
                let Cursor::Value(value) = cursor else {
                    return Err(bad("projected array loan has no actual SSA projection"));
                };
                if let (
                    ViewBorrowStepKind::Index {
                        constant: Some(expected),
                    },
                    InstructionKind::BorrowOwnedArrayElement { local, index, .. },
                ) = (&step.kind, producer(world, function, value)?.0)
                {
                    if index != expected {
                        return Err(bad("owning array loan changes its exact original index"));
                    }
                    cursor = Cursor::Local(*local);
                    continue;
                }
                let parent = match (&step.kind, producer(world, function, value)?.0) {
                    (ViewBorrowStepKind::Slice, InstructionKind::AddressSlice { address, .. })
                    | (
                        ViewBorrowStepKind::Index { .. },
                        InstructionKind::AddressIndex { address, .. },
                    ) => *address,
                    _ => {
                        return Err(bad(
                            "projected array loan changes its real slice/index operation",
                        ));
                    }
                };
                world.get(values, parent)?;
                cursor = Cursor::Value(parent);
            }
            ViewBorrowStepKind::ProjectedLocal {
                statement,
                pattern,
                read,
            } => {
                if root != ViewBorrowRoot::Local(*statement) {
                    return Err(bad("pattern loan changes its original binding root"));
                }
                world.pay(pattern.len().saturating_add(1))?;
                let source = SemanticOriginalTypeSource::BindingProjection {
                    statement: *statement,
                    pattern: pattern.clone(),
                };
                let local = binding_source_local(world, instance, original, &source, step.node)?;
                let actual = world.local(locals, local)?;
                let valid = match cursor {
                    Cursor::FieldPath { .. } => false,
                    Cursor::Local(found) => !*read && found == local,
                    Cursor::Value(value) => match producer(world, function, value)?.0 {
                        InstructionKind::Load(found) => *read && *found == local,
                        InstructionKind::BorrowLocal { local: found }
                        | InstructionKind::BorrowLocalMut { local: found }
                        | InstructionKind::DescribeLocal { local: found } => {
                            !*read && *found == local
                        }
                        _ => false,
                    },
                };
                if !valid {
                    return Err(bad(
                        "pattern loan does not reach its complete actual projected declaration",
                    ));
                }
                let expected = world.physical_type(
                    instance.id,
                    &SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source,
                    },
                    World::type_of(&actual)?,
                )?;
                world.same(instance.id, &actual, &expected, implicit)?;
                reached_input = true;
            }
            ViewBorrowStepKind::Field {
                definition,
                ordinal,
            } => {
                let (value, remaining) = match cursor {
                    Cursor::Value(value) => (value, None),
                    Cursor::FieldPath { value, remaining } => (value, Some(remaining)),
                    Cursor::Local(_) => {
                        return Err(bad("projected loan field has no actual SSA projection"));
                    }
                };
                let operation = producer(world, function, value)?.0;
                // A single instruction can encode several source selectors.
                // Consume one actual ordinal per source step and finish at its
                // actual local or address. Only a stored read unwraps a slot.
                enum FieldAccess {
                    Address(bool),
                    Stored(StoredBorrowLoad),
                }
                let path_source = match operation {
                    InstructionKind::BorrowPlaceLocal { local, path, mutable } => {
                        let holder = world.local(locals, *local)?;
                        Some((path, Cursor::Local(*local), holder, FieldAccess::Address(*mutable)))
                    }
                    InstructionKind::BorrowPlaceAddress { address, path, mutable } => {
                        let address_type = world.get(values, *address)?;
                        let holder = world.pointee(&address_type)?;
                        Some((path, Cursor::Value(*address), holder, FieldAccess::Address(*mutable)))
                    }
                    InstructionKind::LoadStoredBorrow { storage, path, mode } => {
                        let storage_type = world.get(values, *storage)?;
                        let holder = world.pointee(&storage_type)?;
                        Some((path, Cursor::Value(*storage), holder, FieldAccess::Stored(*mode)))
                    }
                    _ => None,
                };
                if let Some((path, base, payload, access)) = path_source {
                    let count = remaining.unwrap_or(path.len());
                    if count == 0
                        || count > path.len()
                        || path.get(count - 1) != Some(&PlaceProjection::Field(*ordinal))
                    {
                        return Err(bad(
                            "stored field loan changes its complete original projection path",
                        ));
                    }
                    let parent = places::path(world, &payload, &path[..count - 1])?;
                    input_nominal(world, *definition, &parent)?;
                    if remaining.is_none() {
                        let selected = projection::place(world, &parent, path[count - 1])?;
                        let actual = world.get(values, value)?;
                        match (access, World::type_of(&selected)?) {
                            (FieldAccess::Address(mutable), _) => {
                                places::borrow_mode(Some(World::type_of(&actual)?), mutable)?;
                                let pointee = world.pointee(&actual)?;
                                world.same(instance.id, &pointee, &selected, implicit)?;
                            }
                            (FieldAccess::Stored(StoredBorrowLoad::CopyShared), Type::Address { mutable: false, .. })
                            | (FieldAccess::Stored(StoredBorrowLoad::CopyDescription), Type::Desc(_)) => {
                                world.same(instance.id, &actual, &selected, implicit)?;
                            }
                            (FieldAccess::Stored(StoredBorrowLoad::Reborrow { mutable }),
                                Type::Address { mutable: available, .. }) if !mutable || *available => {
                                places::borrow_mode(Some(World::type_of(&actual)?), mutable)?;
                                let source = world.pointee(&selected)?;
                                let actual = world.pointee(&actual)?;
                                world.same(instance.id, &source, &actual, implicit)?;
                            }
                            _ => return Err(bad("stored field loan changes its real read or reborrow authority")),
                        }
                    }
                    cursor = if count == 1 {
                        base
                    } else {
                        Cursor::FieldPath {
                            value,
                            remaining: count - 1,
                        }
                    };
                    continue;
                }
                if remaining.is_some() {
                    return Err(bad(
                        "stored field loan loses its actual field-path producer",
                    ));
                }
                // Reading an address-valued retained field is a real read of
                // its field address, not an equality with the receiver head.
                // Authenticate the complete pointee before following that
                // exact AddressField producer back to the receiver loan.
                let operation = if let InstructionKind::ReadAddress { address } = operation {
                    let field_address = world.get(values, *address)?;
                    let field_value = world.get(values, value)?;
                    let pointee = world.pointee(&field_address)?;
                    world.same(instance.id, &field_value, &pointee, implicit)?;
                    producer(world, function, *address)?.0
                } else {
                    operation
                };
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
                    Cursor::FieldPath { .. } => {
                        return Err(bad(
                            "ordinary field operation invents a stored projection cursor",
                        ));
                    }
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
                if !world.instance_sites(instance.id)?.into_iter().any(|row| row.recipe == *site
                    && matches!(row.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After } if actual == block && instruction as usize == index)
                    && matches!(row.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. })) {
                    return Err(bad("projected Store loan changes its exact original call occurrence"));
                }
                cursor = Cursor::Value(store);
                through_store = true;
            }
            ViewBorrowStepKind::PreparedShared => {
                let Cursor::Value(value) = cursor else {
                    return Err(bad("prepared shared loan has no actual value"));
                };
                let (operation, block, at) = producer(world, function, value)?;
                let InstructionKind::Load(local) = operation else {
                    return Err(bad("prepared shared loan must read its temporary"));
                };
                let local_ty = &function.locals.get(local.0 as usize)
                    .ok_or_else(|| bad("prepared shared loan has no actual local"))?.ty;
                if !matches!(local_ty, Type::Address { mutable: false, .. })
                    || !private_pointer_slot(world, function, *local)?
                {
                    return Err(bad("prepared shared loan lacks private shared address storage"));
                }
                let sources = reaching_values(world, function, *local, (block, at))?;
                let [ActivationSource::Value(source)] = sources.as_slice() else {
                    return Err(bad("prepared shared loan must have one initialized reaching value"));
                };
                let (source_operation, _, _) = producer(world, function, *source)?;
                let InstructionKind::ReborrowAddress { address, mutable: false } = source_operation
                else {
                    return Err(bad("prepared shared loan changes its actual source reborrow"));
                };
                let actual = world.get(values, value)?;
                let stored = world.get(values, *source)?;
                world.same(instance.id, &actual, &stored, implicit)?;
                cursor = Cursor::Value(*address);
            }
            ViewBorrowStepKind::Alias { binding } => {
                let local = binding_local(world, instance, original, *binding, step.node)?;
                // An actual load or move reads the previous occupant. At a
                // later reborrow the pointer cell may already be empty.
                let observed = match cursor {
                    Cursor::Value(value) => {
                        let (operation, block, index) = producer(world, function, value)?;
                        if !matches!(operation, InstructionKind::Load(found) | InstructionKind::TakeLocal { local: found } if *found == local) {
                            return Err(bad("projected loan alias does not read its real original local"));
                        }
                        world.get(values, value)?;
                        Some((block, index))
                    }
                    Cursor::Local(found) if found == local => {
                        if !return_boundary { world.local(locals, local)?; }
                        None
                    }
                    _ => return Err(bad("projected loan alias does not read its real original local")),
                };
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
                let incoming = incoming.ok_or_else(|| bad("projected input alias has no real initializer"))?;
                if let Some(at) = observed {
                    let reaching = reaching_values(world, function, local, at)?;
                    if reaching.as_slice() != [ActivationSource::Value(incoming)] {
                        return Err(bad("projected input alias read changes its original initializer"));
                    }
                }
                cursor = Cursor::Value(incoming);
            }
            ViewBorrowStepKind::ReadLocal { binding } => {
                if root != ViewBorrowRoot::Local(*binding) {
                    return Err(bad(
                        "call loan read changes its original mutable local root",
                    ));
                }
                let local = binding_local(world, instance, original, *binding, step.node)?;
                let Cursor::Value(value) = cursor else {
                    return Err(bad("call loan does not read its real reaching mutable local"));
                };
                let (operation, block, instruction) = producer(world, function, value)?;
                if !matches!(operation, InstructionKind::Load(found) if *found == local) {
                    return Err(bad("call loan does not read its real reaching mutable local"));
                }
                // At an exit, the pointer cell may already be dead; the
                // previously checked Load still carries the referent loan.
                let actual = if return_boundary { world.get(values, value)? }
                    else { world.local(locals, local)? };
                local_read_sources(world, instance, original, function, *binding,step.node,
                    local, value, (block, instruction))?;
                let expected = world.physical_type(
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
            ViewBorrowStepKind::Parameter | ViewBorrowStepKind::ParameterStorage => {
                let ViewBorrowRoot::Parameter(parameter) = root else {
                    return Err(bad("projected loan changes its original root kind"));
                };
                let local = *function
                    .parameters
                    .get(parameter as usize)
                    .ok_or_else(|| bad("projected loan original input is absent"))?;
                match cursor {
                    Cursor::Value(value)
                        if matches!(&step.kind, ViewBorrowStepKind::Parameter)
                            && matches!(producer(world, function, value)?.0, InstructionKind::Load(actual) if *actual == local) =>
                        {}
                    Cursor::Value(value)
                        if matches!(&step.kind, ViewBorrowStepKind::ParameterStorage)
                            && matches!(producer(world, function, value)?.0, InstructionKind::BorrowLocal { local: actual } | InstructionKind::BorrowLocalMut { local: actual } | InstructionKind::DescribeLocal { local: actual } if *actual == local) =>
                        {}
                    Cursor::Local(actual) if actual == local => {}
                    _ => {
                        return Err(bad(
                            "projected loan is not derived from its declared actual input",
                        ));
                    }
                }
                let actual = if input_referent && matches!(step.kind, ViewBorrowStepKind::Parameter)
                {
                    if let Cursor::Value(value) = cursor {
                        world.get(values, value)?
                    } else {
                        world.local(locals, local)?
                    }
                } else {
                    world.local(locals, local)?
                };
                let expected = world.parameter_value(
                    instance.id,
                    parameter as usize,
                    World::type_of(&actual)?,
                )?;
                world.same(instance.id, &actual, &expected, implicit)?;
                reached_input = true;
            }
            ViewBorrowStepKind::Local { binding } => {
                if root != ViewBorrowRoot::Local(*binding) {
                    return Err(bad("projected loan changes its original local root"));
                }
                let local = binding_local(world, instance, original, *binding, step.node)?;
                let actual = world.local(locals, local)?;
                match cursor {
                    Cursor::Local(found) if found == local => {}
                    Cursor::Value(value)
                        if matches!(producer(world, function, value)?.0,
                        InstructionKind::BorrowLocal { local: found } | InstructionKind::BorrowLocalMut { local: found } | InstructionKind::DescribeLocal { local: found }
                        if *found == local) => {}
                    Cursor::Value(value)
                        if through_store
                            && matches!(producer(world, function, value)?.0, InstructionKind::Load(found) if *found == local) =>
                        {}
                    _ => {
                        return Err(bad(
                            "projected local loan lacks its exact actual storage root",
                        ));
                    }
                }
                let expected = world.physical_type(
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
/// Consume an original creator/child write theorem at this exact actual
/// WriteCapture. Only the outer bound is viewed as the persistent slot's bound;
/// actual source identity and the complete pointee are proved independently.
#[allow(clippy::too_many_arguments)]
pub(super) fn capture_write(
    world: &mut World<'_, '_>, instance: &SemanticInstance,
    original: &SemanticDefinition, function: &TypedFunction,
    capture: u32, operand: ValueId, actual: &SharedType, expected: &SharedType,
    values: &ssa::Values, locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<bool> {
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof.as_ref().ok_or_else(|| bad("capture write has no complete original Gamma"))?;
    world.pay(index::search(proof.capture_writes.len()))?;
    if proof.capture_writes.range((original.id, NodeId(0))..=(original.id, NodeId(u32::MAX))).next().is_none() {
        return Ok(false);
    }
    let ordinal = super::super::headers::original_capture_ordinal(world.program, &world.view,
        ledger, instance, capture, world.limits, world.left)?;
    let mut selected = None;
    for ((_, node), write) in proof.capture_writes.range((original.id, NodeId(0))..=(original.id, NodeId(u32::MAX))) {
        world.pay(1)?;
        if write.environment != ordinal { continue; }
        if occurrences::expression_value(world, instance, original, *node)? != Some(operand) { continue; }
        if selected.replace(write).is_some() { return Err(bad("actual capture write has multiple original source occurrences")); }
    }
    let Some(write) = selected else { return Ok(false); };
    world.pay(index::search(proof.local_roles.len()))?;
    if !proof.local_roles.contains_key(&(write.creator_role_owner, write.creator_binding)) {
        return Err(bad("captured write loses its immutable creator declaration role"));
    }
    crate::dyn_metadata::semantic_operand_snapshot(actual.as_ref(), world.limits, world.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(expected.as_ref(), world.limits, world.left, 1)?;
    if World::type_of(actual)? != World::type_of(expected)? {
        return Err(bad("captured role write changes its complete actual pointer type"));
    }
    // A source trace is mandatory even when the outer images already agree.
    trace(world, instance, original, function, &write.role, operand, values, locals, implicit, false)?;
    // The capture header names a sealed Type coordinate and the original
    // write can name its Life slot. Compare both in the same original Gamma.
    let expected = world.normalize(instance.id, expected, implicit)?;
    let actual = world.normalize(instance.id, actual, implicit)?;
    let target = role_target(world, instance, &write.role, implicit)?;
    if roles::outer(world, &expected)? != target {
        return Err(bad("captured role write changes its exact persistent environment bound"));
    }
    if apply_role(world, instance, original, function, &write.role, operand, &actual, &expected,
        values, locals, implicit, false)?.is_none() {
        world.same(instance.id, &actual, &expected, implicit)?;
    }
    Ok(true)
}

/// A sealed call can carry the original contextual-loan theorem itself; an
/// extra TypeUse read is not required to manufacture the same source boundary.
/// Run after its real result SSA exists so the ordinary trace still validates
/// every actual input, field, Store borrow, alias and live root.
#[allow(clippy::too_many_arguments)]
pub(super) fn call_result(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    site: &SemanticSite,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(ValueId, SharedType, roles::Source)>> {
    {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("call loan has no complete original Gamma"))?;
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
    let Some(node) = occurrences::node(world, original, site)? else { return Ok(None); };
    {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("call loan lost its complete original Gamma"))?;
        world.pay(index::search(proof.borrow_roles.len()))?;
        if !proof.borrow_roles.contains_key(&(original.id, node)) {
            return Ok(None);
        }
    }
    let operand = occurrences::site_value(world, instance, original, site)?
        .ok_or_else(|| bad("call loan has no exact emitted result occurrence"))?;
    let actual = world.get(values, operand)?;
    let expected = world.physical_type(
        instance.id,
        &SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(node),
        },
        World::type_of(&actual)?,
    )?;
    apply(
        world, instance, original, function, site, operand, &actual, &expected, values, locals,
        implicit,
    )
    .map(|result| result.map(|(value, source)| (operand, value, source)))
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
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(SharedType, roles::Source)>> {
    if !matches!(
        World::type_of(actual)?,
        Type::Address { .. } | Type::Desc(_)
    ) {
        return Ok(None);
    }
    let Some(node) = occurrences::node(world, original, site)? else {
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
        for step in &role.steps {
            if let ViewBorrowStepKind::CallInputs { inputs, .. } = &step.kind {
                if inputs.len() > world.limits.max_nodes {
                    return Err(Diagnostic::limit(Phase::Types, "actual source input count exceeded", None));
                }
                world.pay(inputs.len().saturating_mul(2))?;
            }
            if let ViewBorrowStepKind::Capture { name } = &step.kind {
                world.pay(name.len())?;
            }
            if let ViewBorrowStepKind::ProjectedLocal { pattern, .. } = &step.kind {
                if pattern.len() > world.limits.max_call_depth {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "projected binding path depth exceeded",
                        None,
                    ));
                }
                world.pay(pattern.len())?;
            }
        }
        role.clone()
    };
    apply_role(
        world, instance, original, function, &role, operand, actual, expected, values, locals,
        implicit, false,
    )
}

fn role_target(
    world: &mut World<'_, '_>, instance: &SemanticInstance,
    role: &BorrowRole, implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<roles::Source> {
    Ok(match &role.target_lifetime {
        SemanticOuterLifeRecipe::Static => roles::Source::Static,
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
            roles::Source::Class(world.class(instance.id, &image, implicit)?)
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_role(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    role: &BorrowRole,
    operand: ValueId,
    actual: &SharedType,
    expected: &SharedType,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
    return_boundary: bool,
) -> R<Option<(SharedType, roles::Source)>> {
    let expected_source = roles::outer(world, expected)?;
    let target_source = role_target(world, instance, role, implicit)?;
    if expected_source != target_source {
        // The receipt belongs to a later call adapter, not to this source
        // value's original sigma. Its exact call/parameter consumes it there.
        return Ok(None);
    }
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
        || !matches!(
            World::type_of(actual)?,
            Type::Address { .. } | Type::Desc(_)
        )
    {
        return Err(bad(
            "projected borrow role changes its complete actual type or access mode",
        ));
    }
    let inner = world.pointee(actual)?;
    let target_inner = world.pointee(expected)?;
    world.same(instance.id, &inner, &target_inner, implicit)?;
    trace(
        world,
        instance,
        original,
        function,
        &role,
        operand,
        values,
        locals,
        implicit,
        return_boundary,
    )?;
    let source = roles::outer(world, actual)?;
    let roots = arguments::roots(world, operand, actual)?;
    let input_referents = if return_boundary {
        let proof = world.ledger.original.borrow();
        let proof = proof.as_ref().ok_or_else(|| bad("returned loan has no complete original Gamma"))?;
        proof.view_input_referents(original.id, role.root, &role.steps, world.limits, world.left)?
    } else { Vec::new() };
    for root in &roots {
        world.pay(1)?;
        if return_boundary {
            world.pay(role.steps.len())?;
        }
        // A loaded input pointer's slot can end before Return; its referent
        // loan belongs to the caller and is still the function's declared
        // input. Borrowing the pointer slot (ParameterStorage) never qualifies.
        world.pay(input_referents.len())?;
        let input_referent = return_boundary && input_referents.iter().any(|parameter|
            function.parameters.get(*parameter as usize) == Some(&root.local));
        if world.stale_locals.contains(&root.local)
            || !input_referent
                && locals
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

/// A nominal field uses the loan proved for this constructor and this position.
/// The original argument still has its own occurrence type.
#[allow(clippy::too_many_arguments)]
pub(super) fn payload_view(
    world: &mut World<'_, '_>, instance: &SemanticInstance, original: &SemanticDefinition,
    site: &SemanticSite, position: usize, operand: ValueId, actual: &SharedType, expected: &SharedType,
    values: &ssa::Values, locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<bool> {
    if !matches!(World::type_of(actual)?, Type::Address { mutable: false, .. }) {
        return Ok(false);
    }
    let Some(node) = occurrences::node(world, original, site)? else { return Ok(false); };
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof.as_ref().ok_or_else(|| bad("nominal payload lacks its original Gamma"))?;
    world.pay(index::search(proof.payload_borrows.len()))?;
    let Some((argument, role)) = proof.payload_borrows.get(&(original.id, node, position)) else {
        return Ok(false);
    };
    if occurrences::expression_value(world, instance, original, *argument)? != Some(operand) {
        return Err(bad("nominal payload changes its actual original argument"));
    }
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target else {
        return Err(bad("nominal borrow has no actual function body"));
    };
    let function = world.function(id)?;
    Ok(apply_role(world, instance, original, function, role, operand, actual, expected,
        values, locals, implicit, false)?.is_some())
}

/// Recheck the complete incoming class on this concrete CFG edge. Only the
/// omitted result's outer position can shorten; every pointee class is invariant.
pub(super) fn conditional_edge(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    site: &SemanticSite,
    actual: &SharedType,
    expected: &SharedType,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<bool> {
    if !matches!(World::type_of(expected)?, Type::Address { .. })
        || World::type_of(actual)? != World::type_of(expected)?
    {
        return Ok(false);
    }
    let roles::Source::Class(SemanticClassExpr::Implicit {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    }) = roles::outer(world, expected)?
    else {
        return Ok(false);
    };
    let Some(node) = occurrences::node(world, original, site)? else {
        return Ok(false);
    };
    let sources = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("conditional has no checked original environment"))?;
        world.pay(index::search(proof.call_regions.len()))?;
        let Some(row) = proof
            .call_regions
            .get(&(original.id, ordinal))
            .and_then(|row| row.conditional_sources(node))
        else {
            return Ok(false);
        };
        let mut sources = Vec::new();
        for source in row {
            world.pay(1)?;
            let SemanticOriginalNode::Expression(node) = source.at else {
                return Err(bad("conditional source has no actual expression"));
            };
            sources.push(node);
        }
        sources
    };
    let incoming = roles::outer(world, actual)?;
    let mut found = false;
    for source in sources {
        let original_arm = world.physical_type(
            instance.id,
            &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Expression(source),
            },
            World::type_of(actual)?,
        )?;
        let arm = roles::outer(world, &original_arm)?;
        found |= match (&arm, &incoming) {
            (roles::Source::Class(a), roles::Source::Class(z)) => {
                world.class(instance.id, a, implicit)? == world.class(instance.id, z, implicit)?
            }
            (roles::Source::Static, roles::Source::Static) => true,
            _ => false,
        };
    }
    if !found {
        return Ok(false);
    }
    let child = world.pointee(actual)?;
    let wanted = world.pointee(expected)?;
    world.same(instance.id, &child, &wanted, implicit)?;
    Ok(true)
}

pub(super) fn inferred_result_edge(world: &mut World<'_, '_>, instance: &SemanticInstance,
    original: &SemanticDefinition, actual: &SharedType, expected: &SharedType,
    implicit: &BTreeMap<u32, SemanticClassExpr>) -> R<bool> {
    let Some(ordinal) = original.elided_result_ordinal() else { return Ok(false); };
    if !matches!(World::type_of(expected)?, Type::Address { .. })
        || !matches!(roles::outer(world, expected)?, roles::Source::Class(SemanticClassExpr::Implicit {
            scope: SemanticScopeRef::Ancestor(0), ordinal: expected
        }) if expected == ordinal)
        || World::type_of(actual)? != World::type_of(expected)? { return Ok(false); }
    let source = roles::outer(world, actual)?;
    if let roles::Source::Class(actual_class) = source {
        if actual_class == (SemanticClassExpr::Implicit {scope: SemanticScopeRef::Ancestor(0),ordinal}) {
            return Ok(false);
        }
        let mut admitted=false;
        if let SemanticClassExpr::Implicit {scope:SemanticScopeRef::Ancestor(0),ordinal:source}=actual_class {
            let ledger=world.ledger;
            let proof=ledger.original.borrow();
            let proof=proof.as_ref().ok_or_else(||bad("returned loan has no checked originating body"))?;
            world.pay(index::search(proof.call_regions.len()))?;
            if let Some(role)=proof.call_regions.get(&(original.id,source)) {
                world.pay(role.required_outlives.len())?;
                admitted=role.required_outlives.iter().any(|bound|matches!(bound,
                    SemanticLifeRecipe::Implicit {scope:SemanticScopeRef::Ancestor(0),ordinal:target} if *target==ordinal));
            }
        }
        let actual_class = world.class(instance.id, &actual_class, implicit)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &original.body else { unreachable!(); };
        for (index, parameter) in signature.parameters.iter().enumerate() {
            world.pay(1)?;
            let (SemanticTypeRecipe::Address { lifetime: SemanticOuterLifeRecipe::Class(source), .. } |
                SemanticTypeRecipe::Desc { lifetime: SemanticOuterLifeRecipe::Class(source), .. }) = &parameter.ty else { continue; };
            world.pay(original.requirements.len())?;
            if !original.requirements.iter().any(|r| original.elided_result_bound(r) == Some(source)) { continue; }
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(fid)) = instance.target else { return Ok(false); };
            let function = world.function(fid)?;
            let raw = function.parameters.get(index).and_then(|id| function.locals.get(id.0 as usize))
                .ok_or_else(|| bad("inferred return source has no concrete input position"))?;
            let value = world.parameter_value(instance.id, index, types::Hint::Actual(&raw.ty))?;
            if let roles::Source::Class(input) = roles::outer(world, &value)? {
                let input = world.class(instance.id, &input, implicit)?;
                admitted |= actual_class == input;
            }
        }
        if !admitted { return Ok(false); }
    }
    let inner = world.pointee(actual)?;
    let wanted = world.pointee(expected)?;
    world.same(instance.id, &inner, &wanted, implicit)?;
    Ok(true)
}

/// A return consumes only its own original expression's directed result
/// loan. A call adapter on the same source value is not a result theorem.
#[allow(clippy::too_many_arguments)]
pub(super) fn return_view(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    operand: ValueId,
    actual: &SharedType,
    expected: &SharedType,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<bool> {
    if !matches!(
        World::type_of(actual)?,
        Type::Address { mutable: false, .. }
    ) {
        return Ok(false);
    }
    let role = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("result view lacks original Gamma"))?;
        world.pay(index::search(proof.return_borrows.len()))?;
        if proof
            .return_borrows
            .range((original.id, NodeId(0))..=(original.id, NodeId(u32::MAX)))
            .next()
            .is_none()
        {
            return Ok(false);
        }
        let mut selected = None;
        for site in sites {
            world.pay(1)?;
            let Some(node) = occurrences::node(world, original, site)? else { continue; };
            world.pay(index::search(proof.return_borrows.len()))?;
            let Some(role) = proof.return_borrows.get(&(original.id, node)) else { continue; };
            if occurrences::site_value(world, instance, original, site)? != Some(operand) { continue; }
            if selected.replace((node, role)).is_some_and(|(previous, _)| previous != node) {
                return Err(bad("result SSA has several original result-loan occurrences"));
            }
        }
        let Some((node, role)) = selected else {
            return Ok(false);
        };
        for lifetime in [
            &role.source_lifetime,
            &role.target_lifetime,
            &role.occurrence_lifetime,
        ] {
            crate::dyn_metadata::semantic_operand_snapshot(lifetime, world.limits, world.left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&role.inner, world.limits, world.left, 1)?;
        world.pay(role.steps.len().saturating_add(1))?;
        for step in &role.steps {
            match &step.kind {
                ViewBorrowStepKind::Capture { name } => world.pay(name.len())?,
                ViewBorrowStepKind::CallInputs { inputs, .. } => world.pay(inputs.len().saturating_mul(2))?,
                ViewBorrowStepKind::ProjectedLocal { pattern, .. } => {
                    if pattern.len() > world.limits.max_call_depth {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "result loan path depth exceeded",
                            None,
                        ));
                    }
                    world.pay(pattern.len())?;
                }
                _ => {}
            }
        }
        (node, role.clone())
    };
    let source = world.physical_type(
        instance.id,
        &SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(role.0),
        },
        World::type_of(actual)?,
    )?;
    world.same(instance.id, actual, &source, implicit)?;
    let receiver_upper = matches!(&role.1.target_lifetime,
        SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
            operand: SemanticRecipeOperand::Receiver, path, mode: StoredBorrowMode::SharedAddress,
        }) if path.is_empty());
    if receiver_upper && function.storage_life.as_ref().is_some_and(|header|
        header.bindings.iter().any(|row| row.position.base == StorageBase::Receiver))
    {
        let expected_source = roles::outer(world, expected)?;
        if !matches!(&expected_source, roles::Source::Class(SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Receiver, path, mode: StoredBorrowMode::SharedAddress,
        }) if path.is_empty()) {
            // A stronger entry result remains its exact Fixed type. Consume
            // the actual capture derivation even when no local type adapter
            // is required; a later receiver-bounded operation uses this proof.
            trace(world, instance, original, function, &role.1, operand,
                values, locals, implicit, true)?;
            return Ok(false);
        }
    }
    Ok(apply_role(
        world, instance, original, function, &role.1, operand, actual, expected, values, locals,
        implicit, true,
    )?
    .is_some())
}

/// Consume a shared view only at the exact original binding/ascription
/// boundary. The exclusive read and its actual parent remain unchanged.
#[allow(clippy::too_many_arguments)]
fn shared_view_value(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    target: Query,
    operand: ValueId,
    actual: &SharedType,
    expected: &SharedType,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(SharedType, roles::Source)>> {
    let source_node = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("shared view lacks original Gamma"))?;
        world.pay(index::search(proof.shared_views.len()))?;
        let Some(node) = proof.shared_views.get(&(original.id, target)) else {
            return Ok(None);
        };
        *node
    };
    if !matches!(
        World::type_of(actual)?,
        Type::Address { mutable: false, .. }
    ) || World::type_of(actual)? != World::type_of(expected)?
    {
        return Err(bad(
            "shared input view changes its complete target type or access",
        ));
    }
    let InstructionKind::ReborrowAddress {
        address,
        mutable: false,
    } = producer(world, function, operand)?.0
    else {
        return Err(bad(
            "shared input view lacks its real shared ReborrowAddress",
        ));
    };
    let parent = *address;
    let incoming = world.get(values, parent)?;
    if !matches!(
        World::type_of(&incoming)?,
        Type::Address { mutable: true, .. }
    ) {
        return Err(bad(
            "shared input view has no exact exclusive incoming source",
        ));
    }
    let mut read = false;
    for site in sites {
        world.pay(1)?;
        if !matches!(site.action, SemanticSiteAction::TypeUse {
            subject: SemanticOperand::Value(value), ..
        } if value == parent)
        {
            continue;
        }
        if occurrences::node(world, original, site)? == Some(source_node) {
            if read {
                return Err(bad("shared input view has ambiguous original reads"));
            }
            read = true;
        }
    }
    if !read {
        return Err(bad(
            "shared input view loses its original exclusive read SSA",
        ));
    }
    let source = world.physical_type(
        instance.id,
        &SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(source_node),
        },
        World::type_of(&incoming)?,
    )?;
    world.same(instance.id, &incoming, &source, implicit)?;
    let inner = world.pointee(&incoming)?;
    let actual_inner = world.pointee(actual)?;
    let target_inner = world.pointee(expected)?;
    world.same(instance.id, &inner, &actual_inner, implicit)?;
    world.same(instance.id, &inner, &target_inner, implicit)?;
    let incoming_outer = roles::outer(world, &incoming)?;
    let target_outer = roles::outer(world, expected)?;
    match (&incoming_outer, &target_outer) {
        (roles::Source::Class(source), roles::Source::Class(target)) => {
            world.same_class(instance.id, source, target, implicit)?
        }
        _ => return Err(bad("shared input view changes its rigid original lifetime")),
    }
    for root in arguments::roots(world, parent, &incoming)? {
        world.pay(
            world
                .vacant
                .len()
                .saturating_add(root.path.len())
                .saturating_add(1),
        )?;
        if world.stale_locals.contains(&root.local)
            || locals
                .get(root.local.0 as usize)
                .and_then(Option::as_ref)
                .is_none()
            || world.vacant.iter().any(|(local, path)| {
                *local == root.local
                    && (path.starts_with(&root.path) || root.path.starts_with(path))
            })
        {
            return Err(bad("shared input view has no live initialized parent loan"));
        }
    }
    Ok(Some((world.share(expected)?, incoming_outer)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn shared_expression_view(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    site: &SemanticSite,
    operand: ValueId,
    actual: &SharedType,
    expected: &SharedType,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(SharedType, roles::Source)>> {
    if !matches!(
        World::type_of(actual)?,
        Type::Address { mutable: false, .. }
    ) {
        return Ok(None);
    }
    let Some(node) = occurrences::node(world, original, site)? else {
        return Ok(None);
    };
    shared_view_value(
        world,
        instance,
        original,
        function,
        sites,
        Query::Expression(node),
        operand,
        actual,
        expected,
        values,
        locals,
        implicit,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn shared_binding_view(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    local: LocalId,
    operand: ValueId,
    actual: &SharedType,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(SharedType, roles::Source)>> {
    if !matches!(
        World::type_of(actual)?,
        Type::Address { mutable: false, .. }
    ) {
        return Ok(None);
    }
    {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("shared binding lacks original Gamma"))?;
        world.pay(index::search(proof.shared_views.len()))?;
        if proof
            .shared_views
            .range(
                (original.id, Query::Binding(NodeId(0)))
                    ..=(original.id, Query::Binding(NodeId(u32::MAX))),
            )
            .next()
            .is_none()
        {
            return Ok(None);
        }
    }
    let mut result = None;
    for site in world.local_declarations(instance.id, local)? {
        let SemanticSiteRecipeKind::DeclareLocal {
            expected:
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Binding(binding),
                },
        } = &world.original_recipe(original, site.recipe)?.kind
        else {
            continue;
        };
        let binding = *binding;
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        world.pay(index::search(
            proof.as_ref().map_or(0, |proof| proof.shared_views.len()),
        ))?;
        if !proof.as_ref().is_some_and(|proof| {
            proof
                .shared_views
                .contains_key(&(original.id, Query::Binding(binding)))
        }) {
            continue;
        }
        let Some((_, _, block, store, _)) = world.current_point else {
            return Err(bad("shared binding view lacks its exact Store boundary"));
        };
        let SemanticSiteAddress::Typed {
            block: declaration_block,
            instruction,
            ..
        } = site.at
        else {
            return Err(bad("shared binding has no relocated declaration"));
        };
        if block != declaration_block || instruction as usize >= store {
            continue;
        }
        let body = function
            .blocks
            .iter()
            .find(|row| row.id == block)
            .ok_or_else(|| bad("shared binding leaves its actual CFG"))?;
        world.pay(store.saturating_sub(instruction as usize))?;
        if !matches!(body.instructions.get(instruction as usize).map(|row| &row.kind), Some(InstructionKind::StorageLive(id)) if *id == local)
            || body.instructions[instruction as usize + 1..store].iter().any(|row|
                matches!(row.kind, InstructionKind::Store { local: id, .. }
                    | InstructionKind::StorageDead(id) | InstructionKind::TakeLocal { local: id } if id == local)) {
            continue;
        }
        let expected = world.physical_type(
            instance.id,
            &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Binding(binding),
            },
            World::type_of(actual)?,
        )?;
        if let Some(view) = shared_view_value(
            world,
            instance,
            original,
            function,
            sites,
            Query::Binding(binding),
            operand,
            actual,
            &expected,
            values,
            locals,
            implicit,
        )? {
            if result.replace(view).is_some() {
                return Err(bad("shared binding has multiple declaration loans"));
            }
        }
    }
    Ok(result)
}

/// A directed call loan preserves the source's original complete sigma.
fn original_read_value(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    node: NodeId,
) -> R<Option<ValueId>> {
    occurrences::expression_value(world, instance, original, node)

}

/// Establish an original source receipt only at its real executed expression
/// boundary, after the ordinary whole-type constraint has succeeded. The
/// receipt travels with the existing incoming role fact; it never revives an
/// old SSA cell or grants current root/epoch/permission.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_source(
    world: &mut World<'_, '_>, instance: &SemanticInstance,
    original: &SemanticDefinition, function: &TypedFunction, site: &SemanticSite,
    values: &ssa::Values, locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let Some(node) = occurrences::node(world, original, site)? else { return Ok(()); };
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof.as_ref().ok_or_else(|| bad("incoming source has no original proof"))?;
    let mut source = false;
    world.pay(index::search(proof.local_roles.len()))?;
    for ((_, _), role) in proof.local_roles.range((original.id, NodeId(0))..=(original.id, NodeId(u32::MAX))) {
        world.pay(role.sources.len().saturating_add(1))?;
        source |= role.sources.iter().any(|row| row.at == SemanticOriginalNode::Expression(node));
    }
    if !source { return Ok(()); }
    let Some(operand) = occurrences::site_value(world, instance, original, site)? else { return Ok(()); };
    let actual = world.get(values, operand)?;
    if matches!(World::type_of(&actual)?, Type::Closure(_)) {
        // A receiver access role may name a temporary whole callee, whose
        // logical value is a Closure rather than a raw address source. Its
        // actual prepared place/operation has its own complete callee receipt;
        // never manufacture a raw-pointer origin from that whole environment.
        world.pay(index::search(proof.callable_calls.len()))?;
        for (_, call) in proof.callable_calls.range((original.id, SemanticSiteId(0))..=(original.id, SemanticSiteId(u32::MAX))) {
            world.pay(1)?;
            if call.callee_node == node { return Ok(()); }
        }
        return Err(bad("whole closure source has no exact original receiver access"));
    }
    if !matches!(World::type_of(&actual)?, Type::Address { .. } | Type::Desc(_)) {
        return Err(bad("incoming loan source changes its actual address carrier"));
    }
    let Some((root, steps)) = proof.view_source_steps(original.id, node, world.limits, world.left)? else {
        return Err(bad("incoming source has no complete original projection trace"));
    };
    trace_steps(world, instance, original, function, root, &steps, Cursor::Value(operand),
        values, locals, implicit, false, 0)?;
    // A pure local-pointer copy preserves its already transported source.
    // ReadLocal followed by Field (or another referent-selecting operation)
    // is not such a copy: its projection deliberately dropped the storage
    // origin and the full trace above now forms the leaf's own receipt below.
    world.pay(steps.len().saturating_add(index::search(world.value_role_sources.len())).saturating_add(1))?;
    if super::super::original::local_source_copy(root, &steps) {
        if world.value_role_sources.get(&operand).and_then(|source| source.origin.as_ref()).is_none() {
            return Err(bad("local-pointer source copy has no transported original receipt"));
        }
        return Ok(());
    }
    let roots = proof.view_source_roots(original.id, root, &steps, world.limits, world.left)?;
    // No complete root result is an unknown theorem, never an outside
    // parameter/capture premise (notably ParameterStorage is activation local).
    let roots = roots.unwrap_or_default();
    world.pay(roots.len().saturating_add(1))?;
    let mut anchors = Vec::with_capacity(roots.len().max(1));
    if roots.is_empty() { anchors.push(roles::OriginAnchor::Other); }
    for root in roots {
        let anchor = match root {
            ViewBorrowRoot::Parameter(parameter) => roles::OriginAnchor::Input(parameter),
            ViewBorrowRoot::Capture(source) => {
                let (_, capture_steps) = proof.view_source_steps(original.id, source, world.limits, world.left)?
                    .ok_or_else(|| bad("source capture root lacks its complete original path"))?;
                let capture = capture_steps.iter().find_map(|step| match &step.kind {
                    ViewBorrowStepKind::Capture { name } => Some(name), _ => None,
                }).ok_or_else(|| bad("source capture receipt has no original capture selection"))?;
                let closure = function.environment.ok_or_else(|| bad("source capture receipt has no actual environment"))?;
                let selections = ledger.closure_captures.get(&closure)
                    .ok_or_else(|| bad("source capture receipt has no actual creator selections"))?;
                world.pay(selections.len().saturating_mul(capture.len().saturating_add(1)))?;
                let mut selected = selections.iter().enumerate().filter(|(_, (name, _))| name == capture);
                let (index, _) = selected.next().ok_or_else(|| bad("source capture receipt lost its exact creator field"))?;
                if selected.next().is_some() { return Err(bad("source capture receipt repeats its creator field")); }
                roles::OriginAnchor::Capture(u32::try_from(index).map_err(|_| bad("source capture ordinal overflows"))?)
            }
            _ => roles::OriginAnchor::Other,
        };
        anchors.push(anchor);
    }
    let image = if let Some(incoming) = world.value_role_sources.get(&operand) {
        roles::snapshot_source(&incoming.source, world.limits, world.left)?
    } else { roles::outer(world, &actual)? };
    world.pay(index::search(world.value_role_sources.len()).saturating_add(1))?;
    world.value_role_sources.insert(operand, Rc::new(roles::Incoming { source: image,
        origin: Some(roles::OriginalLoan { node, mint: operand, anchors: Rc::new(anchors) }) }));
    Ok(())
}

/// A mutable pointer's complete original source union is not the pointer
/// cell itself. Authenticate every real reaching copy against the corresponding
/// immutable source expression, including grounded self-copy loop backedges.
#[allow(clippy::too_many_arguments)]
fn local_read_sources(
    world: &mut World<'_, '_>, instance: &SemanticInstance, original: &SemanticDefinition,
    function: &TypedFunction, binding: NodeId, observed:NodeId, local: LocalId, read: ValueId, at: (BlockId, usize),
) -> R<()> {
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof.as_ref().ok_or_else(|| bad("local loan has no original source proof"))?;
    if !proof.return_writers.is_empty(){world.pay(index::search(proof.return_writers.len()))?;}
    let nodes=if let Some((writer,_))=proof.return_writers.get(&(original.id,observed)) {world.pay(writer.writers().len())?;writer.writers().to_vec()} else {
        proof.view_local_sources(original.id,binding,world.limits,world.left)?
            .ok_or_else(||bad("local loan has no complete original incoming sources"))?
    };
    world.pay(nodes.len())?;
    let mut sources = Vec::new();
    for node in nodes {
        let value = original_read_value(world, instance, original, node)?
            .ok_or_else(|| bad("local loan source lacks its exact original actual read"))?;
        sources.push((value, node));
    }
    world.pay(index::search(world.value_role_sources.len()).saturating_add(1))?;
    let origin = world.value_role_sources.get(&read).and_then(|source| source.origin.clone())
        .ok_or_else(|| bad("local loan has no actually reached original source receipt"))?;
    if !sources.iter().any(|(mint, node)| *mint == origin.mint && *node == origin.node)
    { return Err(bad("local loan incoming receipt selects another original source")); }
    world.pay(origin.anchors.len())?;
    for anchor in origin.anchors.iter() {
        match anchor {
            roles::OriginAnchor::Capture(index) => { world.capture(instance, *index)?; }
            roles::OriginAnchor::Input(parameter) => {
                if function.parameters.get(*parameter as usize).is_none() {
                    return Err(bad("local loan incoming receipt loses its actual caller input"));
                }
            }
            roles::OriginAnchor::Other => {}
        }
    }
    let mut pending = Vec::new();
    for source in reaching_values_inner(world, function, local, at, true)? {
        let ActivationSource::Value(value) = source else {
            return Err(bad("local loan replaces its original initializer with a capture"));
        };
        world.pay(1)?;
        pending.push(value);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut grounded = false;
    let mut reaches_current = false;
    while let Some(value) = pending.pop() {
        world.pay(index::search(seen.len()).saturating_add(sources.len()).saturating_add(1))?;
        if !seen.insert(value) { continue; }
        if seen.len() > world.limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "local loan reaching-copy node limit exceeded", None));
        }
        // This finite predecessor traversal visits historical stores on all
        // incoming paths, including a backedge that has not executed in this
        // particular whole-state. Such SSA mints need not dominate the current
        // point. Authenticate their actual producer/type structurally, and
        // consume current permission only at the grounded source trace below.
        let ty = function.values.get(value.0 as usize)
            .ok_or_else(|| bad("local loan historical source has no actual SSA type"))?;
        inference::charge_type_snapshot(ty, Span::default(), world.limits, world.left, 1)?;
        if !matches!(ty, Type::Address { .. } | Type::Desc(_)) {
            return Err(bad("local loan copy graph changes its actual address carrier"));
        }
        if sources.iter().any(|(source, _)| *source == value) {
            // Every such original mint is checked at its own real boundary
            // by record_source in every reaching whole-state. At this use we
            // require the CURRENT occupant's transported receipt, rather than
            // reading a sibling/future mint as a currently available SSA.
            reaches_current |= value == origin.mint;
            grounded = true;
            continue;
        }
        let (operation, block, instruction) = producer(world, function, value)?;
        match operation {
            InstructionKind::ReborrowAddress { address, mutable: false } => {
                world.pay(1)?;
                pending.push(*address);
            }
            InstructionKind::Load(local) => {
                for source in reaching_values_inner(world, function, *local, (block, instruction), true)? {
                    let ActivationSource::Value(value) = source else {
                        return Err(bad("local loan alias changes its original storage source"));
                    };
                    world.pay(1)?;
                    pending.push(value);
                }
            }
            _ => return Err(bad("local loan value does not reach its original complete source")),
        }
        if pending.len() > world.limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "local loan reaching-copy frontier exceeded", None));
        }
    }
    if !grounded { return Err(bad("local loan alias cycle has no original source")); }
    if !reaches_current { return Err(bad("local loan copy graph loses its actually reached source receipt")); }
    // Current root, stale-epoch and vacancy checks remain in the caller's
    // normal directed-loan consumption; this receipt supplies only provenance.
    Ok(())
}

/// Source lowering can stage a field in a hygienic local. Authenticate every
/// reaching Store/Load and real reborrow back to the exact original read;
/// neither a temporary's name nor equality of pointer types is an origin.
#[allow(clippy::too_many_arguments)]
fn read_transport(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    function: &TypedFunction,
    value: ValueId,
    original_read: ValueId,
    values: &ssa::Values,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
    active: &mut std::collections::BTreeSet<ValueId>,
    depth: usize,
) -> R<()> {
    world.pay(index::search(active.len()).saturating_add(1))?;
    if depth > world.limits.max_call_depth || active.len() > world.limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "original loan read transport limit exceeded",
            None,
        ));
    }
    let actual = world.get(values, value)?;
    if value == original_read {
        return Ok(());
    }
    if !active.insert(value) {
        return Err(bad(
            "original loan read has a cyclic actual alias transport",
        ));
    }
    let (operation, block, instruction) = producer(world, function, value)?;
    match operation {
        InstructionKind::ReborrowAddress { address, mutable } => {
            let incoming = world.get(values, *address)?;
            places::address_mutable(world, &incoming, *mutable)?;
            places::borrow_mode(Some(World::type_of(&actual)?), *mutable)?;
            let source = world.pointee(&incoming)?;
            let child = world.pointee(&actual)?;
            world.same(instance.id, &source, &child, implicit)?;
            read_transport(
                world,
                instance,
                function,
                *address,
                original_read,
                values,
                implicit,
                active,
                depth + 1,
            )?;
        }
        InstructionKind::Load(local) => {
            for source in reaching_values(world, function, *local, (block, instruction))? {
                let ActivationSource::Value(source) = source else {
                    return Err(bad(
                        "loan read transport substitutes a capture for its original SSA",
                    ));
                };
                let incoming = world.get(values, source)?;
                world.same(instance.id, &actual, &incoming, implicit)?;
                read_transport(
                    world,
                    instance,
                    function,
                    source,
                    original_read,
                    values,
                    implicit,
                    active,
                    depth + 1,
                )?;
            }
        }
        _ => {
            return Err(bad(
                "loan field value has no actual transport from its original read",
            ));
        }
    }
    active.remove(&value);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn call_region_argument(
    world: &mut World<'_, '_>,
    caller: &SemanticInstance,
    site: &SemanticSite,
    parameter: usize,
    operand: ValueId,
    expected: &SemanticTypeValue,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<bool> {
    let roles::Source::Class(SemanticClassExpr::Implicit {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    }) = roles::outer(world, expected)?
    else {
        return Ok(false);
    };
    let original = world.definition(caller.definition)?;
    let parameter =
        u32::try_from(parameter).map_err(|_| bad("call region parameter ordinal overflows"))?;
    let (source_node, role) = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("call region lost its complete original proof"))?;
        let Some(call) = occurrences::node(world, original, site)? else { return Ok(false); };
        world.pay(index::search(proof.call_regions.len()))?;
        let node = if let Some(region) = proof.call_regions.get(&(original.id, ordinal)) {
            world.pay(index::search(region.inputs.len()))?;
            if call == region.call {
                *region.inputs.get(&parameter)
                    .ok_or_else(|| bad("call region omits its original full-ordinal argument"))?
            } else {
                world.pay(index::search(proof.call_borrows.len()))?;
                let Some(node) = proof.call_borrows.get(&(original.id, call, parameter)) else { return Ok(false); };
                *node
            }
        } else {
            // A parameter may already be fixed to a mutable local's original
            // role by an invariant inner type. That role is not a fresh call
            // region: authenticate its own declaration and this exact input.
            world.pay(proof.local_roles.len())?;
            let lifetime = SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            };
            let mut roles = proof
                .local_roles
                .iter()
                .filter(|((owner, _), role)| *owner == original.id && role.lifetime == lifetime);
            if roles.next().is_none() {
                return Ok(false);
            }
            if roles.next().is_some() {
                return Err(bad(
                    "contextual call target has several original local roles",
                ));
            }
            if !matches!(
                world.original_recipe(original, site.recipe)?.kind,
                SemanticSiteRecipeKind::OriginalCall | SemanticSiteRecipeKind::Construct { .. }
            ) {
                return Ok(false);
            }
            world.pay(index::search(proof.call_borrows.len()))?;
            let Some(node) = proof.call_borrows.get(&(original.id, call, parameter)) else {
                return Ok(false);
            };
            *node
        };
        world.pay(index::search(proof.temporary_descriptions.len()))?;
        if let Some(receipt) = proof.temporary_descriptions.get(&(original.id, call, parameter)) {
            if receipt.argument != node {
                return Err(bad("call temporary description changes its original argument occurrence"));
            }
            return temporary_description(
                world, caller, original, site, receipt.producer, operand, locals, implicit,
            ).map(|()| true);
        }
        world.pay(index::search(proof.borrow_roles.len()))?;
        let Some(role) = proof.borrow_roles.get(&(original.id, node)) else {
            // This input may already have the exact region or a proved Static
            // endpoint. Without a loan receipt this helper grants nothing;
            // the ordinary exact/reaching-edge consumer must still succeed.
            return Ok(false);
        };
        if role.target_lifetime
            != SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            })
        {
            return Err(bad("call loan changes its independently selected region"));
        }
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
        for step in &role.steps {
            match &step.kind {
                ViewBorrowStepKind::ProjectedLocal { pattern, .. } => {
                    if pattern.len() > world.limits.max_call_depth {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "call loan pattern depth exceeded",
                            None,
                        ));
                    }
                    world.pay(pattern.len())?;
                }
                ViewBorrowStepKind::Capture { name } => world.pay(name.len())?,
                ViewBorrowStepKind::CallInputs { inputs, .. } => {
                    if inputs.len() > world.limits.max_nodes {
                        return Err(Diagnostic::limit(Phase::Types, "call source input count exceeded", None));
                    }
                    world.pay(inputs.len().saturating_mul(2))?;
                }
                _ => {}
            }
        }
        (node, role.clone())
    };
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = caller.target
    else {
        return Err(bad("call loan has no actual function"));
    };
    let function = world.function(function)?;
    let staged_constructor = matches!(
        world.original_recipe(original, site.recipe)?.kind,
        SemanticSiteRecipeKind::Construct { .. }
    );
    let read = if staged_constructor {
        original_read_value(world, caller, original, source_node)?
    } else {
        None
    };
    let source = if let Some(source) = read {
        read_transport(
            world,
            caller,
            function,
            operand,
            source,
            values,
            implicit,
            &mut std::collections::BTreeSet::new(),
            0,
        )?;
        source
    } else {
        operand
    };
    trace(
        world, caller, original, function, &role, source, values, locals, implicit, false,
    )?;
    Ok(true)
}

fn temporary_description(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    site: &SemanticSite,
    producer_node: NodeId,
    operand: ValueId,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = instance.target
    else { return Err(bad("temporary description has no actual caller")); };
    let function = world.function(function)?;
    let (operation, block, at) = producer(world, function, operand)?;
    let InstructionKind::DescribeLocal { local } = operation else {
        return Err(bad("temporary description needs its actual owner slot"));
    };
    let SemanticSiteAddress::Typed {
        block: call_block, instruction: call_at, edge: SemanticSiteEdge::After,
    } = site.at else { return Err(bad("temporary description lacks its actual call boundary")); };
    if call_block != block || at >= call_at as usize {
        return Err(bad("temporary description escaped its actual call preparation"));
    }
    let source = occurrences::owning_expression_value(world, instance, original, producer_node)?
        .ok_or_else(|| pending("temporary description requires an exact original owning value producer"))?;
    if reaching_values(world, function, *local, (block, at))?.as_slice()
            != [ActivationSource::Value(source)]
        || reaching_values(world, function, *local, (call_block, call_at as usize))?.as_slice()
            != [ActivationSource::Value(source)]
    {
        return Err(bad("temporary description changes its real reaching owned argument"));
    }
    let actual = world.local(locals, *local)?;
    let expected = world.physical_type(instance.id, &SemanticTypeRecipe::OriginalType {
        scope: SemanticScopeRef::Ancestor(0),
        source: SemanticOriginalTypeSource::Expression(producer_node),
    }, World::type_of(&actual)?)?;
    if !matches!(World::type_of(&expected)?, Type::Managed(_)) {
        return Err(bad("temporary description has no original Managed source"));
    }
    world.same(instance.id, &actual, &expected, implicit)
}
