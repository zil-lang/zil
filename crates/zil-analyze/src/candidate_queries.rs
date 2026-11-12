//! Function candidate queries. Keys and fingerprints never grant Verify authority.
//! Scalar direct calls read callee signatures, not callee implementations. Other
//! domains retain the complete module dependency and run the original analyses.
use super::*;
use std::collections::BTreeMap;
use std::hash::Hash;
use std::sync::Arc;
use zil_base::{Fingerprint, FingerprintBuilder};

/// Results from the most recent successful Analyze epoch. An equal `output`
/// means the raw FunctionPlan is equal; it says nothing about unchanged machine
/// code or unchanged Verify inputs. Consumers must also bind the actual raw CFG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisFunctionResult {
    pub function: FunctionId,
    pub input: Fingerprint,
    pub dependencies: Fingerprint,
    pub output: Fingerprint,
    pub reused: bool,
    /// False uses the complete module dependency and conservative fresh work.
    pub function_scoped: bool,
}

pub(super) struct Entry {
    function: TypedFunction,
    input: Fingerprint,
    dependencies: Arc<Vec<Signature>>,
    context: Fingerprint,
    dependency_fingerprint: Fingerprint,
    limits: Limits,
    costs: [usize; 3],
    result: FunctionPlan,
    output: Fingerprint,
}

struct InitializationEntry {
    input: incremental::Input,
    value: Arc<incremental::Entry>,
}

pub(super) struct Initialized {
    function: TypedFunction,
    value: Arc<InitializationEntry>,
}

type FinishedQueries = (
    Arc<BTreeMap<FunctionId, Arc<Entry>>>,
    Arc<Vec<AnalysisFunctionResult>>,
    resources::incremental::State,
    Arc<BTreeMap<FunctionId, Arc<Initialized>>>,
);

#[derive(Clone, PartialEq, Eq, Hash)]
struct Signature {
    id: FunctionId,
    name: String,
    call_kind: CallKind,
    parameters: Vec<(LocalId, Type)>,
    result: Type,
    result_mode: PassingMode,
}

impl Signature {
    fn of(function: &TypedFunction) -> Option<Self> {
        if !scalar_envelope(function) {
            return None;
        }
        let parameters = function
            .parameters
            .iter()
            .map(|id| {
                function
                    .locals
                    .get(id.0 as usize)
                    .map(|local| (*id, local.ty.clone()))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            id: function.id,
            name: function.name.clone(),
            call_kind: function.call_kind,
            parameters,
            result: function.return_type.clone(),
            result_mode: function.result_mode,
        })
    }
}

struct Work {
    prior: Option<Arc<Entry>>,
    input: Fingerprint,
    dependencies: Arc<Vec<Signature>>,
    context: Fingerprint,
    dependency_fingerprint: Fingerprint,
    compatible: bool,
    costs: [Option<usize>; 3],
    executed: bool,
}

pub(super) struct Queries {
    prior: Arc<BTreeMap<FunctionId, Arc<Entry>>>,
    work: BTreeMap<FunctionId, Work>,
    limits: Limits,
    previous_results: Arc<Vec<AnalysisFunctionResult>>,
    inputs_built: usize,
    dependency_signatures_checked: usize,
    pub(super) resources: resources::incremental::Request,
    prior_initializations: Arc<BTreeMap<FunctionId, Arc<Initialized>>>,
    initializations: BTreeMap<FunctionId, Arc<Initialized>>,
}

impl Queries {
    pub(super) fn record_stats(&self, stats: &mut AnalysisStats) {
        stats.candidate_inputs_built = self.inputs_built;
        stats.dependency_signatures_checked = self.dependency_signatures_checked;
        self.resources.record_stats(stats);
    }

    pub(super) fn new(
        prior: Arc<BTreeMap<FunctionId, Arc<Entry>>>,
        previous_results: Arc<Vec<AnalysisFunctionResult>>,
        resources: resources::incremental::State,
        prior_initializations: Arc<BTreeMap<FunctionId, Arc<Initialized>>>,
        limits: &Limits,
    ) -> Self {
        Self {
            prior,
            work: BTreeMap::new(),
            limits: limits.clone(),
            previous_results,
            inputs_built: 0,
            dependency_signatures_checked: 0,
            resources: resources::incremental::Request::new(resources, limits),
            prior_initializations,
            initializations: BTreeMap::new(),
        }
    }

