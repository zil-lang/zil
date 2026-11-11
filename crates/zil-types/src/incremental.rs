//! Function-scoped raw CFG work products. Nothing here carries Verify authority.
use super::*;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Instant;
use zil_base::{FileId, Fingerprint, FingerprintBuilder, ModuleId};
use zil_db::reactive::{GraphError, ReactiveGraph, ReactiveStats};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FunctionStats {
    pub cost_profiled: bool,
    pub total_ns: u64,
    pub shape_resolution_ns: u64,
    pub context_construction_ns: u64,
    pub projection_context_ns: u64,
    pub core_validation_ns: u64,
    pub inventory_ns: u64,
    pub graph_maintenance_ns: u64,
    /// Graph evaluator time excluding graph reads; body_key_ns is a subset.
    pub evaluator_ns: u64,
    pub body_key_ns: u64,
    pub cfg_reuse_ns: u64,
    pub cfg_execute_ns: u64,
    pub cfg_store_ns: u64,
    pub producer_version_reuses: usize,
    pub inventory_functions_visited: usize,
    pub reused_functions: usize,
    pub executed_functions: usize,
    pub body_key_executions: usize,
    pub body_key_reuses: usize,
    pub core_function_eq_comparisons: usize,
    pub body_key_visited_nodes: usize,
    pub graph_edges: usize,
    pub graph: ReactiveStats,
    pub context_modules_visited: usize,
    pub context_declarations_hashed: usize,
    pub context_imports_hashed: usize,
}

impl FunctionStats {
    pub fn to_json(&self) -> String {
        let graph = self.graph;
        let timings = format!(
            "\"cost_profiled\":{},\"total_ns\":{},\"shape_resolution_ns\":{},\"context_construction_ns\":{},\"projection_context_ns\":{},\"core_validation_ns\":{},\"inventory_ns\":{},\"graph_maintenance_ns\":{},\"evaluator_ns\":{},\"body_key_ns\":{},\"cfg_reuse_ns\":{},\"cfg_execute_ns\":{},\"cfg_store_ns\":{},\"producer_version_reuses\":{},\"inventory_functions_visited\":{}",
            self.cost_profiled,
            self.total_ns,
            self.shape_resolution_ns,
            self.context_construction_ns,
            self.projection_context_ns,
            self.core_validation_ns,
            self.inventory_ns,
            self.graph_maintenance_ns,
            self.evaluator_ns,
            self.body_key_ns,
            self.cfg_reuse_ns,
            self.cfg_execute_ns,
            self.cfg_store_ns,
            self.producer_version_reuses,
            self.inventory_functions_visited,
        );
        format!(
            "{{{},\"reused_functions\":{},\"executed_functions\":{},\"body_key_executions\":{},\"body_key_reuses\":{},\"core_function_eq_comparisons\":{},\"body_key_visited_nodes\":{},\"context_modules_visited\":{},\"context_declarations_hashed\":{},\"context_imports_hashed\":{},\"graph_edges\":{},\"graph\":{{\"executed\":{},\"edge_reads\":{},\"notifications\":{},\"enqueues\":{},\"deduplicated\":{},\"reused_bonds\":{},\"new_bonds\":{},\"removed_bonds\":{},\"rank_visits\":{},\"input_updates\":{},\"retained_nodes\":{},\"retained_edges\":{},\"inventory_visits\":{},\"equal_results\":{},\"changed_results\":{},\"cycle_check_edges\":{},\"validation_edge_reads\":{},\"rank_edge_reads\":{}}}}}",
            timings,
            self.reused_functions,
            self.executed_functions,
            self.body_key_executions,
            self.body_key_reuses,
            self.core_function_eq_comparisons,
            self.body_key_visited_nodes,
            self.context_modules_visited,
            self.context_declarations_hashed,
            self.context_imports_hashed,
            self.graph_edges,
            graph.executed,
            graph.edge_reads,
            graph.notifications,
            graph.enqueues,
            graph.deduplicated,
            graph.reused_bonds,
            graph.new_bonds,
            graph.removed_bonds,
            graph.rank_visits,
            graph.input_updates,
            graph.retained_nodes,
            graph.retained_edges,
            graph.inventory_visits,
            graph.equal_results,
            graph.changed_results,
            graph.cycle_check_edges,
            graph.validation_edge_reads,
            graph.rank_edge_reads,
        )
    }
}

/// Semantic identity is supplied separately from compact relocation IDs and
/// from the evidence that makes a particular projection valid.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FunctionOwner {
    project: String,
    module: String,
    function: String,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ProjectionOwner {
    Context(String),
    Signature(FunctionOwner),
    RawBody(FunctionOwner),
    BodyKey(FunctionOwner),
}
#[derive(Clone)]
struct SourceBinding {
    module: ModuleId,
    index: usize,
    version: Fingerprint,
    producer: Option<zil_expand::ModuleRevision>,
}
#[derive(Clone, Default)]
struct Projections {
    graph: ReactiveGraph<ProjectionOwner, Option<Fingerprint>>,
    // One immutable prior program, shared with the driver's prepared snapshot;
    // no extra deep CoreFunction clone is made for each evidence comparison.
    source: Option<PreparedProgram>,
    bindings: BTreeMap<FunctionOwner, SourceBinding>,
    owners: BTreeSet<ProjectionOwner>,
    version: u64,
}
struct ReactiveRequest<'a> {
    state: &'a mut Projections,
    owners: &'a BTreeMap<ModuleId, (String, String)>,
    prepared: &'a PreparedProgram,
}

#[derive(Clone)]
struct MemoEntry {
    revision: zil_expand::ModuleRevision,
    context: Fingerprint,
    signature: Fingerprint,
    reads: BTreeMap<(ModuleId, String), Fingerprint>,
    key: Option<Fingerprint>,
}
#[derive(Clone, Default)]
struct ProducerMemo {
    entries: BTreeMap<FunctionOwner, MemoEntry>,
}
struct ProducerRequest<'a> {
    state: &'a mut ProducerMemo,
    owners: &'a BTreeMap<ModuleId, (String, String)>,
    prepared: &'a PreparedProgram,
}

fn start_cost(profile: bool) -> Option<Instant> {
    profile.then(Instant::now)
}
fn elapsed_cost(start: Option<Instant>) -> u64 {
    start.map_or(0, |start| {
        start.elapsed().as_nanos().min(u64::MAX as u128) as u64
    })
}

#[derive(Clone)]
struct Entry {
    // All spans are relative to the source function; IDs remain explicitly bound.
    function: TypedFunction,
    cost: [usize; 6],
}

