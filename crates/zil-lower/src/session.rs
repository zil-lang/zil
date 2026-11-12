//! Private lowering admission receipts bound to immutable, independently
//! verified inputs. No caller-supplied change list or digest admits a function.
use super::*;
use std::sync::Arc;
use std::time::Instant;
use zil_ir::{PersistentVec, TypedModule};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LowerStats {
    /// Actual per-function metadata admission executions in this request.
    pub functions_checked: usize,
    pub layout_checks: usize,
    pub functions_reused: usize,
    pub full_checks: usize,
    pub budget_replays: usize,
    pub context_checks: usize,
    /// Includes complete non-function context equality, which remains work.
    pub context_check_ns: u64,
    pub cost_profiled: bool,
}

impl LowerStats {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"functions_checked\":{},\"layout_checks\":{},\"functions_reused\":{},\"full_checks\":{},\"budget_replays\":{},\"context_checks\":{},\"context_check_ns\":{},\"cost_profiled\":{}}}",
            self.functions_checked,
            self.layout_checks,
            self.functions_reused,
            self.full_checks,
            self.budget_replays,
            self.context_checks,
            self.context_check_ns,
            self.cost_profiled,
        )
    }
}

#[derive(Default)]
pub(super) struct AdmissionRecording {
    pub costs: Vec<[usize; 2]>,
    pub metadata_checks: usize,
    pub layout_checks: usize,
}

struct Context {
    // Header/closure/resource environment only. No obsolete function bodies or
    // verifier authority are retained here.
    module: TypedModule,
    limits: zil_base::Limits,
    stored_borrows: bool,
    owned_sum: bool,
}
impl Context {
    fn checked(verified: &VerifiedModule) -> Self {
        let mut module = verified.module().clone();
        module.functions.clear();
        Self {
            module,
            limits: verified.limits().clone(),
            stored_borrows: verified.uses_stored_borrows(),
            owned_sum: verified.uses_owned_sum_extension(),
        }
    }
}
struct Epoch {
    context: Arc<Context>,
    functions: PersistentVec<TypedFunction>,
    costs: PersistentVec<[usize; 2]>,
    total: [usize; 2],
}

/// Reuses only lowering admission, never independent Verify authority. Every
/// call requires the current immutable capability. Clone is a constant-time
/// checkpoint of the last successful epoch; failures leave that epoch intact.
#[derive(Clone, Default)]
pub struct LowerSession {
    previous: Option<Arc<Epoch>>,
    stats: LowerStats,
    cost_profiling: bool,
}

impl LowerSession {
    pub fn stats(&self) -> LowerStats {
        self.stats
    }

    pub fn clear(&mut self) {
        let cost_profiling = self.cost_profiling;
        *self = Self {
            cost_profiling,
            ..Self::default()
        };
    }

    pub fn set_cost_profiling(&mut self, enabled: bool) {
        self.cost_profiling = enabled;
    }

    /// The view's lifetime belongs to `verified`, not to this session. Returned
    /// fields and the fingerprint always come from the current capability;
    /// cached costs cannot manufacture a view of a different raw module.
    pub fn lower_view<'a>(&mut self, verified: &'a VerifiedModule) -> CompileResult<LirView<'a>> {
        self.stats = LowerStats {
            cost_profiled: self.cost_profiling,
            ..LowerStats::default()
        };
        if let Some(previous) = self.previous.as_ref() {
            self.stats.context_checks = 1;
            let started = self.cost_profiling.then(Instant::now);
            let matches = previous.functions.len() == verified.module().functions.len()
                && same_context(&previous.context, verified);
            self.stats.context_check_ns = started.map_or(0, |start| {
                start.elapsed().as_nanos().min(u64::MAX as u128) as u64
            });
            if matches {
                // This walks changed persistent branches, not every clean
                // function. Position changes are actual new immutable leaves.
                let changed = verified
                    .module()
                    .functions
                    .changed_indices(&previous.functions);
                if let Some((costs, total)) =
                    updated_costs(previous, verified, &changed, &mut self.stats)
                {
                    self.stats.functions_reused = verified.module().functions.len() - changed.len();
                    self.stats.budget_replays = 1;
                    self.previous = Some(Arc::new(Epoch {
                        context: previous.context.clone(),
                        functions: verified.module().functions.clone(),
                        costs,
                        total,
                    }));
                    return Ok(current_view(verified));
                }
            }
        }
        // Any uncertain shape, failed local admission or aggregate limit uses
        // the original ordered checks. This preserves their exact first error.
        self.stats.full_checks = 1;
        let mut recording = AdmissionRecording::default();
        let result = lower_view_recording(verified, Some(&mut recording));
        self.stats.functions_checked += recording.metadata_checks;
        self.stats.layout_checks += recording.layout_checks;
        let (view, budget) = result?;
        self.previous = Some(Arc::new(Epoch {
            context: Arc::new(Context::checked(verified)),
            functions: verified.module().functions.clone(),
            costs: recording.costs.into(),
            total: budget.cost(),
        }));
        Ok(view)
    }
}

