//! Process-local independent proof receipts. No caller-supplied key, digest,
//! candidate, analyzer result, or serialized value can construct a receipt.
//!
//! A complete receipt binds the retained immutable input allocations, selected
//! target, and every limit. A function receipt proves only the local flow
//! obligations below. Its dependencies are the complete function and candidate
//! row (except diagnostic spans), target and every limit. Instruction proofs
//! additionally bind every callee signature field they read. Unfactored contextual
//! checks and global obligations still run before issuing a new module authority. In particular a cached CFG is not a callee-effect proof.
use super::*;

#[cfg(test)]
mod tests;

/// Actual proof execution on the most recent request, including failed requests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VerifyStats {
    pub full_receipts_reused: usize,
    pub function_proofs_checked: usize,
    pub function_proofs_reused: usize,
    pub instruction_proofs_reused: usize,
}

/// One retained successful epoch, with independently produced local receipts.
/// Cloning is a cheap transaction checkpoint; failed calls publish no receipts.
#[derive(Clone, Default)]
pub struct VerifySession {
    verified: Option<VerifiedModule>,
    flows: Arc<BTreeMap<FunctionId, Arc<FlowReceipt>>>,
    stats: VerifyStats,
}

impl VerifySession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> VerifyStats {
        self.stats
    }

    pub fn verify_shared_module(
        &mut self,
        module: Arc<TypedModule>,
        plan: Arc<CandidatePlan>,
        expected_target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<VerifiedModule> {
        self.verify(
            VerifiedInputs::SharedModule { module, plan },
            expected_target,
            limits,
        )
    }

    pub fn verify_shared_program_module(
        &mut self,
        program: Arc<TypedProgram>,
        plan: Arc<CandidatePlan>,
        expected_target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<VerifiedModule> {
        self.verify(
            VerifiedInputs::SharedProgram { program, plan },
            expected_target,
            limits,
        )
    }

    fn verify(
        &mut self,
        inputs: VerifiedInputs,
        expected_target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<VerifiedModule> {
        self.stats = VerifyStats::default();
        if let Some(previous) = &self.verified
            && previous.limits() == limits
            && previous.module().target == *expected_target
            && same_allocations(&previous.inputs, &inputs)
        {
            // The retained strong Arc prevents safe mutation in place. get_mut
            // fails and make_mut allocates a different input. Target/rules and
            // schema were checked for these exact bytes; code/rules cannot
            // change during this process-local receipt's lifetime.
            let mut verified = previous.clone();
            verified.inputs = inputs;
            self.stats.full_receipts_reused = 1;
            return Ok(verified);
        }
        let mut attempt = Attempt {
            previous: &self.flows,
            previous_inputs: self.verified.as_ref(),
            next: BTreeMap::new(),
            bound_functions: BTreeMap::new(),
            stats: VerifyStats::default(),
        };
        let result = verify_inputs_with_reuse(inputs, expected_target, limits, Some(&mut attempt));
        self.stats = attempt.stats;
        if let Ok(verified) = &result {
            self.flows = Arc::new(attempt.next);
            self.verified = Some(verified.clone());
        }
        result
    }
}

fn same_allocations(previous: &VerifiedInputs, current: &VerifiedInputs) -> bool {
    match (previous, current) {
        (
            VerifiedInputs::SharedModule {
                module: a,
                plan: ap,
            },
            VerifiedInputs::SharedModule {
                module: b,
                plan: bp,
            },
        ) => Arc::ptr_eq(a, b) && Arc::ptr_eq(ap, bp),
        (
            VerifiedInputs::SharedProgram {
                program: a,
                plan: ap,
            },
            VerifiedInputs::SharedProgram {
                program: b,
                plan: bp,
            },
        ) => Arc::ptr_eq(a, b) && Arc::ptr_eq(ap, bp),
        _ => false,
    }
}

// Bindings live once in the session's retained immutable VerifiedInputs. Every
// successful epoch installs its input and complete next receipt map together.
// Reused proofs are rebound only after structural equality succeeds; receipts
// never retain deep-cloned bodies or a chain of historical whole modules.
struct FlowReceipt {
    work: usize,
    proof: FlowProof,
}

pub(super) struct Attempt<'a> {
    previous: &'a BTreeMap<FunctionId, Arc<FlowReceipt>>,
    previous_inputs: Option<&'a VerifiedModule>,
    next: BTreeMap<FunctionId, Arc<FlowReceipt>>,
    bound_functions: BTreeMap<FunctionId, bool>,
    stats: VerifyStats,
}

impl<'a> Attempt<'a> {
    fn function_matches(
        &mut self,
        function: &TypedFunction,
        target: &TargetProfile,
        limits: &Limits,
    ) -> bool {
        *self.bound_functions.entry(function.id).or_insert_with(|| {
            self.previous_inputs.is_some_and(|previous| {
                previous.limits() == limits
                    && previous.module().target == *target
                    && previous
                        .module()
                        .functions
                        .get(function.id.0 as usize)
                        .is_some_and(|prior| same_function(prior, function))
            })
        })
    }

    pub(super) fn instructions(
        &mut self,
        function: &TypedFunction,
        target: &TargetProfile,
        limits: &Limits,
    ) -> Option<InstructionProofs<'a>> {
        self.function_matches(function, target, limits)
            .then(|| InstructionProofs {
                previous: self.previous_inputs.expect("matched sealed epoch").module(),
                reused: std::cell::Cell::new(0),
            })
    }

    pub(super) fn record_instructions(&mut self, proofs: &InstructionProofs<'_>) {
        self.stats.instruction_proofs_reused += proofs.reused.replace(0);
    }

    pub(super) fn check_flow(
        &mut self,
        inputs: FlowInputs<'_>,
        target: &TargetProfile,
        budget: &mut Budget,
        limits: &Limits,
    ) -> Check<FlowProof> {
        let function_matches = self.function_matches(inputs.function, target, limits);
        if function_matches
            && let Some(receipt) = self.previous.get(&inputs.function.id)
            && self.previous_inputs.is_some_and(|previous| {
                previous.plan().functions.get(inputs.function.id.0 as usize) == Some(inputs.plan)
            })
            && receipt.work <= budget.remaining
        {
            // Equal complete inputs imply the same successful checks and cost.
            // If insufficient budget remains, execute the ordinary checks to
            // preserve their first error and its current diagnostic location.
            budget.spend(receipt.work)?;
            self.next.insert(inputs.function.id, receipt.clone());
            self.stats.function_proofs_reused += 1;
            return Ok(receipt.proof.clone());
        }
        self.stats.function_proofs_checked += 1;
        let before = budget.remaining;
        let proof = check_flow(inputs, budget, limits)?;
        self.next.insert(
            inputs.function.id,
            Arc::new(FlowReceipt {
                work: before - budget.remaining,
                proof: proof.clone(),
            }),
        );
        Ok(proof)
    }
}