    /// Called only after complete raw admission, before any candidate work.
    pub(super) fn prepare(&mut self, module: &TypedModule, budget: &Budget) {
        self.resources.prepare(module, budget);
        if !module.closures.is_empty()
            || !module.resources.is_empty()
            || !module.life_declarations.is_empty()
            || !module.life_instances.is_empty()
            || !module.dyn_declarations.is_empty()
            || !module.dyn_interfaces.is_empty()
            || !module.dyn_vtables.is_empty()
            || !module.unsupported.is_empty()
            || budget.head_descriptions
            || budget.addressed_sums
            || budget.mixed_address_sums
            || budget.affine_packages
            || budget.stored_borrows
            || budget.generic_mutex
            || budget.tasks
            || budget.registrations
            || budget.async_registrations
            || budget.borrowed_callback_inputs
            || budget.borrowed_function_pointers
            || budget.invocation_callbacks
            || budget.scoped_foreign
            || budget.scoped_threads
            || !budget.replacement_functions.is_empty()
            || module.functions.iter().any(|function| {
                matches!(function.return_type, Type::Address { .. })
                    || holds::retained_task_value(&function.return_type)
            })
        {
            return;
        }
        let mut context = FingerprintBuilder::new();
        module.schema_version.hash(&mut context);
        module.module_id.hash(&mut context);
        module.target.fingerprint().hash(&mut context);
        let context = context.digest();
        let mut work = BTreeMap::new();
        'functions: for function in &module.functions {
            let prior = self.prior.get(&function.id).cloned();
            // SharedVec equality first checks immutable allocation identity.
            // Stable products need no body hash or dependency-site scan.
            let same_input = prior
                .as_ref()
                .is_some_and(|prior| semantic_input_eq(&prior.function, function));
            let (input, ids) = if same_input {
                let prior = prior.as_ref().expect("same input");
                (
                    prior.input,
                    prior
                        .dependencies
                        .iter()
                        .map(|signature| signature.id)
                        .collect::<Vec<_>>(),
                )
            } else {
                let Some(input) = scalar_input(function) else {
                    continue;
                };
                self.inputs_built += 1;
                input
            };
            let mut dependencies = Vec::with_capacity(ids.len());
            let mut same_dependencies = same_input;
            for (index, id) in ids.into_iter().enumerate() {
                self.dependency_signatures_checked += 1;
                let Some(target) = module
                    .functions
                    .get(id.0 as usize)
                    .filter(|target| target.id == id)
                else {
                    continue 'functions;
                };
                let Some(signature) = Signature::of(target) else {
                    continue 'functions;
                };
                same_dependencies &= prior
                    .as_ref()
                    .and_then(|prior| prior.dependencies.get(index))
                    == Some(&signature);
                dependencies.push(signature);
            }
            let dependencies = if same_dependencies {
                prior
                    .as_ref()
                    .expect("same dependencies")
                    .dependencies
                    .clone()
            } else {
                Arc::new(dependencies)
            };
            let dependency_fingerprint = if same_dependencies
                && prior.as_ref().is_some_and(|prior| prior.context == context)
            {
                prior
                    .as_ref()
                    .expect("same dependency context")
                    .dependency_fingerprint
            } else {
                let mut hash = FingerprintBuilder::new();
                "zil-analyze-scalar-dependencies-v1".hash(&mut hash);
                context.hash(&mut hash);
                dependencies.as_ref().hash(&mut hash);
                hash.digest()
            };
            let compatible = prior.as_ref().is_some_and(|prior| {
                same_input
                    && prior.dependencies == dependencies
                    && prior.dependency_fingerprint == dependency_fingerprint
                    && prior.limits == self.limits
            });
            work.insert(
                function.id,
                Work {
                    prior,
                    input,
                    dependencies,
                    context,
                    dependency_fingerprint,
                    compatible,
                    costs: [None; 3],
                    executed: false,
                },
            );
        }
        self.work = work;
    }

    pub(super) fn initialization(
        &mut self,
        function: &TypedFunction,
        input: Option<&incremental::Input>,
        budget: &mut Budget,
    ) -> CompileResult<Option<Initialization>> {
        let Some(prior) = self.prior_initializations.get(&function.id) else {
            return Ok(None);
        };
        let same_input = input.is_none() && semantic_input_eq(&prior.function, function);
        if !(same_input || input.is_some_and(|input| prior.value.input == *input))
            || prior.value.value.cost > budget.remaining()
        {
            return Ok(None);
        }
        budget.spend(prior.value.value.cost, "cached initialization")?;
        let initialized = if same_input {
            prior.clone()
        } else {
            Arc::new(Initialized {
                function: function.clone(),
                value: prior.value.clone(),
            })
        };
        let result = initialized.value.value.result.clone();
        self.initializations.insert(function.id, initialized);
        Ok(Some(result))
    }

    pub(super) fn save_initialization(
        &mut self,
        function: &TypedFunction,
        input: incremental::Input,
        value: Arc<incremental::Entry>,
    ) {
        let value = Arc::new(InitializationEntry { input, value });
        self.initializations.insert(
            function.id,
            Arc::new(Initialized {
                function: function.clone(),
                value,
            }),
        );
    }

    fn hit(
        &mut self,
        function: &TypedFunction,
        stage: usize,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        let Some(work) = self.work.get_mut(&function.id) else {
            return Ok(false);
        };
        if work.compatible
            && let Some(prior) = &work.prior
            && prior.costs[stage] <= budget.remaining()
        {
            let cost = prior.costs[stage];
            budget.spend(cost, "cached function candidate stage")?;
            work.costs[stage] = Some(cost);
            return Ok(true);
        }
        work.executed = true;
        Ok(false)
    }

    fn save_cost(
        &mut self,
        function: &TypedFunction,
        stage: usize,
        before: usize,
        budget: &Budget,
    ) {
        if let Some(work) = self.work.get_mut(&function.id) {
            work.costs[stage] = Some(before - budget.remaining());
        }
    }

    pub(super) fn local_checks(
        &mut self,
        module: &TypedModule,
        function: &TypedFunction,
        reachable: &[bool],
        limits: &Limits,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        if self.hit(function, 0, budget)? {
            return Ok(());
        }
        let before = budget.remaining();
        retained::check_function(module, function, reachable, budget)?;
        address_sums::check(module, function, reachable, limits, budget)?;
        mixed_address_sums::check(module, function, reachable, limits, budget)?;
        self.save_cost(function, 0, before, budget);
        Ok(())
    }

    pub(super) fn scopes(
        &mut self,
        module: &TypedModule,
        function: &TypedFunction,
        package: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        if self.hit(function, 1, budget)? {
            return Ok(());
        }
        let before = budget.remaining();
        scopes::check(module, function, package, budget)?;
        self.save_cost(function, 1, before, budget);
        Ok(())
    }

    pub(super) fn holds(
        &mut self,
        function: &TypedFunction,
        budget: &mut Budget,
        execute: impl FnOnce(&mut Budget) -> CompileResult<Vec<ValueHold>>,
    ) -> CompileResult<crate::HoldRows> {
        if self.hit(function, 2, budget)? {
            // prepare/scalar_input admitted only scalar values here. Later
            // stored-proof overlays modify address/qualified rows, so a cached
            // scalar row never incurs an uncharged COW copy in global assembly.
            // Fresh richer rows remain uniquely owned until those checks end.
            return Ok(self.work[&function.id]
                .prior
                .as_ref()
                .expect("hit entry")
                .result
                .holds
                .clone());
        }
        let before = budget.remaining();
        let result = execute(budget)?;
        self.save_cost(function, 2, before, budget);
        Ok(result.into())
    }

    pub(super) fn finish(
        self,
        module: &TypedModule,
        plan: &CandidatePlan,
        stats: &mut AnalysisStats,
    ) -> FinishedQueries {
        stats.candidate_inputs_built = self.inputs_built;
        stats.dependency_signatures_checked = self.dependency_signatures_checked;
        let mut entries = BTreeMap::new();
        let mut results = Vec::with_capacity(plan.functions.len());
        let previous: BTreeMap<_, _> = self
            .previous_results
            .iter()
            .map(|result| (result.function, result.output))
            .collect();
        for (function, result) in module.functions.iter().zip(&plan.functions) {
            let output = self
                .work
                .get(&function.id)
                .filter(|work| !work.executed && work.costs.iter().all(Option::is_some))
                .and_then(|work| work.prior.as_ref())
                .map_or_else(|| output_fingerprint(result), |prior| prior.output);
            let (input, dependencies, reused, function_scoped) = if let Some(work) = self
                .work
                .get(&function.id)
                .filter(|work| work.costs.iter().all(Option::is_some))
            {
                let reused = !work.executed && work.costs.iter().all(Option::is_some);
                if reused {
                    stats.candidate_reused_functions += 1;
                } else {
                    stats.candidate_executed_functions += 1;
                }
                let old = work.prior.as_ref();
                if reused || old.is_some_and(|old| old.output == output && old.result == *result) {
                    stats.equal_function_results += 1;
                } else {
                    stats.changed_function_results += 1;
                }
                let entry = if reused {
                    old.expect("reused entry").clone()
                } else {
                    Arc::new(Entry {
                        function: function.clone(),
                        input: work.input,
                        dependencies: work.dependencies.clone(),
                        context: work.context,
                        dependency_fingerprint: work.dependency_fingerprint,
                        limits: self.limits.clone(),
                        costs: work
                            .costs
                            .map(|cost| cost.expect("complete function stage")),
                        result: result.clone(),
                        output,
                    })
                };
                entries.insert(function.id, entry);
                (work.input, work.dependency_fingerprint, reused, true)
            } else {
                stats.candidate_executed_functions += 1;
                if previous.get(&function.id) == Some(&output) {
                    stats.equal_function_results += 1;
                } else {
                    stats.changed_function_results += 1;
                }
                let mut input = FingerprintBuilder::new();
                "zil-analyze-conservative-input-v1".hash(&mut input);
                plan.module_fingerprint.hash(&mut input);
                function.id.hash(&mut input);
                (input.digest(), plan.module_fingerprint, false, false)
            };
            results.push(AnalysisFunctionResult {
                function: function.id,
                input,
                dependencies,
                output,
                reused,
                function_scoped,
            });
        }
        let resources = self.resources.finish(stats);
        (
            Arc::new(entries),
            Arc::new(results),
            resources,
            Arc::new(self.initializations),
        )
    }
}

