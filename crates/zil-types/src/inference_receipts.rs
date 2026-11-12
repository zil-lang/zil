//! Private successful raw inference identities, not Types/CT/Verify authority.
//! Fee columns retain their original phase order and independent ledgers. A
//! failed replay is discarded and the original current traversal selects errors.
use super::*;
use std::collections::BTreeSet;
use std::sync::Arc;
use zil_expand::{ExpansionEpoch, FunctionOwner, InputContext, ProducedProgram};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawInferenceStats {
    pub full_runs: usize,
    pub sparse_batches: usize,
    pub replay_fallbacks: usize,
    pub closure_bodies_checked: usize,
    pub closure_bodies_reused: usize,
    pub name_functions_checked: usize,
    pub name_functions_reused: usize,
    pub dependency_bodies_checked: usize,
    pub dependency_bodies_reused: usize,
}

impl RawInferenceStats {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"full_runs\":{},\"sparse_batches\":{},\"replay_fallbacks\":{},\"closure_bodies_checked\":{},\"closure_bodies_reused\":{},\"name_functions_checked\":{},\"name_functions_reused\":{},\"dependency_bodies_checked\":{},\"dependency_bodies_reused\":{}}}",
            self.full_runs,
            self.sparse_batches,
            self.replay_fallbacks,
            self.closure_bodies_checked,
            self.closure_bodies_reused,
            self.name_functions_checked,
            self.name_functions_reused,
            self.dependency_bodies_checked,
            self.dependency_bodies_reused,
        )
    }
}