fn updated_costs(
    previous: &Epoch,
    verified: &VerifiedModule,
    changed: &[usize],
    stats: &mut LowerStats,
) -> Option<(PersistentVec<[usize; 2]>, [usize; 2])> {
    let mut costs = previous.costs.clone();
    let mut total = previous.total;
    for &index in changed {
        let function = verified.module().functions.get(index)?;
        let mut budget = owned_sums::Budget::new(verified.limits());
        stats.functions_checked += 1;
        budget.function_metadata(function).ok()?;
        stats.layout_checks += 1;
        admit_function(verified, function, &mut budget).ok()?;
        let cost = budget.cost();
        for dimension in 0..2 {
            total[dimension] = total[dimension]
                .checked_sub(costs[index][dimension])?
                .checked_add(cost[dimension])?;
        }
        costs.replace(index, cost);
    }
    let mut budget = owned_sums::Budget::new(verified.limits());
    budget.replay(total).ok()?;
    Some((costs, total))
}

fn same_context(previous: &Context, current: &VerifiedModule) -> bool {
    if &previous.limits != current.limits()
        || previous.stored_borrows != current.uses_stored_borrows()
        || previous.owned_sum != current.uses_owned_sum_extension()
    {
        return false;
    }
    // Exhaustive fields prevent a future raw schema extension from silently
    // escaping the receipt's environment. Source fingerprint is not a lowering
    // input; function content is independently bound by persistent leaf owners.
    let TypedModule {
        dyn_declarations,
        dyn_interfaces,
        dyn_vtables,
        life_declarations,
        life_instances,
        schema_version,
        module_id,
        source_fingerprint: _,
        target,
        functions: _,
        closures,
        resources,
        unsupported,
    } = current.module();
    let old = &previous.module;
    old.dyn_declarations == *dyn_declarations
        && old.dyn_interfaces == *dyn_interfaces
        && old.dyn_vtables == *dyn_vtables
        && old.life_declarations == *life_declarations
        && old.life_instances == *life_instances
        && old.schema_version == *schema_version
        && old.module_id == *module_id
        && old.target == *target
        && old.closures == *closures
        && old.resources == *resources
        && old.unsupported == *unsupported
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, Limits, TargetProfile};
    use zil_ir::{CandidatePlan, Constant, FunctionId, InstructionKind};

    fn verify_like(previous: &VerifiedModule, module: TypedModule) -> VerifiedModule {
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions = previous.plan().functions.clone();
        let target = module.target.clone();
        zil_verify::verify_module(module, plan, &target, previous.limits()).unwrap()
    }

    fn many_functions(count: u32) -> VerifiedModule {
        let single = crate::tests::checked(TargetProfile::host_llvm());
        let mut module = single.module().clone();
        let mut plan = single.plan().clone();
        for id in 1..count {
            let mut function = module.functions[0].clone();
            function.id = FunctionId(id);
            function.name = format!("function_{id}");
            module.functions.push(function);
            let mut row = plan.functions[0].clone();
            row.function = FunctionId(id);
            plan.functions.push(row);
        }
        plan.module_fingerprint = module.fingerprint();
        let target = module.target.clone();
        zil_verify::verify_module(module, plan, &target, single.limits()).unwrap()
    }

    #[test]
    fn persistent_lowering_checks_only_changed_actual_leaves_and_binds_current_proof() {
        let original = many_functions(128);
        let mut session = LowerSession::default();
        session.lower_view(&original).unwrap();
        assert_eq!(session.stats().functions_checked, 128);
        assert_eq!(session.stats().layout_checks, 128);
        let checkpoint = session.clone();
        session.lower_view(&original).unwrap();
        assert_eq!(session.stats().functions_checked, 0);
        assert_eq!(session.stats().layout_checks, 0);
        assert_eq!(session.stats().functions_reused, 128);
        assert_eq!(session.stats().full_checks, 0);
        assert_eq!(session.stats().context_check_ns, 0);
        let mut module = original.module().clone();
        module.source_fingerprint = Fingerprint::of(b"new exact input epoch");
        module.functions[7].blocks[0].instructions[0].kind =
            InstructionKind::Constant(Constant::I32(19));
        let edited = verify_like(&original, module);
        let view = session.lower_view(&edited).unwrap();
        assert_eq!(session.stats().functions_checked, 1);
        assert_eq!(session.stats().layout_checks, 1);
        assert_eq!(session.stats().functions_reused, 127);
        assert_eq!(session.stats().budget_replays, 1);
        assert!(view.functions.ptr_eq(&edited.module().functions));
        assert_eq!(view.verified_fingerprint, edited.fingerprint());
        assert_ne!(view.verified_fingerprint, original.fingerprint());
        assert_eq!(view.to_owned(), lower_view(&edited).unwrap().to_owned());
        let mut restored = checkpoint;
        restored.lower_view(&original).unwrap();
        assert_eq!(restored.stats().functions_checked, 0);
    }

    #[test]
    fn equal_raw_contents_without_shared_leaf_owners_are_rechecked() {
        let original = many_functions(8);
        let mut session = LowerSession::default();
        session.lower_view(&original).unwrap();
        let mut module = original.module().clone();
        module.functions = module.functions.iter().cloned().collect();
        let rebound = verify_like(&original, module);
        session.lower_view(&rebound).unwrap();
        assert_eq!(session.stats().functions_checked, 8);
        assert_eq!(session.stats().functions_reused, 0);
    }

    #[test]
    fn unsupported_current_target_fails_fresh_and_keeps_the_successful_epoch() {
        let original = crate::tests::checked(TargetProfile::host_llvm());
        let mut session = LowerSession::default();
        session.lower_view(&original).unwrap();
        let mut target = TargetProfile::host_llvm();
        target.triple = "aarch64-unknown-linux-gnu".into();
        let unsupported = crate::tests::checked(target);
        assert_eq!(
            session.lower_view(&unsupported).unwrap_err(),
            lower_view(&unsupported).unwrap_err()
        );
        assert_eq!(session.stats().full_checks, 1);
        session.lower_view(&original).unwrap();
        assert_eq!(session.stats().functions_checked, 0);
        assert_eq!(session.stats().functions_reused, 1);
    }

    #[test]
    fn current_limits_and_module_context_are_not_inherited_from_the_old_epoch() {
        let original = crate::tests::checked(TargetProfile::host_llvm());
        let mut session = LowerSession::default();
        session.lower_view(&original).unwrap();
        let mut module = original.module().clone();
        module.module_id = zil_base::ModuleId(99);
        let changed_context = verify_like(&original, module);
        let view = session.lower_view(&changed_context).unwrap();
        assert_eq!(view.module_id, zil_base::ModuleId(99));
        assert_eq!(session.stats().full_checks, 1);
        let mut limits = original.limits().clone();
        limits.max_nodes -= 1;
        let changed_limits = zil_verify::verify_module(
            changed_context.module().clone(),
            changed_context.plan().clone(),
            &changed_context.module().target,
            &limits,
        )
        .unwrap();
        session.lower_view(&changed_limits).unwrap();
        assert_eq!(session.stats().full_checks, 1);
        assert_eq!(session.stats().functions_reused, 0);
    }

    #[test]
    fn cached_costs_equal_fresh_costs_and_replay_exact_budget_boundaries() {
        let original = many_functions(4);
        let mut session = LowerSession::default();
        session.lower_view(&original).unwrap();
        let mut module = original.module().clone();
        module.functions[2].values.push(Type::I32);
        module.functions[2].blocks[0]
            .instructions
            .push(zil_ir::Instruction {
                result: Some(zil_ir::ValueId(1)),
                kind: InstructionKind::Constant(Constant::I32(9)),
                span: zil_base::Span::default(),
            });
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions = original.plan().functions.clone();
        plan.functions[2].holds.push(zil_ir::ValueHold {
            value: zil_ir::ValueId(1),
            roots: vec![],
        });
        let target = module.target.clone();
        let edited = zil_verify::verify_module(module, plan, &target, original.limits()).unwrap();
        session.lower_view(&edited).unwrap();
        let cached = session.previous.as_ref().unwrap().total;
        let fresh = lower_view_with_budget(&edited).unwrap().1.cost();
        assert_eq!(cached, fresh);
        assert!(cached[0] > 0 && cached[1] > 0);
        let exact = Limits {
            max_nodes: cached[0],
            max_analysis_steps: cached[1],
            ..Limits::default()
        };
        owned_sums::Budget::new(&exact).replay(cached).unwrap();
        for dimension in 0..2 {
            let mut short = exact.clone();
            if dimension == 0 {
                short.max_nodes -= 1;
            } else {
                short.max_analysis_steps -= 1;
            }
            let errors = owned_sums::Budget::new(&short).replay(cached).unwrap_err();
            assert!(errors.iter().all(|error| error.phase == Phase::Lower
                && error.category == zil_base::ResultCategory::ResourceLimit));
        }
    }
}
