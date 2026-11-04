//! One request ledger and frozen source authority for raw CT compilation,
//! independent admission, and VM execution.
use super::{
    authority::FrozenAuthority,
    compiler::{self, CompileRequest, Entry},
    read_view::FrozenReadView,
    render::{self, ParsedSyntax},
    snapshot::{self, ImplSnapshot, ShapeSnapshot},
    source_index::SourceIndex,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit,
    Span, TargetProfile,
};
use zil_ct::{CtExecutionBudget, CtReadView};
use zil_ir::{CoreImport, ct_artifact::*};
use zil_syntax::ParseResult;

fn error(message: &str, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-SOURCE",
        Some(span),
        message,
    )]
}

pub(super) struct Environment<'a> {
    sources: &'a [SourceUnit],
    parsed: &'a [Arc<ParseResult>],
    pub index: SourceIndex,
    pub shape: Option<ShapeSnapshot>,
    pub impl_index: Option<ImplSnapshot>,
    shape_failure: Option<Vec<Diagnostic>>,
    snapshot: CtSnapshotRequest,
    limits: Limits,
    pub budget: CtExecutionBudget,
    pub ct_only: Vec<CtSymbolId>,
    source_fingerprint: Fingerprint,
    target: &'a TargetProfile,
}

impl<'a> Environment<'a> {
    // The immutable project graph and request identity enter together; callers
    // already own these borrowed inputs, so a wrapper would only hide them.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sources: &'a [SourceUnit],
        parsed: &'a [Arc<ParseResult>],
        paths: &BTreeMap<Vec<String>, ModuleId>,
        imports: &[Vec<CoreImport>],
        root: ModuleId,
        fingerprint: Fingerprint,
        target: &'a TargetProfile,
        limits: &Limits,
    ) -> CompileResult<Self> {
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let index = SourceIndex::freeze(sources, &syntax, paths)?;
        // This batch contains only prior, independent ordinary declarations.
        // Failure makes the view unavailable; a query reports the exact phase
        // boundary, while final ordinary checking still diagnoses the source.
        let (shape, shape_failure) = match snapshot::freeze_shapes(
            sources,
            &syntax,
            imports,
            root,
            fingerprint,
            target,
            limits,
        ) {
            Ok(shape) => (Some(shape), None),
            Err(errors)
                if errors.iter().all(|error| {
                    matches!(
                        error.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )
                }) =>
            {
                (None, Some(errors))
            }
            Err(errors) => return Err(errors),
        };
        let impl_index = if let Some(shape) = &shape {
            match snapshot::freeze_impls(
                sources,
                &syntax,
                imports,
                root,
                fingerprint,
                target,
                limits,
                shape,
            ) {
                Ok(index) => Some(index),
                Err(errors)
                    if errors.iter().all(|error| {
                        matches!(
                            error.category,
                            ResultCategory::SafeReject | ResultCategory::Unsupported
                        )
                    }) =>
                {
                    None
                }
                Err(errors) => return Err(errors),
            }
        } else {
            None
        };
        let ready = if shape.is_some() {
            CtReadyStage::Layout
        } else {
            CtReadyStage::Config
        };
        let mut identity = b"zil/ct-project-snapshot/v1\0".to_vec();
        identity.extend_from_slice(&fingerprint.0);
        identity.extend_from_slice(&target.fingerprint().0);
        identity.push(ready as u8);
        identity.push(u8::from(impl_index.is_some()));
        let snapshot = CtSnapshotRequest {
            fingerprint: Fingerprint::of(&identity),
            ready,
            target_fingerprint: Some(target.fingerprint()),
            requested_queries: if shape.is_some() {
                let mut queries = vec![CtQueryKind::Shape, CtQueryKind::Field, CtQueryKind::Layout];
                if impl_index.is_some() {
                    queries.push(CtQueryKind::ImplIndex);
                }
                queries
            } else {
                Vec::new()
            },
        };
        let declared = CtDeclaredBudget {
            max_steps: limits.max_ct_steps,
            max_call_depth: u32::try_from(limits.max_call_depth).unwrap_or(u32::MAX),
            max_live_cells: limits.max_values as u64,
            max_temporary_bytes: limits.max_source_bytes as u64,
            max_generated_nodes: limits.max_nodes as u64,
            max_instances: limits.max_functions as u64,
            max_analysis_work: limits.max_analysis_steps as u64,
        };
        Ok(Self {
            sources,
            parsed,
            index,
            shape,
            impl_index,
            shape_failure,
            snapshot,
            limits: limits.clone(),
            budget: CtExecutionBudget::new(declared),
            ct_only: Vec::new(),
            source_fingerprint: fingerprint,
            target,
        })
    }

    fn remaining_limits(&self) -> Limits {
        let remaining = self.budget.remaining();
        let mut limits = self.limits.clone();
        limits.max_ct_steps = remaining.max_steps;
        limits.max_call_depth = limits.max_call_depth.min(remaining.max_call_depth as usize);
        limits.max_nodes = limits.max_nodes.min(remaining.max_generated_nodes as usize);
        limits.max_functions = limits.max_functions.min(remaining.max_instances as usize);
        limits.max_analysis_steps = limits
            .max_analysis_steps
            .min(remaining.max_analysis_work as usize);
        limits.max_source_bytes = limits
            .max_source_bytes
            .min(remaining.max_temporary_bytes as usize);
        limits.max_values = limits.max_values.min(remaining.max_live_cells as usize);
        limits
    }

    pub(super) fn remaining_limits_for_input(&self) -> Limits {
        self.remaining_limits()
    }

    /// Classify every actual CT-only declaration from the same source artifact
    /// used by execution. Verify its entire body without calling the VM.
    pub(super) fn classify_definitions(&mut self) -> CompileResult<()> {
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        let mut limits = self.remaining_limits();
        let before = self.budget.remaining();
        let result = compiler::definitions_entry(&modules, &mut limits);
        self.budget.reserve_analysis_work(
            before
                .max_analysis_work
                .saturating_sub(limits.max_analysis_steps as u64),
        )?;
        self.budget.reserve_generated_nodes(
            before
                .max_generated_nodes
                .saturating_sub(limits.max_nodes as u64),
        )?;
        let temporary_bytes = before
            .max_temporary_bytes
            .saturating_sub(limits.max_source_bytes as u64);
        self.budget.reserve_temporary_bytes(temporary_bytes)?;
        self.budget.release_temporary_bytes(temporary_bytes);
        for _ in 0..before
            .max_instances
            .saturating_sub(limits.max_functions as u64)
        {
            self.budget.reserve_instance()?;
        }
        let (entry, generic_definitions) = result?;
        self.ct_only.extend(generic_definitions);
        let Some(entry) = entry else {
            return Ok(());
        };
        let span = self.index.span(&entry).ok_or_else(|| {
            error(
                "CT-only definition is absent from the frozen source index",
                Span::default(),
            )
        })?;
        let input_tokens = self.empty_input(span)?;
        let selected_limits = self.remaining_limits();
        let mut expected = compiler::compile(CompileRequest { target_pointer_width: self.target.pointer_width, modules: &modules,
            source_fingerprint: self.source_fingerprint, entry: Entry::Function(entry),
            input_tokens: &input_tokens, snapshot: &self.snapshot, limits: &selected_limits }).map_err(|mut errors| {
                for error in &mut errors {
                    if error.code == "E-CT-SOURCE" && error.message == "foreign functions cannot execute in CT" {
                        error.code = "E-CT-RUNTIME".into();
                        error.message = "compile-time Type or Ident cannot cross a foreign runtime function boundary".into();
                    }
                }
                errors
            })?;
        let classified = compiler::ct_only_definitions(&mut expected)?;
        let declared = expected.budget;
        let before = self.budget.remaining();
        self.budget.reserve_analysis_work(
            before
                .max_analysis_work
                .saturating_sub(declared.max_analysis_work),
        )?;
        self.budget.reserve_generated_nodes(
            before
                .max_generated_nodes
                .saturating_sub(declared.max_generated_nodes),
        )?;
        for _ in 0..before.max_instances.saturating_sub(declared.max_instances) {
            self.budget.reserve_instance()?;
        }
        let authority = FrozenAuthority {
            source_fingerprint: self.source_fingerprint,
            snapshot: &self.snapshot,
            input: &input_tokens,
            index: &self.index,
            expected_functions: &expected.functions,
            expected_artifact: &expected,
        };
        let clone_charge = value_context::program_copy_cost(&expected, &mut self.budget)?;
        self.budget.reserve_temporary_bytes(clone_charge)?;
        let mut meter =
            zil_verify::CtVerificationMeter::new(self.budget.remaining().max_analysis_work);
        let result = zil_verify::verify_ct_artifact_metered(
            expected.clone(),
            &selected_limits,
            &authority,
            &mut meter,
        );
        self.budget.reserve_analysis_work(meter.spent())?;
        self.budget.release_temporary_bytes(clone_charge);
        result?;
        self.ct_only.extend(classified);
        Ok(())
    }

    pub(super) fn evaluate(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        input: Option<&CtTokenTree>,
        arguments: &[CtValue],
        span: Span,
    ) -> CompileResult<CtValue> {
        self.evaluate_data_impl(entry, input_tokens, input, arguments, span, false)
            .map(|output| output.value)
    }
    pub(super) fn evaluate_data(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        input: Option<&CtTokenTree>,
        arguments: &[CtValue],
        span: Span,
    ) -> CompileResult<EvaluatedData> {
        self.evaluate_data_impl(entry, input_tokens, input, arguments, span, true)
    }
    fn evaluate_data_impl(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        input: Option<&CtTokenTree>,
        arguments: &[CtValue],
        span: Span,
        retain_context: bool,
    ) -> CompileResult<EvaluatedData> {
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        let selected_limits = self.remaining_limits();
        let expected = compiler::compile(CompileRequest {
            target_pointer_width: self.target.pointer_width,
            modules: &modules,
            source_fingerprint: self.source_fingerprint,
            entry,
            input_tokens,
            snapshot: &self.snapshot,
            limits: &selected_limits,
        })?;
        let declared = expected.budget;
        let before = self.budget.remaining();
        self.budget.reserve_analysis_work(
            before
                .max_analysis_work
                .saturating_sub(declared.max_analysis_work),
        )?;
        self.budget.reserve_generated_nodes(
            before
                .max_generated_nodes
                .saturating_sub(declared.max_generated_nodes),
        )?;
        for _ in 0..before.max_instances.saturating_sub(declared.max_instances) {
            self.budget.reserve_instance()?;
        }
        let copied_bytes = value_context::program_copy_cost(&expected, &mut self.budget)?;
        self.budget.reserve_temporary_bytes(copied_bytes)?;
        let raw = expected.clone();
        let authority = FrozenAuthority {
            source_fingerprint: self.source_fingerprint,
            snapshot: &self.snapshot,
            input: input_tokens,
            index: &self.index,
            expected_functions: &expected.functions,
            expected_artifact: &expected,
        };
        let mut meter =
            zil_verify::CtVerificationMeter::new(self.budget.remaining().max_analysis_work);
        let result =
            zil_verify::verify_ct_artifact_metered(raw, &selected_limits, &authority, &mut meter);
        self.budget.reserve_analysis_work(meter.spent())?;
        let verified = result?;
        let reader = FrozenReadView {
            index: &self.index,
            sources: self.sources,
            syntax: &syntax,
            shape: self.shape.as_ref(),
            impl_index: self.impl_index.as_ref(),
            shape_failure: self.shape_failure.as_deref(),
            input,
            limits: &selected_limits,
            target: self.target,
            snapshot: &self.snapshot,
        };
        let (value, _usage) =
            zil_ct::evaluate_ct_artifact(&verified, arguments, &reader, &mut self.budget).map_err(
                |mut errors| {
                    for diagnostic in &mut errors {
                        if diagnostic.span.is_none() {
                            diagnostic.span = Some(span);
                        }
                        if diagnostic.code == "E-CT-ARTIFACT"
                            && diagnostic
                                .message
                                .contains("name_expr needs a value name, not export intent")
                        {
                            diagnostic.code = "E-CT-EXPORT-CAPTURE".into();
                        }
                    }
                    errors
                },
            )?;
        self.budget.release_temporary_bytes(copied_bytes);
        let result_ty =
            &verified.program().functions[verified.program().entry.0 as usize].result_type;
        let context = if retain_context
            && matches!(
                result_ty,
                CtTy::Record(_)
                    | CtTy::Enum(_)
                    | CtTy::AnonymousSum(_)
                    | CtTy::Tuple(_)
                    | CtTy::Array { .. }
                    | CtTy::List(_)
            ) {
            Some(value_context::ValueContext::capture(
                verified.program(),
                &mut self.budget,
            )?)
        } else {
            None
        };
        Ok(EvaluatedData { value, context })
    }

    pub(super) fn empty_input(&self, span: Span) -> CompileResult<CtInputTable> {
        let source = self
            .sources
            .iter()
            .find(|source| source.file_id == span.file)
            .ok_or_else(|| error("CT expression is not in project source", span))?;
        let mut bytes = b"zil/ct-expression-invocation/v1\0".to_vec();
        bytes.extend_from_slice(&source.fingerprint().0);
        bytes.extend_from_slice(&span.start.to_le_bytes());
        bytes.extend_from_slice(&span.end.to_le_bytes());
        Ok(CtInputTable {
            invocation_fingerprint: Fingerprint::of(&bytes),
            tokens: Vec::new(),
        })
    }

    pub(super) fn parse_tree(
        &mut self,
        tree: &CtSyntaxTree,
        span: Span,
        extra_aliases: &[(ModuleId, String)],
    ) -> CompileResult<ParsedSyntax> {
        let module = self
            .sources
            .iter()
            .find(|source| source.file_id == span.file)
            .map_or(ModuleId(0), |source| source.module_id);
        let mut aliases = self.index.aliases(module);
        for (target, alias) in extra_aliases {
            aliases.insert(*target, alias.clone());
        }
        render::parse(
            tree,
            span.file,
            module,
            &aliases,
            &self.limits,
            &mut self.budget,
        )
    }

    pub(super) fn type_syntax(&mut self, ty: &CtTypeRef) -> CompileResult<CtSyntaxTree> {
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &self.index,
            sources: self.sources,
            syntax: &syntax,
            shape: self.shape.as_ref(),
            impl_index: self.impl_index.as_ref(),
            shape_failure: self.shape_failure.as_deref(),
            input: None,
            limits: &self.limits,
            target: self.target,
            snapshot: &self.snapshot,
        };
        let clone_charge = (ty.nodes.len() as u64).saturating_mul(96);
        self.budget.reserve_temporary_bytes(clone_charge)?;
        let result = reader.build_syntax(
            CtSyntaxBuilder::TypeRef,
            &[CtValue::Type(ty.clone())],
            &mut self.budget,
        );
        self.budget.release_temporary_bytes(clone_charge);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::FileId;

    #[test]
    fn source_function_passes_frozen_authority_independent_verify_and_vm() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "ct.zil",
            "fn identity(t:Type)->Type{return t}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let mut environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let item = &parsed[0].module.items[0];
        let symbol = environment.index.symbol_at(ModuleId(0), item.span).unwrap();
        let input = environment.empty_input(item.span).unwrap();
        let argument = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let result = environment
            .evaluate(
                Entry::Function(symbol),
                &input,
                None,
                std::slice::from_ref(&argument),
                item.span,
            )
            .unwrap();
        assert_eq!(result, argument);
    }

    #[test]
    fn impl_index_exposes_only_checked_pregeneration_impls() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "traits.zil",
            "trait Marker{fn mark(self:&Self)->i32;}struct Point{x:i32}impl Marker for Point{fn mark()->i32=>.x}fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        assert!(
            environment.impl_index.is_some(),
            "ordinary implementation was not ready"
        );
        let items = &parsed[0].module.items;
        let subject = environment
            .index
            .symbol_at(ModuleId(0), items[1].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), items[3].span)
            .unwrap();
        let value = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Nominal {
                symbol: subject,
                arguments: Vec::new(),
            }],
            root: CtTypeNodeId(0),
        });
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let mut reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let (result, receipt) = reader
            .query(
                &requester,
                CtQueryKind::ImplIndex,
                std::slice::from_ref(&value),
                &mut budget,
            )
            .unwrap();
        assert!(
            matches!(&result, CtValue::List(values) if values.len() == 1 && matches!(values[0], CtValue::ImplInfo(_))),
            "{result:?}"
        );
        assert_eq!(receipt.kind, CtQueryKind::ImplIndex);
        let mut missing_grant = environment.snapshot.clone();
        missing_grant
            .requested_queries
            .retain(|kind| *kind != CtQueryKind::ImplIndex);
        let mut wrong_stage = environment.snapshot.clone();
        wrong_stage.ready = CtReadyStage::Shape;
        reader.snapshot = &missing_grant;
        assert!(
            reader
                .query(
                    &requester,
                    CtQueryKind::ImplIndex,
                    std::slice::from_ref(&value),
                    &mut budget
                )
                .is_err()
        );
        reader.snapshot = &wrong_stage;
        assert!(
            reader
                .query(&requester, CtQueryKind::ImplIndex, &[value], &mut budget)
                .is_err()
        );
    }

    #[test]
    fn impl_index_ignores_unaccepted_generic_impl_and_deduplicates_subject() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "traits.zil",
            "trait Marker{fn mark(self:&Self)->i32;}struct Point{x:i32}impl Marker for Point{fn mark()->i32=>.x}struct Box<T>{value:T}impl<T> Marker for Box<T>{fn mark()->i32=>0}fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        assert!(
            environment.impl_index.is_some(),
            "unrelated unsupported impl blocked the static index"
        );
        let items = &parsed[0].module.items;
        let subject = environment
            .index
            .symbol_at(ModuleId(0), items[1].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), items[5].span)
            .unwrap();
        let value = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Nominal {
                symbol: subject,
                arguments: Vec::new(),
            }],
            root: CtTypeNodeId(0),
        });
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let (result, _) = reader
            .query(
                &requester,
                CtQueryKind::ImplIndex,
                &[value.clone(), value],
                &mut budget,
            )
            .unwrap();
        assert!(matches!(result, CtValue::List(values) if values.len() == 1));
    }

    #[test]
    fn generic_type_application_reflects_instantiated_field_shape() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "generic.zil",
            "struct Cell<T>{value:T}fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let items = &parsed[0].module.items;
        let nominal = environment
            .index
            .symbol_at(ModuleId(0), items[0].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), items[1].span)
            .unwrap();
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let arg = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let applied = reader
            .type_apply(&requester, &nominal, &[arg], &mut budget)
            .unwrap();
        assert!(
            environment.shape.as_ref().unwrap().core.modules[0]
                .core
                .aggregates
                .is_empty(),
            "derived generic instances must not enter the active frozen shape index"
        );
        let (shape, dependency) = reader
            .query(
                &requester,
                CtQueryKind::Shape,
                &[CtValue::Type(applied.clone())],
                &mut budget,
            )
            .unwrap();
        let (expected_result, _) = zil_ir::ct_wire::value_fingerprint_with_usage(
            &shape,
            &limits,
            &mut zil_ir::ct_wire::WireBudget::from_limits(&limits),
        )
        .unwrap();
        assert_eq!(dependency.result_fingerprint, expected_result);
        assert!(
            matches!(shape, CtValue::ShapeInfo(info) if info.visible_fields.len() == 1
            && info.visible_fields[0].ty.nodes == vec![CtTypeNode::Primitive(CtScalarTy::I32)])
        );
        let (layout, _) = reader
            .query(
                &requester,
                CtQueryKind::Layout,
                &[CtValue::Type(applied)],
                &mut budget,
            )
            .unwrap();
        assert!(
            matches!(layout, CtValue::LayoutInfo(info) if info.size == 4 && info.alignment == 4 && info.field_offsets == vec![0])
        );
        let resource = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Builtin(CtBuiltinType::ByteBuffer)],
            root: CtTypeNodeId(0),
        });
        assert!(
            reader
                .type_apply(&requester, &nominal, &[resource], &mut budget)
                .is_err(),
            "ordinary generic qualification must reject an illegal type argument"
        );
    }

    #[test]
    fn nested_generic_field_keeps_its_full_nominal_argument() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "nested.zil",
            "struct Cell<T>{value:T}struct Outer<T>{cell:Cell<T>}fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let items = &parsed[0].module.items;
        let cell = environment
            .index
            .symbol_at(ModuleId(0), items[0].span)
            .unwrap();
        let outer = environment
            .index
            .symbol_at(ModuleId(0), items[1].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), items[2].span)
            .unwrap();
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let i32_type = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let applied = reader
            .type_apply(&requester, &outer, &[i32_type], &mut budget)
            .unwrap();
        let (field, _) = reader
            .query(
                &requester,
                CtQueryKind::Field,
                &[CtValue::Type(applied), CtValue::Text("cell".into())],
                &mut budget,
            )
            .unwrap();
        let CtValue::FieldInfo(info) = field else {
            panic!("expected field info")
        };
        assert!(
            matches!(info.ty.nodes.get(info.ty.root.0 as usize), Some(CtTypeNode::Nominal { symbol, arguments })
            if symbol == &cell && matches!(arguments.as_slice(), [CtTypeArgument::Type(id)]
                if matches!(info.ty.nodes.get(id.0 as usize), Some(CtTypeNode::Primitive(CtScalarTy::I32)))))
        );
        let direct = reader
            .type_apply(
                &requester,
                &cell,
                &[CtValue::Type(CtTypeRef {
                    nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                    root: CtTypeNodeId(0),
                })],
                &mut budget,
            )
            .unwrap();
        assert_eq!(
            info.ty, direct,
            "reflected and directly applied Type must have one canonical DAG"
        );
        let bool_type = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::Bool)],
            root: CtTypeNodeId(0),
        });
        let other = reader
            .type_apply(&requester, &outer, &[bool_type], &mut budget)
            .unwrap();
        let (other_field, _) = reader
            .query(
                &requester,
                CtQueryKind::Field,
                &[CtValue::Type(other), CtValue::Text("cell".into())],
                &mut budget,
            )
            .unwrap();
        let CtValue::FieldInfo(other_info) = other_field else {
            panic!("expected field info")
        };
        assert_ne!(info.ty, other_info.ty);
        assert!(
            matches!(other_info.ty.nodes.get(other_info.ty.root.0 as usize), Some(CtTypeNode::Nominal { symbol, arguments })
            if symbol == &cell && matches!(arguments.as_slice(), [CtTypeArgument::Type(id)]
                if matches!(other_info.ty.nodes.get(id.0 as usize), Some(CtTypeNode::Primitive(CtScalarTy::Bool)))))
        );
    }

    #[test]
    fn nested_multi_argument_field_reorders_checked_type_arguments() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "nested_multi.zil",
            "struct Cell<T,U>{left:T,right:U}struct Outer<A,B>{inner:Cell<B,A>}fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let items = &parsed[0].module.items;
        let cell = environment
            .index
            .symbol_at(ModuleId(0), items[0].span)
            .unwrap();
        let outer = environment
            .index
            .symbol_at(ModuleId(0), items[1].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), items[2].span)
            .unwrap();
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let i32_value = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let bool_value = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::Bool)],
            root: CtTypeNodeId(0),
        });
        let owner = reader
            .type_apply(
                &requester,
                &outer,
                &[i32_value.clone(), bool_value.clone()],
                &mut budget,
            )
            .unwrap();
        let (field, _) = reader
            .query(
                &requester,
                CtQueryKind::Field,
                &[CtValue::Type(owner), CtValue::Text("inner".into())],
                &mut budget,
            )
            .unwrap();
        let CtValue::FieldInfo(field) = field else {
            panic!("expected field info")
        };
        let direct = reader
            .type_apply(&requester, &cell, &[bool_value, i32_value], &mut budget)
            .unwrap();
        assert_eq!(field.ty, direct);
    }

    #[test]
    fn token_cursor_uses_canonical_borrowed_wire_identity() {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "token.zil", "fn main(){}");
        let parsed = Arc::new(zil_syntax::parse(&source));
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let span = parsed[0].module.items[0].span;
        let tree = CtTokenTree {
            nodes: vec![CtTokenNode::Token(CtToken {
                class: CtTokenClass::Identifier,
                text: "x".into(),
                origin: CtOriginId(0),
                name: None,
            })],
            roots: vec![CtTokenNodeId(0)],
            origins: vec![CtOrigin {
                span,
                parent: None,
                role: CtOriginRole::Input,
            }],
        };
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: Some(&tree),
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let cursor = reader
            .token_inspect(
                &CtValue::TokenTree(tree.clone()),
                CtTokenProjection::RootAt,
                Some(0),
                &mut budget,
            )
            .unwrap();
        let CtValue::TokenCursor(cursor_value) = &cursor else {
            panic!("expected token cursor")
        };
        let (expected, _) = zil_ir::ct_wire::token_tree_fingerprint_with_usage(
            &tree,
            &limits,
            &mut zil_ir::ct_wire::WireBudget::from_limits(&limits),
        )
        .unwrap();
        assert_eq!(cursor_value.tree_fingerprint, expected);
        assert!(budget.usage().analysis_work > 0);
        let mut changed = tree.clone();
        let CtTokenNode::Token(token) = &mut changed.nodes[0] else {
            panic!("expected token")
        };
        token.text = "y".into();
        let changed_reader = FrozenReadView {
            input: Some(&changed),
            ..reader
        };
        assert!(
            changed_reader
                .token_inspect(&cursor, CtTokenProjection::Class, None, &mut budget)
                .is_err()
        );
        let mut declared = environment.budget.remaining();
        declared.max_analysis_work = 2;
        let mut tiny = CtExecutionBudget::new(declared);
        assert!(
            changed_reader
                .token_inspect(
                    &CtValue::TokenTree(changed.clone()),
                    CtTokenProjection::RootAt,
                    Some(0),
                    &mut tiny
                )
                .is_err()
        );
        assert_eq!(
            tiny.usage().analysis_work,
            2,
            "failed wire hashing must debit completed work"
        );
    }

    #[test]
    fn generic_enum_type_and_layout_use_checked_concrete_instance() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "enum.zil",
            "enum LegacyOption<T>{None,Some(T)}fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let items = &parsed[0].module.items;
        let nominal = environment
            .index
            .symbol_at(ModuleId(0), items[0].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), items[1].span)
            .unwrap();
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let arg = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let ty = reader
            .type_apply(&requester, &nominal, &[arg], &mut budget)
            .unwrap();
        assert!(
            matches!(ty.nodes.get(ty.root.0 as usize), Some(CtTypeNode::Nominal { symbol, arguments })
            if symbol == &nominal && arguments.len() == 1)
        );
        let (layout, _) = reader
            .query(
                &requester,
                CtQueryKind::Layout,
                &[CtValue::Type(ty)],
                &mut budget,
            )
            .unwrap();
        assert!(
            matches!(layout, CtValue::LayoutInfo(info) if info.size >= 4 && info.alignment >= 4
            && info.target_fingerprint == target.fingerprint())
        );
    }

    #[test]
    fn type_alias_values_normalize_through_ordinary_checked_target() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "alias.zil",
            "struct Cell<T>{value:T}type Entry<T>=Cell<T>;type Number=i32;struct Pair<T,U>{left:T,right:U}type Swapped<T,U>=Pair<U,T>;type Middle=Number;type Top=Middle;type Wrapper<T>=Entry<T>;type Vector=[i32;2];type Callback=fn(i32)->i32;type View=&i32;type PairAlias=(i32,i64);type PairWrap<T>=(T,[T;2]);fn main(){}type Bad=Missing;",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let items = &parsed[0].module.items;
        let cell = environment
            .index
            .symbol_at(ModuleId(0), items[0].span)
            .unwrap();
        let entry = environment
            .index
            .symbol_at(ModuleId(0), items[1].span)
            .unwrap();
        let number = environment
            .index
            .symbol_at(ModuleId(0), items[2].span)
            .unwrap();
        let pair = environment
            .index
            .symbol_at(ModuleId(0), items[3].span)
            .unwrap();
        let swapped = environment
            .index
            .symbol_at(ModuleId(0), items[4].span)
            .unwrap();
        let top = environment
            .index
            .symbol_at(ModuleId(0), items[6].span)
            .unwrap();
        let wrapper = environment
            .index
            .symbol_at(ModuleId(0), items[7].span)
            .unwrap();
        let vector = environment
            .index
            .symbol_at(ModuleId(0), items[8].span)
            .unwrap();
        let callback = environment
            .index
            .symbol_at(ModuleId(0), items[9].span)
            .unwrap();
        let view = environment
            .index
            .symbol_at(ModuleId(0), items[10].span)
            .unwrap();
        let pair_alias = environment
            .index
            .symbol_at(ModuleId(0), items[11].span)
            .unwrap();
        let pair_wrap = environment
            .index
            .symbol_at(ModuleId(0), items[12].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), items[13].span)
            .unwrap();
        let bad = environment
            .index
            .symbol_at(ModuleId(0), items[14].span)
            .unwrap();
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        assert_eq!(
            reader.type_name(&requester, &number, &mut budget).unwrap(),
            CtTypeRef {
                nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                root: CtTypeNodeId(0),
            }
        );
        assert_eq!(
            reader.type_name(&requester, &top, &mut budget).unwrap(),
            CtTypeRef {
                nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                root: CtTypeNodeId(0),
            }
        );
        for (alias, expected) in [
            (vector, "array"),
            (callback, "callback"),
            (view, "reference"),
            (pair_alias, "tuple"),
        ] {
            let value = reader.type_name(&requester, &alias, &mut budget).unwrap();
            let actual = match value.nodes.get(value.root.0 as usize) {
                Some(CtTypeNode::Array { length: 2, .. }) => "array",
                Some(CtTypeNode::FunctionPointer { parameters, .. }) if parameters.len() == 1 => {
                    "callback"
                }
                Some(CtTypeNode::Address { mutable: false, .. }) => "reference",
                Some(CtTypeNode::Tuple(parts)) if parts.len() == 2 => "tuple",
                other => panic!("unexpected reflected alias type {other:?}"),
            };
            assert_eq!(actual, expected);
        }
        let wrapped = reader
            .type_apply(
                &requester,
                &pair_wrap,
                &[CtValue::Type(CtTypeRef {
                    nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                    root: CtTypeNodeId(0),
                })],
                &mut budget,
            )
            .unwrap();
        let Some(CtTypeNode::Tuple(parts)) = wrapped.nodes.get(wrapped.root.0 as usize) else {
            panic!("expected generic tuple alias")
        };
        assert!(matches!(
            wrapped.nodes.get(parts[0].0 as usize),
            Some(CtTypeNode::Primitive(CtScalarTy::I32))
        ));
        assert!(matches!(
            wrapped.nodes.get(parts[1].0 as usize),
            Some(CtTypeNode::Array { length: 2, .. })
        ));
        assert!(
            reader.type_name(&requester, &bad, &mut budget).is_err(),
            "unfinished alias must not become a CT Type"
        );
        let arg = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let aliased = reader
            .type_apply(&requester, &entry, std::slice::from_ref(&arg), &mut budget)
            .unwrap();
        let direct = reader
            .type_apply(&requester, &cell, &[arg], &mut budget)
            .unwrap();
        assert_eq!(
            aliased, direct,
            "alias and target Type values must share canonical identity"
        );
        let chained = reader
            .type_apply(
                &requester,
                &wrapper,
                &[CtValue::Type(CtTypeRef {
                    nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                    root: CtTypeNodeId(0),
                })],
                &mut budget,
            )
            .unwrap();
        assert_eq!(
            chained, direct,
            "generic aliases must normalize through checked alias chains"
        );
        let i32_value = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let bool_value = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::Bool)],
            root: CtTypeNodeId(0),
        });
        let aliased = reader
            .type_apply(
                &requester,
                &swapped,
                &[i32_value.clone(), bool_value.clone()],
                &mut budget,
            )
            .unwrap();
        let direct = reader
            .type_apply(&requester, &pair, &[bool_value, i32_value], &mut budget)
            .unwrap();
        assert_eq!(
            aliased, direct,
            "multi-argument alias must substitute actual Types in declaration order"
        );
    }

    #[test]
    fn checked_tuple_alias_matches_plain_ordinary_type() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "tuple_alias.zil",
            "type Top=(i32,i64);fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let alias = environment
            .index
            .symbol_at(ModuleId(0), parsed[0].module.items[0].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), parsed[0].module.items[1].span)
            .unwrap();
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        assert_eq!(
            reader.type_name(&requester, &alias, &mut budget).unwrap(),
            CtTypeRef {
                nodes: vec![
                    CtTypeNode::Primitive(CtScalarTy::I32),
                    CtTypeNode::Primitive(CtScalarTy::I64),
                    CtTypeNode::Tuple(vec![CtTypeNodeId(0), CtTypeNodeId(1)])
                ],
                root: CtTypeNodeId(2),
            }
        );
    }

    #[test]
    fn repeated_generic_arguments_have_one_canonical_type_value() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "duplicate_type.zil",
            "struct Pair<T,U>{left:T,right:U}type Duplicate<T>=Pair<T,T>;fn main(){}",
        );
        let parsed = Arc::new(zil_syntax::parse(&source));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let sources = vec![source];
        let parsed = vec![parsed];
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let environment = Environment::new(
            &sources,
            &parsed,
            &BTreeMap::new(),
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let pair = environment
            .index
            .symbol_at(ModuleId(0), parsed[0].module.items[0].span)
            .unwrap();
        let duplicate = environment
            .index
            .symbol_at(ModuleId(0), parsed[0].module.items[1].span)
            .unwrap();
        let requester = environment
            .index
            .symbol_at(ModuleId(0), parsed[0].module.items[2].span)
            .unwrap();
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: None,
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let i32_type = CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
        });
        let aliased = reader
            .type_apply(
                &requester,
                &duplicate,
                std::slice::from_ref(&i32_type),
                &mut budget,
            )
            .unwrap();
        let direct = reader
            .type_apply(
                &requester,
                &pair,
                &[i32_type.clone(), i32_type],
                &mut budget,
            )
            .unwrap();
        assert_eq!(
            aliased, direct,
            "sharing and copying the same Type argument must agree"
        );
        assert_eq!(
            direct.nodes.len(),
            2,
            "equal child nodes must share one canonical DAG identity"
        );
    }
}

#[path = "value_context.rs"]
pub(super) mod value_context;
pub(super) struct EvaluatedData {
    pub value: CtValue,
    pub context: Option<value_context::ValueContext>,
}
