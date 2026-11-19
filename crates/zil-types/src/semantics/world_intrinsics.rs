//! Sealed language calls are selected by their real instruction, never a
//! fabricated function definition. Original schemes remain in the caller Gamma.
use super::*;
use crate::semantics::original::{IntrinsicCall, IntrinsicKind, RegistrationOp, TaskOp};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ActualSite {
    representative: SemanticInstanceId,
    recipe: SemanticSiteId,
    block: BlockId,
    instruction: u32,
}

fn mixed_actuals<'p>(
    world: &mut World<'p, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    site: &'p SemanticSite,
    proof: &super::super::original::Proof,
    recipes: &[SemanticActualRecipe],
    actuals: &'p [SemanticActual],
) -> R<()> {
    if actuals.len() != recipes.len() || site.instance != instance.id || original.id != instance.definition {
        return Err(bad("sealed call changes its exact actual world or complete mixed arguments"));
    }
    world.pay(recipes.len())?;
    if actuals.is_empty() {
        return Ok(());
    }
    world.pay(index::search(world.index.type_contexts.len()))?;
    let context = world.index.type_contexts.get(&instance.id).copied();
    let key = match (context, &site.at) {
        (Some(context), SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::After })
            if context.definition == original.id => Some(ActualSite {
                representative: context.representative,
                recipe: site.recipe,
                block: *block,
                instruction: *instruction,
            }),
        _ => None,
    };
    let prior = if let Some(key) = key {
        world.pay(index::search(world.intrinsic_actuals.len()))?;
        world.intrinsic_actuals.get(&key).copied()
    } else {
        None
    };
    if let Some(Some(confirmed)) = prior {
        world.pay(1)?;
        if confirmed.len() == actuals.len() {
            let mut equal = true;
            for (confirmed, actual) in confirmed.iter().zip(actuals) {
                crate::dyn_metadata::semantic_operand_snapshot(confirmed, world.limits, world.left, 1)?;
                if confirmed != actual {
                    equal = false;
                    break;
                }
            }
            if equal {
                return Ok(());
            }
        }
    }
    for (recipe, actual) in recipes.iter().zip(actuals) {
        let expected = super::super::materialize::actual_in_view(
            world.program, &world.view, world.ledger, instance.id, recipe, world.limits, world.left,
        )?;
        crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&expected, world.limits, world.left, 1)?;
        if actual != &expected {
            return Err(bad("sealed call changes an independently solved mixed actual"));
        }
    }
    if let Some(key) = key && prior.is_none() {
        // No original callee result, invocation, creator or enclosing world
        // may influence a reusable proof. Complete actuals are compared even
        // on a hit; the actual operand and all live class equations follow.
        let mut portable = true;
        for recipe in recipes {
            let SemanticActualRecipe::Type(recipe) = recipe else {
                portable = false;
                break;
            };
            if !type_contexts::portable(proof, original.id, recipe, world.limits, world.left, 0)? {
                portable = false;
                break;
            }
        }
        world.pay(index::search(world.intrinsic_actuals.len()).saturating_add(1))?;
        world.intrinsic_actuals.insert(key, portable.then_some(actuals));
    }
    Ok(())
}