/// The complete caller has already been compared against a sealed successful
/// epoch. These instruction groups read no candidate facts or ambient state
/// after the fresh instruction prelude. Remaining groups use the full checker.
pub(super) struct InstructionProofs<'a> {
    previous: &'a TypedModule,
    reused: std::cell::Cell<usize>,
}

impl InstructionProofs<'_> {
    pub(super) fn local(&self, kind: &InstructionKind) -> bool {
        let local = matches!(
            kind,
            InstructionKind::Constant(_)
                | InstructionKind::ConvertNumber { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::WriteAddress { .. }
                | InstructionKind::AddressIndex { .. }
                | InstructionKind::AddressSlice { .. }
                | InstructionKind::SliceLength { .. }
                | InstructionKind::DescribeLocal { .. }
                | InstructionKind::ReadDesc { .. }
                | InstructionKind::WriteDesc { .. }
                | InstructionKind::DropManaged { .. }
                | InstructionKind::MakeErrorUnion { .. }
                | InstructionKind::ErrorTag { .. }
                | InstructionKind::RegisterDefer { .. }
                | InstructionKind::UnsafeEnter { .. }
                | InstructionKind::UnsafeExit { .. }
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::StaticUtf8 { .. }
        );
        if local {
            self.reused.set(self.reused.get() + 1);
        }
        local
    }

    /// Exactly the other-function fields read by ordinary Call's signature
    /// checks. The teardown declaration and pointer-keyed array work are always
    /// checked against the current module separately, including their charges.
    pub(super) fn call(&self, callee: &TypedFunction) -> bool {
        let Some(previous) = self.previous.functions.get(callee.id.0 as usize) else {
            return false;
        };
        let same = previous.foreign == callee.foreign
            && previous.environment == callee.environment
            && previous.return_type == callee.return_type
            && previous.parameters == callee.parameters
            && previous.parameters.iter().all(|id| {
                previous.locals.get(id.0 as usize).map(|local| &local.ty)
                    == callee.locals.get(id.0 as usize).map(|local| &local.ty)
            });
        if same {
            self.reused.set(self.reused.get() + 1);
        }
        same
    }
}

