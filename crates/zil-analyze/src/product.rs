//! Internal push consumer. Every reusable row is Analyze's own successful raw
//! admission/analysis receipt, bound to immutable input owners and actual reads.
//! Rich cross-function domains remain complete global obligations; no producer
//! identity, result digest, or empty intermediate claim grants Verify authority.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use zil_base::{Fingerprint, FingerprintBuilder};

#[derive(Clone)]
pub(super) struct Recording {
    pub(super) costs: Vec<[usize; 5]>,
    pub(super) cost: usize,
}
pub(super) struct Complete {
    program: Arc<TypedProgram>,
    candidate: Arc<CandidatePlan>,
    limits: Limits,
}
#[derive(Clone)]
struct Row {
    function: Arc<TypedFunction>,
    callees: Vec<FunctionId>,
    output: Fingerprint,
    admission_cost: usize,
    dependencies: Fingerprint,
    span_sensitive: bool,
}
#[derive(Clone, Copy, Default)]
struct Total {
    cost: usize,
    blocks: usize,
    instructions: usize,
    locals: usize,
    values: usize,
}
impl Total {
    fn function(function: &TypedFunction, cost: usize) -> Option<Self> {
        Some(Self {
            cost,
            blocks: function.blocks.len(),
            instructions: function
                .blocks
                .iter()
                .try_fold(0usize, |n, b| n.checked_add(b.instructions.len()))?,
            locals: function.locals.len(),
            values: function.values.len(),
        })
    }
    fn add(self, other: Self) -> Option<Self> {
        Some(Self {
            cost: self.cost.checked_add(other.cost)?,
            blocks: self.blocks.checked_add(other.blocks)?,
            instructions: self.instructions.checked_add(other.instructions)?,
            locals: self.locals.checked_add(other.locals)?,
            values: self.values.checked_add(other.values)?,
        })
    }
    fn counts_fit(self, limits: &Limits) -> bool {
        self.blocks <= limits.max_blocks
            && self.instructions <= limits.max_instructions
            && self.locals <= limits.max_locals
            && self.values <= limits.max_values
    }
    fn fits(self, limits: &Limits, global: usize) -> bool {
        self.cost
            .checked_add(global)
            .is_some_and(|n| n <= limits.max_analysis_steps)
            && self.blocks <= limits.max_blocks
            && self.instructions <= limits.max_instructions
            && self.locals <= limits.max_locals
            && self.values <= limits.max_values
    }
}
#[derive(Clone)]
struct Sum {
    total: Total,
    len: usize,
    children: Option<(Arc<Sum>, Arc<Sum>)>,
}
impl Sum {
    fn build(values: &[Total]) -> Option<Arc<Self>> {
        if values.len() == 1 {
            return Some(Arc::new(Self {
                total: values[0],
                len: 1,
                children: None,
            }));
        }
        let middle = values.len().checked_next_power_of_two()? / 2;
        let left = Self::build(values.get(..middle)?)?;
        let right = Self::build(values.get(middle..)?)?;
        Some(Arc::new(Self {
            total: left.total.add(right.total)?,
            len: values.len(),
            children: Some((left, right)),
        }))
    }
    fn get(&self, index: usize) -> Option<Total> {
        if index >= self.len {
            return None;
        }
        match &self.children {
            None => Some(self.total),
            Some((left, right)) if index < left.len => left.get(index),
            Some((left, right)) => right.get(index - left.len),
        }
    }
    fn replace(node: &Arc<Self>, index: usize, value: Total) -> Option<Arc<Self>> {
        if index >= node.len {
            return None;
        }
        let Some((left, right)) = &node.children else {
            return Self::build(&[value]);
        };
        let (left, right) = if index < left.len {
            (Self::replace(left, index, value)?, right.clone())
        } else {
            (left.clone(), Self::replace(right, index - left.len, value)?)
        };
        Some(Arc::new(Self {
            total: left.total.add(right.total)?,
            len: node.len,
            children: Some((left, right)),
        }))
    }
}
#[derive(Clone)]
pub(super) struct Epoch {
    program: Arc<TypedProgram>,
    candidate: Arc<CandidatePlan>,
    rows: PersistentVec<Row>,
    readers: PersistentVec<BTreeSet<usize>>,
    total: Arc<Sum>,
    global_cost: usize,
    limits: Limits,
}
fn scalar(ty: &Type) -> bool {
    ty.is_integer() || ty.is_float() || matches!(ty, Type::Unit | Type::Bool)
}
/// Exhaustive supported local input vocabulary. This is only query eligibility;
/// original metadata, CFG, scope and Hold routines still earn each new receipt.
struct LocalInput {
    callees: Vec<FunctionId>,
    span_sensitive: bool,
}
fn closed(function: &TypedFunction) -> Option<LocalInput> {
    if function.call_kind != CallKind::Direct
        || function.storage_life.is_some()
        || function.reference_return_bound.is_some()
        || function.callable_return_contract.is_some()
        || function.callable_contract.is_some()
        || function.protocol_contract.is_some()
        || function.environment.is_some()
        || !scalar(&function.return_type)
        || function
            .foreign
            .as_ref()
            .is_some_and(|f| f.scoped_callback.is_some())
        || !function.locals.iter().all(|l| scalar(&l.ty))
        || !function.values.iter().all(scalar)
    {
        return None;
    }
    let mut callees = BTreeSet::new();
    let mut span_sensitive = function.foreign.is_some();
    for block in &function.blocks {
        for instruction in &block.instructions {
            match &instruction.kind {
                InstructionKind::Constant(value) if scalar(&value.ty()) => {}
                InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. } => {}
                InstructionKind::ConvertInteger { target, .. }
                | InstructionKind::ConvertNumber { target, .. }
                    if scalar(target) => {}
                InstructionKind::Call { function, .. } => {
                    callees.insert(*function);
                }
                InstructionKind::ForeignCall { function, .. } => {
                    callees.insert(*function);
                    span_sensitive = true;
                }
                InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. } => {
                    span_sensitive = true;
                }
                _ => return None,
            }
        }
    }
    Some(LocalInput {
        callees: callees.into_iter().collect(),
        span_sensitive,
    })
}
// Only bounded raw collections may be traversed by eligibility projection.
// A rejected projection returns to the original preflight to choose its error.
fn projection_bounded(function: &TypedFunction, limits: &Limits) -> bool {
    function.blocks.len() <= limits.max_blocks
        && function.locals.len() <= limits.max_locals
        && function.values.len() <= limits.max_values
        && function.parameters.len() <= limits.max_locals
}

