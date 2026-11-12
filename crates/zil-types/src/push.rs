//! Actual sparse raw CFG updates from sealed producer inputs. This owns no
//! verification capability. Unknown state transitions use the current checker.
use super::*;
use std::collections::BTreeSet;
use std::sync::Arc;
use zil_base::{ModuleId, NodeId};
use zil_expand::{
    ExpansionEpoch, FunctionBodyRevision, FunctionOwner, InputContext, NamespaceContext,
    ProducedProgram,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypesPushStats {
    pub context_rebuilds: usize,
    pub sparse_batches: usize,
    pub body_checks: usize,
    pub position_rebindings: usize,
    pub retained_functions: usize,
    pub receipt_tree_updates: usize,
    pub signature_updates: usize,
    pub signature_readers_checked: usize,
}
impl TypesPushStats {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"context_rebuilds\":{},\"sparse_batches\":{},\"body_checks\":{},\"position_rebindings\":{},\"retained_functions\":{},\"receipt_tree_updates\":{},\"signature_updates\":{},\"signature_readers_checked\":{}}}",
            self.context_rebuilds,
            self.sparse_batches,
            self.body_checks,
            self.position_rebindings,
            self.retained_functions,
            self.receipt_tree_updates,
            self.signature_updates,
            self.signature_readers_checked
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedFunctionChange {
    pub function: FunctionId,
    /// The old dense slot is absent from the complete current snapshot.
    pub removed: bool,
    /// Complete actual relative CFG inequality, conservatively including local
    /// diagnostic positions. Equal public digests never authorize this result.
    pub result_changed: bool,
    pub position_changed: bool,
}
#[derive(Clone, Debug)]
pub struct TypedUpdate {
    pub program: Arc<TypedProgram>,
    pub changes: Vec<TypedFunctionChange>,
    pub stats: TypesPushStats,
}

#[derive(Clone, Default)]
pub struct TypesPushSession {
    current: Option<Arc<Context>>,
}
impl TypesPushSession {
    pub fn clear(&mut self) {
        self.current = None;
    }
    /// Retain raw acceleration state only for the exact successful program
    /// owner returned by this session. Structural equality, a public hash or
    /// a separately decoded program cannot attach an old context checkpoint.
    /// The caller still binds its complete current query receipt and key.
    pub fn checkpoint_for(&self, program: &Arc<TypedProgram>) -> Option<Self> {
        let current = self.current.as_ref()?;
        Arc::ptr_eq(&current.program, program).then(|| self.clone())
    }

    /// Stage the complete request before replacing the successful raw owner.
    /// Wrong history, target, quotas or declaration contexts rebuild current
    /// inputs. Errors leave the prior owner intact, including its shared CFGs.
    pub fn apply(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<TypedUpdate> {
        if let Some(update) = self.try_sparse(prepared, target, limits) {
            let (context, update) = update;
            self.current = Some(Arc::new(context));
            return Ok(update);
        }
        self.rebuild(prepared, target, limits)
    }
    fn rebuild(
        &mut self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<TypedUpdate> {
        let mut recording = Recording::default();
        let result = if prepared.identity_inference(limits) && prepared.produced().is_some() {
            let resolved = aggregates::resolve_prepared_program_recording(
                prepared.program(),
                limits,
                Some(&mut recording.shapes),
            )?;
            let eligible = matches!(resolved, std::borrow::Cow::Borrowed(_));
            program::check_resolved_program_recording(
                &resolved,
                target,
                limits,
                None,
                eligible.then_some(&mut recording),
            )?
        } else {
            check_prepared_program(prepared, target, limits)?
        };
        let program = Arc::new(result);
        let context = prepared
            .produced()
            .and_then(|produced| recording.finish(produced, program.clone(), target, limits));
        let mut changes: Vec<_> = program
            .module
            .functions
            .iter()
            .map(|f| TypedFunctionChange {
                function: f.id,
                removed: false,
                result_changed: true,
                position_changed: true,
            })
            .collect();
        if let Some(previous) = &self.current {
            for function in previous
                .program
                .module
                .functions
                .iter()
                .skip(program.module.functions.len())
            {
                changes.push(TypedFunctionChange {
                    function: function.id,
                    removed: true,
                    result_changed: true,
                    position_changed: true,
                });
            }
        }
        let update = TypedUpdate {
            stats: TypesPushStats {
                context_rebuilds: 1,
                body_checks: program.module.functions.len(),
                ..TypesPushStats::default()
            },
            program,
            changes,
        };
        self.current = context.map(Arc::new);
        Ok(update)
    }
    fn try_sparse(
        &self,
        prepared: &PreparedProgram,
        target: &TargetProfile,
        limits: &Limits,
    ) -> Option<(Context, TypedUpdate)> {
        let prior = self.current.as_ref()?;
        let produced = prepared.produced()?;
        let batch = produced.change_batch();
        if !prepared.identity_inference(limits)
            || &prior.limits != limits
            || &prior.target != target
            || batch.namespace_context() != &prior.namespace
            || prepared.program().root != prior.program.module.module_id
        {
            return None;
        }
        let continuous = batch.previous_epoch() == Some(&prior.epoch);
        if !continuous && batch.context() != &prior.input_context {
            return None;
        }
        if batch.context() != &prior.input_context
            && !batch
                .changes()
                .iter()
                .any(|change| change.interface_changed())
        {
            return None;
        }
        let mut rows = prior.rows.clone();
        let mut signature_values = prior.signature_values.clone();
        let mut signature_costs = prior.signature_costs.clone();
        let mut signature_work = prior.signature_work;
        let mut readers = prior.readers.clone();
        let mut program = prior.program.as_ref().clone();
        let mut work = if continuous {
            BTreeMap::<usize, Work>::new()
        } else {
            // A failed Types request may leave a newer successful raw producer
            // epoch. Its last delta is not a delta from our successful CFGs.
            prior.bridge_inputs(produced)?
        };
        let mut signature_modules = BTreeSet::new();
        let mut stats = TypesPushStats {
            sparse_batches: 1,
            retained_functions: program.module.functions.len(),
            ..TypesPushStats::default()
        };
        // Names/IDs resolve through the immutable namespace table; actual
        // signature contents resolve through current persistent value leaves.
        for change in batch.changes().iter().filter(|_| continuous) {
            let (module, function) = produced.changed_function(change)?;
            let id = prior
                .tables
                .get(&module.core.module_id)?
                .get(&function.name)?
                .concrete_id(function.span)
                .ok()?;
            let index = usize::try_from(id.0).ok()?;
            let old = rows.get(index)?;
            if old.id != id
                || old.module != module.core.module_id
                || old.name != function.name
                || old.owner.as_ref()? != change.owner()
                || change.previous_span() != Some(old.span)
                || change.previous_node_id() != Some(old.node)
                || old.location != (change.module_index(), change.function_index())
            {
                return None;
            }
            let item = work.entry(index).or_insert(Work {
                body: false,
                binding: false,
                reader: false,
            });
            item.body |= change.body_changed() || change.interface_changed();
            item.binding = true;
            if change.interface_changed() {
                let (signature, cost) =
                    checked_signature(module, change.function_index(), id, limits)?;
                let old_signature = signature_values.get(index)?;
                signature_work = signature_work
                    .checked_sub(*signature_costs.get(index)?)?
                    .checked_add(cost)?;
                signature_costs.set_owner(index, Arc::new(cost));
                if &signature != old_signature {
                    signature_values.set_owner(index, Arc::new(signature));
                    stats.signature_updates += 1;
                    for &reader in prior.readers.get(index)? {
                        let entry = work.entry(reader.0 as usize).or_insert(Work {
                            body: true,
                            binding: false,
                            reader: true,
                        });
                        entry.body = true;
                        entry.reader = true;
                    }
                }
                signature_modules.insert(module.core.module_id);
            }
        }
        let mut changes = Vec::with_capacity(work.len());
        // Failure never publishes speculative state or queue order as a
        // diagnostic. It restarts the original complete ordered checker.
        for (index, work) in work {
            let old = rows.get(index)?;
            let module = prepared.program().modules.get(old.location.0)?;
            let function = module.core.functions.get(old.location.1)?;
            if module.core.module_id != old.module
                || function.name != old.name
                || produced.function_owner(old.module, &old.name) != old.owner.as_ref()
                || (!work.binding && (function.id != old.node || function.span != old.span))
            {
                return None;
            }
            let id = old.id;
            let table = prior.tables.get(&old.module)?;
            let before = program.module.functions.get(index)?;
            let mut row = old.clone();
            let mut typed;
            let result_changed;
            if work.body {
                row.shape = scalar_shapes::validate_function(function, limits)?.ok()?;
                row.names = names::check_function(function, limits).ok()?;
                row.header = function
                    .name
                    .len()
                    .checked_add(function.parameters.len())?
                    .checked_add(1)?;
                let mut state = closures::State::new(program.module.functions.len(), limits);
                let imports = program::imports_for(module, &prior.tables).ok()?;
                let observed = std::cell::RefCell::new(BTreeSet::new());
                let mut builder = Builder::new(function, id, table, limits, &mut state).ok()?;
                builder.imports = Some(&imports);
                builder.signature_values = Some(&signature_values);
                builder.observed_signatures = Some(&observed);
                typed = builder.finish(&function.body).ok()?;
                if !state.push_closed_state() || !closed_function(&typed) {
                    return None;
                }
                row.cfg = state.accounting();
                row.post = PostReceipt::check(&typed, limits)?;
                row.visibility = instruction_count(&typed)?;
                let observed = observed.into_inner();
                if observed != *old.observed {
                    for &callee in old.observed.symmetric_difference(&observed) {
                        let callee = callee.0 as usize;
                        let mut set = readers.get(callee)?.clone();
                        if observed.contains(&FunctionId(callee as u32)) {
                            set.insert(id);
                        } else {
                            set.remove(&id);
                        }
                        readers.set_owner(callee, Arc::new(set));
                    }
                    row.observed = Arc::new(observed);
                }
                if module.core.module_id != prepared.program().root {
                    typed.name = format!("@module{}:function{}", module.core.module_id.0, id.0);
                }
                result_changed = !relative_equal(before, &typed)?;
                stats.body_checks += 1;
                stats.signature_readers_checked += usize::from(work.reader);
            } else {
                typed = before.clone();
                if old.span != function.span {
                    if !incremental::rebase(&mut typed, old.span, function.span) {
                        return None;
                    }
                    stats.position_rebindings += 1;
                }
                result_changed = false;
            }
            let position_changed = before.span != function.span;
            row.span = function.span;
            row.node = function.id;
            row.input = Some(
                produced
                    .function_body_revision(old.module, &old.name)?
                    .clone(),
            );
            rows = rows.replace(index, row)?;
            stats.receipt_tree_updates += 1;
            if typed != *before {
                program.module.functions.set_owner(index, Arc::new(typed));
            }
            stats.retained_functions = stats.retained_functions.checked_sub(1)?;
            changes.push(TypedFunctionChange {
                function: id,
                removed: false,
                result_changed,
                position_changed,
            });
        }
        let mut interface_costs = prior.interface_costs.clone();
        let mut fixed_tail = prior.fixed_tail;
        for module_id in signature_modules {
            let interface_index = *prior.interface_indexes.get(&module_id)?;
            let module = prepared
                .program()
                .modules
                .iter()
                .find(|module| module.core.module_id == module_id)?;
            // Namespace/publicness/count are unchanged. Rebuild only actual
            // changed declaration exports, then hash this current interface.
            let interface = program.interfaces.get_mut(interface_index)?;
            for change in batch
                .changes()
                .iter()
                .filter(|change| change.interface_changed())
            {
                let (source_module, source) = produced.changed_function(change)?;
                if source_module.core.module_id != module_id {
                    continue;
                }
                let signature = prior.tables.get(&module_id)?.get(&source.name)?;
                let id = signature.id?;
                let typed = program.module.functions.get(id.0 as usize)?;
                let export = current_export(source, typed);
                if signature.public {
                    let slot = interface
                        .exports
                        .iter_mut()
                        .find(|row| row.name == source.name)?;
                    *slot = export.clone();
                }
                if module_id == prepared.program().root && source.foreign.is_none() {
                    let slot = program
                        .root_functions
                        .iter_mut()
                        .find(|row| row.name == source.name)?;
                    *slot = export;
                }
            }
            let mut remaining = limits.max_analysis_steps;
            interface.signature_fingerprint = program::interface_fingerprint(
                &interface.exports,
                target,
                &program.module,
                &mut remaining,
                limits,
            )
            .ok()?;
            interface.implementation_fingerprint = module.core.source_fingerprint;
            let cost = limits.max_analysis_steps - remaining;
            fixed_tail = fixed_tail
                .checked_sub(*interface_costs.get(&module_id)?)?
                .checked_add(cost)?;
            Arc::make_mut(&mut interface_costs).insert(module_id, cost);
        }
        program.module.source_fingerprint = prepared.program().source_fingerprint;
        if continuous {
            for &index in batch.changed_modules() {
                let module = prepared.program().modules.get(index)?;
                let interface_index = *prior.interface_indexes.get(&module.core.module_id)?;
                program
                    .interfaces
                    .get_mut(interface_index)?
                    .implementation_fingerprint = module.core.source_fingerprint;
            }
        } else {
            // Earlier skipped raw epochs can have changed other modules too.
            // Copy current diagnostic identities for every module; never treat
            // the final producer delta or a fingerprint as an input proof.
            for module in &prepared.program().modules {
                let interface_index = *prior.interface_indexes.get(&module.core.module_id)?;
                program
                    .interfaces
                    .get_mut(interface_index)?
                    .implementation_fingerprint = module.core.source_fingerprint;
            }
        }
        let program = Arc::new(program);
        let context = Context {
            epoch: batch.epoch().clone(),
            input_context: batch.context().clone(),
            rows,
            program: program.clone(),
            signature_values,
            readers,
            signature_costs,
            signature_work,
            interface_costs,
            fixed_tail,
            ..prior.as_ref().clone()
        };
        if !context.fits(context.rows.total()?, limits) {
            return None;
        }
        let update = TypedUpdate {
            program,
            changes,
            stats,
        };
        Some((context, update))
    }
}

struct Work {
    body: bool,
    binding: bool,
    reader: bool,
}
fn checked_signature(
    module: &ProgramModule,
    index: usize,
    id: FunctionId,
    limits: &Limits,
) -> Option<(Signature, usize)> {
    let function = module.core.functions.owner(index)?;
    let mut core = module.core.as_ref().clone();
    core.functions = PersistentVec::new();
    core.functions.push_owner(function.clone());
    let exports = module.exports.iter().copied().collect();
    let mut arrays = false;
    let mut costs = BTreeMap::new();
    let mut table = signatures_recording(
        &core,
        limits,
        id.0 as usize,
        Some(&exports),
        &mut arrays,
        Some(&mut costs),
    )
    .ok()?;
    let signature = table.remove(&function.name)?;
    if arrays || signature.id != Some(id) || !closed_signature(&signature) {
        return None;
    }
    Some((signature, *costs.get(&id)?))
}
fn closed_signature(signature: &Signature) -> bool {
    let Signature {
        storage_life,
        life,
        id,
        callable,
        state_parameter,
        callable_return,
        type_parameter,
        template,
        foreign: _,
        scoped_callback,
        cleanup,
        parameters,
        result,
        public: _,
        receiver,
    } = signature;
    storage_life.is_none()
        && life.is_none()
        && id.is_some()
        && callable.is_empty()
        && state_parameter.is_none()
        && callable_return.is_none()
        && type_parameter.is_none()
        && template.is_none()
        && scoped_callback.is_none()
        && !cleanup
        && parameters.iter().all(primitive)
        && primitive(result)
        && receiver.is_none()
}
fn current_export(source: &CoreFunction, typed: &TypedFunction) -> ScalarFunction {
    ScalarFunction {
        storage_life: typed.storage_life.clone(),
        reference_return_bound: typed.reference_return_bound.clone(),
        name: source.name.clone(),
        function: typed.id,
        parameters: source
            .parameters
            .iter()
            .map(|p| (p.mode, p.ty.clone()))
            .collect(),
        result: typed.return_type.clone(),
        result_mode: source.result_mode,
    }
}

#[derive(Clone)]
struct Context {
    epoch: ExpansionEpoch,
    input_context: InputContext,
    namespace: NamespaceContext,
    signature_values: PersistentVec<Signature>,
    readers: PersistentVec<BTreeSet<FunctionId>>,
    signature_costs: PersistentVec<usize>,
    signature_work: usize,
    project_base: usize,
    interface_costs: Arc<BTreeMap<ModuleId, usize>>,
    target: TargetProfile,
    limits: Limits,
    program: Arc<TypedProgram>,
    tables: Arc<Tables>,
    rows: ReceiptTree,
    context_cost: [usize; 6],
    fixed_tail: usize,
    interface_indexes: Arc<BTreeMap<ModuleId, usize>>,
}
type Tables = BTreeMap<ModuleId, BTreeMap<String, Signature>>;
impl Context {
    /// Reconcile a missing raw-parent edge using every current sealed input.
    /// The successful CFG owner is unchanged until the entire request succeeds.
    /// These are producer-minted input identities, not hashes, dirty hints or
    /// Types/Verify authority. The producer preserves one only for identical
    /// parser declaration bytes plus attribute/file/generated-name binding.
    /// Complete input context is checked below; current raw NodeId/positions
    /// remain separate bindings. Concrete Typed FunctionId comes from the
    /// unchanged declaration namespace, never from a raw Core node number.
    fn bridge_inputs(&self, produced: &ProducedProgram) -> Option<BTreeMap<usize, Work>> {
        let batch = produced.change_batch();
        if batch.context() != &self.input_context
            || batch.namespace_context() != &self.namespace
            || produced
                .program()
                .modules
                .iter()
                .try_fold(0usize, |count, module| {
                    count.checked_add(module.core.functions.len())
                })?
                != self.rows.0.len
        {
            return None;
        }
        let mut work = BTreeMap::new();
        for index in 0..self.rows.0.len {
            let old = self.rows.get(index)?;
            let module = produced.program().modules.get(old.location.0)?;
            let function = module.core.functions.get(old.location.1)?;
            let input = produced.function_body_revision(old.module, &old.name)?;
            if old.id.0 as usize != index
                || self.program.module.functions.get(index)?.id != old.id
                || self
                    .tables
                    .get(&old.module)?
                    .get(&old.name)?
                    .concrete_id(function.span)
                    .ok()?
                    != old.id
                || module.core.module_id != old.module
                || function.name != old.name
                || produced.function_owner(old.module, &old.name) != old.owner.as_ref()
            {
                return None;
            }
            let same_input = input == old.input.as_ref()?;
            // Even equal body inputs must visit the binding path when current
            // raw placement changes. It refreshes Row.node/span and rebases the
            // actual CFG positions; typed FunctionId stays bound to this same
            // declaration slot and the current complete namespace.
            if !same_input || function.span != old.span || function.id != old.node {
                work.insert(
                    index,
                    Work {
                        body: !same_input,
                        binding: true,
                        reader: false,
                    },
                );
            }
        }
        Some(work)
    }

    fn fits(&self, total: Total, limits: &Limits) -> bool {
        let maxima = [
            limits.max_blocks,
            limits.max_locals,
            limits.max_values,
            limits.max_instructions,
            limits.max_analysis_steps,
            limits.max_nodes,
        ];
        for (i, &max) in maxima.iter().enumerate() {
            let Some(cost) = self.context_cost[i].checked_add(total.cfg[i]) else {
                return false;
            };
            if cost > max {
                return false;
            }
            if i == 4
                && cost
                    .checked_add(total.post.shared)
                    .and_then(|c| c.checked_add(self.fixed_tail))
                    .is_none_or(|c| c > max)
            {
                return false;
            }
        }
        self.project_base
            .checked_add(total.header)
            .is_some_and(|work| work <= limits.max_analysis_steps)
            && self.signature_work <= limits.max_analysis_steps
            && total.names <= limits.max_nodes
            && total.shape <= limits.max_analysis_steps
            && total.visibility <= limits.max_analysis_steps
            && total.post.bytes <= limits.max_analysis_steps
            && total.post.stores <= limits.max_analysis_steps
    }
}

/// One pass through the existing full checker records real successful work.
/// This temporary recording is released once the persistent receipt tree owns
/// the current leaves; no independent unbounded history is retained.
#[derive(Default)]
pub(super) struct Recording {
    pub(super) names: Vec<usize>,
    pub(super) shapes: Vec<usize>,
    pub(super) tables: Option<Arc<Tables>>,
    pub(super) context_cost: [usize; 6],
    pub(super) body_cost: [usize; 6],
    pub(super) closed_state: bool,
    pub(super) remaining: usize,
    pub(super) observed_complete: bool,
    pub(super) signature_costs: BTreeMap<FunctionId, usize>,
    pub(super) interface_costs: BTreeMap<ModuleId, usize>,
    pub(super) project_work: usize,
    rows: Vec<Row>,
}
impl Recording {
    pub(super) fn record(
        &mut self,
        location: (usize, usize),
        source: (ModuleId, &CoreFunction),
        typed: &TypedFunction,
        before: [usize; 6],
        after: [usize; 6],
        observed: BTreeSet<FunctionId>,
    ) {
        let (module, source) = source;
        let mut cfg = [0; 6];
        for i in 0..6 {
            let Some(cost) = after[i].checked_sub(before[i]) else {
                self.closed_state = false;
                return;
            };
            cfg[i] = cost;
        }
        self.rows.push(Row {
            id: typed.id,
            module,
            location,
            observed: Arc::new(observed),
            header: source
                .name
                .len()
                .saturating_add(source.parameters.len())
                .saturating_add(1),
            node: source.id,
            name: source.name.clone(),
            owner: None,
            input: None,
            span: source.span,
            cfg,
            names: 0,
            shape: 0,
            visibility: 0,
            post: PostReceipt::default(),
        });
    }
    fn finish(
        mut self,
        produced: &ProducedProgram,
        program: Arc<TypedProgram>,
        target: &TargetProfile,
        limits: &Limits,
    ) -> Option<Context> {
        if !self.closed_state
            || !self.observed_complete
            || self.rows.len() != program.module.functions.len()
            || self.names.len() != self.rows.len()
            || self.shapes.len() != self.rows.len()
            || !closed_module(&program.module)
            || !program.task_interfaces.is_empty()
        {
            return None;
        }
        for (index, ((row, names), shapes)) in self
            .rows
            .iter_mut()
            .zip(&self.names)
            .zip(&self.shapes)
            .enumerate()
        {
            let function = program.module.functions.get(index)?;
            if usize::try_from(function.id.0).ok()? != index
                || row.id != function.id
                || !closed_function(function)
            {
                return None;
            }
            row.owner = Some(produced.function_owner(row.module, &row.name)?.clone());
            row.input = Some(
                produced
                    .function_body_revision(row.module, &row.name)?
                    .clone(),
            );
            row.names = *names;
            row.shape = *shapes;
            row.post = PostReceipt::check(function, limits)?;
            row.visibility = instruction_count(function)?;
        }
        let tables = self.tables.take()?;
        let mut signature_values = Vec::with_capacity(self.rows.len());
        let mut readers = vec![BTreeSet::new(); self.rows.len()];
        let mut signature_costs = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            let signature = tables.get(&row.module)?.get(&row.name)?;
            if signature.id != Some(row.id) || !closed_signature(signature) {
                return None;
            }
            signature_values.push(signature.clone());
            signature_costs.push(*self.signature_costs.get(&row.id)?);
            for &callee in row.observed.iter() {
                readers.get_mut(callee.0 as usize)?.insert(row.id);
            }
        }
        let project_base = self.project_work.checked_sub(
            self.rows
                .iter()
                .try_fold(0usize, |sum, row| sum.checked_add(row.header))?,
        )?;
        let rows = ReceiptTree::build(self.rows)?;
        let total = rows.total()?;
        // All post-body shared work was really charged by the old full pass.
        // In this closed primitive domain only the independently checked per-
        // function resource scans vary with CFG shape; interface work is fixed.
        let fixed_tail = limits
            .max_analysis_steps
            .checked_sub(self.remaining)?
            .checked_sub(self.body_cost[4])?
            .checked_sub(total.post.shared)?;
        for i in 0..6 {
            if self.context_cost[i].checked_add(total.cfg[i])? != self.body_cost[i] {
                return None;
            }
        }
        let context = Context {
            namespace: produced.change_batch().namespace_context().clone(),
            signature_values: signature_values.into(),
            readers: readers.into(),
            signature_work: signature_costs
                .iter()
                .try_fold(0usize, |sum, cost| sum.checked_add(*cost))?,
            signature_costs: signature_costs.into(),
            interface_costs: Arc::new(self.interface_costs),
            project_base,
            epoch: produced.change_batch().epoch().clone(),
            input_context: produced.change_batch().context().clone(),
            target: target.clone(),
            limits: limits.clone(),
            tables,
            rows,
            context_cost: self.context_cost,
            fixed_tail,
            interface_indexes: Arc::new(
                program
                    .interfaces
                    .iter()
                    .enumerate()
                    .map(|(i, m)| (m.module, i))
                    .collect(),
            ),
            program,
        };
        context.fits(total, limits).then_some(context)
    }
}
#[derive(Clone)]
struct Row {
    id: FunctionId,
    module: ModuleId,
    location: (usize, usize),
    observed: Arc<BTreeSet<FunctionId>>,
    header: usize,
    node: NodeId,
    name: String,
    owner: Option<FunctionOwner>,
    input: Option<FunctionBodyRevision>,
    span: Span,
    cfg: [usize; 6],
    names: usize,
    shape: usize,
    visibility: usize,
    post: PostReceipt,
}
#[derive(Clone, Copy, Default)]
struct PostReceipt {
    shared: usize,
    bytes: usize,
    stores: usize,
}
impl PostReceipt {
    fn check(function: &TypedFunction, limits: &Limits) -> Option<Self> {
        if !closed_function(function) {
            return None;
        }
        let functions = std::slice::from_ref(function);
        let mut definitions = Vec::new();
        let mut remaining = limits.max_analysis_steps;
        if tasks::add_definition(functions, &mut definitions, &mut remaining, limits).ok()? {
            return None;
        }
        registration::add_definitions(functions, &mut definitions, &mut remaining).ok()?;
        byte_buffer::add_definition(functions, &mut definitions, limits).ok()?;
        dynamic_store::add_definitions(
            functions,
            &mut definitions,
            &[],
            &[],
            limits,
            &mut remaining,
        )
        .ok()?;
        if !definitions.is_empty() {
            return None;
        }
        // These two independent ledgers reserve roots before walking. The
        // exhaustive primitive predicate below admits no child visits/outputs.
        let roots = function
            .values
            .len()
            .checked_add(function.locals.len())?
            .checked_add(1)?;
        Some(Self {
            shared: limits.max_analysis_steps - remaining,
            bytes: roots.checked_add(1)?,
            stores: roots,
        })
    }
}
#[derive(Clone, Copy, Default)]
struct Total {
    cfg: [usize; 6],
    names: usize,
    shape: usize,
    visibility: usize,
    header: usize,
    post: PostReceipt,
}
impl Total {
    fn row(row: &Row) -> Self {
        Self {
            cfg: row.cfg,
            names: row.names,
            shape: row.shape,
            visibility: row.visibility,
            header: row.header,
            post: row.post,
        }
    }
    fn add(self, other: Self) -> Option<Self> {
        let mut cfg = [0; 6];
        for (i, value) in cfg.iter_mut().enumerate() {
            *value = self.cfg[i].checked_add(other.cfg[i])?;
        }
        Some(Self {
            cfg,
            names: self.names.checked_add(other.names)?,
            shape: self.shape.checked_add(other.shape)?,
            visibility: self.visibility.checked_add(other.visibility)?,
            header: self.header.checked_add(other.header)?,
            post: PostReceipt {
                shared: self.post.shared.checked_add(other.post.shared)?,
                bytes: self.post.bytes.checked_add(other.post.bytes)?,
                stores: self.post.stores.checked_add(other.post.stores)?,
            },
        })
    }
}
#[derive(Clone)]
struct ReceiptTree(Arc<ReceiptNode>);
struct ReceiptNode {
    len: usize,
    total: Option<Total>,
    kind: ReceiptKind,
}
enum ReceiptKind {
    Leaf(Row),
    Branch(ReceiptTree, ReceiptTree),
}
impl ReceiptTree {
    fn build(rows: Vec<Row>) -> Option<Self> {
        if rows.is_empty() {
            return None;
        }
        fn build(mut rows: Vec<Row>) -> ReceiptTree {
            if rows.len() == 1 {
                let row = rows.pop().expect("one receipt");
                return ReceiptTree(Arc::new(ReceiptNode {
                    len: 1,
                    total: Some(Total::row(&row)),
                    kind: ReceiptKind::Leaf(row),
                }));
            }
            let right = rows.split_off(rows.len() / 2);
            ReceiptTree::branch(build(rows), build(right))
        }
        let result = build(rows);
        result.total()?;
        Some(result)
    }
    fn branch(left: Self, right: Self) -> Self {
        Self(Arc::new(ReceiptNode {
            len: left.0.len + right.0.len,
            total: left.total().zip(right.total()).and_then(|(a, b)| a.add(b)),
            kind: ReceiptKind::Branch(left, right),
        }))
    }
    fn total(&self) -> Option<Total> {
        self.0.total
    }
    fn get(&self, index: usize) -> Option<&Row> {
        if index >= self.0.len {
            return None;
        }
        match &self.0.kind {
            ReceiptKind::Leaf(row) => Some(row),
            ReceiptKind::Branch(left, right) => {
                if index < left.0.len {
                    left.get(index)
                } else {
                    right.get(index - left.0.len)
                }
            }
        }
    }
    fn replace(&self, index: usize, row: Row) -> Option<Self> {
        if index >= self.0.len {
            return None;
        }
        Some(match &self.0.kind {
            ReceiptKind::Leaf(_) => Self(Arc::new(ReceiptNode {
                len: 1,
                total: Some(Total::row(&row)),
                kind: ReceiptKind::Leaf(row),
            })),
            ReceiptKind::Branch(left, right) => {
                if index < left.0.len {
                    Self::branch(left.replace(index, row)?, right.clone())
                } else {
                    Self::branch(left.clone(), right.replace(index - left.0.len, row)?)
                }
            }
        })
    }
}
fn primitive(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}
fn closed_function(f: &TypedFunction) -> bool {
    f.call_kind == CallKind::Direct
        && f.storage_life.is_none()
        && f.reference_return_bound.is_none()
        && f.callable_return_contract.is_none()
        && f.callable_contract.is_none()
        && f.protocol_contract.is_none()
        && f.environment.is_none()
        && primitive(&f.return_type)
        && f.locals.iter().all(|l| primitive(&l.ty))
        && f.values.iter().all(primitive)
}
fn closed_module(m: &TypedModule) -> bool {
    m.closures.is_empty()
        && m.resources.is_empty()
        && m.unsupported.is_empty()
        && m.life_declarations.is_empty()
        && m.life_instances.is_empty()
        && m.dyn_declarations.is_empty()
        && m.dyn_interfaces.is_empty()
        && m.dyn_vtables.is_empty()
}
fn instruction_count(f: &TypedFunction) -> Option<usize> {
    f.blocks
        .iter()
        .try_fold(0usize, |n, b| n.checked_add(b.instructions.len()))
}
mod relative;
use relative::relative_equal;

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, SourceUnit};
    use zil_expand::{
        ExpandSession,
        source_context::{DeclarationPlacement, SourceIdentityContext},
    };
    use zil_syntax::{ParsedSource, attributes::AttributeContext};

    fn parse(id: u32, text: &str, limits: &Limits) -> ParsedSource {
        let parsed = ParsedSource::parse(
            Arc::new(SourceUnit::new(
                FileId(id),
                ModuleId(id),
                format!("module{id}.zil"),
                text,
            )),
            limits,
            &AttributeContext::default(),
        );
        assert!(parsed.parsed().is_ok(), "{:?}", parsed.parsed().diagnostics);
        parsed
    }
    fn produced(
        session: &mut ExpandSession,
        sources: &[ParsedSource],
        limits: &Limits,
    ) -> ProducedProgram {
        let aliases = BTreeMap::new();
        session
            .expand_parsed(
                sources,
                &vec![vec![]; sources.len()],
                limits,
                &TargetProfile::default(),
                SourceIdentityContext::ClosedSnapshot {
                    root: ModuleId(0),
                    aliases: &aliases,
                    declarations: DeclarationPlacement::Original,
                },
            )
            .unwrap()
            .expect("closed source producer")
            .into_program(ModuleId(0), Fingerprint::ZERO)
            .0
    }
    fn prepared(produced: &ProducedProgram, limits: &Limits) -> PreparedProgram {
        infer_produced_program_for_ct(produced, limits)
            .unwrap()
            .into_prepared()
    }
    fn fresh(prepared: &PreparedProgram, limits: &Limits) -> CompileResult<TypedProgram> {
        check_prepared_program(prepared, &TargetProfile::default(), limits)
    }
    fn apply(
        session: &mut TypesPushSession,
        prepared: &PreparedProgram,
        limits: &Limits,
    ) -> CompileResult<TypedUpdate> {
        session.apply(prepared, &TargetProfile::default(), limits)
    }
    fn scalar_module() -> &'static str {
        "fn first(x:i32)->i32 {let a=x+1;let b=a*2;return b+3;} fn second(x:i32)->i32 {let a=x+2;let b=a*3;return b+4;} fn third(x:i32)->i32 {let a=x+3;let b=a*4;return b+5;}"
    }
    #[test]
    fn actual_sparse_cfg_consumers_share_untouched_leaves_and_match_fresh() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let mut input = [parse(0, scalar_module(), &limits)];
        let initial = produced(&mut expand, &input, &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        assert!(session.current.is_some(), "actual closed context admitted");
        let next = produced(&mut expand, &input, &limits);
        let warm = apply(&mut session, &prepared(&next, &limits), &limits).unwrap();
        assert_eq!(warm.stats.sparse_batches, 1);
        assert_eq!(warm.stats.body_checks, 0);
        assert!(
            cold.program
                .module
                .functions
                .ptr_eq(&warm.program.module.functions)
        );
        assert!(warm.changes.is_empty());
        input[0] = parse(0, &scalar_module().replace("x+1", "x+111"), &limits);
        let current = produced(&mut expand, &input, &limits);
        let current = prepared(&current, &limits);
        let edited = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(edited.stats.body_checks, 1);
        assert_eq!(edited.stats.position_rebindings, 2);
        assert_eq!(edited.stats.context_rebuilds, 0);
        assert_eq!(edited.program.as_ref(), &fresh(&current, &limits).unwrap());
        assert_eq!(
            edited
                .program
                .module
                .functions
                .changed_indices(&warm.program.module.functions),
            [0, 1, 2]
        );
        // Replace a same-width body next: real unchanged CFG leaves, not only a
        // product sidecar, retain their owners through the actual program.
        input[0] = parse(
            0,
            &input[0].source().text.replace("x+111", "x+222"),
            &limits,
        );
        let next = produced(&mut expand, &input, &limits);
        let next = prepared(&next, &limits);
        let same_width = apply(&mut session, &next, &limits).unwrap();
        assert_eq!(same_width.stats.body_checks, 1);
        assert_eq!(same_width.stats.position_rebindings, 0);
        assert_eq!(
            same_width
                .program
                .module
                .functions
                .changed_indices(&edited.program.module.functions),
            [0]
        );
        for index in [1, 2] {
            assert!(Arc::ptr_eq(
                edited.program.module.functions.owner(index).unwrap(),
                same_width.program.module.functions.owner(index).unwrap()
            ));
        }
        assert_eq!(same_width.program.as_ref(), &fresh(&next, &limits).unwrap());
        assert_eq!(
            cold.program.as_ref(),
            &fresh(&prepared(&initial, &limits), &limits).unwrap()
        );
    }
    #[test]
    fn error_history_limits_and_unsealed_inputs_cannot_retarget_current_context() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let cold_source = parse(0, scalar_module(), &limits);
        let cold_input = produced(&mut expand, std::slice::from_ref(&cold_source), &limits);
        let cold = apply(&mut session, &prepared(&cold_input, &limits), &limits).unwrap();
        let successful = session.current.clone().unwrap();
        let bad_source = parse(
            0,
            &scalar_module().replace("return b+3", "return missing(b)"),
            &limits,
        );
        let bad = produced(&mut expand, &[bad_source], &limits);
        let bad = prepared(&bad, &limits);
        let actual = apply(&mut session, &bad, &limits).unwrap_err();
        assert_eq!(actual, fresh(&bad, &limits).unwrap_err());
        assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &successful));
        let recovered = produced(&mut expand, &[cold_source], &limits);
        let recovered = prepared(&recovered, &limits);
        let recovered = apply(&mut session, &recovered, &limits).unwrap();
        assert_eq!(
            recovered.stats.context_rebuilds, 0,
            "a failed raw epoch must be bridged from all current sealed inputs"
        );
        assert_eq!(recovered.stats.body_checks, 1);
        assert_eq!(recovered.program, cold.program);
        let blocks = cold
            .program
            .module
            .functions
            .iter()
            .map(|f| f.blocks.len())
            .sum::<usize>();
        assert!(blocks > 1);
        let too_small = Limits {
            max_blocks: blocks - 1,
            ..limits.clone()
        };
        let raw = prepared(&cold_input, &too_small);
        assert_eq!(
            apply(&mut session, &raw, &too_small).unwrap_err(),
            fresh(&raw, &too_small).unwrap_err()
        );
        // Materializing an arbitrary Core removes the producer seal. Even a
        // forged source hash and byte-equal copy cannot activate sparse reuse.
        let shared = Arc::new(cold_input.program().as_ref().clone());
        let raw = infer_shared_program_for_ct(&shared, &limits)
            .unwrap()
            .into_prepared();
        let unsealed = apply(&mut session, &raw, &limits).unwrap();
        assert_eq!(unsealed.stats.context_rebuilds, 1);
        assert!(session.current.is_none());
        let weak = Arc::downgrade(&successful);
        drop(successful);
        session.clear();
        assert!(
            weak.upgrade().is_none(),
            "no independent context history survives eviction"
        );
    }
    #[test]
    fn complete_integer_and_foreign_headers_remain_current() {
        let limits = Limits::default();
        let text = [
            "i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize", "f32", "f64",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, ty)| {
            format!(
                "fn value{i}(x:{ty})->{ty} {{let y=x+{};return y;}} ",
                if ty.starts_with('f') { "1.0" } else { "1" }
            )
        })
        .collect::<String>()
            + "extern \"C\" fn foreign_value(x:i32)->i32;";
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        assert!(session.current.is_some());
        let next = produced(
            &mut expand,
            &[parse(0, &text.replace("let y=x+1", "let y=x+2"), &limits)],
            &limits,
        );
        let next = prepared(&next, &limits);
        let actual = apply(&mut session, &next, &limits).unwrap();
        assert_eq!(actual.stats.context_rebuilds, 0);
        assert_eq!(actual.stats.body_checks, 12);
        assert_eq!(actual.program.as_ref(), &fresh(&next, &limits).unwrap());
        assert!(Arc::ptr_eq(
            cold.program.module.functions.owner(12).unwrap(),
            actual.program.module.functions.owner(12).unwrap()
        ));
    }
    #[test]
    fn full_425_function_history_updates_actual_cfg_leaves_without_full_types_inventory() {
        let limits = Limits::default();
        let mut input = vec![
            parse(0, "fn main(x:i32)->i32=>x+1;", &limits),
            parse(
                1,
                "extern \"C\" fn io0(x:i32)->i32;extern \"C\" fn io1(x:i32)->i32;extern \"C\" fn io2(x:i32)->i32;extern \"C\" fn io3(x:i32)->i32;",
                &limits,
            ),
        ];
        for module in 2..22 {
            let text = (0..21)
                .map(|function| {
                    let mut body = format!("fn helper{function}(x:i32)->i32 {{let v0=x+11;");
                    for step in 1..16 {
                        body += &format!("let v{step}=v{}*3+{};", step - 1, step + 1);
                    }
                    body + "return v15;}"
                })
                .collect::<String>();
            input.push(parse(module, &text, &limits));
        }
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let cold_input = produced(&mut expand, &input, &limits);
        let cold = apply(&mut session, &prepared(&cold_input, &limits), &limits).unwrap();
        assert_eq!(cold.program.module.functions.len(), 425);
        assert!(session.current.is_some());
        input[2] = parse(
            2,
            &input[2].source().text.replacen("x+11", "x+111", 1),
            &limits,
        );
        let edited = produced(&mut expand, &input, &limits);
        let edited = prepared(&edited, &limits);
        let edited = apply(&mut session, &edited, &limits).unwrap();
        assert_eq!(edited.stats.body_checks, 1);
        assert_eq!(edited.stats.position_rebindings, 20);
        assert_eq!(edited.stats.retained_functions, 404);
        assert_eq!(edited.stats.context_rebuilds, 0);
        assert_eq!(
            edited
                .program
                .module
                .functions
                .changed_indices(&cold.program.module.functions)
                .len(),
            21
        );
        for index in 26..425 {
            assert!(Arc::ptr_eq(
                cold.program.module.functions.owner(index).unwrap(),
                edited.program.module.functions.owner(index).unwrap()
            ));
        }
        for (index, source) in input.iter_mut().enumerate() {
            *source = parse(
                index as u32,
                &("// current positions\n".to_owned() + &source.source().text),
                &limits,
            );
        }
        let positioned = produced(&mut expand, &input, &limits);
        let positioned = prepared(&positioned, &limits);
        let positions = apply(&mut session, &positioned, &limits).unwrap();
        assert_eq!(positions.stats.body_checks, 0);
        assert_eq!(positions.stats.position_rebindings, 425);
        assert_eq!(positions.stats.context_rebuilds, 0);
        assert_eq!(
            positions.program.as_ref(),
            &fresh(&positioned, &limits).unwrap()
        );
    }
    #[test]
    fn added_and_removed_body_nodes_rebind_current_raw_ids_before_sibling_edits() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let mut input = [parse(0, scalar_module(), &limits)];
        let initial = produced(&mut expand, &input, &limits);
        let initial_node = initial.program().modules[0].core.functions[1].id;
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let expanded = scalar_module().replacen("let a=x+1;", "let extra=x+9;let a=x+1;", 1);
        input[0] = parse(0, &expanded, &limits);
        let current = produced(&mut expand, &input, &limits);
        assert_ne!(
            initial_node,
            current.program().modules[0].core.functions[1].id
        );
        assert_ne!(
            initial.function_input_revision(ModuleId(0), "second"),
            current.function_input_revision(ModuleId(0), "second"),
            "legacy NodeId-bound revision remains current"
        );
        assert_eq!(
            initial.function_owner(ModuleId(0), "second"),
            current.function_owner(ModuleId(0), "second")
        );
        let current = prepared(&current, &limits);
        let inserted = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(inserted.stats.body_checks, 1);
        assert_eq!(inserted.stats.position_rebindings, 2);
        assert_eq!(inserted.stats.context_rebuilds, 0);
        assert_eq!(
            inserted.program.as_ref(),
            &fresh(&current, &limits).unwrap()
        );
        input[0] = parse(0, &expanded.replace("x+2", "x+7"), &limits);
        let current = produced(&mut expand, &input, &limits);
        let current = prepared(&current, &limits);
        let sibling = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(sibling.stats.body_checks, 1);
        assert_eq!(sibling.stats.context_rebuilds, 0);
        assert_eq!(
            sibling
                .program
                .module
                .functions
                .changed_indices(&inserted.program.module.functions),
            [1]
        );
        assert_eq!(sibling.program.as_ref(), &fresh(&current, &limits).unwrap());
        input[0] = parse(
            0,
            &input[0].source().text.replace("let extra=x+9;", ""),
            &limits,
        );
        let current = produced(&mut expand, &input, &limits);
        let current = prepared(&current, &limits);
        let removed = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(removed.stats.body_checks, 1);
        assert_eq!(removed.stats.context_rebuilds, 0);
        assert_eq!(removed.program.as_ref(), &fresh(&current, &limits).unwrap());
        assert_eq!(
            cold.program.as_ref(),
            &fresh(&prepared(&initial, &limits), &limits).unwrap()
        );
    }

    #[test]
    fn same_limits_replay_rejects_aggregate_block_exhaustion_and_recovers_prior_owner() {
        let defaults = Limits::default();
        let text = (0..10)
            .map(|i| format!("fn f{i}(x:i32)->i32=>x+1; "))
            .collect::<String>();
        let mut sizing_expand = ExpandSession::default();
        let sized = produced(&mut sizing_expand, &[parse(0, &text, &defaults)], &defaults);
        let size = fresh(&prepared(&sized, &defaults), &defaults).unwrap();
        let blocks = size
            .module
            .functions
            .iter()
            .map(|f| f.blocks.len())
            .sum::<usize>();
        assert_eq!(blocks, 10);
        let limits = Limits {
            max_blocks: blocks,
            ..defaults
        };
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let previous = session.current.as_ref().unwrap().clone();
        let producer_checkpoint = expand.checkpoint();
        let edited = text.replacen(
            "fn f0(x:i32)->i32=>x+1;",
            "fn f0(x:i32)->i32 {if x>0 {return x+1;}else{return x+2;}}",
            1,
        );
        let current = produced(&mut expand, &[parse(0, &edited, &limits)], &limits);
        let current = prepared(&current, &limits);
        // Its changed function fits alone; rejection requires composing the
        // unchanged nine leaves in the original global block ledger.
        let default_current = fresh(&current, &Limits::default()).unwrap();
        assert!(default_current.module.functions[0].blocks.len() < blocks);
        assert!(
            default_current
                .module
                .functions
                .iter()
                .map(|f| f.blocks.len())
                .sum::<usize>()
                > blocks
        );
        assert_eq!(
            apply(&mut session, &current, &limits).unwrap_err(),
            fresh(&current, &limits).unwrap_err()
        );
        assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &previous));
        expand = producer_checkpoint;
        let recovery = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        let recovery = prepared(&recovery, &limits);
        let recovered = apply(&mut session, &recovery, &limits).unwrap();
        assert_eq!(recovered.stats.sparse_batches, 1);
        assert_eq!(recovered.stats.body_checks, 0);
        assert_eq!(recovered.program, cold.program);
        let mut minus = limits.clone();
        minus.max_blocks -= 1;
        assert_eq!(
            apply(&mut session, &recovery, &minus).unwrap_err(),
            fresh(&recovery, &minus).unwrap_err()
        );
    }
    #[test]
    fn node_numbered_discard_names_are_rebuilt_from_current_lowering_state() {
        let limits = Limits::default();
        let text = "fn first(x:i32)->i32 {let a=x+1;return a;} fn second(x:i32)->i32 {let _=x+2;return x+3;} fn third(x:i32)->i32=>x+4;";
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, text, &limits)], &limits);
        apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let text = text.replacen("let a=x+1;", "let extra=x+9;let a=x+1;", 1);
        let current = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        assert_eq!(
            current
                .change_batch()
                .changes()
                .iter()
                .filter(|change| change.body_changed())
                .count(),
            2,
            "current compiler-generated discard binding is a real body input"
        );
        let current = prepared(&current, &limits);
        let actual = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(actual.stats.body_checks, 2);
        assert_eq!(actual.stats.context_rebuilds, 0);
        assert_eq!(actual.program.as_ref(), &fresh(&current, &limits).unwrap());
    }
    #[test]
    fn current_signature_leaves_push_only_actual_readers_and_replace_changed_edges() {
        let limits = Limits::default();
        let text = "fn leaf(x:i32)->i32=>i32(x)+1; fn reader()->i32=>leaf(1)+2; fn other()->i32=>leaf(2)+3; fn keep()->i32=>4;";
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, text, &limits)], &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        assert_eq!(
            session.current.as_ref().unwrap().readers[0],
            BTreeSet::from([FunctionId(1), FunctionId(2)])
        );
        let current_text = text.replacen("x:i32", "x:u32", 1);
        let current = produced(&mut expand, &[parse(0, &current_text, &limits)], &limits);
        assert_ne!(
            initial.change_batch().context(),
            current.change_batch().context()
        );
        assert_eq!(
            initial.change_batch().namespace_context(),
            current.change_batch().namespace_context()
        );
        assert_eq!(current.change_batch().changes().len(), 1);
        assert!(current.change_batch().changes()[0].interface_changed());
        let current = prepared(&current, &limits);
        let changed = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(changed.stats.context_rebuilds, 0);
        assert_eq!(changed.stats.signature_updates, 1);
        assert_eq!(changed.stats.signature_readers_checked, 2);
        assert_eq!(changed.stats.body_checks, 3);
        assert_eq!(
            changed
                .program
                .module
                .functions
                .changed_indices(&cold.program.module.functions),
            [0, 1, 2]
        );
        assert!(Arc::ptr_eq(
            changed.program.module.functions.owner(3).unwrap(),
            cold.program.module.functions.owner(3).unwrap()
        ));
        assert_eq!(changed.program.as_ref(), &fresh(&current, &limits).unwrap());
        let edge_text = current_text.replacen("leaf(1)+2", "keep()+2", 1);
        let edges = produced(&mut expand, &[parse(0, &edge_text, &limits)], &limits);
        let edges = prepared(&edges, &limits);
        let edges = apply(&mut session, &edges, &limits).unwrap();
        assert_eq!(edges.stats.body_checks, 1);
        assert_eq!(
            session.current.as_ref().unwrap().readers[0],
            BTreeSet::from([FunctionId(2)])
        );
        assert_eq!(
            session.current.as_ref().unwrap().readers[3],
            BTreeSet::from([FunctionId(1)])
        );
        let final_text = edge_text.replacen("x:u32", "x:i32", 1);
        let final_input = produced(&mut expand, &[parse(0, &final_text, &limits)], &limits);
        let final_input = prepared(&final_input, &limits);
        let final_result = apply(&mut session, &final_input, &limits).unwrap();
        assert_eq!(final_result.stats.body_checks, 2);
        assert_eq!(final_result.stats.signature_readers_checked, 1);
        assert_eq!(final_result.stats.context_rebuilds, 0);
        assert_eq!(
            final_result.program.as_ref(),
            &fresh(&final_input, &limits).unwrap()
        );
        assert_eq!(
            cold.program.as_ref(),
            &fresh(&prepared(&initial, &limits), &limits).unwrap()
        );
    }
    #[test]
    fn changed_signature_error_and_same_limit_header_growth_preserve_successful_owner() {
        let defaults = Limits::default();
        let text = (0..10)
            .map(|i| format!("fn f{i}(x:i32)->i32=>x+1; "))
            .collect::<String>();
        let mut sizing = ExpandSession::default();
        let sized = produced(&mut sizing, &[parse(0, &text, &defaults)], &defaults);
        let cold_fresh = fresh(&prepared(&sized, &defaults), &defaults).unwrap();
        let locals = cold_fresh
            .module
            .functions
            .iter()
            .map(|f| f.locals.len())
            .sum::<usize>();
        let limits = Limits {
            max_locals: locals,
            ..defaults
        };
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let owner = session.current.as_ref().unwrap().clone();
        let checkpoint = expand.checkpoint();
        let changed = text.replacen("f0(x:i32)", "f0(x:i32,y:i32)", 1);
        let current = produced(&mut expand, &[parse(0, &changed, &limits)], &limits);
        let current = prepared(&current, &limits);
        let generous = fresh(&current, &Limits::default()).unwrap();
        assert!(generous.module.functions[0].locals.len() < locals);
        assert!(
            generous
                .module
                .functions
                .iter()
                .map(|f| f.locals.len())
                .sum::<usize>()
                > locals
        );
        assert_eq!(
            apply(&mut session, &current, &limits).unwrap_err(),
            fresh(&current, &limits).unwrap_err()
        );
        assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &owner));
        expand = checkpoint;
        let recovery = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        let recovery = prepared(&recovery, &limits);
        let recovered = apply(&mut session, &recovery, &limits).unwrap();
        assert_eq!(recovered.stats.body_checks, 0);
        assert_eq!(recovered.program, cold.program);
        let error = text.replacen(
            "fn f0(x:i32)->i32=>x+1;",
            "fn f0(x:i32)->i32=>missing(x);",
            1,
        );
        let error = produced(&mut expand, &[parse(0, &error, &limits)], &limits);
        let error = prepared(&error, &limits);
        assert_eq!(
            apply(&mut session, &error, &limits).unwrap_err(),
            fresh(&error, &limits).unwrap_err()
        );
    }
    #[test]
    fn changed_imported_signature_reads_current_values_and_namespace_rebinding_rebuilds() {
        let limits = Limits::default();
        let target = TargetProfile::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let main = parse(0, "import dep\nfn main()->i32=>dep.leaf(1);", &limits);
        let other = parse(2, "pub fn leaf(x:i32)->i32=>i32(x)+8;", &limits);
        let mut provider = parse(1, "pub fn leaf(x:i32)->i32=>i32(x)+1;", &limits);
        let make = |expand: &mut ExpandSession, provider: &ParsedSource, owner: u32| {
            let imports = vec![
                vec![CoreImport {
                    alias: "dep".into(),
                    module: ModuleId(owner),
                    span: main.parsed().module.items[0].span,
                }],
                vec![],
                vec![],
            ];
            let aliases = BTreeMap::new();
            expand
                .expand_parsed(
                    &[main.clone(), provider.clone(), other.clone()],
                    &imports,
                    &limits,
                    &target,
                    SourceIdentityContext::ClosedSnapshot {
                        root: ModuleId(0),
                        aliases: &aliases,
                        declarations: DeclarationPlacement::Original,
                    },
                )
                .unwrap()
                .unwrap()
                .into_program(ModuleId(0), Fingerprint::ZERO)
                .0
        };
        let initial = make(&mut expand, &provider, 1);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        provider = parse(1, "pub fn leaf(x:u32)->i32=>i32(x)+1;", &limits);
        let current = make(&mut expand, &provider, 1);
        let current = prepared(&current, &limits);
        let changed = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(changed.stats.signature_updates, 1);
        assert_eq!(changed.stats.signature_readers_checked, 1);
        assert_eq!(changed.stats.body_checks, 2);
        assert_eq!(changed.stats.context_rebuilds, 0);
        assert!(Arc::ptr_eq(
            cold.program.module.functions.owner(2).unwrap(),
            changed.program.module.functions.owner(2).unwrap()
        ));
        assert_eq!(changed.program.as_ref(), &fresh(&current, &limits).unwrap());
        let rebound = make(&mut expand, &provider, 2);
        let rebound = prepared(&rebound, &limits);
        let rebound = apply(&mut session, &rebound, &limits).unwrap();
        assert_eq!(rebound.stats.context_rebuilds, 1);
        assert_ne!(
            rebound.program.module.functions[0],
            changed.program.module.functions[0]
        );
        let actual_source = make(&mut expand, &provider, 2);
        let actual_source = prepared(&actual_source, &limits);
        assert_eq!(
            rebound.program.as_ref(),
            &fresh(&actual_source, &limits).unwrap()
        );
    }
    #[test]
    fn checkpoint_requires_exact_program_owner_and_reactivates_only_its_raw_epoch() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, scalar_module(), &limits)], &limits);
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let mut checkpoint = session
            .checkpoint_for(&cold.program)
            .expect("exact successful raw owner");
        let weak = Arc::downgrade(checkpoint.current.as_ref().unwrap());
        let same_bytes = Arc::new(cold.program.as_ref().clone());
        assert_eq!(same_bytes, cold.program);
        assert!(
            session.checkpoint_for(&same_bytes).is_none(),
            "equal decoded/COW program cannot retarget a private epoch"
        );
        let producer_checkpoint = expand.checkpoint();
        let broad_text = scalar_module()
            .replace("x+1", "x+7")
            .replace("x+2", "x+8")
            .replace("x+3", "x+9");
        let broad = produced(&mut expand, &[parse(0, &broad_text, &limits)], &limits);
        let broad = prepared(&broad, &limits);
        let broad = apply(&mut session, &broad, &limits).unwrap();
        assert!(
            session.checkpoint_for(&cold.program).is_none(),
            "current different successful epoch cannot own an old program"
        );
        assert!(session.checkpoint_for(&broad.program).is_some());
        expand = producer_checkpoint;
        let private = produced(
            &mut expand,
            &[parse(
                0,
                &scalar_module().replacen("x+1", "x+7", 1),
                &limits,
            )],
            &limits,
        );
        let private = prepared(&private, &limits);
        let current = apply(&mut checkpoint, &private, &limits).unwrap();
        assert_eq!(current.stats.context_rebuilds, 0);
        assert_eq!(current.stats.body_checks, 1);
        assert_eq!(current.program.as_ref(), &fresh(&private, &limits).unwrap());
        for index in [1, 2] {
            assert!(Arc::ptr_eq(
                cold.program.module.functions.owner(index).unwrap(),
                current.program.module.functions.owner(index).unwrap()
            ));
        }
        assert!(
            weak.upgrade().is_none(),
            "reactivation does not create an independent context history"
        );
        checkpoint.clear();
        assert!(checkpoint.current.is_none());
    }

    #[test]
    fn failed_epoch_bridge_rechecks_all_current_inputs_and_module_identities() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let root = "fn first(x:i32)->i32=>x+1; fn sibling(x:i32)->i32=>x+2;";
        let other = "pub fn remote(x:i32)->i32=>x+3; fn spare(x:i32)->i32=>x+4;";
        let initial = produced(
            &mut expand,
            &[parse(0, root, &limits), parse(1, other, &limits)],
            &limits,
        );
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let checkpoint = session.current.clone().unwrap();
        // The first failed raw epoch changes a SECOND module and all positions.
        // The next error and recovery only change module zero. Its last sparse
        // delta cannot describe all differences from the successful Types owner.
        // Model the resident parser's unchanged snapshot owner. Reparsing the
        // same text would intentionally mint a new module revision and would
        // put this module in every delta, defeating this counterexample.
        let edited_other = parse(
            1,
            &format!("\n\n{}", other.replacen("x+3", "x+8", 1)),
            &limits,
        );
        let mut skipped_module_revision = None;
        for invalid in ["missing", "unknown"] {
            let input = produced(
                &mut expand,
                &[
                    parse(
                        0,
                        &format!("\n\n{}", root.replacen("x+1", &format!("x+{invalid}"), 1)),
                        &limits,
                    ),
                    edited_other.clone(),
                ],
                &limits,
            );
            let revision = input.module_revision(ModuleId(1)).unwrap();
            assert_ne!(Some(revision), initial.module_revision(ModuleId(1)));
            if let Some(previous) = &skipped_module_revision {
                assert_eq!(revision, previous);
            } else {
                skipped_module_revision = Some(revision.clone());
            }
            let current = prepared(&input, &limits);
            let actual = apply(&mut session, &current, &limits).unwrap_err();
            assert_eq!(actual, fresh(&current, &limits).unwrap_err());
            assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &checkpoint));
        }
        let recovered = produced(
            &mut expand,
            &[
                parse(
                    0,
                    &format!("\n\n{}", root.replacen("x+1", "x+9", 1)),
                    &limits,
                ),
                edited_other,
            ],
            &limits,
        );
        assert_eq!(
            recovered.module_revision(ModuleId(1)),
            skipped_module_revision.as_ref()
        );
        assert_eq!(recovered.change_batch().changed_modules(), &[0]);
        let current = prepared(&recovered, &limits);
        let update = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(update.stats.context_rebuilds, 0);
        assert_eq!(update.stats.body_checks, 2);
        assert_eq!(update.stats.position_rebindings, 2);
        assert_eq!(update.program.as_ref(), &fresh(&current, &limits).unwrap());
        assert_ne!(
            update.program.interfaces[1].implementation_fingerprint,
            cold.program.interfaces[1].implementation_fingerprint
        );
        assert_eq!(
            cold.program.as_ref(),
            &fresh(&prepared(&initial, &limits), &limits).unwrap()
        );
    }

    #[test]
    fn failed_epoch_bridge_does_not_cross_changed_headers_namespace_or_limits() {
        let limits = Limits::default();
        for replacement in [
            "fn first(x:bool)->bool=>x; fn sibling(x:i32)->i32=>x+2;",
            "fn renamed(x:i32)->i32=>x+1; fn sibling(x:i32)->i32=>x+2;",
        ] {
            let mut expand = ExpandSession::default();
            let mut session = TypesPushSession::default();
            let initial = produced(
                &mut expand,
                &[parse(
                    0,
                    "fn first(x:i32)->i32=>x+1; fn sibling(x:i32)->i32=>x+2;",
                    &limits,
                )],
                &limits,
            );
            apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
            let bad = produced(
                &mut expand,
                &[parse(
                    0,
                    "fn first(x:i32)->i32=>x+missing; fn sibling(x:i32)->i32=>x+unknown;",
                    &limits,
                )],
                &limits,
            );
            let bad = prepared(&bad, &limits);
            assert_eq!(
                apply(&mut session, &bad, &limits).unwrap_err(),
                fresh(&bad, &limits).unwrap_err()
            );
            let input = produced(&mut expand, &[parse(0, replacement, &limits)], &limits);
            let input = prepared(&input, &limits);
            let actual = apply(&mut session, &input, &limits).unwrap();
            assert_eq!(actual.stats.context_rebuilds, 1);
            assert_eq!(actual.program.as_ref(), &fresh(&input, &limits).unwrap());
            // A real user quota change remains an exact validity input. The
            // producer's independent +8 lowering allowance is not this quota.
            let larger = Limits {
                max_analysis_steps: limits.max_analysis_steps + 8,
                ..limits.clone()
            };
            let input = produced(&mut expand, &[parse(0, replacement, &larger)], &larger);
            let input = prepared(&input, &larger);
            let actual = apply(&mut session, &input, &larger).unwrap();
            assert_eq!(actual.stats.context_rebuilds, 1);
            assert_eq!(actual.program.as_ref(), &fresh(&input, &larger).unwrap());
        }
    }

    #[test]
    fn failed_epoch_bridge_keeps_aggregate_budget_and_original_first_error() {
        let limits = Limits {
            max_blocks: 10,
            ..Limits::default()
        };
        let text = (0..10)
            .map(|i| format!("fn f{i}(x:i32)->i32=>x+1; "))
            .collect::<String>();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let initial = produced(&mut expand, &[parse(0, &text, &limits)], &limits);
        apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let checkpoint = session.current.clone().unwrap();
        let bad = produced(
            &mut expand,
            &[parse(
                0,
                &text.replacen(
                    "fn f9(x:i32)->i32=>x+1;",
                    "fn f9(x:i32)->i32=>x+missing;",
                    1,
                ),
                &limits,
            )],
            &limits,
        );
        let bad = prepared(&bad, &limits);
        assert_eq!(
            apply(&mut session, &bad, &limits).unwrap_err(),
            fresh(&bad, &limits).unwrap_err()
        );
        let larger_body = produced(
            &mut expand,
            &[parse(
                0,
                &text.replacen(
                    "fn f9(x:i32)->i32=>x+1;",
                    "fn f9(x:i32)->i32 {if x>0 {return x+1;}else{return x+2;}}",
                    1,
                ),
                &limits,
            )],
            &limits,
        );
        assert_eq!(checkpoint.bridge_inputs(&larger_body).unwrap().len(), 1);
        let current = prepared(&larger_body, &limits);
        let unrestricted = fresh(&current, &Limits::default()).unwrap();
        assert!(unrestricted.module.functions[9].blocks.len() < limits.max_blocks);
        assert!(
            unrestricted
                .module
                .functions
                .iter()
                .map(|f| f.blocks.len())
                .sum::<usize>()
                > limits.max_blocks
        );
        assert_eq!(
            apply(&mut session, &current, &limits).unwrap_err(),
            fresh(&current, &limits).unwrap_err()
        );
        assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &checkpoint));
        let recovered = produced(
            &mut expand,
            &[parse(
                0,
                &text.replacen("fn f9(x:i32)->i32=>x+1;", "fn f9(x:i32)->i32=>x+7;", 1),
                &limits,
            )],
            &limits,
        );
        let recovered = prepared(&recovered, &limits);
        let actual = apply(&mut session, &recovered, &limits).unwrap();
        assert_eq!(actual.stats.context_rebuilds, 0);
        assert_eq!(actual.stats.body_checks, 1);
        assert_eq!(actual.stats.retained_functions, 9);
        assert_eq!(
            actual.program.as_ref(),
            &fresh(&recovered, &limits).unwrap()
        );
    }

    #[test]
    fn failed_structural_gap_updates_raw_placement_without_rechecking_equal_bodies() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let text = |body: &str| {
            format!(
                "fn first(x:i32)->i32 {{{body:<64}}} fn second(x:i32)->i32=>x+2; fn third(x:i32)->i32=>x+3;"
            )
        };
        let initial = produced(
            &mut expand,
            &[parse(0, &text("return x;"), &limits)],
            &limits,
        );
        let cold = apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let successful = session.current.clone().unwrap();
        let bad = produced(
            &mut expand,
            &[parse(0, &text("let alias=x;return missing;"), &limits)],
            &limits,
        );
        let bad = prepared(&bad, &limits);
        assert_eq!(
            apply(&mut session, &bad, &limits).unwrap_err(),
            fresh(&bad, &limits).unwrap_err()
        );
        assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &successful));
        let current = produced(
            &mut expand,
            &[parse(0, &text("let alias=x;return alias;"), &limits)],
            &limits,
        );
        let before = &initial.program().modules[0].core.functions[1];
        let after = &current.program().modules[0].core.functions[1];
        assert_ne!(before.id, after.id);
        assert_eq!(
            before.span, after.span,
            "isolate NodeId placement from positions"
        );
        assert_ne!(
            initial.function_input_revision(ModuleId(0), "second"),
            current.function_input_revision(ModuleId(0), "second")
        );
        assert_eq!(
            initial.function_body_revision(ModuleId(0), "second"),
            current.function_body_revision(ModuleId(0), "second")
        );
        let prepared_current = prepared(&current, &limits);
        let update = apply(&mut session, &prepared_current, &limits).unwrap();
        assert_eq!(update.stats.context_rebuilds, 0);
        assert_eq!(update.stats.body_checks, 1);
        assert_eq!(update.stats.position_rebindings, 0);
        assert_eq!(update.stats.receipt_tree_updates, 3);
        assert_eq!(
            update.program.as_ref(),
            &fresh(&prepared_current, &limits).unwrap()
        );
        for index in [1, 2] {
            assert!(Arc::ptr_eq(
                cold.program.module.functions.owner(index).unwrap(),
                update.program.module.functions.owner(index).unwrap()
            ));
            assert_eq!(
                session
                    .current
                    .as_ref()
                    .unwrap()
                    .rows
                    .get(index)
                    .unwrap()
                    .node,
                current.program().modules[0].core.functions[index].id
            );
            assert_eq!(
                update.program.module.functions[index].id,
                FunctionId(index as u32)
            );
        }
        // A following ordinary sparse edit must consume the newly bound raw
        // NodeId, rather than hiding a stale row behind an unchanged Typed ID.
        let next = produced(
            &mut expand,
            &[parse(
                0,
                &text("let alias=x;return alias;").replace("x+2", "x+7"),
                &limits,
            )],
            &limits,
        );
        let next = prepared(&next, &limits);
        let update = apply(&mut session, &next, &limits).unwrap();
        assert_eq!(update.stats.context_rebuilds, 0);
        assert_eq!(update.stats.body_checks, 1);
        assert_eq!(update.program.as_ref(), &fresh(&next, &limits).unwrap());
    }

    #[test]
    fn failed_structural_gap_rechecks_actual_node_named_body_dependencies() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = TypesPushSession::default();
        let source = "fn first(x:i32)->i32=>x; fn discard(x:i32)->i32 {let _=x;return x;} fn sibling(x:i32)->i32=>x;";
        let initial = produced(&mut expand, &[parse(0, source, &limits)], &limits);
        apply(&mut session, &prepared(&initial, &limits), &limits).unwrap();
        let bad = produced(
            &mut expand,
            &[parse(
                0,
                &source.replacen(
                    "fn first(x:i32)->i32=>x;",
                    "fn first(x:i32)->i32 {let alias=x;return missing;}",
                    1,
                ),
                &limits,
            )],
            &limits,
        );
        let bad = prepared(&bad, &limits);
        assert_eq!(
            apply(&mut session, &bad, &limits).unwrap_err(),
            fresh(&bad, &limits).unwrap_err()
        );
        let current = produced(
            &mut expand,
            &[parse(
                0,
                &source.replacen(
                    "fn first(x:i32)->i32=>x;",
                    "fn first(x:i32)->i32 {let alias=x;return alias;}",
                    1,
                ),
                &limits,
            )],
            &limits,
        );
        assert_ne!(
            initial.function_body_revision(ModuleId(0), "discard"),
            current.function_body_revision(ModuleId(0), "discard")
        );
        assert_eq!(
            initial.function_body_revision(ModuleId(0), "sibling"),
            current.function_body_revision(ModuleId(0), "sibling")
        );
        let current = prepared(&current, &limits);
        let update = apply(&mut session, &current, &limits).unwrap();
        assert_eq!(update.stats.context_rebuilds, 0);
        assert_eq!(
            update.stats.body_checks, 2,
            "generated $discard names changed in the second body"
        );
        assert_eq!(update.stats.position_rebindings, 1);
        assert_eq!(update.program.as_ref(), &fresh(&current, &limits).unwrap());
    }
}
