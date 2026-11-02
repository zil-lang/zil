//! One request ledger and frozen source authority for raw CT compilation,
//! independent admission, and VM execution.
use super::{authority::FrozenAuthority, compiler::{self, CompileRequest, Entry}, read_view::FrozenReadView, render::{self, ParsedSyntax}, snapshot::{self, ShapeSnapshot}, source_index::SourceIndex};
use std::collections::BTreeMap;
use std::sync::Arc;
use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, Span, TargetProfile};
use zil_ct::{CtExecutionBudget, CtReadView};
use zil_ir::{CoreImport, ct_artifact::*};
use zil_syntax::ParseResult;

fn error(message: &str, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-SOURCE", Some(span), message)]
}

pub(super) struct Environment<'a> {
    sources: &'a [SourceUnit],
    parsed: &'a [Arc<ParseResult>],
    pub index: SourceIndex,
    pub shape: Option<ShapeSnapshot>,
    shape_failure: Option<Vec<Diagnostic>>,
    snapshot: CtSnapshotRequest,
    limits: Limits,
    pub budget: CtExecutionBudget,
    source_fingerprint: Fingerprint,
    target: &'a TargetProfile,
}

impl<'a> Environment<'a> {
    pub(super) fn new(
        sources: &'a [SourceUnit], parsed: &'a [Arc<ParseResult>], paths: &BTreeMap<Vec<String>, ModuleId>,
        imports: &[Vec<CoreImport>], root: ModuleId, fingerprint: Fingerprint,
        target: &'a TargetProfile, limits: &Limits,
    ) -> CompileResult<Self> {
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let index = SourceIndex::freeze(sources, &syntax, paths)?;
        // This batch contains only prior, independent ordinary declarations.
        // Failure makes the view unavailable; a query reports the exact phase
        // boundary, while final ordinary checking still diagnoses the source.
        let (shape, shape_failure) = match snapshot::freeze_shapes(sources, &syntax, imports, root, fingerprint, target, limits) {
            Ok(shape) => (Some(shape), None),
            Err(errors) if errors.iter().all(|error| matches!(error.category, ResultCategory::SafeReject | ResultCategory::Unsupported)) => (None, Some(errors)),
            Err(errors) => return Err(errors),
        };
        let ready = if shape.is_some() { CtReadyStage::Layout } else { CtReadyStage::Config };
        let mut identity = b"zil/ct-project-snapshot/v1\0".to_vec();
        identity.extend_from_slice(&fingerprint.0);
        identity.extend_from_slice(&target.fingerprint().0);
        identity.push(ready as u8);
        let snapshot = CtSnapshotRequest { fingerprint: Fingerprint::of(&identity), ready,
            target_fingerprint: Some(target.fingerprint()),
            requested_queries: if shape.is_some() { vec![CtQueryKind::Shape, CtQueryKind::Field, CtQueryKind::Layout] } else { Vec::new() } };
        let declared = CtDeclaredBudget {
            max_steps: limits.max_ct_steps,
            max_call_depth: u32::try_from(limits.max_call_depth).unwrap_or(u32::MAX),
            max_live_cells: limits.max_values as u64,
            max_temporary_bytes: limits.max_source_bytes as u64,
            max_generated_nodes: limits.max_nodes as u64,
            max_instances: limits.max_functions as u64,
            max_analysis_work: limits.max_analysis_steps as u64,
        };
        Ok(Self { sources, parsed, index, shape, shape_failure, snapshot, limits: limits.clone(), budget: CtExecutionBudget::new(declared), source_fingerprint: fingerprint, target })
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

    pub(super) fn remaining_limits_for_input(&self) -> Limits { self.remaining_limits() }

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
            shape: self.shape.as_ref(), shape_failure: self.shape_failure.as_deref(), input, limits: &selected_limits, target: self.target, snapshot: &self.snapshot };
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

    pub(super) fn parse_tree(&mut self, tree: &CtSyntaxTree, span: Span) -> CompileResult<ParsedSyntax> {
        render::parse(tree, span.file, self.sources.iter().find(|source| source.file_id == span.file)
            .map_or(ModuleId(0), |source| source.module_id), &self.limits, &mut self.budget)
    }

    pub(super) fn type_syntax(&mut self, ty: CtTypeRef) -> CompileResult<CtSyntaxTree> {
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView { index: &self.index, sources: self.sources, syntax: &syntax,
            shape: self.shape.as_ref(), shape_failure: self.shape_failure.as_deref(), input: None,
            limits: &self.limits, target: self.target, snapshot: &self.snapshot };
        reader.build_syntax(CtSyntaxBuilder::TypeRef, &[CtValue::Type(ty)], &mut self.budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::FileId;

    #[test]
    fn source_function_passes_frozen_authority_independent_verify_and_vm() {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "ct.zil", "fn identity(t:Type)->Type{return t}");
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let mut environment = Environment::new(&sources, &parsed, &BTreeMap::new(), &[Vec::new()], ModuleId(0), sources[0].fingerprint(), &target, &limits).unwrap();
        let item = &parsed[0].module.items[0];
        let symbol = environment.index.symbol_at(ModuleId(0), item.span).unwrap();
        let input = environment.empty_input(item.span).unwrap();
        let argument = CtValue::Type(CtTypeRef { nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)], root: CtTypeNodeId(0) });
        let result = environment.evaluate(Entry::Function(symbol), &input, None, &[argument.clone()], item.span).unwrap();
        assert_eq!(result, argument);
    }
}
