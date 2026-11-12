//! Actual immutable closure scopes for Task input structure. A known scope
//! supplies class ordinals only: roots, phase, cleanup and migration remain
//! obligations of their separate executable consumers.
use super::*;
use zil_base::Span;
use zil_ir::{ClosureDefinition, ClosureId, StorageLifeContract};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Domain {
    Unknown,
    Known(u32),
}
pub(crate) struct Table {
    definitions: Vec<ClosureDefinition>,
    domains: Vec<Domain>,
}

fn conjunction(domains: impl IntoIterator<Item = Domain>) -> Domain {
    if domains.into_iter().all(|d| d == Domain::Known(0)) {
        Domain::Known(0)
    } else {
        // A bare product has no class domain of its own. Nonzero children
        // need its actual qualified physical class mapping, including phantom
        // classes; concatenating their ordinals would invent that mapping.
        Domain::Unknown
    }
}
fn walk(
    ty: &Type,
    b: &mut Budget,
    closure: &mut impl FnMut(ClosureId, &mut Budget) -> CompileResult<Domain>,
) -> CompileResult<Domain> {
    b.spend(1, "actual Task input scope node")?;
    Ok(match ty {
        Type::Closure(id) => return closure(*id, b),
        Type::BorrowedStorage { life, .. } => Domain::Known(life.binder_count),
        Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
            Domain::Known(retained_life.binder_count)
        }
        Type::BorrowedFunctionPointer { life, .. } => Domain::Known(life.fixed_binder_count),
        Type::Managed(inner)
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner } => return walk(inner, b, closure),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            let mut children = Vec::new();
            b.spend(fields.len(), "actual Task Product scope children")?;
            for (_, field) in fields {
                children.push(walk(field, b, closure)?);
            }
            conjunction(children)
        }
        Type::Tuple(fields) => {
            let mut children = Vec::new();
            b.spend(fields.len(), "actual Task Tuple scope children")?;
            for field in fields {
                children.push(walk(field, b, closure)?);
            }
            conjunction(children)
        }
        Type::Array { element, .. }
        | Type::OwnedStore { element }
        | Type::Mutex { payload: element } => conjunction([walk(element, b, closure)?]),
        Type::ErrorUnion { error, result } => {
            conjunction([walk(error, b, closure)?, walk(result, b, closure)?])
        }
        Type::Enum { variants, .. } => {
            let mut children = Vec::new();
            b.spend(variants.len(), "actual Task Sum scope alternatives")?;
            for (_, fields) in variants {
                b.spend(fields.len(), "actual Task Sum scope children")?;
                for field in fields {
                    children.push(walk(field, b, closure)?);
                }
            }
            conjunction(children)
        }
        Type::Unit
        | Type::Bool
        | Type::ForeignHandle
        | Type::CWideString
        | Type::ByteBuffer
        | Type::SyncCell(_)
        | Type::TaskI32
        | Type::FunctionPointer { .. }
        | Type::ForeignCallback { .. } => Domain::Known(0),
        number if number.is_integer() || number.is_float() => Domain::Known(0),
        _ => Domain::Unknown,
    })
}

