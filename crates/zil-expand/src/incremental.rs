//! Process-local reuse of successful scalar expansion from sealed parser output.
//! Receipts reproduce bounded work; they confer no type or safety authority.
use super::*;
use std::sync::Arc;
use std::time::Instant;
use zil_base::{FileId, Fingerprint, ModuleId, TargetProfile};
use zil_syntax::ParsedSource;

#[derive(Clone, Debug)]
pub struct ModuleRevision(Arc<()>);
impl PartialEq for ModuleRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ModuleRevision {}
#[derive(Clone, Debug)]
pub struct ProducedProgram {
    core: Arc<CoreProgram>,
    revisions: BTreeMap<ModuleId, ModuleRevision>,
}
impl ProducedProgram {
    pub fn program(&self) -> &Arc<CoreProgram> {
        &self.core
    }
    pub fn module_revision(&self, module: ModuleId) -> Option<&ModuleRevision> {
        self.revisions.get(&module)
    }
}
/// A raw source-map value, not a producer/version or verification proof. The
/// shared arm owns only the actual map: historical maps cannot retain Core.
#[derive(Clone)]
pub struct ProducedSourceMap(MapOwnership);
#[derive(Clone)]
enum MapOwnership {
    Owned(SourceMap),
    Shared(Arc<SourceMap>),
}
impl std::ops::Deref for ProducedSourceMap {
    type Target = SourceMap;
    fn deref(&self) -> &SourceMap {
        match &self.0 {
            MapOwnership::Owned(map) => map,
            MapOwnership::Shared(map) => map,
        }
    }
}
impl std::fmt::Debug for ProducedSourceMap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, formatter)
    }
}
impl From<SourceMap> for ProducedSourceMap {
    fn from(map: SourceMap) -> Self {
        Self(MapOwnership::Owned(map))
    }
}
impl ProducedSourceMap {
    pub(crate) fn shared(map: Arc<SourceMap>) -> Self {
        Self(MapOwnership::Shared(map))
    }
    pub(crate) fn owned_mut(&mut self) -> Option<&mut SourceMap> {
        match &mut self.0 {
            MapOwnership::Owned(map) => Some(map),
            MapOwnership::Shared(_) => None,
        }
    }
}
pub(crate) struct CachedLower {
    expanded: ExpandedModule,
    map: Arc<SourceMap>,
}
impl std::ops::Deref for CachedLower {
    type Target = ExpandedModule;
    fn deref(&self) -> &ExpandedModule {
        &self.expanded
    }
}
impl CachedLower {
    pub(crate) fn shared_map(&self) -> Arc<SourceMap> {
        self.map.clone()
    }
}
pub struct ExpandedProgram {
    modules: Vec<(ProgramModule, ProducedSourceMap)>,
    revisions: BTreeMap<ModuleId, ModuleRevision>,
}
impl ExpandedProgram {
    pub fn into_program(
        self,
        root: ModuleId,
        source_fingerprint: Fingerprint,
    ) -> (ProducedProgram, Vec<ProducedSourceMap>) {
        let (modules, maps) = self.modules.into_iter().unzip();
        (
            ProducedProgram {
                core: Arc::new(CoreProgram {
                    root,
                    source_fingerprint,
                    modules,
                }),
                revisions: self.revisions,
            },
            maps,
        )
    }
}
macro_rules! statistics {
    ($($field:ident),* $(,)?) => {
        #[derive(Clone, Debug, Default)]
        pub struct ProducerStats { $(pub $field: u64,)* }
        impl ProducerStats {
            pub fn add(&mut self, other: &Self) { $(self.$field = self.$field.saturating_add(other.$field);)* }
            pub fn to_json(&self) -> String {
                let entries = [$(format!("\"{}\":{}", stringify!($field), self.$field)),*];
                format!("{{{}}}", entries.join(","))
            }
        }
    };
}
// `visited_nodes`/`receipt_nodes` count successful original Budget::node units,
// not every compiler visit. `syntax_clone_receipt_nodes` counts those units for
// complete syntax clones. `cloned_source_map_entries` counts copied mapping
// entries (including span markers), not all nested Core/type/string copies.
// `shared_maps` includes fresh sidecars and cache hits actually emitted here;
// their entries are counted separately from the one real fresh map clone.
// `core_module_deep_clones` counts actual final Core Vec assembly copies.
statistics!(
    classified_modules,
    classifier_nodes,
    preflight_modules,
    normalize_modules,
    plain_prepare_modules,
    plain_lifetimes_modules,
    restore_modules,
    lower_modules,
    reused_lower_modules,
    receipt_hits,
    receipt_nodes,
    receipt_budget_misses,
    lower_budget_misses,
    visited_nodes,
    syntax_clone_receipt_nodes,
    normalized_item_deep_clones,
    restored_module_deep_clones,
    retained_normalized_items,
    retained_restored_modules,
    cloned_source_map_entries,
    source_map_deep_clones,
    shared_maps,
    shared_entries,
    core_module_deep_clones,
    cloned_source_bytes,
    context_ns,
    production_ns,
    reuse_ns,
    assembly_ns,
    total_ns,
    environment_cold,
    environment_source_layout_changed,
    environment_headers_changed,
    environment_imports_changed,
    environment_target_changed,
    environment_context_changed,
    env_limit_max_source_bytes_changed,
    env_limit_max_tokens_changed,
    env_limit_max_nodes_changed,
    env_limit_max_functions_changed,
    env_limit_max_blocks_changed,
    env_limit_max_instructions_changed,
    env_limit_max_locals_changed,
    env_limit_max_values_changed,
    env_limit_max_analysis_steps_changed,
    env_limit_max_ct_steps_changed,
    env_limit_max_call_depth_changed,
    allowance_work_increase,
    allowance_work_decrease,
    allowance_nodes_increase,
    allowance_nodes_decrease
);
#[derive(Clone, Debug, PartialEq)]
struct Environment {
    sources: Vec<(ModuleId, FileId, String)>,
    headers: Vec<Vec<s::Item>>,
    imports: Vec<Vec<CoreImport>>,
    limits: Limits,
    target: TargetProfile,
    context: String,
}
// Only monotone node/work allowances are replayable in this slice. Every
// other limit (including temporary/source bytes and depth) remains an exact
// validity input. A successful scalar receipt carries no lineage/feature side
// effect; current insufficient allowances re-enter the original stage untouched.
fn same_limit_domain(left: &Limits, right: &Limits) -> bool {
    left.max_source_bytes == right.max_source_bytes
        && left.max_tokens == right.max_tokens
        && left.max_functions == right.max_functions
        && left.max_blocks == right.max_blocks
        && left.max_instructions == right.max_instructions
        && left.max_locals == right.max_locals
        && left.max_values == right.max_values
        && left.max_ct_steps == right.max_ct_steps
        && left.max_call_depth == right.max_call_depth
}
impl Environment {
    fn compatible(&self, current: &Self) -> bool {
        self.sources == current.sources
            && self.headers == current.headers
            && self.imports == current.imports
            && same_limit_domain(&self.limits, &current.limits)
            && self.target == current.target
            && self.context == current.context
    }
}
impl ProducerStats {
    fn environment_change(&mut self, previous: Option<&Environment>, current: &Environment) {
        let Some(previous) = previous else {
            self.environment_cold += 1;
            return;
        };
        self.environment_source_layout_changed += u64::from(previous.sources != current.sources);
        self.environment_headers_changed += u64::from(previous.headers != current.headers);
        self.environment_imports_changed += u64::from(previous.imports != current.imports);
        self.environment_target_changed += u64::from(previous.target != current.target);
        self.environment_context_changed += u64::from(previous.context != current.context);
        macro_rules! limit_change {
            ($($limit:ident => $counter:ident),* $(,)?) => { $(self.$counter += u64::from(previous.limits.$limit != current.limits.$limit);)* };
        }
        limit_change!(
            max_source_bytes => env_limit_max_source_bytes_changed,
            max_tokens => env_limit_max_tokens_changed,
            max_nodes => env_limit_max_nodes_changed,
            max_functions => env_limit_max_functions_changed,
            max_blocks => env_limit_max_blocks_changed,
            max_instructions => env_limit_max_instructions_changed,
            max_locals => env_limit_max_locals_changed,
            max_values => env_limit_max_values_changed,
            max_analysis_steps => env_limit_max_analysis_steps_changed,
            max_ct_steps => env_limit_max_ct_steps_changed,
            max_call_depth => env_limit_max_call_depth_changed,
        );
        self.allowance_work_increase += current
            .limits
            .max_analysis_steps
            .saturating_sub(previous.limits.max_analysis_steps)
            as u64;
        self.allowance_work_decrease += previous
            .limits
            .max_analysis_steps
            .saturating_sub(current.limits.max_analysis_steps)
            as u64;
        self.allowance_nodes_increase += current
            .limits
            .max_nodes
            .saturating_sub(previous.limits.max_nodes)
            as u64;
        self.allowance_nodes_decrease += previous
            .limits
            .max_nodes
            .saturating_sub(current.limits.max_nodes)
            as u64;
    }
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Stage {
    Preflight,
    Normalize,
    Prepare,
    Lifetimes,
    Distribute,
    Restore,
}
#[derive(Clone, Copy)]
struct Receipt {
    nodes: usize,
    work: usize,
}
#[derive(Clone)]
struct Entry {
    parsed: ParsedSource,
    scalar: bool,
    receipts: BTreeMap<(Stage, usize), Receipt>,
    normalized: BTreeMap<usize, Arc<s::Item>>,
    restored: Option<Arc<(s::Module, Vec<CoreImport>)>>,
    lower: Option<(Limits, Arc<CachedLower>)>,
    revision: ModuleRevision,
}
#[derive(Clone, Default)]
pub struct ExpandSession {
    environment: Option<Environment>,
    entries: BTreeMap<ModuleId, Entry>,
    stats: ProducerStats,
    profiling: bool,
}
impl ExpandSession {
    pub fn set_cost_profiling(&mut self, enabled: bool) {
        self.profiling = enabled;
    }
    pub fn checkpoint(&self) -> Self {
        self.clone()
    }
    pub fn last_stats(&self) -> &ProducerStats {
        &self.stats
    }
    pub fn expand_parsed(
        &mut self,
        parsed: &[ParsedSource],
        imports: &[Vec<CoreImport>],
        limits: &Limits,
        target: &TargetProfile,
        context: source_context::SourceIdentityContext<'_>,
    ) -> CompileResult<Option<ExpandedProgram>> {
        self.stats = ProducerStats::default();
        let total = self.profiling.then(Instant::now);
        let result = self.expand_parsed_inner(parsed, imports, limits, target, context);
        if let Some(start) = total {
            self.stats.total_ns = nanos(start);
            // Before production starts, early fallback/errors belong entirely
            // to source classification and environment preparation.
            if self.stats.context_ns == 0 {
                self.stats.context_ns = self.stats.total_ns.saturating_sub(
                    self.stats.production_ns + self.stats.reuse_ns + self.stats.assembly_ns,
                );
            }
        }
        result
    }
    fn expand_parsed_inner(
        &mut self,
        parsed: &[ParsedSource],
        imports: &[Vec<CoreImport>],
        limits: &Limits,
        target: &TargetProfile,
        context: source_context::SourceIdentityContext<'_>,
    ) -> CompileResult<Option<ExpandedProgram>> {
        let context_clock = self.profiling.then(Instant::now);
        if parsed.is_empty() || parsed.len() != imports.len() {
            return Ok(None);
        }
        // The raw normalizer checks duplicate files after complete input
        // preflight. A file-keyed receipt must never merge two distinct inputs;
        // fall back so raw budget/identity diagnostics keep their original order.
        let mut files = BTreeSet::new();
        if parsed
            .iter()
            .any(|parsed| !files.insert(parsed.source().file_id))
        {
            return Ok(None);
        }
        let context_key = match context {
            source_context::SourceIdentityContext::ClosedSnapshot {
                root,
                aliases,
                declarations: source_context::DeclarationPlacement::Original,
            } => format!("closed:{root:?}:{aliases:?}"),
            // The sealed scalar domain needs no declaration lineage. A late
            // materializer route may therefore retain an unavailable marker;
            // it remains a distinct environment input, never restored origins.
            source_context::SourceIdentityContext::ClosedSnapshot {
                root,
                aliases,
                declarations: source_context::DeclarationPlacement::Unavailable,
            } => format!("closed-unavailable:{root:?}:{aliases:?}"),
            source_context::SourceIdentityContext::Explicit {
                package,
                modules,
                declarations: source_context::DeclarationPlacement::Original,
            } => format!("explicit:{package:?}:{modules:?}"),
            _ => return Ok(None),
        };
        let mut entries = BTreeMap::new();
        for parsed in parsed {
            let module = &parsed.parsed().module;
            if !parsed.parsed().diagnostics.is_empty()
                || !parsed.parsed().deferred_attributes.is_empty()
                || !parsed.parsed().cfg_removed.is_empty()
                || parsed.parsed().type_meta_candidate
            {
                return Ok(None);
            }
            let old = self
                .entries
                .get(&module.id)
                .filter(|entry| entry.parsed.same_revision(parsed));
            let entry = if let Some(old) = old {
                old.clone()
            } else {
                self.stats.classified_modules += 1;
                let (scalar, nodes) = scalar(module, limits);
                self.stats.classifier_nodes += nodes;
                Entry {
                    parsed: parsed.clone(),
                    scalar,
                    receipts: BTreeMap::new(),
                    normalized: BTreeMap::new(),
                    restored: None,
                    lower: None,
                    revision: ModuleRevision(Arc::new(())),
                }
            };
            if !entry.scalar || entries.insert(module.id, entry).is_some() {
                return Ok(None);
            }
        }
        let mut headers = Vec::new();
        for parsed in parsed {
            let mut module_headers = Vec::new();
            for item in &parsed.parsed().module.items {
                let mut header = match &item.kind {
                    s::ItemKind::Function(function) => s::Item {
                        kind: s::ItemKind::Function(header(function)),
                        attributes: item.attributes.clone(),
                        public: item.public,
                        span: Span::default(),
                    },
                    s::ItemKind::Extern { abi, function } => s::Item {
                        kind: s::ItemKind::Extern {
                            abi: abi.clone(),
                            function: header(function),
                        },
                        attributes: item.attributes.clone(),
                        public: item.public,
                        span: Span::default(),
                    },
                    s::ItemKind::Import(_) => item.clone(),
                    _ => unreachable!("scalar inventory"),
                };
                header.span = Span::default();
                for attribute in &mut header.attributes {
                    attribute.span = Span::default();
                }
                module_headers.push(header);
            }
            headers.push(module_headers);
        }
        let environment = Environment {
            sources: parsed
                .iter()
                .map(|p| {
                    (
                        p.source().module_id,
                        p.source().file_id,
                        p.source().path.clone(),
                    )
                })
                .collect(),
            headers,
            imports: imports.to_vec(),
            limits: limits.clone(),
            target: target.clone(),
            context: context_key,
        };
        // Detailed invalidation diagnostics are opt-in: normal requests keep
        // the original single Environment equality check and no second header walk.
        if self.profiling {
            self.stats
                .environment_change(self.environment.as_ref(), &environment);
        }
        if self
            .environment
            .as_ref()
            .is_none_or(|previous| !previous.compatible(&environment))
        {
            for entry in entries.values_mut() {
                entry.receipts.clear();
                entry.normalized.clear();
                entry.restored = None;
                entry.lower = None;
                entry.revision = ModuleRevision(Arc::new(()));
            }
        }
        if let Some(start) = context_clock {
            self.stats.context_ns += nanos(start);
        }
        let assembly = self.profiling.then(Instant::now);
        let sources: Vec<_> = parsed.iter().map(|p| p.source().clone()).collect();
        self.stats.cloned_source_bytes += sources.iter().map(|s| s.text.len() as u64).sum::<u64>();
        if let Some(start) = assembly {
            self.stats.assembly_ns += nanos(start);
        }
        let syntax: Vec<_> = parsed.iter().map(|p| &p.parsed().module).collect();
        let files = parsed
            .iter()
            .map(|p| (p.source().file_id, p.source().module_id))
            .collect();
        let mut reuse = Reuse {
            entries: &mut entries,
            files,
            stats: &mut self.stats,
            executed: BTreeSet::new(),
            profiling: self.profiling,
        };
        let Some(modules) = super::expand_program_cached(
            &sources, &syntax, imports, limits, target, context, &mut reuse,
        )?
        else {
            return Ok(None);
        };
        let revisions = entries
            .iter()
            .map(|(id, entry)| (*id, entry.revision.clone()))
            .collect();
        self.entries = entries;
        self.environment = Some(environment);
        Ok(Some(ExpandedProgram { modules, revisions }))
    }
}
// In the sealed scalar domain, Normalize's Index stores only names/publicness/
// kind and Restore's Functions stores only owner/name. Neither stores definition
// spans. Lower's declaration_index contains only aggregates (absent here), and
// collect_static_traits returns before project lookup when traits/impls are absent.
// All emitted spans therefore come from the current module. Keep semantic headers
// global while the ParsedSource revision protects that module's local positions.
fn header(function: &s::Function) -> s::Function {
    fn ty(value: &mut s::TypeRef) {
        value.span = Span::default();
        if let s::TypeKind::Group(inner) = &mut value.kind {
            ty(inner);
        }
    }
    let mut params = function.params.clone();
    for parameter in &mut params {
        parameter.span = Span::default();
        ty(&mut parameter.ty);
        for attribute in &mut parameter.attributes {
            attribute.span = Span::default();
        }
    }
    let mut result = function.return_type.clone();
    if let Some(result) = &mut result {
        ty(result);
    }
    s::Function {
        name: function.name.clone(),
        generics: function.generics.clone(),
        params,
        return_type: result,
        where_clause: function.where_clause.clone(),
        contract: function.contract.clone(),
        body: s::FunctionBody::Declaration,
        span: Span::default(),
    }
}
pub(crate) struct Reuse<'a> {
    entries: &'a mut BTreeMap<ModuleId, Entry>,
    files: BTreeMap<FileId, ModuleId>,
    stats: &'a mut ProducerStats,
    executed: BTreeSet<(Stage, FileId)>,
    profiling: bool,
}
fn nanos(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
impl Reuse<'_> {
    pub(crate) fn clock(&self) -> Option<Instant> {
        self.profiling.then(Instant::now)
    }
    pub(crate) fn production(&mut self, clock: Option<Instant>) {
        if let Some(start) = clock {
            self.stats.production_ns += nanos(start);
        }
    }
    pub(crate) fn assembly(&mut self, clock: Option<Instant>) {
        if let Some(start) = clock {
            self.stats.assembly_ns += nanos(start);
        }
    }

    fn entry(&mut self, file: FileId) -> &mut Entry {
        let id = self.files[&file];
        self.entries.get_mut(&id).expect("sealed module")
    }
    pub(crate) fn replay(
        &mut self,
        stage: Stage,
        span: Span,
        budget: &mut generics::Budget<'_>,
    ) -> bool {
        let clock = self.clock();
        let replayed = self.replay_inner(stage, span, budget);
        if let Some(start) = clock {
            self.stats.reuse_ns += nanos(start);
        }
        replayed
    }
    fn replay_inner(
        &mut self,
        stage: Stage,
        span: Span,
        budget: &mut generics::Budget<'_>,
    ) -> bool {
        let receipt = self
            .entry(span.file)
            .receipts
            .get(&(stage, span.start))
            .copied();
        let Some(receipt) = receipt else {
            return false;
        };
        let remaining = budget.remaining_limits();
        if remaining.max_nodes < receipt.nodes || remaining.max_analysis_steps < receipt.work {
            self.stats.receipt_budget_misses += 1;
            return false;
        }
        let mut after = remaining;
        after.max_nodes -= receipt.nodes;
        after.max_analysis_steps -= receipt.work;
        budget.adopt_remaining(&after);
        self.stats.receipt_hits += 1;
        self.stats.receipt_nodes += receipt.nodes as u64;
        true
    }
    pub(crate) fn record(
        &mut self,
        stage: Stage,
        span: Span,
        before: Limits,
        budget: &generics::Budget<'_>,
    ) {
        let clock = self.clock();
        let after = budget.remaining_limits();
        let receipt = Receipt {
            nodes: before.max_nodes - after.max_nodes,
            work: before.max_analysis_steps - after.max_analysis_steps,
        };
        self.stats.visited_nodes += receipt.nodes as u64;
        self.entry(span.file)
            .receipts
            .insert((stage, span.start), receipt);
        if self.executed.insert((stage, span.file)) {
            match stage {
                Stage::Preflight => self.stats.preflight_modules += 1,
                Stage::Normalize => self.stats.normalize_modules += 1,
                Stage::Prepare => self.stats.plain_prepare_modules += 1,
                Stage::Lifetimes => self.stats.plain_lifetimes_modules += 1,
                Stage::Restore => self.stats.restore_modules += 1,
                Stage::Distribute => {}
            }
        }
        if let Some(start) = clock {
            self.stats.reuse_ns += nanos(start);
        }
    }
    pub(crate) fn normalized(&mut self, span: Span) -> Option<Arc<s::Item>> {
        let clock = self.clock();
        let value = self.entry(span.file).normalized.get(&span.start).cloned();
        if let Some(start) = clock {
            self.stats.reuse_ns += nanos(start);
        }
        value
    }
    pub(crate) fn normalized_input_cloned(&mut self) {
        self.stats.normalized_item_deep_clones += 1;
    }
    pub(crate) fn restored_input_cloned(&mut self) {
        self.stats.restored_module_deep_clones += 1;
    }
    pub(crate) fn retain_normalized(&mut self) {
        self.stats.retained_normalized_items += 1;
    }
    pub(crate) fn retain_restored(&mut self) {
        self.stats.retained_restored_modules += 1;
    }
    pub(crate) fn save_normalized_shared(&mut self, span: Span, item: Arc<s::Item>) {
        self.entry(span.file).normalized.insert(span.start, item);
    }
    pub(crate) fn save_restored_owned(
        &mut self,
        file: FileId,
        module: s::Module,
        imports: Vec<CoreImport>,
    ) -> Arc<(s::Module, Vec<CoreImport>)> {
        let clock = self.clock();
        let module = Arc::new((module, imports));
        self.assembly(clock);
        self.entry(file).restored = Some(module.clone());
        module
    }
    pub(crate) fn save_normalized(&mut self, span: Span, item: &s::Item) {
        let clock = self.clock();
        let value = Arc::new(item.clone());
        self.assembly(clock);
        self.entry(span.file).normalized.insert(span.start, value);
    }
    pub(crate) fn restored(&mut self, file: FileId) -> Option<Arc<(s::Module, Vec<CoreImport>)>> {
        let clock = self.clock();
        let value = self.entry(file).restored.clone();
        if let Some(start) = clock {
            self.stats.reuse_ns += nanos(start);
        }
        value
    }
    pub(crate) fn save_restored(&mut self, file: FileId, module: &s::Module, edges: &[CoreImport]) {
        let clock = self.clock();
        let value = Arc::new((module.clone(), edges.to_vec()));
        self.assembly(clock);
        self.entry(file).restored = Some(value);
    }
    pub(crate) fn lower(&mut self, file: FileId, limits: &Limits) -> Option<Arc<CachedLower>> {
        let clock = self.clock();
        let candidate = self
            .entry(file)
            .lower
            .as_ref()
            .map(|(old, value)| (old.clone(), value.clone()));
        let cached = candidate.and_then(|(old, value)| {
            if !same_limit_domain(&old, limits) || !value.shareable() {
                return None;
            }
            if limits.max_analysis_steps < value.lowering_demand.work
                || limits.max_nodes < value.lowering_demand.nodes
            {
                self.stats.lower_budget_misses += 1;
                return None;
            }
            Some(value)
        });
        if let Some(start) = clock {
            self.stats.reuse_ns += nanos(start);
        }
        let cached = cached?;
        self.stats.reused_lower_modules += 1;
        Some(cached)
    }
    pub(crate) fn lower_executed(&mut self) {
        self.stats.lower_modules += 1;
    }
    pub(crate) fn save_lower(
        &mut self,
        file: FileId,
        limits: &Limits,
        expanded: ExpandedModule,
    ) -> Arc<CachedLower> {
        // One fresh map copy builds the sidecar, replacing the map copy paid by
        // the former whole-ExpandedModule cache clone. Core itself moves in.
        self.stats.cloned_source_map_entries += expanded.source_map.entries.len() as u64;
        let clock = self.clock();
        self.stats.source_map_deep_clones += 1;
        let map = Arc::new(expanded.source_map.clone());
        let value = Arc::new(CachedLower { expanded, map });
        self.assembly(clock);
        let entry = self.entry(file);
        entry.lower = Some((limits.clone(), value.clone()));
        entry.revision = ModuleRevision(Arc::new(()));
        value
    }
    pub(crate) fn clone_published_core(&mut self, cached: &CachedLower) -> CoreModule {
        let clock = self.clock();
        self.stats.core_module_deep_clones += 1;
        let core = cached.core.clone();
        self.assembly(clock);
        core
    }
    pub(crate) fn publish_shared_map(&mut self, entries: usize) {
        self.stats.shared_maps += 1;
        self.stats.shared_entries += entries as u64;
    }
    pub(crate) fn cloned(&mut self, span: Span, stage: Stage) {
        if let Some(receipt) = self.entry(span.file).receipts.get(&(stage, span.start)) {
            self.stats.syntax_clone_receipt_nodes += receipt.nodes as u64;
        }
    }
}
fn scalar(module: &s::Module, limits: &Limits) -> (bool, u64) {
    struct Scalar {
        nodes: u64,
        left: usize,
        depth: usize,
    }
    impl ct_selection::Visitor for Scalar {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.nodes += 1;
            if self.left == 0 || depth > self.depth {
                return Err(vec![Diagnostic::limit(
                    Phase::Expand,
                    "producer inventory budget exceeded",
                    Some(span),
                )]);
            }
            self.left -= 1;
            Ok(())
        }
        fn text(&mut self, _: &str, _: Span) -> CompileResult<()> {
            Ok(())
        }
        fn item(&mut self, item: &s::Item, depth: usize) -> CompileResult<()> {
            let valid = match &item.kind {
                s::ItemKind::Import(import) => !import.syntax,
                s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => {
                    function.generics.is_empty()
                        && function.where_clause.is_empty()
                        && function.contract.is_empty()
                }
                _ => false,
            };
            if !valid
                || item.attributes.iter().any(|a| {
                    a.path.as_slice() != ["test"]
                        || !a.arguments.is_empty()
                        || a.string_value.is_some()
                })
            {
                return Err(vec![]);
            }
            ct_selection::walk_item(self, item, depth)
        }
        fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
            if !matches!(&ty.kind, s::TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() && (super::integer_name(&path[0]) || matches!(path[0].as_str(), "bool" | "void")))
                && !matches!(ty.kind, s::TypeKind::Group(_))
            {
                return Err(vec![]);
            }
            ct_selection::walk_type(self, ty, depth)
        }
        fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
            match &expression.kind {
                s::ExprKind::Member {
                    member: s::MemberName::Name(name),
                    ..
                } if matches!(name.as_str(), "Poll" | "TaskCreateError" | "TaskPollState") => {
                    return Err(vec![]);
                }
                s::ExprKind::Literal(_)
                | s::ExprKind::Group(_)
                | s::ExprKind::Unary { .. }
                | s::ExprKind::Binary { .. }
                | s::ExprKind::Pipeline { .. }
                | s::ExprKind::Member { .. }
                | s::ExprKind::Call { .. }
                | s::ExprKind::If { .. } => {}
                s::ExprKind::Name(name)
                    if !matches!(
                        name.as_str(),
                        "Option"
                            | "Task"
                            | "Poll"
                            | "task_create"
                            | "TaskCreateError"
                            | "TaskPollState"
                    ) => {}
                _ => return Err(vec![]),
            }
            ct_selection::walk_expression(self, expression, depth)
        }
        fn statement(&mut self, statement: &s::Stmt, depth: usize) -> CompileResult<()> {
            match &statement.kind {
                s::StmtKind::Binding { pattern, .. }
                    if matches!(
                        pattern.kind,
                        s::PatternKind::Name(_) | s::PatternKind::Wildcard
                    ) => {}
                s::StmtKind::Return(_)
                | s::StmtKind::Break
                | s::StmtKind::Continue
                | s::StmtKind::If {
                    condition: s::Condition::Expression(_),
                    ..
                }
                | s::StmtKind::While {
                    condition: s::Condition::Expression(_),
                    ..
                }
                | s::StmtKind::Loop(_)
                | s::StmtKind::Unsafe(_)
                | s::StmtKind::Assign { .. }
                | s::StmtKind::Expression(_) => {}
                _ => return Err(vec![]),
            }
            ct_selection::walk_statement(self, statement, depth)
        }
    }
    let mut scan = Scalar {
        nodes: 0,
        left: limits.max_nodes,
        depth: limits.max_call_depth,
    };
    let success = module
        .items
        .iter()
        .all(|item| ct_selection::Visitor::item(&mut scan, item, 0).is_ok());
    (success, scan.nodes)
}

#[cfg(test)]
#[path = "incremental_tests.rs"]
mod tests;