pub(super) fn evaluate<'p>(
    world: &mut World<'p, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    instruction: &InstructionKind,
    ty: Option<&Type>,
    values: &ssa::Values,
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<SharedType>> {
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let mut selected = None;
    for site in world.point_sites(sites, block, index)? {
        let actuals = match &site.action {
            SemanticSiteAction::BodyCall { actuals } => Some(actuals.as_slice()),
            SemanticSiteAction::Construct { .. } => None,
            _ => continue,
        };
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("intrinsic has no checked original Gamma"))?;
        let Some(call) = proof.intrinsics.get(&(original.id, site.recipe)) else {
            continue;
        };
        let kind = &world.original_recipe(original, site.recipe)?.kind;
        if !(matches!(kind, SemanticSiteRecipeKind::OriginalCall)
            || matches!(call.kind, IntrinsicKind::NominalConstructor { .. })
                && matches!(kind, SemanticSiteRecipeKind::Construct { .. }))
        {
            return Err(bad("sealed call changes its original operation recipe"));
        }
        if selected.is_some() {
            return Err(bad("sealed call repeats its original scheme"));
        }
        let recipes = proof
            .calls
            .get(&(original.id, site.recipe))
            .ok_or_else(|| bad("sealed call has no full original mixed scheme solution"))?;
        if let Some(actuals) = actuals {
            mixed_actuals(world, instance, original, site, proof, recipes, actuals)?;
        }
        selected = Some((call, site));
    }
    let Some((call, site)) = selected else {
        return Ok(None);
    };
    world.pay(call.parameters.len())?;
    identity(world, instance, site, call, instruction, ty, implicit)?;
    let actual_effect = if let IntrinsicKind::DynCall { call_kind, .. } = &call.kind {
        *call_kind
    } else if matches!(
        instruction,
        InstructionKind::TaskYield | InstructionKind::SuspendCall { .. }
    ) {
        CallKind::Suspend
    } else {
        CallKind::Direct
    };
    if call.effect != actual_effect {
        return Err(bad(
            "actual sealed opcode changes its independently proved original effect",
        ));
    }
    world.pay(operand_work(instruction))?;
    let operands = operand_ids(&call.kind, instruction)?;
    if operands.len() != call.parameters.len() {
        return Err(bad(
            "sealed intrinsic omits an original receiver or argument",
        ));
    }
    world.pay(call.parameters.len())?;
    let mut parameters = Vec::new();
    for (ordinal, (operand, parameter)) in operands.iter().zip(&call.parameters).enumerate() {
        let actual = world.get(values, *operand)?;
        if ordinal == 0
            && let Some(manager) = call.temporary_manager
        {
            temporary_manager_operand(world, instance, original, manager, *operand, parameter, values)?;
            parameters.push(actual);
            continue;
        }
        let expected = world.physical_type(instance.id, parameter, World::type_of(&actual)?)?;
        if matches!(call.kind, IntrinsicKind::NominalConstructor { .. })
            && borrow_roles::payload_view(world, instance, original, site, ordinal, *operand,
                &actual, &expected, values, locals, implicit)?
        {
            parameters.push(expected);
            continue;
        }
        arguments::satisfy(
            world, instance, None, site, ordinal, *operand, &actual, &expected, values, locals,
            implicit,
        )?;
        parameters.push(expected);
    }
    if let Some((signature, actuals)) = &call.worker_scheme {
        let (function, definition) = match (&call.kind, instruction) {
            (
                IntrinsicKind::ThreadScope {
                    worker: definition, ..
                },
                InstructionKind::WithScopedThread { worker, .. },
            )
            | (
                IntrinsicKind::TaskCreate {
                    worker: definition, ..
                },
                InstructionKind::TaskCreate { worker, .. }
                | InstructionKind::TaskCreateOwned { worker, .. },
            ) => (*worker, *definition),
            (
                IntrinsicKind::TaskCreate {
                    worker: definition, ..
                },
                InstructionKind::SuspendCall { function, .. },
            ) => (*function, *definition),
            _ => {
                return Err(bad(
                    "sealed worker receipt belongs to another actual opcode",
                ));
            }
        };
        let callee = if matches!(call.kind, IntrinsicKind::TaskCreate { .. }) {
            task_worker_instance(world, instance, site, function, definition)?
        } else {
            function_instance(world, function, definition)?
        };
        let function = world.function(function)?;
        methods::sealed_worker(
            world, instance, callee, function, signature, actuals, implicit,
        )?;
    }
    let ty = ty.ok_or_else(|| bad("sealed intrinsic omits its actual result type"))?;
    let result = world.physical_type(instance.id, &call.result, ty)?;
    if matches!(&call.kind, IntrinsicKind::DynCall { .. }) {
        dynamic::call(
            world,
            instance,
            call,
            site,
            instruction,
            &parameters,
            &result,
            implicit,
        )?;
    }
    if let SemanticSiteAction::Construct {
        result: subject,
        value: claimed,
    } = &site.action
    {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(bad("constructor is outside a real Typed function"));
        };
        let function = world.function(function)?;
        let actual = function
            .blocks
            .iter()
            .find(|row| row.id == block)
            .and_then(|row| row.instructions.get(index))
            .and_then(|op| op.result)
            .ok_or_else(|| bad("constructor site has no actual SSA result"))?;
        if subject != &SemanticOperand::Value(actual) {
            return Err(bad(
                "constructor original site changes its actual result SSA",
            ));
        }
        world.same(instance.id, claimed, &result, implicit)?;
    }
    if World::type_of(&result)? != ty {
        return Err(bad(
            "sealed intrinsic changes its complete actual result type",
        ));
    }
    Ok(Some(result))
}