/// A bounded compiler-local raw epoch. Clone is an immutable checkpoint; no
/// decoded Core or downstream Typed result can mint its successful receipts.
#[derive(Clone, Debug, Default)]
pub struct RawInferenceSession {
    current: Option<Arc<Context>>,
}
impl RawInferenceSession {
    pub fn clear(&mut self) {
        self.current = None;
    }
    pub fn checkpoint_for(&self, program: &Arc<CoreProgram>) -> Option<Self> {
        Arc::ptr_eq(&self.current.as_ref()?.program, program).then(|| self.clone())
    }
    pub(super) fn infer<'a>(
        &mut self,
        produced: &'a ProducedProgram,
        limits: &Limits,
    ) -> CompileResult<(std::borrow::Cow<'a, CoreProgram>, RawInferenceStats)> {
        let mut work = self
            .current
            .as_ref()
            .and_then(|prior| Work::reusing(prior, produced, limits));
        if let Some(ref mut work) = work
            && let Ok(result) =
                inference::infer_program_recording(produced.program(), limits, Some(work))
        {
            let stats = work.stats.clone();
            if matches!(result, std::borrow::Cow::Borrowed(_)) && work.eligible {
                self.current = Some(Arc::new(work.finish(produced, limits)));
            } else {
                self.current = None;
            }
            return Ok((result, stats));
        }
        // Replay cannot choose a diagnostic span or first error. The original
        // ordered visitors run with the complete current input and allowances.
        let fallback = work.is_some();
        let mut fresh = Work::fresh(produced, limits);
        let result =
            inference::infer_program_recording(produced.program(), limits, fresh.as_mut())?;
        let mut stats = fresh.as_ref().map_or_else(
            || RawInferenceStats {
                full_runs: 1,
                ..RawInferenceStats::default()
            },
            |work| work.stats.clone(),
        );
        stats.replay_fallbacks = usize::from(fallback);
        self.current = if matches!(result, std::borrow::Cow::Borrowed(_)) {
            fresh
                .filter(|work| work.eligible)
                .map(|work| Arc::new(work.finish(produced, limits)))
        } else {
            None
        };
        Ok((result, stats))
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct VisitFee {
    pub work: usize,
    pub nodes: usize,
}
#[derive(Clone, Debug)]
struct Row {
    owner: FunctionOwner,
    node: zil_base::NodeId,
    span: Span,
    location: (usize, usize),
    closure: VisitFee,
    names: usize,
    dependencies: usize,
}
#[derive(Clone, Debug)]
struct Context {
    program: Arc<CoreProgram>,
    epoch: ExpansionEpoch,
    input: InputContext,
    limits: Limits,
    offsets: Arc<[usize]>,
    rows: PersistentVec<Row>,
}

pub(super) struct Work {
    rows: PersistentVec<Row>,
    offsets: Arc<[usize]>,
    dirty: BTreeSet<usize>,
    reuse: bool,
    pub eligible: bool,
    pub stats: RawInferenceStats,
}
impl Work {
    fn fresh(produced: &ProducedProgram, limits: &Limits) -> Option<Self> {
        if produced.program().modules.len() > limits.max_nodes
            || limits.max_analysis_steps == 0
            || produced
                .program()
                .modules
                .iter()
                .try_fold(0usize, |total, module| {
                    total.checked_add(module.core.functions.len())
                })?
                > limits.max_functions
        {
            return None;
        }
        let mut rows = Vec::new();
        let mut offsets = Vec::new();
        for (module_index, module) in produced.program().modules.iter().enumerate() {
            offsets.push(rows.len());
            for (function_index, function) in module.core.functions.iter().enumerate() {
                rows.push(Row {
                    owner: produced
                        .function_owner(module.core.module_id, &function.name)?
                        .clone(),
                    node: function.id,
                    span: function.span,
                    location: (module_index, function_index),
                    closure: VisitFee::default(),
                    names: 0,
                    dependencies: 0,
                });
            }
        }
        Some(Self {
            rows: rows.into(),
            offsets: offsets.into(),
            dirty: BTreeSet::new(),
            reuse: false,
            eligible: true,
            stats: RawInferenceStats {
                full_runs: 1,
                ..RawInferenceStats::default()
            },
        })
    }
    fn reusing(prior: &Context, produced: &ProducedProgram, limits: &Limits) -> Option<Self> {
        let batch = produced.change_batch();
        if limits != &prior.limits
            || batch.previous_epoch() != Some(&prior.epoch)
            || batch.context() != &prior.input
            || produced.program().root != prior.program.root
            || produced.program().modules.len() != prior.offsets.len()
        {
            return None;
        }
        let mut rows = prior.rows.clone();
        let mut dirty = BTreeSet::new();
        for change in batch.changes() {
            let (module, function) = produced.changed_function(change)?;
            let index = prior
                .offsets
                .get(change.module_index())?
                .checked_add(change.function_index())?;
            let old = rows.get(index)?;
            let old_function = prior
                .program
                .modules
                .get(change.module_index())?
                .core
                .functions
                .get(change.function_index())?;
            if old.owner != *change.owner()
                || old_function.name != function.name
                || old.location != (change.module_index(), change.function_index())
                || change.previous_node_id() != Some(old.node)
                || change.previous_span() != Some(old.span)
                || old.span.file != function.span.file
                || module.core.module_id
                    != prior
                        .program
                        .modules
                        .get(change.module_index())?
                        .core
                        .module_id
                || change.interface_changed()
            {
                return None;
            }
            let mut row = old.clone();
            row.node = function.id;
            row.span = function.span;
            rows.replace(index, row);
            if change.body_changed() {
                dirty.insert(index);
            }
        }
        Some(Self {
            rows,
            offsets: prior.offsets.clone(),
            dirty,
            reuse: true,
            eligible: true,
            stats: RawInferenceStats {
                sparse_batches: 1,
                ..RawInferenceStats::default()
            },
        })
    }
    fn finish(&self, produced: &ProducedProgram, limits: &Limits) -> Context {
        Context {
            program: produced.program().clone(),
            epoch: produced.change_batch().epoch().clone(),
            input: produced.change_batch().context().clone(),
            limits: limits.clone(),
            offsets: self.offsets.clone(),
            rows: self.rows.clone(),
        }
    }
    pub fn is_reusing(&self) -> bool {
        self.reuse
    }
    pub fn complete(&self, count: usize) -> bool {
        self.rows.len() == count
    }
    fn reusable(&self, index: usize) -> bool {
        self.reuse && !self.dirty.contains(&index)
    }
    pub fn closure_fee(&mut self, index: usize) -> Option<VisitFee> {
        if self.reusable(index) {
            self.stats.closure_bodies_reused += 1;
            Some(self.rows.get(index)?.closure.clone())
        } else {
            self.stats.closure_bodies_checked += 1;
            None
        }
    }
    pub fn record_closure(&mut self, index: usize, fee: VisitFee) {
        if let Some(row) = self.rows.get_mut(index) {
            row.closure = fee;
        }
    }
    pub fn names_fee(&mut self, index: usize) -> Option<usize> {
        if self.reusable(index) {
            self.stats.name_functions_reused += 1;
            Some(self.rows.get(index)?.names)
        } else {
            self.stats.name_functions_checked += 1;
            None
        }
    }
    pub fn record_names(&mut self, index: usize, fee: usize) {
        if let Some(row) = self.rows.get_mut(index) {
            row.names = fee;
        }
    }
    pub fn dependencies_fee(&mut self, index: usize) -> Option<usize> {
        if self.reusable(index) {
            self.stats.dependency_bodies_reused += 1;
            Some(self.rows.get(index)?.dependencies)
        } else {
            self.stats.dependency_bodies_checked += 1;
            None
        }
    }
    pub fn record_dependencies(&mut self, index: usize, fee: usize) {
        if let Some(row) = self.rows.get_mut(index) {
            row.dependencies = fee;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, SourceUnit};
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
        expand: &mut ExpandSession,
        sources: &[ParsedSource],
        limits: &Limits,
    ) -> ProducedProgram {
        let aliases = BTreeMap::new();
        expand
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
    fn prepare(
        session: &mut RawInferenceSession,
        produced: &ProducedProgram,
        limits: &Limits,
    ) -> CompileResult<(PreparedProgram, RawInferenceStats)> {
        let inferred = infer_produced_program_for_ct_with_session(produced, limits, session)?;
        let stats = inferred.raw_inference_stats().unwrap().clone();
        Ok((inferred.into_prepared(), stats))
    }
    fn fresh(produced: &ProducedProgram, limits: &Limits) -> CompileResult<PreparedProgram> {
        infer_produced_program_for_ct(produced, limits).map(InferredProgram::into_prepared)
    }
    fn assert_cfg_current(prepared: &PreparedProgram, current: &ProducedProgram, limits: &Limits) {
        assert!(
            std::ptr::eq(prepared.program(), current.program().as_ref()),
            "actual current Core is retained"
        );
        assert_eq!(
            check_prepared_program(prepared, &TargetProfile::default(), limits),
            check_program(current.program(), &TargetProfile::default(), limits)
        );
    }
    fn text(count: usize) -> String {
        (0..count)
            .map(|i| format!("fn f{i}(x:i32)->i32 {{let a=x+1;return a+2;}} "))
            .collect()
    }

    #[test]
    fn full_425_function_private_and_positions_use_current_core_and_one_real_visit() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = RawInferenceSession::default();
        let mut sources = (0..22)
            .map(|m| parse(m, &text(if m == 21 { 5 } else { 20 }), &limits))
            .collect::<Vec<_>>();
        let initial = produced(&mut expand, &sources, &limits);
        let (cold, stats) = prepare(&mut session, &initial, &limits).unwrap();
        assert_eq!(
            (
                stats.full_runs,
                stats.closure_bodies_checked,
                stats.name_functions_checked,
                stats.dependency_bodies_checked
            ),
            (1, 425, 425, 425)
        );
        let repeated = produced(&mut expand, &sources, &limits);
        let (_, stats) = prepare(&mut session, &repeated, &limits).unwrap();
        assert_eq!(
            (
                stats.sparse_batches,
                stats.closure_bodies_checked,
                stats.closure_bodies_reused,
                stats.name_functions_reused,
                stats.dependency_bodies_reused
            ),
            (1, 0, 425, 425, 425)
        );
        let producer_checkpoint = expand.checkpoint();
        let checkpoint = session.checkpoint_for(repeated.program()).unwrap();
        assert!(
            session
                .checkpoint_for(&Arc::new(repeated.program().as_ref().clone()))
                .is_none()
        );
        sources[0] = parse(0, &text(20).replacen("x+1", "x+111", 1), &limits);
        let private = produced(&mut expand, &sources, &limits);
        let (current, stats) = prepare(&mut session, &private, &limits).unwrap();
        assert_eq!(
            (
                stats.closure_bodies_checked,
                stats.closure_bodies_reused,
                stats.name_functions_checked,
                stats.name_functions_reused,
                stats.dependency_bodies_checked,
                stats.dependency_bodies_reused
            ),
            (1, 424, 1, 424, 1, 424)
        );
        assert_cfg_current(&current, &private, &limits);
        assert_eq!(
            cold.program(),
            initial.program().as_ref(),
            "old raw owner remains immutable"
        );
        assert!(session.checkpoint_for(repeated.program()).is_none());
        expand = producer_checkpoint;
        session = checkpoint;
        for (id, source) in sources.iter_mut().enumerate() {
            *source = parse(
                id as u32,
                &(String::from("// current positions\n") + &text(if id == 21 { 5 } else { 20 })),
                &limits,
            );
        }
        let positions = produced(&mut expand, &sources, &limits);
        let (current, stats) = prepare(&mut session, &positions, &limits).unwrap();
        assert_eq!(
            (
                stats.closure_bodies_checked,
                stats.closure_bodies_reused,
                stats.name_functions_checked,
                stats.dependency_bodies_checked
            ),
            (0, 425, 0, 0)
        );
        assert!(current.program().modules.iter().all(|module| {
            module
                .core
                .functions
                .iter()
                .all(|function| function.span.start >= 21)
        }));
        assert_cfg_current(&current, &positions, &limits);
        let weak = Arc::downgrade(session.current.as_ref().unwrap());
        session.clear();
        assert!(
            weak.upgrade().is_none(),
            "no unbounded independent epoch history"
        );
    }

    fn minimum(program: &ProducedProgram, analysis: bool) -> usize {
        let defaults = Limits::default();
        let mut low = 0;
        let mut high = if analysis {
            defaults.max_analysis_steps
        } else {
            defaults.max_nodes
        };
        while low < high {
            let mid = low + (high - low) / 2;
            let mut limits = defaults.clone();
            if analysis {
                limits.max_analysis_steps = mid;
            } else {
                limits.max_nodes = mid;
            }
            if fresh(program, &limits).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        low
    }
    #[test]
    fn same_limits_aggregate_work_and_nodes_fail_at_the_original_current_error_and_recover() {
        let defaults = Limits::default();
        for analysis in [true, false] {
            let mut expand = ExpandSession::default();
            let baseline = text(12);
            let initial = produced(&mut expand, &[parse(0, &baseline, &defaults)], &defaults);
            let bound = minimum(&initial, analysis);
            assert!(bound > 12);
            let mut limits = defaults.clone();
            if analysis {
                limits.max_analysis_steps = bound;
            } else {
                limits.max_nodes = bound;
            }
            let mut session = RawInferenceSession::default();
            prepare(&mut session, &initial, &limits).unwrap();
            let old = session.current.as_ref().unwrap().clone();
            let producer_checkpoint = expand.checkpoint();
            let edited =
                baseline.replacen("x+1", if analysis { "x+111111111" } else { "x+1+2" }, 1);
            let current = produced(&mut expand, &[parse(0, &edited, &defaults)], &defaults);
            let mut one_expand = ExpandSession::default();
            let one = produced(
                &mut one_expand,
                &[parse(
                    0,
                    &text(1).replacen("x+1", if analysis { "x+111111111" } else { "x+1+2" }, 1),
                    &defaults,
                )],
                &defaults,
            );
            assert!(
                fresh(&one, &limits).is_ok(),
                "changed leaf fits; only aggregate must reject"
            );
            let expected = fresh(&current, &limits)
                .expect_err("aggregate N is exhausted by the edit");
            assert_eq!(
                prepare(&mut session, &current, &limits).err().unwrap(),
                expected
            );
            assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &old));
            expand = producer_checkpoint;
            let recovery = produced(&mut expand, &[parse(0, &baseline, &defaults)], &defaults);
            let (prepared, stats) = prepare(&mut session, &recovery, &limits).unwrap();
            assert_eq!((stats.sparse_batches, stats.closure_bodies_checked), (1, 0));
            assert_eq!(prepared.program(), initial.program().as_ref());
            if analysis {
                limits.max_analysis_steps -= 1;
            } else {
                limits.max_nodes -= 1;
            }
            assert_eq!(
                prepare(&mut session, &recovery, &limits).err().unwrap(),
                fresh(&recovery, &limits).err().unwrap()
            );
        }
    }

    #[test]
    fn names_errors_current_lowered_discard_bindings_and_history_keep_fresh_semantics() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = RawInferenceSession::default();
        let baseline = "fn first(x:i32)->i32 {let a=x+1;return a;} fn second(x:i32)->i32 {let _=x+2;return x+3;} fn third(x:i32)->i32=>x+4;";
        let initial = produced(&mut expand, &[parse(0, baseline, &limits)], &limits);
        prepare(&mut session, &initial, &limits).unwrap();
        let checkpoint = expand.checkpoint();
        let old = session.current.as_ref().unwrap().clone();
        let invalid = baseline.replacen("let a=x+1", "let i32=x+1", 1);
        let invalid = produced(&mut expand, &[parse(0, &invalid, &limits)], &limits);
        assert_eq!(
            prepare(&mut session, &invalid, &limits).err().unwrap(),
            fresh(&invalid, &limits).err().unwrap()
        );
        assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &old));
        expand = checkpoint;
        let inserted = baseline.replacen("let a=x+1;", "let extra=x+9;let a=x+1;", 1);
        let inserted = produced(&mut expand, &[parse(0, &inserted, &limits)], &limits);
        let (prepared, stats) = prepare(&mut session, &inserted, &limits).unwrap();
        assert_eq!(
            (
                stats.closure_bodies_checked,
                stats.name_functions_checked,
                stats.dependency_bodies_checked
            ),
            (2, 2, 2),
            "node-numbered discard names are actual raw inference inputs"
        );
        assert_cfg_current(&prepared, &inserted, &limits);
        let sibling = baseline
            .replacen("let a=x+1;", "let extra=x+9;let a=x+1;", 1)
            .replacen("return x+3", "return x+333", 1);
        let sibling = produced(&mut expand, &[parse(0, &sibling, &limits)], &limits);
        let (prepared, stats) = prepare(&mut session, &sibling, &limits).unwrap();
        assert_eq!(stats.closure_bodies_checked, 1);
        assert_cfg_current(&prepared, &sibling, &limits);
    }

    #[test]
    fn current_headers_and_unsealed_complex_inputs_retain_the_original_pipeline() {
        let limits = Limits::default();
        let mut expand = ExpandSession::default();
        let mut session = RawInferenceSession::default();
        let initial = produced(&mut expand, &[parse(0, &text(3), &limits)], &limits);
        prepare(&mut session, &initial, &limits).unwrap();
        let changed = text(3).replacen("x:i32)->i32", "x:u32)->u32", 1);
        let changed = produced(&mut expand, &[parse(0, &changed, &limits)], &limits);
        let (prepared, stats) = prepare(&mut session, &changed, &limits).unwrap();
        assert_eq!(
            (stats.full_runs, stats.sparse_batches),
            (1, 0),
            "full raw header context changed"
        );
        assert_cfg_current(&prepared, &changed, &limits);
        let materialized =
            infer_produced_program_for_ct_with_session(&changed, &limits, &mut session)
                .unwrap()
                .after_evaluation(changed.program().as_ref().clone());
        assert!(
            materialized.produced().is_none(),
            "CT materialization does not mint a producer seal"
        );
        let closure = parse(0, "fn main()->i32 {let f=(x)=>x+1;return f(2);}", &limits);
        let aliases = BTreeMap::new();
        assert!(
            expand
                .expand_parsed(
                    std::slice::from_ref(&closure),
                    &[vec![]],
                    &limits,
                    &TargetProfile::default(),
                    SourceIdentityContext::ClosedSnapshot {
                        root: ModuleId(0),
                        aliases: &aliases,
                        declarations: DeclarationPlacement::Original
                    }
                )
                .unwrap()
                .is_none()
        );
        let modules = zil_expand::expand_program_with_limits(
            &[closure.source().clone()],
            &[&closure.parsed().module],
            &[vec![]],
            &limits,
        )
        .unwrap();
        let raw = Arc::new(CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            modules: modules.into_iter().map(|(module, _)| module).collect(),
        });
        let inferred = infer_shared_program_for_ct(&raw, &limits).unwrap();
        assert!(inferred.raw_inference_stats().is_none());
        let prepared = inferred.into_prepared();
        assert!(prepared.produced().is_none());
        assert_eq!(
            check_prepared_program(&prepared, &TargetProfile::default(), &limits),
            check_program(&raw, &TargetProfile::default(), &limits)
        );
    }
}
