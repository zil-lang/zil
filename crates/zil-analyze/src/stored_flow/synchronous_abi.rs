//! Capability routing for a complete ordinary callback graph whose interface
//! may be Suspend while all execution and cleanup is synchronously closed.
//! No ABI is rewritten. Real Task, foreign, open indirect, or owning-resource
//! dependencies do not enter this lane.
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
        Type::BorrowedStorage { storage, life } if life.slots.is_empty() => {
            storage.is_copy_value() && data(module, storage, b, depth + 1)?
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
        Type::Closure(id) => {
            ordered(b, b.semantic_closures_proven.len(), 0)?;
            b.semantic_closures_proven.contains(id)
                && module
                    .closures
                    .get(id.0 as usize)
                    .is_some_and(|d| d.id == *id)
        }
        // This includes every manager, weak description, resource, Task,
        // registration, opaque shell and unresolved/open code pointer.
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
    if module.semantic.is_none() || module.closures.is_empty() {
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
    let mut synchronous = BTreeSet::new();
    for function in &module.functions {
        fee(b, 1)?;
        if function.foreign.is_some() {
            return Ok(false);
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
                    InstructionKind::Call {
                        function: target, ..
                    } => Some(*target),
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
                    InstructionKind::Constant(_)
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
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::MakeClosure { .. }
                    | InstructionKind::ReadCapture { .. }
                    | InstructionKind::WriteCapture { .. }
                    | InstructionKind::DropClosure { .. } => None,
                    // In particular FunctionRef/CallIndirect, SuspendCall,
                    // TaskYield and all owner/foreign/structured execution
                    // families need another complete capability kernel.
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
                    if callee.call_kind != CallKind::Direct || callee.foreign.is_some() {
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
    // The only cleanup left is Copy data or ordinary environments recursively
    // containing that data. Actual affine environment/SSA and loan checks are
    // still forced below, including empty-debt environment lifetime checks.
    b.stored_synchronous_abis = synchronous;
    b.owning_environment_captures = true;
    Ok(true)
}
