//! Verifier-local actual closure scope inventory. This worklist reconstructs
//! immutable class domains; it never supplies ownership, loans or retirement.
use super::*;
use zil_ir::ClosureDefinition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Domain {
    Unknown,
    Known(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingScope {
    Waiting,
    Unknown,
    Known(u32),
}
pub(crate) struct Table {
    definitions: Vec<ClosureDefinition>,
    domains: Vec<Domain>,
}

// A complete opaque scope owns its class domain. Unqualified containers only
// establish zero when all children have zero domains; they cannot merge or
// alpha-rename nonzero children without actual parent class rows.
fn probe(
    ty: &Type,
    states: Option<&[PendingScope]>,
    signature: bool,
    b: &mut Budget,
) -> Check<PendingScope> {
    let mut stack = vec![(ty, false, signature)];
    let mut result = 0;
    let mut waiting = false;
    while let Some((t, require_zero, signature)) = stack.pop() {
        b.spend(1)?;
        let counted = match t {
            Type::Closure(id) => {
                let length = states.map_or_else(
                    || {
                        b.task_input_scopes
                            .as_ref()
                            .map_or(0, |s| s.definitions.len())
                    },
                    |s| s.len(),
                );
                b.spend(length.saturating_add(1))?;
                if let Some(states) = states {
                    states
                        .get(id.0 as usize)
                        .copied()
                        .unwrap_or(PendingScope::Unknown)
                } else {
                    match b
                        .task_input_scopes
                        .as_ref()
                        .and_then(|s| s.domains.get(id.0 as usize))
                        .copied()
                        .unwrap_or(Domain::Unknown)
                    {
                        Domain::Unknown => PendingScope::Unknown,
                        Domain::Known(n) => PendingScope::Known(n),
                    }
                }
            }
            Type::BorrowedStorage { life, .. } => PendingScope::Known(life.binder_count),
            Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
                PendingScope::Known(retained_life.binder_count)
            }
            Type::BorrowedFunctionPointer { life, .. } => {
                PendingScope::Known(life.fixed_binder_count)
            }
            Type::Managed(inner)
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner } => {
                stack.push((inner, require_zero, false));
                continue;
            }
            Type::Address { inner, .. } if signature => {
                stack.push((inner, true, false));
                continue;
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                stack.extend(fields.iter().map(|t| (t, true, signature)));
                continue;
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                stack.extend(fields.iter().map(|(_, t)| (t, true, false)));
                continue;
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Mutex { payload: element } => {
                stack.push((element, true, false));
                continue;
            }
            Type::ErrorUnion { error, result } => {
                stack.extend([
                    (error.as_ref(), true, false),
                    (result.as_ref(), true, false),
                ]);
                continue;
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    stack.extend(fields.iter().map(|t| (t, true, false)));
                }
                continue;
            }
            Type::Unit
            | Type::Bool
            | Type::ForeignHandle
            | Type::CWideString
            | Type::ByteBuffer
            | Type::TaskI32
            | Type::SyncCell(_)
            | Type::FunctionPointer { .. }
            | Type::ForeignCallback { .. } => PendingScope::Known(0),
            scalar if scalar.is_integer() || scalar.is_float() => PendingScope::Known(0),
            _ => PendingScope::Unknown,
        };
        match counted {
            PendingScope::Unknown => return Ok(PendingScope::Unknown),
            PendingScope::Waiting => waiting = true,
            PendingScope::Known(n) => {
                if require_zero && n != 0 {
                    return Ok(PendingScope::Unknown);
                }
                if !require_zero {
                    result = n;
                }
            }
        }
    }
    Ok(if waiting {
        PendingScope::Waiting
    } else {
        PendingScope::Known(result)
    })
}

