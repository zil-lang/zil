#![forbid(unsafe_code)]

//! Small independent checking boundary. Public IR and candidate facts are always
//! untrusted. No type solver, analyzer, host reference rule, or candidate hash can
//! create the wrappers in this crate without the checks below succeeding.
//!
//! These fields are intentionally inaccessible to a downstream crate:
//! ```compile_fail
//! use zil_verify::VerifiedModule;
//! fn counterfeit(raw: zil_ir::TypedModule, plan: zil_ir::CandidatePlan) {
//!     let _ = VerifiedModule { module: raw, plan, fingerprint: zil_base::Fingerprint::ZERO };
//! }
//! ```
//! ```compile_fail
//! use zil_verify::VerifiedCTProgram;
//! fn counterfeit(verified: zil_verify::VerifiedModule) {
//!     let _ = VerifiedCTProgram { verified, entry: zil_ir::FunctionId(0),
//!         max_steps: 1, max_call_depth: 1, max_live_slots: 1 };
//! }
//! ```
//! ```compile_fail
//! use zil_verify::VerifiedCTIR;
//! fn counterfeit(raw: zil_ir::CtModule) { let _ = VerifiedCTIR { module: raw }; }
//! ```
//! Read-only access cannot mutate a checked snapshot:
//! ```compile_fail
//! fn mutate(verified: &zil_verify::VerifiedModule) {
//!     verified.module().functions.clear();
//! }
//! ```
//! ```compile_fail
//! fn mutate(verified: &zil_verify::VerifiedCTIR) {
//!     verified.module().instructions.clear();
//! }
//! ```

use std::{collections::BTreeSet, fmt, sync::Arc};
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, Limits, Phase, RULE_VERSION, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

mod address_sums;
mod borrowed_fp;
mod borrows;
mod byte_buffers;
mod c_layout;
mod cleanup;
#[cfg(test)]
mod closure_tests;
mod closures;
mod ct_artifact;
mod ct_program;
#[cfg(test)]
mod defer_tests;
mod defers;
mod dynamic_dispatch;
mod dynamic_store;
#[cfg(test)]
mod error_scope_tests;
mod errors;
mod foreign;
mod head_access;
#[cfg(test)]
mod helper_tests;
mod helpers;
mod mixed_address_sums;
mod native_values;
mod owned_callbacks;
#[cfg(test)]
mod package_tests;
mod packages;
mod place_replacements;
mod registration;
#[cfg(test)]
mod resource_closure_tests;
#[cfg(test)]
mod resource_tests;
mod resources;
#[cfg(test)]
mod static_utf8_tests;
pub use resources::task_frames::{
    TaskDebtBoundary, TaskOwner, TaskOwnerAction, TaskOwnerActions, TaskOwnerSlot, TaskOwnership,
};
mod class_flow;
mod retained;
#[cfg(test)]
mod retained_tests;
mod scoped_callback;
mod scoped_thread;
mod signature_life;
mod slices;
mod stored_borrows;
mod stored_callables;
mod stored_flow;
mod stored_owned;
mod stored_partition;
mod sums;
mod tasks;
pub use ct_artifact::{
    CtArtifactAuthority, CtScopeParameterKind, CtSymbolUsage, CtVerificationMeter,
    VerifiedCTArtifactProgram, verify_ct_artifact, verify_ct_artifact_metered,
    verify_ct_type_graph_metered,
};
pub use ct_program::{VerifiedCTProgram, verify_ct_program, verify_ct_program_bytes};

/// A checked, immutable scalar/readonly-closure CFG, including independently
/// reconstructed local shared-address provenance and lifetime constraints.
#[derive(Clone)]
pub struct VerifiedModule {
    inputs: VerifiedInputs,
    fingerprint: Fingerprint,
    limits: Limits,
    uses_owned_sum_extension: bool,
    task_workers: BTreeSet<FunctionId>,
    suspend_functions: BTreeSet<FunctionId>,
    uses_scalar_tasks: bool,
    uses_retained_registration: bool,
    uses_stored_borrows: bool,
    task_exports: BTreeSet<FunctionId>,
    task_input_fingerprint: Fingerprint,
    task_ownership: Arc<std::collections::BTreeMap<FunctionId, TaskOwnership>>,
}

// Keep the owned entry allocation-free. Shared entries retain the exact raw
// snapshot checked on this call; the owner itself carries no prior proof.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum VerifiedInputs {
    Owned {
        module: TypedModule,
        plan: CandidatePlan,
    },
    SharedModule {
        module: Arc<TypedModule>,
        plan: Arc<CandidatePlan>,
    },
    SharedProgram {
        program: Arc<TypedProgram>,
        plan: Arc<CandidatePlan>,
    },
}
impl VerifiedInputs {
    fn program(&self) -> Option<&TypedProgram> {
        match self {
            Self::SharedProgram { program, .. } => Some(program),
            _ => None,
        }
    }
    fn module(&self) -> &TypedModule {
        match self {
            Self::Owned { module, .. } => module,
            Self::SharedModule { module, .. } => module,
            Self::SharedProgram { program, .. } => &program.module,
        }
    }
    fn plan(&self) -> &CandidatePlan {
        match self {
            Self::Owned { plan, .. } => plan,
            Self::SharedModule { plan, .. } | Self::SharedProgram { plan, .. } => plan,
        }
    }
}
impl fmt::Debug for VerifiedModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Preserve the existing module view. Scalar interfaces remain raw;
        // checked task interfaces have their own explicit accessor.
        f.debug_struct("VerifiedModule")
            .field("module", self.module())
            .field("plan", self.plan())
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}
impl VerifiedModule {
    pub fn module(&self) -> &TypedModule {
        self.inputs.module()
    }
    pub fn plan(&self) -> &CandidatePlan {
        self.inputs.plan()
    }
    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
    /// Limits used for this successful verification request. Retaining an
    /// immutable input or a cached candidate never retains an earlier budget.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }
    /// Enables additional lowering work only; it grants no type permission.
    pub fn task_worker_ids(&self) -> &BTreeSet<FunctionId> {
        &self.task_workers
    }
    /// Canonical frame obligations derived from this snapshot's actual checked
    /// resource transfer. Raw IR and candidate plans cannot supply these rows.
    pub fn task_ownership(&self, id: FunctionId) -> Option<&TaskOwnership> {
        self.task_ownership.get(&id)
    }
    pub fn is_suspend(&self, id: FunctionId) -> bool {
        self.suspend_functions.contains(&id)
    }
    pub fn is_task_export(&self, id: FunctionId) -> bool {
        self.task_exports.contains(&id)
    }
    pub fn task_abi_input_fingerprint(&self) -> Fingerprint {
        self.task_input_fingerprint
    }
    pub fn uses_scalar_tasks(&self) -> bool {
        self.uses_scalar_tasks
    }
    /// These semantic exports were independently bound to this exact immutable
    /// program and its actual closed Suspend workers on the current request.
    pub fn uses_retained_registration(&self) -> bool {
        self.uses_retained_registration
    }
    /// Enables downstream work reservation for the actual checked profile.
    /// Layout and instructions must still bind to this immutable module.
    pub fn uses_stored_borrows(&self) -> bool {
        self.uses_stored_borrows
    }
    pub fn task_interfaces(&self) -> &[TaskInterface] {
        self.inputs
            .program()
            .map_or(&[], |program| &program.task_interfaces)
    }
    pub fn uses_owned_sum_extension(&self) -> bool {
        self.uses_owned_sum_extension
    }
    /// Only contracts from this independently checked immutable snapshot are
    /// available to an interface publisher or public caller.
    pub fn callable_contract(&self, function: FunctionId) -> Option<&CallableParameterContract> {
        self.module()
            .functions
            .get(function.0 as usize)?
            .callable_contract
            .as_ref()
    }

    pub fn callable_return_contract(
        &self,
        function: FunctionId,
    ) -> Option<&CallableReturnContract> {
        self.module()
            .functions
            .get(function.0 as usize)?
            .callable_return_contract
            .as_ref()
    }

    /// Derived from the checked snapshot, never from a raw interface claim.
    /// Scalar returns have an empty set; an unknown FunctionId returns None.
    pub fn result_origins(&self, function: FunctionId) -> Option<Vec<u32>> {
        let f = self.module().functions.get(function.0 as usize)?;
        let plan = self.plan().functions.get(function.0 as usize)?;
        if !matches!(f.return_type, Type::Address { .. }) {
            return Some(Vec::new());
        }
        let roots = f
            .blocks
            .iter()
            .filter_map(|block| {
                if let Terminator::Return(Some(value)) = block.terminator {
                    Some(&plan.holds[value.0 as usize].roots)
                } else {
                    None
                }
            })
            .flatten()
            .filter_map(|root| match root {
                HoldRoot::Parameter(index) => Some(*index),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        Some(roots.into_iter().collect())
    }

    /// Program-static storage actually returned by the independently checked CFG.
    /// This observation cannot authorize a raw interface claim or mutable access.
    pub fn result_has_static_origin(&self, function: FunctionId) -> Option<bool> {
        let f = self.module().functions.get(function.0 as usize)?;
        let plan = self.plan().functions.get(function.0 as usize)?;
        if !matches!(f.return_type, Type::Address { .. }) {
            return Some(false);
        }
        Some(f.blocks.iter().any(|block| {
            if let Terminator::Return(Some(value)) = block.terminator {
                plan.holds[value.0 as usize]
                    .roots
                    .contains(&HoldRoot::Static)
            } else {
                false
            }
        }))
    }
}

/// A closed pure CT operation list whose types, dependencies, and budget were
/// checked. Its result is still ordinary untrusted data when re-entering Core.
#[derive(Clone, Debug)]
pub struct VerifiedCTIR {
    module: CtModule,
}
impl VerifiedCTIR {
    pub fn module(&self) -> &CtModule {
        &self.module
    }
}

type Check<T> = Result<T, Diagnostic>;

fn reject(code: &str, span: Option<Span>, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(Phase::Verify, code, span, message)
}
fn unsupported(message: impl Into<String>, span: Option<Span>) -> Diagnostic {
    Diagnostic::unsupported(Phase::Verify, message, span)
}
fn limit(message: impl Into<String>) -> Diagnostic {
    Diagnostic::limit(Phase::Verify, message, None)
}

struct Budget {
    remaining: usize,
    // Derived from raw instructions; only avoids work in unaffected modules.
    static_utf8: bool,
    pod_stores: bool,
    // Raw type roots that actually contain the new Store element family.
    // This is a work gate only; each consumer still checks the complete type.
    inline_store_roots: BTreeSet<usize>,
    inline_store_addresses: bool,
    inline_store_package_addresses: bool,
    sums: bool,
    tasks: bool,
    // Independent paid raw-type inventory; this only enables a proof, not permission.
    task_origin_roots: bool,
    borrowed_task_ports: bool,
    registrations: bool,
    async_registrations: bool,
    protocol_contracts: bool,
    protocol_checked: bool,
    replacements: bool,
    task_workers: BTreeSet<FunctionId>,
    suspend_functions: BTreeSet<FunctionId>,
    task_exports: BTreeSet<FunctionId>,
    task_ownership: std::collections::BTreeMap<FunctionId, TaskOwnership>,
    limits: Limits,
    sum_functions: Vec<bool>,
    // Filled by the already paid closure-definition walk only for newly
    // admitted direct owning Sum capture roots.
    direct_sum_capture_closures: BTreeSet<u32>,
    function_count: usize,
    current_function: Option<usize>,
    private_slot_functions: Option<Vec<bool>>,
    scoped_functions: Option<Vec<bool>>,
    shared_thread_functions: Option<Vec<bool>>,
    borrowed_function_pointers: bool,
    invocation_callbacks: bool,
    owning_address_captures: bool,
    head_descriptions: bool,
    stored_borrows: bool,
    // Eligibility only; pure reads and values still prove their actual shape.
    pure_nominal: bool,
    stored_context: bool,
}
impl Budget {
    fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.max_analysis_steps,
            static_utf8: false,
            pod_stores: false,
            inline_store_roots: BTreeSet::new(),
            inline_store_addresses: false,
            inline_store_package_addresses: false,
            sums: false,
            tasks: false,
            task_origin_roots: false,
            borrowed_task_ports: false,
            registrations: false,
            async_registrations: false,
            protocol_contracts: false,
            protocol_checked: false,
            replacements: false,
            task_workers: BTreeSet::new(),
            suspend_functions: BTreeSet::new(),
            task_exports: BTreeSet::new(),
            task_ownership: std::collections::BTreeMap::new(),
            limits: limits.clone(),
            sum_functions: Vec::new(),
            direct_sum_capture_closures: BTreeSet::new(),
            function_count: 0,
            current_function: None,
            private_slot_functions: None,
            scoped_functions: None,
            shared_thread_functions: None,
            borrowed_function_pointers: false,
            invocation_callbacks: false,
            owning_address_captures: false,
            head_descriptions: false,
            stored_borrows: false,
            pure_nominal: false,
            stored_context: false,
        }
    }
    fn spend(&mut self, amount: usize) -> Check<()> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or_else(|| limit("independent verification work budget exhausted"))?;
        Ok(())
    }
    fn inline_store_root(&mut self, ty: &Type) -> Check<bool> {
        if self.inline_store_roots.is_empty() {
            return Ok(false);
        }
        self.spend(self.inline_store_roots.len().saturating_add(1))?;
        Ok(self
            .inline_store_roots
            .contains(&(ty as *const Type as usize)))
    }
    fn mark_private_slot(&mut self, function: usize) -> Check<()> {
        if self.private_slot_functions.is_none() {
            self.spend(self.function_count)?;
            self.private_slot_functions = Some(vec![false; self.function_count]);
        }
        self.private_slot_functions.as_mut().unwrap()[function] = true;
        Ok(())
    }
    fn mark_sum(&mut self, function: Option<usize>) -> Check<()> {
        self.sums = true;
        if let Some(index) = function {
            if self.sum_functions.is_empty() {
                self.spend(self.function_count)?;
                self.sum_functions = vec![false; self.function_count];
            }
            self.sum_functions[index] = true;
        }
        Ok(())
    }
    fn sum_function(&self, function: FunctionId) -> bool {
        self.sum_functions
            .get(function.0 as usize)
            .copied()
            .unwrap_or(false)
    }
}

/// Verify public candidates against the caller's selected target. Every error
/// returns no wrapper. This function never calls zil-types or zil-analyze.
pub fn verify_module(
    module: TypedModule,
    plan: CandidatePlan,
    expected_target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    verify_inputs(
        VerifiedInputs::Owned { module, plan },
        expected_target,
        limits,
    )
}

/// Independently verify immutable shared raw inputs on every call. An existing
/// Arc, candidate fingerprint, or earlier successful call is not a proof.
pub fn verify_shared_module(
    module: Arc<TypedModule>,
    plan: Arc<CandidatePlan>,
    expected_target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    verify_inputs(
        VerifiedInputs::SharedModule { module, plan },
        expected_target,
        limits,
    )
}

/// Check the embedded module and the independent semantic task export boundary.
/// Scalar interfaces and root mappings remain untrusted. All checks execute on
/// every call with the current target and cumulative request budget.
pub fn verify_shared_program_module(
    program: Arc<TypedProgram>,
    plan: Arc<CandidatePlan>,
    expected_target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    verify_inputs(
        VerifiedInputs::SharedProgram { program, plan },
        expected_target,
        limits,
    )
}