/// Holds only the most recent successful scalar Types epoch, not an unbounded
/// edit history. Complete declaration contexts and current checks stay fresh.
#[derive(Clone, Default)]
pub struct FunctionSession {
    entries: Arc<BTreeMap<Fingerprint, Arc<Entry>>>,
    stats: FunctionStats,
    projections: Projections,
    producer_memo: ProducerMemo,
    cost_profiling: bool,
}
impl FunctionSession {
    pub fn set_cost_profiling(&mut self, enabled: bool) {
        self.cost_profiling = enabled;
    }
    pub fn clear(&mut self) {
        let cost_profiling = self.cost_profiling;
        *self = Self::default();
        self.cost_profiling = cost_profiling;
    }
    /// Transaction rollback needs the prior raw content epoch, not a deep copy
    /// of the acceleration graph. Rebuilding that graph after failure is safe.
    pub fn checkpoint(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            stats: self.stats,
            projections: Projections::default(),
            producer_memo: ProducerMemo::default(),
            cost_profiling: self.cost_profiling,
        }
    }
    pub fn last_stats(&self) -> FunctionStats {
        self.stats
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn check_prepared_program(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<TypedProgram> {
        self.check_prepared_program_inner(prepared, target, limits, None, false)
    }

    /// Opt-in semantic owners. These strings only identify nodes: the current
    /// immutable Core, complete binding context, signature reads and limits
    /// independently determine whether their previous results remain valid.
    pub fn check_prepared_program_with_owners(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
        owners: &BTreeMap<ModuleId, (String, String)>,
    ) -> CompileResult<TypedProgram> {
        self.check_prepared_program_inner(prepared, target, limits, Some(owners), true)
    }

    /// Simple owner/version memoization uses the same producer evidence and
    /// observed signatures as the graph path, without graph maintenance.
    pub fn check_prepared_program_with_producer_owners(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
        owners: &BTreeMap<ModuleId, (String, String)>,
    ) -> CompileResult<TypedProgram> {
        self.check_prepared_program_inner(prepared, target, limits, Some(owners), false)
    }

    fn check_prepared_program_inner(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
        owners: Option<&BTreeMap<ModuleId, (String, String)>>,
        graph: bool,
    ) -> CompileResult<TypedProgram> {
        self.stats = FunctionStats {
            cost_profiled: self.cost_profiling,
            ..FunctionStats::default()
        };
        let total_started = start_cost(self.cost_profiling);
        let shape_started = start_cost(self.cost_profiling);
        // Preserve the existing inference/shape path, including all raw limits.
        let inferred;
        let resolved;
        let graph_eligible;
        let program = if prepared.identity_inference(limits) {
            resolved = match aggregates::resolve_prepared_program(prepared.program(), limits) {
                Ok(resolved) => resolved,
                Err(error) => {
                    self.projections = Projections::default();
                    self.producer_memo = ProducerMemo::default();
                    self.stats.shape_resolution_ns = elapsed_cost(shape_started);
                    self.stats.total_ns = elapsed_cost(total_started);
                    return Err(error);
                }
            };
            graph_eligible = matches!(resolved, std::borrow::Cow::Borrowed(_));
            &*resolved
        } else {
            graph_eligible = false;
            inferred = match program::resolve_program_shapes(prepared.program(), limits) {
                Ok(inferred) => inferred,
                Err(error) => {
                    self.projections = Projections::default();
                    self.producer_memo = ProducerMemo::default();
                    self.stats.shape_resolution_ns = elapsed_cost(shape_started);
                    self.stats.total_ns = elapsed_cost(total_started);
                    return Err(error);
                }
            };
            &inferred
        };
        let shape_resolution_ns = elapsed_cost(shape_started);
        let owners = owners.filter(|_| graph_eligible);
        if owners.is_none() || !graph {
            self.projections = Projections::default();
        }
        if owners.is_none() || graph {
            self.producer_memo = ProducerMemo::default();
        }
        let mut next = BTreeMap::new();
        let mut stats = FunctionStats {
            cost_profiled: self.cost_profiling,
            shape_resolution_ns,
            ..FunctionStats::default()
        };
        let mut reuse = Reuse {
            prior: &self.entries,
            next: &mut next,
            stats: &mut stats,
            keys: None,
            reactive: owners.filter(|_| graph).map(|owners| ReactiveRequest {
                state: &mut self.projections,
                owners,
                prepared,
            }),
            producer: owners.filter(|_| !graph).map(|owners| ProducerRequest {
                state: &mut self.producer_memo,
                owners,
                prepared,
            }),
        };
        let result =
            program::check_resolved_program_with_reuse(program, target, limits, Some(&mut reuse));
        if result.is_ok() {
            self.entries = Arc::new(next);
            if owners.is_some() && graph {
                self.projections.source = Some(prepared.clone());
            }
        } else {
            // A failed request cannot publish a partial graph epoch. The last
            // successful raw CFGs remain available through their full keys.
            self.projections = Projections::default();
            self.producer_memo = ProducerMemo::default();
        }
        stats.total_ns = elapsed_cost(total_started);
        self.stats = stats;
        result
    }

    /// Optional transport of raw, explicitly trusted local work products. The
    /// caller must bind the exact compiler and protect the artifact destination.
    pub fn encode_work_products(&self, target: &TargetProfile, limits: &Limits) -> Option<Vec<u8>> {
        if self.entries.is_empty() || self.entries.len() > limits.max_functions {
            return None;
        }
        let mut bytes = b"ZILFUN01".to_vec();
        bytes.extend_from_slice(&u32::try_from(self.entries.len()).ok()?.to_le_bytes());
        let mut functions = Vec::new();
        for (key, entry) in self.entries.iter() {
            bytes.extend_from_slice(&key.0);
            for cost in entry.cost {
                bytes.extend_from_slice(&u64::try_from(cost).ok()?.to_le_bytes());
            }
            if bytes.len() > limits.max_source_bytes {
                return None;
            }
            functions.push(entry.function.clone());
        }
        let module = TypedModule {
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: target.clone(),
            functions,
            closures: vec![],
            resources: vec![],
            unsupported: vec![],
        };
        let candidate = CandidatePlan {
            schema_version: SCHEMA_VERSION,
            module_fingerprint: Fingerprint::ZERO,
            target_fingerprint: Fingerprint::ZERO,
            functions: vec![],
            origins: vec![],
            responsibilities: vec![],
            unsupported: vec![],
        };
        let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, limits).ok()?;
        if bytes.len().checked_add(wire.len())? > limits.max_source_bytes {
            return None;
        }
        bytes.extend_from_slice(&wire);
        Some(bytes)
    }

    pub fn restore_work_products(
        &mut self,
        bytes: &[u8],
        target: &TargetProfile,
        limits: &Limits,
    ) -> bool {
        let Some(entries) = decode(bytes, target, limits) else {
            return false;
        };
        self.entries = Arc::new(entries);
        self.projections = Projections::default();
        self.stats = FunctionStats::default();
        true
    }
}

