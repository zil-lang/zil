//! A raw emitter result, after current independent Verify and actual LIR binding.
//! No hash, previous VerifiedModule, or native checker permission is retained.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::emit) struct WholeOptions {
    pub(in crate::emit) emit: EmitOptions,
    pub(in crate::emit) scalar_policy: bool,
    pub(in crate::emit) closed_collection: bool,
    pub(in crate::emit) requested_jobs: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModuleInput {
    dyn_declarations: Vec<zil_ir::DynDeclaration>,
    dyn_interfaces: Vec<zil_ir::DynInterface>,
    dyn_vtables: Vec<zil_ir::DynVtable>,
    life_declarations: Vec<zil_ir::LifeNominalDeclaration>,
    life_instances: Vec<zil_ir::LifeNominalInstance>,
    schema_version: u32,
    module_id: zil_base::ModuleId,
    source_fingerprint: zil_base::Fingerprint,
    target: TargetProfile,
    closures: Vec<ClosureDefinition>,
    resources: Vec<ResourceDefinition>,
    unsupported: Vec<zil_ir::UnsupportedFeature>,
}
impl ModuleInput {
    fn new(module: &zil_ir::TypedModule) -> Self {
        // An added module field must be classified before this cache compiles.
        let zil_ir::TypedModule {
            dyn_declarations,
            dyn_interfaces,
            dyn_vtables,
            life_declarations,
            life_instances,
            schema_version,
            module_id,
            source_fingerprint,
            target,
            functions: _,
            closures,
            resources,
            unsupported,
        } = module;
        Self {
            dyn_declarations: dyn_declarations.clone(),
            dyn_interfaces: dyn_interfaces.clone(),
            dyn_vtables: dyn_vtables.clone(),
            life_declarations: life_declarations.clone(),
            life_instances: life_instances.clone(),
            schema_version: *schema_version,
            module_id: *module_id,
            source_fingerprint: *source_fingerprint,
            target: target.clone(),
            closures: closures.clone(),
            resources: resources.clone(),
            unsupported: unsupported.clone(),
        }
    }
    fn matches(&self, module: &zil_ir::TypedModule) -> bool {
        let zil_ir::TypedModule {
            dyn_declarations,
            dyn_interfaces,
            dyn_vtables,
            life_declarations,
            life_instances,
            schema_version,
            module_id,
            source_fingerprint,
            target,
            functions: _,
            closures,
            resources,
            unsupported,
        } = module;
        self.schema_version == *schema_version
            && self.module_id == *module_id
            && self.source_fingerprint == *source_fingerprint
            && self.target == *target
            && self.dyn_declarations == *dyn_declarations
            && self.dyn_interfaces == *dyn_interfaces
            && self.dyn_vtables == *dyn_vtables
            && self.life_declarations == *life_declarations
            && self.life_instances == *life_instances
            && self.closures == *closures
            && self.resources == *resources
            && self.unsupported == *unsupported
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MachineProduct {
    module: ModuleInput,
    functions: zil_ir::PersistentVec<TypedFunction>,
    layouts: Vec<ScalarLayout>,
    limits: zil_base::Limits,
    options: WholeOptions,
    before: usize,
    spent: usize,
    machine: crate::machine_output::MachineOutput,
    collected: bool,
    epoch: LowerEpoch,
}

/// Raw function and downstream-identity roots from exactly this successful
/// machine product. Restoring only `machine` would leave the preceding request's
/// transform/partition identity live and could substitute a different module.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LowerEpoch {
    context: Option<Arc<Context>>,
    entries: EntryTable,
    context_identity: Arc<()>,
    products: crate::incremental::ScalarProducts,
    output_bodies: zil_ir::PersistentVec<m::SharedVec<m::Item>>,
    admitted: Arc<BTreeMap<FunctionId, Vec<FunctionId>>>,
}

// A count bound, not a byte/RSS budget: retain at most four successful immutable
// raw epochs. Bodies and machine items share their existing owners; no complete
// serialization or entry/header clone is performed to publish or restore one.
// No-session calls consume none of this history. The driver clears the complete
// BackendSession only after a successful native request disables producer reuse;
// its existing checkpoint prevents failed/cancelled native builds from committing
// either these roots or any later transform/partition work.
const MACHINE_HISTORY_EPOCHS: usize = 4;

// This first complete-result slice deliberately retains the original closed
// scalar domain. In particular no helper/worker/ownership result is inferred
// from a previous checker. Every role gate reads THIS independently checked cap.
fn checked_scalar_domain(verified: &VerifiedModule) -> bool {
    verified.task_interfaces().is_empty()
        && verified.task_worker_ids().is_empty()
        && !verified.uses_scalar_tasks()
        && !verified.uses_retained_registration()
        && !verified.uses_stored_borrows()
        && !verified.uses_owned_sum_extension()
        && verified.module().functions.iter().all(|function| {
            !verified.is_suspend(function.id)
                && !verified.is_task_export(function.id)
                && verified.task_ownership(function.id).is_none()
        })
}
fn reference_planner() -> bool {
    #[cfg(test)]
    {
        super::super::copy_aggregates::reference_planner_enabled()
    }
    #[cfg(not(test))]
    {
        false
    }
}

impl Session {
    pub(in crate::emit) fn begin_machine_request(&mut self) {
        self.hits = 0;
        self.misses = 0;
        self.source_projection_traversals = 0;
        self.source_projection_instruction_visits = 0;
        self.source_projection_reuses = 0;
        self.reused_machine_module = false;
        self.whole_machine_input_functions_compared = 0;
    }

