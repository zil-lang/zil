//! Stored source transport across a real Task child call. This is a frame
//! execution boundary, not a synchronous-ABI theorem. Pending inputs and
//! cancellation remain on the independently checked Task/resource ledgers.
use super::*;

pub(crate) fn prepare(module: &TypedModule, b: &mut Budget) -> CompileResult<bool> {
    if !b.tasks || !b.task_origin_roots || module.semantic.is_none() {
        return Ok(false);
    }
    let mut calls = false;
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                fee(b, 1)?;
                calls |= matches!(instruction.kind, InstructionKind::SuspendCall { .. });
            }
        }
    }
    if !calls { return Ok(false); }
    if !module.closures.is_empty() && b.task_input_scopes.is_none() {
        return Err(failure("stored Task child call has no complete actual input scopes"));
    }
    if !b.stored_task_frame_functions.is_empty() || b.stored_task_frame_pending {
        return Err(failure("stored Task child frame graph was scheduled more than once"));
    }
    // Reconstruct every declared frame and actual call edge, including
    // unreachable instructions. Exact operands/results, execution selection,
    // static child graph and canonical resources are checked by this kernel.
    // The ordinary resource/loan solution still has to run afterwards.
    crate::tasks::check(module, b)?;
    let mut frames = BTreeSet::new();
    for function in &module.functions {
        fee(b, 1)?;
        if function.call_kind == CallKind::Suspend {
            crate::tasks::cleanup_frame_profile(module, function, b)?;
            ordered(b, frames.len(), 0)?;
            frames.insert(function.id);
        }
    }
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                fee(b, 1)?;
                let InstructionKind::SuspendCall { function: target, .. } = instruction.kind
                else { continue; };
                ordered(b, frames.len(), 0)?;
                if !frames.contains(&function.id) || !frames.contains(&target) {
                    return Err(failure("stored Task child call changes its actual caller or callee frame"));
                }
            }
        }
    }
    b.stored_task_frame_functions = frames;
    b.stored_task_frame_pending = true;
    b.owning_environment_captures = true;
    Ok(true)
}

/// The same normal-return source substitution is required by both call ABIs.
/// A real SuspendCall stays a child-frame operation; no result exists on its
/// Pending/cancellation edge, whose responsibilities are checked separately.
pub(crate) fn call(kind: &InstructionKind, b: &mut Budget) -> CompileResult<bool> {
    if synchronous_abi::ordinary_call(kind, b)? { return Ok(true); }
    if let InstructionKind::SuspendCall { function, .. } = kind {
        ordered(b, b.stored_task_frame_functions.len(), 0)?;
        return Ok(b.stored_task_frame_functions.contains(function));
    }
    Ok(false)
}

pub(crate) fn body(function: &TypedFunction, b: &mut Budget) -> CompileResult<bool> {
    if synchronous_abi::ordinary_body(function, b)? { return Ok(true); }
    ordered(b, b.stored_task_frame_functions.len(), 0)?;
    Ok(function.call_kind == CallKind::Suspend
        && b.stored_task_frame_functions.contains(&function.id))
}

/// Invoked only after the ordinary Task graph, resource ledger, actual Hold
/// reconstruction and borrowed-Task loan/separation checks all succeeded.
pub(crate) fn finish(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    if !b.stored_task_frame_pending { return Ok(()); }
    fee(b, b.stored_task_frame_functions.len())?;
    let covered = b.stored_completed_functions.as_ref().ok_or_else(|| failure(
        "stored Task child call has no completed actual source/summary proof"))?;
    for function in &b.stored_task_frame_functions {
        if module.functions.get(function.0 as usize).is_none_or(|actual| actual.id != *function)
            || covered.get(function.0 as usize) != Some(&true)
        {
            return Err(failure("stored Task child frame omits complete actual summary coverage"));
        }
    }
    b.stored_task_frame_pending = false;
    Ok(())
}