pub(super) fn operand_work(instruction: &InstructionKind) -> usize {
    match instruction {
        InstructionKind::MakeVariant { fields, .. } => fields.len().saturating_add(1),
        InstructionKind::TaskCreate { arguments, .. }
        | InstructionKind::TaskCreateOwned { arguments, .. }
        | InstructionKind::SuspendCall { arguments, .. } => arguments.len().saturating_add(1),
        InstructionKind::CallDynSlot { arguments, .. } => arguments.len().saturating_add(2),
        _ => 8,
    }
}
pub(super) fn operand_ids(kind: &IntrinsicKind, instruction: &InstructionKind) -> R<Vec<ValueId>> {
    Ok(match (kind, instruction) {
        (IntrinsicKind::ForeignCallback { .. }, InstructionKind::ForeignFunctionRef { .. }) => {
            Vec::new()
        }
        (
            IntrinsicKind::DynCall { .. },
            InstructionKind::CallDynSlot {
                callee, arguments, ..
            },
        ) => {
            let mut operands = Vec::with_capacity(arguments.len().saturating_add(1));
            operands.push(*callee);
            operands.extend_from_slice(arguments);
            operands
        }
        (
            IntrinsicKind::NominalConstructor {
                variant: expected, ..
            },
            InstructionKind::MakeVariant { variant, fields },
        ) if expected == variant => fields.clone(),
        (IntrinsicKind::MutexCreate, InstructionKind::MutexCreate { value }) => vec![*value],
        (IntrinsicKind::MutexLock, InstructionKind::MutexLock { mutex }) => vec![*mutex],
        (IntrinsicKind::MutexValue, InstructionKind::MutexValue { guard })
        | (IntrinsicKind::MutexUnlock, InstructionKind::MutexUnlock { guard }) => vec![*guard],
        (IntrinsicKind::StoreNew, InstructionKind::OwnedStoreNew { .. })
        | (IntrinsicKind::BytesNew, InstructionKind::ByteBufferNew)
        | (IntrinsicKind::TaskYield, InstructionKind::TaskYield) => Vec::new(),
        (IntrinsicKind::StoreReserve, InstructionKind::OwnedStoreReserve { store, capacity }) => {
            vec![*store, *capacity]
        }
        (IntrinsicKind::StorePush, InstructionKind::OwnedStorePush { store, value }) => {
            vec![*store, *value]
        }
        (IntrinsicKind::StorePop, InstructionKind::OwnedStorePop { store })
        | (IntrinsicKind::StoreLen, InstructionKind::OwnedStoreLen { store })
        | (IntrinsicKind::StoreCapacity, InstructionKind::OwnedStoreCapacity { store }) => {
            vec![*store]
        }
        (
            IntrinsicKind::StoreBorrow { mutable: false },
            InstructionKind::OwnedStoreBorrow { store, index },
        )
        | (
            IntrinsicKind::StoreBorrow { mutable: true },
            InstructionKind::OwnedStoreBorrowMut { store, index },
        ) => vec![*store, *index],
        (IntrinsicKind::BytesReserve, InstructionKind::ByteBufferReserve { buffer, capacity }) => {
            vec![*buffer, *capacity]
        }
        (IntrinsicKind::BytesTruncate, InstructionKind::ByteBufferTruncate { buffer, length }) => {
            vec![*buffer, *length]
        }
        (IntrinsicKind::BytesPush, InstructionKind::ByteBufferPush { buffer, value }) => {
            vec![*buffer, *value]
        }
        (IntrinsicKind::BytesLen, InstructionKind::ByteBufferLen { buffer })
        | (IntrinsicKind::BytesCapacity, InstructionKind::ByteBufferCapacity { buffer }) => {
            vec![*buffer]
        }
        (
            IntrinsicKind::BytesBorrow { mutable: expected },
            InstructionKind::ByteBufferBorrow { buffer, mutable },
        ) if expected == mutable => vec![*buffer],
        (IntrinsicKind::Length, InstructionKind::SliceLength { address }) => vec![*address],
        (IntrinsicKind::SyncCellScope(expected), InstructionKind::WithSyncCell { kind, initial, body })
            if expected == kind => vec![*initial, *body],
        (IntrinsicKind::SyncCellOp(_, expected), InstructionKind::SyncI32 { op, address, value, ordering })
            if expected == op && *ordering == AtomicOrdering::SeqCst => {
                let mut operands = vec![*address];
                operands.extend(value);
                operands
            }
        (IntrinsicKind::SyncCellCompareExchange, InstructionKind::SyncI32CompareExchange { address, expected, desired, success, failure })
            if *success == AtomicOrdering::SeqCst && *failure == AtomicOrdering::SeqCst => vec![*address, *expected, *desired],
        (
            IntrinsicKind::Numeric { target: expected },
            InstructionKind::ConvertInteger { value, target }
            | InstructionKind::ConvertNumber { value, target },
        ) if expected == target => vec![*value],
        (
            IntrinsicKind::ThreadScope { .. },
            InstructionKind::WithScopedThread { argument, body, .. },
        ) => vec![*argument, *body],
        (
            IntrinsicKind::TaskCreate {
                owned: false,
                await_result: false,
                ..
            },
            InstructionKind::TaskCreate { arguments, .. },
        )
        | (
            IntrinsicKind::TaskCreate {
                owned: true,
                await_result: false,
                ..
            },
            InstructionKind::TaskCreateOwned { arguments, .. },
        )
        | (
            IntrinsicKind::TaskCreate {
                await_result: true, ..
            },
            InstructionKind::SuspendCall { arguments, .. },
        ) => arguments.clone(),
        (IntrinsicKind::TaskOperation(TaskOp::Valid), InstructionKind::TaskValid { task })
        | (IntrinsicKind::TaskOperation(TaskOp::Poll), InstructionKind::TaskPoll { task })
        | (
            IntrinsicKind::TaskOperation(TaskOp::RequestCancel),
            InstructionKind::TaskRequestCancel { task },
        )
        | (
            IntrinsicKind::TaskOperation(TaskOp::PollClose),
            InstructionKind::TaskPollClose { task },
        )
        | (
            IntrinsicKind::TaskOperation(TaskOp::FinishDestroy),
            InstructionKind::TaskFinishDestroy { task },
        ) => vec![*task],
        (IntrinsicKind::RegistrationStart { form: CallbackRegistration::Scoped, .. },
            InstructionKind::WithForeignCallback { callback, body, .. }) => vec![*callback, *body],
        (
            IntrinsicKind::RegistrationStart {
                form: CallbackRegistration::Owned,
                ..
            },
            InstructionKind::RegistrationStart { callback, .. },
        )
        | (
            IntrinsicKind::RegistrationStart {
                form: CallbackRegistration::AsyncOwned, ..
            },
            InstructionKind::AsyncRegistrationStart { callback, .. },
        ) => vec![*callback],
        (
            IntrinsicKind::RegistrationOperation(RegistrationOp::Valid),
            InstructionKind::RegistrationValid { registration },
        )
        | (
            IntrinsicKind::RegistrationOperation(RegistrationOp::Close),
            InstructionKind::RegistrationClose { registration },
        )
        | (
            IntrinsicKind::RegistrationOperation(RegistrationOp::AsyncValid),
            InstructionKind::AsyncRegistrationValid { registration },
        )
        | (
            IntrinsicKind::RegistrationOperation(RegistrationOp::AsyncRequest),
            InstructionKind::AsyncRegistrationRequestClose { registration },
        )
        | (
            IntrinsicKind::RegistrationOperation(RegistrationOp::AsyncPollClose),
            InstructionKind::AsyncRegistrationPollClose { registration },
        )
        | (
            IntrinsicKind::RegistrationOperation(RegistrationOp::AsyncFinish),
            InstructionKind::AsyncRegistrationFinishDestroy { registration },
        )
        | (
            IntrinsicKind::RegistrationOperation(RegistrationOp::AsyncFinishChecked),
            InstructionKind::AsyncRegistrationFinishDestroyChecked { registration },
        ) => vec![*registration],
        (IntrinsicKind::OwnedDyn, InstructionKind::MakeDynOwned { source, .. }) => vec![*source],
        _ => {
            return Err(bad(
                "actual opcode differs from its complete original sealed scheme",
            ));
        }
    })
}
fn temporary_manager_operand(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    source: zil_base::NodeId,
    descriptor: ValueId,
    expected: &SemanticTypeRecipe,
    values: &ssa::Values,
) -> R<()> {
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target else {
        return Err(bad("prepared ByteBuffer has no actual function"));
    };
    let function = world.function(id)?;
    let manager = occurrences::expression_value(world, instance, original, source)?
        .ok_or_else(|| bad("prepared ByteBuffer lost its exact original producing expression"))?;
    let (InstructionKind::DescribeLocal { local }, block, description) =
        borrow_roles::producer(world, function, descriptor)?
    else {
        return Err(bad("prepared ByteBuffer requires its actual temporary description"));
    };
    let before = description.checked_sub(1)
        .and_then(|index| function.blocks.get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|actual| actual.instructions.get(index)));
    if !matches!(before.map(|i| &i.kind), Some(InstructionKind::Store { local: dest, value })
        if dest == local && *value == manager)
    {
        return Err(bad("temporary description does not select its exact original manager store"));
    }
    let value = world.get(values, manager)?;
    let physical = World::type_of(&value)?;
    let wanted = world.physical_type(instance.id, expected, physical)?;
    let description = world.get(values, descriptor)?;
    if !matches!(physical, Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer)
        || !matches!(World::type_of(&description)?, Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer)
        || !function.locals.get(local.0 as usize).is_some_and(|declaration|
            declaration.id == *local && &declaration.ty == physical)
    {
        return Err(bad("temporary ByteBuffer changes its complete actual categories"));
    }
    world.same(instance.id, &value, &wanted, &BTreeMap::new())
}