/// Successful rows contain no diagnostics. Source positions are rebound by the
/// caller and are deliberately absent from semantic input equality. Keep the
/// destructure exhaustive so a new raw field cannot silently escape the key.
pub(super) fn semantic_input_eq(previous: &TypedFunction, current: &TypedFunction) -> bool {
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
    } = current;
    if (
        storage_life,
        call_kind,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contract,
    ) != (
        &previous.storage_life,
        &previous.call_kind,
        &previous.reference_return_bound,
        &previous.callable_return_contract,
        &previous.callable_contract,
        &previous.protocol_contract,
    ) || (
        id,
        name,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        entry,
    ) != (
        &previous.id,
        &previous.name,
        &previous.foreign,
        &previous.environment,
        &previous.parameters,
        &previous.return_type,
        &previous.result_mode,
        &previous.entry,
    ) || previous.values != *values
    {
        return false;
    }
    let locals_equal = previous.locals.ptr_eq(locals)
        || previous.locals.len() == locals.len()
            && previous
                .locals
                .iter()
                .zip(locals)
                .all(|(previous, current)| {
                    let LocalDecl {
                        id,
                        name,
                        ty,
                        mutable,
                        span: _,
                    } = current;
                    (id, name, ty, mutable)
                        == (
                            &previous.id,
                            &previous.name,
                            &previous.ty,
                            &previous.mutable,
                        )
                });
    locals_equal
        && (previous.blocks.ptr_eq(blocks)
            || previous.blocks.len() == blocks.len()
                && previous
                    .blocks
                    .iter()
                    .zip(blocks)
                    .all(|(previous, current)| {
                        let BasicBlock {
                            id,
                            instructions,
                            terminator,
                        } = current;
                        previous.id == *id
                            && previous.terminator == *terminator
                            && previous.instructions.len() == instructions.len()
                            && previous.instructions.iter().zip(instructions).all(
                                |(previous, current)| {
                                    let Instruction {
                                        result,
                                        kind,
                                        span: _,
                                    } = current;
                                    previous.result == *result && previous.kind == *kind
                                },
                            )
                    }))
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
    )
}

