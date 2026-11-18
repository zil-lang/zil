//! Physical representation after immutable capability/LIR binding. Lifetime
//! metadata never enters machine fields and never supplies an address/loan.
use super::*;
use zil_base::Limits;
use zil_ir::{
    PlaceProjection, StorageLifeContract, StorageLifeShape, StoragePosition, StorageSource,
    StoredBorrowLoad, TypedModule,
};

fn exhausted() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Codegen,
        "stored physical representation budget exceeded",
        None,
    )]
}
fn pay(work: &mut usize, n: usize) -> CompileResult<()> {
    *work = work.checked_sub(n).ok_or_else(exhausted)?;
    Ok(())
}
fn source_metadata(source: &StorageSource, work: &mut usize, limits: &Limits) -> CompileResult<()> {
    let position = |position: &StoragePosition, work: &mut usize| {
        if position.path.len() > limits.max_call_depth {
            return Err(exhausted());
        }
        pay(work, position.path.len().saturating_add(1))
    };
    pay(work, 1)?;
    match source {
        StorageSource::Exact(value) => position(value, work),
        StorageSource::PlaceAddress { place, .. } => {
            position(place, work)?;
            pay(work, 1)
        }
        StorageSource::OwnedView { head, leaf, .. } => {
            position(head, work)?;
            pay(work, 1)?;
            type_work(leaf, work, limits, 0)
        }
        StorageSource::ReborrowedLeaf { source, head } => {
            position(source, work)?;
            position(head, work)
        }
        StorageSource::Static { leaf, .. } => {
            pay(work, 1)?;
            type_work(leaf, work, limits, 0)
        }
    }
}
pub(super) fn shape(life: &StorageLifeShape, work: &mut usize) -> CompileResult<()> {
    pay(
        work,
        life.slots
            .len()
            .saturating_add(life.retained.len())
            .saturating_add(life.nominal_arguments.len())
            .saturating_add(1),
    )?;
    for row in &life.slots {
        pay(work, row.path.len().saturating_add(2))?;
    }
    for row in &life.retained {
        pay(work, row.path.len().saturating_add(2))?;
    }
    for row in &life.nominal_arguments {
        pay(work, row.path.len().saturating_add(4))?;
    }
    Ok(())
}
pub(super) fn contract(life: &StorageLifeContract, work: &mut usize) -> CompileResult<()> {
    // A public successor must update this visitor instead of silently adding
    // unmetered semantic signature metadata.
    let StorageLifeContract {
        static_bindings: _,
        fixed_binder_count: _,
        invocation_binder_count: _,
        bindings: _,
        classes: _,
    } = life;
    pay(
        work,
        life.bindings
            .len()
            .saturating_add(life.classes.len())
            .saturating_add(life.static_bindings.len())
            .saturating_mul(16)
            .saturating_add(32),
    )?;
    for row in &life.bindings {
        pay(work, row.position.path.len().saturating_mul(8))?;
    }
    for position in &life.static_bindings {
        pay(work, position.path.len().saturating_mul(8))?;
    }
    for row in &life.classes {
        pay(work, row.position.path.len().saturating_mul(8))?;
    }
    Ok(())
}
pub(super) fn type_work(
    t: &Type,
    work: &mut usize,
    limits: &Limits,
    depth: usize,
) -> CompileResult<()> {
    pay(work, 8)?;
    if depth > limits.max_call_depth {
        return Err(exhausted());
    }
    match t {
        Type::OwnedDyn { .. } => {}
        Type::Mutex { payload } | Type::MutexGuard { payload, .. } => {
            type_work(payload, work, limits, depth + 1)?;
        }
        Type::BorrowedStorage { storage, life } => {
            shape(life, work)?;
            type_work(storage, work, limits, depth + 1)?;
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            pay(work, name.len().saturating_add(fields.len()))?;
            for (name, t) in fields {
                pay(work, name.len())?;
                type_work(t, work, limits, depth + 1)?;
            }
        }
        Type::Tuple(fields) => {
            pay(work, fields.len())?;
            for t in fields {
                type_work(t, work, limits, depth + 1)?;
            }
        }
        Type::Enum { name, variants } => {
            pay(work, name.len().saturating_add(variants.len()))?;
            for (name, fs) in variants {
                pay(work, name.len().saturating_add(fs.len()))?;
                for t in fs {
                    type_work(t, work, limits, depth + 1)?;
                }
            }
        }
        Type::Address { inner, .. }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::OwnedStore { element: inner }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner }
        | Type::Opaque { hidden: inner, .. } => type_work(inner, work, limits, depth + 1)?,
        Type::ErrorUnion { error, result } => {
            type_work(error, work, limits, depth + 1)?;
            type_work(result, work, limits, depth + 1)?;
        }
        Type::BorrowedFunctionPointer {
            parameters,
            result,
            life,
        } => {
            contract(life, work)?;
            pay(work, parameters.len())?;
            for t in parameters {
                type_work(t, work, limits, depth + 1)?;
            }
            type_work(result, work, limits, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            pay(work, parameters.len())?;
            for t in parameters {
                type_work(t, work, limits, depth + 1)?;
            }
            type_work(result, work, limits, depth + 1)?;
        }
        Type::Task {
            result,
            retained_life,
        } => {
            pay(
                work,
                retained_life
                    .ports
                    .len()
                    .saturating_add(retained_life.retained.len())
                    .saturating_add(retained_life.nominal_arguments.len())
                    .saturating_add(1),
            )?;
            for port in &retained_life.ports {
                pay(work, 2)?;
                type_work(&port.leaf, work, limits, depth + 1)?;
            }
            for row in &retained_life.retained {
                pay(work, row.path.len().saturating_add(2))?;
            }
            for row in &retained_life.nominal_arguments {
                pay(work, row.path.len().saturating_add(4))?;
            }
            type_work(result, work, limits, depth + 1)?;
        }
        Type::Named(text) | Type::Unsupported(text) => pay(work, text.len())?,
        Type::Dyn { .. }
        | Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I8
        | Type::I16
        | Type::Isize
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::ByteBuffer
        | Type::ForeignHandle
        | Type::SyncCell(_)
        | Type::TaskI32
        | Type::CWideString
        | Type::Closure(_) => {}
    }
    Ok(())
}

/// Reserve full new callable metadata before raw LIR equality or any retained
/// source-type clone. ABI erasure is physical only; no relation is flattened.
pub(super) fn reserve_callable_metadata(
    module: &TypedModule,
    limits: &Limits,
    work: &mut usize,
) -> CompileResult<()> {
    pay(
        work,
        module
            .functions
            .len()
            .saturating_add(module.closures.len())
            .saturating_add(module.resources.len()),
    )?;
    for function in &module.functions {
        if let Some(instance) = &function.dyn_instance {
            super::static_utf8::reserve_dyn_instance(instance, limits, work)?;
        }

        for rows in function.retained_return_sources.iter().chain(
            function
                .callable_return_contract
                .iter()
                .filter_map(|contract| contract.capture_sources.as_ref()),
        ) {
            pay(work, rows.len().saturating_add(1))?;
            for row in rows {
                pay(
                    work,
                    row.position
                        .path
                        .len()
                        .saturating_add(row.sources.len())
                        .saturating_add(1),
                )?;
                for source in &row.sources {
                    source_metadata(source, work, limits)?;
                }
            }
        }
        for protocol in &function.protocol_contracts {
            pay(
                work,
                protocol
                    .entry
                    .len()
                    .saturating_add(protocol.exits.len())
                    .saturating_add(4),
            )?;
            for exit in &protocol.exits {
                pay(work, exit.output.len().saturating_add(2))?;
            }
        }
        if let Some(life) = &function.storage_life {
            contract(life, work)?;
        }
        if let Some(c) = &function.callable_contract {
            pay(work, c.roles.len())?;
            for role in &c.roles {
                if let Some(life) = &role.life {
                    contract(life, work)?;
                }
            }
        }
        type_work(&function.return_type, work, limits, 0)?;
        for ty in function
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&function.values)
        {
            type_work(ty, work, limits, 0)?;
        }
    }
    for closure in &module.closures {
        pay(work, closure.captures.len())?;
        if let Some(life) = &closure.storage_life {
            contract(life, work)?;
        }
        type_work(&closure.result, work, limits, 0)?;
        for ty in closure.parameters.iter().chain(&closure.captures) {
            type_work(ty, work, limits, 0)?;
        }
    }
    for resource in &module.resources {
        type_work(&resource.payload, work, limits, 0)?;
    }
    Ok(())
}

