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

use std::collections::BTreeSet;
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, Limits, Phase, RULE_VERSION, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

mod borrows;
mod byte_buffers;
mod c_layout;
mod cleanup;
#[cfg(test)]
mod closure_tests;
mod closures;
mod ct_program;
mod dynamic_store;
#[cfg(test)]
mod error_scope_tests;
mod errors;
mod foreign;
#[cfg(test)]
mod helper_tests;
mod helpers;
mod native_values;
#[cfg(test)]
mod package_tests;
mod packages;
#[cfg(test)]
mod resource_closure_tests;
#[cfg(test)]
mod resource_tests;
mod resources;
mod retained;
#[cfg(test)]
mod retained_tests;
mod slices;
pub use ct_program::{VerifiedCTProgram, verify_ct_program, verify_ct_program_bytes};

/// A checked, immutable scalar/readonly-closure CFG, including independently
/// reconstructed local shared-address provenance and lifetime constraints.
#[derive(Clone, Debug)]
pub struct VerifiedModule {
    module: TypedModule,
    plan: CandidatePlan,
    fingerprint: Fingerprint,
}
impl VerifiedModule {
    pub fn module(&self) -> &TypedModule {
        &self.module
    }
    pub fn plan(&self) -> &CandidatePlan {
        &self.plan
    }
    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
    /// Only contracts from this independently checked immutable snapshot are
    /// available to an interface publisher or public caller.
    pub fn callable_contract(&self, function: FunctionId) -> Option<&CallableParameterContract> {
        self.module
            .functions
            .get(function.0 as usize)?
            .callable_contract
            .as_ref()
    }

    pub fn callable_return_contract(
        &self,
        function: FunctionId,
    ) -> Option<&CallableReturnContract> {
        self.module
            .functions
            .get(function.0 as usize)?
            .callable_return_contract
            .as_ref()
    }

