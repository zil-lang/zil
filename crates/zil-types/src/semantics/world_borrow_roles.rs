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
) -> R<()> {
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
                let expected = world.ty(
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
                through_store = true;
            }
            ViewBorrowStepKind::Alias { binding } => {
                let local = binding_local(world, instance, original, *binding, step.node)?;
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
    let expected = world.ty(
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

/// A directed call loan preserves the source's original complete sigma.
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
    let role = {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("call region lost its complete original proof"))?;
        world.pay(index::search(proof.call_regions.len()))?;
        let Some(region) = proof.call_regions.get(&(original.id, ordinal)) else {
            return Ok(false);
        };
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            return Err(bad("call region has no complete original body"));
        };
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
        let node = region
            .inputs
            .get(&parameter)
            .ok_or_else(|| bad("call region omits its original full-ordinal argument"))?;
        world.pay(index::search(proof.borrow_roles.len()))?;
        let Some(role) = proof.borrow_roles.get(&(original.id, *node)) else {
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
        role.clone()
    };
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = caller.target
    else {
        return Err(bad("call loan has no actual function"));
    };
    let function = world.function(function)?;
    trace(
        world, caller, original, function, &role, operand, values, locals, implicit,
    )?;
    Ok(true)
}