/// Only this representation slice is currently implemented. This predicate is
/// used after Verify; it grants no source Copy/Store/capture qualification.
pub(super) fn representation_type(t: &Type) -> bool {
    shared_address_array(t)
        || shared_address_array_address(t)
        || matches!(t,Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{..}|Type::Package{..}|Type::Tuple(_)))
        || matches!(t,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{..}|Type::Package{..}|Type::Tuple(_))))
        || matches!(t,Type::Tuple(fields) if fields.iter().all(|field| field.is_copy_value() || representation_type(field)))
        || shared_pointer_enum(t)
        || matches!(t,Type::ErrorUnion{error,result} if (error.is_copy_value() || representation_type(error)) && (result.is_copy_value() || representation_type(result)))
        || pure_composite(t)
}

// Machine shape only, after immutable Verify capability and full LIR binding.
// A Slice pointee is deliberately excluded: that Address is a fat {ptr,len}.
// No nominal name, class or source permission is inferred by this predicate.
// Finite thin element storage only. The full representation/type walk is
// already prepaid by reserve_callable_metadata before any machine query.
pub(super) fn shared_address_array(t: &Type) -> bool {
    matches!(t, Type::Array { element, length } if *length > 0
        && matches!(element.as_ref(), Type::Address { mutable: false, inner }
            if pure_composite(inner)))
}
pub(super) fn shared_address_array_address(t: &Type) -> bool {
    matches!(t, Type::Address { inner, .. } if shared_address_array(inner))
}

/// Snapshot classification under immutable Verify, not language Copy. Only
/// finite non-owning products containing actual thin shared array leaves enter.
/// Each caller prepays its complete actual type before this recursive walk.
pub(super) fn shared_array_snapshot(t: &Type) -> bool {
    fn shape(t: &Type) -> Option<bool> {
        match t {
            Type::BorrowedStorage { storage, life }
                if matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                    && life
                        .slots
                        .iter()
                        .all(|s| s.mode == zil_ir::StoredBorrowMode::SharedAddress) =>
            {
                shape(storage)
            }
            Type::Record { fields, .. } => {
                let mut contains = false;
                for (_, field) in fields {
                    contains |= shape(field)?;
                }
                Some(contains)
            }
            Type::Tuple(fields) => {
                let mut contains = false;
                for field in fields {
                    contains |= shape(field)?;
                }
                Some(contains)
            }
            Type::Array { .. } if shared_address_array(t) => Some(true),
            _ => pure_composite(t).then_some(false),
        }
    }
    shape(t) == Some(true)
}

pub(super) fn shared_pointer_enum(t: &Type) -> bool {
    let Type::BorrowedStorage { storage, .. } = t else {
        return false;
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        return false;
    };
    !variants.is_empty()
        && variants.iter().all(|(_, fields)| {
            fields.iter().all(|field| {
                field.is_copy_value()
                    || matches!(field, Type::Address { mutable: false, inner }
                    if inner.is_copy_value() || matches!(inner.as_ref(), Type::Package { .. }))
            })
        })
}

pub(super) fn shared_sum(t: &Type) -> bool {
    shared_pointer_enum(t)
        || matches!(t, Type::ErrorUnion { error, result }
            if (shared_sum(error) || shared_sum(result))
                && (error.is_copy_value() || shared_sum(error))
                && (result.is_copy_value() || shared_sum(result)))
}

/// An actual verified root pointer to pure storage. Never use this predicate
/// for the pointee's fields: pure_composite rejects nested Address nodes.
/// The complete type visit was prepaid before physical representation queries.
pub(super) fn pure_storage_address(t: &Type) -> bool {
    matches!(t, Type::Address { inner, .. } if pure_composite(inner))
}

// reserve_callable_metadata has already bounded and paid the complete type
// walk before bind_lir calls this predicate. All variant payloads are visited,
// including inactive ones; no address or owning/resource leaf enters this new
// physical-only path. Existing Record/Tuple borrow representations above keep
// their separately verified profile.
pub(super) fn pure_composite(t: &Type) -> bool {
    match t {
        Type::BorrowedStorage { storage, life } => life.slots.is_empty() && pure_composite(storage),
        Type::Tuple(fields) => fields.iter().all(pure_composite),
        Type::Record { fields, .. } => fields.iter().all(|(_, t)| pure_composite(t)),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, fields)| fields.iter().all(pure_composite)),
        Type::Array { element, .. } => pure_composite(element),
        Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I8
        | Type::I16
        | Type::Isize
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64
        | Type::U8
        | Type::U16
        | Type::U32 => true,
        _ => false,
    }
}