fn decode(
    bytes: &[u8],
    target: &TargetProfile,
    limits: &Limits,
) -> Option<BTreeMap<Fingerprint, Arc<Entry>>> {
    if bytes.len() > limits.max_source_bytes || bytes.get(..8)? != b"ZILFUN01" {
        return None;
    }
    let count = u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?) as usize;
    if count > limits.max_functions {
        return None;
    }
    let boundary = 12usize.checked_add(count.checked_mul(80)?)?;
    let metadata = bytes.get(12..boundary)?;
    let (module, candidate) =
        zil_ir::wire::decode_module_candidate(bytes.get(boundary..)?, limits).ok()?;
    if module.target != *target
        || module.functions.len() != count
        || !module.closures.is_empty()
        || !module.life_declarations.is_empty()
        || !module.life_instances.is_empty()
        || !module.dyn_declarations.is_empty()
        || !module.dyn_interfaces.is_empty()
        || !module.dyn_vtables.is_empty()
        || !module.resources.is_empty()
        || !module.unsupported.is_empty()
        || !candidate.functions.is_empty()
        || !candidate.origins.is_empty()
        || !candidate.responsibilities.is_empty()
        || !candidate.unsupported.is_empty()
    {
        return None;
    }
    let maxima = [
        limits.max_blocks,
        limits.max_locals,
        limits.max_values,
        limits.max_instructions,
        limits.max_analysis_steps,
        limits.max_nodes,
    ];
    let mut entries = BTreeMap::new();
    for (record, function) in metadata.as_chunks::<80>().0.iter().zip(module.functions) {
        if !scalar_typed(&function) || function.span.file != FileId(0) || function.span.start != 0 {
            return None;
        }
        let key = Fingerprint(record[..32].try_into().ok()?);
        let mut cost = [0; 6];
        for (i, chunk) in record[32..].as_chunks::<8>().0.iter().enumerate() {
            cost[i] = usize::try_from(u64::from_le_bytes(*chunk)).ok()?;
            if cost[i] > maxima[i] {
                return None;
            }
        }
        if entries
            .insert(key, Arc::new(Entry { function, cost }))
            .is_some()
        {
            return None;
        }
    }
    Some(entries)
}

pub(super) struct Reuse<'a> {
    prior: &'a BTreeMap<Fingerprint, Arc<Entry>>,
    next: &'a mut BTreeMap<Fingerprint, Arc<Entry>>,
    stats: &'a mut FunctionStats,
    keys: Option<BTreeMap<(ModuleId, String), Fingerprint>>,
    reactive: Option<ReactiveRequest<'a>>,
    producer: Option<ProducerRequest<'a>>,
}
impl Reuse<'_> {
    pub(super) fn start_cost(&self) -> Option<Instant> {
        start_cost(self.stats.cost_profiled)
    }
    pub(super) fn context_constructed(&mut self, started: Option<Instant>) {
        self.stats.context_construction_ns += elapsed_cost(started);
    }
    pub(super) fn cfg_executed(&mut self, started: Option<Instant>) {
        self.stats.cfg_execute_ns += elapsed_cost(started);
    }
    pub(super) fn prepare(
        &mut self,
        program: &CoreProgram,
        tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
        target: &TargetProfile,
        limits: &Limits,
    ) {
        if let Some(request) = &mut self.producer {
            self.keys = request.keys(program, tables, target, limits, self.stats);
            return;
        }
        self.keys = match &mut self.reactive {
            Some(request) => request.keys(program, tables, target, limits, self.stats),
            None => keys(program, tables, target, limits, self.stats),
        };
    }
    pub(super) fn get(
        &mut self,
        module: ModuleId,
        function: &CoreFunction,
        state: &mut closures::State,
        limits: &Limits,
    ) -> R<Option<TypedFunction>> {
        let started = self.start_cost();
        let result = self.get_inner(module, function, state, limits);
        self.stats.cfg_reuse_ns += elapsed_cost(started);
        result
    }
    fn get_inner(
        &mut self,
        module: ModuleId,
        function: &CoreFunction,
        state: &mut closures::State,
        limits: &Limits,
    ) -> R<Option<TypedFunction>> {
        // Array billing is request state beyond the legacy scalar cost row.
        // Scalar-only cached CFGs cannot create an array; once an actual array
        // has appeared, rebuild instead of replaying a different billing mode.
        if state.array_billing {
            return Ok(None);
        }
        let Some(key) = self
            .keys
            .as_ref()
            .and_then(|keys| keys.get(&(module, function.name.clone())))
        else {
            return Ok(None);
        };
        let Some(entry) = self.prior.get(key) else {
            return Ok(None);
        };
        if function
            .call_kind
            .is_some_and(|selection| selection != entry.function.call_kind)
        {
            return Ok(None);
        }
        // A matching cache key is not declaration validation. Recheck the
        // current Core life header before returning any prior Typed body.
        let before = state.accounting();
        let bound = lifetimes::compile(function, state, limits)?;
        let after = state.accounting();
        if entry.function.reference_return_bound != bound {
            return Ok(None);
        }
        let mut replay = entry.cost;
        for i in 0..replay.len() {
            let Some(delta) = after[i].checked_sub(before[i]) else {
                return Ok(None);
            };
            let Some(left) = replay[i].checked_sub(delta) else {
                return Ok(None);
            };
            replay[i] = left;
        }
        let mut typed = entry.function.clone();
        if !rebase(
            &mut typed,
            Span::new(
                FileId(0),
                0,
                match function.span.end.checked_sub(function.span.start) {
                    Some(n) => n,
                    None => return Ok(None),
                },
            ),
            function.span,
        ) || !state.replay_accounting(replay)
        {
            return Ok(None);
        }
        self.next.insert(*key, entry.clone());
        self.stats.reused_functions += 1;
        Ok(Some(typed))
    }
    pub(super) fn save(
        &mut self,
        module: ModuleId,
        source: &CoreFunction,
        function: &TypedFunction,
        before: [usize; 6],
        after: [usize; 6],
        array_billing: bool,
    ) {
        let started = self.start_cost();
        self.save_inner(module, source, function, before, after, array_billing);
        self.stats.cfg_store_ns += elapsed_cost(started);
    }
    fn save_inner(
        &mut self,
        module: ModuleId,
        source: &CoreFunction,
        function: &TypedFunction,
        before: [usize; 6],
        after: [usize; 6],
        array_billing: bool,
    ) {
        self.stats.executed_functions += 1;
        if array_billing {
            return;
        }
        let Some(key) = self
            .keys
            .as_ref()
            .and_then(|keys| keys.get(&(module, source.name.clone())))
        else {
            return;
        };
        if !scalar_typed(function) {
            return;
        }
        let mut function = function.clone();
        let Some(length) = source.span.end.checked_sub(source.span.start) else {
            return;
        };
        if !rebase(&mut function, source.span, Span::new(FileId(0), 0, length)) {
            return;
        }
        let mut cost = [0; 6];
        for i in 0..6 {
            let Some(delta) = after[i].checked_sub(before[i]) else {
                return;
            };
            cost[i] = delta;
        }
        self.next.insert(*key, Arc::new(Entry { function, cost }));
    }
}