pub(crate) fn build(module: &TypedModule, limits: &Limits, b: &mut Budget) -> Check<Table> {
    let count = module.closures.len();
    if count > limits.max_nodes || count > u32::MAX as usize {
        return Err(limit("actual Task input scope count exceeded"));
    }
    b.spend(count.saturating_mul(5))?;
    let mut states = vec![PendingScope::Waiting; count];
    // Pay and bind the full actual declaration and entry before cloning any
    // scope candidate or starting the dependency worklist.
    for (ordinal, d) in module.closures.iter().enumerate() {
        if d.captures
            .len()
            .saturating_add(d.parameters.len())
            .saturating_add(d.lending_captures.len())
            > limits.max_nodes
        {
            return Err(limit("actual Task input scope fields exceeded"));
        }
        b.spend(
            module
                .functions
                .len()
                .saturating_add(d.lending_captures.len())
                .saturating_add(3),
        )?;
        let f = module.functions.get(d.function.0 as usize).ok_or_else(|| {
            reject(
                "E-VERIFY-TASK",
                None,
                "actual scope closure entry is absent",
            )
        })?;
        if d.id.0 as usize != ordinal
            || f.id != d.function
            || f.environment != Some(d.id)
            || d.parameters.len() != f.parameters.len()
        {
            return Err(reject(
                "E-VERIFY-TASK",
                None,
                "actual closure scope entry identity differs",
            ));
        }
        for t in d
            .captures
            .iter()
            .chain(&d.parameters)
            .chain(std::iter::once(&d.result))
        {
            crate::stored_flow::type_work(t, limits, b)?;
            crate::resources::charge_payload(t, b)?;
        }
        for (parameter, expected) in f.parameters.iter().zip(&d.parameters) {
            let actual = f
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| reject("E-VERIFY-TASK", None, "scope entry parameter is absent"))?;
            crate::stored_flow::type_work(&actual.ty, limits, b)?;
            if actual.ty != *expected {
                return Err(reject(
                    "E-VERIFY-TASK",
                    None,
                    "actual scope parameter type differs",
                ));
            }
        }
        crate::stored_flow::type_work(&f.return_type, limits, b)?;
        for contract in [&d.storage_life, &f.storage_life].into_iter().flatten() {
            if contract.fixed_binder_count as usize > limits.max_nodes
                || contract.invocation_binder_count as usize > limits.max_nodes
                || contract
                    .bindings
                    .len()
                    .saturating_add(contract.classes.len())
                    > limits.max_nodes
                || contract
                    .bindings
                    .iter()
                    .any(|r| r.position.path.len() > limits.max_call_depth)
                || contract
                    .classes
                    .iter()
                    .any(|r| r.position.path.len() > limits.max_call_depth)
            {
                return Err(limit("actual Task input scope relation exceeded"));
            }
            crate::signature_life::charge(contract, b)?;
        }
        if f.return_type != d.result || f.storage_life != d.storage_life {
            return Err(reject(
                "E-VERIFY-TASK",
                None,
                "actual scope result or complete relation differs",
            ));
        }
        if let Some(contract) = &d.storage_life
            && contract.fixed_binder_count != 0
        {
            states[ordinal] = PendingScope::Known(contract.fixed_binder_count);
        }
    }
    let definitions = module.closures.clone();
    // Independently solve forward references to closed environments. Waiting
    // cycles become Unknown; absence of a relation is never directly zero.
    let mut frontier: std::collections::VecDeque<usize> = (0..count)
        .filter(|&i| states[i] == PendingScope::Waiting)
        .collect();
    let mut progress = true;
    while progress && !frontier.is_empty() {
        progress = false;
        let round = frontier.len();
        b.spend(round.saturating_add(1))?;
        for _ in 0..round {
            let i = frontier.pop_front().expect("bounded scope frontier");
            let d = &definitions[i];
            b.spend(
                d.captures
                    .len()
                    .saturating_add(d.parameters.len())
                    .saturating_add(1),
            )?;
            let mut outcome = PendingScope::Known(0);
            let mut scopes = Vec::new();
            for (ordinal, ty) in d.captures.iter().enumerate() {
                let constant = if let Some(contract) = &d.storage_life {
                    b.spend(contract.static_bindings.len())?;
                    matches!(ty, Type::Address { mutable: false, .. })
                        && contract.static_bindings.iter().any(|position| {
                            position.base == StorageBase::Capture(ordinal as u32)
                                && position.path.is_empty()
                        })
                } else {
                    false
                };
                // Exclude the exact outer occurrence, retaining every inner
                // class check. Source/ancestor Static proof remains a global
                // all-constructor and all-write obligation, never this bit.
                scopes.push((ty, constant));
            }
            if d.storage_life.is_none() {
                scopes.extend(
                    d.parameters
                        .iter()
                        .chain(std::iter::once(&d.result))
                        .map(|ty| (ty, true)),
                );
            }
            for (ty, signature) in scopes {
                match probe(ty, Some(&states), signature, b)? {
                    PendingScope::Unknown | PendingScope::Known(1..) => {
                        outcome = PendingScope::Unknown;
                        break;
                    }
                    PendingScope::Waiting => outcome = PendingScope::Waiting,
                    PendingScope::Known(0) => {}
                }
            }
            if outcome == PendingScope::Waiting {
                frontier.push_back(i);
            } else {
                states[i] = outcome;
                progress = true;
            }
        }
    }
    b.spend(count)?;
    let domains = states
        .into_iter()
        .map(|state| match state {
            PendingScope::Known(n) => Domain::Known(n),
            PendingScope::Unknown | PendingScope::Waiting => Domain::Unknown,
        })
        .collect();
    Ok(Table {
        definitions,
        domains,
    })
}
pub(crate) fn domain(ty: &Type, b: &mut Budget) -> Check<Domain> {
    crate::resources::charge_payload(ty, b)?;
    Ok(match probe(ty, None, false, b)? {
        PendingScope::Known(n) => Domain::Known(n),
        PendingScope::Waiting | PendingScope::Unknown => Domain::Unknown,
    })
}

