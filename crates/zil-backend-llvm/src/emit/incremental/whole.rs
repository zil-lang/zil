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
enum FunctionInput {
    // Existing successful function consumers already own complete immutable
    // inputs. Reuse those owners rather than clone all function headers again.
    Entry(Arc<Entry>),
    // Per-function native equality excludes diagnostic spans; a current full
    // input with different spans therefore needs its own exact snapshot.
    Current(Arc<TypedFunction>),
}
impl FunctionInput {
    fn get(&self) -> &TypedFunction {
        match self {
            Self::Entry(entry) => &entry.input,
            Self::Current(input) => input,
        }
    }
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
    functions: Vec<FunctionInput>,
    layouts: Vec<ScalarLayout>,
    limits: zil_base::Limits,
    options: WholeOptions,
    before: usize,
    spent: usize,
    machine: m::Module,
    collected: bool,
}

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
    ) -> Option<(m::Module, bool, usize)> {
        let product = self.whole_machine.as_ref()?;
        if !self.eligible
            || reference_planner()
            || product.options != options
            || product.limits != *verified.limits()
            || product.before != before
            || !product.module.matches(verified.module())
            || product.layouts != lir.layouts.as_ref()
            || product.functions.len() != lir.functions.len()
            || !checked_scalar_domain(verified)
        {
            return None;
        }
        for (previous, current) in product.functions.iter().zip(lir.functions) {
            self.whole_machine_input_functions_compared += 1;
            if previous.get() != current {
                return None;
            }
        }
        // Exact current initial ledger and full limits, not a renewed budget.
        // A mismatching/insufficient start always executes the original planner
        // and preserves its particular first failure and diagnostic location.
        let remaining = before.checked_sub(product.spent)?;
        let mut machine = product.machine.clone();
        machine.fingerprint = verified.fingerprint().to_hex();
        self.reused_machine_module = true;
        Some((machine, product.collected, remaining))
    }

    pub(in crate::emit) fn discard_machine(&mut self) {
        self.whole_machine = None;
    }

    pub(in crate::emit) fn remember_machine(
        &mut self,
        verified: &VerifiedModule,
        lir: &LirView<'_>,
        options: WholeOptions,
        before: usize,
        output: &(m::Module, bool, usize),
    ) {
        if !self.eligible || reference_planner() || !checked_scalar_domain(verified) {
            return;
        }
        let Some(spent) = before.checked_sub(output.2) else {
            return;
        };
        let functions = lir
            .functions
            .iter()
            .map(|function| {
                match self
                    .entries
                    .get(&function.id)
                    .filter(|entry| entry.input == *function)
                {
                    Some(entry) => FunctionInput::Entry(entry.clone()),
                    None => FunctionInput::Current(Arc::new(function.clone())),
                }
            })
            .collect();
        self.whole_machine = Some(Arc::new(MachineProduct {
            module: ModuleInput::new(verified.module()),
            functions,
            layouts: lir.layouts.to_vec(),
            limits: verified.limits().clone(),
            options,
            before,
            spent,
            machine: output.0.clone(),
            collected: output.1,
        }));
    }
}