impl ProducerRequest<'_> {
    fn keys(
        &mut self,
        program: &CoreProgram,
        tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
        target: &TargetProfile,
        limits: &Limits,
        stats: &mut FunctionStats,
    ) -> Option<BTreeMap<(ModuleId, String), Fingerprint>> {
        let context = key_context(program, tables, target, limits, stats)?;
        let inventory_started = start_cost(stats.cost_profiled);
        let mut excluded_ns = 0u64;
        let mut identities = BTreeSet::new();
        let mut project = None;
        for module in &program.modules {
            let identity = self.owners.get(&module.core.module_id)?;
            if !identities.insert(identity.clone())
                || project.as_ref().is_some_and(|prior| prior != &identity.0)
            {
                self.state.entries.clear();
                return keys(program, tables, target, limits, stats);
            }
            project = Some(identity.0.clone());
        }
        let mut next = BTreeMap::new();
        let mut result = BTreeMap::new();
        for module in &program.modules {
            for function in &module.core.functions {
                stats.inventory_functions_visited += 1;
                if !worthwhile(function) {
                    continue;
                }
                let (project, path) = self.owners.get(&module.core.module_id)?;
                let owner = FunctionOwner {
                    project: project.clone(),
                    module: path.clone(),
                    function: function.name.clone(),
                };
                let revision = self.prepared.module_revision(module.core.module_id);
                let signature = *context
                    .declarations
                    .get(&module.core.module_id)?
                    .get(&function.name)?;
                let previous = self.state.entries.get(&owner).filter(|entry| {
                    revision == Some(&entry.revision)
                        && entry.context == context.fingerprint
                        && entry.signature == signature
                        && entry.reads.iter().all(|((module, name), previous)| {
                            context
                                .declarations
                                .get(module)
                                .and_then(|row| row.get(name))
                                == Some(previous)
                        })
                });
                let entry = if let Some(entry) = previous {
                    stats.producer_version_reuses += 1;
                    stats.body_key_reuses += 1;
                    entry.clone()
                } else {
                    let mut reads = BTreeMap::new();
                    let mut observe = |module, name: &str| {
                        let value = *context.declarations.get(&module)?.get(name)?;
                        reads.insert((module, name.to_owned()), value);
                        Some(())
                    };
                    let before = stats.body_key_ns;
                    let key = function_key(
                        module,
                        function,
                        tables,
                        &context,
                        limits,
                        stats,
                        Some(&mut observe),
                    );
                    excluded_ns += stats.body_key_ns - before;
                    if let Some(key) = key {
                        result.insert((module.core.module_id, function.name.clone()), key);
                    }
                    let Some(revision) = revision else { continue };
                    MemoEntry {
                        revision: revision.clone(),
                        context: context.fingerprint,
                        signature,
                        reads,
                        key,
                    }
                };
                if let Some(key) = entry.key {
                    result.insert((module.core.module_id, function.name.clone()), key);
                }
                next.insert(owner, entry);
            }
        }
        stats.inventory_ns += elapsed_cost(inventory_started).saturating_sub(excluded_ns);
        self.state.entries = next;
        Some(result)
    }
}

