//! Capability routing for a complete ordinary callback graph whose interface
//! may be Suspend while all execution and cleanup is synchronously closed.
//! No ABI is rewritten. Real suspension, open indirect calls and execution
//! protocols need their own kernel. Owned data and stored leaves retain all
//! actual resource, target, loan and cleanup obligations in this lane.
use super::*;

fn data(module: &TypedModule, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(failure("synchronous callback data depth exceeded"));
    }
    b.type_work(ty, false, "synchronous callback complete data")?;
    if ty.is_b0_scalar() || *ty == Type::ForeignHandle {
        return Ok(true);
    }
    Ok(match ty {
        Type::Address { inner, .. }
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner } => data(module, inner, b, depth + 1)?,
        Type::BorrowedStorage { storage, .. } => {
            // This selects the full stored-target solver; it does not erase
            // physical slots, nominal arguments or the complete child domain.
            // preflight/header/class/actual-flow each still check those rows.
            data(module, storage, b, depth + 1)?
        }
        Type::Managed(_) | Type::Desc(_) | Type::Package { .. } => {
            super::owned::ordinary_value(module, ty, b)?
        }
        Type::Record { fields, .. } => {
            let mut complete = true;
            for (_, field) in fields {
                complete &= data(module, field, b, depth + 1)?;
            }
            complete
        }
        Type::Tuple(fields) => {
            let mut complete = true;
            for field in fields {
                complete &= data(module, field, b, depth + 1)?;
            }
            complete
        }
        Type::Enum { variants, .. } => {
            let mut complete = true;
            for (_, fields) in variants {
                for field in fields {
                    complete &= data(module, field, b, depth + 1)?;
                }
            }
            complete
        }
        Type::ErrorUnion { error, result } => {
            data(module, error, b, depth + 1)? && data(module, result, b, depth + 1)?
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            ordered(b, module.dyn_interfaces.len(), 0)?;
            let mut found = module.dyn_interfaces.iter().filter(|entry| entry.id == *interface);
            let Some(entry) = found.next() else { return Ok(false); };
            if found.next().is_some() { return Ok(false); }
            if matches!(ty, Type::OwnedDyn { .. }) {
                super::super::dyn_cleanup::capture_interface(module, *interface, b)?;
            }
            match &entry.context {
                Some(context) => data(module, context, b, depth + 1)?,
                None => entry.retained_life.binder_count == 0
                    && entry.retained_life.ports.is_empty()
                    && entry.retained_life.retained.is_empty()
                    && entry.retained_life.nominal_arguments.is_empty(),
            }
        }
        Type::Closure(id) => {
            ordered(b, b.semantic_closures_proven.len(), 0)?;
            b.semantic_closures_proven.contains(id)
                && module
                    .closures
                    .get(id.0 as usize)
                    .is_some_and(|d| d.id == *id)
        }
        // Task/registration protocols, opaque shells and unresolved/open
        // code pointers do not acquire synchronous capability from their type.
        _ => false,
    })
}
fn closure_entry(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<Option<FunctionId>> {
    let ty = match ty {
        Type::Address { inner, .. } => inner.as_ref(),
        _ => ty,
    };
    let Type::Closure(id) = ty else {
        return Ok(None);
    };
    ordered(b, b.semantic_closures_proven.len(), 0)?;
    let Some(closure) = module.closures.get(id.0 as usize).filter(|d| d.id == *id) else {
        return Ok(None);
    };
    if !b.semantic_closures_proven.contains(id) {
        return Ok(None);
    }
    let Some(entry) = module
        .functions
        .get(closure.function.0 as usize)
        .filter(|f| f.id == closure.function)
    else {
        return Ok(None);
    };
    Ok((entry.environment == Some(*id)
        && entry.call_kind == CallKind::Direct
        && entry.foreign.is_none())
    .then_some(entry.id))
}

pub(super) fn module(module: &TypedModule, b: &mut Budget) -> CompileResult<bool> {
    if module.semantic.is_none()
        || module.closures.is_empty() && module.dyn_vtables.is_empty() {
        return Ok(false);
    }
    for closure in &module.closures {
        ordered(b, b.semantic_closures_proven.len(), 0)?;
        if !b.semantic_closures_proven.contains(&closure.id) || !closure.lending_captures.is_empty()
        {
            return Ok(false);
        }
        // Captured nested F is closed only after every actual definition in
        // this same loop passed; an open callable parameter is not evidence.
        for ty in &closure.captures {
            if !data(module, ty, b, 0)? {
                return Ok(false);
            }
        }
        let Some(entry) = module.functions.get(closure.function.0 as usize) else {
            return Ok(false);
        };
        if entry.environment != Some(closure.id)
            || entry.call_kind != CallKind::Direct
            || entry.foreign.is_some()
        {
            return Ok(false);
        }
    }
    // A dynamic slot remains an erased call with its own complete Context
    // and current permissions. Only fully checked actual tables whose entries
    // all keep the declared Direct machine ABI belong to this finite graph.
    for table in &module.dyn_vtables {
        ordered(b, b.semantic_dyn_tables_proven.len(), 0)?;
        if !b.semantic_dyn_tables_proven.contains(&table.id)
            || !data(module, &table.self_type, b, 0)? {
            return Ok(false);
        }
        for entry in table.entries.iter().chain(&table.cleanup) {
            fee(b, 1)?;
            let Some(target) = module.functions.get(entry.function.0 as usize)
                .filter(|target| target.id == entry.function) else { return Ok(false); };
            if target.call_kind != CallKind::Direct || target.foreign.is_some() {
                return Ok(false);
            }
        }
    }
    // Cleanup edges are part of the same actual graph, not a consequence of
    // a callback kind or the presence of a resource name in its type.
    for resource in &module.resources {
        fee(b, 1)?;
        super::owned::teardown(module, resource, b)?;
    }
    let mut synchronous = BTreeSet::new();
    for function in &module.functions {
        fee(b, 1)?;
        if function.foreign.is_some() {
            // A real Direct scalar FFI cannot carry an environment or issue a
            // ZIL suspension. Its exact declaration and every ForeignCall
            // remain independently checked; callback/provider ABIs stay out.
            if !super::super::stored_callables::scalar_foreign(function, b)? {
                return Ok(false);
            }
            continue;
        }
        for ty in function
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(&function.values)
            .chain(std::iter::once(&function.return_type))
        {
            if !data(module, ty, b, 0)? {
                return Ok(false);
            }
        }
        // Every block is inspected, including currently unreachable branches.
        for block in &function.blocks {
            for instruction in &block.instructions {
                fee(b, 1)?;
                let target = match &instruction.kind {
                    InstructionKind::Call { function: target, .. }
                    | InstructionKind::SuspendCall { function: target, .. }
                    | InstructionKind::ForeignCall { function: target, .. } => Some(*target),
                    InstructionKind::CallClosure { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::CallClosureBorrowed { callee, .. } => {
                        let Some(ty) = function.values.get(callee.0 as usize) else {
                            return Ok(false);
                        };
                        let Some(entry) = closure_entry(module, ty, b)? else {
                            return Ok(false);
                        };
                        Some(entry)
                    }
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        let Some(local) = function
                            .locals
                            .get(callee.0 as usize)
                            .filter(|local| local.id == *callee)
                        else {
                            return Ok(false);
                        };
                        let Some(entry) = closure_entry(module, &local.ty, b)? else {
                            return Ok(false);
                        };
                        Some(entry)
                    }
                    InstructionKind::CallDynSlot { interface, slot, .. } => {
                        ordered(b, module.dyn_interfaces.len(), 0)?;
                        let Some(interface) = module.dyn_interfaces.iter()
                            .find(|entry| entry.id == *interface) else { return Ok(false); };
                        let Some(slot) = interface.slots.get(*slot as usize) else { return Ok(false); };
                        if slot.call_kind != CallKind::Direct { return Ok(false); }
                        None
                    }
                    InstructionKind::Constant(_)
                    | InstructionKind::StaticUtf8 { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::ReadAddress { .. }
                    | InstructionKind::WriteAddress { .. }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::AddressIndex { .. }
                    | InstructionKind::AddressSlice { .. }
                    | InstructionKind::SliceLength { .. }
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::ExtractIndex { .. }
                    | InstructionKind::UpdateIndex { .. }
                    | InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::ReservePlaceAddress { .. }
                    | InstructionKind::CommitPlaceReplace { .. }
                    | InstructionKind::DescribeLocal { .. }
                    | InstructionKind::DescribeAddress { .. }
                    | InstructionKind::DescribeCapture { .. }
                    | InstructionKind::MakeVariant { .. }
                    | InstructionKind::ReadVariant { .. }
                    | InstructionKind::VariantTag { .. }
                    | InstructionKind::ErrorTag { .. }
                    | InstructionKind::TakePackageField { .. }
                    | InstructionKind::OwnedStoreNew { .. }
                    | InstructionKind::OwnedStoreReserve { .. }
                    | InstructionKind::OwnedStorePush { .. }
                    | InstructionKind::OwnedStorePop { .. }
                    | InstructionKind::OwnedStoreBorrow { .. }
                    | InstructionKind::OwnedStoreBorrowMut { .. }
                    | InstructionKind::OwnedStoreLen { .. }
                    | InstructionKind::OwnedStoreCapacity { .. }
                    | InstructionKind::ByteBufferNew
                    | InstructionKind::ByteBufferReserve { .. }
                    | InstructionKind::ByteBufferPush { .. }
                    | InstructionKind::ByteBufferTruncate { .. }
                    | InstructionKind::ByteBufferBorrow { .. }
                    | InstructionKind::ByteBufferLen { .. }
                    | InstructionKind::ByteBufferCapacity { .. }
                    | InstructionKind::LoadStoredBorrow { .. }
                    | InstructionKind::MakeDynBorrow { .. }
                    | InstructionKind::MakeDynOwned { .. }
                    | InstructionKind::BorrowDynOwned { .. }
                    | InstructionKind::DropDynOwned { .. }
                    | InstructionKind::MakeErrorUnion { .. }
                    | InstructionKind::DropErrorUnion { .. }
                    | InstructionKind::PlaceTagLocal { .. }
                    | InstructionKind::TakeVariantField { .. }
                    | InstructionKind::ReadErrorUnion { .. }
                    | InstructionKind::TakeErrorUnion { .. }
                    | InstructionKind::StoreStoredBorrow { .. }
                    | InstructionKind::MakeManaged { .. }
                    | InstructionKind::MakePackage { .. }
                    | InstructionKind::ReadDesc { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::BorrowPackageField { .. }
                    | InstructionKind::DropManaged { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::BindPackageCapture { .. }
                    | InstructionKind::EndPackageCapture { .. }
                    | InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. }
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::MakeClosure { .. }
                    | InstructionKind::ReadCapture { .. }
                    | InstructionKind::BorrowCapture { .. }
                    | InstructionKind::TakeCapture { .. }
                    | InstructionKind::WriteCapture { .. }
                    | InstructionKind::DropClosure { .. } => None,
                    // Open code pointers and real TaskYield/protocol execution
                    // need another complete kernel. A SuspendCall above keeps
                    // its real machine ABI and is closed only after every
                    // callee body and cleanup in this module passes this scan.
                    _ => return Ok(false),
                };
                if let Some(target) = target {
                    let Some(callee) = module
                        .functions
                        .get(target.0 as usize)
                        .filter(|callee| callee.id == target)
                    else {
                        return Ok(false);
                    };
                    let suspend = matches!(instruction.kind, InstructionKind::SuspendCall { .. });
                    if callee.call_kind != if suspend { CallKind::Suspend } else { CallKind::Direct }
                        || suspend && function.call_kind != CallKind::Suspend {
                        return Ok(false);
                    }
                    if matches!(instruction.kind, InstructionKind::ForeignCall { .. }) {
                        if callee.foreign.is_none()
                            || !super::super::stored_callables::scalar_foreign(callee, b)? {
                            return Ok(false);
                        }
                    } else if callee.foreign.is_some() {
                        return Ok(false);
                    }
                }
            }
        }
        if function.call_kind == CallKind::Suspend {
            ordered(b, synchronous.len(), 0)?;
            synchronous.insert(function.id);
        }
    }
    // Finite synchronous execution is not a SyncDrop premise. The complete
    // stored flow still checks every current target and parent loan; setting
    // owning_environment_captures forces the independent resource/affine CFG
    // proof before any Hold or cleanup result can be published.
    b.stored_synchronous_abis = synchronous;
    b.owning_environment_captures = true;
    Ok(true)
}

/// Classify one actual call edge only after the complete immutable module
/// graph succeeded. This changes no opcode, ABI, class, target or loan fact.
/// The request entry clears the set before any new semantic/preflight work.
pub(crate) fn ordinary_call(kind: &InstructionKind, b: &mut Budget) -> CompileResult<bool> {
    match kind {
        InstructionKind::Call { .. } => Ok(true),
        InstructionKind::SuspendCall { function, .. } => {
            ordered(b, b.stored_synchronous_abis.len(), 0)?;
            Ok(b.stored_synchronous_abis.contains(function))
        }
        _ => Ok(false),
    }
}

pub(crate) fn ordinary_body(function: &TypedFunction, b: &mut Budget) -> CompileResult<bool> {
    if function.call_kind == CallKind::Direct { return Ok(true); }
    ordered(b, b.stored_synchronous_abis.len(), 0)?;
    Ok(function.call_kind == CallKind::Suspend && b.stored_synchronous_abis.contains(&function.id))
}