fn function_instance<'a>(
    world: &mut World<'a, '_>,
    function: FunctionId,
    definition: SemanticDefinitionId,
) -> R<&'a SemanticInstance> {
    world.pay(world.program.instances.len())?;
    let mut instances=world.program.instances.iter().filter(|instance|matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function));
    let instance = instances
        .next()
        .ok_or_else(|| bad("sealed provider has no real actual declaration world"))?;
    if instances.next().is_some() || instance.definition != definition {
        return Err(bad(
            "sealed provider changes its unique original declaration",
        ));
    }
    Ok(instance)
}
/// Code sharing does not share a Task's original caller world. Generated
/// workers are selected by the actual creation request; a nonrequested
/// ordinary worker retains the existing unique declaration instance.
fn task_worker_instance<'a>(
    world: &mut World<'a, '_>,
    caller: &SemanticInstance,
    site: &SemanticSite,
    function: FunctionId,
    definition: SemanticDefinitionId,
) -> R<&'a SemanticInstance> {
    let ledger = world.ledger;
    world.pay(world.program.instances.len())?;
    let mut requested = None;
    let mut ordinary = None;
    let mut ordinary_count = 0usize;
    for instance in &world.program.instances {
        if !matches!(instance.target,
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(actual)) if actual == function)
        {
            continue;
        }
        world.pay(index::search(ledger.instance_requests.borrow().len()))?;
        let request = ledger.instance_requests.borrow().get(&instance.id).copied();
        if request == Some((caller.id, site.recipe, function)) {
            if instance.definition != definition || requested.replace(instance).is_some() {
                return Err(bad(
                    "Task worker request has conflicting complete original instances",
                ));
            }
        } else if request.is_none() && instance.definition == definition {
            ordinary = Some(instance);
            ordinary_count = ordinary_count.saturating_add(1);
        }
    }
    if let Some(instance) = requested {
        return Ok(instance);
    }
    if ordinary_count == 1 {
        return Ok(ordinary.expect("one nonrequested original worker counted"));
    }
    Err(bad(
        "Task worker lacks its exact actual caller request or unique ordinary declaration",
    ))
}
fn identity(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    site: &SemanticSite,
    call: &IntrinsicCall,
    instruction: &InstructionKind,
    ty: Option<&Type>,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    match (&call.kind, instruction) {
        (
            IntrinsicKind::ForeignCallback { target },
            InstructionKind::ForeignFunctionRef { function },
        ) => {
            let SemanticTypeRecipe::Code {
                kind: SemanticCodeKind::ForeignCallback,
                signature,
            } = &call.result
            else {
                return Err(bad(
                    "callback formation loses its complete original C signature",
                ));
            };
            let actual = world.function(*function)?;
            if actual.foreign.is_some()
                || actual.environment.is_some()
                || actual.retained_return_sources.is_some()
                || actual.callable_contract.is_some()
                || actual.callable_return_contract.is_some()
                || actual.call_kind != CallKind::Direct
                || !actual.protocol_contracts.is_empty()
                || !matches!(ty, Some(Type::ForeignCallback { parameters, result }) if crate::foreign::callback_signature(parameters, result))
            {
                return Err(bad(
                    "callback formation changes the actual ordinary target or finite C ABI",
                ));
            }
            world.pay(world.program.instances.len())?;
            let mut count = 0usize;
            for callee in &world.program.instances {
                if !matches!(callee.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == *function)
                {
                    continue;
                }
                if callee.definition != *target
                    || callee.parent.is_some()
                    || !callee.actuals.is_empty()
                    || callee.environment.is_some()
                    || callee.self_type.is_some()
                    || !matches!(
                        callee.lifetime_instantiation,
                        SemanticLifetimeInstantiation::Rigid
                    )
                {
                    return Err(bad(
                        "callback formation changes its exact original target or captures a specialization environment",
                    ));
                }
                methods::sealed_worker(world, instance, callee, actual, signature, &[], implicit)?;
                count += 1;
            }
            if count == 0 {
                return Err(bad(
                    "callback target has no authentic original executable instance",
                ));
            }
        }

        (
            IntrinsicKind::OwnedDyn,
            InstructionKind::MakeDynOwned {
                interface,
                vtable,
                source,
                route,
            },
        ) => {
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
                instance.target
            else {
                return Err(bad("owned dyn constructor lacks a real Typed caller"));
            };
            let source_type = world
                .function(function)?
                .values
                .get(source.0 as usize)
                .ok_or_else(|| bad("owned dyn source SSA has no actual complete type"))?;
            if !matches!(ty, Some(Type::ErrorUnion { result, .. }) if matches!(result.as_ref(), Type::OwnedDyn { interface: actual } if actual == interface))
            {
                return Err(bad(
                    "owned dyn opcode changes its exact actual result interface",
                ));
            }
            world.check_owned_provider(*interface, *vtable, source_type, *route)?;
        }
        (
            IntrinsicKind::NominalConstructor {
                definition,
                variant,
            },
            InstructionKind::MakeVariant {
                variant: actual, ..
            },
        ) => {
            let original = world.definition(*definition)?;
            if *variant != *actual
                || !matches!(&original.body, SemanticDefinitionBody::Nominal { body: SemanticNominalBody::Enum { variants }, .. } if (*variant as usize) < variants.len())
            {
                return Err(bad(
                    "enum constructor changes its actual original variant identity",
                ));
            }
        }
        (IntrinsicKind::StoreNew, InstructionKind::OwnedStoreNew { element }) => {
            let Some(Type::Managed(inner)) = ty else {
                return Err(bad("Store construction has no actual owning result"));
            };
            if !matches!(inner.as_ref(),Type::OwnedStore{element:expected}if expected.as_ref()==element)
            {
                return Err(bad(
                    "Store opcode changes its complete original element type",
                ));
            }
        }
        (
            IntrinsicKind::ThreadScope {
                worker: expected,
                body: expected_body,
            },
            InstructionKind::WithScopedThread { worker, body, .. },
        ) => {
            function_instance(world, *worker, *expected)?;
            let function = world.function(*worker)?;
            if function.call_kind != CallKind::Direct || function.return_type != Type::I32 {
                return Err(bad("thread worker changes its original Direct/i32 ABI"));
            }
            let value=world.module.functions.iter().find(|function|matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function.id)).and_then(|function|function.values.get(body.0 as usize)).ok_or_else(||bad("thread body operand has no actual type"))?;
            let Type::Closure(closure) = value else {
                return Err(bad("thread body is not its actual closure operand"));
            };
            let (_, children) = world.child_worlds(*closure)?;
            world.pay(children.len())?;
            if children.iter().any(|child| child.definition != *expected_body) {
                return Err(bad(
                    "thread operation selects another original body closure",
                ));
            }
        }
        (
            IntrinsicKind::TaskCreate {
                worker: expected,
                await_result,
                ..
            },
            InstructionKind::TaskCreate {
                worker,
                port_bindings,
                ..
            }
            | InstructionKind::TaskCreateOwned {
                worker,
                port_bindings,
                ..
            },
        ) => {
            let callee = task_worker_instance(world, instance, site, *worker, *expected)?;
            let function = world.function(*worker)?;
            let (signature, _) = call.worker_scheme.as_ref().ok_or_else(|| {
                bad("Task creation lacks its complete original worker ABI scheme")
            })?;
            if function.call_kind != signature.call_kind || *await_result {
                return Err(bad("Task creation changes its actual original worker ABI"));
            }
            let Some(Type::ErrorUnion { result, .. }) = ty else {
                return Err(bad(
                    "Task creation changes its complete fallible owning result",
                ));
            };
            let Type::Managed(task) = result.as_ref() else {
                return Err(bad("Task success result is not an owner"));
            };
            if !matches!(task.as_ref(),Type::Task{result,..}if result.as_ref()==&function.return_type)
            {
                return Err(bad(
                    "Task worker result differs from its complete actual payload",
                ));
            }
            world.pay(function.parameters.len())?;
            let mut bindings = Vec::new();
            for ordinal in 0..function.parameters.len() {
                let parameter = u32::try_from(ordinal)
                    .map_err(|_| bad("Task worker parameter ordinal overflows"))?;
                let actual = &function
                    .locals
                    .get(function.parameters[ordinal].0 as usize)
                    .ok_or_else(|| bad("Task worker input local is absent"))?
                    .ty;
                let value = world.parameter_value(callee.id, ordinal, actual)?;
                let SemanticTypeValue::Resolved { ty, life, .. } = value.as_ref() else {
                    return Err(bad(
                        "Task worker port retains an unresolved full parameter domain",
                    ));
                };
                let projected = crate::tasks::context_ports::from_type(
                    &world.view,
                    ty,
                    life,
                    world.limits,
                    world.left,
                )?;
                world.pay(projected.len())?;
                for projected in projected {
                    if bindings.len() >= world.limits.max_nodes {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "Task complete worker port occurrence limit exceeded",
                            None,
                        ));
                    }
                    let port = u32::try_from(bindings.len())
                        .map_err(|_| bad("Task worker port ordinal overflows"))?;
                    bindings.push(RetainedPortBinding {
                        port,
                        source: StoragePosition {
                            base: StorageBase::Parameter(parameter),
                            path: projected.path,
                        },
                    });
                }
            }
            if bindings != *port_bindings {
                return Err(bad(
                    "Task opcode changes complete actual worker port/input bindings",
                ));
            }
        }
        (
            IntrinsicKind::TaskCreate {
                worker: expected,
                await_result: true,
                ..
            },
            InstructionKind::SuspendCall { function, .. },
        ) => {
            task_worker_instance(world, instance, site, *function, *expected)?;
            let function = world.function(*function)?;
            if function.call_kind != CallKind::Suspend || ty != Some(&function.return_type) {
                return Err(bad("Task await changes its real worker ABI or result"));
            }
        }
        (IntrinsicKind::RegistrationStart { providers, form: CallbackRegistration::Scoped, .. },
            InstructionKind::WithForeignCallback { register, close, .. })
        | (IntrinsicKind::RegistrationStart { providers, form: CallbackRegistration::Owned, .. },
            InstructionKind::RegistrationStart { register, close, .. }) => {
            if providers.len() != 2 {
                return Err(bad("registration changes its synchronous original provider arity"));
            }
            for (id, definition, role) in [
                (*register, providers[0], ScopedCallbackRole::Register),
                (*close, providers[1], ScopedCallbackRole::Close),
            ] {
                function_instance(world, id, definition)?;
                let foreign = world.function(id)?.foreign.as_ref().ok_or_else(|| {
                    bad("registration provider is not its actual external declaration")
                })?;
                if foreign.abi != "C"
                    || !foreign.static_callbacks
                    || foreign.scoped_callback != Some(role)
                {
                    return Err(bad(
                        "registration changes its exact external C protocol role",
                    ));
                }
            }
        }
        (
            IntrinsicKind::RegistrationStart {
                providers,
                form: CallbackRegistration::AsyncOwned,
                threaded_capable,
            },
            InstructionKind::AsyncRegistrationStart {
                register,
                request_close,
                poll_drain,
                ..
            },
        ) => {
            if providers.len() != 3 {
                return Err(bad(
                    "registration changes its asynchronous original provider arity",
                ));
            }
            function_instance(world, *register, providers[0])?;
            let threaded = world
                .function(*register)?
                .foreign
                .as_ref()
                .and_then(|foreign| foreign.scoped_callback)
                == Some(ScopedCallbackRole::ThreadAsyncRegister);
            if threaded && !*threaded_capable {
                return Err(bad(
                    "thread registration lacks original Transfer and ThreadCallable proofs",
                ));
            }
            let roles = if threaded {
                [
                    ScopedCallbackRole::ThreadAsyncRegister,
                    ScopedCallbackRole::ThreadAsyncRequestClose,
                    ScopedCallbackRole::ThreadAsyncPollDrain,
                ]
            } else {
                [
                    ScopedCallbackRole::AsyncRegister,
                    ScopedCallbackRole::AsyncRequestClose,
                    ScopedCallbackRole::AsyncPollDrain,
                ]
            };
            for ((id, definition), role) in [*register, *request_close, *poll_drain]
                .into_iter()
                .zip(providers)
                .zip(roles)
            {
                function_instance(world, id, *definition)?;
                let foreign = world.function(id)?.foreign.as_ref().ok_or_else(|| {
                    bad("asynchronous provider is not its actual external declaration")
                })?;
                if foreign.abi != "C"
                    || !foreign.static_callbacks
                    || foreign.scoped_callback != Some(role)
                {
                    return Err(bad(
                        "asynchronous registration changes its exact actual protocol roles",
                    ));
                }
            }
        }
        _ => {}
    }
    Ok(())
}