/// Only an actual zero-domain owner is opaque to stored-target transport. The
/// registration signature, actual constructor/provider, phase and owner debts
/// are still checked by their independent consumers.
pub(crate) fn registration_operation(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.task_failure_carriers {
        return Ok(false);
    }
    let actual = match &i.kind {
        InstructionKind::AsyncRegistrationStart { .. } => {
            i.result.and_then(|r| f.values.get(r.0 as usize))
        }
        InstructionKind::AsyncRegistrationValid { registration }
        | InstructionKind::AsyncRegistrationRequestClose { registration }
        | InstructionKind::AsyncRegistrationPollClose { registration }
        | InstructionKind::AsyncRegistrationFinishDestroy { registration }
        | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
            f.values.get(registration.0 as usize)
        }
        _ => return Ok(false),
    };
    let Some(actual) = actual else {
        return Ok(false);
    };
    let actual = if let Type::Address { inner, .. } = actual {
        inner.as_ref()
    } else {
        actual
    };
    if !matches!(actual,Type::Managed(payload) if matches!(payload.as_ref(),Type::AsyncRegistration{environment} if matches!(environment.as_ref(),Type::Closure(_))))
    {
        return Ok(false);
    }
    Ok(domain(actual, b)? == Domain::Known(0))
}

pub(crate) fn provider(
    module: &TypedModule,
    candidate: &TypedFunction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.task_failure_carriers
        || !candidate.foreign.as_ref().is_some_and(|f| {
            matches!(
                f.scoped_callback,
                Some(
                    zil_ir::ScopedCallbackRole::AsyncRegister
                        | zil_ir::ScopedCallbackRole::AsyncRequestClose
                        | zil_ir::ScopedCallbackRole::AsyncPollDrain
                        | zil_ir::ScopedCallbackRole::ThreadAsyncRegister
                        | zil_ir::ScopedCallbackRole::ThreadAsyncRequestClose
                        | zil_ir::ScopedCallbackRole::ThreadAsyncPollDrain
                )
            )
        })
    {
        return Ok(false);
    }
    crate::scoped_callback::declaration(candidate, b)?;
    for f in &module.functions {
        b.spend(f.blocks.len())?;
        for i in f.blocks.iter().flat_map(|block| &block.instructions) {
            b.spend(1)?;
            if let InstructionKind::AsyncRegistrationStart {
                register,
                request_close,
                poll_drain,
                ..
            } = i.kind
                && [register, request_close, poll_drain].contains(&candidate.id)
                && registration_operation(f, i, b)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
