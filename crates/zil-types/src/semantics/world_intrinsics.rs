//! Sealed language calls are selected by their real instruction, never a
//! fabricated function definition. Original schemes remain in the caller Gamma.
use super::*;
use crate::semantics::original::{IntrinsicCall, IntrinsicKind, RegistrationOp, TaskOp};

pub(super) fn evaluate(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    instruction: &InstructionKind,
    ty: Option<&Type>,
    values: &[Option<SemanticTypeValue>],
    locals: &[Option<SemanticTypeValue>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<SemanticTypeValue>> {
    let mut selected: Option<(IntrinsicCall, &SemanticSite)> = None;
    for site in world.point_sites(sites, block, index)? {
        let actuals = match &site.action {
            SemanticSiteAction::BodyCall { actuals } => Some(actuals.as_slice()),
            SemanticSiteAction::Construct { .. } => None,
            _ => continue,
        };
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("intrinsic has no checked original Gamma"))?;
        let Some(call) = proof.intrinsics.get(&(original.id, site.recipe)) else {
            continue;
        };
        if !matches!(
            world.original_recipe(original, site.recipe)?.kind,
            SemanticSiteRecipeKind::OriginalCall
        ) && !(matches!(call.kind, IntrinsicKind::NominalConstructor { .. })
            && matches!(
                world.original_recipe(original, site.recipe)?.kind,
                SemanticSiteRecipeKind::Construct { .. }
            ))
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
        if actuals.is_some_and(|actuals| recipes.len() != actuals.len()) {
            return Err(bad("sealed call omits complete mixed actuals"));
        }
        world.pay(recipes.len())?;
        for (recipe, actual) in recipes.iter().zip(actuals.into_iter().flatten()) {
            let expected = super::super::materialize::actual_at(
                world.program,
                world.module,
                world.ledger,
                instance.id,
                recipe,
                world.limits,
                world.left,
            )?;
            crate::dyn_metadata::semantic_operand_snapshot(actual, world.limits, world.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, world.limits, world.left, 1)?;
            if actual != &expected {
                return Err(bad(
                    "sealed call changes an independently solved mixed actual",
                ));
            }
        }
        world.pay(call.parameters.len().saturating_add(1))?;
        for parameter in &call.parameters {
            crate::dyn_metadata::semantic_operand_snapshot(parameter, world.limits, world.left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&call.result, world.limits, world.left, 1)?;
        if let IntrinsicKind::RegistrationStart { providers, .. } = &call.kind {
            world.pay(providers.len())?;
        }
        if let Some((signature, actuals)) = &call.worker_scheme {
            crate::dyn_metadata::semantic_operand_snapshot(signature, world.limits, world.left, 1)?;
            world.pay(actuals.len())?;
            for actual in actuals {
                crate::dyn_metadata::semantic_operand_snapshot(
                    actual,
                    world.limits,
                    world.left,
                    1,
                )?;
            }
        }
        selected = Some((call.clone(), site));
    }
    let Some((call, site)) = selected else {
        return Ok(None);
    };
    world.pay(call.parameters.len())?;
    identity(world, instance, &call, instruction, ty)?;
    let actual_effect = if matches!(
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
    world.pay(call.parameters.len().saturating_add(8))?;
    let operands = operand_ids(&call.kind, instruction)?;
    if operands.len() != call.parameters.len() {
        return Err(bad(
            "sealed intrinsic omits an original receiver or argument",
        ));
    }
    for (ordinal, (operand, parameter)) in operands.iter().zip(&call.parameters).enumerate() {
        let actual = world.get(values, *operand)?;
        let expected = world.ty(instance.id, parameter, World::type_of(&actual)?)?;
        arguments::satisfy(
            world, instance, None, site, ordinal, *operand, &actual, &expected, locals, implicit,
        )?;
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
                ))
            }
        };
        let callee = function_instance(world, function, definition)?;
        let function = world.function(function)?;
        methods::sealed_worker(
            world, instance, callee, function, signature, actuals, implicit,
        )?;
    }
    let ty = ty.ok_or_else(|| bad("sealed intrinsic omits its actual result type"))?;
    let result = world.ty(instance.id, &call.result, ty)?;
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

pub(super) fn operand_ids(kind: &IntrinsicKind, instruction: &InstructionKind) -> R<Vec<ValueId>> {
    Ok(match (kind, instruction) {
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
        (
            IntrinsicKind::RegistrationStart {
                asynchronous: false,
                ..
            },
            InstructionKind::RegistrationStart { callback, .. },
        )
        | (
            IntrinsicKind::RegistrationStart {
                asynchronous: true, ..
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
            ))
        }
    })
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
fn identity(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    call: &IntrinsicCall,
    instruction: &InstructionKind,
    ty: Option<&Type>,
) -> R<()> {
    match (&call.kind, instruction) {
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
            super::super::materialize::dynamic::owned_provider(
                world.program,
                world.module,
                world.ledger,
                *interface,
                *vtable,
                source_type,
                *route,
                world.limits,
                world.left,
            )?;
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
            let (_, child) = world.child(*closure)?;
            if child.definition != *expected_body {
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
            function_instance(world, *worker, *expected)?;
            let function = world.function(*worker)?;
            if function.call_kind != CallKind::Suspend || *await_result {
                return Err(bad("Task creation changes its actual worker Suspend ABI"));
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
            let mut parameters = Vec::new();
            for parameter in &function.parameters {
                let ty = &function
                    .locals
                    .get(parameter.0 as usize)
                    .ok_or_else(|| bad("Task worker input local is absent"))?
                    .ty;
                inference::charge_type_snapshot(ty, Span::default(), world.limits, world.left, 1)?;
                parameters.push(ty.clone());
            }
            if crate::tasks::retained_bindings(&parameters) != *port_bindings {
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
            function_instance(world, *function, *expected)?;
            let function = world.function(*function)?;
            if function.call_kind != CallKind::Suspend || ty != Some(&function.return_type) {
                return Err(bad("Task await changes its real worker ABI or result"));
            }
        }
        (
            IntrinsicKind::RegistrationStart {
                providers,
                asynchronous,
                threaded_capable,
            },
            InstructionKind::RegistrationStart {
                register, close, ..
            },
        ) => {
            if *asynchronous || providers.len() != 2 {
                return Err(bad(
                    "registration changes its synchronous original provider arity",
                ));
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
            let _ = threaded_capable;
        }
        (
            IntrinsicKind::RegistrationStart {
                providers,
                asynchronous,
                threaded_capable,
            },
            InstructionKind::AsyncRegistrationStart {
                register,
                request_close,
                poll_drain,
                ..
            },
        ) => {
            if !*asynchronous || providers.len() != 3 {
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