    /// The caller has already run the original current metadata prefix and
    /// bind_lir, in their original order. An independently allocated raw view
    /// therefore cannot obtain this result merely by copying a fingerprint.
    pub(in crate::emit) fn reuse_machine(
        &mut self,
        verified: &VerifiedModule,
        lir: &LirView<'_>,
        options: WholeOptions,
        before: usize,
    ) -> Option<(crate::machine_output::MachineOutput, bool, usize)> {
        if self.machine_history.is_empty() || reference_planner() {
            return None;
        }
        for index in 0..self.machine_history.len() {
            let product = self.machine_history[index].clone();
            if product.options != options
                || product.limits != *verified.limits()
                || product.before != before
                || !product.module.matches(verified.module())
                || product.layouts.as_slice() != lir.layouts.as_ref()
                || product.functions.len() != lir.functions.len()
            {
                continue;
            }
            if !product.functions.ptr_eq(lir.functions) {
                let mut equal = true;
                for (previous, current) in product.functions.iter().zip(lir.functions) {
                    self.whole_machine_input_functions_compared += 1;
                    if previous != current {
                        equal = false;
                        break;
                    }
                }
                if !equal {
                    continue;
                }
            }
            // Exact current initial ledger and full limits, not a renewed budget.
            // All current metadata and bind_lir checks ran before this lookup.
            let Some(remaining) = before.checked_sub(product.spent) else {
                continue;
            };
            if !checked_scalar_domain(verified) {
                return None;
            }
            let mut machine = product.machine.clone();
            machine.fingerprint = verified.fingerprint().to_hex();
            self.context = product.epoch.context.clone();
            self.entries = product.epoch.entries.clone();
            self.context_identity = product.epoch.context_identity.clone();
            self.products = product.epoch.products.clone();
            self.output_bodies = product.epoch.output_bodies.clone();
            self.product_count = self.products.len();
            self.admitted = product.epoch.admitted.clone();
            self.eligible = true;
            self.machine_history.remove(index);
            self.machine_history.push_front(product.clone());
            self.reused_machine_module = true;
            return Some((machine, product.collected, remaining));
        }
        None
    }

    pub(in crate::emit) fn discard_machine(&mut self) {
        // A failing miss must not expose a stale transform/partition identity.
        // Completed historical epochs are unchanged; no partial one is added.
        self.eligible = false;
    }

    pub(in crate::emit) fn assemble_output(
        &self,
        triple: String,
        fingerprint: String,
        prefix: m::SharedVec<m::Item>,
        suffix: m::SharedVec<m::Item>,
    ) -> crate::machine_output::MachineOutput {
        crate::machine_output::MachineOutput::from_pieces(
            triple,
            fingerprint,
            prefix,
            self.output_bodies.clone(),
            suffix,
            self.machine_history.front().map(|old| &old.machine),
        )
    }

    pub(in crate::emit) fn remember_machine(
        &mut self,
        verified: &VerifiedModule,
        lir: &LirView<'_>,
        options: WholeOptions,
        before: usize,
        output: &(crate::machine_output::MachineOutput, bool, usize),
    ) {
        if self.identity().is_none() || reference_planner() || !checked_scalar_domain(verified) {
            return;
        }
        let Some(spent) = before.checked_sub(output.2) else {
            return;
        };
        let product = Arc::new(MachineProduct {
            module: ModuleInput::new(verified.module()),
            functions: lir.functions.clone(),
            layouts: lir.layouts.to_vec(),
            limits: verified.limits().clone(),
            options,
            before,
            spent,
            machine: output.0.clone(),
            collected: output.1,
            epoch: LowerEpoch {
                context: self.context.clone(),
                entries: self.entries.clone(),
                context_identity: self.context_identity.clone(),
                products: self.products.clone(),
                output_bodies: self.output_bodies.clone(),
                admitted: self.admitted.clone(),
            },
        });
        self.machine_history.push_front(product);
        self.machine_history.truncate(MACHINE_HISTORY_EPOCHS);
    }
}

// Isolate the original emitter's budget boundary without changing the current
// independent Verify limit. Production always starts from the checked limit;
// tests may only reduce the actual ledger before the unchanged current prefix.
#[cfg(test)]
thread_local! {
    static INITIAL_WORK: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(in crate::emit) fn initial_work(limit: usize) -> usize {
    INITIAL_WORK.get().map_or(limit, |work| work.min(limit))
}
#[cfg(test)]
pub(crate) fn with_initial_work<T>(work: usize, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<usize>);
    impl Drop for Reset {
        fn drop(&mut self) {
            INITIAL_WORK.set(self.0);
        }
    }
    let _reset = Reset(INITIAL_WORK.replace(Some(work)));
    run()
}