/// Pay comparison against separately allocated public LIR from the same
/// request ledger later consumed by Copy ABI and task/registration emission.
pub(super) fn reserve_binding(
    module: &TypedModule,
    limits: &Limits,
    work: &mut usize,
) -> CompileResult<()> {
    // A verified module may contain separate stored and ordinary components.
    // Exact LIR binding compares the complete original module, so this ledger
    // covers ordinary Task/registration instructions as well as stored bodies.
    // Like a MIR visitor, the comparison ledger exhaustively names variants
    // and fields: a new payload must force this consumer to be reconsidered.
    // Scalar IDs/flags are covered by each fixed reservation; recursive Type,
    // path, argument, port and byte payloads are paid explicitly.
    pay(work, module.functions.len())?;
    for f in &module.functions {
        pay(
            work,
            f.name
                .len()
                .saturating_add(f.locals.len())
                .saturating_add(f.values.len())
                .saturating_add(f.blocks.len()),
        )?;
        if let Some(declaration) = &f.foreign {
            pay(
                work,
                declaration
                    .abi
                    .len()
                    .saturating_add(declaration.symbol.len())
                    .saturating_add(declaration.noescape_parameters.len())
                    .saturating_add(3),
            )?;
        }
        if let Some(c) = &f.storage_life {
            contract(c, work)?;
        }
        if let Some(c) = &f.callable_contract {
            pay(work, c.roles.len())?;
            for role in &c.roles {
                pay(
                    work,
                    role.hold
                        .len()
                        .saturating_add(role.call.len())
                        .saturating_add(role.writes.len())
                        .saturating_add(8),
                )?;
            }
            match &c.traces {
                zil_ir::CallableTraces::Finite(traces) => {
                    pay(work, traces.len().saturating_add(1))?;
                    for trace in traces {
                        pay(work, trace.calls.len().saturating_mul(2).saturating_add(1))?;
                    }
                }
                zil_ir::CallableTraces::Flow(flow) => {
                    pay(work, flow.nodes.len().saturating_add(1))?;
                    for node in &flow.nodes {
                        pay(work, node.calls.len().saturating_mul(2).saturating_add(1))?;
                        if let zil_ir::CallableFlowNext::Edges(edges) = &node.next {
                            pay(work, edges.len())?;
                        }
                    }
                }
            }
        }
        type_work(&f.return_type, work, limits, 0)?;
        for l in &f.locals {
            pay(work, l.name.len())?;
            type_work(&l.ty, work, limits, 0)?;
        }
        for t in &f.values {
            type_work(t, work, limits, 0)?;
        }
        for block in &f.blocks {
            pay(work, block.instructions.len().saturating_add(1))?;
            for i in &block.instructions {
                let n = match &i.kind {
                    InstructionKind::Call { arguments, function: _ }
                    | InstructionKind::SuspendCall { arguments, function: _ }
                    | InstructionKind::ForeignCall { arguments, function: _, region: _ }
                    | InstructionKind::MakeAggregate { fields: arguments }
                    | InstructionKind::MakeVariant {
                        fields: arguments, variant: _
                    }
                    | InstructionKind::MakeManaged { fields: arguments }
                    | InstructionKind::MakePackage { fields: arguments }
                    | InstructionKind::MakeOwnedTuple { fields: arguments }
                    | InstructionKind::MakeOwnedArray {
                        elements: arguments,
                    }
                    | InstructionKind::CallDynSlot { arguments, callee: _, interface: _, slot: _ } => arguments.len(),
                    InstructionKind::TaskCreate {
                        arguments,
                        port_bindings,
                        worker: _
                    }
                    | InstructionKind::TaskCreateOwned {
                        arguments,
                        port_bindings,
                        worker: _
                    } => {
                        pay(work, port_bindings.len())?;
                        for binding in port_bindings {
                            pay(work, binding.source.path.len().saturating_add(3))?;
                        }
                        arguments.len()
                    }
                    InstructionKind::UnpackTuple { fields, local: _ }
                    | InstructionKind::UnpackVariant { fields, local: _, variant: _ } => fields.len(),
                    InstructionKind::CallIndirect { arguments, callee: _ }
                    | InstructionKind::CallClosure { arguments, callee: _ }
                    | InstructionKind::CallClosureOnce { arguments, callee: _ }
                    | InstructionKind::CallClosureBorrowed { arguments, callee: _, kind: _ }
                    | InstructionKind::CallClosureShared { arguments, callee: _ }
                    | InstructionKind::CallClosureMut { arguments, callee: _ } => {
                        arguments.len().saturating_add(1)
                    }
                    InstructionKind::MakeClosure { captures, closure: _ } => {
                        captures.len().saturating_add(1)
                    }
                    InstructionKind::BindPackageCapture { local: _, index: _ }
                    | InstructionKind::EndPackageCapture { local: _, index: _ } => 2,
                    InstructionKind::FunctionRef { function: _ } => 1,
                    InstructionKind::WriteDesc { desc: _, index: _, value: _ } => 2,
                    InstructionKind::ExtractIndex { aggregate: _, index: _ } | InstructionKind::UpdateField { aggregate: _, index: _, value: _ } => 2,
                    InstructionKind::UpdateIndex { aggregate: _, index: _, value: _ } => 3,
                    InstructionKind::VariantTag { value: _ } | InstructionKind::ReadVariant { value: _, variant: _, index: _ } => 1,
                    InstructionKind::ExtractField { aggregate: _, index: _ }
                    | InstructionKind::ReadPackageField { local: _, index: _ }
                    | InstructionKind::ReadCapture { index: _ }
                    | InstructionKind::BorrowPackageField { local: _, index: _, mutable: _ }
                    | InstructionKind::DescribePackageField { local: _, index: _ }
                    | InstructionKind::BorrowCapture { index: _, mutable: _ }
                    | InstructionKind::WriteCapture { index: _, value: _ }
                    | InstructionKind::OwnedStoreBorrow { store: _, index: _ }
                    | InstructionKind::OwnedStoreBorrowMut { store: _, index: _ }
                    | InstructionKind::TakeLocal { local: _ }
                    | InstructionKind::TakeCapture { index: _ }
                    | InstructionKind::DescribeLocal { local: _ }
                    | InstructionKind::DescribeCapture { index: _ }
                    | InstructionKind::ReadDesc { desc: _, index: _ }
                    | InstructionKind::DropManaged { local: _ }
                    | InstructionKind::DropClosure { local: _ } => 1,
                    InstructionKind::LoadStoredBorrow { path, storage: _, mode: _ }
                    | InstructionKind::StoreStoredBorrow { path, storage: _, value: _ }
                    | InstructionKind::TakeTupleField { path, local: _ }
                    | InstructionKind::TakeVariantField { path, local: _ }
                    | InstructionKind::BorrowPlaceLocal { path, local: _, mutable: _ }
                    | InstructionKind::PlaceTagLocal { path, local: _ }
                    | InstructionKind::BorrowPlaceAddress { path, address: _, mutable: _ }
                    | InstructionKind::PlaceTagAddress { path, address: _ }
                    | InstructionKind::ReservePlaceLocal { path, reservation: _, local: _ }
                    | InstructionKind::ReservePlaceAddress { path, reservation: _, address: _ } => path.len(),
                    InstructionKind::BorrowFieldLocal { path, local: _, mutable: _ } => path.len(),
                    InstructionKind::VariantTagLocal { path, local: _ }
                    | InstructionKind::ReadVariantLocal { path, local: _ }
                    | InstructionKind::BorrowVariantLocal { path, local: _, mutable: _ }
                    | InstructionKind::VariantTagAddress { path, address: _ }
                    | InstructionKind::BorrowVariantAddress { path, address: _, mutable: _ } => {
                        path.len().saturating_mul(2)
                    }
                    InstructionKind::Constant(Constant::WideString(s)) => s.len(),
                    InstructionKind::OwnedStoreNew { element }
                    | InstructionKind::ConvertInteger { target: element, value: _ }
                    | InstructionKind::ConvertNumber { target: element, value: _ } => {
                        type_work(element, work, limits, 0)?;
                        8
                    }
                    InstructionKind::StaticUtf8 { bytes } => bytes.len(),
                    InstructionKind::Constant(Constant::Unit | Constant::Bool(_)
                        | Constant::I32(_) | Constant::I64(_) | Constant::U64(_)
                        | Constant::Usize(_) | Constant::F32(_) | Constant::F64(_)
                        | Constant::U8(_) | Constant::U16(_) | Constant::U32(_)
                        | Constant::I8(_) | Constant::I16(_) | Constant::Isize(_)
                        | Constant::NullHandle | Constant::NullWideString)
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { local: _, value: _ }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::BorrowLocal { local: _ }
                    | InstructionKind::BorrowLocalMut { local: _ }
                    | InstructionKind::ReborrowAddress { address: _, mutable: _ }
                    | InstructionKind::AddressField { address: _, index: _ }
                    | InstructionKind::AddressIndex { address: _, index: _ }
                    | InstructionKind::AddressSlice { address: _, start: _, end: _ }
                    | InstructionKind::ReadAddress { address: _ }
                    | InstructionKind::WriteAddress { address: _, value: _ }
                    | InstructionKind::SliceLength { address: _ }
                    | InstructionKind::Unary { op: _, operand: _ }
                    | InstructionKind::Binary { op: _, left: _, right: _ }
                    | InstructionKind::ScopeEnter { scope: _ }
                    | InstructionKind::ScopeExit { scope: _, kind: _ }
                    | InstructionKind::ByteBufferNew
                    | InstructionKind::ByteBufferLen { buffer: _ }
                    | InstructionKind::ByteBufferCapacity { buffer: _ }
                    | InstructionKind::ByteBufferBorrow { buffer: _, mutable: _ }
                    | InstructionKind::ByteBufferPush { buffer: _, value: _ }
                    | InstructionKind::ByteBufferReserve { buffer: _, capacity: _ }
                    | InstructionKind::ByteBufferTruncate { buffer: _, length: _ }
                    | InstructionKind::DescribeAddress { address: _ }
                    | InstructionKind::DropPackage { local: _ }
                    | InstructionKind::TakePackageField { local: _, index: _ }
                    | InstructionKind::DropPackageField { local: _, index: _ }
                    | InstructionKind::StorePackageField { local: _, index: _, value: _ }
                    | InstructionKind::UnsafeEnter { region: _ }
                    | InstructionKind::UnsafeExit { region: _ } => 1,
                    InstructionKind::MakeDynOwned { interface: _, vtable: _, source: _, route: _ }
                    | InstructionKind::MakeDynBorrow { interface: _, vtable: _, source: _, mutable: _ }
                    | InstructionKind::BorrowDynOwned { local: _, mutable: _ }
                    | InstructionKind::DropDynOwned { local: _ }
                    | InstructionKind::MakeErrorUnion { is_error: _, value: _ }
                    | InstructionKind::OwnedStoreReserve { store: _, capacity: _ }
                    | InstructionKind::OwnedStorePush { store: _, value: _ }
                    | InstructionKind::OwnedStorePop { store: _ }
                    | InstructionKind::OwnedStoreLen { store: _ }
                    | InstructionKind::OwnedStoreCapacity { store: _ }
                    | InstructionKind::ErrorTag { value: _ }
                    | InstructionKind::ReadErrorUnion { value: _, is_error: _ }
                    | InstructionKind::TakeErrorUnion { value: _, is_error: _ }
                    | InstructionKind::DropErrorUnion { local: _ }
                    | InstructionKind::DropTuple { local: _ }
                    | InstructionKind::DropEnum { local: _ }
                    | InstructionKind::ForeignFunctionRef { function: _ }
                    | InstructionKind::TaskStart { worker: _, argument: _ }
                    | InstructionKind::TaskYield
                    | InstructionKind::TaskValid { task: _ }
                    | InstructionKind::TaskPoll { task: _ }
                    | InstructionKind::TaskRequestCancel { task: _ }
                    | InstructionKind::TaskPollClose { task: _ }
                    | InstructionKind::TaskFinishDestroy { task: _ }
                    | InstructionKind::RegistrationStart { register: _, close: _, callback: _, region: _ }
                    | InstructionKind::RegistrationValid { registration: _ }
                    | InstructionKind::RegistrationClose { registration: _ }
                    | InstructionKind::AsyncRegistrationStart { register: _, request_close: _, poll_drain: _, callback: _, region: _ }
                    | InstructionKind::AsyncRegistrationValid { registration: _ }
                    | InstructionKind::AsyncRegistrationRequestClose { registration: _ }
                    | InstructionKind::AsyncRegistrationPollClose { registration: _ }
                    | InstructionKind::AsyncRegistrationFinishDestroy { registration: _ }
                    | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration: _ }
                    | InstructionKind::RegisterDefer { scope: _, local: _ }
                    | InstructionKind::CommitPlaceReplace { reservation: _, value: _ }
                    | InstructionKind::AbortPlaceReservation { reservation: _ }
                    | InstructionKind::ProjectOwnedArrayField { local: _, index: _, field: _ }
                    | InstructionKind::TakeOwnedArrayElement { local: _, index: _ }
                    | InstructionKind::DropOwnedArrayElement { local: _, index: _ }
                    | InstructionKind::StoreOwnedArrayElement { local: _, index: _, value: _ }
                    | InstructionKind::DropOwnedArray { local: _ }
                    | InstructionKind::BorrowOwnedArrayElement { local: _, index: _, mutable: _ }
                    | InstructionKind::WithForeignCallback { register: _, close: _, callback: _, body: _, region: _ }
                    | InstructionKind::WithScopedThread { worker: _, argument: _, body: _ }
                    | InstructionKind::WithSyncCell { kind: _, initial: _, body: _ }
                    | InstructionKind::SyncI32 { op: _, address: _, value: _, ordering: _ }
                    | InstructionKind::SyncI32CompareExchange { address: _, expected: _, desired: _, success: _, failure: _ }
                    | InstructionKind::SyncInteger { op: _, address: _, value: _, ordering: _ }
                    | InstructionKind::SyncIntegerCompareExchange { address: _, expected: _, desired: _, success: _, failure: _ }
                    | InstructionKind::MutexCreate { value: _ }
                    | InstructionKind::MutexLock { mutex: _ }
                    | InstructionKind::MutexValue { guard: _ }
                    | InstructionKind::MutexUnlock { guard: _ }
                    | InstructionKind::DropMutexGuard { local: _ } => 8,
                    InstructionKind::Unsupported(_) => {
                        return invalid(
                            "E-CODEGEN-STORAGE-PROFILE",
                            "stored profile instruction comparison has no prepaid consumer",
                            Some(i.span),
                        );
                    }
                };
                pay(work, n.saturating_add(4))?;
            }
        }
    }
    Ok(())
}