fn verify_inputs(
    inputs: VerifiedInputs,
    expected_target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    match check_module(
        inputs.module(),
        inputs.plan(),
        expected_target,
        limits,
        inputs.program(),
    ) {
        Ok(checked) => Ok(VerifiedModule {
            inputs,
            fingerprint: checked.fingerprint,
            limits: limits.clone(),
            uses_owned_sum_extension: checked.uses_owned_sum_extension,
            uses_scalar_tasks: checked.uses_scalar_tasks,
            uses_retained_registration: checked.uses_retained_registration,
            uses_stored_borrows: checked.uses_stored_borrows,
            task_workers: checked.task_workers,
            suspend_functions: checked.suspend_functions,
            task_exports: checked.task_exports,
            task_input_fingerprint: checked.task_input_fingerprint,
            task_ownership: Arc::new(checked.task_ownership),
        }),
        Err(diagnostic) => Err(vec![diagnostic]),
    }
}

/// Cache decoding returns candidates, then this function executes the same
/// independent checks as a fresh compilation. No cache hash is a safety proof.
pub fn verify_module_bytes(
    bytes: &[u8],
    expected_target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, limits)?;
    verify_module(module, plan, expected_target, limits)
}

fn check_target(actual: &TargetProfile, expected: &TargetProfile) -> Check<()> {
    if actual != expected {
        return Err(reject(
            "E-VERIFY-TARGET",
            None,
            "candidate target differs from the selected target",
        ));
    }
    if actual.rule_version != RULE_VERSION {
        return Err(reject(
            "E-VERIFY-RULE-VERSION",
            None,
            "candidate rule version is not supported",
        ));
    }
    if actual.backend != "llvm" || !matches!(actual.pointer_width, 32 | 64) {
        return Err(unsupported(
            "B0 verification supports explicit 32/64-bit LLVM scalar profiles only",
            None,
        ));
    }
    if !actual.checked_arithmetic {
        return Err(reject(
            "E-VERIFY-ARITHMETIC",
            None,
            "ordinary integer arithmetic requires checked target semantics",
        ));
    }
    Ok(())
}

struct CheckedModule {
    fingerprint: Fingerprint,
    uses_owned_sum_extension: bool,
    uses_scalar_tasks: bool,
    uses_retained_registration: bool,
    uses_stored_borrows: bool,
    task_workers: BTreeSet<FunctionId>,
    suspend_functions: BTreeSet<FunctionId>,
    task_exports: BTreeSet<FunctionId>,
    task_input_fingerprint: Fingerprint,
    task_ownership: std::collections::BTreeMap<FunctionId, TaskOwnership>,
}
fn check_module(
    module: &TypedModule,
    plan: &CandidatePlan,
    expected_target: &TargetProfile,
    limits: &Limits,
    program: Option<&TypedProgram>,
) -> Check<CheckedModule> {
    if module.schema_version != SCHEMA_VERSION || plan.schema_version != SCHEMA_VERSION {
        return Err(reject(
            "E-VERIFY-SCHEMA",
            None,
            "unknown module or candidate schema version",
        ));
    }
    check_target(&module.target, expected_target)?;
    let has_dyn = dynamic_dispatch::present(module);
    if let Some(feature) = module
        .unsupported
        .first()
        .or_else(|| plan.unsupported.first())
    {
        return Err(unsupported(
            format!("unimplemented safety semantics: {}", feature.feature),
            feature.span,
        ));
    }
    if !plan.origins.is_empty() || !plan.responsibilities.is_empty() {
        return Err(unsupported(
            "origin, responsibility, and cleanup certificates are outside the B0 scalar checker",
            None,
        ));
    }
    if module.functions.is_empty() {
        return Err(reject(
            "E-VERIFY-EMPTY",
            None,
            "a B0 executable module must contain a function",
        ));
    }
    if module.functions.len() > limits.max_functions {
        return Err(limit("function count exceeds verification limit"));
    }
    if plan.functions.len() != module.functions.len() {
        return Err(reject(
            "E-VERIFY-PLAN",
            None,
            "candidate plan must describe every function exactly once",
        ));
    }

    let mut budget = Budget::new(limits);
    budget.stored_context = true;
    if has_dyn {
        dynamic_dispatch::inventory(module, &mut budget)?;
    }
    if !module.life_declarations.is_empty() || !module.life_instances.is_empty() {
        stored_flow::inventory(module, limits, &mut budget)?;
        stored_flow::inventory(module, limits, &mut budget)?;
        budget.stored_borrows = true;
    }
    budget.function_count = module.functions.len();
    resources::check_definitions(module, limits, &mut budget)?;
    for text in [
        &module.target.id,
        &module.target.triple,
        &module.target.backend,
        &module.target.rule_version,
    ] {
        budget.spend(text.len())?;
    }
    // Bound every aggregate shape before definitions, equality, cloning or
    // fingerprinting. Raw APIs receive no protection from a source producer.
    for (index, function) in module.functions.iter().enumerate() {
        budget.protocol_contracts |= function.protocol_contract.is_some();
        if function.storage_life.is_some()
            && !tasks::borrowed_signature(module, function, &mut budget)?
        {
            stored_flow::header(module, function, &mut budget)?;
            budget.stored_borrows = true;
        }
        budget.current_function = Some(index);
        for ty in std::iter::once(&function.return_type)
            .chain(function.locals.iter().map(|l| &l.ty))
            .chain(&function.values)
        {
            check_type_shape(ty, &module.target, limits, &mut budget)?;
        }
        check_reference_return_bound(function, &mut budget)?;
    }
    budget.current_function = None;
    for ty in module.closures.iter().flat_map(|c| {
        c.captures
            .iter()
            .chain(c.parameters.iter())
            .chain(std::iter::once(&c.result))
    }) {
        check_type_shape(ty, &module.target, limits, &mut budget)?;
    }
    registration::storage(module, &mut budget)?;
    closures::check_definitions(module, limits, &mut budget)?;
    foreign::check_declarations(module, &mut budget)?;
    let mut names = BTreeSet::new();
    let (mut block_count, mut instruction_count, mut local_count, mut value_count) =
        (0usize, 0usize, 0usize, 0usize);
    for (index, function) in module.functions.iter().enumerate() {
        if function.call_kind == CallKind::Suspend {
            budget.tasks = true;
        }
        helpers::check_claim_shape(function, limits, &mut budget)?;
        retained::check_claim(module, function, limits, &mut budget)?;
        let mut private_slot = false;
        budget.spend(function.name.len().saturating_add(1))?;
        if function.id.0 as usize != index {
            return Err(reject(
                "E-VERIFY-ID",
                Some(function.span),
                "FunctionId must equal its vector index",
            ));
        }
        if !names.insert(&function.name) {
            return Err(reject(
                "E-VERIFY-SYMBOL",
                Some(function.span),
                "duplicate function name",
            ));
        }
        if function.name.is_empty() {
            return Err(reject(
                "E-VERIFY-SYMBOL",
                Some(function.span),
                "empty function name",
            ));
        }
        if !matches!(function.return_type, Type::Closure(_)) {
            check_function_signature_type(
                module,
                &function.return_type,
                Some(function.span),
                &mut budget,
            )?;
        }
        budget.spend(function.parameters.len())?;
        for local in &function.locals {
            budget.spend(local.name.len().saturating_add(1))?;
            private_slot |= check_local_type(&local.ty, Some(local.span), module, &mut budget)?;
        }
        for ty in &function.values {
            budget.spend(1)?;
            private_slot |= check_local_type(ty, Some(function.span), module, &mut budget)?;
        }
        if private_slot {
            budget.mark_private_slot(index)?;
        }
        for parameter in &function.parameters {
            let ty = local_type(function, *parameter, Some(function.span))?;
            if let Some((id, _)) = helpers::parameter_type(ty) {
                closures::definition(module, id, Some(function.span))?;
            } else {
                check_function_signature_type(module, ty, Some(function.span), &mut budget)?;
            }
        }
        block_count = add_count(
            block_count,
            function.blocks.len(),
            limits.max_blocks,
            "block",
        )?;
        local_count = add_count(
            local_count,
            function.locals.len(),
            limits.max_locals,
            "local",
        )?;
        value_count = add_count(
            value_count,
            function.values.len(),
            limits.max_values,
            "value",
        )?;
        for block in &function.blocks {
            instruction_count = add_count(
                instruction_count,
                block.instructions.len(),
                limits.max_instructions,
                "instruction",
            )?;
            budget.spend(1)?;
            for instruction in &block.instructions {
                budget.spend(1)?;
                match &instruction.kind {
                    InstructionKind::ReservePlaceLocal { path, .. }
                    | InstructionKind::ReservePlaceAddress { path, .. }
                    | InstructionKind::TakeTupleField { path, .. }
                    | InstructionKind::TakeVariantField { path, .. }
                    | InstructionKind::BorrowPlaceLocal { path, .. }
                    | InstructionKind::PlaceTagLocal { path, .. }
                    | InstructionKind::BorrowPlaceAddress { path, .. }
                    | InstructionKind::PlaceTagAddress { path, .. } => {
                        if path.len() > limits.max_call_depth {
                            return Err(limit(
                                "mixed/replacement path exceeds depth limit before fingerprint",
                            ));
                        }
                        budget.spend(
                            path.len()
                                .checked_mul(24)
                                .and_then(|n| n.checked_add(1))
                                .ok_or_else(|| {
                                    limit("mixed/replacement preflight path work overflow")
                                })?,
                        )?;
                        if matches!(
                            instruction.kind,
                            InstructionKind::ReservePlaceLocal { .. }
                                | InstructionKind::ReservePlaceAddress { .. }
                        ) {
                            budget.replacements = true;
                        }
                    }
                    InstructionKind::BorrowVariantLocal { path, .. }
                    | InstructionKind::BorrowVariantAddress { path, .. }
                    | InstructionKind::VariantTagLocal { path, .. }
                    | InstructionKind::VariantTagAddress { path, .. }
                    | InstructionKind::ReadVariantLocal { path, .. } => {
                        if path.len() > limits.max_call_depth {
                            return Err(limit(
                                "variant path exceeds depth limit before fingerprint",
                            ));
                        }
                        budget.spend(
                            path.len()
                                .checked_mul(24)
                                .and_then(|n| n.checked_add(1))
                                .ok_or_else(|| limit("variant preflight path work overflow"))?,
                        )?;
                    }
                    InstructionKind::BorrowFieldLocal { path, .. } => {
                        if path.len() > limits.max_call_depth {
                            return Err(limit("field path exceeds depth limit before fingerprint"));
                        }
                        budget.spend(
                            path.len()
                                .checked_mul(12)
                                .and_then(|n| n.checked_add(1))
                                .ok_or_else(|| limit("field preflight path work overflow"))?,
                        )?;
                    }
                    InstructionKind::CommitPlaceReplace { .. }
                    | InstructionKind::AbortPlaceReservation { .. } => {
                        budget.replacements = true;
                    }
                    InstructionKind::StaticUtf8 { bytes } => {
                        budget.static_utf8 = true;
                        if bytes.len() > limits.max_nodes {
                            return Err(limit("static UTF8 byte count exceeds the raw bound"));
                        }
                        // Reserve byte validation and module fingerprint traversal first.
                        budget.spend(bytes.len().saturating_mul(2))?;
                        if std::str::from_utf8(bytes).is_err() {
                            return Err(reject(
                                "E-VERIFY-UTF8",
                                Some(instruction.span),
                                "program-static text contains invalid UTF8",
                            ));
                        }
                    }
                    InstructionKind::Constant(constant) => native_values::constant(
                        constant,
                        &module.target,
                        Some(instruction.span),
                        limits,
                        &mut budget,
                    )?,
                    InstructionKind::MakeManaged { fields }
                    | InstructionKind::MakePackage { fields }
                    | InstructionKind::MakeOwnedArray { elements: fields }
                    | InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakeOwnedTuple { fields }
                    | InstructionKind::MakeVariant { fields, .. } => budget.spend(fields.len())?,
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::SuspendCall { arguments, .. }
                    | InstructionKind::ForeignCall { arguments, .. } => {
                        budget.spend(arguments.len())?
                    }
                    InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. } => {
                        budget.spend(arguments.len().saturating_add(1))?
                    }
                    InstructionKind::WithForeignCallback { .. } => {
                        budget.spend(3)?;
                        if budget.scoped_functions.is_none() {
                            budget.spend(budget.function_count)?;
                            budget.scoped_functions = Some(vec![false; budget.function_count]);
                        }
                        budget.scoped_functions.as_mut().unwrap()[index] = true;
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        budget.spend(captures.len())?
                    }
                    InstructionKind::ConvertInteger { target, .. }
                    | InstructionKind::ConvertNumber { target, .. } => {
                        // This embedded raw type is not necessarily one of the
                        // function value types. Bound it before fingerprinting.
                        check_type_shape(target, &module.target, limits, &mut budget)?;
                    }
                    InstructionKind::OwnedStoreNew { element } => {
                        check_type_shape(element, &module.target, limits, &mut budget)?;
                    }
                    InstructionKind::Unsupported(feature) => {
                        return Err(unsupported(
                            format!("unknown or unimplemented instruction: {feature}"),
                            Some(instruction.span),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    // Deep unsupported type trees have already been rejected without recursing.
    // A candidate's digest is an identity check, not evidence of correctness.
    let stored_partition = if budget.stored_borrows && (budget.tasks || budget.registrations) {
        stored_partition::mixed(module, &mut budget)?
    } else {
        None
    };
    if budget.stored_borrows {
        stored_flow::qualifications(module, limits, stored_partition.as_deref(), &mut budget)?;
    }
    let fingerprint = module.fingerprint();
    if plan.module_fingerprint != fingerprint
        || plan.target_fingerprint != module.target.fingerprint()
    {
        return Err(reject(
            "E-VERIFY-PLAN-IDENTITY",
            None,
            "candidate plan is bound to different module or target bytes",
        ));
    }
    let mut graphs = Vec::with_capacity(module.functions.len());
    for (function, function_plan) in module.functions.iter().zip(&plan.functions) {
        if function_plan.function != function.id {
            return Err(reject(
                "E-VERIFY-PLAN",
                Some(function.span),
                "candidate function order or identity is incorrect",
            ));
        }
        let graph = check_function(module, function, function_plan, &mut budget, limits)?;
        if budget
            .private_slot_functions
            .as_ref()
            .is_some_and(|rows| rows[function.id.0 as usize])
        {
            resources::arrays::check_internal_slot_uses(module, function, &graph, &mut budget)?;
        }
        retained::check_body(module, function, &graph, &mut budget)?;
        graphs.push(graph);
    }
    tasks::module(module, &mut budget)?;
    let mut task_input_fingerprint = fingerprint;
    if let Some(program) = program {
        tasks::interfaces(program, &mut budget)?;
        task_input_fingerprint = tasks::interface_fingerprint(program, fingerprint, &mut budget)?;
    }
    helpers::check_module(module, &graphs, limits, &mut budget)?;
    if budget.stored_borrows && stored_partition.is_none() {
        for f in &module.functions {
            stored_flow::header(module, f, &mut budget)?;
        }
        stored_flow::check(module, &graphs, plan, &mut budget)?;
    } else if let Some(mask) = stored_partition.as_deref() {
        for (index, f) in module.functions.iter().enumerate() {
            if mask[index] {
                stored_flow::header(module, f, &mut budget)?;
            }
        }
        stored_flow::check_mixed(module, &graphs, plan, mask, &mut budget)?;
        borrows::check_mixed(module, plan, &graphs, mask, &mut budget)?;
    } else {
        borrows::check_module(module, plan, &graphs, &mut budget)?;
    }
    head_access::check(module, &graphs, &mut budget)?;
    resources::check_module(module, &graphs, &mut budget)?;
    registration::check_phase(module, &mut budget)?;
    Ok(CheckedModule {
        fingerprint,
        uses_owned_sum_extension: budget.sums,
        uses_scalar_tasks: budget.tasks,
        uses_retained_registration: budget.registrations,
        uses_stored_borrows: budget.stored_borrows,
        task_workers: budget.task_workers,
        suspend_functions: budget.suspend_functions,
        task_exports: budget.task_exports,
        task_input_fingerprint,
        task_ownership: budget.task_ownership,
    })
}

fn add_count(before: usize, add: usize, maximum: usize, kind: &str) -> Check<usize> {
    match before.checked_add(add) {
        Some(total) if total <= maximum => Ok(total),
        _ => Err(limit(format!("{kind} count exceeds verification limit"))),
    }
}
fn check_anonymous_sum(ty: &Type, limits: &Limits, budget: &mut Budget) -> Check<()> {
    let Type::Enum { name, variants } = ty else {
        unreachable!()
    };
    if name != "@anonymous-sum:v1" || variants.len() < 2 {
        return Err(reject(
            "E-VERIFY-ENUM",
            None,
            "invalid anonymous sum family or arity",
        ));
    }
    let mut identity_work = 0usize;
    let mut pending = Vec::new();
    for (index, (label, fields)) in variants.iter().enumerate() {
        budget.spend(32)?;
        if label != &format!("Member{index}") || fields.len() != 1 {
            return Err(reject(
                "E-VERIFY-ENUM",
                None,
                "anonymous sum requires canonical ordinal labels and one payload per member",
            ));
        }
        if matches!(&fields[0], Type::Enum { name, .. } if name == "@anonymous-sum:v1") {
            return Err(reject(
                "E-VERIFY-ENUM",
                None,
                "anonymous sum directly nests another anonymous sum",
            ));
        }
        pending.push((&fields[0], 0usize));
    }
    while let Some((ty, depth)) = pending.pop() {
        tasks::require_closed_context(ty, None)?;
        budget.spend(1)?;
        identity_work = identity_work.saturating_add(1);
        if depth > limits.max_call_depth {
            return Err(limit(
                "anonymous sum identity nesting exceeds verification limit",
            ));
        }
        let text = match ty {
            Type::Record { name, fields } | Type::Package { name, fields } => {
                for (label, field) in fields {
                    budget.spend(label.len())?;
                    identity_work = identity_work.saturating_add(label.len());
                    pending.push((field, depth + 1));
                }
                name.len()
            }
            Type::Enum { name, variants } => {
                for (label, fields) in variants {
                    budget.spend(label.len())?;
                    identity_work = identity_work.saturating_add(label.len());
                    pending.extend(fields.iter().map(|field| (field, depth + 1)));
                }
                name.len()
            }
            Type::Tuple(fields) => {
                pending.extend(fields.iter().map(|field| (field, depth + 1)));
                0
            }
            Type::Task {
                result: element, ..
            }
            | Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element) => {
                pending.push((element, depth + 1));
                0
            }
            Type::ErrorUnion { error, result } => {
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
                0
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                pending.extend(parameters.iter().map(|field| (field, depth + 1)));
                pending.push((result, depth + 1));
                0
            }
            Type::Named(_) | Type::Unsupported(_) | Type::Opaque { .. } | Type::Closure(_) => {
                return Err(reject(
                    "E-VERIFY-ENUM",
                    None,
                    "anonymous sum identity contains an unresolved or nonordinary member type",
                ));
            }
            _ => 0,
        };
        budget.spend(text)?;
        identity_work = identity_work.saturating_add(text);
    }
    budget.spend(identity_work.saturating_mul(2))?;
    if variants.windows(2).any(|pair| pair[0].1[0] >= pair[1].1[0]) {
        return Err(reject(
            "E-VERIFY-ENUM",
            None,
            "anonymous sum members are repeated or not in complete-type canonical order",
        ));
    }
    Ok(())
}

fn check_type_shape(
    ty: &Type,
    target: &TargetProfile,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    let mut first = Some((ty, 0usize, 0u8));
    let mut pending = Vec::new();
    let mut options = Vec::new();
    let mut anonymous = Vec::new();
    let mut pod_stores = Vec::new();
    while let Some((ty, depth, context)) = first.take().or_else(|| pending.pop()) {
        budget.spend(1)?;
        budget.task_origin_roots |= matches!(ty, Type::Task { .. });
        budget.borrowed_task_ports |=
            matches!(ty, Type::Task { retained_life, .. } if !retained_life.ports.is_empty());
        tasks::require_closed_context(ty, None)?;
        // Detect the new ordinary profile during the already paid raw-tree
        // traversal. Old functions do not acquire an extra classifier pass.
        if (context & 64 != 0 && matches!(ty, Type::Managed(_) | Type::Package { .. }))
            || (context & 1 != 0
                && matches!(
                    ty,
                    Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
                ))
            || (context & 8 != 0 && matches!(ty, Type::Managed(_) | Type::Package { .. }))
            || (context & 32 != 0 && context & 4 != 0 && matches!(ty, Type::Package { .. }))
            || (context & 2 != 0
                && context & 16 != 0
                && matches!(ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. })))
            || (context & 2 != 0 && dynamic_store::inline_owner(ty))
        {
            budget.mark_sum(budget.current_function)?;
        }
        if depth > limits.max_call_depth {
            return Err(limit("aggregate type nesting exceeds verification limit"));
        }
        if matches!(
            ty,
            Type::ForeignHandle
                | Type::CWideString
                | Type::ForeignCallback { .. }
                | Type::ByteBuffer
        ) {
            native_values::target(target, None)?;
        }
        match ty {
            Type::BorrowedStorage { storage, .. } => {
                if !budget.stored_context {
                    return Err(unsupported(
                        "stored type requires an actual runtime nominal inventory context",
                        None,
                    ));
                }
                stored_flow::physical(ty, limits, budget)?;
                budget.stored_borrows = true;
                pending.push((storage, depth + 1, context));
            }
            Type::Dyn { .. } => {}
            Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::OwnedStore { element } => {
                budget.async_registrations |= matches!(ty, Type::AsyncRegistration { .. });
                if matches!(ty, Type::OwnedStore { .. })
                    && (matches!(element.as_ref(), Type::Record { .. })
                        || dynamic_store::direct_scalar(element))
                {
                    // Ordinary owning Product/Sum positions retain the same
                    // sealed Store identity and independent resource check.
                    // Bit 128 still excludes plain Records and owning arrays.
                    if context & 128 != 0 {
                        return Err(unsupported(
                            "POD Store ownership is not admitted inside owning aggregate fields",
                            None,
                        ));
                    }
                    budget.spend(1)?;
                    pod_stores.push(element.as_ref());
                }
                pending.push((element, depth + 1, context));
            }
            Type::Slice { element } => {
                slices::target(target)?;
                pending.push((element, depth + 1, context));
            }
            Type::Array { element, length } => {
                budget.spend(*length as usize)?;
                pending.push((element, depth + 1, context | 128));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                for ty in fields {
                    pending.push((ty, depth + 1, context | 64));
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                budget.spend(name.len())?;
                budget.spend(fields.len())?;
                let context = context
                    | if matches!(ty, Type::Package { .. }) {
                        4 | if context & 4 != 0 { 16 } else { 0 }
                    } else {
                        128
                    };
                for (name, ty) in fields {
                    budget.spend(name.len())?;
                    pending.push((ty, depth + 1, context));
                }
            }
            Type::Enum { name, variants } => {
                if name.starts_with("@anonymous-sum") {
                    budget.spend(1)?;
                    anonymous.push(ty);
                }
                budget.spend(name.len().saturating_add(variants.len()))?;
                if name.starts_with("@builtin:Option") {
                    options.push((name, variants));
                }
                for (name, fields) in variants {
                    budget.spend(name.len().saturating_add(fields.len()))?;
                    for ty in fields {
                        pending.push((
                            ty,
                            depth + 1,
                            context | 1 | if context & 4 != 0 { 8 } else { 0 },
                        ));
                    }
                }
            }
            Type::Opaque { hidden, .. } => {
                pending.push((hidden, depth + 1, context));
            }
            Type::Task { result: inner, .. }
            | Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner) => {
                pending.push((inner, depth + 1, context));
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                // Record the gate while visiting the existing paid nodes;
                // do not scan an as-yet-unbounded raw Package field list here.
                let context = context
                    | 2
                    | if context & (4 | 2) != 0 { 8 } else { 0 }
                    | if **error != Type::Unit && **result != Type::Unit {
                        32
                    } else {
                        0
                    };
                pending.push((error, depth + 1, context));
                pending.push((result, depth + 1, context));
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                budget.borrowed_function_pointers = true;
                crate::signature_life::charge(life, budget)?;
                for ty in parameters.iter().chain(std::iter::once(result.as_ref())) {
                    stored_flow::type_work(ty, limits, budget)?;
                }
                borrowed_fp::signature_work(parameters, result, budget)?;
                borrowed_fp::relation(parameters, result, life, None)?;
                pending.push((result, depth + 1, context));
                for parameter in parameters {
                    pending.push((parameter, depth + 1, context));
                }
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len())?;
                pending.push((result, depth + 1, context));
                for parameter in parameters {
                    pending.push((parameter, depth + 1, context));
                }
            }
            _ => {}
        }
    }
    // Unused local/SSA/signature/definition types receive the same independent
    // POD qualification, after the whole raw tree has passed the depth bound.
    let inline_store_root = !pod_stores.is_empty()
        && (matches!(
            ty,
            Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
        ) || matches!(ty, Type::Address { inner, .. }
            if matches!(inner.as_ref(), Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. })
                || matches!(inner.as_ref(), Type::Managed(payload) if matches!(payload.as_ref(), Type::Package { .. }))
                || matches!(inner.as_ref(), Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))));
    for element in pod_stores {
        dynamic_store::check_pod_element(element, target, None, budget)?;
        budget.pod_stores = true;
        budget.head_descriptions = true;
    }
    if inline_store_root {
        budget.spend(budget.inline_store_roots.len().saturating_add(1))?;
        budget.inline_store_roots.insert(ty as *const Type as usize);
        budget.inline_store_addresses |= matches!(ty, Type::Address { inner, .. }
            if matches!(inner.as_ref(), Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }));
        budget.inline_store_package_addresses |= matches!(ty, Type::Address { inner, .. }
            if matches!(inner.as_ref(), Type::Package { .. }));
    }
    // Fingerprint only after the complete raw type tree has been bounded.
    // This independently checks the sealed family key and payload shape.
    for (name, variants) in options {
        let [(none, empty), (some, payload)] = variants.as_slice() else {
            return Err(reject(
                "E-VERIFY-OPTION",
                None,
                "Option must have exactly None and Some variants",
            ));
        };
        let [payload] = payload.as_slice() else {
            return Err(reject(
                "E-VERIFY-OPTION",
                None,
                "Some requires exactly one payload",
            ));
        };
        if none != "None" || !empty.is_empty() || some != "Some" {
            return Err(reject(
                "E-VERIFY-OPTION",
                None,
                "invalid Option variant shape",
            ));
        }
        resources::charge_payload(payload, budget)?;
        if *name != format!("@builtin:Option:{}", payload.fingerprint()) {
            return Err(reject(
                "E-VERIFY-OPTION",
                None,
                "Option identity does not match its complete payload type",
            ));
        }
    }
    for ty in anonymous {
        check_anonymous_sum(ty, limits, budget)?;
    }
    Ok(())
}

