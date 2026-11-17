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
    StoredField { value: ValueId, remaining: usize },
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ActivationSource {
    Value(ValueId),
    Capture(u32),
}
/// All real definitions reaching this Load, stopped at the nearest write on
/// each CFG predecessor path. A loop backedge is not an arbitrary first store.
fn reaching_values(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    local: LocalId,
    at: (BlockId, usize),
) -> R<Vec<ActivationSource>> {
    world.pay(index::search(world.cfg_shapes.len()).saturating_add(1))?;
    let shape = world
        .cfg_shapes
        .get(&function.id)
        .cloned()
        .ok_or_else(|| bad("capture source query lacks its checked immutable CFG"))?;
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
        world.pay(function.blocks.len())?;
        let body = function
            .blocks
            .iter()
            .find(|row| row.id == block)
            .ok_or_else(|| bad("capture read reaches an absent actual block"))?;
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
                _ => {}
            }
        }
        if let Some(value) = assigned {
            values.insert(value);
            continue;
        }
        let mut predecessor = false;
        for parent in &function.blocks {
            world.pay(1)?;
            let reaches = match parent.terminator {
                Terminator::Jump(target) => target == block,
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => then_block == block || else_block == block,
                _ => false,
            };
            if reaches && shape.entry_reaches(world, function, parent.id)? {
                pending.push((parent.id, parent.instructions.len()));
                predecessor = true;
            }
        }
        if !predecessor {
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
            let (definition, child) = world.child(closure)?;
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
            let (definition, child) = world.child(closure)?;
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
            let payload = world.pointee(&captured)?;
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
            let payload = world.pointee(&owner)?;
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
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Err(bad("loan binding has no original observation directory"));
    };
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = instance.target
    else {
        return Err(bad("loan binding has no actual function"));
    };
    let function = world.function(function)?;
    world.pay(site_nodes.len().saturating_add(world.program.sites.len()))?;
    for node in site_nodes
        .iter()
        .filter(|row| row.node == SemanticOriginalNode::Expression(observed))
    {
        let recipe = world.original_recipe(original, node.site)?;
        if !matches!(&recipe.kind, SemanticSiteRecipeKind::TypeUse { expected: SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Expression(actual) } } if *actual == observed)
        {
            continue;
        }
        for site in &world.program.sites {
            if site.instance != instance.id || site.recipe != node.site {
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
    }
    found.ok_or_else(|| {
        bad("projected loan alias lacks its exact original declaration or emitted binding read")
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
    return_boundary: bool,
) -> R<()> {
    if return_boundary {
        world.pay(role.steps.len())?;
    }
    let input_referent = return_boundary
        && matches!(role.root, ViewBorrowRoot::Parameter(_))
        && role
            .steps
            .iter()
            .any(|step| matches!(step.kind, ViewBorrowStepKind::Parameter));
    let mut cursor = Cursor::Value(operand);
    world.pay(role.steps.len())?;
    let mut reached_input = false;
    let mut through_store = false;
    for step in role.steps.iter().rev() {
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
            ViewBorrowStepKind::CallResult { site } => {
                let Cursor::Value(value) = cursor else {
                    return Err(bad("call result loan has no real produced SSA"));
                };
                if role.root != ViewBorrowRoot::Result(step.node) {
                    return Err(bad("call result loan changes its original root expression"));
                }
                let (_, block, index) = producer(world, function, value)?;
                let recipe = world.original_recipe(original, *site)?;
                if !matches!(recipe.kind, SemanticSiteRecipeKind::OriginalCall) {
                    return Err(bad("call result loan is not an original Call"));
                }
                let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body
                else {
                    return Err(bad("call result loan has no original body"));
                };
                world.pay(site_nodes.len().saturating_add(world.program.sites.len()))?;
                if !site_nodes.iter().any(|row| row.site == *site && row.node == SemanticOriginalNode::Expression(step.node))
                    || !world.program.sites.iter().any(|row| row.instance == instance.id && row.recipe == *site
                        && matches!(row.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After } if actual == block && instruction as usize == index)
                        && matches!(row.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. } | SemanticSiteAction::CallableCall { .. })) {
                    return Err(bad("borrowed call result is not its exact checked original call result"));
                }
                // This value was produced by the independently checked actual
                // invocation. Never select one input as an alias for its result.
                world.get(values, value)?;
                reached_input = true;
            }
            ViewBorrowStepKind::Capture { name } => {
                if !matches!(role.root, ViewBorrowRoot::Capture(_)) {
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
                    Cursor::StoredField { .. } => {
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
                if role.root != ViewBorrowRoot::Local(*statement) {
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
                    Cursor::StoredField { .. } => false,
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
                    Cursor::StoredField { value, remaining } => (value, Some(remaining)),
                    Cursor::Local(_) => {
                        return Err(bad("projected loan field has no actual SSA projection"));
                    }
                };
                let operation = producer(world, function, value)?.0;
                if let InstructionKind::LoadStoredBorrow {
                    storage,
                    path,
                    mode,
                } = operation
                {
                    let count = remaining.unwrap_or(path.len());
                    if count == 0
                        || count > path.len()
                        || path.get(count - 1) != Some(&PlaceProjection::Field(*ordinal))
                    {
                        return Err(bad(
                            "stored field loan changes its complete original projection path",
                        ));
                    }
                    let storage_value = world.get(values, *storage)?;
                    let payload = world.pointee(&storage_value)?;
                    let parent = places::path(world, &payload, &path[..count - 1])?;
                    input_nominal(world, *definition, &parent)?;
                    if remaining.is_none() {
                        let selected = projection::place(world, &parent, path[count - 1])?;
                        let actual = world.get(values, value)?;
                        match (mode, World::type_of(&selected)?) {
                            (
                                StoredBorrowLoad::CopyShared,
                                Type::Address { mutable: false, .. },
                            )
                            | (StoredBorrowLoad::CopyDescription, Type::Desc(_)) => {
                                world.same(instance.id, &actual, &selected, implicit)?;
                            }
                            (
                                StoredBorrowLoad::Reborrow { mutable },
                                Type::Address {
                                    mutable: available, ..
                                },
                            ) if !*mutable || *available => {
                                places::borrow_mode(Some(World::type_of(&actual)?), *mutable)?;
                                let source = world.pointee(&selected)?;
                                let actual = world.pointee(&actual)?;
                                world.same(instance.id, &source, &actual, implicit)?;
                            }
                            _ => {
                                return Err(bad(
                                    "stored field loan changes its real read or reborrow authority",
                                ));
                            }
                        }
                    }
                    cursor = if count == 1 {
                        Cursor::Value(*storage)
                    } else {
                        Cursor::StoredField {
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
                    Cursor::StoredField { .. } => {
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
                world.pay(world.program.sites.len())?;
                if !world.program.sites.iter().any(|row| row.instance == instance.id && row.recipe == *site
                    && matches!(row.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After } if actual == block && instruction as usize == index)
                    && matches!(row.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. })) {
                    return Err(bad("projected Store loan changes its exact original call occurrence"));
                }
                cursor = Cursor::Value(store);
                through_store = true;
            }
            ViewBorrowStepKind::Alias { binding } => {
                let local = binding_local(world, instance, original, *binding, step.node)?;
                if !input_referent {
                    world.local(locals, local)?;
                }
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
            ViewBorrowStepKind::ReadLocal { binding } => {
                if role.root != ViewBorrowRoot::Local(*binding) {
                    return Err(bad(
                        "call loan read changes its original mutable local root",
                    ));
                }
                let local = binding_local(world, instance, original, *binding, step.node)?;
                let actual = world.local(locals, local)?;
                match cursor {
                    Cursor::Value(value) if matches!(producer(world, function, value)?.0, InstructionKind::Load(found) if *found == local) =>
                        {}
                    _ => {
                        return Err(bad(
                            "call loan does not read its real reaching mutable local",
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
            ViewBorrowStepKind::Parameter | ViewBorrowStepKind::ParameterStorage => {
                let ViewBorrowRoot::Parameter(parameter) = role.root else {
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
                if role.root != ViewBorrowRoot::Local(*binding) {
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
    values: &[Option<SharedType>],
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
            "call loan has ambiguous original occurrence correspondence",
        ));
    }
    let SemanticOriginalNode::Expression(node) = node.node else {
        return Ok(None);
    };
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
    let SemanticSiteAddress::Typed {
        block,
        instruction,
        edge: SemanticSiteEdge::After,
    } = site.at
    else {
        return Err(bad("call loan is not its actual post-result boundary"));
    };
    world.pay(function.blocks.len())?;
    let operand = function
        .blocks
        .iter()
        .find(|row| row.id == block)
        .and_then(|row| row.instructions.get(instruction as usize))
        .and_then(|op| op.result)
        .ok_or_else(|| bad("call loan has no exact emitted result SSA"))?;
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
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<(SharedType, roles::Source)>> {
    if !matches!(
        World::type_of(actual)?,
        Type::Address { .. } | Type::Desc(_)
    ) {
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
        for step in &role.steps {
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
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
    return_boundary: bool,
) -> R<Option<(SharedType, roles::Source)>> {
    let expected_source = roles::outer(world, expected)?;
    let target_source = match &role.target_lifetime {
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
    };
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
    for root in &roots {
        world.pay(1)?;
        if return_boundary {
            world.pay(role.steps.len())?;
        }
        // A loaded input pointer's slot can end before Return; its referent
        // loan belongs to the caller and is still the function's declared
        // input. Borrowing the pointer slot (ParameterStorage) never qualifies.
        let input_referent = return_boundary
            && role
                .steps
                .iter()
                .any(|step| matches!(step.kind, ViewBorrowStepKind::Parameter))
            && matches!(role.root, ViewBorrowRoot::Parameter(parameter)
                if function.parameters.get(parameter as usize) == Some(&root.local));
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
    values: &[Option<SharedType>],
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
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            return Err(bad("result view has no original expression directory"));
        };
        let mut selected = None;
        for site in sites {
            world.pay(1)?;
            if !matches!(site.action, SemanticSiteAction::TypeUse {
                subject: SemanticOperand::Value(value), ..
            } if value == operand)
            {
                continue;
            }
            world.pay(site_nodes.len())?;
            let mut nodes = site_nodes.iter().filter(|row| row.site == site.recipe);
            let Some(row) = nodes.next() else {
                continue;
            };
            if nodes.next().is_some() {
                return Err(bad(
                    "result view changes its exact original expression occurrence",
                ));
            }
            let SemanticOriginalNode::Expression(node) = row.node else {
                continue;
            };
            world.pay(index::search(proof.return_borrows.len()))?;
            let Some(role) = proof.return_borrows.get(&(original.id, node)) else {
                continue;
            };
            let frame = ledger
                .frames
                .get(&function.id)
                .ok_or_else(|| bad("result view has no exact actual emission owner"))?;
            // Original nodes are local to their immutable definition; the
            // emitter keys are actual Core mints. The existing relocated
            // TypeUse is the bridge between them, at the real end boundary
            // selected by lower::site, not by numeric NodeId equality.
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge: SemanticSiteEdge::Before,
            } = site.at
            else {
                return Err(bad(
                    "result view has no relocated actual expression boundary",
                ));
            };
            world.pay(index::search(world.index.expressions.len()))?;
            let emitted = world
                .index
                .expressions
                .get(&(function.id, operand))
                .ok_or_else(|| bad("result view loses its actual expression emission"))?;
            index::pay(world.left, emitted.len())?;
            let emitted = emitted.clone();
            let mut reaches_boundary = false;
            for mint in &emitted {
                world.pay(index::search(frame.expressions.len()).saturating_add(1))?;
                let expression = frame
                    .expressions
                    .get(mint)
                    .ok_or_else(|| bad("result view changes its actual emitter index"))?;
                if expression.value == Some(operand)
                    && expression.end.is_some_and(|end| {
                        end.block == block && end.instruction == instruction as usize
                    })
                {
                    reaches_boundary = true;
                }
            }
            if !reaches_boundary {
                return Err(bad(
                    "result view changes its relocated expression boundary or actual SSA",
                ));
            }
            if selected.replace((node, role)).is_some() {
                return Err(bad(
                    "result SSA has several original result-loan occurrences",
                ));
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
    values: &[Option<SharedType>],
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
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Err(bad("shared input view has no original read directory"));
    };
    let mut read = false;
    for site in sites {
        world.pay(1)?;
        if !matches!(site.action, SemanticSiteAction::TypeUse {
            subject: SemanticOperand::Value(value), ..
        } if value == parent)
        {
            continue;
        }
        world.pay(site_nodes.len())?;
        if site_nodes.iter().any(|row| {
            row.site == site.recipe && row.node == SemanticOriginalNode::Expression(source_node)
        }) {
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
    values: &[Option<SharedType>],
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
            .ok_or_else(|| bad("shared view lacks original Gamma"))?;
        world.pay(index::search(proof.shared_views.len()))?;
        if proof
            .shared_views
            .range(
                (original.id, Query::Expression(NodeId(0)))
                    ..=(original.id, Query::Expression(NodeId(u32::MAX))),
            )
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
    let mut rows = site_nodes.iter().filter(|row| row.site == site.recipe);
    let Some(row) = rows.next() else {
        return Ok(None);
    };
    if rows.next().is_some() {
        return Err(bad("shared view repeats its original expression"));
    }
    let SemanticOriginalNode::Expression(node) = row.node else {
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
    values: &[Option<SharedType>],
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
    for site in sites {
        world.pay(1)?;
        if !matches!(site.action, SemanticSiteAction::DeclareLocal {
            local: SemanticLocalTarget::Typed(target), ..
        } if target == local)
        {
            continue;
        }
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
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Ok(None);
    };
    world.pay(site_nodes.len())?;
    let recipes = site_nodes
        .iter()
        .filter(|row| row.node == SemanticOriginalNode::Expression(node))
        .map(|row| row.site)
        .collect::<Vec<_>>();
    world.pay(
        recipes
            .len()
            .saturating_add(index::search(world.index.sites.len())),
    )?;
    let Some(offsets) = world.index.sites.get(&instance.id) else {
        return Ok(None);
    };
    index::pay(world.left, offsets.len())?;
    let offsets = offsets.clone();
    let program = world.program;
    let mut selected = None;
    for offset in offsets {
        world.pay(recipes.len().saturating_add(1))?;
        let site = &program.sites[offset];
        if site.instance != instance.id || !recipes.contains(&site.recipe) {
            continue;
        }
        let SemanticSiteAction::TypeUse {
            subject: SemanticOperand::Value(value),
            ..
        } = site.action
        else {
            continue;
        };
        if selected.replace(value).is_some_and(|old| old != value) {
            return Err(bad("one original loan read has different actual SSA mints"));
        }
    }
    Ok(selected)
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
    values: &[Option<SharedType>],
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
    values: &[Option<SharedType>],
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
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            return Err(bad("call region has no complete original body"));
        };
        world.pay(index::search(proof.call_regions.len()))?;
        let node = if let Some(region) = proof.call_regions.get(&(original.id, ordinal)) {
            world.pay(
                site_nodes
                    .len()
                    .saturating_add(index::search(region.inputs.len())),
            )?;
            if !site_nodes.iter().any(|row| {
                row.site == site.recipe && row.node == SemanticOriginalNode::Expression(region.call)
            }) {
                return Ok(false);
            }
            *region
                .inputs
                .get(&parameter)
                .ok_or_else(|| bad("call region omits its original full-ordinal argument"))?
        } else {
            // A parameter may already be fixed to a mutable local's original
            // role by an invariant inner type. That role is not a fresh call
            // region: authenticate its own declaration and this exact input.
            world.pay(proof.local_roles.len().saturating_add(site_nodes.len()))?;
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
            let mut nodes = site_nodes.iter().filter(|row| row.site == site.recipe);
            let Some(row) = nodes.next() else {
                return Ok(false);
            };
            if nodes.next().is_some() {
                return Err(bad("contextual call has several original occurrences"));
            }
            let SemanticOriginalNode::Expression(call) = row.node else {
                return Ok(false);
            };
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
