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
    fn cost_to(self, after: Self) -> Option<BodyCost> {
        // Plain admission never runs CT or allocates rewritten node identities.
        // Reject rather than retain a receipt if that closed-state contract grows.
        if self.steps_left != after.steps_left
            || self.next_node.is_some()
            || after.next_node.is_some()
        {
            return None;
        }
        Some(BodyCost {
            nodes: after.nodes.checked_sub(self.nodes)?,
            work: self.work_left.checked_sub(after.work_left)?,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BodyCost {
    nodes: usize,
    work: usize,
}
impl BodyCost {
    fn replay(self, walker: &mut Walker<'_>) -> Option<()> {
        if !walker.scopes.is_empty() || walker.next_node.is_some() {
            return None;
        }
        let nodes = walker.nodes.checked_add(self.nodes)?;
        if nodes > walker.limits.max_nodes {
            return None;
        }
        let work = walker.work_left.checked_sub(self.work)?;
        // charge/text/hint_type consume only nonnegative amounts. The complete
        // successful cost fitting here proves each original prefix also fits.
        // Depth and non-ledger limits remain bound by the epoch's full Limits.
        walker.nodes = nodes;
        walker.work_left = work;
        Some(())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FunctionReceipt {
    shape_cost: usize,
    body_cost: BodyCost,
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
    /// successful costs debited from current shape/Walker ledgers. None asks the
    /// caller to restart its complete original ordered path, including its
    /// first-error behavior.
    pub fn validate_program(
        &mut self,
        program: &CoreProgram,
        target: &TargetProfile,
        limits: &Limits,
        context: &SourceProgramContext<'_>,
    ) -> Option<Vec<SourceEvaluationUsage>> {
        self.validate_program_with_allowance(program, target, limits, limits, context)
    }
    /// Bind the complete user contract separately from node/CT allowances
    /// already consumed by the driver's current selection/expansion prefix.
    /// Every current header, shape/body fee and ordered project charge still
    /// uses `allowance`; this never replenishes either ledger.
    pub fn validate_program_with_allowance(
        &mut self,
        program: &CoreProgram,
        target: &TargetProfile,
        user_limits: &Limits,
        allowance: &Limits,
        context: &SourceProgramContext<'_>,
    ) -> Option<Vec<SourceEvaluationUsage>> {
        self.stats = PlainSourceStats::default();
        let result = self.try_program(program, target, user_limits, allowance, context);
        if result.is_none() {
            self.stats.fallbacks += 1;
        }
        result
    }
    fn try_program(
        &mut self,
        program: &CoreProgram,
        target: &TargetProfile,
        user_limits: &Limits,
        limits: &Limits,
        context: &SourceProgramContext<'_>,
    ) -> Option<Vec<SourceEvaluationUsage>> {
        // This is the live, private index just built from this exact program,
        // not a hash, old context, public dirty list or independent capability.
        let mut expected = user_limits.clone();
        expected.max_nodes = limits.max_nodes;
        expected.max_ct_steps = limits.max_ct_steps;
        if limits.max_nodes > user_limits.max_nodes
            || limits.max_ct_steps > user_limits.max_ct_steps
            || expected != *limits
            || !std::ptr::eq(context.program, program)
            || context.limits != *limits
        {
            return None;
        }
        let old = self.current.as_ref().filter(|old| {
            old.target == *target
                && old.limits == *user_limits
                && same_plain_hint_environment(&old.program, program)
        });
        self.stats.context_reuses = usize::from(old.is_some());
        let mut remaining = limits.clone();
        let mut modules = Vec::with_capacity(program.modules.len());
        let mut usages = Vec::with_capacity(program.modules.len());
        for (index, module) in program.modules.iter().enumerate() {
            // Only project node/function/CT allowances change between modules;
            // the complete original user Limits were compared above. Current
            // declarations enforce max_functions, and all body fees are debited
            // from the current (possibly smaller) module allowance below.
            let prior = old.map(|old| {
                (
                    old.program.modules[index].core.as_ref(),
                    &old.modules[index],
                )
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
            limits: user_limits.clone(),
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
        if same_owner {
            fuel = fuel.checked_sub(rows[index].shape_cost)?;
            stats.shape_reuses += 1;
        } else {
            stats.shape_checks += 1;
            unchanged_function_shape(function, limits, &mut fuel, &mut stack)?;
        }
        shapes.push(before.checked_sub(fuel)?);
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
        if reusable[index] {
            rows[index].body_cost.replay(&mut walker)?;
            stats.body_reuses += 1;
        } else {
            stats.body_checks += 1;
            walker.unchanged_function(function).ok()?;
        }
        if !walker.scopes.is_empty() {
            return None;
        }
        let row = FunctionReceipt {
            shape_cost: shapes[index],
            body_cost: before.cost_to(Ledger::of(&walker))?,
        };
        if rows[index] != row {
            rows.replace(index, row);
        }
    }
    let usage = SourceEvaluationUsage {
        steps: limits.max_ct_steps.checked_sub(walker.steps_left)?,
        nodes: walker.nodes,
    };
    Some((ModuleReceipt { functions: rows }, usage))
}

// This is only equality of inputs consumed by successful plain body hints.
// SourceProgramContext::function resolves (module, name) and current imports;
// its index and unchanged_function/unchanged_expr never read export NodeIds.
// A raw function ID is copied by the full rebuilding evaluator, but it is not
// consumed by these no-rewrite visitors. Every returned raw program, current
// context and declarations walk is still current. This is NOT checkpoint or
// Typed/Verify identity: checkpoint_for above intentionally retains the full
// program fingerprint, immutable Core owners, imports AND exact exports.
fn same_plain_hint_environment(previous: &CoreProgram, current: &CoreProgram) -> bool {
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
                exports: _,
            } = current;
            old.imports == *imports && same_plain_hint_module(&old.core, core)
        })
}
fn same_plain_hint_module(previous: &CoreModule, current: &CoreModule) -> bool {
    // Retain all inventories and actual hint/header dependencies. Body owners,
    // raw declaration IDs and absolute positions are bound separately; they do
    // not select a different callee hint under the same (module, name) table.
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
// Function/parameter locations are read only for declaration diagnostics (run
// fresh) and the function's own body admission (still exact-owner bound).
// Cross-function hints read parameter/return types using the current call span;
// no hint reads a callee body, raw ID, ID allocation state or diagnostic location.
// A changed ID still changes that function's immutable owner and requires its
// actual current admission; it only stops invalidating unrelated callers.
fn same_header(previous: &CoreFunction, current: &CoreFunction) -> bool {
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
    } = current;
    previous.storage_life == *storage_life
        && previous.call_kind == *call_kind
        && previous.life == *life
        && previous.type_parameters == *type_parameters
        && previous.state_parameters == *state_parameters
        && previous.protocol_contract == *protocol_contract
        && previous.callable_return == *callable_return
        && previous.name == *name
        && previous.callable_parameters == *callable_parameters
        && previous.foreign == *foreign
        && previous.parameters.len() == parameters.len()
        && previous
            .parameters
            .iter()
            .zip(parameters)
            .all(|(old, new)| {
                let CoreParameter {
                    name,
                    ty,
                    mode,
                    span: _,
                } = new;
                old.name == *name && old.ty == *ty && old.mode == *mode
            })
        && previous.return_type == *return_type
        && previous.result_mode == *result_mode
        && previous.receiver == *receiver
}

#[cfg(test)]
#[path = "plain_session_tests.rs"]
mod tests;