// Called only after check_type_shape has bounded the raw tree. Keeping
// admission independent from producer helpers is part of the trust boundary.
fn check_type(ty: &Type, span: Option<Span>) -> Check<()> {
    let mut first = Some(ty);
    let mut pending = Vec::new();
    while let Some(ty) = first.take().or_else(|| pending.pop()) {
        match ty {
            // Qualification and every physical leaf have already passed the
            // bounded module preflight and exact instance correspondence.
            Type::BorrowedStorage { .. } => {}
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::F32
            | Type::F64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::ForeignHandle
            | Type::CWideString => {}
            Type::Array { element, .. } => pending.push(element),
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            Type::Tuple(fields) => pending.extend(fields),
            Type::FunctionPointer { parameters, result } => {
                pending.extend(parameters);
                pending.push(result);
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                borrowed_fp::relation(parameters, result, life, span)?;
                for parameter in parameters {
                    check_signature_type(parameter, span)?;
                }
                check_signature_type(result, span)?;
            }
            Type::ForeignCallback { parameters, result } => {
                native_values::callback_signature(parameters, result, span)?;
            }
            Type::Record { name, fields } => {
                if name.is_empty() {
                    return Err(reject(
                        "E-VERIFY-AGGREGATE",
                        span,
                        "empty nominal record identity",
                    ));
                }
                let mut names = BTreeSet::new();
                for (name, ty) in fields {
                    if name.is_empty() || !names.insert(name) {
                        return Err(reject(
                            "E-VERIFY-AGGREGATE",
                            span,
                            "record fields must have unique nonempty names",
                        ));
                    }
                    pending.push(ty);
                }
            }
            Type::Enum { name, variants } => {
                if name.is_empty() {
                    return Err(reject("E-VERIFY-ENUM", span, "empty nominal enum identity"));
                }
                let mut names = BTreeSet::new();
                for (name, fields) in variants {
                    if name.is_empty() || !names.insert(name) {
                        return Err(reject(
                            "E-VERIFY-ENUM",
                            span,
                            "enum variants must have unique nonempty names",
                        ));
                    }
                    pending.extend(fields);
                }
            }
            _ => return Err(unsupported("non-Copy type requires another checker", span)),
        }
    }
    Ok(())
}
fn affine_type(module: &TypedModule, ty: &Type) -> bool {
    match ty {
        Type::Address { mutable: true, .. } => true,
        Type::Closure(id) => module.closures[id.0 as usize]
            .captures
            .iter()
            .any(|t| matches!(t, Type::Address { mutable: true, .. })),
        _ => false,
    }
}
fn check_signature_type(ty: &Type, span: Option<Span>) -> Check<()> {
    if ty.is_shared_sync_cell_address() {
        return Ok(());
    }
    match ty {
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::BorrowedStorage { .. }) => {
            Ok(())
        }
        Type::Address { inner, .. } => slices::check_pointee(inner, span),
        _ => check_type(ty, span),
    }
}

fn check_function_signature_type(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    if matches!(ty, Type::BorrowedStorage { .. }) {
        // The paid stored_flow::qualifications pass checks this same complete
        // signature type after all local/value shape preflights. Do not repeat
        // its deep nominal scan at every signature occurrence.
        return if budget.stored_borrows {
            Ok(())
        } else {
            Err(reject(
                "E-VERIFY-STORED",
                span,
                "stored nominal type lacks its instance inventory",
            ))
        };
    }
    if stored_flow::pure_inline(module, ty, budget)? {
        return Ok(());
    }
    if matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Dyn { .. })) {
        return dynamic_dispatch::type_address(module, ty, span, budget);
    }
    match ty {
        Type::Address { inner, .. }
            if budget.inline_store_root(ty)?
                && matches!(inner.as_ref(), Type::Package { .. })
                && dynamic_store::package_inline_store(inner, budget)? =>
        {
            packages::check_ordinary_type(module, inner, span, budget)
        }
        Type::Address { inner, .. }
            if packages::has_children(inner) || packages::shared_product_head(inner) =>
        {
            packages::check_shared_product_type(module, inner, span, budget)
        }
        Type::Address { inner, .. }
            if matches!(
                inner.as_ref(),
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) =>
        {
            sums::check_type_for(module, inner, span, budget)
        }
        Type::Address { inner, .. } if ty.is_package_address() => {
            dynamic_store::check_element(module, inner, span, budget)
        }
        Type::Address { inner, .. }
            if resources::head_address(ty) || ty.is_exclusive_task_head_address() =>
        {
            resources::check_type_for(module, inner, span, budget)
        }
        Type::Managed(_) | Type::Desc(_) => resources::check_type_for(module, ty, span, budget),
        Type::Package { .. } => packages::check_ordinary_type(module, ty, span, budget),
        Type::ErrorUnion { .. } => errors::check_type_for(module, ty, span, budget),
        Type::Enum { .. } | Type::Tuple(_) if budget.sums && sums::owned(ty) => {
            sums::check_type_for(module, ty, span, budget)
        }
        ty if resources::arrays::is_owned_type(ty) => {
            resources::arrays::check_type_for(module, ty, span, budget)
        }
        _ => check_signature_type(ty, span),
    }
}