pub(super) fn reserve_emission(
    lir: &LirView<'_>,
    limits: &Limits,
    snapshots: &BTreeMap<FunctionId, copy_aggregates::Plan>,
    work: &mut usize,
) -> CompileResult<()> {
    pay(work, lir.functions.len())?;
    let total_instructions = lir.functions.iter().try_fold(0usize, |n, f| {
        pay(work, f.blocks.len())?;
        f.blocks.iter().try_fold(n, |n, b| {
            n.checked_add(b.instructions.len()).ok_or_else(exhausted)
        })
    })?;
    for f in lir.functions {
        // Array ownership flags are expanded per element at entry and at
        // each initialization/transfer/drop site, even when data is copied by
        // one aggregate load/store. Reuse the complete multiplied machine
        // bound; visiting the compact Array type alone cannot pay this work.
        pay(work, f.locals.len())?;
        let mut array_expansion = Vec::with_capacity(f.locals.len());
        for local in &f.locals {
            let owning_array = matches!(&local.ty, Type::Array { element, .. }
                if matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_)));
            let bound = if owning_array {
                let mut remaining = limits.clone();
                remaining.max_analysis_steps = *work;
                let mut budget = drop_plan::Budget::new(&remaining);
                let bound = budget.store_bound(&local.ty, 0);
                pay(work, budget.spent())?;
                bound?
            } else { 0 };
            pay(work, bound)?; // actual entry allocation/initial flags
            array_expansion.push(bound);
        }
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            let whole_array_local = match i.kind {
                InstructionKind::Store { local, .. }
                | InstructionKind::TakeLocal { local }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local)
                | InstructionKind::DropOwnedArray { local } => Some(local),
                _ => None,
            };
            if let Some(local) = whole_array_local {
                pay(work, array_expansion[local.0 as usize])?;
            }
            let count = match &i.kind {
                InstructionKind::Call { arguments, .. }
                | InstructionKind::SuspendCall { arguments, .. }
                | InstructionKind::ForeignCall { arguments, .. }
                | InstructionKind::TaskCreate { arguments, .. }
                | InstructionKind::TaskCreateOwned { arguments, .. }
                | InstructionKind::MakeAggregate { fields: arguments }
                | InstructionKind::MakeOwnedTuple { fields: arguments }
                | InstructionKind::MakeOwnedArray { elements: arguments }
                | InstructionKind::MakePackage { fields: arguments }
                | InstructionKind::MakeVariant {
                    fields: arguments, ..
                }
                | InstructionKind::MakeManaged { fields: arguments }
                | InstructionKind::MakeClosure {
                    captures: arguments,
                    ..
                }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. } => arguments.len(),
                InstructionKind::CallDynSlot { arguments, .. }
                | InstructionKind::CallIndirect { arguments, .. }
                | InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. } => {
                    arguments.len().saturating_add(1)
                }
                _ => 3,
            };
            pay(work, count.saturating_add(32))?;
            if let InstructionKind::Constant(Constant::WideString(s)) = &i.kind {
                let cost = s
                    .len()
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(total_instructions.saturating_add(1)))
                    .and_then(|n| n.checked_mul(16))
                    .ok_or_else(exhausted)?;
                pay(work, cost)?;
            }
            // Local field/path consumers inspect the complete actual parent
            // layout, not only their (possibly scalar) result. A stored module
            // can contain ordinary Package/resource helpers as separate bodies.
            if let InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. }
            | InstructionKind::TakeLocal { local: callee }
            | InstructionKind::DescribeLocal { local: callee }
            | InstructionKind::DropManaged { local: callee }
            | InstructionKind::DropClosure { local: callee }
            | InstructionKind::BorrowDynOwned { local: callee, .. }
            | InstructionKind::DropDynOwned { local: callee }
            | InstructionKind::DropMutexGuard { local: callee }
            | InstructionKind::ProjectOwnedArrayField { local: callee, .. }
            | InstructionKind::TakeOwnedArrayElement { local: callee, .. }
            | InstructionKind::DropOwnedArrayElement { local: callee, .. }
            | InstructionKind::StoreOwnedArrayElement { local: callee, .. }
            | InstructionKind::DropOwnedArray { local: callee }
            | InstructionKind::BorrowOwnedArrayElement { local: callee, .. }
            | InstructionKind::ReadPackageField { local: callee, .. }
            | InstructionKind::BorrowPackageField { local: callee, .. }
            | InstructionKind::DescribePackageField { local: callee, .. }
            | InstructionKind::BorrowPlaceLocal { local: callee, .. }
            | InstructionKind::PlaceTagLocal { local: callee, .. } = &i.kind
            {
                type_work(&f.locals[callee.0 as usize].ty, work, limits, 0)?;
            }
            if let InstructionKind::TakeCapture { index }
            | InstructionKind::DescribeCapture { index }
            | InstructionKind::BorrowCapture { index, .. } = &i.kind
            {
                let definition =
                    &lir.closures[f.environment.expect("verified capture entry").0 as usize];
                type_work(&definition.captures[*index as usize], work, limits, 0)?;
            }
            if let InstructionKind::MakeVariant { fields, .. } = &i.kind
                && let Some(result) = i.result
                && !f.values[result.0 as usize].is_copy_value()
                && (pure_composite(&f.values[result.0 as usize])
                    || shared_pointer_enum(&f.values[result.0 as usize]))
                && snapshots
                    .get(&f.id)
                    .is_none_or(|plan| !plan.contains_key(&result))
            {
                // Planned enum results already prepay each nonsnapshot field's
                // canonical store in copy_aggregates::plan. Only an SSA enum
                // result reaches this additional reservation.
                for field in fields {
                    pay(
                        work,
                        copy_aggregates::canonical_payload_work(&f.values[field.0 as usize])
                            .saturating_mul(16),
                    )?;
                }
            }
            for v in i.kind.value_operands().chain(i.result) {
                type_work(&f.values[v.0 as usize], work, limits, 0)?;
            }
            if let InstructionKind::MakeAggregate { fields }
            | InstructionKind::MakePackage { fields }
            | InstructionKind::MakeOwnedTuple { fields }
            | InstructionKind::MakeOwnedArray { elements: fields }
            | InstructionKind::MakeManaged { fields } = &i.kind
                && let Some(v) = i.result
            {
                for _ in fields {
                    type_work(&f.values[v.0 as usize], work, limits, 0)?;
                }
            }
        }
    }
    Ok(())
}