impl ReactiveRequest<'_> {
    fn keys(
        &mut self,
        program: &CoreProgram,
        tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
        target: &TargetProfile,
        limits: &Limits,
        stats: &mut FunctionStats,
    ) -> Option<BTreeMap<(ModuleId, String), Fingerprint>> {
        match self.try_keys(program, tables, target, limits, stats) {
            Ok(keys) => keys,
            Err(_) => {
                stats.graph = self.state.graph.stats();
                stats.graph_edges = stats.graph.edge_reads as usize;
                // Graph metadata never changes language acceptance. An uncertain
                // identity, removed owner or graph failure takes the old path.
                *self.state = Projections::default();
                keys(program, tables, target, limits, stats)
            }
        }
    }

    fn try_keys(
        &mut self,
        program: &CoreProgram,
        tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
        target: &TargetProfile,
        limits: &Limits,
        stats: &mut FunctionStats,
    ) -> Result<Option<BTreeMap<(ModuleId, String), Fingerprint>>, GraphError> {
        let Some(context) = key_context(program, tables, target, limits, stats) else {
            *self.state = Projections::default();
            return Ok(None);
        };
        let inventory_started = start_cost(stats.cost_profiled);
        let mut validation_ns = 0;
        let mut identities = BTreeSet::new();
        let mut project = None;
        for module in &program.modules {
            let identity = self
                .owners
                .get(&module.core.module_id)
                .ok_or(GraphError::MissingOwner)?;
            if !identities.insert(identity.clone())
                || project.as_ref().is_some_and(|prior| prior != &identity.0)
            {
                return Err(GraphError::InputQueryConflict);
            }
            project = Some(identity.0.clone());
        }
        let project = project.ok_or(GraphError::MissingOwner)?;
        let owner_for = |module: ModuleId, name: &str| -> Result<FunctionOwner, GraphError> {
            let (project, module) = self.owners.get(&module).ok_or(GraphError::MissingOwner)?;
            Ok(FunctionOwner {
                project: project.clone(),
                module: module.clone(),
                function: name.to_owned(),
            })
        };
        let context_owner = ProjectionOwner::Context(project);
        let mut inputs = Vec::new();
        if self.state.graph.get_current(&context_owner) != Some(&Some(context.fingerprint)) {
            inputs.push((context_owner.clone(), Some(context.fingerprint)));
        }
        let mut live = BTreeSet::from([context_owner.clone()]);
        let mut bindings = BTreeMap::new();
        let mut metadata = BTreeMap::new();
        let prior_modules: BTreeMap<_, _> = self
            .state
            .source
            .as_ref()
            .map(|source| {
                source
                    .program()
                    .modules
                    .iter()
                    .map(|module| (module.core.module_id, module))
                    .collect()
            })
            .unwrap_or_default();
        for module in &program.modules {
            for (index, function) in module.core.functions.iter().enumerate() {
                stats.inventory_functions_visited += 1;
                let owner = owner_for(module.core.module_id, &function.name)?;
                let signature_owner = ProjectionOwner::Signature(owner.clone());
                let signature = *context
                    .declarations
                    .get(&module.core.module_id)
                    .and_then(|declarations| declarations.get(&function.name))
                    .ok_or(GraphError::MissingOwner)?;
                if self.state.graph.get_current(&signature_owner) != Some(&Some(signature)) {
                    inputs.push((signature_owner.clone(), Some(signature)));
                }
                live.insert(signature_owner);
                if !worthwhile(function) {
                    continue;
                }
                let producer = self.prepared.module_revision(module.core.module_id);
                let prior = self.state.bindings.get(&owner).and_then(|binding| {
                    if producer.is_some() && producer == binding.producer.as_ref() {
                        stats.producer_version_reuses += 1;
                        return Some(binding.version);
                    }
                    // A new producer revision proves that this module needs a
                    // new body projection; do not walk its old Core for equality.
                    if producer.is_some() {
                        return None;
                    }
                    let previous = prior_modules
                        .get(&binding.module)?
                        .core
                        .functions
                        .get(binding.index)?;
                    let started = start_cost(stats.cost_profiled);
                    stats.core_function_eq_comparisons += 1;
                    let same = previous == function;
                    validation_ns += elapsed_cost(started);
                    same.then_some(binding.version)
                });
                let version = match prior {
                    Some(version) => version,
                    None => {
                        self.state.version =
                            self.state.version.checked_add(1).ok_or_else(|| {
                                GraphError::Evaluation("body input version exhausted".into())
                            })?;
                        Fingerprint::of(&self.state.version.to_le_bytes())
                    }
                };
                let body_owner = ProjectionOwner::RawBody(owner.clone());
                if prior != Some(version) {
                    inputs.push((body_owner.clone(), Some(version)));
                }
                live.insert(body_owner);
                live.insert(ProjectionOwner::BodyKey(owner.clone()));
                bindings.insert(
                    owner.clone(),
                    SourceBinding {
                        module: module.core.module_id,
                        index,
                        version,
                        producer: producer.cloned(),
                    },
                );
                metadata.insert(owner, (module, function));
            }
        }
        stats.core_validation_ns += validation_ns;
        stats.inventory_ns += elapsed_cost(inventory_started).saturating_sub(validation_ns);
        let graph_started = start_cost(stats.cost_profiled);
        self.state.graph.take_stats();
        // Install the entire request's inputs before publishing any notification.
        self.state.graph.set_inputs(inputs)?;
        for owner in metadata.keys() {
            let query = ProjectionOwner::BodyKey(owner.clone());
            if !self.state.owners.contains(&query) {
                self.state.graph.ensure_query(query)?;
            }
        }
        if live != self.state.owners {
            self.state.graph.retain(&live)?;
        }
        let executed = RefCell::new(FunctionStats {
            cost_profiled: stats.cost_profiled,
            ..FunctionStats::default()
        });
        let evaluator_ns = RefCell::new(0u64);
        let graph_read_ns = RefCell::new(0u64);
        let evaluate =
            |owner: &ProjectionOwner,
             read: &mut dyn FnMut(&ProjectionOwner) -> Result<Option<Fingerprint>, GraphError>|
             -> Result<Option<Fingerprint>, GraphError> {
                let started = start_cost(stats.cost_profiled);
                let mut metered_read = |key: &ProjectionOwner| {
                    let started = start_cost(stats.cost_profiled);
                    let result = read(key);
                    *graph_read_ns.borrow_mut() += elapsed_cost(started);
                    result
                };
                let result = (|| {
                    let ProjectionOwner::BodyKey(owner) = owner else {
                        return Err(GraphError::InputQueryConflict);
                    };
                    let (module, function) = metadata.get(owner).ok_or(GraphError::MissingOwner)?;
                    if metered_read(&context_owner)? != Some(context.fingerprint)
                        || metered_read(&ProjectionOwner::RawBody(owner.clone()))?
                            != Some(bindings[owner].version)
                        || metered_read(&ProjectionOwner::Signature(owner.clone()))?
                            != context
                                .declarations
                                .get(&module.core.module_id)
                                .and_then(|row| row.get(&function.name))
                                .copied()
                    {
                        return Err(GraphError::Evaluation(
                            "projection input differs from current Core".into(),
                        ));
                    }
                    let mut read_error = None;
                    let mut observe = |module, name: &str| -> Option<()> {
                        let result = owner_for(module, name)
                            .and_then(|owner| metered_read(&ProjectionOwner::Signature(owner)));
                        match result {
                            Ok(Some(observed))
                                if Some(&observed)
                                    == context
                                        .declarations
                                        .get(&module)
                                        .and_then(|row| row.get(name)) =>
                            {
                                Some(())
                            }
                            Ok(_) => {
                                read_error = Some(GraphError::Evaluation(
                                    "signature differs from current declaration".into(),
                                ));
                                None
                            }
                            Err(error) => {
                                read_error = Some(error);
                                None
                            }
                        }
                    };
                    let body_reads_before = *graph_read_ns.borrow();
                    let key = function_key(
                        module,
                        function,
                        tables,
                        &context,
                        limits,
                        &mut executed.borrow_mut(),
                        Some(&mut observe),
                    );
                    let body_reads = *graph_read_ns.borrow() - body_reads_before;
                    let mut work = executed.borrow_mut();
                    work.body_key_ns = work.body_key_ns.saturating_sub(body_reads);
                    drop(work);
                    if let Some(error) = read_error {
                        return Err(error);
                    }
                    Ok(key)
                })();
                *evaluator_ns.borrow_mut() += elapsed_cost(started);
                result
            };
        let flushed = self.state.graph.flush(&evaluate);
        let evaluator_ns = evaluator_ns.into_inner();
        let graph_read_ns = graph_read_ns.into_inner();
        let executed = executed.into_inner();
        stats.evaluator_ns += evaluator_ns.saturating_sub(graph_read_ns);
        stats.body_key_ns += executed.body_key_ns;
        stats.body_key_executions += executed.body_key_executions;
        stats.body_key_visited_nodes += executed.body_key_visited_nodes;
        if flushed.is_ok() {
            stats.body_key_reuses += metadata.len().saturating_sub(executed.body_key_executions);
        }
        stats.graph = self.state.graph.stats();
        stats.graph_edges = stats.graph.edge_reads as usize;
        if flushed.is_err() {
            stats.graph_maintenance_ns +=
                elapsed_cost(graph_started).saturating_sub(evaluator_ns) + graph_read_ns;
        }
        flushed?;
        let mut keys = BTreeMap::new();
        for (owner, (module, function)) in metadata {
            if let Some(Some(key)) = self
                .state
                .graph
                .get_current(&ProjectionOwner::BodyKey(owner))
            {
                keys.insert((module.core.module_id, function.name.clone()), *key);
            }
        }
        self.state.bindings = bindings;
        self.state.owners = live;
        stats.graph_maintenance_ns +=
            elapsed_cost(graph_started).saturating_sub(evaluator_ns) + graph_read_ns;
        Ok(Some(keys))
    }
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64
    )
}
fn scalar_typed(f: &TypedFunction) -> bool {
    f.storage_life.is_none()
        && f.foreign.is_none()
        && f.environment.is_none()
        && f.callable_contract.is_none()
        && f.callable_return_contract.is_none()
        && f.reference_return_bound.is_none()
        && scalar(&f.return_type)
        && f.locals.iter().all(|l| scalar(&l.ty))
        && f.values.iter().all(scalar)
}
fn rebase(f: &mut TypedFunction, old: Span, new: Span) -> bool {
    let convert = |span: &mut Span| -> Option<()> {
        if span.file != old.file
            || span.start < old.start
            || span.end > old.end
            || span.start > span.end
        {
            return None;
        }
        *span = Span::new(
            new.file,
            new.start.checked_add(span.start - old.start)?,
            new.start.checked_add(span.end - old.start)?,
        );
        if span.end > new.end {
            return None;
        }
        Some(())
    };
    convert(&mut f.span).is_some()
        && f.locals.iter_mut().all(|l| convert(&mut l.span).is_some())
        && f.blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .all(|i| convert(&mut i.span).is_some())
}

// Hashing, rebasing and transporting a CFG can cost more than rebuilding a
// tiny scalar leaf. This cheap source-shape gate is a scheduling choice only:
// excluded functions retain the full original builder and budget behavior.
fn worthwhile(function: &CoreFunction) -> bool {
    function.foreign.is_none()
        && (function.body.statements.len() >= 4
            || function.body.statements.iter().any(|statement| {
                matches!(
                    statement.kind,
                    CoreStatementKind::If { .. }
                        | CoreStatementKind::While { .. }
                        | CoreStatementKind::ForRange { .. }
                )
            }))
}

