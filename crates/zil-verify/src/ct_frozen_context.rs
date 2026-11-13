//! Contextual CT verification keeps the supplied frozen source view outside the
//! executable package. A binding request alone never creates either authority.
use super::*;

pub fn verify_ct_program_in_context(
    raw:CtProgram,
    source:&CoreProgram,
    expected:&CtFrozenContext,
    target:&TargetProfile,
    limits:&Limits,
)->CompileResult<VerifiedCTProgram> {
    ct_program::quotas(&raw,limits)?;
    if raw.frozen_context.as_ref()!=Some(expected)||expected.query.wrapper!=SemanticFunctionTarget::Typed(raw.entry) {
        return Err(vec![Diagnostic::error(Phase::Ct,"E-CT-FROZEN-CONTEXT",None,"CT request differs from the independently selected frozen source/query binding")]);
    }
    let mut budget=Budget::new(limits);
    let frozen=semantic::FrozenContext::new(source,expected,&mut budget).map_err(|error|vec![error])?;
    let checked=check_module_budget(&raw.module,&raw.plan,target,limits,None,&mut budget,Some(&frozen)).map_err(|error|vec![error])?;
    let verified=VerifiedModule{
        inputs:VerifiedInputs::Owned{module:raw.module,plan:raw.plan},
        fingerprint:checked.fingerprint,limits:limits.clone(),
        uses_owned_sum_extension:checked.uses_owned_sum_extension,
        uses_scalar_tasks:checked.uses_scalar_tasks,
        uses_retained_registration:checked.uses_retained_registration,
        uses_stored_borrows:checked.uses_stored_borrows,
        task_workers:checked.task_workers,suspend_functions:checked.suspend_functions,
        task_exports:checked.task_exports,task_input_fingerprint:checked.task_input_fingerprint,
        task_ownership:Arc::new(checked.task_ownership),
    };
    ct_program::admit(verified,raw.entry,raw.max_steps,raw.max_call_depth,raw.max_live_slots)
}