impl FunctionEmitter<'_> {
    pub(super) fn stored_store(
        &mut self,
        i: &Instruction,
        storage: ValueId,
        path: &[PlaceProjection],
        value: ValueId,
    ) -> CompileResult<()> {
        let Type::Address {
            mutable: true,
            inner,
        } = self.value_type(storage)?
        else {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored replacement lacks a mutable carrier",
                Some(i.span),
            );
        };
        let Type::BorrowedStorage {
            storage: physical, ..
        } = inner.as_ref()
        else {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored replacement lacks its qualified physical type",
                Some(i.span),
            );
        };
        if path.is_empty() {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored replacement has an empty path",
                Some(i.span),
            );
        }
        let mut current = physical.as_ref();
        let mut indices = vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0))];
        for part in path {
            let PlaceProjection::Field(index) = part else {
                return unsupported(
                    "stored replacement requires an actual inline field path",
                    Some(i.span),
                );
            };
            current = match current {
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    fields.get(*index as usize).map(|(_, ty)| ty)
                }
                Type::Tuple(fields) => fields.get(*index as usize),
                _ => None,
            }
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-STORAGE",
                    Some(i.span),
                    "stored replacement crosses a non-inline field",
                )]
            })?;
            indices.push(m::Arg::new(m::Ty::Int(32), m::Val::Int(i128::from(*index))));
        }
        if !matches!(current, Type::Address { .. })
            || self.value_type(value)? != current
            || i.result.is_some()
        {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored replacement value differs from its actual address leaf",
                Some(i.span),
            );
        }
        let root_type = self.layouts.value_type(physical)?;
        let leaf_type = self.layouts.value_type(current)?;
        let align = self.layouts.alignment(current)?;
        let slot = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(slot.clone())),
            m::Op::Gep {
                ty: root_type,
                ptr: m::local(format!("v.{}", storage.0)),
                indices,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(leaf_type, m::local(format!("v.{}", value.0))),
                ptr: m::val(slot),
                align,
            },
        )));
        Ok(())
    }

    pub(super) fn stored_load(
        &mut self,
        i: &Instruction,
        storage: ValueId,
        path: &[PlaceProjection],
        mode: StoredBorrowLoad,
    ) -> CompileResult<()> {
        if !matches!(
            mode,
            StoredBorrowLoad::CopyShared
                | StoredBorrowLoad::Reborrow { mutable: true }
                | StoredBorrowLoad::Take
        ) {
            return unsupported(
                "stored mutable/reborrow/description native operation is not yet implemented",
                Some(i.span),
            );
        }
        let Type::Address { inner, .. } = self.value_type(storage)? else {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored base is not an actual Address",
                Some(i.span),
            );
        };
        let physical = match inner.as_ref() {
            Type::BorrowedStorage { storage, .. } => storage.as_ref(),
            Type::Array { element, length }
                if *length > 0
                    && matches!(element.as_ref(), Type::Address { mutable: false, .. })
                    && mode == StoredBorrowLoad::CopyShared =>
            {
                inner.as_ref()
            }
            _ => {
                return invalid(
                    "E-CODEGEN-STORAGE",
                    "stored base lacks its actual physical type",
                    Some(i.span),
                );
            }
        };
        if path.is_empty() {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored physical path is empty",
                Some(i.span),
            );
        }
        let mut current = physical;
        let mut indices = vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0))];
        for part in path {
            let PlaceProjection::Field(index) = part else {
                return unsupported(
                    "stored enum/error native path is not yet implemented",
                    Some(i.span),
                );
            };
            current = match current {
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    fields.get(*index as usize).map(|(_, t)| t)
                }
                Type::Tuple(fs) => fs.get(*index as usize),
                Type::Array { element, length } if *index < *length => Some(element.as_ref()),
                _ => None,
            }
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-STORAGE",
                    Some(i.span),
                    "stored physical path crosses non-inline storage",
                )]
            })?;
            indices.push(m::Arg::new(m::Ty::Int(32), m::Val::Int(i128::from(*index))));
        }
        let qualified_child_take = mode == StoredBorrowLoad::Take
            && matches!(
                current,
                Type::Tuple(_) | Type::Record { .. } | Type::Package { .. }
            )
            && matches!(self.value_type(result(i)?)?,Type::BorrowedStorage{storage,..} if storage.as_ref()==current);
        if !qualified_child_take
            && (!matches!(
                (current, mode),
                (
                    Type::Address { mutable: false, .. },
                    StoredBorrowLoad::CopyShared
                ) | (
                    Type::Address { mutable: true, .. },
                    StoredBorrowLoad::Reborrow { mutable: true }
                ) | (Type::Address { .. }, StoredBorrowLoad::Take)
            ) || self.value_type(result(i)?)? != current)
        {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored shared leaf differs from actual result",
                Some(i.span),
            );
        }
        let root_type = self.layouts.value_type(physical)?;
        let leaf_type = self.layouts.value_type(current)?;
        let align = self.layouts.alignment(current)?;
        let slot = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(slot.clone())),
            m::Op::Gep {
                ty: root_type,
                ptr: m::local(format!("v.{}", storage.0)),
                indices,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result(i)?.0)),
            m::Op::Load {
                ty: leaf_type,
                ptr: m::val(slot),
                align,
            },
        )));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owning_array_emission_pays_length_at_every_transfer_site() {
        // A physical budget probe, not an independently verified program.
        fn measure(length: u32, stores: usize, allowance: usize) -> CompileResult<usize> {
            let mut module = fixture::module();
            module.functions.truncate(1);
            let f = &mut module.functions[0];
            f.parameters.clear();
            f.locals.truncate(1);
            let ty = Type::Array { element: Box::new(Type::Closure(ClosureId(0))), length };
            f.locals[0].ty = ty.clone();
            f.values = vec![ty];
            f.blocks.truncate(1);
            f.blocks[0].instructions = (0..stores).map(|_| fixture::instruction(None,
                InstructionKind::Store { local: LocalId(0), value: ValueId(0) })).collect();
            f.blocks[0].terminator = Terminator::Return(None);
            let view = LirView {
                schema_version: module.schema_version, module_id: module.module_id,
                verified_fingerprint: zil_base::Fingerprint::ZERO, target: &module.target,
                functions: &module.functions, closures: &[], resources: &[],
                layouts: std::borrow::Cow::Borrowed(&[]),
            };
            let mut work = allowance;
            reserve_emission(&view, &Limits::default(), &BTreeMap::new(), &mut work)?;
            Ok(allowance - work)
        }
        let entry_growth = measure(5, 0, 1_000_000).unwrap() - measure(1, 0, 1_000_000).unwrap();
        let repeated_growth = measure(5, 3, 1_000_000).unwrap() - measure(1, 3, 1_000_000).unwrap();
        assert!(entry_growth > 0);
        assert!(repeated_growth >= 4 * entry_growth,
            "three transfers must pay their own length expansion, in addition to entry");
        let exact = measure(5, 3, 1_000_000).unwrap();
        assert_eq!(measure(5, 3, exact).unwrap(), exact);
        assert_eq!(measure(5, 3, exact - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit);
        assert_eq!(measure(5, 3, exact).unwrap(), exact);
    }

    #[test]
    fn borrowed_enum_representation_is_a_thin_pointer_slice_only() {
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

        // Synthetic machine-layout probe only. Source/native acceptance still
        // requires a genuine immutable Verify capability and full LIR binding.
        let qualify = |payload| Type::BorrowedStorage {
            storage: Box::new(Type::Enum {
                name: "machine-shape-only".into(),
                variants: vec![("empty".into(), vec![]), ("present".into(), vec![payload])],
            }),
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let thin = qualify(Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        });
        assert!(shared_pointer_enum(&thin));
        assert!(representation_type(&thin));
        assert_eq!(
            layouts.value_type(&thin).unwrap(),
            m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::array(1, m::Ty::Int(64))])
        );
        assert_eq!(layouts.alignment(&thin).unwrap(), 8);
        assert_eq!(layouts.union(&thin).unwrap().words, 1);
        assert!(!representation_type(&qualify(Type::Address {
            mutable: true,
            inner: Box::new(Type::I64)
        })));
        assert!(!representation_type(&qualify(Type::Address {
            mutable: false,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::I64)
            })
        })));
    }

    #[test]
    fn pure_representation_checks_every_variant_and_array_element() {
        let qualify = |storage| Type::BorrowedStorage {
            storage: Box::new(storage),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let mut sum = qualify(Type::Enum {
            name: "Choice".into(),
            variants: vec![("Empty".into(), vec![]), ("Value".into(), vec![Type::U64])],
        });
        assert!(representation_type(&sum));
        let Type::BorrowedStorage { storage, .. } = &mut sum else {
            unreachable!()
        };
        let Type::Enum { variants, .. } = storage.as_mut() else {
            unreachable!()
        };
        variants.push((
            "Inactive".into(),
            vec![Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }],
        ));
        // Ordinary shared borrowed Enums now have a physical representation.
        // This layout predicate does not establish their lifetime inventory.
        assert!(representation_type(&sum));
        let Type::BorrowedStorage { storage, .. } = &mut sum else {
            unreachable!()
        };
        let Type::Enum { variants, .. } = storage.as_mut() else {
            unreachable!()
        };
        variants.push(("Unsupported".into(), vec![Type::Unsupported("leaf".into())]));
        assert!(!representation_type(&sum));
        let array = |element| Type::Array {
            element: Box::new(element),
            length: 2,
        };
        assert!(representation_type(&array(qualify(Type::Record {
            name: "Leaf".into(),
            fields: vec![("bits".into(), Type::U64)]
        }))));
        assert!(!representation_type(&array(Type::Managed(Box::new(
            Type::ByteBuffer
        )))));
        let mut slotted = qualify(Type::Enum {
            name: "Empty".into(),
            variants: vec![("None".into(), vec![])],
        });
        let Type::BorrowedStorage { life, .. } = &mut slotted else {
            unreachable!()
        };
        life.slots.push(zil_ir::StorageLifeSlot {
            path: vec![],
            binder: 0,
            mode: zil_ir::StoredBorrowMode::SharedAddress,
        });
        // The physical Enum layout is still known. This deliberately invalid
        // slot inventory must be rejected by Verify before layout selection.
        assert!(representation_type(&slotted));
    }
    #[test]
    fn index_root_address_is_pointer_not_snapshot_or_embedded_value() {
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

        // Synthetic representation probe only; not source/loan authority.
        let qualify = |storage| Type::BorrowedStorage {
            storage: Box::new(storage),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let leaf = qualify(Type::Record {
            name: "Leaf".into(),
            fields: vec![("bits".into(), Type::U64)],
        });
        let pointee = Type::Array {
            element: Box::new(leaf),
            length: 4096,
        };
        let root = Type::Address {
            inner: Box::new(pointee.clone()),
            mutable: false,
        };
        assert!(pure_storage_address(&root));
        assert!(!pure_composite(&root));
        assert_eq!(layouts.value_type(&root).unwrap(), m::Ty::Ptr);
        assert_eq!(layouts.alignment(&root).unwrap(), 8);
        let mut work = usize::MAX;
        assert!(
            copy_aggregates::classify(layouts, &root, &mut work, &Limits::default())
                .unwrap()
                .is_none(),
            "pointer itself must not become a pointee-sized snapshot"
        );
        assert!(
            copy_aggregates::classify(layouts, &pointee, &mut work, &Limits::default())
                .unwrap()
                .is_some(),
            "ReadAddress result retains the existing large snapshot"
        );
        let shared_array_root = Type::Address {
            inner: Box::new(Type::Array {
                element: Box::new(root),
                length: 2,
            }),
            mutable: false,
        };
        // The fixed shared-address Array has its own post-Verify pointer
        // representation; it still is not pure storage or a value snapshot.
        assert!(!pure_storage_address(&shared_array_root));
        assert!(shared_address_array_address(&shared_array_root));
        assert_eq!(layouts.value_type(&shared_array_root).unwrap(), m::Ty::Ptr);
        assert_eq!(layouts.alignment(&shared_array_root).unwrap(), 8);
        assert!(
            copy_aggregates::classify(layouts, &shared_array_root, &mut work, &Limits::default())
                .unwrap()
                .is_none()
        );
        let address_array = |element, length| Type::Address {
            inner: Box::new(Type::Array {
                element: Box::new(element),
                length,
            }),
            mutable: false,
        };
        for bad in [
            address_array(
                Type::Address {
                    mutable: true,
                    inner: Box::new(Type::I32),
                },
                2,
            ),
            address_array(
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::Slice {
                        element: Box::new(Type::I32),
                    }),
                },
                2,
            ),
            address_array(
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
                0,
            ),
        ] {
            assert!(!shared_address_array_address(&bad));
            assert!(!pure_storage_address(&bad));
            // Physical pointer layout grants none of the rejected snapshot
            // or borrow capabilities checked above.
            assert_eq!(layouts.value_type(&bad).unwrap(),m::Ty::Ptr);
            assert_eq!(layouts.alignment(&bad).unwrap(),8);
        }
    }

    #[test]
    fn callable_relation_rows_paths_and_pointer_layout_are_metered() {
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

        use zil_ir::{
            LifeTypeProjection, SignatureLifeClass, SignatureLifeClassBinding, StorageBase,
            StorageBinder,
        };
        let limits = Limits::default();
        let mut growth = vec![];
        // Raw metadata growth probe only: these selectors grant no kernel
        // permission. Real verified semantic/native controls are separate.
        for (rows, path) in [(0, 0), (8, 4), (32, 32)] {
            let life = StorageLifeContract {
                static_bindings: Vec::new(),
                fixed_binder_count: 0,
                invocation_binder_count: 1,
                bindings: vec![],
                classes: (0..rows)
                    .map(|class| SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base: StorageBase::Parameter(0),
                            path: vec![LifeTypeProjection::Pointee; path],
                            class,
                        },
                        binder: StorageBinder::Invocation(0),
                    })
                    .collect(),
            };
            let ty = Type::BorrowedFunctionPointer {
                parameters: vec![Type::I32],
                result: Box::new(Type::I32),
                life,
            };
            let mut left = usize::MAX;
            type_work(&ty, &mut left, &limits, 0).unwrap();
            let needed = usize::MAX - left;
            let mut exact = needed;
            type_work(&ty, &mut exact, &limits, 0).unwrap();
            assert_eq!(exact, 0);
            let copied = ty.clone();
            assert_eq!(copied, ty);
            assert_eq!(layouts.value_type(&ty).unwrap(), m::Ty::Ptr);
            let mut short = needed - 1;
            assert_eq!(
                type_work(&ty, &mut short, &limits, 0).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            let mut fresh = needed;
            type_work(&ty, &mut fresh, &limits, 0).unwrap();
            assert_eq!(fresh, 0);
            if rows > 0 {
                let mut changed = ty.clone();
                let Type::BorrowedFunctionPointer { life, .. } = &mut changed else {
                    unreachable!()
                };
                life.classes[0].binder = StorageBinder::Fixed(0);
                assert_ne!(changed.fingerprint(), ty.fingerprint());
                assert_eq!(layouts.value_type(&changed).unwrap(), m::Ty::Ptr);
            }
            growth.push(needed);
            eprintln!("callable40 metadata rows={rows} path={path} Codegen N={needed}");
        }
        assert!(growth[2] > growth[1] * 8);
    }

    use zil_ir::{BlockId, BlockPlan, CandidatePlan, FunctionPlan, HoldRoot, ValueHold};
    #[allow(dead_code)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
        ));
    }
    #[test]
    fn ordinary_owner_byte_and_error_consumers_prepay_complete_actual_types() {
        // Physical budget probes only. Real Source and independent Verify
        // establish admission in the ordinary stored-composition consumers.
        let limits = Limits::default();
        let record = Type::Record {
            name: "ActualErrorPayload".repeat(32),
            fields: vec![("retained_field_name".repeat(32), Type::I64)],
        };
        let union = Type::ErrorUnion {
            error: Box::new(record.clone()),
            result: Box::new(Type::I64),
        };
        let package = Type::Package {
            name: "ActualItem".repeat(32),
            fields: vec![
                ("data".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("tag".into(), Type::I64),
                ("sibling".into(), record.clone()),
            ],
        };
        let address = |ty| Type::Address {
            mutable: false,
            inner: Box::new(ty),
        };
        for (kind, input, output, local) in [
            (
                InstructionKind::ReadPackageField {
                    local: LocalId(0),
                    index: 1,
                },
                package,
                Type::I64,
                true,
            ),
            (
                InstructionKind::ByteBufferBorrow {
                    buffer: ValueId(0),
                    mutable: false,
                },
                Type::Desc(Box::new(Type::ByteBuffer)),
                address(Type::Slice {
                    element: Box::new(Type::U8),
                }),
                false,
            ),
            (
                InstructionKind::ReadErrorUnion {
                    value: ValueId(0),
                    is_error: true,
                },
                union.clone(),
                record.clone(),
                false,
            ),
            (
                InstructionKind::BorrowPlaceLocal {
                    local: LocalId(0),
                    path: vec![PlaceProjection::ErrorArm(true)],
                    mutable: false,
                },
                union.clone(),
                address(record.clone()),
                true,
            ),
            (
                InstructionKind::BorrowPlaceAddress {
                    address: ValueId(0),
                    path: vec![PlaceProjection::ErrorArm(true)],
                    mutable: false,
                },
                address(union),
                address(record),
                false,
            ),
        ] {
            let mut module = fixture::module();
            module.functions.truncate(1);
            let f = &mut module.functions[0];
            f.parameters.clear();
            f.locals.truncate(1);
            f.locals[0].ty = input.clone();
            f.return_type = Type::Unit;
            let result = if local { 0 } else { 1 };
            f.values = if local {
                vec![output.clone()]
            } else {
                vec![input.clone(), output.clone()]
            };
            f.blocks.truncate(1);
            f.blocks[0].instructions = vec![fixture::instruction(Some(result), kind)];
            f.blocks[0].terminator = Terminator::Return(None);
            let view = LirView {
                schema_version: module.schema_version,
                module_id: module.module_id,
                verified_fingerprint: zil_base::Fingerprint::ZERO,
                target: &module.target,
                functions: &module.functions,
                closures: &[],
                resources: &[],
                layouts: std::borrow::Cow::Borrowed(&[]),
            };
            let mut types = usize::MAX;
            type_work(&input, &mut types, &limits, 0).unwrap();
            type_work(&output, &mut types, &limits, 0).unwrap();
            // One function, one block and the existing fixed event reservation,
            // plus the full actual input parent and complete result type.
            let needed = (usize::MAX - types) + 37 + view.functions[0].locals.len();
            let mut exact = needed;
            reserve_emission(&view, &limits, &BTreeMap::new(), &mut exact).unwrap();
            assert_eq!(exact, 0);
            let failed =
                reserve_emission(&view, &limits, &BTreeMap::new(), &mut (needed - 1)).unwrap_err();
            assert!(failed.iter().all(|d| d.phase == Phase::Codegen
                && d.category == zil_base::ResultCategory::ResourceLimit));
            let mut fresh = needed;
            reserve_emission(&view, &limits, &BTreeMap::new(), &mut fresh).unwrap();
            assert_eq!(fresh, 0);
            // The same complete instruction also has an explicit, paid LIR
            // comparison consumer before any separately supplied LIR is bound.
            let mut available = usize::MAX;
            reserve_binding(&module, &limits, &mut available).unwrap();
            let binding = usize::MAX - available;
            let mut exact_binding = binding;
            reserve_binding(&module, &limits, &mut exact_binding).unwrap();
            assert_eq!(exact_binding, 0);
            assert!(
                reserve_binding(&module, &limits, &mut (binding - 1))
                    .unwrap_err()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }

    #[test]
    fn descriptor_field_access_prepays_parent_layout_and_write_input() {
        // Physical ledger probes, not descriptor access authority. Both
        // instructions use a scalar field of the complete actual resource;
        // WriteDesc has no result whose type could accidentally pay its input.
        let limits = Limits::default();
        let desc = Type::Desc(Box::new(Type::Record {
            name: "ActualQuietPayload".repeat(32),
            fields: vec![
                ("value".into(), Type::I64),
                ("unselected_parent_field".repeat(32), Type::I32),
            ],
        }));
        for (kind, output) in [
            (
                InstructionKind::ReadDesc {
                    desc: ValueId(0),
                    index: 0,
                },
                Some(1),
            ),
            (
                InstructionKind::WriteDesc {
                    desc: ValueId(0),
                    index: 0,
                    value: ValueId(1),
                },
                None,
            ),
        ] {
            let mut module = fixture::module();
            module.functions.truncate(1);
            let f = &mut module.functions[0];
            f.parameters.clear();
            f.locals.clear();
            f.return_type = Type::Unit;
            f.values = vec![desc.clone(), Type::I64];
            f.blocks.truncate(1);
            f.blocks[0].instructions = vec![fixture::instruction(output, kind)];
            f.blocks[0].terminator = Terminator::Return(None);
            let view = LirView {
                schema_version: module.schema_version,
                module_id: module.module_id,
                verified_fingerprint: zil_base::Fingerprint::ZERO,
                target: &module.target,
                functions: &module.functions,
                closures: &[],
                resources: &[],
                layouts: std::borrow::Cow::Borrowed(&[]),
            };
            let mut types = usize::MAX;
            type_work(&desc, &mut types, &limits, 0).unwrap();
            type_work(&Type::I64, &mut types, &limits, 0).unwrap();
            let needed = (usize::MAX - types) + 37;
            for _ in 0..2 {
                let mut exact = needed;
                reserve_emission(&view, &limits, &BTreeMap::new(), &mut exact).unwrap();
                assert_eq!(exact, 0);
            }
            let failed =
                reserve_emission(&view, &limits, &BTreeMap::new(), &mut (needed - 1)).unwrap_err();
            assert_eq!(failed[0].category, zil_base::ResultCategory::ResourceLimit);
            let mut binding = usize::MAX;
            reserve_binding(&module, &limits, &mut binding).unwrap();
            let needed = usize::MAX - binding;
            let mut exact = needed;
            reserve_binding(&module, &limits, &mut exact).unwrap();
            assert_eq!(exact, 0);
            assert_eq!(
                reserve_binding(&module, &limits, &mut (needed - 1)).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }

    #[test]
    fn snapshot_enum_canonical_store_is_reserved_once() {
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

        // Synthetic physical budgeting probe, not a verified source program.
        let mut module = fixture::module();
        module.functions.truncate(1);
        let qualify = |storage| Type::BorrowedStorage {
            storage: Box::new(storage),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let input = qualify(Type::Record {
            name: "Leaf".into(),
            fields: vec![("bits".into(), Type::U64)],
        });
        let output = qualify(Type::Enum {
            name: "Wide".into(),
            variants: vec![
                ("Small".into(), vec![input.clone()]),
                (
                    "Wide".into(),
                    vec![Type::Array {
                        element: Box::new(Type::U64),
                        length: 1024,
                    }],
                ),
            ],
        });
        let f = &mut module.functions[0];
        f.parameters.clear();
        f.locals.clear();
        f.return_type = Type::Unit;
        f.values = vec![input.clone(), output];
        f.blocks.truncate(1);
        f.blocks[0].instructions = vec![fixture::instruction(
            Some(1),
            InstructionKind::MakeVariant {
                variant: 0,
                fields: vec![ValueId(0)],
            },
        )];
        f.blocks[0].terminator = Terminator::Return(None);
        let view = LirView {
            schema_version: module.schema_version,
            module_id: module.module_id,
            verified_fingerprint: zil_base::Fingerprint::ZERO,
            target: &module.target,
            functions: &module.functions,
            closures: &[],
            resources: &[],
            layouts: std::borrow::Cow::Borrowed(&[]),
        };
        let limits = Limits::default();
        let planned = copy_aggregates::plan(layouts, &view, &limits, usize::MAX).unwrap();
        let snapshot = &planned.values[&module.functions[0].id];
        assert!(snapshot.contains_key(&ValueId(1)));
        assert!(!snapshot.contains_key(&ValueId(0)));
        let mut with_snapshot = usize::MAX;
        reserve_emission(&view, &limits, &planned.values, &mut with_snapshot).unwrap();
        let mut without_snapshot = usize::MAX;
        reserve_emission(&view, &limits, &BTreeMap::new(), &mut without_snapshot).unwrap();
        assert_eq!(
            with_snapshot - without_snapshot,
            copy_aggregates::canonical_payload_work(&input) * 16
        );
        let needed = usize::MAX - with_snapshot;
        let mut exact = needed;
        reserve_emission(&view, &limits, &planned.values, &mut exact).unwrap();
        assert_eq!(exact, 0);
        let errors =
            reserve_emission(&view, &limits, &planned.values, &mut (needed - 1)).unwrap_err();
        assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
    }

    fn verified() -> VerifiedModule {
        let module = fixture::wide_shared(64);
        let origins = vec![
            vec![
                vec![HoldRoot::Parameter(0)],
                vec![HoldRoot::Parameter(0)],
                vec![HoldRoot::Parameter(0)],
                vec![],
            ],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0)), HoldRoot::Local(LocalId(1))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
            ],
            vec![vec![HoldRoot::Parameter(0)]],
        ];
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions = module
            .functions
            .iter()
            .zip(origins)
            .enumerate()
            .map(|(i, (f, rows))| FunctionPlan {
                function: f.id,
                blocks: vec![BlockPlan {
                    block: BlockId(0),
                    initialized_in: if i == 3 { vec![] } else { vec![LocalId(0)] },
                    initialized_out: vec![LocalId(0)],
                }],
                holds: rows
                    .into_iter()
                    .enumerate()
                    .map(|(i, roots)| ValueHold {
                        value: ValueId(i as u32),
                        roots,
                    })
                    .collect(),
            })
            .collect();
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap()
    }
    fn lir(v: &VerifiedModule) -> LirModule {
        let m = v.module();
        LirModule {
            schema_version: m.schema_version,
            module_id: m.module_id,
            verified_fingerprint: v.fingerprint(),
            target: m.target.clone(),
            functions: m.functions.clone(),
            closures: m.closures.clone(),
            resources: m.resources.clone(),
            layouts: vec![
                ScalarLayout {
                    ty: Type::Unit,
                    size: 0,
                    alignment: 1,
                },
                ScalarLayout {
                    ty: Type::Bool,
                    size: 1,
                    alignment: 1,
                },
                ScalarLayout {
                    ty: Type::I32,
                    size: 4,
                    alignment: 4,
                },
                ScalarLayout {
                    ty: Type::I64,
                    size: 8,
                    alignment: 8,
                },
                ScalarLayout {
                    ty: Type::F32,
                    size: 4,
                    alignment: 4,
                },
                ScalarLayout {
                    ty: Type::F64,
                    size: 8,
                    alignment: 8,
                },
                ScalarLayout {
                    ty: Type::U8,
                    size: 1,
                    alignment: 1,
                },
                ScalarLayout {
                    ty: Type::U16,
                    size: 2,
                    alignment: 2,
                },
                ScalarLayout {
                    ty: Type::U32,
                    size: 4,
                    alignment: 4,
                },
                ScalarLayout {
                    ty: Type::I8,
                    size: 1,
                    alignment: 1,
                },
                ScalarLayout {
                    ty: Type::I16,
                    size: 2,
                    alignment: 2,
                },
                ScalarLayout {
                    ty: Type::Isize,
                    size: 8,
                    alignment: 8,
                },
                ScalarLayout {
                    ty: Type::ForeignHandle,
                    size: 8,
                    alignment: 8,
                },
                ScalarLayout {
                    ty: Type::CWideString,
                    size: 8,
                    alignment: 8,
                },
            ],
        }
    }
    #[test]
    fn stored_lir_binding_reservation_is_exact_before_separate_matching_or_forged_input() {
        let verified = verified();
        let lir = lir(&verified);
        let full = verified.limits().max_analysis_steps;
        let mut left = full;
        reserve_binding(verified.module(), verified.limits(), &mut left).unwrap();
        let needed = full - left;
        let run = |n, raw: &LirModule| -> CompileResult<usize> {
            let mut left = n;
            reserve_binding(verified.module(), verified.limits(), &mut left)?;
            bind_exact(&verified, &raw.as_view(), &mut left)?;
            Ok(left)
        };
        assert_eq!(run(needed, &lir).unwrap(), 0);
        assert!(
            run(needed - 1, &lir)
                .unwrap_err()
                .iter()
                .all(|d| d.phase == Phase::Codegen
                    && d.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(run(needed, &lir).unwrap(), 0);
        let mut forged = lir.clone();
        forged.functions[1].storage_life = None;
        assert!(
            run(needed, &forged)
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-CODEGEN-UNBOUND-LIR")
        );
        assert!(
            run(needed - 1, &forged)
                .unwrap_err()
                .iter()
                .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        println!(
            "342 actual verified wide stored LIR binding component Codegen N={needed}; N-1/fresh and forged-input prepayment verified"
        );
    }
}
