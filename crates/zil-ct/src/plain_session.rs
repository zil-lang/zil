//! Successful raw unchanged-body checks, consumed under a current header index.
//! No CT operation, checker capability or rewritten result is retained here.
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlainSourceStats {
    pub shape_checks: usize,
    pub shape_reuses: usize,
    pub body_checks: usize,
    pub body_reuses: usize,
    pub declaration_modules: usize,
    pub context_reuses: usize,
    pub fallbacks: usize,
}
#[derive(Clone, Default)]
pub struct PlainSourceSession {
    current: Option<Arc<Epoch>>,
    stats: PlainSourceStats,
}
struct Epoch {
    program: CoreProgram,
    target: TargetProfile,
    limits: Limits,
    modules: Vec<ModuleReceipt>,
}
#[derive(Clone)]
struct ModuleReceipt {
    limits: Limits,
    functions: PersistentVec<FunctionReceipt>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Ledger {
    nodes: usize,
    work_left: usize,
    steps_left: u64,
    next_node: Option<u32>,
}
impl Ledger {
    fn of(walker: &Walker<'_>) -> Self {
        Self {
            nodes: walker.nodes,
            work_left: walker.work_left,
            steps_left: walker.steps_left,
            next_node: walker.next_node,
        }
    }
    fn install(self, walker: &mut Walker<'_>) {
        walker.nodes = self.nodes;
        walker.work_left = self.work_left;
        walker.steps_left = self.steps_left;
        walker.next_node = self.next_node;
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FunctionReceipt {
    shape_before: usize,
    shape_after: usize,
    before: Ledger,
    after: Ledger,
}
impl PlainSourceSession {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn last_stats(&self) -> &PlainSourceStats {
        &self.stats
    }
    /// Attach acceleration only to the exact immutable module owners from a
    /// completed plain run, plus complete program identity/import/export values.
    /// This is a raw checkpoint, never permission to skip the next live binding.
    pub fn checkpoint_for(&self, program: &CoreProgram) -> Option<Self> {
        let current = &self.current.as_ref()?.program;
        (current.root == program.root
            && current.source_fingerprint == program.source_fingerprint
            && current.modules.len() == program.modules.len()
            && current
                .modules
                .iter()
                .zip(&program.modules)
                .all(|(old, new)| {
                    Arc::ptr_eq(&old.core, &new.core)
                        && old.imports == new.imports
                        && old.exports == new.exports
                }))
        .then(|| self.clone())
    }

    /// Current SourceProgramContext construction and every module's original
    /// declarations walk remain mandatory. Only successful, exact plain-body
    /// admissions can replay, with the complete current target/limit/context and
    /// original shape/Walker entry ledgers. None asks the caller to restart its
    /// complete original ordered path, including its first-error behavior.
    pub fn validate_program(
        &mut self,
        program: &CoreProgram,
        target: &TargetProfile,
        limits: &Limits,
        context: &SourceProgramContext<'_>,
    ) -> Option<Vec<SourceEvaluationUsage>> {
        self.stats = PlainSourceStats::default();
        let result = self.try_program(program, target, limits, context);
        if result.is_none() {
            self.stats.fallbacks += 1;
        }
        result
    }
    fn try_program(
        &mut self,
        program: &CoreProgram,
        target: &TargetProfile,
        limits: &Limits,
        context: &SourceProgramContext<'_>,
    ) -> Option<Vec<SourceEvaluationUsage>> {
        // This is the live, private index just built from this exact program,
        // not a hash, old context, public dirty list or independent capability.
        if !std::ptr::eq(context.program, program) || context.limits != *limits {
            return None;
        }
        let old = self.current.as_ref().filter(|old| {
            old.target == *target
                && old.limits == *limits
                && same_program_context(&old.program, program)
        });
        self.stats.context_reuses = usize::from(old.is_some());
        let mut remaining = limits.clone();
        let mut modules = Vec::with_capacity(program.modules.len());
        let mut usages = Vec::with_capacity(program.modules.len());
        for (index, module) in program.modules.iter().enumerate() {
            let prior = old.and_then(|old| {
                let receipt = old.modules.get(index)?;
                (receipt.limits == remaining)
                    .then_some((old.program.modules[index].core.as_ref(), receipt))
            });
            let (receipt, usage) = validate_module(
                &module.core,
                target,
                &remaining,
                context,
                prior,
                &mut self.stats,
            )?;
            // Exactly the driver's project allowance transitions, without
            // manufacturing a diagnostic from this speculative path. Any
            // insufficiency replays the original charge_ct_usage in order.
            remaining.max_ct_steps = remaining.max_ct_steps.checked_sub(usage.steps)?;
            remaining.max_nodes = remaining.max_nodes.checked_sub(usage.nodes)?;
            remaining.max_functions = remaining
                .max_functions
                .checked_sub(module.core.functions.len())?;
            modules.push(receipt);
            usages.push(usage);
        }
        // Publish only after the whole plain program and all ordered project
        // charges succeed. Persistent function rows and Core owners are shared;
        // this retains one raw epoch, never an unbounded body/history map.
        self.current = Some(Arc::new(Epoch {
            program: program.clone(),
            target: target.clone(),
            limits: limits.clone(),
            modules,
        }));
        Some(usages)
    }
}

fn validate_module(
    core: &CoreModule,
    target: &TargetProfile,
    limits: &Limits,
    context: &SourceProgramContext<'_>,
    prior: Option<(&CoreModule, &ModuleReceipt)>,
    stats: &mut PlainSourceStats,
) -> Option<(ModuleReceipt, SourceEvaluationUsage)> {
    unchanged_inventory(core)?;
    let mut rows = prior
        .map(|(_, receipt)| receipt.functions.clone())
        .unwrap_or_default();
    rows.resize(core.functions.len(), FunctionReceipt::default());
    let mut fuel = limits.max_nodes.min(1_000_000);
    let mut stack = Vec::new();
    let mut shapes = Vec::with_capacity(core.functions.len());
    let mut reusable = Vec::with_capacity(core.functions.len());
    for (index, function) in core.functions.iter().enumerate() {
        let same_owner = prior.is_some_and(|(old, _)| {
            matches!((old.functions.owner(index), core.functions.owner(index)),
                (Some(old), Some(current)) if Arc::ptr_eq(old, current))
        });
        reusable.push(same_owner);
        let before = fuel;
        if same_owner && rows[index].shape_before == fuel {
            fuel = rows[index].shape_after;
            stats.shape_reuses += 1;
        } else {
            stats.shape_checks += 1;
            unchanged_function_shape(function, limits, &mut fuel, &mut stack)?;
        }
        shapes.push((before, fuel));
    }
    if core.schema_version != SCHEMA_VERSION {
        return None;
    }
    let mut walker = Walker {
        core,
        target,
        limits,
        nodes: 0,
        work_left: limits.max_analysis_steps,
        next_node: None,
        steps_left: limits.max_ct_steps,
        context: Some(context),
        compiler: None,
        scopes: Vec::new(),
    };
    stats.declaration_modules += 1;
    walker.declarations().ok()?;
    for (index, function) in core.functions.iter().enumerate() {
        let before = Ledger::of(&walker);
        // Local hint scopes must not escape a function; this is the complete
        // mutable Walker state and every other input is independently bound.
        if reusable[index] && rows[index].before == before && walker.scopes.is_empty() {
            rows[index].after.install(&mut walker);
            stats.body_reuses += 1;
        } else {
            stats.body_checks += 1;
            walker.unchanged_function(function).ok()?;
        }
        if !walker.scopes.is_empty() {
            return None;
        }
        let row = FunctionReceipt {
            shape_before: shapes[index].0,
            shape_after: shapes[index].1,
            before,
            after: Ledger::of(&walker),
        };
        if rows[index] != row {
            rows.replace(index, row);
        }
    }
    let usage = SourceEvaluationUsage {
        steps: limits.max_ct_steps.checked_sub(walker.steps_left)?,
        nodes: walker.nodes,
    };
    Some((
        ModuleReceipt {
            limits: limits.clone(),
            functions: rows,
        },
        usage,
    ))
}

fn same_program_context(previous: &CoreProgram, current: &CoreProgram) -> bool {
    let CoreProgram {
        root,
        source_fingerprint: _,
        modules,
    } = current;
    previous.root == *root
        && previous.modules.len() == modules.len()
        && previous.modules.iter().zip(modules).all(|(old, current)| {
            let ProgramModule {
                core,
                imports,
                exports,
            } = current;
            old.imports == *imports
                && old.exports == *exports
                && same_module_context(&old.core, core)
        })
}
fn same_module_context(previous: &CoreModule, current: &CoreModule) -> bool {
    // Source fingerprints are diagnostic identities, not inputs read by this
    // unchanged validator. All actual inventories and every declaration are
    // retained; only bodies may differ under the same context.
    let CoreModule {
        dyn_declarations,
        dyn_interfaces,
        dyn_vtables,
        life_declarations,
        life_instances,
        closure_life_contexts,
        schema_version,
        module_id,
        source_fingerprint: _,
        functions,
        aggregates,
        aggregate_templates,
        aggregate_applications,
        enums,
        unsupported,
    } = current;
    previous.dyn_declarations == *dyn_declarations
        && previous.dyn_interfaces == *dyn_interfaces
        && previous.dyn_vtables == *dyn_vtables
        && previous.life_declarations == *life_declarations
        && previous.life_instances == *life_instances
        && previous.closure_life_contexts == *closure_life_contexts
        && previous.schema_version == *schema_version
        && previous.module_id == *module_id
        && previous.aggregates == *aggregates
        && previous.aggregate_templates == *aggregate_templates
        && previous.aggregate_applications == *aggregate_applications
        && previous.enums == *enums
        && previous.unsupported == *unsupported
        && previous.functions.len() == functions.len()
        && functions
            .changed_indices(&previous.functions)
            .into_iter()
            .all(|index| {
                matches!((previous.functions.get(index), functions.get(index)),
                (Some(old), Some(current)) if same_header(old, current))
            })
}
fn same_header(previous: &CoreFunction, current: &CoreFunction) -> bool {
    let CoreFunction {
        storage_life,
        call_kind,
        life,
        type_parameters,
        state_parameters,
        protocol_contract,
        callable_return,
        id,
        name,
        callable_parameters,
        foreign,
        parameters,
        return_type,
        result_mode,
        body: _,
        receiver,
        span,
    } = current;
    previous.storage_life == *storage_life
        && previous.call_kind == *call_kind
        && previous.life == *life
        && previous.type_parameters == *type_parameters
        && previous.state_parameters == *state_parameters
        && previous.protocol_contract == *protocol_contract
        && previous.callable_return == *callable_return
        && previous.id == *id
        && previous.name == *name
        && previous.callable_parameters == *callable_parameters
        && previous.foreign == *foreign
        && previous.parameters == *parameters
        && previous.return_type == *return_type
        && previous.result_mode == *result_mode
        && previous.receiver == *receiver
        && previous.span == *span
}

#[cfg(test)]
#[path = "plain_session_tests.rs"]
mod tests;
