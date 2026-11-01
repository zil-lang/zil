//! Reuse an unchanged inference computation within the current raw pipeline.
//! These values carry no safety/verification capability and are never serialized.
use super::*;
use std::borrow::Cow;

/// Closure preparation before CT. Only a borrowed result proves that inference
/// is the identity on this exact program under these exact limits.
pub struct InferredProgram<'a> {
    core: Cow<'a, CoreProgram>,
    limits: Limits,
}
impl InferredProgram<'_> {
    pub fn program(&self) -> &CoreProgram {
        &self.core
    }
    /// CT still executes its complete ordered traversal and charges all current
    /// work. Exact equality includes every declaration, body, identity and span.
    /// Changed output cannot reuse the earlier inference computation.
    pub fn after_evaluation(self, current: CoreProgram) -> PreparedProgram {
        let unchanged = matches!(self.core, Cow::Borrowed(_)) && current == *self.core;
        PreparedProgram {
            core: current,
            unchanged_limits: unchanged.then_some(self.limits),
        }
    }
}

/// Immutable raw Core plus an optional identity-inference result. Private fields
/// prevent callers from modifying the program while retaining the old result.
#[derive(Clone, Debug)]
pub struct PreparedProgram {
    core: CoreProgram,
    unchanged_limits: Option<Limits>,
}
impl PreparedProgram {
    pub fn program(&self) -> &CoreProgram {
        &self.core
    }
}

pub fn infer_program_for_ct<'a>(
    program: &'a CoreProgram,
    limits: &Limits,
) -> CompileResult<InferredProgram<'a>> {
    Ok(InferredProgram {
        core: inference::infer_program_closures_borrowed(program, limits)?,
        limits: limits.clone(),
    })
}

pub fn check_prepared_program(
    prepared: &PreparedProgram,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<TypedProgram> {
    if prepared.unchanged_limits.as_ref() != Some(limits) {
        return check_program(&prepared.core, target, limits);
    }
    // The identical deterministic inference call already completed successfully.
    // Its result is the same raw input. Every subsequent Types operation remains
    // current, including shape resolution; Analyze/Verify are separate consumers.
    let resolved = aggregates::resolve_program(&prepared.core, limits)?;
    program::check_resolved_program(&resolved, target, limits)
}
