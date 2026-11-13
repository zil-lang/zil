//! Process-local reuse of successful scalar expansion from sealed parser output.
//! Receipts reproduce bounded work; they confer no type or safety authority.
use super::*;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Instant;
use zil_base::{FileId, Fingerprint, FingerprintBuilder, ModuleId, TargetProfile};
use zil_syntax::ParsedSource;

#[derive(Clone, Debug)]
pub struct ModuleRevision(Arc<()>);
impl PartialEq for ModuleRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ModuleRevision {}
/// Process-local scalar input identity, distinct from the current absolute
/// source binding. Only the successful closed producer can mint or preserve it.
/// Equal versions mean byte-identical function input under the same admitted
/// environment, not a Typed/Verified result or a public content-hash promise.
#[derive(Clone, Debug)]
pub struct FunctionInputRevision(Arc<()>);
impl PartialEq for FunctionInputRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for FunctionInputRevision {}
/// Sealed relative function-body input, distinct from raw NodeId placement.
/// Minted only by actual successful lowering; equality binds declaration bytes,
/// parser attribute/file/slot inputs and all generated-name dependencies in
/// FunctionLowerBinding. Consumers must separately bind the current complete
/// InputContext, owner, declaration identity, positions, target and quotas.
/// This is neither a content hash nor Types/Verify authority.
#[derive(Clone, Debug)]
pub struct FunctionBodyRevision(Arc<()>);
impl PartialEq for FunctionBodyRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for FunctionBodyRevision {}