fn check_local_type(
    ty: &Type,
    span: Option<Span>,
    module: &TypedModule,
    budget: &mut Budget,
) -> Check<bool> {
    if matches!(ty, Type::BorrowedStorage { .. }) {
        return if budget.stored_borrows {
            Ok(false)
        } else {
            Err(reject(
                "E-VERIFY-STORED",
                span,
                "stored nominal type lacks its instance inventory",
            ))
        };
    }
    if stored_flow::pure_inline(module, ty, budget)? {
        return Ok(false);
    }
    if matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Dyn { .. })) {
        dynamic_dispatch::type_address(module, ty, span, budget)?;
        return Ok(false);
    }
    if let Type::Address {
        inner,
        mutable: false,
    } = ty
        && stored_flow::pure_inline(module, inner, budget)?
    {
        return Ok(false);
    }
    if resources::arrays::is_owned_type(ty) {
        return resources::arrays::check_internal_single_slot_type_for(module, ty, span, budget);
    }
    let checked = match ty {
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::BorrowedStorage { .. }) => {
            Ok(())
        }
        ty if ty.is_shared_sync_cell_address() => Ok(()),
        Type::Closure(id) if (id.0 as usize) < module.closures.len() => Ok(()),
        Type::Closure(_) => Err(reject(
            "E-VERIFY-CLOSURE",
            span,
            "local closure type references an absent environment",
        )),
        Type::Address { inner, .. }
            if budget.inline_store_root(ty)?
                && matches!(inner.as_ref(), Type::Package { .. })
                && dynamic_store::package_inline_store(inner, budget)? =>
        {
            packages::check_ordinary_type(module, inner, span, budget)
        }
        Type::Address { inner, .. }
            if packages::has_children(inner) || packages::shared_product_head(inner) =>
        {
            packages::check_shared_product_type(module, inner, span, budget)
        }
        Type::Address { inner, .. }
            if matches!(
                inner.as_ref(),
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) =>
        {
            sums::check_type_for(module, inner, span, budget)
        }
        Type::Address { inner, .. } if ty.is_package_address() => {
            dynamic_store::check_element(module, inner, span, budget)
        }
        Type::Address { inner, .. }
            if resources::head_address(ty) || ty.is_exclusive_task_head_address() =>
        {
            resources::check_type_for(module, inner, span, budget)
        }
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => closures::definition(module, *id, span).map(|_| ()),
            _ => slices::check_pointee(inner, span),
        },
        Type::Managed(_) | Type::Desc(_) => resources::check_type_for(module, ty, span, budget),
        Type::Package { .. } => packages::check_ordinary_type(module, ty, span, budget),
        Type::ErrorUnion { .. } => errors::check_type_for(module, ty, span, budget),
        Type::Enum { .. } | Type::Tuple(_) if budget.sums && sums::owned(ty) => {
            sums::check_type_for(module, ty, span, budget)
        }
        _ => check_type(ty, span),
    };
    checked.map(|_| false)
}

struct FunctionGraph {
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    reachable: Vec<bool>,
    package_views: packages::CaptureViews,
    role_data_uses: Vec<ValueId>,
}

fn check_reference_return_bound(f: &TypedFunction, budget: &mut Budget) -> Check<()> {
    let Some(bound) = &f.reference_return_bound else {
        return Ok(());
    };
    budget.spend(bound.len().saturating_mul(2).saturating_add(1))?;
    resources::charge_payload(&f.return_type, budget)?;
    let fail = || {
        reject(
            "E-VERIFY-LIFE",
            Some(f.span),
            "invalid address direct return-life upper bound",
        )
    };
    if bound.is_empty()
        || f.environment.is_some()
        || f.foreign.is_some()
        || f.callable_contract.is_some()
        || !matches!(&f.return_type,Type::Address{inner,..} if inner.is_address_pointee())
    {
        return Err(fail());
    }
    let mut previous = None;
    for &position in bound {
        if previous.is_some_and(|last| last >= position) {
            return Err(fail());
        }
        let parameter = f
            .parameters
            .get(position as usize)
            .and_then(|id| f.locals.get(id.0 as usize))
            .ok_or_else(fail)?;
        resources::charge_payload(&parameter.ty, budget)?;
        if !matches!(&parameter.ty,Type::Address{inner,..} if inner.is_address_pointee()) {
            return Err(fail());
        }
        previous = Some(position);
    }
    Ok(())
}

fn check_function(
    module: &TypedModule,
    function: &TypedFunction,
    plan: &FunctionPlan,
    budget: &mut Budget,
    limits: &Limits,
) -> Check<FunctionGraph> {
    let fail = |code, message| reject(code, Some(function.span), message);
    let n = function.blocks.len();
    let m = function.locals.len();
    if function.foreign.is_some() {
        if !plan.blocks.is_empty() || !plan.holds.is_empty() {
            return Err(fail(
                "E-VERIFY-FOREIGN",
                "foreign declarations cannot carry CFG or Hold claims",
            ));
        }
        return Ok(FunctionGraph {
            successors: Vec::new(),
            predecessors: Vec::new(),
            reachable: Vec::new(),
            package_views: packages::CaptureViews::default(),
            role_data_uses: Vec::new(),
        });
    }
    if n == 0 || function.entry.0 as usize >= n {
        return Err(fail("E-VERIFY-CFG", "function has no valid entry block"));
    }
    if plan.blocks.len() != n {
        return Err(fail(
            "E-VERIFY-PLAN",
            "candidate plan must describe every block exactly once",
        ));
    }
    for (i, local) in function.locals.iter().enumerate() {
        if local.id.0 as usize != i {
            return Err(fail("E-VERIFY-ID", "LocalId must equal its vector index"));
        }
    }
    let mut parameters = vec![false; m];
    for local in &function.parameters {
        let p = parameters
            .get_mut(local.0 as usize)
            .ok_or_else(|| fail("E-VERIFY-ID", "parameter references an absent local"))?;
        if *p {
            return Err(fail("E-VERIFY-PARAMETER", "duplicate parameter local"));
        }
        *p = true;
    }
    if matches!(function.return_type, Type::Address { .. }) {
        budget.spend(function.locals.len().saturating_add(function.values.len()))?;
        let mut store_parent_address = false;
        if budget.inline_store_package_addresses {
            for ty in function
                .locals
                .iter()
                .map(|local| &local.ty)
                .chain(&function.values)
            {
                if let Type::Address { inner, .. } = ty
                    && ty.is_package_address()
                    && budget.inline_store_root(ty)?
                    && dynamic_store::package_inline_store(inner, budget)?
                {
                    store_parent_address = true;
                    break;
                }
            }
        }
        if function.return_type.is_package_address()
            || store_parent_address
            || function
                .locals
                .iter()
                .map(|local| &local.ty)
                .chain(&function.values)
                .any(|ty| matches!(ty,Type::Address{inner,..} if ty.is_package_address() && packages::has_store_fields(inner)))
        {
            return Err(unsupported(
                "Store element windows are synchronous inputs; address results derived from element windows are not implemented",
                Some(function.span),
            ));
        }
    }
    if function.return_type.is_shared_resource_head_address()
        || (matches!(function.return_type, Type::Address { .. })
            && function.parameters.iter().any(|local| {
                function.locals[local.0 as usize]
                    .ty
                    .is_shared_resource_head_address()
            }))
    {
        return Err(unsupported(
            "resource-head windows are synchronous inputs; head-derived returns require a separate contract",
            Some(function.span),
        ));
    }
    let mut predecessors = vec![Vec::<usize>::new(); n];
    let mut successors = vec![Vec::<usize>::new(); n];
    let mut definitions = vec![None; function.values.len()];
    let mut addressed_sums = false;
    let mut mixed_address_sums = false;
    let mut immutable_stores = vec![0usize; m];
    for (block_index, block) in function.blocks.iter().enumerate() {
        budget.spend(1)?;
        if block.id.0 as usize != block_index {
            return Err(fail("E-VERIFY-ID", "BlockId must equal its vector index"));
        }
        if plan.blocks[block_index].block != block.id {
            return Err(fail(
                "E-VERIFY-PLAN",
                "candidate block order or identity is incorrect",
            ));
        }
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            if let Some(result) = instruction.result {
                let definition = definitions.get_mut(result.0 as usize).ok_or_else(|| {
                    fail(
                        "E-VERIFY-ID",
                        "instruction result references an absent value",
                    )
                })?;
                if definition
                    .replace((block_index, instruction_index))
                    .is_some()
                {
                    return Err(fail(
                        "E-VERIFY-SSA",
                        "an SSA value has more than one definition",
                    ));
                }
            }
            mixed_address_sums |= matches!(
                instruction.kind,
                InstructionKind::BorrowPlaceAddress { .. }
                    | InstructionKind::PlaceTagAddress { .. }
            );
            addressed_sums |= matches!(
                instruction.kind,
                InstructionKind::VariantTagAddress { .. }
                    | InstructionKind::BorrowVariantAddress { .. }
            );
            check_instruction(module, function, instruction, budget)?;
            if let InstructionKind::Store { local, .. }
            | InstructionKind::BindPackageCapture { local, .. } = instruction.kind
            {
                let index = local.0 as usize;
                if !function.locals[index].mutable {
                    immutable_stores[index] += 1;
                    if parameters[index] || immutable_stores[index] > 1 {
                        return Err(reject(
                            "E-VERIFY-MUTABILITY",
                            Some(instruction.span),
                            "immutable local has an assignment beyond its single initialization site",
                        ));
                    }
                }
            }
            if let InstructionKind::UnpackVariant { ref fields, .. }
            | InstructionKind::UnpackTuple { ref fields, .. } = instruction.kind
            {
                budget.spend(fields.len())?;
                for target in fields {
                    let index = target.0 as usize;
                    if !function.locals[index].mutable {
                        immutable_stores[index] += 1;
                        if parameters[index] || immutable_stores[index] > 1 {
                            return Err(reject(
                                "E-VERIFY-MUTABILITY",
                                Some(instruction.span),
                                "Unpack destination has another immutable initialization site",
                            ));
                        }
                    }
                }
            }
        }
        let edges = match &block.terminator {
            Terminator::Jump(target) => vec![*target],
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                if value_type(function, *condition, Some(function.span))? != &Type::Bool {
                    return Err(fail(
                        "E-VERIFY-BRANCH",
                        "branch condition must have bool type",
                    ));
                }
                if then_block == else_block {
                    vec![*then_block]
                } else {
                    vec![*then_block, *else_block]
                }
            }
            Terminator::Return(value) => {
                match value {
                    None if function.return_type == Type::Unit => {}
                    Some(value)
                        if value_type(function, *value, Some(function.span))?
                            == &function.return_type => {}
                    _ => {
                        return Err(fail(
                            "E-VERIFY-RETURN",
                            "return value does not match the function result type",
                        ));
                    }
                }
                Vec::new()
            }
            Terminator::Unreachable | Terminator::Trap => Vec::new(),
        };
        for edge in edges {
            let target = edge.0 as usize;
            if target >= n {
                return Err(fail(
                    "E-VERIFY-CFG",
                    "control edge references an absent block",
                ));
            }
            successors[block_index].push(target);
            predecessors[target].push(block_index);
        }
    }
    if definitions.iter().any(Option::is_none) {
        return Err(fail("E-VERIFY-SSA", "declared SSA value has no definition"));
    }
    let reachable = reachability(function.entry.0 as usize, &successors, budget)?;
    let mut role_data_uses = Vec::new();
    for (b, block) in function.blocks.iter().enumerate() {
        if reachable[b] && matches!(block.terminator, Terminator::Unreachable) {
            return Err(fail(
                "E-VERIFY-UNREACHABLE",
                "reachable Unreachable is not a safe runtime exit",
            ));
        }
    }
    // Bounding matrix cells before allocation closes a memory-exhaustion path.
    let cells = n.checked_mul(n).and_then(|x| {
        n.checked_mul(m)
            .and_then(|y| x.checked_add(y.saturating_mul(2)))
    });
    if cells.is_none_or(|x| x > limits.max_analysis_steps) {
        return Err(limit(
            "verification data-flow matrices exceed the work/memory budget",
        ));
    }
    let dominators = dominators(function.entry.0 as usize, &predecessors, &reachable, budget)?;
    if addressed_sums {
        address_sums::check(function, &successors, &reachable, budget)?;
    }
    if mixed_address_sums {
        mixed_address_sums::check(function, &successors, &reachable, budget)?;
    }
    let (initialized_in, initialized_out) =
        initialization(function, &predecessors, &reachable, &parameters, budget)?;
    for (b, block) in function.blocks.iter().enumerate() {
        check_claimed_set(
            &plan.blocks[b].initialized_in,
            &initialized_in[b],
            function.span,
        )?;
        check_claimed_set(
            &plan.blocks[b].initialized_out,
            &initialized_out[b],
            function.span,
        )?;
        let mut initialized = initialized_in[b].clone();
        for (i, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            for operand in instruction_operands(&instruction.kind) {
                budget.spend(1)?;
                check_use(operand, b, i, &definitions, &dominators, instruction.span)?;
                // Reuse this already-paid operand visit. Only actual callable
                // data transfers allocate a new private use-site; ordinary
                // data and the independently checked Parameter slot stay on
                // their existing exact budget path.
                if helpers::role_data_operand(function, instruction, operand) {
                    budget.spend(1)?;
                    role_data_uses.push(operand);
                }
            }
            if matches!(instruction.kind, InstructionKind::TakeErrorUnion { .. }) {
                errors::check_take_tag(
                    function,
                    b,
                    instruction,
                    &predecessors,
                    &definitions,
                    budget,
                )?;
            }
            match instruction.kind {
                InstructionKind::Load(local)
                | InstructionKind::VariantTagLocal { local, .. }
                | InstructionKind::ReadVariantLocal { local, .. }
                | InstructionKind::BorrowVariantLocal { local, .. }
                | InstructionKind::BorrowFieldLocal { local, .. }
                | InstructionKind::BorrowPlaceLocal { local, .. }
                | InstructionKind::PlaceTagLocal { local, .. }
                | InstructionKind::TakeTupleField { local, .. }
                | InstructionKind::TakeVariantField { local, .. }
                | InstructionKind::RegisterDefer { local, .. }
                | InstructionKind::DescribeLocal { local }
                | InstructionKind::ReadPackageField { local, .. }
                | InstructionKind::DescribePackageField { local, .. }
                | InstructionKind::TakePackageField { local, .. }
                | InstructionKind::DropPackageField { local, .. }
                | InstructionKind::StorePackageField { local, .. }
                | InstructionKind::ProjectOwnedArrayField { local, .. }
                | InstructionKind::TakeOwnedArrayElement { local, .. }
                | InstructionKind::DropOwnedArrayElement { local, .. }
                | InstructionKind::StoreOwnedArrayElement { local, .. }
                | InstructionKind::BorrowOwnedArrayElement { local, .. }
                | InstructionKind::BorrowPackageField { local, .. }
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::TakeLocal { local }
                | InstructionKind::CallClosureMut { callee: local, .. }
                | InstructionKind::CallClosureShared { callee: local, .. }
                    if !initialized[local.0 as usize] =>
                {
                    return Err(reject(
                        "E-VERIFY-INITIALIZATION",
                        Some(instruction.span),
                        "load may read a local before initialization",
                    ));
                }
                InstructionKind::Store { local, .. }
                | InstructionKind::BindPackageCapture { local, .. } => {
                    initialized[local.0 as usize] = true
                }
                InstructionKind::UnpackVariant {
                    local, ref fields, ..
                }
                | InstructionKind::UnpackTuple { local, ref fields } => {
                    if !initialized[local.0 as usize] {
                        return Err(reject(
                            "E-VERIFY-INITIALIZATION",
                            Some(instruction.span),
                            "Unpack source is not initialized",
                        ));
                    }
                    initialized[local.0 as usize] = false;
                    budget.spend(fields.len())?;
                    for target in fields {
                        initialized[target.0 as usize] = true;
                    }
                }
                InstructionKind::TakeLocal { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropEnum { local }
                | InstructionKind::DropTuple { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::EndPackageCapture { local, .. }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local) => initialized[local.0 as usize] = false,
                _ => {}
            }
        }
        let term_use = match block.terminator {
            Terminator::Return(value) => value,
            Terminator::Branch { condition, .. } => Some(condition),
            _ => None,
        };
        if let Some(value) = term_use {
            check_use(
                value,
                b,
                block.instructions.len(),
                &definitions,
                &dominators,
                function.span,
            )?;
        }
    }
    let package_views = packages::capture_views(module, function, &predecessors, budget)?;
    let graph = FunctionGraph {
        successors,
        predecessors,
        reachable,
        package_views,
        role_data_uses,
    };
    foreign::check_regions(function, &graph, budget)?;
    cleanup::check_function(module, function, &graph, budget)?;
    Ok(graph)
}

