//! Reuse an unchanged inference computation within the current raw pipeline.
//! These values carry no safety/verification capability and are never serialized.
use super::*;
use std::borrow::Cow;
use std::sync::Arc;

/// Closure preparation before CT. Only a borrowed result proves that inference
/// is the identity on this exact program under these exact limits.
pub struct InferredProgram<'a> {
    core: Cow<'a, CoreProgram>,
    limits: Limits,
    shared: Option<&'a Arc<CoreProgram>>,
    producer: Option<&'a zil_expand::ProducedProgram>,
    // Keep optional research receipts off the default worker's stack. This raw
    // result crosses inference/shared/driver closure return slots even when
    // push reuse is off; None must remain a single pointer with no allocation.
    inference_stats: Option<Box<RawInferenceStats>>,
}
impl InferredProgram<'_> {
    pub fn program(&self) -> &CoreProgram {
        &self.core
    }
    pub fn raw_inference_stats(&self) -> Option<&RawInferenceStats> {
        self.inference_stats.as_deref()
    }
    /// Retain the current raw input without claiming CT evaluation or Verify.
    /// Only identical inference is reusable; callers still perform all stages.
    pub fn into_prepared(self) -> PreparedProgram {
        let unchanged = matches!(self.core, Cow::Borrowed(_));
        let core = match self.core {
            Cow::Borrowed(core) => self
                .shared
                .cloned()
                .unwrap_or_else(|| Arc::new(core.clone())),
            Cow::Owned(core) => Arc::new(core),
        };
        PreparedProgram {
            core,
            unchanged_limits: unchanged.then_some(self.limits),
            producer: unchanged.then(|| self.producer.cloned()).flatten(),
        }
    }
    /// CT still executes its complete ordered traversal and charges all current
    /// work. Exact equality includes every declaration, body, identity and span.
    /// Changed output cannot reuse the earlier inference computation.
    pub fn after_evaluation(self, current: CoreProgram) -> PreparedProgram {
        let unchanged = matches!(self.core, Cow::Borrowed(_)) && current == *self.core;
        PreparedProgram {
            core: Arc::new(current),
            unchanged_limits: unchanged.then_some(self.limits),
            // A materialized evaluation result has no producer identity proof.
            producer: None,
        }
    }
}

/// Immutable raw Core plus an optional identity-inference result. Private fields
/// prevent callers from modifying the program while retaining the old result.
#[derive(Clone, Debug)]
pub struct PreparedProgram {
    core: Arc<CoreProgram>,
    unchanged_limits: Option<Limits>,
    producer: Option<zil_expand::ProducedProgram>,
}
impl PreparedProgram {
    pub(super) fn identity_inference(&self, limits: &Limits) -> bool {
        self.unchanged_limits.as_ref() == Some(limits)
    }
    pub fn program(&self) -> &CoreProgram {
        &self.core
    }
    pub(super) fn produced(&self) -> Option<&zil_expand::ProducedProgram> {
        self.producer.as_ref()
    }

    pub(super) fn function_input_revision(
        &self,
        module: zil_base::ModuleId,
        name: &str,
    ) -> Option<&zil_expand::FunctionInputRevision> {
        self.producer
            .as_ref()?
            .function_input_revision(module, name)
    }
    pub fn module_revision(
        &self,
        module: zil_base::ModuleId,
    ) -> Option<&zil_expand::ModuleRevision> {
        self.producer.as_ref()?.module_revision(module)
    }
}

pub fn infer_program_for_ct<'a>(
    program: &'a CoreProgram,
    limits: &Limits,
) -> CompileResult<InferredProgram<'a>> {
    Ok(InferredProgram {
        core: inference::infer_program_closures_borrowed(program, limits)?,
        limits: limits.clone(),
        shared: None,
        producer: None,
        inference_stats: None,
    })
}

pub fn infer_shared_program_for_ct<'a>(
    program: &'a Arc<CoreProgram>,
    limits: &Limits,
) -> CompileResult<InferredProgram<'a>> {
    let mut inferred = infer_program_for_ct(program, limits)?;
    inferred.shared = Some(program);
    Ok(inferred)
}

/// Accept only a program assembled inside the successful Expand producer.
/// Versions survive solely when inference borrows this exact immutable Core;
/// CT materialization and shape resolution retain their ordinary checks.
pub fn infer_produced_program_for_ct<'a>(
    program: &'a zil_expand::ProducedProgram,
    limits: &Limits,
) -> CompileResult<InferredProgram<'a>> {
    let mut inferred = infer_shared_program_for_ct(program.program(), limits)?;
    inferred.producer = Some(program);
    Ok(inferred)
}

/// Reuse only private successful raw identity receipts. CT and later current
/// checking still consume this exact Core; no downstream Typed seal is read.
pub fn infer_produced_program_for_ct_with_session<'a>(
    program: &'a zil_expand::ProducedProgram,
    limits: &Limits,
    session: &mut RawInferenceSession,
) -> CompileResult<InferredProgram<'a>> {
    let (core, stats) = session.infer(program, limits)?;
    Ok(InferredProgram {
        core,
        limits: limits.clone(),
        shared: Some(program.program()),
        producer: Some(program),
        inference_stats: Some(Box::new(stats)),
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
    let resolved = aggregates::resolve_prepared_program(&prepared.core, limits)?;
    program::check_resolved_program(&resolved, target, limits)
}