fn keys(
    program: &CoreProgram,
    tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    target: &TargetProfile,
    limits: &Limits,
    stats: &mut FunctionStats,
) -> Option<BTreeMap<(ModuleId, String), Fingerprint>> {
    let context = key_context(program, tables, target, limits, stats)?;
    let started = start_cost(stats.cost_profiled);
    let body_key_before = stats.body_key_ns;
    let mut keys = BTreeMap::new();
    for module in &program.modules {
        for function in &module.core.functions {
            stats.inventory_functions_visited += 1;
            if let Some(key) = function_key(module, function, tables, &context, limits, stats, None)
            {
                keys.insert((module.core.module_id, function.name.clone()), key);
            }
        }
    }
    stats.inventory_ns += elapsed_cost(started).saturating_sub(stats.body_key_ns - body_key_before);
    Some(keys)
}

struct KeyContext {
    fingerprint: Fingerprint,
    declarations: BTreeMap<ModuleId, BTreeMap<String, Fingerprint>>,
}

fn key_context(
    program: &CoreProgram,
    tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    target: &TargetProfile,
    limits: &Limits,
    stats: &mut FunctionStats,
) -> Option<KeyContext> {
    let started = start_cost(stats.cost_profiled);
    let result = key_context_inner(program, tables, target, limits, stats);
    stats.projection_context_ns += elapsed_cost(started);
    result
}

fn key_context_inner(
    program: &CoreProgram,
    tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    target: &TargetProfile,
    limits: &Limits,
    stats: &mut FunctionStats,
) -> Option<KeyContext> {
    if !program
        .modules
        .iter()
        .flat_map(|module| &module.core.functions)
        .any(worthwhile)
    {
        return None;
    }
    let mut context = FingerprintBuilder::new();
    context.update(b"ZIL-FUNCTION-TYPES-3\0");
    target.hash(&mut context);
    // Limits is an explicit complete struct so adding a field forces review.
    let Limits {
        max_source_bytes,
        max_tokens,
        max_nodes,
        max_functions,
        max_blocks,
        max_instructions,
        max_locals,
        max_values,
        max_analysis_steps,
        max_ct_steps,
        max_call_depth,
    } = limits;
    (
        max_source_bytes,
        max_tokens,
        max_nodes,
        max_functions,
        max_blocks,
        max_instructions,
        max_locals,
        max_values,
        max_analysis_steps,
        max_ct_steps,
        max_call_depth,
    )
        .hash(&mut context);
    program.root.hash(&mut context);
    program.modules.len().hash(&mut context);
    // Keep the complete name/ID/import namespace in the context. Its size and
    // order affect resolution and accounting, even for a body that uses only
    // some declarations. Signature contents are a separate observed facet.
    let mut declarations = BTreeMap::new();
    for module in &program.modules {
        stats.context_modules_visited += 1;
        let core = &module.core;
        if core.schema_version != SCHEMA_VERSION
            || !core.aggregate_templates.is_empty()
            || !core.aggregate_applications.is_empty()
            || !core.aggregates.is_empty()
            || !core.enums.is_empty()
            || !core.unsupported.is_empty()
        {
            return None;
        }
        core.module_id.hash(&mut context);
        module.imports.len().hash(&mut context);
        for import in &module.imports {
            stats.context_imports_hashed += 1;
            import.alias.hash(&mut context);
            import.module.hash(&mut context);
        }
        core.functions.len().hash(&mut context);
        for f in &core.functions {
            stats.context_declarations_hashed += 1;
            if f.storage_life.is_some()
                || f.protocol_contract.is_some()
                || !f.type_parameters.is_empty()
                || !f.state_parameters.is_empty()
                || !f.callable_parameters.is_empty()
                || f.callable_return.is_some()
                || f.receiver.is_some()
                || (f.foreign.is_none()
                    && (!scalar(&f.return_type) || !f.parameters.iter().all(|p| scalar(&p.ty))))
            {
                return None;
            }
            let signature = tables.get(&core.module_id)?.get(&f.name)?;
            // Adding, removing or renumbering a declaration still invalidates
            // the epoch; a signature-only edit need not invalidate non-users.
            signature.id.hash(&mut context);
            f.name.hash(&mut context);
            let mut declaration = FingerprintBuilder::new();
            signature.id.hash(&mut declaration);
            signature.public.hash(&mut declaration);
            f.name.hash(&mut declaration);
            f.call_kind.hash(&mut declaration);
            f.life.is_some().hash(&mut declaration);
            if let Some(life) = &f.life {
                life.binders.hash(&mut declaration);
                life.parameter_lives.hash(&mut declaration);
                life.result_life.hash(&mut declaration);
            }
            f.foreign.is_some().hash(&mut declaration);
            if let Some(foreign) = &f.foreign {
                foreign.abi.hash(&mut declaration);
                foreign.symbol.hash(&mut declaration);
                foreign.noescape_parameters.hash(&mut declaration);
                foreign.static_callbacks.hash(&mut declaration);
            }
            f.return_type.hash(&mut declaration);
            f.result_mode.hash(&mut declaration);
            f.parameters.len().hash(&mut declaration);
            for p in &f.parameters {
                p.name.hash(&mut declaration);
                p.ty.hash(&mut declaration);
                p.mode.hash(&mut declaration);
            }
            declarations
                .entry(core.module_id)
                .or_insert_with(BTreeMap::new)
                .insert(f.name.clone(), declaration.digest());
        }
    }
    Some(KeyContext {
        fingerprint: context.digest(),
        declarations,
    })
}

// Keep the historical byte stream intact. The graph controls whether this
// projection executes; persistent raw work-product keys retain their meaning.
fn function_key(
    module: &ProgramModule,
    function: &CoreFunction,
    tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    context: &KeyContext,
    limits: &Limits,
    stats: &mut FunctionStats,
    observe: Option<&mut dyn FnMut(ModuleId, &str) -> Option<()>>,
) -> Option<Fingerprint> {
    let started = start_cost(stats.cost_profiled);
    let result = function_key_inner(module, function, tables, context, limits, stats, observe);
    let elapsed = elapsed_cost(started);
    stats.body_key_ns += elapsed;
    stats.evaluator_ns += elapsed;
    result
}

fn function_key_inner(
    module: &ProgramModule,
    function: &CoreFunction,
    tables: &BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    context: &KeyContext,
    limits: &Limits,
    stats: &mut FunctionStats,
    observe: Option<&mut dyn FnMut(ModuleId, &str) -> Option<()>>,
) -> Option<Fingerprint> {
    if !worthwhile(function) {
        return None;
    }
    stats.body_key_executions += 1;
    let mut encoder = BodyKey {
        hash: FingerprintBuilder::new(),
        origin: function.span,
        depth: limits.max_call_depth,
        remaining: limits.max_nodes,
        visited: 0,
        module,
        tables,
        declarations: &context.declarations,
        observe,
    };
    encoder.hash.update(&context.fingerprint.0);
    encoder.hash.update(
        &context
            .declarations
            .get(&module.core.module_id)?
            .get(&function.name)?
            .0,
    );
    module.core.module_id.hash(&mut encoder.hash);
    function.name.hash(&mut encoder.hash);
    let valid = encoder.span(function.span).is_some()
        && function
            .parameters
            .iter()
            .all(|parameter| encoder.span(parameter.span).is_some())
        && encoder.block(&function.body, 0).is_some();
    stats.body_key_visited_nodes += encoder.visited;
    valid.then(|| encoder.hash.digest())
}