fn global(module: &TypedModule, program: &TypedProgram) -> bool {
    module.closures.is_empty()
        && module.resources.is_empty()
        && module.life_declarations.is_empty()
        && module.life_instances.is_empty()
        && module.dyn_declarations.is_empty()
        && module.dyn_interfaces.is_empty()
        && module.dyn_vtables.is_empty()
        && module.unsupported.is_empty()
        && program.task_interfaces.is_empty()
}
fn same_global(a: &TypedProgram, b: &TypedProgram) -> bool {
    global(&b.module, b)
        && a.module.schema_version == b.module.schema_version
        && a.module.module_id == b.module.module_id
        && a.module.target == b.module.target
        && a.module.functions.len() == b.module.functions.len()
}
fn signature_eq(a: &TypedFunction, b: &TypedFunction) -> bool {
    (
        a.id,
        &a.name,
        a.call_kind,
        &a.foreign,
        &a.parameters,
        &a.return_type,
        a.result_mode,
    ) == (
        b.id,
        &b.name,
        b.call_kind,
        &b.foreign,
        &b.parameters,
        &b.return_type,
        b.result_mode,
    ) && a.parameters.iter().all(|id| {
        a.locals.get(id.0 as usize).map(|l| &l.ty) == b.locals.get(id.0 as usize).map(|l| &l.ty)
    })
}
fn fingerprint_reads(module: &TypedModule, callees: &[FunctionId]) -> Fingerprint {
    use std::fmt::Write;
    struct Output(FingerprintBuilder);
    impl std::fmt::Write for Output {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0.update(s.as_bytes());
            Ok(())
        }
    }
    let mut output = Output(FingerprintBuilder::new());
    for id in callees {
        let f = &module.functions[id.0 as usize];
        let parameters: Vec<_> = f
            .parameters
            .iter()
            .map(|p| f.locals.get(p.0 as usize).map(|l| &l.ty))
            .collect();
        write!(
            &mut output,
            "{:?}",
            (
                f.id,
                &f.name,
                f.call_kind,
                &f.foreign,
                &f.parameters,
                parameters,
                &f.return_type,
                f.result_mode
            )
        )
        .expect("digest sink");
    }
    output.0.digest()
}
fn global_dependencies(program: &TypedProgram) -> Fingerprint {
    use std::fmt::Write;
    struct Sink(FingerprintBuilder);
    impl std::fmt::Write for Sink {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.0.update(s.as_bytes());
            Ok(())
        }
    }
    let mut output = Sink(FingerprintBuilder::new());
    output.0.update(b"zil-analyze-global-input-v1");
    output.0.update(&program.module.structural_fingerprint().0);
    write!(&mut output, "{:?}", program.task_interfaces).expect("digest sink");
    output.0.digest()
}
impl Epoch {
    fn new(
        program: &Arc<TypedProgram>,
        candidate: Arc<CandidatePlan>,
        limits: &Limits,
        recording: Recording,
    ) -> Option<Self> {
        if !global(&program.module, program) || program.module.functions.is_empty() {
            return None;
        }
        let mut rows = Vec::new();
        let mut totals = Vec::new();
        let mut readers = vec![BTreeSet::new(); program.module.functions.len()];
        let mut local_cost = 0usize;
        for (index, function) in program.module.functions.iter().enumerate() {
            let local = closed(function)?;
            let callees = local.callees;
            for id in &callees {
                let callee = program.module.functions.get(id.0 as usize)?;
                if callee.id != *id {
                    return None;
                }
                readers[id.0 as usize].insert(index);
            }
            let cost = recording
                .costs
                .get(index)?
                .iter()
                .try_fold(0usize, |n, c| n.checked_add(*c))?;
            local_cost = local_cost.checked_add(cost)?;
            rows.push(Row {
                function: program.module.functions.owner(index)?.clone(),
                callees: callees.clone(),
                output: candidate.functions.item_fingerprint(index)?,
                admission_cost: recording.costs[index][0].checked_add(recording.costs[index][1])?,
                dependencies: fingerprint_reads(&program.module, &callees),
                span_sensitive: local.span_sensitive,
            });
            totals.push(Total::function(function, cost)?);
        }
        Some(Self {
            program: program.clone(),
            candidate,
            rows: rows.into(),
            readers: readers.into(),
            total: Sum::build(&totals)?,
            global_cost: recording.cost.checked_sub(local_cost)?,
            limits: limits.clone(),
        })
    }
    fn update(
        &self,
        program: &Arc<TypedProgram>,
        limits: &Limits,
    ) -> Option<(Self, AnalysisStats, Vec<AnalysisFunctionResult>)> {
        if self.limits != *limits || !same_global(&self.program, program) {
            return None;
        }
        let dirty = program
            .module
            .functions
            .changed_indices(&self.program.module.functions);
        let mut next = self.clone();
        // Check current aggregate inventories before projecting a single body.
        // Persistent totals update only changed leaves. No per-item quota is
        // multiplied by the number of changed items.
        for &index in &dirty {
            let function = program.module.functions.get(index)?;
            if !projection_bounded(function, limits) {
                return None;
            }
            let old = next.total.get(index)?;
            let value = Total {
                blocks: function.blocks.len(),
                locals: function.locals.len(),
                values: function.values.len(),
                ..old
            };
            next.total = Sum::replace(&next.total, index, value)?;
        }
        if !next.total.total.counts_fit(limits) {
            return None;
        }
        for &index in &dirty {
            let old = next.total.get(index)?;
            next.total = Sum::replace(
                &next.total,
                index,
                Total::function(&program.module.functions[index], old.cost)?,
            )?;
        }
        let inventory = next.total.total;
        let projection = program
            .module
            .functions
            .len()
            .checked_add(inventory.blocks)?
            .checked_add(inventory.instructions)?
            .checked_add(inventory.locals)?
            .checked_add(inventory.values)?;
        if !inventory.counts_fit(limits) || projection > limits.max_analysis_steps {
            return None;
        }
        let mut work = BTreeSet::new();
        let mut eligibility = BTreeMap::new();
        let mut relocated = BTreeSet::new();
        for &index in &dirty {
            let prior = self.rows.get(index)?;
            let before = prior.function.as_ref();
            let after = program.module.functions.get(index)?;
            let same = if prior.span_sensitive {
                before == after
            } else {
                candidate_queries::semantic_input_eq(before, after)
            };
            if same {
                relocated.insert(index);
                continue;
            }
            work.insert(index);
            eligibility.insert(index, closed(after)?);
            if !signature_eq(before, after) {
                work.extend(self.readers.get(index)?.iter().copied());
            }
        }
        let old_work = work.iter().try_fold(0usize, |total, index| {
            total.checked_add(self.total.get(*index)?.cost)
        })?;
        let unchanged = self.total.total.cost.checked_sub(old_work)?;
        let mut available = limits
            .max_analysis_steps
            .checked_sub(self.global_cost)?
            .checked_sub(unchanged)?;
        let mut admissions = BTreeMap::new();
        // All changed raw metadata is admitted before any caller may inspect
        // it. A malformed later callee cannot cause an early caller to index an
        // unvalidated parameter. The provisional allowance is shared, bounded,
        // and never supplies a diagnostic: failure uses the original traversal.
        for &index in &work {
            let function = &program.module.functions[index];
            let admission = if eligibility.contains_key(&index) {
                admit(&program.module, index, function, limits, available).ok()?
            } else {
                let prior = self.rows.get(index)?;
                if !Arc::ptr_eq(&prior.function, program.module.functions.owner(index)?)
                    && !relocated.contains(&index)
                {
                    return None;
                }
                if prior.admission_cost > available {
                    return None;
                }
                Admission {
                    budget: Budget::new(limits),
                    cost: prior.admission_cost,
                    package: false,
                }
            };
            available = available.checked_sub(admission.cost)?;
            admissions.insert(index, admission);
        }
        let mut candidate = (*self.candidate).clone();
        let mut changes = Vec::new();
        let mut stats = AnalysisStats::default();
        let mut changed_outputs = 0;
        for &index in &work {
            let function = program.module.functions.get(index)?;
            let prior = self.rows.get(index)?;
            let local = if let Some(local) = eligibility.remove(&index) {
                local
            } else if Arc::ptr_eq(&prior.function, program.module.functions.owner(index)?)
                || relocated.contains(&index)
            {
                LocalInput {
                    callees: prior.callees.clone(),
                    span_sensitive: prior.span_sensitive,
                }
            } else {
                return None;
            };
            let callees = local.callees;
            if callees.iter().any(|id| {
                program
                    .module
                    .functions
                    .get(id.0 as usize)
                    .is_none_or(|f| f.id != *id)
            }) {
                return None;
            }
            // A local failure or insufficient total allowance returns to the
            // original complete traversal; it must emit the original first error.
            let admission = admissions.remove(&index)?;
            let admission_cost = admission.cost;
            let (result, cost, remaining) =
                execute(&program.module, function, limits, admission, available).ok()?;
            available = remaining;
            for id in &prior.callees {
                if !callees.contains(id) {
                    next.readers.get_mut(id.0 as usize)?.remove(&index);
                }
            }
            for id in &callees {
                if !prior.callees.contains(id) {
                    next.readers.get_mut(id.0 as usize)?.insert(index);
                }
            }
            let output = if self.candidate.functions[index] == result {
                prior.output
            } else {
                changed_outputs += 1;
                stats.product_plan_rows_replaced += 1;
                candidate.functions.replace(index, result);
                candidate.functions.item_fingerprint(index)?
            };
            next.total = Sum::replace(&next.total, index, Total::function(function, cost)?)?;
            let dependencies = fingerprint_reads(&program.module, &callees);
            changes.push(AnalysisFunctionResult {
                function: function.id,
                input: program.module.functions.item_fingerprint(index)?,
                dependencies,
                output,
                reused: false,
                function_scoped: true,
            });
            next.rows.replace(
                index,
                Row {
                    function: program.module.functions.owner(index)?.clone(),
                    callees,
                    output,
                    admission_cost,
                    dependencies,
                    span_sensitive: local.span_sensitive,
                },
            );
            stats.executed_functions += usize::from(function.foreign.is_none());
            stats.product_function_queries_executed += 1;
        }
        for &index in &relocated {
            if work.contains(&index) {
                continue;
            }
            let old = self.rows.get(index)?;
            let function = program.module.functions.owner(index)?.clone();
            changes.push(AnalysisFunctionResult {
                function: function.id,
                input: program.module.functions.item_fingerprint(index)?,
                dependencies: old.dependencies,
                output: old.output,
                reused: true,
                function_scoped: true,
            });
            next.rows.replace(
                index,
                Row {
                    function,
                    ..old.clone()
                },
            );
        }
        changes.sort_by_key(|change| change.function);
        if !next.total.total.fits(limits, next.global_cost) {
            return None;
        }
        candidate.module_fingerprint = program.module.structural_fingerprint();
        next.program = program.clone();
        next.candidate = Arc::new(candidate);
        let count = program.module.functions.len();
        let executed = stats.product_function_queries_executed;
        stats.product_semantic_functions_compared = dirty.len();
        stats.product_relocated_function_bindings = relocated.len();
        stats.product_admission_functions_checked = dirty.len() - relocated.len();
        stats.product_function_queries_reused = count - executed;
        stats.product_global_obligations_reused = 1;
        stats.candidate_executed_functions = executed;
        stats.candidate_reused_functions = count - executed;
        stats.equal_function_results = count - changed_outputs;
        stats.changed_function_results = changed_outputs;
        stats.retained_candidate_queries = count;
        Some((next, stats, changes))
    }
}
fn admit(
    module: &TypedModule,
    index: usize,
    function: &TypedFunction,
    limits: &Limits,
    available: usize,
) -> CompileResult<Admission> {
    let mut budget = Budget::new(limits);
    budget.restrict_product_balance(available);
    let before = budget.remaining();
    // The successful closed vocabulary has one scalar node per root; use the
    // same original ledger operation, including the reserved identity traversal.
    let mut roots = 1usize
        .checked_add(function.locals.len())
        .and_then(|n| n.checked_add(function.values.len()))
        .ok_or_else(|| bad("type inventory overflow"))?;
    for block in &function.blocks {
        for instruction in &block.instructions {
            if matches!(
                instruction.kind,
                InstructionKind::ConvertInteger { .. } | InstructionKind::ConvertNumber { .. }
            ) {
                roots = roots
                    .checked_add(1)
                    .ok_or_else(|| bad("type inventory overflow"))?;
            }
        }
    }
    budget.product(roots, 2, 1, "type shape")?;
    let package = budget::preflight_function(
        module,
        index,
        function,
        limits,
        &mut budget,
        &mut budget::AdmissionCounts::default(),
    )?;
    let cost = before - budget.remaining();
    Ok(Admission {
        budget,
        cost,
        package,
    })
}
struct Admission {
    budget: Budget,
    cost: usize,
    package: bool,
}
fn execute(
    module: &TypedModule,
    function: &TypedFunction,
    limits: &Limits,
    admission: Admission,
    available: usize,
) -> CompileResult<(FunctionPlan, usize, usize)> {
    let Admission {
        mut budget,
        cost: admission_cost,
        package,
    } = admission;
    budget.restrict_product_balance(available);
    let before = budget.remaining();
    budget.spend(1, "function analysis")?;
    let (blocks, graph) = if function.foreign.is_some() {
        retained::check_function(module, function, &[], &mut budget)?;
        if !function.blocks.is_empty() || !function.values.is_empty() {
            return Err(bad("foreign declaration carries an executable body"));
        }
        (
            SharedVec::default(),
            (SharedVec::default(), SharedVec::default()),
        )
    } else {
        let Initialization {
            blocks,
            predecessors,
            reachable,
        } = initialize(function, &mut budget)?;
        retained::check_function(module, function, &reachable, &mut budget)?;
        address_sums::check(module, function, &reachable, limits, &mut budget)?;
        mixed_address_sums::check(module, function, &reachable, limits, &mut budget)?;
        (blocks, (predecessors, reachable))
    };
    scopes::check(module, function, package, &mut budget)?;
    budget.spend(
        function
            .locals
            .len()
            .saturating_add(function.values.len())
            .saturating_add(1),
        "callback State type occurrence",
    )?;
    budget.spend(
        function.locals.len().saturating_add(function.values.len()),
        "owning array presence",
    )?;
    let holds = holds::infer_product_function(module, function, &graph, &mut budget)?;
    Ok((
        FunctionPlan {
            function: function.id,
            blocks,
            holds,
        },
        admission_cost + before - budget.remaining(),
        budget.remaining(),
    ))
}
impl AnalysisSession {
    /// Internal structural identity contract. Ordinary raw Analyze continues to
    /// check the complete input and produce the legacy canonical module hash.
    pub fn analyze_product(
        &mut self,
        program: &Arc<TypedProgram>,
        limits: &Limits,
    ) -> CompileResult<Arc<CandidatePlan>> {
        if let Some(complete) = &self.product_complete
            && Arc::ptr_eq(&complete.program, program)
            && complete.limits == *limits
        {
            self.stats = AnalysisStats {
                shared_program_reuses: 1,
                retained_queries: self.retained_initializations(),
                product_function_queries_reused: program.module.functions.len(),
                product_global_obligations_reused: 1,
                candidate_reused_functions: program.module.functions.len(),
                equal_function_results: program.module.functions.len(),
                ..AnalysisStats::default()
            };
            self.product_changes.clear();
            self.function_results = Arc::default();
            return Ok(complete.candidate.clone());
        }
        if let Some((next, stats, changes)) = self
            .product
            .as_ref()
            .and_then(|old| old.update(program, limits))
        {
            let result = next.candidate.clone();
            self.product_complete = Some(Arc::new(Complete {
                program: program.clone(),
                candidate: result.clone(),
                limits: limits.clone(),
            }));
            self.product = Some(next);
            self.stats = stats;
            self.stats.retained_queries = self.retained_initializations();
            self.product_changes = changes;
            // This specialized entry exports a patch; it does not pretend an old
            // complete legacy result list was recomputed for the new raw input.
            self.function_results = Arc::default();
            return Ok(result);
        }
        self.product_capture = None;
        let analyzed = self.analyze_input(&program.module, Some(program), limits, true, true);
        // Failed complete obligations are still actual attempted work. Their
        // diagnostics come from the original checker; no partial epoch commits.
        self.stats.product_global_obligations_executed = 1;
        let candidate = Arc::new(analyzed?);
        let recording = self
            .product_capture
            .take()
            .expect("successful internal recording");
        self.product = Epoch::new(program, candidate.clone(), limits, recording);
        self.product_complete = Some(Arc::new(Complete {
            program: program.clone(),
            candidate: candidate.clone(),
            limits: limits.clone(),
        }));
        self.stats.product_admission_functions_checked = program.module.functions.len();
        self.stats.product_function_queries_executed = program.module.functions.len();
        self.stats.product_global_obligations_executed = 1;
        let global_dependencies = global_dependencies(program);
        self.product_changes = candidate
            .functions
            .iter()
            .enumerate()
            .map(|(index, _result)| {
                let function = &program.module.functions[index];
                let row = self
                    .product
                    .as_ref()
                    .and_then(|epoch| epoch.rows.get(index));
                AnalysisFunctionResult {
                    function: function.id,
                    input: program
                        .module
                        .functions
                        .item_fingerprint(index)
                        .expect("admitted current function"),
                    dependencies: row.map_or(global_dependencies, |row| row.dependencies),
                    output: row.map_or_else(
                        || {
                            candidate
                                .functions
                                .item_fingerprint(index)
                                .expect("complete candidate row")
                        },
                        |row| row.output,
                    ),
                    reused: false,
                    function_scoped: row.is_some(),
                }
            })
            .collect();
        Ok(candidate)
    }
    /// Only actual executed item results for the last successful product epoch.
    /// A missing ID means its old immutable row remains current, not new Verify authority.
    pub fn last_product_changes(&self) -> &[AnalysisFunctionResult] {
        &self.product_changes
    }
}

#[cfg(test)]
#[path = "product_tests.rs"]
mod tests;