/// Explicit equality excludes diagnostic locations only. Do not replace with a
/// fingerprint: raw input equality is part of the proof boundary. Destructuring
/// all fields makes future IR field additions require a binding decision here.
fn same_function(a: &TypedFunction, b: &TypedFunction) -> bool {
    let TypedFunction {
        storage_life,
        call_kind,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contract,
        id,
        name,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        locals,
        values,
        blocks,
        entry,
        span: _,
    } = a;
    storage_life == &b.storage_life
        && call_kind == &b.call_kind
        && reference_return_bound == &b.reference_return_bound
        && callable_return_contract == &b.callable_return_contract
        && callable_contract == &b.callable_contract
        && protocol_contract == &b.protocol_contract
        && id == &b.id
        && name == &b.name
        && foreign == &b.foreign
        && environment == &b.environment
        && parameters == &b.parameters
        && return_type == &b.return_type
        && result_mode == &b.result_mode
        && values == &b.values
        && entry == &b.entry
        && (locals.ptr_eq(&b.locals)
            || (locals.len() == b.locals.len()
                && locals.iter().zip(&b.locals).all(|(a, b)| {
                    let LocalDecl {
                        id,
                        name,
                        ty,
                        mutable,
                        span: _,
                    } = a;
                    id == &b.id && name == &b.name && ty == &b.ty && mutable == &b.mutable
                })))
        && (blocks.ptr_eq(&b.blocks)
            || (blocks.len() == b.blocks.len()
                && blocks.iter().zip(&b.blocks).all(|(a, b)| {
                    let BasicBlock {
                        id,
                        instructions,
                        terminator,
                    } = a;
                    id == &b.id
                        && terminator == &b.terminator
                        && instructions.len() == b.instructions.len()
                        && instructions.iter().zip(&b.instructions).all(|(a, b)| {
                            let Instruction {
                                result,
                                kind,
                                span: _,
                            } = a;
                            result == &b.result && kind == &b.kind
                        })
                })))
}

#[derive(Clone)]
pub(super) struct FlowProof {
    pub(super) reachable: Vec<bool>,
    pub(super) role_data_uses: Vec<ValueId>,
}

/// The auxiliary arrays are reconstructed from the current checked instructions
/// immediately before this call. They are deterministic functions of `function`.
#[derive(Clone, Copy)]
pub(super) struct FlowInputs<'a> {
    pub(super) function: &'a TypedFunction,
    pub(super) plan: &'a FunctionPlan,
    pub(super) predecessors: &'a [Vec<usize>],
    pub(super) successors: &'a [Vec<usize>],
    pub(super) definitions: &'a [Option<(usize, usize)>],
    pub(super) parameters: &'a [bool],
    pub(super) addressed_sums: bool,
    pub(super) mixed_address_sums: bool,
}

pub(super) fn check_flow(
    inputs: FlowInputs<'_>,
    request_budget: &mut Budget,
    limits: &Limits,
) -> Check<FlowProof> {
    // This subproof has no module or ambient derived-state input. Its callees
    // use only function data and spend(), including both addressed-tag checks.
    // Isolating the meter prevents accidental dependency on prior function state.
    let mut meter = Budget::new(limits);
    meter.remaining = request_budget.remaining;
    let result = check_flow_inner(inputs, &mut meter, limits);
    request_budget.remaining = meter.remaining;
    result
}

fn check_flow_inner(
    inputs: FlowInputs<'_>,
    budget: &mut Budget,
    limits: &Limits,
) -> Check<FlowProof> {
    let FlowInputs {
        function,
        plan,
        predecessors,
        successors,
        definitions,
        parameters,
        addressed_sums,
        mixed_address_sums,
    } = inputs;
    let fail = |code, message| reject(code, Some(function.span), message);
    let n = function.blocks.len();
    let m = function.locals.len();
    // Original check order is retained, including work-limit failures.
    if definitions.iter().any(Option::is_none) {
        return Err(fail("E-VERIFY-SSA", "declared SSA value has no definition"));
    }
    let reachable = reachability(function.entry.0 as usize, successors, budget)?;
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
    let dominators = dominators(function.entry.0 as usize, predecessors, &reachable, budget)?;
    if addressed_sums {
        address_sums::check(function, successors, &reachable, budget)?;
    }
    if mixed_address_sums {
        mixed_address_sums::check(function, successors, &reachable, budget)?;
    }
    let (initialized_in, initialized_out) =
        initialization(function, predecessors, &reachable, parameters, budget)?;
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
                check_use(operand, b, i, definitions, &dominators, instruction.span)?;
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
                    predecessors,
                    definitions,
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
                definitions,
                &dominators,
                function.span,
            )?;
        }
    }
    Ok(FlowProof {
        reachable,
        role_data_uses,
    })
}