/// Process-local identity of an actual declaration owner. Bodies and current
/// diagnostic bindings may change without changing this identity. Constructors
/// stay private: neither a source name nor a public hash can mint an owner.
#[derive(Clone, Debug)]
pub struct FunctionOwner(Arc<()>);
impl PartialEq for FunctionOwner {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for FunctionOwner {}

/// One successful producer epoch. It carries raw input identity only.
#[derive(Clone, Debug)]
pub struct ExpansionEpoch(Arc<()>);
impl PartialEq for ExpansionEpoch {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ExpansionEpoch {}

/// Complete closed producer environment, with import diagnostic offsets kept
/// in the current program separately. This is not type or Verify authority.
#[derive(Clone, Debug)]
pub struct InputContext(Arc<()>);
impl PartialEq for InputContext {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for InputContext {}

/// Stable complete declaration namespace, separate from signature contents.
/// Current signature checks and observed readers remain Types obligations.
#[derive(Clone, Debug)]
pub struct NamespaceContext(Arc<()>);
impl PartialEq for NamespaceContext {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for NamespaceContext {}

/// Sparse changes admitted by actual successful lowering. Current function
/// references are resolved from the exact immutable ProducedProgram, never from
/// caller supplied digests or a prior Typed result.
#[derive(Clone, Debug)]
pub struct FunctionInputChange {
    epoch: ExpansionEpoch,
    owner: FunctionOwner,
    module: usize,
    function: usize,
    body_changed: bool,
    interface_changed: bool,
    previous_span: Option<Span>,
    previous_node: Option<zil_base::NodeId>,
}
impl FunctionInputChange {
    pub fn owner(&self) -> &FunctionOwner {
        &self.owner
    }
    pub fn body_changed(&self) -> bool {
        self.body_changed
    }
    pub fn interface_changed(&self) -> bool {
        self.interface_changed
    }
    pub fn previous_span(&self) -> Option<Span> {
        self.previous_span
    }
    pub fn previous_node_id(&self) -> Option<zil_base::NodeId> {
        self.previous_node
    }
    pub fn module_index(&self) -> usize {
        self.module
    }
    pub fn function_index(&self) -> usize {
        self.function
    }
}
#[derive(Clone, Debug)]
pub struct FunctionChangeBatch {
    previous: Option<ExpansionEpoch>,
    current: ExpansionEpoch,
    context: InputContext,
    namespace: NamespaceContext,
    changes: Arc<[FunctionInputChange]>,
    modules: Arc<[usize]>,
}
impl FunctionChangeBatch {
    pub fn previous_epoch(&self) -> Option<&ExpansionEpoch> {
        self.previous.as_ref()
    }
    pub fn epoch(&self) -> &ExpansionEpoch {
        &self.current
    }
    pub fn context(&self) -> &InputContext {
        &self.context
    }
    pub fn namespace_context(&self) -> &NamespaceContext {
        &self.namespace
    }
    pub fn changes(&self) -> &[FunctionInputChange] {
        &self.changes
    }
    pub fn changed_modules(&self) -> &[usize] {
        &self.modules
    }
}
#[derive(Clone, Debug)]
pub struct ProducedProgram {
    core: Arc<CoreProgram>,
    revisions: BTreeMap<ModuleId, ModuleRevision>,
    functions: BTreeMap<ModuleId, Arc<BTreeMap<String, FunctionInput>>>,
    changes: FunctionChangeBatch,
}
impl ProducedProgram {
    pub fn program(&self) -> &Arc<CoreProgram> {
        &self.core
    }
    /// The producer's complete sparse delta from its previous successful epoch.
    /// A consumer must match both prior epoch and complete current context.
    pub fn change_batch(&self) -> &FunctionChangeBatch {
        &self.changes
    }
    pub fn changed_function(
        &self,
        change: &FunctionInputChange,
    ) -> Option<(&ProgramModule, &CoreFunction)> {
        // A change's private epoch binds these indexes to this exact immutable
        // input, including when callers clone the read-only event.
        if change.epoch != self.changes.current {
            return None;
        }
        let module = self.core.modules.get(change.module)?;
        Some((module, module.core.functions.get(change.function)?))
    }
    pub fn function_owner(&self, module: ModuleId, name: &str) -> Option<&FunctionOwner> {
        Some(&self.functions.get(&module)?.get(name)?.owner)
    }
    pub fn module_revision(&self, module: ModuleId) -> Option<&ModuleRevision> {
        self.revisions.get(&module)
    }
    /// Sealed scalar producer evidence for this exact declaration/body binding.
    /// Context, signatures, budgets and current safety checks remain separate.
    pub fn function_revision(&self, module: ModuleId, name: &str) -> Option<&ModuleRevision> {
        Some(&self.functions.get(&module)?.get(name)?.revision)
    }
    /// Sealed source input equality with absolute positions tracked separately.
    /// Current declaration IDs, context, signature reads and budgets are still
    /// consumers' obligations; this cannot retarget an arbitrary raw Core.
    pub fn function_input_revision(
        &self,
        module: ModuleId,
        name: &str,
    ) -> Option<&FunctionInputRevision> {
        Some(&self.functions.get(&module)?.get(name)?.input_revision)
    }
    /// The producer's closed body projection for this exact immutable input.
    /// Unlike the legacy input revision, unrelated raw NodeId relocation does
    /// not change this token. Generated temporary/discard naming dependencies
    /// remain part of its producer proof; current placement is not authorized.
    pub fn function_body_revision(
        &self,
        module: ModuleId,
        name: &str,
    ) -> Option<&FunctionBodyRevision> {
        Some(&self.functions.get(&module)?.get(name)?.body_revision)
    }
    /// Compiler-local fingerprint of current declaration bytes and absolute
    /// source binding; relocation still changes it. This is not a wire key.
    pub fn function_source_fingerprint(&self, module: ModuleId, name: &str) -> Option<Fingerprint> {
        Some(self.functions.get(&module)?.get(name)?.fingerprint)
    }
}
pub struct ExpandedProgram {
    modules: Vec<(ProgramModule, Arc<SourceMap>)>,
    revisions: BTreeMap<ModuleId, ModuleRevision>,
    functions: BTreeMap<ModuleId, Arc<BTreeMap<String, FunctionInput>>>,
    changes: FunctionChangeBatch,
}
impl ExpandedProgram {
    pub fn into_program(
        self,
        root: ModuleId,
        source_fingerprint: Fingerprint,
    ) -> (ProducedProgram, Vec<Arc<SourceMap>>) {
        let (modules, maps) = self.modules.into_iter().unzip();
        (
            ProducedProgram {
                core: Arc::new(CoreProgram {
                    root,
                    source_fingerprint,
                    modules,
                }),
                revisions: self.revisions,
                functions: self.functions,
                changes: self.changes,
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
// complete syntax clones. Deferred normalized hits retain recipes, not an
// extra normalized body; reconstruction work is reported separately and never
// debits the already-paid formal Normalize ledger again.
// `cloned_source_map_entries` counts copied mapping
// entries (including span markers), not all nested Core/type/string copies.
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
    reused_lower_items,
    executed_lower_items,
    copied_lower_map_entries,
    receipt_hits,
    receipt_nodes,
    receipt_budget_misses,
    lower_budget_misses,
    visited_nodes,
    syntax_clone_receipt_nodes,
    normalized_item_deep_clones,
    normalized_recipe_hits,
    normalized_materializations,
    normalized_reconstruction_nodes,
    normalized_reconstruction_work,
    owned_restore_items,
    plain_header_items,
    restored_module_deep_clones,
    retained_normalized_items,
    retained_restored_modules,
    retained_restored_items,
    restored_item_deep_clones,
    retained_lower_cores,
    retained_lower_source_maps,
    cloned_source_map_entries,
    cloned_source_bytes,
    function_input_hashes,
    function_input_reuses,
    function_input_text_comparisons,
    function_input_binding_hashes,
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
#[derive(Clone, Debug)]
struct HeaderInput(Arc<Vec<s::Item>>);
impl PartialEq for HeaderInput {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}
impl std::ops::Deref for HeaderInput {
    type Target = [s::Item];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[cfg(test)]
impl std::ops::DerefMut for HeaderInput {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0).as_mut_slice()
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Environment {
    sources: Vec<(ModuleId, FileId, String)>,
    headers: Vec<HeaderInput>,
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

    // This narrower comparison preserves only parser-bound function INPUT
    // evidence, never Normalize/Restore/materialization receipts or old output.
    // Every current function is still freshly lowered and then checked against
    // the real prior/current declaration bytes, IDs and file before a relative
    // input revision can survive. Exhaustive matching makes new environment or
    // import fields require an explicit decision here.
    fn function_inputs_compatible(&self, current: &Self) -> bool {
        self.projected_inputs_compatible(current, self.headers == current.headers)
    }
    fn namespace_compatible(&self, current: &Self) -> bool {
        let headers = self.headers.len() == current.headers.len()
            && self
                .headers
                .iter()
                .zip(&current.headers)
                .all(|(a, b)| namespace_headers(a, b));
        self.projected_inputs_compatible(current, headers)
    }
    fn projected_inputs_compatible(&self, current: &Self, same_headers: bool) -> bool {
        let Self {
            sources,
            headers: _,
            imports,
            limits,
            target,
            context,
        } = self;
        sources == &current.sources
            && same_headers
            && same_limit_domain(limits, &current.limits)
            && target == &current.target
            && context == &current.context
            && imports.len() == current.imports.len()
            && imports
                .iter()
                .zip(&current.imports)
                .all(|(prior, current)| {
                    prior.len() == current.len()
                        && prior.iter().zip(current).all(|(prior, current)| {
                            let CoreImport {
                                alias,
                                module,
                                span,
                            } = prior;
                            let CoreImport {
                                alias: current_alias,
                                module: current_module,
                                span: current_span,
                            } = current;
                            let Span {
                                file,
                                start: _,
                                end: _,
                            } = span;
                            let Span {
                                file: current_file,
                                start: _,
                                end: _,
                            } = current_span;
                            alias == current_alias
                                && module == current_module
                                && file == current_file
                        })
                })
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
    Headers,
    Distribute,
    Restore,
    RestoreItem,
}
#[derive(Clone, Copy)]
struct Receipt {
    nodes: usize,
    work: usize,
}
#[derive(Clone, Debug)]
pub(super) struct RestoreEpoch(Arc<()>);
impl Default for RestoreEpoch {
    fn default() -> Self {
        Self(Arc::new(()))
    }
}
impl PartialEq for RestoreEpoch {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for RestoreEpoch {}
#[derive(Clone)]
pub(super) struct RestoredItem {
    pub(super) item: Arc<s::Item>,
    before: RestoreEpoch,
    pub(super) after: RestoreEpoch,
    pub(super) added: Arc<[CoreImport]>,
}
#[derive(Clone, Debug)]
struct FunctionInput {
    // The declaration slice excludes other functions and includes all local
    // relative positions. The old Entry owns its real ParsedSource until this
    // epoch is replaced; no copied source buffer or independent history exists.
    id: zil_base::NodeId,
    ordinal: usize,
    owner: FunctionOwner,
    // Private body projection binds actual source bytes and declaration slot;
    // unlike the legacy input_revision it does not identify relocated NodeIds.
    body_revision: FunctionBodyRevision,
    interface_revision: FunctionInputRevision,
    lower_binding: FunctionLowerBinding,
    span: Span,
    text_fingerprint: Fingerprint,
    fingerprint: Fingerprint,
    revision: ModuleRevision,
    input_revision: FunctionInputRevision,
}
/// A complete successful normalized input is either retained syntax or a
/// private recipe. A recipe never grants access to restored syntax as normalized.
#[derive(Clone)]
pub(crate) enum NormalizedCache {
    Shared(Arc<s::Item>),
    Deferred(NormalizedRecipe),
}
#[derive(Clone)]
pub(crate) struct NormalizedRecipe {
    parsed: ParsedSource,
    ordinal: usize,
    span: Span,
    module: ModuleId,
    environment: Arc<Environment>,
    declaration: modules::scalar_products::Declaration,
}
impl NormalizedRecipe {
    pub(crate) fn span(&self) -> Span {
        self.span
    }
    pub(crate) fn module(&self) -> ModuleId {
        self.module
    }
    pub(crate) fn declaration(&self) -> &modules::scalar_products::Declaration {
        &self.declaration
    }
    pub(crate) fn source_item(&self) -> &s::Item {
        &self.parsed.parsed().module.items[self.ordinal]
    }
}
/// All effects whose absence admits a scalar Header replay are explicit. This
/// is published only after the real header visitor has succeeded.
#[derive(Clone, Copy)]
pub(crate) struct ScalarHeaderOutcome {
    pub(crate) has_where: bool,
    pub(crate) has_defaults: bool,
    pub(crate) has_associated: bool,
    pub(crate) has_generic_methods: bool,
    pub(crate) has_impl_recipes: bool,
    pub(crate) has_inherent: bool,
    pub(crate) has_alias: bool,
    pub(crate) has_parameters: bool,
    pub(crate) has_primitive_binders: bool,
    pub(crate) has_enum_profiles: bool,
    pub(crate) needs_source_identity: bool,
    pub(crate) has_borrowed_keys: bool,
}
impl ScalarHeaderOutcome {
    pub(crate) fn closed(self) -> bool {
        let Self {
            has_where,
            has_defaults,
            has_associated,
            has_generic_methods,
            has_impl_recipes,
            has_inherent,
            has_alias,
            has_parameters,
            has_primitive_binders,
            has_enum_profiles,
            needs_source_identity,
            has_borrowed_keys,
        } = self;
        !(has_where
            || has_defaults
            || has_associated
            || has_generic_methods
            || has_impl_recipes
            || has_inherent
            || has_alias
            || has_parameters
            || has_primitive_binders
            || has_enum_profiles
            || needs_source_identity
            || has_borrowed_keys)
    }
}
#[derive(Clone)]
struct Entry {
    parsed: ParsedSource,
    headers: HeaderInput,
    scalar: bool,
    scalar_nodes: u64,
    scalar_depth_limit: usize,
    receipts: Arc<BTreeMap<(Stage, usize), Receipt>>,
    normalized: Arc<BTreeMap<usize, NormalizedCache>>,
    scalar_headers: Arc<BTreeMap<usize, ScalarHeaderOutcome>>,
    restored_items: Arc<BTreeMap<usize, RestoredItem>>,
    restore_base: RestoreEpoch,
    shared_restored: Option<Arc<modules::SharedScalarModule>>,
    restored: Option<Arc<(s::Module, Vec<CoreImport>)>>,
    lower: Option<(Limits, Arc<ExpandedModule>)>,
    revision: ModuleRevision,
    functions: Arc<BTreeMap<String, FunctionInput>>,
}
#[derive(Clone, Default)]
pub struct ExpandSession {
    environment: Option<Environment>,
    normalization_environment: Option<Arc<Environment>>,
    input_context: Option<InputContext>,
    namespace_context: Option<NamespaceContext>,
    epoch: Option<ExpansionEpoch>,
    entries: BTreeMap<ModuleId, Entry>,
    scalar_inventory: Option<Arc<modules::ScalarInventory>>,
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
    /// Probe the sealed scalar input domain without lowering or publishing an
    /// epoch. This is eligibility only, never a receipt for later stage work.
    /// A retained successful classification avoids revisiting unchanged syntax.
    pub fn parsed_scalar(&self, parsed: &ParsedSource, limits: &Limits) -> bool {
        let syntax = parsed.parsed();
        if !syntax.diagnostics.is_empty()
            || !syntax.deferred_attributes.is_empty()
            || !syntax.cfg_removed.is_empty()
            || syntax.type_meta_candidate
        {
            return false;
        }
        if self.entries.get(&syntax.module.id).is_some_and(|entry| {
            entry.scalar
                && entry.parsed.same_revision(parsed)
                && (limits.max_nodes as u64) >= entry.scalar_nodes
                && limits.max_call_depth >= entry.scalar_depth_limit
        }) {
            return true;
        }
        scalar(&syntax.module, limits).0
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
                    headers: {
                        let current = if scalar {
                            module_headers(module)
                        } else {
                            Vec::new()
                        };
                        self.entries
                            .get(&module.id)
                            .map(|old| &old.headers)
                            .filter(|old| old.0.as_ref() == &current)
                            .cloned()
                            .unwrap_or_else(|| HeaderInput(Arc::new(current)))
                    },
                    scalar,
                    scalar_nodes: nodes,
                    scalar_depth_limit: limits.max_call_depth,
                    receipts: Arc::default(),
                    normalized: Arc::default(),
                    scalar_headers: Arc::default(),
                    restored_items: Arc::default(),
                    restore_base: self
                        .entries
                        .get(&module.id)
                        .map(|entry| entry.restore_base.clone())
                        .unwrap_or_default(),
                    shared_restored: None,
                    restored: None,
                    lower: None,
                    revision: ModuleRevision(Arc::new(())),
                    // Only the complete compatible environment below can keep
                    // a previous function's sealed lowering input evidence.
                    functions: self
                        .entries
                        .get(&module.id)
                        .map(|entry| entry.functions.clone())
                        .unwrap_or_default(),
                }
            };
            if !entry.scalar || entries.insert(module.id, entry).is_some() {
                return Ok(None);
            }
        }
        let headers = parsed
            .iter()
            .map(|parsed| entries[&parsed.source().module_id].headers.clone())
            .collect();
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
            let preserve_function_inputs = self
                .environment
                .as_ref()
                .is_some_and(|previous| previous.namespace_compatible(&environment));
            for entry in entries.values_mut() {
                entry.receipts = Arc::default();
                entry.normalized = Arc::default();
                entry.scalar_headers = Arc::default();
                entry.restored_items = Arc::default();
                entry.shared_restored = None;
                entry.restore_base = RestoreEpoch::default();
                entry.restored = None;
                entry.lower = None;
                entry.revision = ModuleRevision(Arc::new(()));
                if !preserve_function_inputs {
                    entry.functions = Arc::default();
                }
            }
        }
        if self
            .environment
            .as_ref()
            .is_some_and(|old| old.compatible(&environment))
        {
            for (module_id, entry) in &mut entries {
                let Some(old) = self.entries.get(module_id) else {
                    continue;
                };
                if old.parsed.same_revision(&entry.parsed)
                    || !old.parsed.same_attribute_context(&entry.parsed)
                {
                    continue;
                }
                for (item, prior) in entry
                    .parsed
                    .parsed()
                    .module
                    .items
                    .iter()
                    .zip(&old.parsed.parsed().module.items)
                {
                    if item.span != prior.span || item.span.file != entry.parsed.source().file_id {
                        continue;
                    }
                    let current = entry
                        .parsed
                        .source()
                        .text
                        .get(item.span.start..item.span.end);
                    let previous = old
                        .parsed
                        .source()
                        .text
                        .get(prior.span.start..prior.span.end);
                    if current.is_none() || current != previous {
                        continue;
                    }
                    if let Some(normalized) = old.normalized.get(&item.span.start) {
                        Arc::make_mut(&mut entry.normalized)
                            .insert(item.span.start, normalized.clone());
                    }
                    if let Some(outcome) = old.scalar_headers.get(&item.span.start) {
                        Arc::make_mut(&mut entry.scalar_headers).insert(item.span.start, *outcome);
                    }
                    if let Some(restored) = old.restored_items.get(&item.span.start) {
                        Arc::make_mut(&mut entry.restored_items)
                            .insert(item.span.start, restored.clone());
                    }
                    for stage in [
                        Stage::Normalize,
                        Stage::Prepare,
                        Stage::Lifetimes,
                        Stage::Headers,
                        Stage::Distribute,
                        Stage::RestoreItem,
                    ] {
                        if let Some(receipt) = old.receipts.get(&(stage, item.span.start)) {
                            Arc::make_mut(&mut entry.receipts)
                                .insert((stage, item.span.start), *receipt);
                        }
                    }
                }
            }
        }
        if let Some(start) = context_clock {
            self.stats.context_ns += nanos(start);
        }
        let assembly = self.profiling.then(Instant::now);
        let sources: Vec<_> = parsed.iter().map(|p| p.source()).collect();
        if let Some(start) = assembly {
            self.stats.assembly_ns += nanos(start);
        }
        let syntax: Vec<_> = parsed.iter().map(|p| &p.parsed().module).collect();
        let files = parsed
            .iter()
            .map(|p| (p.source().file_id, p.source().module_id))
            .collect();
        // Keep tentative publication local. Errors/fallback never replace the
        // last successful inventory or pair it with a different environment.
        let mut scalar_inventory = self
            .environment
            .as_ref()
            .filter(|old| old.compatible(&environment))
            .and(self.scalar_inventory.as_ref())
            .cloned();
        // Share the exact immutable complete environment binding across warm
        // requests; no per-item deep Environment comparison on an exact hit.
        let normalization_environment = self
            .normalization_environment
            .as_ref()
            .filter(|old| old.as_ref() == &environment)
            .cloned()
            .unwrap_or_else(|| Arc::new(environment.clone()));
        let mut reuse = Reuse {
            normalization_environment: normalization_environment.clone(),
            scalar_inventory: &mut scalar_inventory,
            prior_entries: &self.entries,
            item_reuse: self
                .environment
                .as_ref()
                .is_some_and(|old| old.function_inputs_compatible(&environment)),
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
        let same_legacy_context = self
            .environment
            .as_ref()
            .is_some_and(|old| old.function_inputs_compatible(&environment));
        let namespace_context = self
            .environment
            .as_ref()
            .filter(|old| old.namespace_compatible(&environment))
            .and(self.namespace_context.as_ref())
            .cloned()
            .unwrap_or_else(|| NamespaceContext(Arc::new(())));
        let input_context = self
            .environment
            .as_ref()
            .filter(|old| old.function_inputs_compatible(&environment))
            .and(self.input_context.as_ref())
            .cloned()
            .unwrap_or_else(|| InputContext(Arc::new(())));
        let epoch = ExpansionEpoch(Arc::new(()));
        let mut changes = Vec::new();
        let mut changed_modules = Vec::new();
        let mut functions = BTreeMap::new();
        for (module_index, (module, _)) in modules.iter().enumerate() {
            let Some(entry) = entries.get_mut(&module.core.module_id) else {
                return Ok(None);
            };
            if self
                .entries
                .get(&module.core.module_id)
                .is_some_and(|old| old.revision == entry.revision)
            {
                self.stats.function_input_reuses += entry.functions.len() as u64;
                functions.insert(module.core.module_id, entry.functions.clone());
                continue;
            }
            changed_modules.push(module_index);
            let source = entry.parsed.source();
            let mut next_functions = BTreeMap::new();
            for (function_index, function) in module.core.functions.iter().enumerate() {
                // This closed scalar producer has rejected constants, CT,
                // generics, traits, captures and all other rewriting domains.
                // The compatible environment above binds current declarations,
                // imports, target and source identity. Compare real old/current
                // parser-owned declaration bytes, never a caller-provided hash
                // or an old Typed result, before preserving relative input.
                let Some(text) = source
                    .text
                    .get(function.span.start..function.span.end)
                    .filter(|_| function.span.file == source.file_id)
                else {
                    return Ok(None);
                };
                let Some(lower_binding) = entry
                    .lower
                    .as_ref()
                    .and_then(|(_, lower)| lower.function_lower_bindings.get(&function.id))
                    .copied()
                else {
                    return Ok(None);
                };
                let declaration = entry.functions.get(&function.name).filter(|old| {
                    old.ordinal == function_index && old.span.file == function.span.file
                });
                let previous_owner = declaration.map(|old| old.owner.clone());
                let same_body = declaration.and_then(|previous| {
                    let old = self.entries.get(&source.module_id)?;
                    let old_source = old.parsed.source();
                    if !old.parsed.same_attribute_context(&entry.parsed)
                        || old_source.file_id != source.file_id
                    {
                        return None;
                    }
                    let old_text = old_source
                        .text
                        .get(previous.span.start..previous.span.end)?;
                    self.stats.function_input_text_comparisons += 1;
                    (old_text == text
                        && previous.lower_binding.same_body_input(
                            lower_binding,
                            previous.id,
                            function.id,
                        ))
                    .then_some(previous)
                });
                // Preserve the legacy absolute/raw input contract exactly.
                // The new private body projection above has a distinct domain.
                let previous =
                    same_body.filter(|previous| same_legacy_context && previous.id == function.id);
                let previous_interface = declaration.and_then(|old_input| {
                    let old_module = self
                        .entries
                        .get(&source.module_id)?
                        .lower
                        .as_ref()?
                        .1
                        .core
                        .as_ref();
                    let old_function = old_module.functions.get(old_input.ordinal)?;
                    same_function_interface(old_function, function).then_some(old_input)
                });
                let text_fingerprint = previous.map_or_else(
                    || {
                        // Counts full declaration-text hashing, separately from the
                        // small current-position binding digest below.
                        self.stats.function_input_hashes += 1;
                        Fingerprint::of(text.as_bytes())
                    },
                    |previous| {
                        self.stats.function_input_reuses += 1;
                        previous.text_fingerprint
                    },
                );
                let input = if let Some(previous) = previous.filter(|p| p.span == function.span) {
                    previous.clone()
                } else {
                    let mut fingerprint = FingerprintBuilder::new();
                    fingerprint.update(b"ZIL-SCALAR-FUNCTION-SOURCE-2\0");
                    source.module_id.hash(&mut fingerprint);
                    source.file_id.hash(&mut fingerprint);
                    function.id.hash(&mut fingerprint);
                    function.span.start.hash(&mut fingerprint);
                    function.span.end.hash(&mut fingerprint);
                    function.name.hash(&mut fingerprint);
                    fingerprint.update(&text_fingerprint.0);
                    self.stats.function_input_binding_hashes += 1;
                    FunctionInput {
                        id: function.id,
                        ordinal: function_index,
                        lower_binding,
                        interface_revision: previous_interface.map_or_else(
                            || FunctionInputRevision(Arc::new(())),
                            |old| old.interface_revision.clone(),
                        ),
                        body_revision: same_body.map_or_else(
                            || FunctionBodyRevision(Arc::new(())),
                            |old| old.body_revision.clone(),
                        ),
                        owner: previous_owner.unwrap_or_else(|| FunctionOwner(Arc::new(()))),
                        span: function.span,
                        text_fingerprint,
                        fingerprint: fingerprint.digest(),
                        revision: ModuleRevision(Arc::new(())),
                        input_revision: previous.map_or_else(
                            || FunctionInputRevision(Arc::new(())),
                            |previous| previous.input_revision.clone(),
                        ),
                    }
                };
                let old = entry.functions.get(&function.name);
                if old.is_none_or(|old| {
                    old.owner != input.owner
                        || old.body_revision != input.body_revision
                        || old.interface_revision != input.interface_revision
                        || old.span != input.span
                        || old.id != input.id
                }) {
                    changes.push(FunctionInputChange {
                        epoch: epoch.clone(),
                        owner: input.owner.clone(),
                        module: module_index,
                        function: function_index,
                        body_changed: old
                            .is_none_or(|old| old.body_revision != input.body_revision),
                        interface_changed: old
                            .is_none_or(|old| old.interface_revision != input.interface_revision),
                        previous_span: old.map(|old| old.span),
                        previous_node: old.map(|old| old.id),
                    });
                }
                next_functions.insert(function.name.clone(), input);
            }
            entry.functions = Arc::new(next_functions);
            functions.insert(source.module_id, entry.functions.clone());
        }
        let revisions = entries
            .iter()
            .map(|(id, entry)| (*id, entry.revision.clone()))
            .collect();
        let changes = FunctionChangeBatch {
            previous: self.epoch.replace(epoch.clone()),
            current: epoch,
            context: input_context.clone(),
            namespace: namespace_context.clone(),
            changes: changes.into(),
            modules: changed_modules.into(),
        };
        self.entries = entries;
        self.scalar_inventory = scalar_inventory;
        self.environment = Some(environment);
        self.normalization_environment = Some(normalization_environment);
        self.input_context = Some(input_context);
        self.namespace_context = Some(namespace_context);
        Ok(Some(ExpandedProgram {
            modules,
            revisions,
            functions,
            changes,
        }))
    }
}
fn namespace_headers(left: &[s::Item], right: &[s::Item]) -> bool {
    fn function(left: &s::Function, right: &s::Function) -> bool {
        let s::Function {
            name,
            generics,
            params: _,
            return_type: _,
            where_clause,
            contract,
            body: _,
            span: _,
        } = left;
        let s::Function {
            name: rn,
            generics: rg,
            params: _,
            return_type: _,
            where_clause: rw,
            contract: rc,
            body: _,
            span: _,
        } = right;
        name == rn && generics == rg && where_clause == rw && contract == rc
    }
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            let s::Item {
                kind,
                attributes,
                public,
                span: _,
            } = left;
            let s::Item {
                kind: rk,
                attributes: ra,
                public: rp,
                span: _,
            } = right;
            attributes == ra
                && public == rp
                && match (kind, rk) {
                    (s::ItemKind::Function(a), s::ItemKind::Function(b)) => function(a, b),
                    (
                        s::ItemKind::Extern {
                            abi: _,
                            function: a,
                        },
                        s::ItemKind::Extern {
                            abi: _,
                            function: b,
                        },
                    ) => function(a, b),
                    (s::ItemKind::Import(a), s::ItemKind::Import(b)) => a == b,
                    _ => false,
                }
        })
}
fn same_function_interface(left: &CoreFunction, right: &CoreFunction) -> bool {
    let CoreFunction {
        storage_life,
        call_kind,
        life,
        type_parameters,
        state_parameters,
        protocol_contract,
        callable_return,
        id: _,
        name,
        callable_parameters,
        foreign,
        parameters,
        return_type,
        result_mode,
        body: _,
        receiver,
        span: _,
    } = left;
    let CoreFunction {
        storage_life: rs,
        call_kind: rc,
        life: rl,
        type_parameters: rt,
        state_parameters: rsp,
        protocol_contract: rp,
        callable_return: rcr,
        id: _,
        name: rn,
        callable_parameters: rcp,
        foreign: rf,
        parameters: rpar,
        return_type: rret,
        result_mode: rm,
        body: _,
        receiver: rr,
        span: _,
    } = right;
    storage_life == rs
        && call_kind == rc
        && life == rl
        && type_parameters == rt
        && state_parameters == rsp
        && protocol_contract == rp
        && callable_return == rcr
        && name == rn
        && callable_parameters == rcp
        && foreign == rf
        && return_type == rret
        && result_mode == rm
        && receiver == rr
        && parameters.len() == rpar.len()
        && parameters.iter().zip(rpar).all(|(a, b)| {
            let CoreParameter {
                name,
                ty,
                mode,
                span: _,
            } = a;
            let CoreParameter {
                name: rn,
                ty: rt,
                mode: rm,
                span: _,
            } = b;
            name == rn && ty == rt && mode == rm
        })
}
fn module_headers(module: &s::Module) -> Vec<s::Item> {
    let mut module_headers = Vec::new();
    for item in &module.items {
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
    module_headers
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
    normalization_environment: Arc<Environment>,
    scalar_inventory: &'a mut Option<Arc<modules::ScalarInventory>>,
    prior_entries: &'a BTreeMap<ModuleId, Entry>,
    item_reuse: bool,
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
    pub(super) fn scalar_inventory(
        &mut self,
        budget: &mut generics::Budget<'_>,
    ) -> Option<Arc<modules::ScalarInventory>> {
        let inventory = self.scalar_inventory.as_ref()?;
        inventory.replay(budget).then(|| inventory.clone())
    }
    pub(super) fn save_scalar_inventory(&mut self, inventory: Arc<modules::ScalarInventory>) {
        *self.scalar_inventory = Some(inventory);
    }
    pub(super) fn item_lower_plan(&self, file: FileId) -> Option<item_lowering::ItemLowerPlan> {
        if !self.item_reuse {
            return None;
        }
        let module = self.files.get(&file)?;
        let old = self.prior_entries.get(module)?;
        let current = self.entries.get(module)?;
        if !old.parsed.same_attribute_context(&current.parsed) {
            return None;
        }
        Some(item_lowering::ItemLowerPlan {
            old: old.lower.as_ref()?.1.clone(),
            source: old.parsed.clone(),
        })
    }
    pub(super) fn restore_base(&mut self, file: FileId) -> RestoreEpoch {
        self.entry(file).restore_base.clone()
    }
    pub(super) fn shared_restored(
        &mut self,
        file: FileId,
    ) -> Option<Arc<modules::SharedScalarModule>> {
        self.entry(file).shared_restored.clone()
    }
    pub(super) fn save_shared_restored(
        &mut self,
        file: FileId,
        module: Arc<modules::SharedScalarModule>,
    ) {
        self.entry(file).shared_restored = Some(module);
    }
    pub(super) fn restored_item(
        &mut self,
        span: Span,
        epoch: &RestoreEpoch,
    ) -> Option<RestoredItem> {
        self.entry(span.file)
            .restored_items
            .get(&span.start)
            .filter(|item| &item.before == epoch)
            .cloned()
    }
    pub(super) fn retain_restored_item(&mut self) {
        self.stats.retained_restored_items += 1;
    }
    pub(super) fn restored_item_cloned(&mut self) {
        self.stats.restored_item_deep_clones += 1;
    }
    pub(super) fn save_restored_item(
        &mut self,
        span: Span,
        item: Arc<s::Item>,
        before: RestoreEpoch,
        added: &[CoreImport],
    ) -> RestoreEpoch {
        let after = if added.is_empty() {
            before.clone()
        } else {
            self.files
                .get(&span.file)
                .and_then(|module| self.prior_entries.get(module))
                .and_then(|entry| entry.restored_items.get(&span.start))
                .filter(|old| old.before == before && old.added.as_ref() == added)
                .map(|old| old.after.clone())
                .unwrap_or_default()
        };
        Arc::make_mut(&mut self.entry(span.file).restored_items).insert(
            span.start,
            RestoredItem {
                item,
                before,
                after: after.clone(),
                added: added.into(),
            },
        );
        after
    }
    pub(super) fn record_restore_module(
        &mut self,
        span: Span,
        before: Limits,
        budget: &generics::Budget<'_>,
    ) {
        let after = budget.remaining_limits();
        Arc::make_mut(&mut self.entry(span.file).receipts).insert(
            (Stage::Restore, span.start),
            Receipt {
                nodes: before.max_nodes - after.max_nodes,
                work: before.max_analysis_steps - after.max_analysis_steps,
            },
        );
    }
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
        Arc::make_mut(&mut self.entry(span.file).receipts).insert((stage, span.start), receipt);
        if stage == Stage::Headers {
            self.stats.plain_header_items += 1;
        }
        if self.executed.insert((stage, span.file)) {
            match stage {
                Stage::Preflight => self.stats.preflight_modules += 1,
                Stage::Normalize => self.stats.normalize_modules += 1,
                Stage::Prepare => self.stats.plain_prepare_modules += 1,
                Stage::Lifetimes => self.stats.plain_lifetimes_modules += 1,
                Stage::Restore | Stage::RestoreItem => self.stats.restore_modules += 1,
                Stage::Headers => {}
                Stage::Distribute => {}
            }
        }
        if let Some(start) = clock {
            self.stats.reuse_ns += nanos(start);
        }
    }
    pub(crate) fn normalized(&mut self, span: Span) -> Option<Arc<s::Item>> {
        let clock = self.clock();
        let value =
            self.entry(span.file)
                .normalized
                .get(&span.start)
                .and_then(|cached| match cached {
                    NormalizedCache::Shared(item) => Some(item.clone()),
                    NormalizedCache::Deferred(_) => None,
                });
        if let Some(start) = clock {
            self.stats.reuse_ns += nanos(start);
        }
        value
    }
    /// Rebind a transferred recipe to this request's actual parser owner,
    /// exact slot/span/text/attributes and complete normalization environment.
    /// Namespace/function-input compatibility alone never reaches this path.
    pub(crate) fn scalar_normalized(
        &mut self,
        span: Span,
        ordinal: usize,
    ) -> Option<NormalizedCache> {
        let environment = self.normalization_environment.clone();
        let entry = self.entry(span.file);
        let cached = entry.normalized.get(&span.start)?.clone();
        match cached {
            NormalizedCache::Shared(item) => Some(NormalizedCache::Shared(item)),
            NormalizedCache::Deferred(mut recipe) => {
                let current = entry.parsed.parsed().module.items.get(ordinal)?;
                if recipe.ordinal != ordinal
                    || recipe.span != span
                    || current.span != span
                    || recipe.module != entry.parsed.source().module_id
                    || span.file != entry.parsed.source().file_id
                    || (!Arc::ptr_eq(&recipe.environment, &environment)
                        && !recipe.environment.compatible(&environment))
                    || (!recipe.parsed.same_revision(&entry.parsed)
                        && (!recipe.parsed.same_attribute_context(&entry.parsed)
                            || recipe
                                .parsed
                                .source()
                                .text
                                .get(span.start..span.end)
                                .is_none()
                            || recipe.parsed.source().text.get(span.start..span.end)
                                != entry.parsed.source().text.get(span.start..span.end)))
                {
                    return None;
                }
                if recipe.parsed.same_revision(&entry.parsed)
                    && Arc::ptr_eq(&recipe.environment, &environment)
                {
                    return Some(NormalizedCache::Deferred(recipe));
                }
                recipe.parsed = entry.parsed.clone();
                recipe.environment = environment;
                Arc::make_mut(&mut entry.normalized)
                    .insert(span.start, NormalizedCache::Deferred(recipe.clone()));
                Some(NormalizedCache::Deferred(recipe))
            }
        }
    }
    pub(crate) fn save_normalized_recipe(
        &mut self,
        span: Span,
        ordinal: usize,
        declaration: modules::scalar_products::Declaration,
    ) {
        let environment = self.normalization_environment.clone();
        let entry = self.entry(span.file);
        let recipe = NormalizedRecipe {
            parsed: entry.parsed.clone(),
            ordinal,
            span,
            module: entry.parsed.source().module_id,
            environment,
            declaration,
        };
        Arc::make_mut(&mut entry.normalized).insert(span.start, NormalizedCache::Deferred(recipe));
    }
    pub(crate) fn normalized_recipe_retained(&mut self) {
        self.stats.normalized_recipe_hits += 1;
        self.retain_normalized();
    }
    pub(crate) fn normalized_reconstructed(&mut self, before: &Limits, after: &Limits) {
        self.stats.normalized_materializations += 1;
        self.stats.normalized_reconstruction_nodes += (before.max_nodes - after.max_nodes) as u64;
        self.stats.normalized_reconstruction_work +=
            (before.max_analysis_steps - after.max_analysis_steps) as u64;
    }
    pub(crate) fn normalized_reconstruction_matches(
        &mut self,
        span: Span,
        before: &Limits,
        after: &Limits,
    ) -> bool {
        self.entry(span.file)
            .receipts
            .get(&(Stage::Normalize, span.start))
            .is_some_and(|receipt| {
                receipt.nodes == before.max_nodes - after.max_nodes
                    && receipt.work == before.max_analysis_steps - after.max_analysis_steps
            })
    }
    pub(crate) fn owned_restore_item(&mut self) {
        self.stats.owned_restore_items += 1;
    }
    pub(crate) fn replay_scalar_headers(
        &mut self,
        span: Span,
        budget: &mut generics::Budget<'_>,
    ) -> bool {
        if !self
            .entry(span.file)
            .scalar_headers
            .get(&span.start)
            .is_some_and(|outcome| outcome.closed())
        {
            return false;
        }
        self.replay(Stage::Headers, span, budget)
    }
    pub(crate) fn save_scalar_headers(&mut self, span: Span, outcome: ScalarHeaderOutcome) {
        if outcome.closed() {
            Arc::make_mut(&mut self.entry(span.file).scalar_headers).insert(span.start, outcome);
        }
    }
    pub(crate) fn normalized_input_cloned(&mut self) {
        self.stats.normalized_item_deep_clones += 1;
    }
    pub(crate) fn retain_normalized(&mut self) {
        self.stats.retained_normalized_items += 1;
    }
    pub(crate) fn retain_restored(&mut self) {
        self.stats.retained_restored_modules += 1;
    }
    pub(crate) fn save_normalized(&mut self, span: Span, item: &s::Item) {
        let clock = self.clock();
        let value = Arc::new(item.clone());
        self.assembly(clock);
        Arc::make_mut(&mut self.entry(span.file).normalized)
            .insert(span.start, NormalizedCache::Shared(value));
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
    pub(crate) fn lower(&mut self, file: FileId, limits: &Limits) -> Option<ExpandedModule> {
        let clock = self.clock();
        let candidate = self
            .entry(file)
            .lower
            .as_ref()
            .map(|(old, value)| (old.clone(), value.clone()));
        let cached = candidate.and_then(|(old, value)| {
            if !same_limit_domain(&old, limits) {
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
        let clock = self.clock();
        let expanded = cached.as_ref().clone();
        self.assembly(clock);
        self.stats.reused_lower_modules += 1;
        self.stats.retained_lower_cores += 1;
        self.stats.retained_lower_source_maps += 1;
        Some(expanded)
    }
    pub(crate) fn lower_executed(&mut self) {
        self.stats.lower_modules += 1;
    }
    pub(crate) fn save_lower(&mut self, file: FileId, limits: &Limits, expanded: &ExpandedModule) {
        let clock = self.clock();
        let value = Arc::new(expanded.clone());
        self.assembly(clock);
        self.stats.reused_lower_items += value.reused_lower_items as u64;
        self.stats.executed_lower_items += value.executed_lower_items as u64;
        self.stats.copied_lower_map_entries += value.copied_lower_map_entries as u64;
        let entry = self.entry(file);
        entry.lower = Some((limits.clone(), value));
        entry.revision = ModuleRevision(Arc::new(()));
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