fn value_type(function: &TypedFunction, value: ValueId, span: Option<Span>) -> Check<&Type> {
    function.values.get(value.0 as usize).ok_or_else(|| {
        reject(
            "E-VERIFY-ID",
            span,
            "operand references an absent SSA value",
        )
    })
}
fn local_type(function: &TypedFunction, local: LocalId, span: Option<Span>) -> Check<&Type> {
    function
        .locals
        .get(local.0 as usize)
        .map(|l| &l.ty)
        .ok_or_else(|| {
            reject(
                "E-VERIFY-ID",
                span,
                "instruction references an absent local",
            )
        })
}
fn check_instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<()> {
    let span = Some(instruction.span);
    if budget.pod_stores {
        dynamic_store::instruction_type_work(module, function, instruction, budget)?;
    }
    if budget.stored_borrows {
        stored_flow::instruction_work(function, instruction, budget)?;
    }
    let expected = match &instruction.kind {
        InstructionKind::AsyncRegistrationStart { .. }
        | InstructionKind::AsyncRegistrationValid { .. }
        | InstructionKind::AsyncRegistrationRequestClose { .. }
        | InstructionKind::AsyncRegistrationPollClose { .. }
        | InstructionKind::AsyncRegistrationFinishDestroy { .. }
        | InstructionKind::AsyncRegistrationFinishDestroyChecked { .. } => {
            registration::instruction(module, function, instruction, budget)?
        }
        InstructionKind::MakeDynBorrow { .. } | InstructionKind::CallDynSlot { .. } => {
            dynamic_dispatch::instruction(module, function, instruction, budget)?
        }
        InstructionKind::LoadStoredBorrow { .. } | InstructionKind::StoreStoredBorrow { .. } => {
            stored_borrows::instruction(function, instruction, budget)?
        }
        // Publication precedes the independent pin/ownership consumers. Keep
        // raw reservations closed until those consumers are complete.
        InstructionKind::ReservePlaceLocal { .. }
        | InstructionKind::ReservePlaceAddress { .. }
        | InstructionKind::CommitPlaceReplace { .. }
        | InstructionKind::AbortPlaceReservation { .. } => {
            budget.replacements = true;
            None
        }
        InstructionKind::RegistrationStart { .. }
        | InstructionKind::RegistrationValid { .. }
        | InstructionKind::RegistrationClose { .. } => {
            registration::instruction(module, function, instruction, budget)?
        }
        InstructionKind::TaskStart { .. }
        | InstructionKind::TaskCreate { .. }
        | InstructionKind::TaskYield
        | InstructionKind::SuspendCall { .. }
        | InstructionKind::TaskValid { .. }
        | InstructionKind::TaskPoll { .. }
        | InstructionKind::TaskRequestCancel { .. }
        | InstructionKind::TaskPollClose { .. }
        | InstructionKind::TaskFinishDestroy { .. } => {
            tasks::instruction(module, function, instruction, budget)?
        }
        InstructionKind::OwnedStoreNew { .. }
        | InstructionKind::OwnedStoreReserve { .. }
        | InstructionKind::OwnedStorePush { .. }
        | InstructionKind::OwnedStorePop { .. }
        | InstructionKind::OwnedStoreLen { .. }
        | InstructionKind::OwnedStoreCapacity { .. }
        | InstructionKind::OwnedStoreBorrow { .. }
        | InstructionKind::OwnedStoreBorrowMut { .. } => {
            dynamic_store::check_instruction(module, function, instruction, budget)?
        }
        InstructionKind::ByteBufferNew
        | InstructionKind::ByteBufferReserve { .. }
        | InstructionKind::ByteBufferTruncate { .. }
        | InstructionKind::ByteBufferPush { .. }
        | InstructionKind::ByteBufferLen { .. }
        | InstructionKind::ByteBufferCapacity { .. }
        | InstructionKind::ByteBufferBorrow { .. } => {
            byte_buffers::check_instruction(module, function, instruction, budget)?
        }
        InstructionKind::BindPackageCapture { .. }
        | InstructionKind::EndPackageCapture { .. }
        | InstructionKind::MakePackage { .. }
        | InstructionKind::ReadPackageField { .. }
        | InstructionKind::DescribePackageField { .. }
        | InstructionKind::TakePackageField { .. }
        | InstructionKind::DropPackageField { .. }
        | InstructionKind::StorePackageField { .. }
        | InstructionKind::DropPackage { .. }
        | InstructionKind::BorrowPackageField { .. } => {
            packages::check_instruction(module, function, instruction, budget)?
        }
        InstructionKind::BorrowPlaceLocal { .. } | InstructionKind::PlaceTagLocal { .. } => {
            sums::place_instruction(module, function, instruction, budget)?
        }
        InstructionKind::BorrowPlaceAddress { .. } | InstructionKind::PlaceTagAddress { .. } => {
            mixed_address_sums::instruction(module, function, instruction, budget)?
        }
        InstructionKind::BorrowFieldLocal { .. }
        | InstructionKind::VariantTagAddress { .. }
        | InstructionKind::BorrowVariantAddress { .. } => {
            address_sums::instruction(module, function, instruction, budget)?
        }
        InstructionKind::MakeOwnedArray { .. }
        | InstructionKind::ProjectOwnedArrayField { .. }
        | InstructionKind::TakeOwnedArrayElement { .. }
        | InstructionKind::DropOwnedArrayElement { .. }
        | InstructionKind::StoreOwnedArrayElement { .. }
        | InstructionKind::DropOwnedArray { .. }
        | InstructionKind::BorrowOwnedArrayElement { .. } => {
            resources::arrays::check_instruction(module, function, instruction, budget)?
        }
        InstructionKind::BorrowCapture { .. }
        | InstructionKind::DescribeCapture { .. }
        | InstructionKind::TakeCapture { .. }
        | InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureOnce { .. }
        | InstructionKind::CallClosureBorrowed { .. }
        | InstructionKind::DropClosure { .. } => {
            closures::check_resource_instruction(module, function, instruction, budget)?
        }
        InstructionKind::RegisterDefer { local, .. } => {
            local_type(function, *local, span)?;
            None
        }
        InstructionKind::TakeTupleField { .. }
        | InstructionKind::TakeVariantField { .. }
        | InstructionKind::MakeOwnedTuple { .. }
        | InstructionKind::UnpackTuple { .. }
        | InstructionKind::DropTuple { .. }
        | InstructionKind::VariantTagLocal { .. }
        | InstructionKind::ReadVariantLocal { .. }
        | InstructionKind::BorrowVariantLocal { .. }
        | InstructionKind::UnpackVariant { .. }
        | InstructionKind::DropEnum { .. } => {
            let limits = budget.limits.clone();
            sums::check_instruction(function, instruction, &limits, budget)?
        }
        InstructionKind::UnsafeEnter { .. }
        | InstructionKind::UnsafeExit { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. } => None,
        InstructionKind::TakeErrorUnion { .. } | InstructionKind::DropErrorUnion { .. } => {
            errors::check_instruction(function, instruction, budget)?
        }
        InstructionKind::MakeErrorUnion { is_error, value } => {
            let result = instruction.result.ok_or_else(|| {
                reject(
                    "E-VERIFY-ERROR",
                    span,
                    "error construction needs a typed result",
                )
            })?;
            let ty = value_type(function, result, span)?;
            let Type::ErrorUnion { error, result } = ty else {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "MakeErrorUnion must produce an error-union value",
                ));
            };
            let payload = if *is_error { error } else { result };
            if value_type(function, *value, span)? != payload.as_ref() {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "error-union payload does not match its selected arm",
                ));
            }
            Some(ty.clone())
        }
        InstructionKind::ErrorTag { value } => {
            if !matches!(value_type(function, *value, span)?, Type::ErrorUnion { .. }) {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "ErrorTag requires an error-union value",
                ));
            }
            Some(Type::Bool)
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            let source = value_type(function, *value, span)?;
            let Type::ErrorUnion { error, result } = source else {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "ReadErrorUnion requires an error-union value",
                ));
            };
            if !source.is_copy_value() {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "an owned error union requires consuming TakeErrorUnion, never ReadErrorUnion",
                ));
            }
            // The ordinary backend checks the active tag before projection.
            // A failed projection is a defined trap, never error propagation.
            Some(if *is_error {
                (**error).clone()
            } else {
                (**result).clone()
            })
        }
        InstructionKind::WithScopedThread { .. } => {
            scoped_thread::instruction(module, function, instruction, budget)?
        }
        InstructionKind::WithSyncCell { .. }
        | InstructionKind::SyncI32 { .. }
        | InstructionKind::SyncI32CompareExchange { .. } => {
            scoped_thread::sync_instruction(module, function, instruction, budget)?
        }
        InstructionKind::MutexCreate { .. }
        | InstructionKind::MutexLock { .. }
        | InstructionKind::MutexValue { .. }
        | InstructionKind::MutexUnlock { .. }
        | InstructionKind::DropMutexGuard { .. } => {
            return Err(unsupported(
                "generic mutex verification is not implemented",
                span,
            ));
        }
        InstructionKind::WithForeignCallback { .. } => {
            scoped_callback::instruction(module, function, instruction, budget)?
        }
        InstructionKind::ForeignCall {
            function: callee,
            arguments,
            ..
        } => foreign::check_call(module, function, *callee, arguments, span, budget)?,
        InstructionKind::StaticUtf8 { .. } => Some(Type::Address {
            mutable: false,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        }),
        InstructionKind::Constant(constant) => Some(constant.ty()),
        InstructionKind::ConvertNumber { value, target } => Some(number_conversion(
            value_type(function, *value, span)?,
            target,
            span,
        )?),
        InstructionKind::ConvertInteger { value, target } => Some(integer_conversion(
            value_type(function, *value, span)?,
            target,
            span,
        )?),
        InstructionKind::Load(local) => {
            let ty = local_type(function, *local, span)?;
            if errors::owned(ty, budget.sums) || (budget.sums && sums::owned(ty)) {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "an owned error union requires TakeLocal, never copying Load",
                ));
            }
            if matches!(ty, Type::Package { .. })
                || matches!(ty, Type::BorrowedStorage { storage, .. }
                    if matches!(storage.as_ref(), Type::Package { .. }))
                || resources::arrays::is_owned_type(ty)
            {
                return Err(reject(
                    "E-VERIFY-PACKAGE",
                    span,
                    "a non-Copy package requires field projection or whole TakeLocal, never Load",
                ));
            }
            if matches!(ty, Type::Managed(_)) || closures::owned_type(module, ty, budget)? {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "Managed positions require DescribeLocal or TakeLocal, never copying Load",
                ));
            }
            Some(ty.clone())
        }
        InstructionKind::TakeLocal { local } => {
            let ty = local_type(function, *local, span)?;
            if !affine_type(module, ty)
                && !matches!(ty, Type::Closure(_))
                && !matches!(ty, Type::Managed(_))
                && !matches!(ty, Type::Package { .. })
                && !matches!(ty, Type::BorrowedStorage { storage, .. }
                    if matches!(storage.as_ref(), Type::Package { .. }))
                && !errors::owned(ty, budget.sums)
                && !(budget.sums && sums::owned(ty))
                && !resources::arrays::is_owned_type(ty)
                && !closures::owned_type(module, ty, budget)?
            {
                return Err(reject(
                    "E-VERIFY-MOVE",
                    span,
                    "TakeLocal requires a non-Copy address carrier",
                ));
            }
            Some(ty.clone())
        }
        InstructionKind::BorrowLocal { local } => {
            let inner = local_type(function, *local, span)?;
            if let Type::Closure(id) = inner {
                closures::definition(module, *id, span)?;
            } else if budget.inline_store_root(inner)?
                && matches!(inner, Type::Package { .. })
                && dynamic_store::package_inline_store(inner, budget)?
            {
                packages::check_ordinary_type(module, inner, span, budget)?;
            } else if matches!(
                inner,
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) {
                sums::check_type_for(module, inner, span, budget)?;
            } else if matches!(inner, Type::Package { .. }) {
                packages::check_shared_product_type(module, inner, span, budget)?;
            } else if matches!(inner, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { .. }))
            {
                resources::check_type_for(module, inner, span, budget)?;
            } else {
                check_type(inner, span)?;
            }
            Some(Type::Address {
                mutable: false,
                inner: Box::new(inner.clone()),
            })
        }
        InstructionKind::BorrowLocalMut { local } => {
            let inner = local_type(function, *local, span)?;
            if let Type::Closure(id) = inner {
                closures::definition(module, *id, span)?;
            } else if budget.inline_store_root(inner)?
                && matches!(inner, Type::Package { .. })
                && dynamic_store::package_inline_store(inner, budget)?
            {
                packages::check_ordinary_type(module, inner, span, budget)?;
            } else if matches!(
                inner,
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) {
                sums::check_type_for(module, inner, span, budget)?;
            } else if matches!(inner, Type::Package { .. }) {
                packages::check_shared_product_type(module, inner, span, budget)?;
            } else if matches!(inner,Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
            {
                registration::carrier(module, inner, span, budget)?;
            } else if matches!(inner, Type::Managed(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..}))
            {
                tasks::carrier(module, inner, span, budget)?;
            } else {
                check_type(inner, span)?;
            }
            if !function.locals[local.0 as usize].mutable {
                return Err(reject(
                    "E-VERIFY-MUTABILITY",
                    span,
                    "exclusive borrow requires mutable storage",
                ));
            }
            Some(Type::Address {
                mutable: true,
                inner: Box::new(inner.clone()),
            })
        }
        InstructionKind::ReborrowAddress { address, mutable } => {
            let Type::Address {
                mutable: available,
                inner,
            } = value_type(function, *address, span)?
            else {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "reborrow requires an address",
                ));
            };
            if *mutable && !available {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "shared input cannot grant exclusive permission",
                ));
            }
            Some(Type::Address {
                mutable: *mutable,
                inner: inner.clone(),
            })
        }
        InstructionKind::WriteAddress { address, value } => {
            let Type::Address {
                mutable: true,
                inner,
            } = value_type(function, *address, span)?
            else {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "indirect write requires an exclusive address",
                ));
            };
            check_type(inner, span)?;
            if inner.as_ref() != value_type(function, *value, span)? {
                return Err(reject(
                    "E-VERIFY-TYPE",
                    span,
                    "indirect write value differs from pointee type",
                ));
            }
            None
        }
        InstructionKind::AddressField { address, index } => {
            let Type::Address { mutable, inner } = value_type(function, *address, span)? else {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "field projection requires a shared address",
                ));
            };
            let projected = match inner.as_ref() {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                Type::Managed(payload)
                    if resources::head_address(value_type(function, *address, span)?) =>
                {
                    resources::check_type_for(module, inner, span, budget)?;
                    payload.as_ref()
                }
                other => other,
            };
            let field = match projected {
                Type::Tuple(fields) => fields.get(*index as usize),
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, ty)| ty),
                Type::Package { fields, .. } => {
                    if matches!(inner.as_ref(), Type::BorrowedStorage { .. }) {
                        packages::check_ordinary_type(module, inner, span, budget)?;
                    } else if budget.inline_store_root(value_type(function, *address, span)?)?
                        && dynamic_store::package_inline_store(projected, budget)?
                    {
                        packages::check_ordinary_type(module, projected, span, budget)?;
                    } else if packages::shared_product_head(projected) {
                        packages::check_shared_product_type(module, projected, span, budget)?;
                    } else {
                        dynamic_store::check_element(module, projected, span, budget)?;
                    }
                    fields.get(*index as usize).map(|(_, ty)| ty)
                }
                _ => None,
            }
            .ok_or_else(|| {
                reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "field projection index outside its record or tuple",
                )
            })?;
            if matches!(projected, Type::Package { .. })
                && let Type::Closure(id) = field
            {
                // The real Package element was independently admitted above.
                // Projection borrows its actual field; it grants neither a
                // Copy read nor a consuming/replacing indirect operation.
                closures::definition(module, *id, span)?;
            } else if budget.inline_store_root(value_type(function, *address, span)?)?
                && matches!(field, Type::Package { .. })
                && dynamic_store::package_inline_store(field, budget)?
            {
                packages::check_ordinary_type(module, field, span, budget)?;
            } else if budget.sums
                && matches!(
                    field,
                    Type::Tuple(_)
                        | Type::Package { .. }
                        | Type::Enum { .. }
                        | Type::ErrorUnion { .. }
                )
            {
                sums::check_type_for(module, field, span, budget)?;
            } else if matches!(field, Type::Package { .. }) {
                packages::check_shared_product_type(module, field, span, budget)?;
            } else if matches!(field, Type::Managed(_))
                && resources::head_address(&Type::Address {
                    mutable: *mutable,
                    inner: Box::new(field.clone()),
                })
            {
                resources::check_type_for(module, field, span, budget)?;
            } else {
                check_type(field, span)?;
            }
            Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(field.clone()),
            })
        }
        InstructionKind::AddressIndex { address, index } => {
            let Type::Address { mutable, inner } = value_type(function, *address, span)? else {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "index projection requires a shared address",
                ));
            };
            let (Type::Array { element, .. } | Type::Slice { element }) = inner.as_ref() else {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "index projection requires an array or borrowed slice",
                ));
            };
            if !value_type(function, *index, span)?.is_integer() {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "index projection requires an integer",
                ));
            }
            check_type(element, span)?;
            Some(Type::Address {
                mutable: *mutable,
                inner: element.clone(),
            })
        }
        InstructionKind::AddressSlice {
            address,
            start,
            end,
        } => {
            let Type::Address { mutable, inner } = value_type(function, *address, span)? else {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "slice projection requires an address",
                ));
            };
            let (Type::Array { element, .. } | Type::Slice { element }) = inner.as_ref() else {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "slice projection requires an array or borrowed slice",
                ));
            };
            check_type(element, span)?;
            for endpoint in start.iter().chain(end.iter()) {
                if !value_type(function, *endpoint, span)?.is_integer() {
                    return Err(reject(
                        "E-VERIFY-ADDRESS",
                        span,
                        "slice endpoints require integers",
                    ));
                }
            }
            Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(Type::Slice {
                    element: element.clone(),
                }),
            })
        }
        InstructionKind::SliceLength { address } => {
            if !matches!(value_type(function, *address, span)?,
                Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Slice { .. }))
            {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "slice length requires a borrowed slice",
                ));
            }
            Some(Type::I64)
        }
        InstructionKind::ReadAddress { address } => match value_type(function, *address, span)? {
            Type::Address { inner, .. }
                if inner.is_copy_value()
                    || stored_flow::pure_read(module, inner, budget)?
                    || stored_flow::pure_inline(module, inner, budget)? =>
            {
                Some(inner.as_ref().clone())
            }
            _ => {
                return Err(reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "ReadAddress requires a shared address to a finite Copy value",
                ));
            }
        },
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            local_type(function, *local, span)?;
            None
        }
        InstructionKind::Store { local, value } => {
            if local_type(function, *local, span)? != value_type(function, *value, span)? {
                return Err(reject(
                    "E-VERIFY-TYPE",
                    span,
                    "stored value does not match local type",
                ));
            }
            None
        }
        InstructionKind::Unary { op, operand } => Some(unary_result(
            *op,
            value_type(function, *operand, span)?,
            span,
        )?),
        InstructionKind::Binary { op, left, right } => Some(binary_result(
            *op,
            value_type(function, *left, span)?,
            value_type(function, *right, span)?,
            span,
        )?),
        InstructionKind::Call {
            function: callee,
            arguments,
        } => {
            let callee = module.functions.get(callee.0 as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-CALL",
                    span,
                    "call target is absent from the checked module",
                )
            })?;
            if resources::is_teardown(module, callee.id, budget)? {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "teardown is callable only by a checked cleanup activation",
                ));
            }
            if callee.foreign.is_some() {
                return Err(reject(
                    "E-VERIFY-FOREIGN",
                    span,
                    "ordinary Call cannot enter a foreign declaration",
                ));
            }
            if callee.environment.is_some() {
                return Err(reject(
                    "E-VERIFY-CLOSURE-ENTRY",
                    span,
                    "ordinary Call cannot enter a function that requires a closure environment",
                ));
            }
            if arguments.len() != callee.parameters.len() {
                return Err(reject(
                    "E-VERIFY-CALL",
                    span,
                    "call arity does not match the target signature",
                ));
            }
            for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
                if value_type(function, *argument, span)? != local_type(callee, *parameter, span)? {
                    return Err(reject(
                        "E-VERIFY-CALL",
                        span,
                        "call argument does not match the target parameter type",
                    ));
                }
            }
            if callee.return_type == Type::Unit {
                None
            } else {
                Some(callee.return_type.clone())
            }
        }
        InstructionKind::ForeignFunctionRef { function: target } => {
            Some(native_values::function_ref(module, *target, span, budget)?)
        }
        InstructionKind::FunctionRef { function: target } => {
            if resources::is_teardown(module, *target, budget)? {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "teardown cannot become an ordinary function pointer",
                ));
            }
            let target = module.functions.get(target.0 as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-FUNCTION-REF",
                    span,
                    "function pointer target is absent",
                )
            })?;
            if target.foreign.is_some() {
                return Err(reject(
                    "E-VERIFY-FOREIGN",
                    span,
                    "ordinary function pointers cannot erase foreign ABI and unsafe requirements",
                ));
            }
            if target.environment.is_some() {
                return Err(reject(
                    "E-VERIFY-FUNCTION-REF",
                    span,
                    "function pointer target requires a closure environment",
                ));
            }
            if let Some(
                pointer @ Type::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                },
            ) = instruction
                .result
                .and_then(|id| function.values.get(id.0 as usize))
            {
                borrowed_fp::relation(parameters, result, life, span)?;
                if target.parameters.len() != parameters.len() || target.return_type != **result {
                    return Err(reject(
                        "E-VERIFY-FUNCTION-LIFE",
                        span,
                        "borrowed pointer target has a different complete signature",
                    ));
                }
                for (parameter, expected) in target.parameters.iter().zip(parameters) {
                    if local_type(target, *parameter, span)? != expected {
                        return Err(reject(
                            "E-VERIFY-FUNCTION-LIFE",
                            span,
                            "borrowed pointer target parameter mode differs",
                        ));
                    }
                }
                Some(pointer.clone())
            } else {
                let mut parameters = Vec::with_capacity(target.parameters.len());
                for parameter in &target.parameters {
                    let ty = local_type(target, *parameter, span)?;
                    check_type(ty, span)?;
                    parameters.push(ty.clone());
                }
                check_type(&target.return_type, span)?;
                Some(Type::FunctionPointer {
                    parameters,
                    result: Box::new(target.return_type.clone()),
                })
            }
        }
        InstructionKind::CallIndirect { callee, arguments } => {
            let (parameters, result) = match value_type(function, *callee, span)? {
                Type::FunctionPointer { parameters, result }
                | Type::BorrowedFunctionPointer {
                    parameters, result, ..
                } => (parameters, result),
                _ => {
                    return Err(reject(
                        "E-VERIFY-INDIRECT-CALL",
                        span,
                        "callee must be a function pointer",
                    ));
                }
            };
            if arguments.len() != parameters.len() {
                return Err(reject(
                    "E-VERIFY-INDIRECT-CALL",
                    span,
                    "indirect call arity differs from the signature",
                ));
            }
            for (argument, parameter) in arguments.iter().zip(parameters) {
                if value_type(function, *argument, span)? != parameter {
                    return Err(reject(
                        "E-VERIFY-INDIRECT-CALL",
                        span,
                        "indirect call argument differs from the signature",
                    ));
                }
            }
            if **result == Type::Unit {
                None
            } else {
                Some((**result).clone())
            }
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            if resources::arrays::is_owned_type(value_type(function, *aggregate, span)?) {
                return Err(reject(
                    "E-VERIFY-OWNED-ARRAY",
                    span,
                    "Copy index extraction cannot transfer an owning array element",
                ));
            }
            if !value_type(function, *index, span)?.is_integer() {
                return Err(reject(
                    "E-VERIFY-INDEX",
                    span,
                    "array index must be integer",
                ));
            }
            let Type::Array { element, .. } = value_type(function, *aggregate, span)? else {
                return Err(reject("E-VERIFY-INDEX", span, "index needs a fixed array"));
            };
            Some((**element).clone())
        }
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            if !value_type(function, *index, span)?.is_integer() {
                return Err(reject(
                    "E-VERIFY-INDEX",
                    span,
                    "array index must be integer",
                ));
            }
            let ty = value_type(function, *aggregate, span)?;
            if resources::arrays::is_owned_type(ty) {
                return Err(reject(
                    "E-VERIFY-OWNED-ARRAY",
                    span,
                    "Copy array update cannot replace an owning element",
                ));
            }
            let Type::Array { element, .. } = ty else {
                return Err(reject(
                    "E-VERIFY-INDEX",
                    span,
                    "update index needs a fixed array",
                ));
            };
            if element.as_ref() != value_type(function, *value, span)? {
                return Err(reject(
                    "E-VERIFY-INDEX",
                    span,
                    "array update element type mismatch",
                ));
            }
            Some(ty.clone())
        }
        InstructionKind::MakeAggregate { fields } => {
            let result = instruction.result.ok_or_else(|| {
                reject(
                    "E-VERIFY-AGGREGATE",
                    span,
                    "aggregate construction requires a result",
                )
            })?;
            let ty = value_type(function, result, span)?;
            if resources::arrays::is_owned_type(ty) {
                return Err(reject(
                    "E-VERIFY-OWNED-ARRAY",
                    span,
                    "Copy aggregate construction cannot create an owning array",
                ));
            }
            check_type(ty, span)?;
            let physical = match ty {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                other => other,
            };
            let expected = physical.aggregate_fields().ok_or_else(|| {
                reject(
                    "E-VERIFY-AGGREGATE",
                    span,
                    "aggregate construction result is not a record or tuple",
                )
            })?;
            if expected.len() != fields.len() {
                return Err(reject(
                    "E-VERIFY-AGGREGATE",
                    span,
                    "aggregate field count mismatch",
                ));
            }
            for (field, expected) in fields.iter().zip(expected) {
                if value_type(function, *field, span)? != expected {
                    return Err(reject(
                        "E-VERIFY-AGGREGATE",
                        span,
                        "aggregate field type mismatch",
                    ));
                }
            }
            Some(ty.clone())
        }
        InstructionKind::MakeVariant { variant, fields } => {
            let result = instruction.result.ok_or_else(|| {
                reject("E-VERIFY-ENUM", span, "enum construction requires a result")
            })?;
            let ty = value_type(function, result, span)?;
            let physical = match ty {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                other => other,
            };
            let Type::Enum { variants, .. } = physical else {
                return Err(reject(
                    "E-VERIFY-ENUM",
                    span,
                    "variant result must be an enum",
                ));
            };
            let (_, expected) = variants
                .get(*variant as usize)
                .ok_or_else(|| reject("E-VERIFY-ENUM", span, "variant index out of bounds"))?;
            if expected.len() != fields.len() {
                return Err(reject(
                    "E-VERIFY-ENUM",
                    span,
                    "variant payload arity mismatch",
                ));
            }
            for (value, expected) in fields.iter().zip(expected) {
                if value_type(function, *value, span)? != expected {
                    return Err(reject(
                        "E-VERIFY-ENUM",
                        span,
                        "variant payload type mismatch",
                    ));
                }
            }
            Some(ty.clone())
        }
        InstructionKind::VariantTag { value } => {
            let ty = value_type(function, *value, span)?;
            let physical = match ty {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                other => other,
            };
            if !matches!(physical, Type::Enum { .. }) || (budget.sums && sums::owned(ty)) {
                return Err(reject("E-VERIFY-ENUM", span, "tag requires an enum value"));
            }
            Some(Type::I32)
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let source = value_type(function, *value, span)?;
            let physical = match source {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                other => other,
            };
            let Type::Enum { variants, .. } = physical else {
                return Err(reject(
                    "E-VERIFY-ENUM",
                    span,
                    "payload projection requires an enum value",
                ));
            };
            if budget.sums && sums::owned_enum(source) {
                return Err(reject(
                    "E-VERIFY-SUM",
                    span,
                    "owned Enum observation requires explicit local operations",
                ));
            }
            let ty = variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| {
                    reject(
                        "E-VERIFY-ENUM",
                        span,
                        "payload variant or field index out of bounds",
                    )
                })?;
            Some(ty.clone())
        }
        InstructionKind::ExtractField { aggregate, index } => {
            let ty = value_type(function, *aggregate, span)?;
            if resources::arrays::is_owned_type(ty) {
                return Err(reject(
                    "E-VERIFY-OWNED-ARRAY",
                    span,
                    "Copy field operations cannot access owning array elements",
                ));
            }
            let physical = match ty {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                other => other,
            };
            let fields = physical.aggregate_fields().ok_or_else(|| {
                reject(
                    "E-VERIFY-AGGREGATE",
                    span,
                    "field projection needs a record or tuple",
                )
            })?;
            Some(
                (*fields.get(*index as usize).ok_or_else(|| {
                    reject(
                        "E-VERIFY-AGGREGATE",
                        span,
                        "field projection index out of bounds",
                    )
                })?)
                .clone(),
            )
        }
        InstructionKind::UpdateField {
            aggregate,
            index,
            value,
        } => {
            let ty = value_type(function, *aggregate, span)?;
            if resources::arrays::is_owned_type(ty) {
                return Err(reject(
                    "E-VERIFY-OWNED-ARRAY",
                    span,
                    "Copy field operations cannot access owning array elements",
                ));
            }
            let physical = match ty {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                other => other,
            };
            let fields = physical.aggregate_fields().ok_or_else(|| {
                reject(
                    "E-VERIFY-AGGREGATE",
                    span,
                    "field update needs a record or tuple",
                )
            })?;
            if fields.get(*index as usize).copied() != Some(value_type(function, *value, span)?) {
                return Err(reject(
                    "E-VERIFY-AGGREGATE",
                    span,
                    "field update index or value type mismatch",
                ));
            }
            Some(ty.clone())
        }
        InstructionKind::MakeClosure { closure, captures } => {
            let definition = closures::definition(module, *closure, span)?;
            if captures.len() != definition.captures.len() {
                return Err(reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "capture count differs from the concrete environment",
                ));
            }
            for (value, ty) in captures.iter().zip(&definition.captures) {
                if value_type(function, *value, span)? != ty {
                    return Err(reject(
                        "E-VERIFY-CAPTURE",
                        span,
                        "capture operand type differs from the environment field",
                    ));
                }
            }
            Some(Type::Closure(*closure))
        }
        InstructionKind::ReadCapture { index } => {
            let environment = function.environment.ok_or_else(|| {
                reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "ReadCapture requires a closure entry environment",
                )
            })?;
            let definition = closures::definition(module, environment, span)?;
            let ty = definition.captures.get(*index as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "capture index is outside the entry environment",
                )
            })?;
            if matches!(
                ty,
                Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
            ) || (budget.inline_store_root(ty)?
                && dynamic_store::inline_sum_capture(ty, budget)?)
            {
                return Err(reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "owned captures require their explicit resource or Package view operations, never copying ReadCapture",
                ));
            }
            Some(ty.clone())
        }
        InstructionKind::WriteCapture { index, value } => {
            let environment = function.environment.ok_or_else(|| {
                reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "capture write requires an environment",
                )
            })?;
            let definition = closures::definition(module, environment, span)?;
            let field = definition
                .captures
                .get(*index as usize)
                .ok_or_else(|| reject("E-VERIFY-CAPTURE", span, "capture field is absent"))?;
            let owned_sum = budget.inline_store_root(field)?
                && dynamic_store::inline_sum_capture(field, budget)?;
            if !matches!(definition.kind, CallableKind::FnMut | CallableKind::FnOnce)
                || !(field.is_copy_value() || (definition.kind == CallableKind::FnMut && owned_sum))
                || value_type(function, *value, span)? != field
            {
                return Err(reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "capture writes require FnMut and a matching pure Copy field",
                ));
            }
            None
        }
        InstructionKind::CallClosureMut { callee, arguments } => {
            let Type::Closure(id) = local_type(function, *callee, span)? else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "mutable closure call requires a concrete local environment",
                ));
            };
            if !function.locals[callee.0 as usize].mutable {
                return Err(reject(
                    "E-VERIFY-MUTABILITY",
                    span,
                    "mutable closure call requires a mutable local",
                ));
            }
            let definition = closures::definition(module, *id, span)?;
            if !matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut) {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "mutable closure call requires its FnMut entry",
                ));
            }
            if arguments.len() != definition.parameters.len() {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call arity differs from its concrete signature",
                ));
            }
            for (argument, parameter) in arguments.iter().zip(&definition.parameters) {
                if value_type(function, *argument, span)? != parameter {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "closure argument type differs from its concrete signature",
                    ));
                }
            }
            if definition.result == Type::Unit {
                None
            } else {
                Some(definition.result.clone())
            }
        }
        InstructionKind::CallClosure { callee, arguments } => {
            let Type::Closure(id) = value_type(function, *callee, span)? else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call requires a concrete environment value",
                ));
            };
            let definition = closures::definition(module, *id, span)?;
            if definition.kind != CallableKind::Fn
                || closures::owned_type(module, &Type::Closure(*id), budget)?
            {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "FnMut must call the actual writable environment, never a by-value snapshot",
                ));
            }
            if arguments.len() != definition.parameters.len() {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call arity differs from its concrete signature",
                ));
            }
            for (argument, parameter) in arguments.iter().zip(&definition.parameters) {
                if value_type(function, *argument, span)? != parameter {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "closure argument type differs from its concrete signature",
                    ));
                }
            }
            if definition.result == Type::Unit {
                None
            } else {
                Some(definition.result.clone())
            }
        }
        InstructionKind::MakeManaged { fields } => {
            let result = instruction.result.ok_or_else(|| {
                reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "resource construction needs a managed result",
                )
            })?;
            let ty = value_type(function, result, span)?;
            resources::check_type_for(module, ty, span, budget)?;
            let Type::Managed(inner) = ty else {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "resource construction cannot manufacture a borrowed description",
                ));
            };
            if matches!(inner.as_ref(), Type::OwnedStore { .. }) {
                return Err(reject(
                    "E-VERIFY-OWNED-STORE",
                    span,
                    "sealed Store cannot be assembled with MakeManaged",
                ));
            }
            let Type::Record {
                fields: expected, ..
            } = inner.as_ref()
            else {
                return Err(reject(
                    "E-VERIFY-BYTE-BUFFER",
                    span,
                    "sealed byte storage cannot be assembled with MakeManaged",
                ));
            };
            if fields.len() != expected.len() {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "resource construction requires every payload field exactly once",
                ));
            }
            for (value, (_, expected)) in fields.iter().zip(expected) {
                if value_type(function, *value, span)? != expected {
                    return Err(reject(
                        "E-VERIFY-RESOURCE",
                        span,
                        "resource payload field has the wrong type",
                    ));
                }
            }
            Some(ty.clone())
        }
        InstructionKind::DescribeAddress { address } => {
            budget.head_descriptions = true;
            let ty = value_type(function, *address, span)?;
            if !resources::head_address(ty) {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "DescribeAddress requires an actual finite resource head address",
                ));
            }
            let Type::Address { inner, .. } = ty else {
                unreachable!()
            };
            resources::check_type_for(module, inner, span, budget)?;
            let Type::Managed(payload) = inner.as_ref() else {
                unreachable!()
            };
            Some(Type::Desc(payload.clone()))
        }
        InstructionKind::DescribeLocal { local } => {
            let Type::Managed(inner) = local_type(function, *local, span)? else {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "DescribeLocal requires an actual managed position",
                ));
            };
            Some(Type::Desc(inner.clone()))
        }
        InstructionKind::ReadDesc { desc, index }
        | InstructionKind::WriteDesc { desc, index, .. } => {
            let Type::Desc(inner) = value_type(function, *desc, span)? else {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "resource field access requires a description",
                ));
            };
            if matches!(inner.as_ref(), Type::OwnedStore { .. }) {
                return Err(reject(
                    "E-VERIFY-OWNED-STORE",
                    span,
                    "sealed Store has no ordinary readable or writable head fields",
                ));
            }
            let Type::Record { fields, .. } = inner.as_ref() else {
                return Err(reject(
                    "E-VERIFY-BYTE-BUFFER",
                    span,
                    "sealed byte storage has no ordinary readable or writable fields",
                ));
            };
            let (_, field) = fields.get(*index as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "resource field index is outside the payload",
                )
            })?;
            if let InstructionKind::WriteDesc { value, .. } = instruction.kind {
                if value_type(function, value, span)? != field {
                    return Err(reject(
                        "E-VERIFY-RESOURCE",
                        span,
                        "resource field write has the wrong type",
                    ));
                }
                None
            } else {
                Some(field.clone())
            }
        }
        InstructionKind::DropManaged { local } => {
            if !matches!(local_type(function, *local, span)?, Type::Managed(_)) {
                return Err(reject(
                    "E-VERIFY-RESOURCE",
                    span,
                    "cleanup requires an actual managed position",
                ));
            }
            None
        }
        InstructionKind::Unsupported(feature) => {
            return Err(unsupported(
                format!("unknown or unimplemented instruction: {feature}"),
                span,
            ));
        }
    };
    match (instruction.result, expected) {
        (None, None) => Ok(()),
        (Some(value), Some(ty)) if value_type(function, value, span)? == &ty => Ok(()),
        _ => Err(reject(
            "E-VERIFY-TYPE",
            span,
            "instruction result presence or type is incorrect",
        )),
    }
}