/// Compiler-local Hash encoding is not persisted. Complete equality is checked
/// as well as the SHA digest before any candidate computation can be skipped.
fn scalar_envelope(function: &TypedFunction) -> bool {
    function.call_kind == CallKind::Direct
        && function.storage_life.is_none()
        && function.reference_return_bound.is_none()
        && function.callable_return_contract.is_none()
        && function.callable_contract.is_none()
        && function.protocol_contract.is_none()
        && function.foreign.is_none()
        && function.environment.is_none()
        && scalar(&function.return_type)
}

fn scalar_input(function: &TypedFunction) -> Option<(Fingerprint, Vec<FunctionId>)> {
    if !scalar_envelope(function)
        || !function.locals.iter().all(|local| scalar(&local.ty))
        || !function.values.iter().all(scalar)
    {
        return None;
    }
    let mut hash = FingerprintBuilder::new();
    let mut callees = BTreeSet::new();
    "zil-analyze-scalar-input-v1".hash(&mut hash);
    function.id.hash(&mut hash);
    function.name.hash(&mut hash);
    function.parameters.hash(&mut hash);
    function.return_type.hash(&mut hash);
    function.result_mode.hash(&mut hash);
    function.entry.hash(&mut hash);
    function.locals.len().hash(&mut hash);
    for local in &function.locals {
        (local.id, &local.name, &local.ty, local.mutable).hash(&mut hash);
    }
    function.values.hash(&mut hash);
    function.blocks.len().hash(&mut hash);
    for block in &function.blocks {
        block.id.hash(&mut hash);
        block.instructions.len().hash(&mut hash);
        for instruction in &block.instructions {
            instruction.result.hash(&mut hash);
            match &instruction.kind {
                InstructionKind::Constant(value) if scalar(&value.ty()) => {
                    0u8.hash(&mut hash);
                    value.hash(&mut hash);
                }
                InstructionKind::Load(local) => {
                    1u8.hash(&mut hash);
                    local.hash(&mut hash);
                }
                InstructionKind::Store { local, value } => {
                    2u8.hash(&mut hash);
                    (local, value).hash(&mut hash);
                }
                InstructionKind::StorageLive(local) => {
                    3u8.hash(&mut hash);
                    local.hash(&mut hash);
                }
                InstructionKind::StorageDead(local) => {
                    4u8.hash(&mut hash);
                    local.hash(&mut hash);
                }
                InstructionKind::Unary { op, operand } => {
                    5u8.hash(&mut hash);
                    (op, operand).hash(&mut hash);
                }
                InstructionKind::Binary { op, left, right } => {
                    6u8.hash(&mut hash);
                    (op, left, right).hash(&mut hash);
                }
                InstructionKind::ConvertInteger { value, target } if scalar(target) => {
                    7u8.hash(&mut hash);
                    (value, target).hash(&mut hash);
                }
                InstructionKind::ConvertNumber { value, target } if scalar(target) => {
                    8u8.hash(&mut hash);
                    (value, target).hash(&mut hash);
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    9u8.hash(&mut hash);
                    (function, arguments).hash(&mut hash);
                    callees.insert(*function);
                }
                InstructionKind::ScopeEnter { scope } => {
                    10u8.hash(&mut hash);
                    scope.hash(&mut hash);
                }
                InstructionKind::ScopeExit { scope, kind } => {
                    11u8.hash(&mut hash);
                    (scope, kind).hash(&mut hash);
                }
                _ => return None,
            }
        }
        match block.terminator {
            Terminator::Return(value) => {
                0u8.hash(&mut hash);
                value.hash(&mut hash);
            }
            Terminator::Jump(target) => {
                1u8.hash(&mut hash);
                target.hash(&mut hash);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                2u8.hash(&mut hash);
                (condition, then_block, else_block).hash(&mut hash);
            }
            Terminator::Unreachable => 3u8.hash(&mut hash),
            Terminator::Trap => 4u8.hash(&mut hash),
        }
    }
    Some((hash.digest(), callees.into_iter().collect()))
}

fn output_fingerprint(result: &FunctionPlan) -> Fingerprint {
    let mut hash = FingerprintBuilder::new();
    "zil-analyze-function-plan-v1".hash(&mut hash);
    result.function.hash(&mut hash);
    result.blocks.len().hash(&mut hash);
    for block in &result.blocks {
        (block.block, &block.initialized_in, &block.initialized_out).hash(&mut hash);
    }
    result.holds.len().hash(&mut hash);
    for hold in &result.holds {
        (hold.value, &hold.roots).hash(&mut hash);
    }
    hash.digest()
}

#[cfg(test)]
#[path = "candidate_queries_tests.rs"]
mod tests;