    /// Derived from the checked snapshot, never from a raw interface claim.
    /// Scalar returns have an empty set; an unknown FunctionId returns None.
    pub fn result_origins(&self, function: FunctionId) -> Option<Vec<u32>> {
        let f = self.module.functions.get(function.0 as usize)?;
        let plan = self.plan.functions.get(function.0 as usize)?;
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
}
impl Budget {
    fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.max_analysis_steps,
        }
    }
    fn spend(&mut self, amount: usize) -> Check<()> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or_else(|| limit("independent verification work budget exhausted"))?;
        Ok(())
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
    match check_module(&module, &plan, expected_target, limits) {
        Ok(fingerprint) => Ok(VerifiedModule {
            module,
            plan,
            fingerprint,
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

fn check_module(
    module: &TypedModule,
    plan: &CandidatePlan,
    expected_target: &TargetProfile,
    limits: &Limits,
) -> Check<Fingerprint> {
    if module.schema_version != SCHEMA_VERSION || plan.schema_version != SCHEMA_VERSION {
        return Err(reject(
            "E-VERIFY-SCHEMA",
            None,
            "unknown module or candidate schema version",
        ));
    }
    check_target(&module.target, expected_target)?;
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
    for ty in module
        .functions
        .iter()
        .flat_map(|f| {
            std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(f.values.iter())
        })
        .chain(module.closures.iter().flat_map(|c| {
            c.captures
                .iter()
                .chain(c.parameters.iter())
                .chain(std::iter::once(&c.result))
        }))
    {
        check_type_shape(ty, &module.target, limits, &mut budget)?;
    }
    closures::check_definitions(module, limits, &mut budget)?;
    foreign::check_declarations(module, &mut budget)?;
    let mut names = BTreeSet::new();
    let (mut block_count, mut instruction_count, mut local_count, mut value_count) =
        (0usize, 0usize, 0usize, 0usize);
    for (index, function) in module.functions.iter().enumerate() {
        helpers::check_claim_shape(function, limits, &mut budget)?;
        retained::check_claim(module, function, limits, &mut budget)?;
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
            check_local_type(&local.ty, Some(local.span), module, &mut budget)?;
        }
        for ty in &function.values {
            budget.spend(1)?;
            check_local_type(ty, Some(function.span), module, &mut budget)?;
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
                    | InstructionKind::MakeVariant { fields, .. } => budget.spend(fields.len())?,
                    InstructionKind::Call { arguments, .. }
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
                    InstructionKind::MakeClosure { captures, .. } => {
                        budget.spend(captures.len())?
                    }
                    InstructionKind::ConvertInteger { target, .. } => {
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
        retained::check_body(module, function, &graph, &mut budget)?;
        graphs.push(graph);
    }
    helpers::check_module(module, &graphs, &mut budget)?;
    borrows::check_module(module, plan, &graphs, &mut budget)?;
    resources::check_module(module, &graphs, &mut budget)?;
    Ok(fingerprint)
}

fn add_count(before: usize, add: usize, maximum: usize, kind: &str) -> Check<usize> {
    match before.checked_add(add) {
        Some(total) if total <= maximum => Ok(total),
        _ => Err(limit(format!("{kind} count exceeds verification limit"))),
    }
}
fn check_type_shape(
    ty: &Type,
    target: &TargetProfile,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    let mut pending = vec![(ty, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        budget.spend(1)?;
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
            Type::OwnedStore { element } => {
                pending.push((element, depth + 1));
            }
            Type::Slice { element } => {
                slices::target(target)?;
                pending.push((element, depth + 1));
            }
            Type::Array { element, length } => {
                budget.spend(*length as usize)?;
                pending.push((element, depth + 1));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                for ty in fields {
                    pending.push((ty, depth + 1));
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                budget.spend(name.len())?;
                budget.spend(fields.len())?;
                for (name, ty) in fields {
                    budget.spend(name.len())?;
                    pending.push((ty, depth + 1));
                }
            }
            Type::Enum { name, variants } => {
                budget.spend(name.len().saturating_add(variants.len()))?;
                for (name, fields) in variants {
                    budget.spend(name.len().saturating_add(fields.len()))?;
                    for ty in fields {
                        pending.push((ty, depth + 1));
                    }
                }
            }
            Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => {
                pending.push((inner, depth + 1));
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len())?;
                pending.push((result, depth + 1));
                for parameter in parameters {
                    pending.push((parameter, depth + 1));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// Called only after check_type_shape has bounded the raw tree. Keeping
// admission independent from producer helpers is part of the trust boundary.
fn check_type(ty: &Type, span: Option<Span>) -> Check<()> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
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
    match ty {
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
    match ty {
        Type::Address { inner, .. } if ty.is_shared_resource_head_address() => {
            resources::check_type_for(module, inner, span, budget)
        }
        Type::Managed(_) | Type::Desc(_) => resources::check_type_for(module, ty, span, budget),
        Type::Package { .. } => packages::check_type_for(module, ty, span, budget),
        Type::ErrorUnion { .. } => errors::check_type_for(module, ty, span, budget),
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
) -> Check<()> {
    match ty {
        Type::Closure(id) if (id.0 as usize) < module.closures.len() => Ok(()),
        Type::Closure(_) => Err(reject(
            "E-VERIFY-CLOSURE",
            span,
            "local closure type references an absent environment",
        )),
        Type::Address { inner, .. } if ty.is_shared_resource_head_address() => {
            resources::check_type_for(module, inner, span, budget)
        }
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => closures::definition(module, *id, span).map(|_| ()),
            _ => slices::check_pointee(inner, span),
        },
        Type::Managed(_) | Type::Desc(_) => resources::check_type_for(module, ty, span, budget),
        Type::Package { .. } => packages::check_type_for(module, ty, span, budget),
        Type::ErrorUnion { .. } => errors::check_type_for(module, ty, span, budget),
        ty if resources::arrays::is_owned_type(ty) => {
            resources::arrays::check_type_for(module, ty, span, budget)
        }
        _ => check_type(ty, span),
    }
}

struct FunctionGraph {
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    reachable: Vec<bool>,
    package_views: packages::CaptureViews,
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
                InstructionKind::TakeLocal { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
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
    };
    foreign::check_regions(function, &graph, budget)?;
    cleanup::check_function(function, &graph, budget)?;
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
    let expected = match &instruction.kind {
        InstructionKind::OwnedStoreBorrow { .. } | InstructionKind::BorrowPackageField { .. } => {
            return Err(Diagnostic::unsupported(
                Phase::Verify,
                "State field/window semantics are not yet implemented",
                span,
            ));
        }
        InstructionKind::OwnedStoreNew { .. }
        | InstructionKind::OwnedStoreReserve { .. }
        | InstructionKind::OwnedStorePush { .. }
        | InstructionKind::OwnedStorePop { .. }
        | InstructionKind::OwnedStoreLen { .. }
        | InstructionKind::OwnedStoreCapacity { .. } => {
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
        | InstructionKind::DropPackage { .. } => {
            packages::check_instruction(module, function, instruction, budget)?
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
        InstructionKind::UnsafeEnter { .. }
        | InstructionKind::UnsafeExit { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. } => None,
        InstructionKind::TakeErrorUnion { .. } | InstructionKind::DropErrorUnion { .. } => {
            errors::check_instruction(function, instruction)?
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
        InstructionKind::ForeignCall {
            function: callee,
            arguments,
            ..
        } => foreign::check_call(module, function, *callee, arguments, span, budget)?,
        InstructionKind::Constant(constant) => Some(constant.ty()),
        InstructionKind::ConvertInteger { value, target } => Some(integer_conversion(
            value_type(function, *value, span)?,
            target,
            span,
        )?),
        InstructionKind::Load(local) => {
            let ty = local_type(function, *local, span)?;
            if errors::owned(ty) {
                return Err(reject(
                    "E-VERIFY-ERROR",
                    span,
                    "an owned error union requires TakeLocal, never copying Load",
                ));
            }
            if matches!(ty, Type::Package { .. }) || resources::arrays::is_owned_type(ty) {
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
                && !errors::owned(ty)
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
                Type::Managed(payload) if !*mutable => {
                    resources::check_type_for(module, inner, span, budget)?;
                    payload.as_ref()
                }
                other => other,
            };
            let field = match projected {
                Type::Tuple(fields) => fields.get(*index as usize),
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, ty)| ty),
                _ => None,
            }
            .ok_or_else(|| {
                reject(
                    "E-VERIFY-ADDRESS",
                    span,
                    "field projection index outside its record or tuple",
                )
            })?;
            check_type(field, span)?;
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
            Type::Address { inner, .. } if inner.is_copy_value() => Some(inner.as_ref().clone()),
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
        InstructionKind::CallIndirect { callee, arguments } => {
            let Type::FunctionPointer { parameters, result } = value_type(function, *callee, span)?
            else {
                return Err(reject(
                    "E-VERIFY-INDIRECT-CALL",
                    span,
                    "callee must be a function pointer",
                ));
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
            let expected = ty.aggregate_fields().ok_or_else(|| {
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
            let Type::Enum { variants, .. } = ty else {
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
            if !matches!(value_type(function, *value, span)?, Type::Enum { .. }) {
                return Err(reject("E-VERIFY-ENUM", span, "tag requires an enum value"));
            }
            Some(Type::I32)
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let Type::Enum { variants, .. } = value_type(function, *value, span)? else {
                return Err(reject(
                    "E-VERIFY-ENUM",
                    span,
                    "payload projection requires an enum value",
                ));
            };
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
            let fields = ty.aggregate_fields().ok_or_else(|| {
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
            let fields = ty.aggregate_fields().ok_or_else(|| {
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
            ) {
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
            if !matches!(definition.kind, CallableKind::FnMut | CallableKind::FnOnce)
                || !field.is_copy_value()
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

fn unary_result(op: UnaryOp, operand: &Type, span: Option<Span>) -> Check<Type> {
    match op {
        UnaryOp::Not if operand == &Type::Bool => Ok(Type::Bool),
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
        if matches!(left, Type::U8 | Type::U16 | Type::I32 | Type::I64) && left == right {
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
    match op {
        BinaryOp::Equal | BinaryOp::NotEqual
            if matches!(
                left,
                Type::Bool
                    | Type::I32
                    | Type::I64
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
        InstructionKind::Constant(_)
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
        InstructionKind::OwnedStoreReserve { store, capacity } => {
            ([Some(*store), Some(*capacity)], &[])
        }
        InstructionKind::OwnedStorePush { store, value } => ([Some(*store), Some(*value)], &[]),
        InstructionKind::OwnedStoreBorrow { store, index } => ([Some(*store), Some(*index)], &[]),
        InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store } => ([Some(*store), None], &[]),
        InstructionKind::Store { value, .. }
        | InstructionKind::ConvertInteger { value, .. }
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
                InstructionKind::TakeLocal { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
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
                    InstructionKind::TakeLocal { local }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropClosure { local }
                    | InstructionKind::DropErrorUnion { local }
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
            CtInstructionKind::ConvertInteger { .. } => 2,
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
        if let CtInstructionKind::ConvertInteger { target, .. } = &instruction.kind {
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
            CtInstructionKind::Constant(constant) => constant.ty(),
            CtInstructionKind::Unary { op, operand } => unary_result(*op, lookup(*operand)?, None)?,
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
