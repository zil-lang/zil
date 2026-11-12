//! Actual frame migration qualification. A request byte never proves Transfer.
use super::*;
mod constructed;
fn bad(span: zil_base::Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-TASK-TRANSFER",
        Some(span),
        message,
    )]
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
) -> CompileResult<()> {
    b.type_work(t, false, "Task Transfer complete data")?;
    let mut pending = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = pending.pop() {
        b.spend(1, "Task Transfer data")?;
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
                    b.spend(module.resources.len(), "Task Transfer sealed allocator")?;
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
                        "Task Transfer resource identity",
                    )?;
                    let mut found = None;
                    for r in &module.resources {
                        b.type_work(&r.payload, false, "Task Transfer cleanup candidate")?;
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
                b.lookup(closures.len(), "Task Transfer capture visit")?;
                if closures.insert(*id) {
                    let c = module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| bad(span, "Task Transfer capture entry absent"))?;
                    b.spend(
                        c.captures.len().saturating_add(c.lending_captures.len()),
                        "Task Transfer actual captures",
                    )?;
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
                b.spend(fields.len(), "Task Transfer fields")?;
                pending.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len(), "Task Transfer tuple")?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len(), "Task Transfer variants")?;
                for (_, fields) in variants {
                    b.spend(fields.len(), "Task Transfer fields")?;
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
fn graph(
    module: &TypedModule,
    root: FunctionId,
    resume: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut pending = vec![(root, resume)];
    let mut seen = BTreeSet::new();
    while let Some((id, worker)) = pending.pop() {
        b.lookup(seen.len(), "Task Transfer actual graph")?;
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
                b.spend(1, "Task Transfer actual operation")?;
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
                        crate::registration::transfer_callback(module, *id, b, i.span)?;
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
pub(super) fn declared(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    for f in &module.functions {
        if f.resume_bound == zil_ir::TaskResumeBound::Transfer {
            graph(module, f.id, true, b)?;
        }
    }
    Ok(())
}
pub(crate) fn thread_argument(
    module: &TypedModule,
    caller: &TypedFunction,
    argument: ValueId,
    driver: FunctionId,
    b: &mut Budget,
    span: zil_base::Span,
) -> CompileResult<()> {
    if !task_owner(
        caller
            .values
            .get(argument.0 as usize)
            .ok_or_else(|| bad(span, "Task thread payload absent"))?,
    ) {
        return Err(bad(span, "Task thread consumes one whole owner"));
    }
    constructed::argument(module,caller,argument,b)?;
    graph(module, driver, false, b)
}

// A weak external description does not transfer exclusive storage, nor does it
// retain the source loan. Frame-internal descriptions are checked in graph().
fn external_input(t:&Type,b:&mut Budget,span:zil_base::Span)->CompileResult<()> {
    b.type_work(t,false,"Task migration actual input")?;
    let mut work=vec![t];
    while let Some(t)=work.pop(){
        b.spend(1,"Task migration external input")?;
        match t {
            Type::Desc(_)=>return Err(bad(span,"external weak description has no cross-thread storage loan")),
            Type::Managed(t)|Type::Address{inner:t,..}|Type::Array{element:t,..}=>work.push(t),
            Type::Tuple(ts)=>work.extend(ts),
            Type::Record{fields,..}|Type::Package{fields,..}=>work.extend(fields.iter().map(|(_,t)|t)),
            Type::Enum{variants,..}=>{for (_,ts) in variants {work.extend(ts);}},
            Type::ErrorUnion{error,result}=>work.extend([error.as_ref(),result.as_ref()]),
            _=>{}
        }
    }Ok(())
}