fn integer_conversion(source: &Type, target: &Type, span: Option<Span>) -> Check<Type> {
    if !source.is_integer() || !target.is_integer() {
        return Err(reject(
            "E-VERIFY-TYPE",
            span,
            "integer conversion requires u8/u16/i32/i64 source and target types",
        ));
    }
    // Value-preserving conversion is checked by the runtime/CT execution path;
    // verification grants no premise that a narrowing conversion will succeed.
    Ok(target.clone())
}

fn number_conversion(source: &Type, target: &Type, span: Option<Span>) -> Check<Type> {
    if !source.is_numeric() || !target.is_numeric() || !(source.is_float() || target.is_float()) {
        return Err(reject(
            "E-VERIFY-TYPE",
            span,
            "ConvertNumber requires numeric types and a floating endpoint",
        ));
    }
    Ok(target.clone())
}
fn unary_result(op: UnaryOp, operand: &Type, span: Option<Span>) -> Check<Type> {
    match op {
        UnaryOp::Not if operand == &Type::Bool => Ok(Type::Bool),
        UnaryOp::Negate if operand.is_float() => Ok(operand.clone()),
        UnaryOp::Negate | UnaryOp::BitNot if operand.is_integer() => Ok(operand.clone()),
        UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Dereference | UnaryOp::Give => {
            Err(unsupported(
                "address and responsibility unary operations require their dedicated checker",
                span,
            ))
        }
        _ => Err(reject(
            "E-VERIFY-TYPE",
            span,
            "unary operation has an invalid operand type",
        )),
    }
}
fn binary_result(op: BinaryOp, left: &Type, right: &Type, span: Option<Span>) -> Check<Type> {
    if matches!(op, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
        if matches!(
            left,
            Type::U8 | Type::U16 | Type::U64 | Type::Usize | Type::I32 | Type::I64
        ) && left == right
        {
            return Ok(left.clone());
        }
        return Err(unsupported(
            "shift operations require matching u8, u16, i32 or i64 operands",
            span,
        ));
    }
    if left != right {
        return Err(reject(
            "E-VERIFY-TYPE",
            span,
            "binary operands have different types",
        ));
    }
    if left.is_float() {
        return match op {
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                Ok(left.clone())
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => Ok(Type::Bool),
            _ => Err(reject(
                "E-VERIFY-TYPE",
                span,
                "operator is not an IEEE floating operation",
            )),
        };
    }
    match op {
        BinaryOp::Equal | BinaryOp::NotEqual
            if matches!(
                left,
                Type::Bool
                    | Type::I32
                    | Type::I64
                    | Type::U64
                    | Type::Usize
                    | Type::U8
                    | Type::U16
                    | Type::Unit
                    | Type::ForeignHandle
            ) =>
        {
            Ok(Type::Bool)
        }
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual
            if left.is_integer() =>
        {
            Ok(Type::Bool)
        }
        BinaryOp::And | BinaryOp::Or if left == &Type::Bool => Ok(Type::Bool),
        BinaryOp::Add
        | BinaryOp::Subtract
        | BinaryOp::Multiply
        | BinaryOp::Divide
        | BinaryOp::Remainder
        | BinaryOp::AddWrapping
        | BinaryOp::SubtractWrapping
        | BinaryOp::MultiplyWrapping
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
            if left.is_integer() =>
        {
            Ok(left.clone())
        }
        _ => Err(reject(
            "E-VERIFY-TYPE",
            span,
            "binary operation has invalid operand types",
        )),
    }
}

