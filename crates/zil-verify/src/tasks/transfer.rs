//! Actual frame migration qualification. A request byte never proves Transfer.
use super::*;
mod actual;
fn bad(span: zil_base::Span, message: &str) -> Diagnostic {
    reject("E-VERIFY-TASK-TRANSFER", Some(span), message)
}

pub(crate) fn task_owner(t: &Type) -> bool {
    matches!(t,Type::Managed(inner) if matches!(inner.as_ref(),Type::Task{..}))
}
/// Shape selection only. WithScopedThread independently checks complete result identity.
pub(crate) fn outcome_owner(t: &Type) -> Option<&Type> {
    let Type::Enum { name, variants } = t else {
        return None;
    };
    let [(failed, fields), (completed, results)] = variants.as_slice() else {
        return None;
    };
    let [owner] = fields.as_slice() else {
        return None;
    };
    (name.starts_with("@std.task:v1:TaskThreadScopeResult:")
        && failed == "Failed"
        && completed == "Completed"
        && results == &[Type::I32, Type::I32]
        && task_owner(owner))
    .then_some(owner)
}
fn data(
    module: &TypedModule,
    t: &Type,
    b: &mut Budget,
    drop_graph: &mut Vec<FunctionId>,
    span: zil_base::Span,
) -> Check<()> {
    resources::charge_payload(t, b)?;
    let mut pending = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = pending.pop() {
        b.spend(1)?;
        match t {
            Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64 => {}
            Type::Address { inner, .. } => pending.push(inner),
            Type::Managed(p) | Type::Desc(p) => match p.as_ref() {
                Type::ByteBuffer => {
                    b.spend(module.resources.len())?;
                    let mut rs = module.resources.iter().filter(|r| {
                        r.payload == Type::ByteBuffer || r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
                    });
                    let r = rs
                        .next()
                        .ok_or_else(|| bad(span, "Task Transfer ByteBuffer definition absent"))?;
                    if r.payload != Type::ByteBuffer
                        || r.name != zil_ir::BYTE_BUFFER_RESOURCE_NAME
                        || r.teardown.is_some()
                        || rs.next().is_some()
                    {
                        return Err(bad(span, "Task Transfer ByteBuffer cleanup not sealed"));
                    }
                }
                Type::Record { name, .. } => {
                    b.spend(
                        module
                            .resources
                            .len()
                            .saturating_mul(name.len().saturating_add(1)),
                    )?;
                    let mut found = None;
                    for r in &module.resources {
                        resources::charge_payload(&r.payload, b)?;
                        if r.name == *name || r.payload == **p {
                            if r.name != *name || r.payload != **p || found.is_some() {
                                return Err(bad(
                                    span,
                                    "Task Transfer resource identity is not canonical",
                                ));
                            }
                            found = Some(r);
                        }
                    }
                    let r = found.ok_or_else(|| bad(span, "Task Transfer resource absent"))?;
                    if let Some(drop) = r.teardown {
                        drop_graph.push(drop);
                    }
                    pending.push(p);
                }
                Type::Task { result, .. } => pending.push(result),
                Type::AsyncRegistration { environment } => pending.push(environment),
                _ => {
                    return Err(bad(
                        span,
                        "Task Transfer has an unproved owner/allocator dependency",
                    ));
                }
            },
            Type::Task { result, .. } => pending.push(result),
            Type::Closure(id) => {
                b.spend(closures.len().saturating_add(1))?;
                if closures.insert(*id) {
                    let c = module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| bad(span, "Task Transfer capture entry absent"))?;
                    b.spend(c.captures.len().saturating_add(c.lending_captures.len()))?;
                    if !c.lending_captures.is_empty() {
                        return Err(bad(
                            span,
                            "Task Transfer cannot carry a borrowed environment",
                        ));
                    }
                    pending.extend(&c.captures);
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            Type::Array { element, .. } | Type::Slice { element } => pending.push(element),
            _ => {
                return Err(bad(
                    span,
                    "Task Transfer retains Guard, TLS, code pointer or unknown data",
                ));
            }
        }
    }
    Ok(())
}
fn graph(module: &TypedModule, root: FunctionId, resume: bool, b: &mut Budget) -> Check<()> {
    let mut pending = vec![(root, resume)];
    let mut seen = BTreeSet::new();
    while let Some((id, worker)) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if !seen.insert((id, worker)) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| bad(zil_base::Span::default(), "Task Transfer entry absent"))?;
        if f.foreign.is_some()
            || f.environment.is_some()
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || (worker
                && (f.call_kind != CallKind::Suspend
                    || f.resume_bound != zil_ir::TaskResumeBound::Transfer))
            || (!worker && f.call_kind != CallKind::Direct)
        {
            return Err(bad(
                f.span,
                "Task Transfer requires actual qualified worker and synchronous helpers",
            ));
        }
        let mut cleanup = Vec::new();
        for t in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            data(module, t, b, &mut cleanup, f.span)?;
        }
        pending.extend(cleanup.into_iter().map(|id| (id, false)));
        for block in &f.blocks {
            for i in &block.instructions {
                b.spend(1)?;
                match &i.kind {
                    InstructionKind::Call { function, .. } => pending.push((*function, false)),
                    InstructionKind::SuspendCall { function, .. } => {
                        pending.push((*function, true))
                    }
                    InstructionKind::TaskCreate { worker, .. } => pending.push((*worker, true)),
                    InstructionKind::AsyncRegistrationStart {
                        register,
                        request_close,
                        poll_drain,
                        callback,
                        ..
                    } => {
                        for (id, role) in [
                            (*register, ScopedCallbackRole::ThreadAsyncRegister),
                            (*request_close, ScopedCallbackRole::ThreadAsyncRequestClose),
                            (*poll_drain, ScopedCallbackRole::ThreadAsyncPollDrain),
                        ] {
                            let actual = module
                                .functions
                                .get(id.0 as usize)
                                .and_then(|f| f.foreign.as_ref());
                            if !actual.is_some_and(|f| {
                                f.static_callbacks && f.thread_driver_transfer && f.scoped_callback == Some(role)
                            }) {
                                return Err(bad(
                                    i.span,
                                    "Task migration requires the actual threaded registration family",
                                ));
                            }
                        }
                        let Some(Type::Closure(id)) = f.values.get(callback.0 as usize) else {
                            return Err(bad(
                                i.span,
                                "Task threaded callback has no actual closure",
                            ));
                        };
                        crate::registration::transfer_callback(module, *id, Some(i.span), b)?;
                    }
                    InstructionKind::Constant(_)
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::TaskYield
                    | InstructionKind::TaskPoll { .. }
                    | InstructionKind::TaskValid { .. }
                    | InstructionKind::TaskRequestCancel { .. }
                    | InstructionKind::TaskPollClose { .. }
                    | InstructionKind::TaskFinishDestroy { .. }
                    | InstructionKind::MakeManaged { .. }
                    | InstructionKind::DropManaged { .. }
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::DescribeLocal { .. }
                    | InstructionKind::ReadDesc { .. }
                    | InstructionKind::WriteDesc { .. }
                    | InstructionKind::MakeOwnedTuple { .. }
                    | InstructionKind::DropTuple { .. }
                    | InstructionKind::UnpackTuple { .. }
                    | InstructionKind::TakeTupleField { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::MakePackage { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::TakePackageField { .. }
                    | InstructionKind::StorePackageField { .. }
                    | InstructionKind::DropPackageField { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::DescribePackageField { .. }
                    | InstructionKind::MakeVariant { .. }
                    | InstructionKind::VariantTag { .. }
                    | InstructionKind::ReadVariant { .. }
                    | InstructionKind::VariantTagLocal { .. }
                    | InstructionKind::ReadVariantLocal { .. }
                    | InstructionKind::UnpackVariant { .. }
                    | InstructionKind::TakeVariantField { .. }
                    | InstructionKind::DropEnum { .. }
                    | InstructionKind::MakeErrorUnion { .. }
                    | InstructionKind::ErrorTag { .. }
                    | InstructionKind::ReadErrorUnion { .. }
                    | InstructionKind::TakeErrorUnion { .. }
                    | InstructionKind::DropErrorUnion { .. }
                    | InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::BorrowFieldLocal { .. }
                    | InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::PlaceTagLocal { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::ReadAddress { .. }
                    | InstructionKind::WriteAddress { .. }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::AddressIndex { .. }
                    | InstructionKind::AddressSlice { .. }
                    | InstructionKind::ByteBufferNew
                    | InstructionKind::ByteBufferReserve { .. }
                    | InstructionKind::ByteBufferPush { .. }
                    | InstructionKind::ByteBufferTruncate { .. }
                    | InstructionKind::ByteBufferLen { .. }
                    | InstructionKind::ByteBufferCapacity { .. }
                    | InstructionKind::ByteBufferBorrow { .. }
                    | InstructionKind::MakeClosure { .. }
                    | InstructionKind::DropClosure { .. }
                    | InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. }
                    | InstructionKind::AsyncRegistrationValid { .. }
                    | InstructionKind::AsyncRegistrationRequestClose { .. }
                    | InstructionKind::AsyncRegistrationPollClose { .. }
                    | InstructionKind::AsyncRegistrationFinishDestroy { .. }
                    | InstructionKind::AsyncRegistrationFinishDestroyChecked { .. } => {}
                    _ => {
                        return Err(bad(
                            i.span,
                            "Task migration cannot execute an unproved foreign, indirect or thread dependency",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
// Migration qualification runs after Verify has reconstructed ownership. The
// early instruction check establishes shape only, without certifying a frame.
pub(super) fn declared(_module: &TypedModule, _b: &mut Budget) -> Check<()> { Ok(()) }
pub(crate) fn thread_argument(
    _module: &TypedModule, caller: &TypedFunction, argument: ValueId,
    _driver: FunctionId, _b: &mut Budget, span: zil_base::Span,
) -> Check<()> {
    if !caller.values.get(argument.0 as usize).is_some_and(task_owner) {
        return Err(bad(span, "Task thread consumes one whole owner"));
    }
    Ok(())
}
pub(crate) fn finish(module: &TypedModule, b: &mut Budget) -> Check<()> {
    actual::finish(module,b)
}