/// Prepay the complete actual definitions before cloning or recursive lookup.
/// The stored function identity is checked as well as its public signature.
pub(crate) fn build(module: &TypedModule, limits: &Limits, b: &mut Budget) -> CompileResult<Table> {
    if module.closures.len() > limits.max_nodes || module.closures.len() > u32::MAX as usize {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "Task input scope inventory exceeds limit",
            None,
        )]);
    }
    b.spend(
        module.closures.len().saturating_mul(4),
        "Task actual scope table",
    )?;
    for (index, d) in module.closures.iter().enumerate() {
        if d.captures
            .len()
            .saturating_add(d.parameters.len())
            .saturating_add(d.lending_captures.len())
            > limits.max_nodes
        {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "Task actual scope fields exceed limit",
                None,
            )]);
        }
        b.spend(
            d.lending_captures.len(),
            "Task full actual lending scope snapshot",
        )?;
        b.lookup(module.functions.len(), "Task actual closure entry")?;
        let f = module
            .functions
            .get(d.function.0 as usize)
            .ok_or_else(|| fail(Span::default(), "Task closure scope entry is absent"))?;
        if d.id.0 as usize != index
            || f.id != d.function
            || f.environment != Some(d.id)
            || f.parameters.len() != d.parameters.len()
        {
            return Err(fail(
                f.span,
                "Task closure scope has a different actual entry",
            ));
        }
        for ty in d
            .captures
            .iter()
            .chain(&d.parameters)
            .chain(std::iter::once(&d.result))
        {
            crate::stored_flow::shape_work(ty, limits, b)?;
            b.type_work(ty, true, "Task actual closure scope snapshot")?;
        }
        for (formal, expected) in f.parameters.iter().zip(&d.parameters) {
            let actual = f
                .locals
                .get(formal.0 as usize)
                .ok_or_else(|| fail(f.span, "Task closure scope parameter is absent"))?;
            crate::stored_flow::shape_work(&actual.ty, limits, b)?;
            b.type_work(
                &actual.ty,
                true,
                "Task actual scope entry parameter equality",
            )?;
            if actual.ty != *expected {
                return Err(fail(
                    f.span,
                    "Task closure scope parameter differs from entry",
                ));
            }
        }
        crate::stored_flow::shape_work(&f.return_type, limits, b)?;
        b.type_work(
            &f.return_type,
            true,
            "Task actual scope entry result equality",
        )?;
        if let Some(actual) = &f.storage_life {
            charge_contract(actual, limits, b)?;
        }
        if f.return_type != d.result || f.storage_life != d.storage_life {
            return Err(fail(
                f.span,
                "Task closure scope relation differs from entry",
            ));
        }
        if let Some(c) = &d.storage_life {
            charge_contract(c, limits, b)?;
            charge_contract(c, limits, b)?;
        }
    }
    let definitions = module.closures.clone();
    let mut domains = vec![Domain::Unknown; definitions.len()];
    let mut visiting = vec![0u8; definitions.len()];
    fn resolve(
        index: ClosureId,
        defs: &[ClosureDefinition],
        domains: &mut [Domain],
        marks: &mut [u8],
        depth: usize,
        limits: &Limits,
        b: &mut Budget,
    ) -> CompileResult<Domain> {
        if depth > limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "Task input scope depth exceeded",
                None,
            )]);
        }
        b.lookup(defs.len(), "Task closure scope dependency")?;
        let ordinal = index.0 as usize;
        let Some(d) = defs.get(ordinal) else {
            return Ok(Domain::Unknown);
        };
        if marks[ordinal] == 2 {
            return Ok(domains[ordinal]);
        }
        if marks[ordinal] == 1 {
            return Ok(Domain::Unknown);
        }
        marks[ordinal] = 1;
        let result = if let Some(c) = &d.storage_life {
            // Full rows are retained in the snapshot, including phantom fixed
            // classes. The ordinary closure relation and class flow still
            // authenticate each actual capture and source independently.
            if c.fixed_binder_count == 0 {
                let mut captures = Vec::new();
                b.spend(d.captures.len(), "Task actual zero-fixed captured scopes")?;
                for ty in &d.captures {
                    captures.push(walk(ty, b, &mut |id, b| {
                        resolve(id, defs, domains, marks, depth + 1, limits, b)
                    })?);
                }
                conjunction(captures)
            } else {
                Domain::Known(c.fixed_binder_count)
            }
        } else {
            let mut children = Vec::new();
            b.spend(
                d.captures
                    .len()
                    .saturating_add(d.parameters.len())
                    .saturating_add(1),
                "Task inferred closed scope dependencies",
            )?;
            for ty in &d.captures {
                children.push(walk(ty, b, &mut |id, b| {
                    resolve(id, defs, domains, marks, depth + 1, limits, b)
                })?);
            }
            fn signature(
                ty: &Type,
                b: &mut Budget,
                closure: &mut impl FnMut(ClosureId, &mut Budget) -> CompileResult<Domain>,
            ) -> CompileResult<Domain> {
                match ty {
                    Type::Address { inner, .. } => Ok(conjunction([walk(inner, b, closure)?])),
                    Type::Tuple(fields) => {
                        b.spend(fields.len(), "Task actual invocation scope children")?;
                        let mut results = Vec::new();
                        for field in fields {
                            results.push(signature(field, b, closure)?);
                        }
                        Ok(conjunction(results))
                    }
                    _ => walk(ty, b, closure),
                }
            }
            for ty in d.parameters.iter().chain(std::iter::once(&d.result)) {
                children.push(signature(ty, b, &mut |id, b| {
                    resolve(id, defs, domains, marks, depth + 1, limits, b)
                })?);
            }
            conjunction(children)
        };
        domains[ordinal] = result;
        marks[ordinal] = 2;
        Ok(result)
    }
    for index in 0..definitions.len() {
        resolve(
            ClosureId(index as u32),
            &definitions,
            &mut domains,
            &mut visiting,
            0,
            limits,
            b,
        )?;
    }
    Ok(Table {
        definitions,
        domains,
    })
}
fn charge_contract(c: &StorageLifeContract, limits: &Limits, b: &mut Budget) -> CompileResult<()> {
    if c.fixed_binder_count as usize > limits.max_nodes
        || c.invocation_binder_count as usize > limits.max_nodes
        || c.bindings.len().saturating_add(c.classes.len()) > limits.max_nodes
        || c.bindings
            .iter()
            .any(|r| r.position.path.len() > limits.max_call_depth)
        || c.classes
            .iter()
            .any(|r| r.position.path.len() > limits.max_call_depth)
    {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "Task actual scope relation exceeds limit",
            None,
        )]);
    }
    crate::signature_life::charge(c, b)
}
pub(crate) fn domain(ty: &Type, b: &mut Budget) -> CompileResult<Domain> {
    b.type_work(ty, true, "Task full input scope identity")?;
    walk(ty, b, &mut |id, b| {
        let count = b
            .task_input_scopes
            .as_ref()
            .map_or(0, |table| table.definitions.len());
        b.lookup(count, "Task full actual closure scope lookup")?;
        Ok(b.task_input_scopes
            .as_ref()
            .and_then(|table| table.domains.get(id.0 as usize))
            .copied()
            .unwrap_or(Domain::Unknown))
    })
}

/// Only an actual zero-domain owner is opaque to stored-target transport. The
/// registration signature, actual constructor/provider, phase and owner debts
/// are still checked by their independent consumers.
pub(crate) fn registration_operation(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.task_sources() {
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
) -> CompileResult<bool> {
    if !b.task_sources()
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
    let Some(role) = candidate.foreign.as_ref().and_then(|f| f.scoped_callback) else {
        return Ok(false);
    };
    crate::foreign_scoped::declaration(candidate, role, b)?;
    for f in &module.functions {
        b.spend(
            f.blocks.len(),
            "actual Task registration provider inventory",
        )?;
        for i in f.blocks.iter().flat_map(|block| &block.instructions) {
            b.spend(1, "actual Task registration provider inventory")?;
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
