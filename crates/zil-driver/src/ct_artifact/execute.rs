//! One request ledger and frozen source authority for raw CT compilation,
//! independent admission, and VM execution.
use super::{authority::FrozenAuthority, compiler::{self, CompileRequest, Entry}, read_view::FrozenReadView, snapshot::{self, ShapeSnapshot}, source_index::SourceIndex};
use std::collections::BTreeMap;
use std::sync::Arc;
use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, Phase, SourceUnit, Span, TargetProfile};
use zil_ct::CtExecutionBudget;
use zil_ir::{CoreImport, ct_artifact::*};
use zil_syntax::{ParseResult, ast as s};

fn error(message: &str, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-SOURCE", Some(span), message)]
}

pub(super) struct Environment<'a> {
    sources: &'a [SourceUnit],
    parsed: &'a [Arc<ParseResult>],
    pub index: SourceIndex,
    pub shape: Option<ShapeSnapshot>,
    snapshot: CtSnapshotRequest,
    limits: Limits,
    pub budget: CtExecutionBudget,
    source_fingerprint: Fingerprint,
}

impl<'a> Environment<'a> {
    pub(super) fn new(
        sources: &'a [SourceUnit], parsed: &'a [Arc<ParseResult>], paths: &BTreeMap<Vec<String>, ModuleId>,
        imports: &[Vec<CoreImport>], root: ModuleId, fingerprint: Fingerprint,
        target: &TargetProfile, limits: &Limits,
    ) -> CompileResult<Self> {
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let index = SourceIndex::freeze(sources, &syntax, paths)?;
        // This batch contains only prior, independent ordinary declarations.
        // Failure makes the view unavailable; a query reports the exact phase
        // boundary, while final ordinary checking still diagnoses the source.
        let shape = snapshot::freeze_shapes(sources, &syntax, imports, root, fingerprint, target, limits).ok();
        let ready = if shape.is_some() { CtReadyStage::Shape } else { CtReadyStage::Config };
        let mut identity = b"zil/ct-project-snapshot/v1\0".to_vec();
        identity.extend_from_slice(&fingerprint.0);
        identity.extend_from_slice(&target.fingerprint().0);
        identity.push(ready as u8);
        let snapshot = CtSnapshotRequest { fingerprint: Fingerprint::of(&identity), ready,
            target_fingerprint: Some(target.fingerprint()),
            requested_queries: if shape.is_some() { vec![CtQueryKind::Shape, CtQueryKind::Field] } else { Vec::new() } };
        let declared = CtDeclaredBudget {
            max_steps: limits.max_ct_steps,
            max_call_depth: u32::try_from(limits.max_call_depth).unwrap_or(u32::MAX),
            max_live_cells: limits.max_values as u64,
            max_temporary_bytes: limits.max_source_bytes as u64,
            max_generated_nodes: limits.max_nodes as u64,
            max_instances: limits.max_functions as u64,
            max_analysis_work: limits.max_analysis_steps as u64,
        };
        Ok(Self { sources, parsed, index, shape, snapshot, limits: limits.clone(), budget: CtExecutionBudget::new(declared), source_fingerprint: fingerprint })
    }

    fn remaining_limits(&self) -> Limits {
        let remaining = self.budget.remaining();
        let mut limits = self.limits.clone();
        limits.max_ct_steps = remaining.max_steps;
        limits.max_call_depth = limits.max_call_depth.min(remaining.max_call_depth as usize);
        limits.max_nodes = limits.max_nodes.min(remaining.max_generated_nodes as usize);
        limits.max_functions = limits.max_functions.min(remaining.max_instances as usize);
        limits.max_analysis_steps = limits.max_analysis_steps.min(remaining.max_analysis_work as usize);
        limits.max_source_bytes = limits.max_source_bytes.min(remaining.max_temporary_bytes as usize);
        limits.max_values = limits.max_values.min(remaining.max_live_cells as usize);
        limits
    }

    pub(super) fn evaluate(&mut self, entry: Entry<'_>, input_tokens: &CtInputTable, input: Option<&CtTokenTree>, arguments: &[CtValue], span: Span) -> CompileResult<CtValue> {
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        let selected_limits = self.remaining_limits();
        let expected = compiler::compile(CompileRequest {
            modules: &modules, source_fingerprint: self.source_fingerprint, entry,
            input_tokens, snapshot: &self.snapshot, limits: &selected_limits,
        })?;
        let declared = expected.budget;
        let before = self.budget.remaining();
        self.budget.reserve_analysis_work(before.max_analysis_work.saturating_sub(declared.max_analysis_work))?;
        self.budget.reserve_generated_nodes(before.max_generated_nodes.saturating_sub(declared.max_generated_nodes))?;
        for _ in 0..before.max_instances.saturating_sub(declared.max_instances) { self.budget.reserve_instance()?; }
        let copied_bytes = expected.constants.iter().map(|constant| match constant { CtConstant::Text(value) => value.len() as u64, _ => 16 }).sum::<u64>()
            .saturating_add(expected.templates.iter().map(|template| template.syntax.nodes.len() as u64 * 96).sum::<u64>())
            .saturating_add(expected.functions.iter().map(|function| function.blocks.len() as u64 * 96).sum::<u64>());
        self.budget.reserve_temporary_bytes(copied_bytes)?;
        let raw = expected.clone();
        let authority = FrozenAuthority { source_fingerprint: self.source_fingerprint, snapshot: &self.snapshot,
            input: input_tokens, index: &self.index, expected_functions: &expected.functions, expected_artifact: &expected };
        let verified = zil_verify::verify_ct_artifact(raw, &selected_limits, &authority)?;
        let reader = FrozenReadView { index: &self.index, sources: self.sources, syntax: &syntax,
            shape: self.shape.as_ref(), input, limits: &selected_limits };
        let (value, _usage) = zil_ct::evaluate_ct_artifact(&verified, arguments, &reader, &mut self.budget)
            .map_err(|mut errors| { for diagnostic in &mut errors { if diagnostic.span.is_none() { diagnostic.span = Some(span); } } errors })?;
        self.budget.release_temporary_bytes(copied_bytes);
        Ok(value)
    }

    pub(super) fn empty_input(&self, span: Span) -> CompileResult<CtInputTable> {
        let source = self.sources.iter().find(|source| source.file_id == span.file).ok_or_else(|| error("CT expression is not in project source", span))?;
        let mut bytes = b"zil/ct-expression-invocation/v1\0".to_vec();
        bytes.extend_from_slice(&source.fingerprint().0);
        bytes.extend_from_slice(&span.start.to_le_bytes());
        bytes.extend_from_slice(&span.end.to_le_bytes());
        Ok(CtInputTable { invocation_fingerprint: Fingerprint::of(&bytes), tokens: Vec::new() })
    }
}