struct BodyKey<'a, 'o> {
    hash: FingerprintBuilder,
    origin: Span,
    depth: usize,
    remaining: usize,
    visited: usize,
    module: &'a ProgramModule,
    tables: &'a BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    declarations: &'a BTreeMap<ModuleId, BTreeMap<String, Fingerprint>>,
    observe: Option<&'o mut dyn FnMut(ModuleId, &str) -> Option<()>>,
}
impl BodyKey<'_, '_> {
    fn visit(&mut self, depth: usize) -> Option<()> {
        self.visited += 1;
        self.remaining = self.remaining.checked_sub(1)?;
        (depth <= self.depth).then_some(())
    }
    fn span(&mut self, span: Span) -> Option<()> {
        if span.file != self.origin.file
            || span.start < self.origin.start
            || span.end > self.origin.end
            || span.start > span.end
        {
            return None;
        }
        (span.start - self.origin.start).hash(&mut self.hash);
        (span.end - self.origin.start).hash(&mut self.hash);
        Some(())
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> Option<()> {
        self.visit(depth)?;
        self.span(block.span)?;
        block.statements.len().hash(&mut self.hash);
        for s in &block.statements {
            self.visit(depth)?;
            self.span(s.span)?;
            match &s.kind {
                // This cache only fingerprints scalar statements; preserve the
                // ordinary checked path for aggregate pattern bindings.
                CoreStatementKind::TupleBinding { .. } => return None,
                CoreStatementKind::Binding {
                    name,
                    mutable,
                    ty,
                    initializer,
                } => {
                    0u8.hash(&mut self.hash);
                    name.hash(&mut self.hash);
                    mutable.hash(&mut self.hash);
                    if ty.as_ref().is_some_and(|t| !scalar(t)) {
                        return None;
                    }
                    ty.hash(&mut self.hash);
                    initializer.is_some().hash(&mut self.hash);
                    if let Some(e) = initializer {
                        self.expr(e, depth + 1)?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    1u8.hash(&mut self.hash);
                    self.expr(target, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::Expression(e) => {
                    2u8.hash(&mut self.hash);
                    self.expr(e, depth + 1)?;
                }
                CoreStatementKind::Return(e) => {
                    3u8.hash(&mut self.hash);
                    e.is_some().hash(&mut self.hash);
                    if let Some(e) = e {
                        self.expr(e, depth + 1)?;
                    }
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    4u8.hash(&mut self.hash);
                    self.expr(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    else_body.is_some().hash(&mut self.hash);
                    if let Some(b) = else_body {
                        self.block(b, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    5u8.hash(&mut self.hash);
                    self.expr(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    6u8.hash(&mut self.hash);
                    name.hash(&mut self.hash);
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::Block(b) => {
                    7u8.hash(&mut self.hash);
                    self.block(b, depth + 1)?;
                }
                CoreStatementKind::Break => 8u8.hash(&mut self.hash),
                CoreStatementKind::Continue => 9u8.hash(&mut self.hash),
                _ => return None,
            }
        }
        Some(())
    }
    fn call(&mut self, callee: &CoreExpr) -> Option<()> {
        let (module, name) = match &callee.kind {
            CoreExprKind::Name(name) => (self.module.core.module_id, name.as_str()),
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return None;
                };
                let import = self.module.imports.iter().find(|i| &i.alias == alias)?;
                (import.module, member.as_str())
            }
            _ => return None,
        };
        if self.tables.get(&module)?.get(name)?.foreign {
            return None;
        }
        // Bind the actual current declaration selected through the current
        // import/name tables, including visibility, ID, types and life header.
        // A callee body edit is not a Types dependency of its scalar caller;
        // Analyze and independent Verify still observe the entire current CFG.
        if let Some(observe) = &mut self.observe {
            observe(module, name)?;
        }
        self.hash
            .update(&self.declarations.get(&module)?.get(name)?.0);
        Some(())
    }
    fn expr(&mut self, e: &CoreExpr, depth: usize) -> Option<()> {
        self.visit(depth)?;
        self.span(e.span)?;
        match &e.kind {
            CoreExprKind::Literal(c) if scalar(&c.ty()) => {
                0u8.hash(&mut self.hash);
                c.hash(&mut self.hash);
            }
            CoreExprKind::Integer(s) => {
                1u8.hash(&mut self.hash);
                s.hash(&mut self.hash);
            }
            CoreExprKind::Float(s) => {
                12u8.hash(&mut self.hash);
                s.hash(&mut self.hash);
            }
            CoreExprKind::NumericConvert { value, target } if scalar(target) => {
                13u8.hash(&mut self.hash);
                target.hash(&mut self.hash);
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::Bool(b) => {
                2u8.hash(&mut self.hash);
                b.hash(&mut self.hash);
            }
            CoreExprKind::Unit => 3u8.hash(&mut self.hash),
            CoreExprKind::Name(n) => {
                4u8.hash(&mut self.hash);
                n.hash(&mut self.hash);
            }
            CoreExprKind::Unary { op, operand }
                if matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot) =>
            {
                5u8.hash(&mut self.hash);
                op.hash(&mut self.hash);
                self.expr(operand, depth + 1)?;
            }
            CoreExprKind::Binary { op, left, right } => {
                6u8.hash(&mut self.hash);
                op.hash(&mut self.hash);
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::IntegerConvert { value, target } if scalar(target) => {
                7u8.hash(&mut self.hash);
                target.hash(&mut self.hash);
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::Ascribed { ty, value } if scalar(ty) => {
                8u8.hash(&mut self.hash);
                ty.hash(&mut self.hash);
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.call(callee)?;
                9u8.hash(&mut self.hash);
                self.expr(callee, depth + 1)?;
                arguments.len().hash(&mut self.hash);
                for a in arguments {
                    self.expr(a, depth + 1)?;
                }
            }
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } if matches!(object.kind, CoreExprKind::Name(_)) => {
                10u8.hash(&mut self.hash);
                member.hash(&mut self.hash);
                self.expr(object, depth + 1)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                11u8.hash(&mut self.hash);
                self.expr(condition, depth + 1)?;
                self.expr(then_value, depth + 1)?;
                self.expr(else_value, depth + 1)?;
            }
            _ => return None,
        }
        Some(())
    }
}

#[cfg(test)]
#[path = "incremental_producer_tests.rs"]
mod producer_tests;

#[cfg(test)]
mod reactive_tests {
    use super::*;
    use zil_base::NodeId;

    fn span() -> Span {
        Span::new(FileId(0), 0, 100)
    }
    fn expr(kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: NodeId(0),
            kind,
            span: span(),
        }
    }
    fn function(index: u32, name: &str, result: CoreExpr) -> CoreFunction {
        CoreFunction {
            storage_life: None,
            call_kind: None,
            life: None,
            type_parameters: vec![],
            state_parameters: vec![],
            protocol_contract: None,
            callable_return: None,
            id: NodeId(index),
            name: name.into(),
            callable_parameters: vec![],
            foreign: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            body: CoreBlock {
                statements: (0..4)
                    .map(|i| CoreStatement {
                        id: NodeId(i),
                        span: span(),
                        kind: if i == 3 {
                            CoreStatementKind::Return(Some(result.clone()))
                        } else {
                            CoreStatementKind::Expression(expr(CoreExprKind::Integer(
                                i.to_string(),
                            )))
                        },
                    })
                    .collect(),
                span: span(),
            },
            receiver: None,
            span: span(),
        }
    }
    fn program(literal: &str) -> CoreProgram {
        let mut leaf = function(0, "leaf", expr(CoreExprKind::Integer(literal.into())));
        leaf.parameters.push(CoreParameter {
            name: "x".into(),
            ty: Type::I32,
            mode: PassingMode::Infer,
            span: span(),
        });
        let caller = function(
            1,
            "caller",
            expr(CoreExprKind::Call {
                callee: Box::new(expr(CoreExprKind::Name("leaf".into()))),
                arguments: vec![expr(CoreExprKind::Integer("42".into()))],
            }),
        );
        let unrelated = function(2, "unrelated", expr(CoreExprKind::Integer("7".into())));
        CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            modules: vec![ProgramModule {
                imports: vec![],
                exports: vec![NodeId(0), NodeId(1), NodeId(2)],
                core: CoreModule {
                    dyn_declarations: vec![],
                    dyn_interfaces: vec![],
                    dyn_vtables: vec![],
                    life_declarations: vec![],
                    life_instances: vec![],
                    closure_life_contexts: vec![],
                    schema_version: SCHEMA_VERSION,
                    module_id: ModuleId(0),
                    source_fingerprint: Fingerprint::ZERO,
                    functions: vec![leaf, caller, unrelated],
                    aggregates: vec![],
                    aggregate_templates: vec![],
                    aggregate_applications: vec![],
                    enums: vec![],
                    unsupported: vec![],
                },
            }],
        }
    }
    fn owners(module: &str) -> BTreeMap<ModuleId, (String, String)> {
        BTreeMap::from([(ModuleId(0), ("project/main.zil".into(), module.into()))])
    }
    fn check(
        session: &mut FunctionSession,
        program: &CoreProgram,
        graph: bool,
    ) -> CompileResult<TypedProgram> {
        let limits = Limits::default();
        let prepared = infer_program_for_ct(program, &limits)?.into_prepared();
        if graph {
            session.check_prepared_program_with_owners(
                &prepared,
                &TargetProfile::default(),
                &limits,
                &owners("main.zil"),
            )
        } else {
            session.check_prepared_program(&prepared, &TargetProfile::default(), &limits)
        }
    }
    fn same_wire(graph: &FunctionSession, legacy: &FunctionSession) {
        assert_eq!(
            graph.encode_work_products(&TargetProfile::default(), &Limits::default()),
            legacy.encode_work_products(&TargetProfile::default(), &Limits::default())
        );
    }

    #[test]
    fn body_projection_skips_ast_hashes_and_keeps_historical_wire_keys() {
        let mut graph = FunctionSession::default();
        let mut legacy = FunctionSession::default();
        for (literal, executions, reused) in [("4", 3, 0), ("5", 1, 2), ("6", 1, 2)] {
            let source = program(literal);
            assert_eq!(
                check(&mut graph, &source, true),
                check(&mut legacy, &source, false)
            );
            let stats = graph.last_stats();
            assert_eq!(
                (stats.body_key_executions, stats.body_key_reuses),
                (executions, reused)
            );
            assert_eq!(stats.graph.executed as usize, executions);
            assert_eq!(legacy.last_stats().body_key_executions, 3);
            assert!(stats.body_key_visited_nodes <= legacy.last_stats().body_key_visited_nodes);
            same_wire(&graph, &legacy);
        }
        assert_eq!(graph.last_stats().core_function_eq_comparisons, 3);
        assert_eq!(graph.last_stats().reused_functions, 2);
        assert_eq!(graph.last_stats().executed_functions, 1);
    }

    #[test]
    fn signature_reads_rebind_and_failed_epoch_cannot_publish_graph_values() {
        let mut graph = FunctionSession::default();
        let mut legacy = FunctionSession::default();
        let mut source = program("4");
        assert_eq!(
            check(&mut graph, &source, true),
            check(&mut legacy, &source, false)
        );
        source.modules[0].core.functions[0].parameters[0].name = "renamed".into();
        assert_eq!(
            check(&mut graph, &source, true),
            check(&mut legacy, &source, false)
        );
        assert_eq!(graph.last_stats().body_key_executions, 2);
        assert_eq!(graph.last_stats().body_key_reuses, 1);
        same_wire(&graph, &legacy);
        let valid = source.clone();
        source.modules[0].core.functions[0].name = "missing".into();
        let result = check(&mut graph, &source, true);
        assert_eq!(result, check(&mut legacy, &source, false));
        assert!(result.is_err());
        assert!(graph.projections.source.is_none());
        assert_eq!(
            check(&mut graph, &valid, true),
            check(&mut legacy, &valid, false)
        );
        assert_eq!(graph.last_stats().body_key_executions, 3);
        assert_eq!(graph.last_stats().reused_functions, 3);
        same_wire(&graph, &legacy);
    }

    #[test]
    fn owners_and_raw_fingerprints_do_not_replace_current_core_evidence() {
        let mut graph = FunctionSession::default();
        let source = program("4");
        check(&mut graph, &source, true).unwrap();
        // Both source fingerprints deliberately stay ZERO across the edit.
        let edited = program("8");
        let actual = check(&mut graph, &edited, true).unwrap();
        assert_eq!(
            actual,
            check(&mut FunctionSession::default(), &edited, false).unwrap()
        );
        assert_eq!(graph.last_stats().body_key_executions, 1);
        let prepared = infer_program_for_ct(&edited, &Limits::default())
            .unwrap()
            .into_prepared();
        let moved = graph
            .check_prepared_program_with_owners(
                &prepared,
                &TargetProfile::default(),
                &Limits::default(),
                &owners("moved.zil"),
            )
            .unwrap();
        assert_eq!(moved, actual);
        assert_eq!(graph.last_stats().body_key_executions, 3);
        assert_eq!(graph.last_stats().reused_functions, 3);
        let checkpoint = graph.checkpoint();
        assert!(Arc::ptr_eq(&checkpoint.entries, &graph.entries));
        assert!(checkpoint.projections.source.is_none());
        assert_eq!(checkpoint.projections.graph.stats().retained_nodes, 0);
    }
}