fn instruction_operands(instruction: &InstructionKind) -> impl Iterator<Item = ValueId> + '_ {
    let (prefix, rest): ([Option<ValueId>; 2], &[ValueId]) = match instruction {
        InstructionKind::MutexCreate { value } => ([Some(*value), None], &[]),
        InstructionKind::MutexLock { mutex } => ([Some(*mutex), None], &[]),
        InstructionKind::MutexValue { guard } | InstructionKind::MutexUnlock { guard } => {
            ([Some(*guard), None], &[])
        }
        InstructionKind::DropMutexGuard { .. } => ([None, None], &[]),
        InstructionKind::AsyncRegistrationStart { callback, .. } => ([Some(*callback), None], &[]),
        InstructionKind::AsyncRegistrationValid { registration }
        | InstructionKind::AsyncRegistrationRequestClose { registration }
        | InstructionKind::AsyncRegistrationPollClose { registration }
        | InstructionKind::AsyncRegistrationFinishDestroy { registration }
        | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
            ([Some(*registration), None], &[])
        }
        InstructionKind::MakeDynBorrow { source, .. } => ([Some(*source), None], &[]),
        InstructionKind::CallDynSlot {
            callee, arguments, ..
        } => ([Some(*callee), None], arguments),
        InstructionKind::LoadStoredBorrow { storage, .. } => ([Some(*storage), None], &[]),
        InstructionKind::StoreStoredBorrow { storage, value, .. } => {
            ([Some(*storage), Some(*value)], &[])
        }
        InstructionKind::ReservePlaceLocal { .. }
        | InstructionKind::AbortPlaceReservation { .. } => ([None, None], &[]),
        InstructionKind::ReservePlaceAddress { address, .. } => ([Some(*address), None], &[]),
        InstructionKind::CommitPlaceReplace { value, .. } => ([Some(*value), None], &[]),
        InstructionKind::RegistrationStart { callback, .. } => ([Some(*callback), None], &[]),
        InstructionKind::RegistrationValid { registration }
        | InstructionKind::RegistrationClose { registration } => ([Some(*registration), None], &[]),
        InstructionKind::TaskStart { argument, .. } => ([Some(*argument), None], &[]),
        InstructionKind::TaskCreate { arguments, .. } => ([None, None], arguments),
        InstructionKind::TaskValid { task }
        | InstructionKind::TaskPoll { task }
        | InstructionKind::TaskRequestCancel { task }
        | InstructionKind::TaskPollClose { task }
        | InstructionKind::TaskFinishDestroy { task } => ([Some(*task), None], &[]),
        InstructionKind::TaskYield => ([None, None], &[]),
        InstructionKind::MakeOwnedTuple { fields } => ([None, None], fields),
        InstructionKind::BorrowPlaceLocal { .. }
        | InstructionKind::PlaceTagLocal { .. }
        | InstructionKind::UnpackTuple { .. }
        | InstructionKind::DropTuple { .. }
        | InstructionKind::TakeTupleField { .. }
        | InstructionKind::TakeVariantField { .. } => ([None, None], &[]),

        InstructionKind::PlaceTagAddress { address, .. }
        | InstructionKind::BorrowPlaceAddress { address, .. }
        | InstructionKind::VariantTagAddress { address, .. }
        | InstructionKind::BorrowVariantAddress { address, .. } => ([Some(*address), None], &[]),
        InstructionKind::StaticUtf8 { .. }
        | InstructionKind::Constant(_)
        | InstructionKind::VariantTagLocal { .. }
        | InstructionKind::ReadVariantLocal { .. }
        | InstructionKind::BorrowVariantLocal { .. }
        | InstructionKind::BorrowFieldLocal { .. }
        | InstructionKind::UnpackVariant { .. }
        | InstructionKind::DropEnum { .. }
        | InstructionKind::RegisterDefer { .. }
        | InstructionKind::UnsafeEnter { .. }
        | InstructionKind::UnsafeExit { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. }
        | InstructionKind::BindPackageCapture { .. }
        | InstructionKind::EndPackageCapture { .. }
        | InstructionKind::FunctionRef { .. }
        | InstructionKind::ForeignFunctionRef { .. }
        | InstructionKind::Load(_)
        | InstructionKind::DescribeLocal { .. }
        | InstructionKind::ReadPackageField { .. }
        | InstructionKind::DescribePackageField { .. }
        | InstructionKind::TakePackageField { .. }
        | InstructionKind::DropPackageField { .. }
        | InstructionKind::ProjectOwnedArrayField { .. }
        | InstructionKind::TakeOwnedArrayElement { .. }
        | InstructionKind::DropOwnedArrayElement { .. }
        | InstructionKind::DropOwnedArray { .. }
        | InstructionKind::BorrowOwnedArrayElement { .. }
        | InstructionKind::BorrowPackageField { .. }
        | InstructionKind::DropManaged { .. }
        | InstructionKind::DropPackage { .. }
        | InstructionKind::DropErrorUnion { .. }
        | InstructionKind::ReadCapture { .. }
        | InstructionKind::BorrowCapture { .. }
        | InstructionKind::DescribeCapture { .. }
        | InstructionKind::TakeCapture { .. }
        | InstructionKind::DropClosure { .. }
        | InstructionKind::BorrowLocal { .. }
        | InstructionKind::BorrowLocalMut { .. }
        | InstructionKind::TakeLocal { .. }
        | InstructionKind::StorageLive(_)
        | InstructionKind::StorageDead(_)
        | InstructionKind::ByteBufferNew
        | InstructionKind::OwnedStoreNew { .. }
        | InstructionKind::Unsupported(_) => ([None, None], &[]),
        InstructionKind::WithScopedThread { argument, body, .. } => {
            ([Some(*argument), Some(*body)], &[])
        }
        InstructionKind::WithSyncCell { initial, body, .. } => ([Some(*initial), Some(*body)], &[]),
        InstructionKind::SyncI32 { address, value, .. } => ([Some(*address), *value], &[]),
        InstructionKind::SyncI32CompareExchange {
            address,
            expected,
            desired,
            ..
        } => (
            [Some(*address), Some(*expected)],
            std::slice::from_ref(desired),
        ),
        InstructionKind::WithForeignCallback { callback, body, .. } => {
            ([Some(*callback), Some(*body)], &[])
        }
        InstructionKind::OwnedStoreReserve { store, capacity } => {
            ([Some(*store), Some(*capacity)], &[])
        }
        InstructionKind::OwnedStorePush { store, value } => ([Some(*store), Some(*value)], &[]),
        InstructionKind::OwnedStoreBorrow { store, index }
        | InstructionKind::OwnedStoreBorrowMut { store, index } => {
            ([Some(*store), Some(*index)], &[])
        }
        InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store } => ([Some(*store), None], &[]),
        InstructionKind::Store { value, .. }
        | InstructionKind::ConvertInteger { value, .. }
        | InstructionKind::ConvertNumber { value, .. }
        | InstructionKind::StorePackageField { value, .. }
        | InstructionKind::StoreOwnedArrayElement { value, .. }
        | InstructionKind::WriteCapture { value, .. } => ([Some(*value), None], &[]),
        InstructionKind::ByteBufferReserve { buffer, capacity } => {
            ([Some(*buffer), Some(*capacity)], &[])
        }
        InstructionKind::ByteBufferTruncate { buffer, length } => {
            ([Some(*buffer), Some(*length)], &[])
        }
        InstructionKind::ByteBufferPush { buffer, value } => ([Some(*buffer), Some(*value)], &[]),
        InstructionKind::ByteBufferLen { buffer }
        | InstructionKind::ByteBufferCapacity { buffer }
        | InstructionKind::ByteBufferBorrow { buffer, .. } => ([Some(*buffer), None], &[]),
        InstructionKind::Unary { operand, .. } => ([Some(*operand), None], &[]),
        InstructionKind::ReadDesc { desc: address, .. }
        | InstructionKind::DescribeAddress { address }
        | InstructionKind::ReadAddress { address }
        | InstructionKind::SliceLength { address }
        | InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressField { address, .. } => ([Some(*address), None], &[]),
        InstructionKind::AddressIndex { address, index } => ([Some(*address), Some(*index)], &[]),
        InstructionKind::AddressSlice {
            address,
            start,
            end,
        } => ([Some(*address), *start], end.as_slice()),
        InstructionKind::WriteAddress { address, value }
        | InstructionKind::WriteDesc {
            desc: address,
            value,
            ..
        } => ([Some(*address), Some(*value)], &[]),
        InstructionKind::Binary { left, right, .. } => ([Some(*left), Some(*right)], &[]),
        InstructionKind::Call { arguments, .. }
        | InstructionKind::SuspendCall { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. } => ([None, None], arguments),
        InstructionKind::MakeClosure { captures, .. } => ([None, None], captures),
        InstructionKind::MakeManaged { fields }
        | InstructionKind::MakePackage { fields }
        | InstructionKind::MakeOwnedArray { elements: fields }
        | InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakeVariant { fields, .. } => ([None, None], fields),
        InstructionKind::VariantTag { value }
        | InstructionKind::ReadVariant { value, .. }
        | InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::ErrorTag { value }
        | InstructionKind::ReadErrorUnion { value, .. }
        | InstructionKind::TakeErrorUnion { value, .. } => ([Some(*value), None], &[]),
        InstructionKind::ExtractIndex { aggregate, index } => {
            ([Some(*aggregate), Some(*index)], &[])
        }
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => (
            [Some(*aggregate), Some(*index)],
            std::slice::from_ref(value),
        ),
        InstructionKind::ExtractField { aggregate, .. } => ([Some(*aggregate), None], &[]),
        InstructionKind::UpdateField {
            aggregate, value, ..
        } => ([Some(*aggregate), Some(*value)], &[]),
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallClosureBorrowed {
            callee, arguments, ..
        }
        | InstructionKind::CallIndirect { callee, arguments } => ([Some(*callee), None], arguments),
    };
    prefix.into_iter().flatten().chain(rest.iter().copied())
}
fn reachability(entry: usize, successors: &[Vec<usize>], budget: &mut Budget) -> Check<Vec<bool>> {
    let mut reachable = vec![false; successors.len()];
    let mut queue = vec![entry];
    while let Some(b) = queue.pop() {
        budget.spend(1)?;
        if reachable[b] {
            continue;
        }
        reachable[b] = true;
        queue.extend_from_slice(&successors[b]);
    }
    Ok(reachable)
}
fn dominators(
    entry: usize,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    budget: &mut Budget,
) -> Check<Vec<Vec<bool>>> {
    let n = predecessors.len();
    let mut dom = vec![reachable.to_vec(); n];
    for b in 0..n {
        if b == entry || !reachable[b] {
            dom[b].fill(false);
            dom[b][b] = true;
        }
    }
    loop {
        let mut changed = false;
        for b in 0..n {
            if b == entry || !reachable[b] {
                continue;
            }
            let mut next = reachable.to_vec();
            for &p in &predecessors[b] {
                if !reachable[p] {
                    continue;
                }
                budget.spend(n)?;
                for (bit, &previous) in next.iter_mut().zip(&dom[p]) {
                    *bit &= previous;
                }
            }
            next[b] = true;
            if next != dom[b] {
                dom[b] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(dom)
}
type InitializationSets = (Vec<Vec<bool>>, Vec<Vec<bool>>);
fn initialization(
    function: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    parameters: &[bool],
    budget: &mut Budget,
) -> Check<InitializationSets> {
    let n = function.blocks.len();
    let m = function.locals.len();
    let entry = function.entry.0 as usize;
    let mut inputs = vec![vec![true; m]; n];
    let mut outputs = inputs.clone();
    for b in 0..n {
        if b == entry {
            inputs[b] = parameters.to_vec();
        } else if !reachable[b] {
            inputs[b].fill(false);
        }
        outputs[b] = inputs[b].clone();
        for instruction in &function.blocks[b].instructions {
            match instruction.kind {
                InstructionKind::Store { local, .. }
                | InstructionKind::BindPackageCapture { local, .. } => {
                    outputs[b][local.0 as usize] = true
                }
                InstructionKind::UnpackVariant {
                    local, ref fields, ..
                }
                | InstructionKind::UnpackTuple { local, ref fields } => {
                    outputs[b][local.0 as usize] = false;
                    for target in fields {
                        outputs[b][target.0 as usize] = true;
                    }
                }
                InstructionKind::TakeLocal { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropEnum { local }
                | InstructionKind::DropTuple { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::EndPackageCapture { local, .. }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local) => outputs[b][local.0 as usize] = false,
                _ => {}
            }
        }
    }
    loop {
        let mut changed = false;
        for b in 0..n {
            let mut next = if b == entry {
                parameters.to_vec()
            } else if !reachable[b] {
                vec![false; m]
            } else {
                vec![true; m]
            };
            if b != entry && reachable[b] {
                for &p in &predecessors[b] {
                    if !reachable[p] {
                        continue;
                    }
                    budget.spend(m.max(1))?;
                    for (bit, &previous) in next.iter_mut().zip(&outputs[p]) {
                        *bit &= previous;
                    }
                }
            }
            let mut out = next.clone();
            for instruction in &function.blocks[b].instructions {
                budget.spend(1)?;
                match instruction.kind {
                    InstructionKind::Store { local, .. }
                    | InstructionKind::BindPackageCapture { local, .. } => {
                        out[local.0 as usize] = true
                    }
                    InstructionKind::UnpackVariant {
                        local, ref fields, ..
                    }
                    | InstructionKind::UnpackTuple { local, ref fields } => {
                        out[local.0 as usize] = false;
                        budget.spend(fields.len())?;
                        for target in fields {
                            out[target.0 as usize] = true;
                        }
                    }
                    InstructionKind::TakeLocal { local }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropClosure { local }
                    | InstructionKind::DropErrorUnion { local }
                    | InstructionKind::DropEnum { local }
                    | InstructionKind::DropTuple { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropOwnedArray { local }
                    | InstructionKind::EndPackageCapture { local, .. }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local) => out[local.0 as usize] = false,
                    _ => {}
                }
            }
            if inputs[b] != next || outputs[b] != out {
                inputs[b] = next;
                outputs[b] = out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok((inputs, outputs))
}
fn check_claimed_set(claim: &[LocalId], actual: &[bool], span: Span) -> Check<()> {
    let expected: Vec<LocalId> = actual
        .iter()
        .enumerate()
        .filter(|(_, present)| **present)
        .map(|(i, _)| LocalId(i as u32))
        .collect();
    if claim != expected {
        Err(reject(
            "E-VERIFY-INITIALIZATION-CLAIM",
            Some(span),
            "candidate initialization set differs from independently checked CFG facts",
        ))
    } else {
        Ok(())
    }
}
fn check_use(
    value: ValueId,
    block: usize,
    before_instruction: usize,
    definitions: &[Option<(usize, usize)>],
    dom: &[Vec<bool>],
    span: Span,
) -> Check<()> {
    let &(definition_block, instruction) = definitions
        .get(value.0 as usize)
        .and_then(Option::as_ref)
        .ok_or_else(|| reject("E-VERIFY-SSA", Some(span), "operand has no definition"))?;
    if (definition_block == block && instruction >= before_instruction)
        || !dom[block][definition_block]
    {
        return Err(reject(
            "E-VERIFY-DOMINANCE",
            Some(span),
            "SSA definition does not precede and dominate its use",
        ));
    }
    Ok(())
}

pub fn verify_ct(
    module: CtModule,
    expected_target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedCTIR> {
    match check_ct(&module, expected_target, limits) {
        Ok(()) => Ok(VerifiedCTIR { module }),
        Err(diagnostic) => Err(vec![diagnostic]),
    }
}
/// Mandatory raw-wire decoding and independent CT verification. The `CERT`
/// wire section carries untrusted type claims, never a preverified wrapper.
pub fn verify_ct_bytes(
    bytes: &[u8],
    expected_target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<VerifiedCTIR> {
    verify_ct(
        zil_ir::wire::decode_ct(bytes, limits)?,
        expected_target,
        limits,
    )
}
fn check_ct(module: &CtModule, expected_target: &TargetProfile, limits: &Limits) -> Check<()> {
    if module.schema_version != SCHEMA_VERSION {
        return Err(reject("E-VERIFY-SCHEMA", None, "unknown CT schema version"));
    }
    check_target(&module.target, expected_target)?;
    if !module.dependencies.is_empty() {
        return Err(unsupported(
            "CT dependency authority is not implemented; a raw completed flag is not a verified query capability",
            None,
        ));
    }
    if module.max_steps == 0
        || module.max_steps > limits.max_ct_steps
        || module.instructions.len() as u64 > module.max_steps
        || module.instructions.len() > limits.max_instructions
        || module.instructions.len() > limits.max_values
    {
        return Err(limit(
            "CT operation count or declared execution budget exceeds its permitted bound",
        ));
    }
    let mut budget = Budget::new(limits);
    // Charge before allocating the type table, including on the public raw API.
    // A trusted source producer's earlier limits cannot protect cached inputs.
    budget.spend(module.instructions.len())?;
    for text in [
        &module.target.id,
        &module.target.triple,
        &module.target.backend,
        &module.target.rule_version,
    ] {
        budget.spend(text.len())?;
    }
    let mut types: Vec<Type> = Vec::with_capacity(module.instructions.len());
    for (index, instruction) in module.instructions.iter().enumerate() {
        budget.spend(match &instruction.kind {
            CtInstructionKind::Constant(_) => 1,
            CtInstructionKind::Unary { .. } => 2,
            CtInstructionKind::ConvertInteger { .. } | CtInstructionKind::ConvertNumber { .. } => 2,
            CtInstructionKind::Binary { .. } => 3,
            CtInstructionKind::Forbidden(name) => name.len().saturating_add(1),
        })?;
        if instruction.result.0 as usize != index {
            return Err(reject(
                "E-CT-SSA",
                None,
                "CT result IDs must be sequential and defined exactly once",
            ));
        }
        check_type_shape(&instruction.ty, &module.target, limits, &mut budget)?;
        if let CtInstructionKind::ConvertInteger { target, .. }
        | CtInstructionKind::ConvertNumber { target, .. } = &instruction.kind
        {
            check_type_shape(target, &module.target, limits, &mut budget)?;
        }
        // CT has its own closed capability set. Expanding runtime Copy
        // admission must never admit static native addresses through raw CT.
        if !ct_program::serializable(&instruction.ty)
            || matches!(
                &instruction.kind,
                CtInstructionKind::Constant(
                    Constant::NullHandle | Constant::NullWideString | Constant::WideString(_)
                )
            )
        {
            return Err(reject(
                "E-CT-FORBIDDEN",
                None,
                "CT cannot construct or carry runtime foreign values",
            ));
        }
        check_type(&instruction.ty, None)?;
        let lookup = |value: ValueId| {
            types.get(value.0 as usize).ok_or_else(|| {
                reject(
                    "E-CT-SSA",
                    None,
                    "CT operand is not a previously defined value",
                )
            })
        };
        let ty = match &instruction.kind {
            CtInstructionKind::Constant(constant) => {
                native_values::constant(constant, &module.target, None, limits, &mut budget)?;
                constant.ty()
            }
            CtInstructionKind::Unary { op, operand } => unary_result(*op, lookup(*operand)?, None)?,
            CtInstructionKind::ConvertNumber { value, target } => {
                number_conversion(lookup(*value)?, target, None)?
            }
            CtInstructionKind::ConvertInteger { value, target } => {
                integer_conversion(lookup(*value)?, target, None)?
            }
            CtInstructionKind::Binary { op, left, right } => {
                binary_result(*op, lookup(*left)?, lookup(*right)?, None)?
            }
            CtInstructionKind::Forbidden(operation) => {
                return Err(reject(
                    "E-CT-FORBIDDEN",
                    None,
                    format!("CT execution cannot perform operation {operation}"),
                ));
            }
        };
        if ty != instruction.ty {
            return Err(reject(
                "E-CT-TYPE",
                None,
                "CT result type disagrees with checked operation",
            ));
        }
        types.push(ty);
    }
    if module.result.0 as usize >= types.len() {
        return Err(reject(
            "E-CT-RESULT",
            None,
            "CT result references an absent value",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
