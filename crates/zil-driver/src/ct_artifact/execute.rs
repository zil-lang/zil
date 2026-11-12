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
#[path = "execute_static_host.rs"]
mod static_host;

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
    paths: &'a BTreeMap<Vec<String>, ModuleId>,
    pub(super) scoped: Option<&'a crate::ScopedImports>,
    root: ModuleId,
    parsed: &'a [Arc<ParseResult>],
    pub index: SourceIndex,
    pub shape: Option<ShapeSnapshot>,
    dynamic_catalogue: Option<CtDynamicCatalogue>,
    stage_inventory: Option<Arc<super::stage_inventory::FrozenCtStageInventory>>,
    generation_epoch: u64,
    snapshot_base_fingerprint: Fingerprint,
    pub impl_index: Option<ImplSnapshot>,
    shape_failure: Option<Vec<Diagnostic>>,
    snapshot: CtSnapshotRequest,
    limits: Limits,
    pub budget: CtExecutionBudget,
    pub ct_only: Vec<CtSymbolId>,
    source_fingerprint: Fingerprint,
    target: &'a TargetProfile,
    source_origins: Option<Arc<Vec<zil_expand::source_context::ModuleDeclarationOrigins>>>,
    pub(super) generated_catalog: zil_expand::source_context::GeneratedSourceCatalog,
    static_host_invocations: Vec<static_host::Invocation>,
}

impl<'a> Environment<'a> {
    fn freeze_stage_before_execution(&mut self) -> CompileResult<()> {
        if self.stage_inventory.is_some() {
            return Ok(());
        }
        self.require_source_origins()?;
        let origins = self.source_origins.as_ref().ok_or_else(|| {
            error(
                "CT execution requires complete original source origins",
                Span::default(),
            )
        })?;
        self.budget.reserve_temporary_bytes(
            std::mem::size_of::<super::stage_inventory::FrozenCtStageInventory>() as u64 + 32,
        )?;
        let inventory = super::stage_inventory::FrozenCtStageInventory::freeze(
            self.source_fingerprint,
            self.generation_epoch,
            self.parsed,
            Arc::clone(origins),
            &self.generated_catalog,
            &mut self.budget,
            &self.limits,
        )?;
        let stage = inventory.identity();
        self.budget.reserve_analysis_work(177)?;
        let mut fingerprint = zil_base::FingerprintBuilder::new();
        fingerprint.update(b"zil/ct-source-epoch-snapshot/v1\0");
        fingerprint.update(&self.snapshot_base_fingerprint.0);
        fingerprint.update(&stage.source_fingerprint.0);
        fingerprint.update(&stage.epoch.to_le_bytes());
        fingerprint.update(&stage.committed_generation_fingerprint.0);
        fingerprint.update(&stage.declaration_inventory_fingerprint.0);
        self.snapshot.fingerprint = fingerprint.digest();
        self.snapshot.stage = Some(stage);
        self.stage_inventory = Some(Arc::new(inventory));
        Ok(())
    }
    /// Bind only an actual complete immutable Core snapshot. The pure shape
    /// snapshot is intentionally insufficient: it omits traits and providers.
    #[cfg(test)]
    pub(super) fn bind_frozen_core_inventory(
        &mut self,
        core: &zil_ir::CoreProgram,
    ) -> CompileResult<()> {
        if self.dynamic_catalogue.is_some() || self.budget.usage().steps != 0 {
            return Err(error(
                "CT full Core inventory must be frozen before execution",
                Span::default(),
            ));
        }
        if core.source_fingerprint != self.source_fingerprint {
            return Err(error(
                "CT Core inventory belongs to another source snapshot",
                Span::default(),
            ));
        }
        self.dynamic_catalogue = Some(super::dynamic_catalogue::from_core(
            core,
            &mut self.budget,
            &self.limits,
        )?);
        Ok(())
    }
    fn attach_dynamic_catalogue(&mut self, expected: &mut CtArtifactProgram) -> CompileResult<()> {
        expected.dynamic_catalogue = if let Some(catalogue) = &self.dynamic_catalogue {
            let cost = super::dynamic_catalogue::copy_cost(catalogue, &mut self.budget)?;
            self.budget.reserve_temporary_bytes(cost)?;
            let mut copied = catalogue.clone();
            self.budget.reserve_analysis_work(129)?;
            copied.stage = self.snapshot.stage.clone();
            Some(copied)
        } else {
            None
        };
        Ok(())
    }
    // The immutable project graph and request identity enter together; callers
    // already own these borrowed inputs, so a wrapper would only hide them.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sources: &'a [SourceUnit],
        parsed: &'a [Arc<ParseResult>],
        paths: &'a BTreeMap<Vec<String>, ModuleId>,
        imports: &[Vec<CoreImport>],
        root: ModuleId,
        fingerprint: Fingerprint,
        target: &'a TargetProfile,
        limits: &Limits,
    ) -> CompileResult<Self> {
        Self::new_scoped(
            sources,
            parsed,
            paths,
            None,
            imports,
            root,
            fingerprint,
            target,
            limits,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_scoped(
        sources: &'a [SourceUnit],
        parsed: &'a [Arc<ParseResult>],
        paths: &'a BTreeMap<Vec<String>, ModuleId>,
        scoped: Option<&'a crate::ScopedImports>,
        imports: &[Vec<CoreImport>],
        root: ModuleId,
        fingerprint: Fingerprint,
        target: &'a TargetProfile,
        limits: &Limits,
    ) -> CompileResult<Self> {
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let index = SourceIndex::freeze_scoped(sources, &syntax, paths, scoped)?;
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
            stage: None,
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
            paths,
            scoped,
            root,
            parsed,
            index,
            shape,
            dynamic_catalogue: None,
            stage_inventory: None,
            generation_epoch: 0,
            snapshot_base_fingerprint: snapshot.fingerprint,
            impl_index,
            shape_failure,
            snapshot,
            limits: limits.clone(),
            budget: CtExecutionBudget::new(declared),
            ct_only: Vec::new(),
            source_fingerprint: fingerprint,
            target,
            source_origins: None,
            generated_catalog: Default::default(),
            static_host_invocations: Vec::new(),
        })
    }
    pub(super) fn resolve_path(&self, importer: ModuleId, path: &[String]) -> Option<ModuleId> {
        self.scoped
            .filter(|_| path.first().is_none_or(|part| part != "std"))
            .and_then(|map| map.get(&(importer, path.to_vec())))
            .or_else(|| self.paths.get(path))
            .copied()
    }

    /// Retain the exact parse+hygiene tree before output remapping. The row is
    /// only an address; frozen source admission independently authenticates it.
    pub(super) fn register_generated_tree(
        &mut self,
        tree: Arc<zil_expand::source_context::GeneratedSourceTree>,
    ) -> CompileResult<u32> {
        use zil_expand::source_context::SourceTreeIdentity;
        let span = Span::default();
        let SourceTreeIdentity::Generated { module, event_path } = &tree.identity else {
            return Err(error(
                "generated catalog requires a generated tree identity",
                span,
            ));
        };
        if event_path.is_empty()
            || *module != tree.emitter.tree.module()
            || !self.parsed.iter().any(|source| source.module.id == *module)
            || tree.declarations.len() > self.limits.max_nodes
            || self.generated_catalog.trees.len() >= self.limits.max_nodes
        {
            return Err(error(
                "generated source inventory exceeds its source domain",
                span,
            ));
        }
        let work = self.generated_catalog.trees.iter().fold(
            tree.declarations.len() as u64 + event_path.len() as u64 + 1,
            |cost, row| {
                cost.saturating_add(
                    (row.declarations.len() as u64 + 1)
                        .saturating_mul(tree.declarations.len() as u64 + 1),
                )
            },
        );
        self.budget.reserve_analysis_work(work)?;
        if self.generated_catalog.find(&tree.identity).is_some() {
            return Err(error("duplicate generated source tree identity", span));
        }
        let seen_bytes = tree.declarations.len() as u64 * 48;
        self.budget.reserve_temporary_bytes(seen_bytes)?;
        let mut seen = std::collections::BTreeSet::new();
        for declaration in &tree.declarations {
            if !seen.insert(*declaration)
                || self.generated_catalog.trees.iter().any(|row| {
                    row.identity.module() == *module && row.declarations.contains(declaration)
                })
            {
                return Err(error(
                    "generated declarations must have distinct output identities",
                    span,
                ));
            }
        }
        let row = u32::try_from(self.generated_catalog.trees.len())
            .map_err(|_| error("generated catalog row exceeds its index domain", span))?;
        self.budget.reserve_temporary_bytes(
            (self.generated_catalog.trees.len() as u64 + 1)
                .saturating_mul(16)
                .saturating_add(tree.declarations.len() as u64 * 48)
                .saturating_add(event_path.len() as u64 * 4)
                .saturating_add(128),
        )?;
        self.index
            .register_generated_tree(&tree, &self.limits, &mut self.budget)?;
        self.budget.release_temporary_bytes(seen_bytes);
        self.generated_catalog.trees.reserve_exact(1);
        self.generated_catalog.trees.push(tree);
        self.classify_definitions()?;
        // The VM query that produced this emission has finished. Publish the
        // next epoch only after parse/hygiene, unique declaration registration
        // and actual generated CT-body validation all succeed. An error never
        // supplies a query view of a partially committed source batch.
        self.generation_epoch = self
            .generation_epoch
            .checked_add(1)
            .ok_or_else(|| error("CT generation epoch overflow", span))?;
        self.stage_inventory = None;
        Ok(row)
    }

    fn require_source_origins(&mut self) -> CompileResult<()> {
        if self.source_origins.is_some() {
            return Ok(());
        }
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let limits = self.remaining_limits();
        let (origins, remaining) = zil_expand::source_context::capture_original_required(
            self.sources,
            &syntax,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: self.root,
                aliases: self.paths,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
            &limits,
        )?;
        self.budget.reserve_analysis_work(
            limits
                .max_analysis_steps
                .saturating_sub(remaining.max_analysis_steps) as u64,
        )?;
        self.budget
            .reserve_generated_nodes(limits.max_nodes.saturating_sub(remaining.max_nodes) as u64)?;
        self.budget.reserve_temporary_bytes(
            limits
                .max_source_bytes
                .saturating_sub(remaining.max_source_bytes) as u64,
        )?;
        self.bind_source_origins(Some(&origins))
    }

    pub(super) fn bind_source_origins(
        &mut self,
        origins: Option<&[zil_expand::source_context::ModuleDeclarationOrigins]>,
    ) -> CompileResult<()> {
        if let Some(origins) = origins {
            let bytes = super::invocation_context::origins_cost(origins, &mut self.budget)?;
            self.budget.reserve_temporary_bytes(bytes)?;
            self.source_origins = Some(Arc::new(origins.to_vec()));
        }
        Ok(())
    }
    pub(super) fn validate_original_host_origin(
        &mut self,
        host: &CtSymbolId,
        row: &zil_expand::source_context::DeclarationOrigin,
    ) -> CompileResult<()> {
        let origins = self
            .source_origins
            .as_ref()
            .ok_or_else(|| error("original member host inventory absent", Span::default()))?;
        self.budget
            .reserve_analysis_work(origins.len() as u64 + 1)?;
        let original = origins
            .iter()
            .find(|module| module.module == host.module)
            .and_then(|module| module.items.get(host.declaration.0 as usize))
            .ok_or_else(|| error("original member host row absent", Span::default()))?;
        super::invocation_context::identity_cost(&original.identity, &mut self.budget)?;
        super::invocation_context::identity_cost(&row.identity, &mut self.budget)?;
        if host.generation_path.is_some()
            || original.declaration != host.declaration
            || row.declaration != original.declaration
            || row.identity != original.identity
            || original.retained_members.is_some()
            || row.retained_members.is_some()
        {
            return Err(error(
                "retained member host differs from the original source inventory",
                Span::default(),
            ));
        }
        Ok(())
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

    pub(super) fn request_fingerprint(&self) -> Fingerprint {
        self.source_fingerprint
    }

    pub(super) fn target_width(&self) -> u16 {
        self.target.pointer_width
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
        let catalog = (!self.generated_catalog.trees.is_empty()).then_some(&self.generated_catalog);
        let result = if catalog.is_some() {
            compiler::definitions_entry_with_catalog(&modules, &mut limits, catalog)
        } else {
            compiler::definitions_entry(&modules, &mut limits)
        };
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
        let request = CompileRequest {
            target_pointer_width: self.target.pointer_width,
            modules: &modules,
            source_fingerprint: self.source_fingerprint,
            entry: Entry::Function(entry),
            input_tokens: &input_tokens,
            snapshot: &self.snapshot,
            limits: &selected_limits,
        };
        let compiled = if catalog.is_some() {
            compiler::compile_scoped_with_catalog(request, catalog).map(|(program, _)| program)
        } else {
            compiler::compile(request)
        };
        let mut expected = compiled.map_err(|mut errors| {
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
        self.attach_dynamic_catalogue(&mut expected)?;
        let authority = FrozenAuthority {
            source_fingerprint: self.source_fingerprint,
            snapshot: &self.snapshot,
            input: &input_tokens,
            index: &self.index,
            expected_functions: &expected.functions,
            expected_artifact: &expected,
            invocation_context: None,
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

    /// Check a symbolic scalar equation's real CT program without inventing
    /// values for its typed dependency parameters or executing the VM.
    pub(super) fn verify_expression(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
    ) -> CompileResult<()> {
        let invocation_context = super::invocation_context::FrozenInvocationContext::freeze(
            &entry,
            self.parsed,
            &self.index,
            self.source_origins.as_ref(),
            &mut self.budget,
        )?;
        self.budget
            .reserve_analysis_work(self.parsed.len() as u64 * 2)?;
        let module_bytes = self.parsed.len() as u64
            * (std::mem::size_of::<&zil_syntax::ast::Module>()
                + std::mem::size_of::<compiler::SourceModule<'_>>()) as u64;
        self.budget.reserve_temporary_bytes(module_bytes)?;
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        let selected_limits = self.remaining_limits();
        let (mut expected, _) = compiler::compile_scoped(CompileRequest {
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
        self.attach_dynamic_catalogue(&mut expected)?;
        let copied_bytes = value_context::program_copy_cost(&expected, &mut self.budget)?;
        self.budget.reserve_temporary_bytes(copied_bytes)?;
        let authority = FrozenAuthority {
            source_fingerprint: self.source_fingerprint,
            snapshot: &self.snapshot,
            input: input_tokens,
            index: &self.index,
            expected_functions: &expected.functions,
            expected_artifact: &expected,
            invocation_context: invocation_context.as_deref(),
        };
        let mut meter =
            zil_verify::CtVerificationMeter::new(self.budget.remaining().max_analysis_work);
        let result = zil_verify::verify_ct_artifact_metered(
            expected.clone(),
            &selected_limits,
            &authority,
            &mut meter,
        );
        self.budget.reserve_analysis_work(meter.spent())?;
        self.budget.release_temporary_bytes(copied_bytes);
        if let Some(context) = invocation_context {
            self.budget.release_temporary_bytes(context.bytes);
        }
        self.budget.release_temporary_bytes(module_bytes);
        result.map(|_| ())
    }

    pub(super) fn evaluate(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        input: Option<&CtTokenTree>,
        arguments: &[CtValue],
        span: Span,
    ) -> CompileResult<CtValue> {
        self.evaluate_data_impl(
            entry,
            input_tokens,
            input,
            arguments,
            span,
            false,
            None,
            None,
        )
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
        self.evaluate_data_impl(
            entry,
            input_tokens,
            input,
            arguments,
            span,
            true,
            None,
            None,
        )
    }
    pub(super) fn evaluate_data_at_source(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        input: Option<&CtTokenTree>,
        arguments: &[CtValue],
        span: Span,
        site: compiler::source_sites::SourceSite,
    ) -> CompileResult<EvaluatedData> {
        self.evaluate_data_impl(
            entry,
            input_tokens,
            input,
            arguments,
            span,
            true,
            Some(site),
            None,
        )
    }
    pub(super) fn evaluate_runtime_data_at_source(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        span: Span,
        source: &compiler::runtime_context::RuntimeEntrySource,
    ) -> CompileResult<EvaluatedData> {
        self.evaluate_data_impl(
            entry,
            input_tokens,
            None,
            &[],
            span,
            true,
            Some(source.expression_site),
            Some(source),
        )
    }

    pub(super) fn check_runtime_definition_expression(
        &mut self,
        definition: &CtSymbolId,
        site: compiler::source_sites::SourceSite,
    ) -> CompileResult<zil_syntax::ast::TypeRef> {
        self.require_source_origins()?;
        let module_bytes = (self.parsed.len() as u64)
            .saturating_mul(std::mem::size_of::<&zil_syntax::ast::Module>() as u64)
            .saturating_add(
                (self.sources.len().min(self.parsed.len()) as u64)
                    .saturating_mul(std::mem::size_of::<compiler::SourceModule<'_>>() as u64),
            );
        self.budget
            .reserve_analysis_work((self.parsed.len() + self.sources.len()) as u64)?;
        self.budget.reserve_temporary_bytes(module_bytes)?;
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        let before = self.remaining_limits();
        let mut remaining = before.clone();
        let result = compiler::runtime_context::check_runtime_definition_expression(
            &modules,
            Some(&self.generated_catalog),
            definition,
            site,
            &mut remaining,
        );
        self.budget.reserve_analysis_work(
            before
                .max_analysis_steps
                .saturating_sub(remaining.max_analysis_steps) as u64,
        )?;
        self.budget
            .reserve_generated_nodes(before.max_nodes.saturating_sub(remaining.max_nodes) as u64)?;
        for _ in 0..before.max_functions.saturating_sub(remaining.max_functions) {
            self.budget.reserve_instance()?;
        }
        let bytes = before
            .max_source_bytes
            .saturating_sub(remaining.max_source_bytes) as u64;
        self.budget.reserve_temporary_bytes(bytes)?;
        // Scratch is released; only the derived result AST survives in Expand.
        let keep = result.as_ref().map_or(0, |(_, bytes)| *bytes);
        self.budget
            .release_temporary_bytes(bytes.saturating_sub(keep));
        self.budget.release_temporary_bytes(module_bytes);
        result.map(|(ty, _)| ty)
    }

    // Keep the source, candidate, scope and request meter explicit at this proof boundary.

    #[allow(clippy::too_many_arguments)]
    fn evaluate_data_impl(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        input: Option<&CtTokenTree>,
        arguments: &[CtValue],
        span: Span,
        retain_context: bool,
        source_site: Option<compiler::source_sites::SourceSite>,
        runtime_source: Option<&compiler::runtime_context::RuntimeEntrySource>,
    ) -> CompileResult<EvaluatedData> {
        // This retained source authority is outside invocation scratch and
        // survives failed expression cleanup until the epoch commits.
        self.freeze_stage_before_execution()?;
        let before = self.budget.live_temporary_bytes();
        let query_capacity = self.budget.usage().queries.capacity();
        let result = self.evaluate_data_inner(
            entry,
            input_tokens,
            input,
            arguments,
            span,
            retain_context,
            source_site,
            runtime_source,
        );
        if result.is_err() {
            let keep = (self
                .budget
                .usage()
                .queries
                .capacity()
                .saturating_sub(query_capacity) as u64)
                .saturating_mul(std::mem::size_of::<CtQueryDependency>() as u64);
            let added = self.budget.live_temporary_bytes().saturating_sub(before);
            self.budget
                .release_temporary_bytes(added.saturating_sub(keep));
        }
        result
    }
    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    fn evaluate_data_inner(
        &mut self,
        entry: Entry<'_>,
        input_tokens: &CtInputTable,
        input: Option<&CtTokenTree>,
        arguments: &[CtValue],
        span: Span,
        retain_context: bool,
        source_site: Option<compiler::source_sites::SourceSite>,
        runtime_source: Option<&compiler::runtime_context::RuntimeEntrySource>,
    ) -> CompileResult<EvaluatedData> {
        // Fixed-size source seed only; type/category inputs are not proof and
        // are unnecessary when retrying complete lexical freezing after compile.
        let retry_entry = match &entry {
            Entry::Expression {
                module,
                expression,
                definition,
                lexical_owner,
                ..
            } => Some(Entry::Expression {
                module: *module,
                expression,
                definition: definition.clone(),
                lexical_owner: lexical_owner.clone(),
                expected: None,
                parameters: &[],
            }),
            Entry::Function(symbol) => Some(Entry::Function(symbol.clone())),
        };
        // Missing source origins may need a required capture once fresh source
        // calls are known. Defer this one freeze, rather than fail before the
        // existing call-triggered capture or repeat the old no-call scan.
        let delayed_original_freeze = runtime_source.is_none() && self.source_origins.is_none();
        let mut invocation_context = if let Some(source) = runtime_source {
            self.require_source_origins()?;
            super::invocation_context::FrozenInvocationContext::freeze_runtime_at_source(
                &entry,
                self.parsed,
                &self.index,
                self.source_origins.as_ref(),
                Some(&self.generated_catalog),
                source,
                &mut self.budget,
            )?
        } else if delayed_original_freeze {
            None
        } else {
            super::invocation_context::FrozenInvocationContext::freeze_at_source(
                &entry,
                self.parsed,
                &self.index,
                self.source_origins.as_ref(),
                Some(&self.generated_catalog),
                source_site,
                &mut self.budget,
            )?
        };
        let runtime_module_bytes = if runtime_source.is_some() {
            let bytes = (self.parsed.len() as u64)
                .saturating_mul(std::mem::size_of::<&zil_syntax::ast::Module>() as u64)
                .saturating_add(
                    (self.sources.len().min(self.parsed.len()) as u64)
                        .saturating_mul(std::mem::size_of::<compiler::SourceModule<'_>>() as u64),
                );
            self.budget
                .reserve_analysis_work((self.parsed.len() + self.sources.len()) as u64)?;
            self.budget.reserve_temporary_bytes(bytes)?;
            bytes
        } else {
            0
        };
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        let selected_limits = self.remaining_limits();
        let request = CompileRequest {
            target_pointer_width: self.target.pointer_width,
            modules: &modules,
            source_fingerprint: self.source_fingerprint,
            entry,
            input_tokens,
            snapshot: &self.snapshot,
            limits: &selected_limits,
        };
        let (mut expected, source_bindings) = if let Some(source) = runtime_source {
            compiler::compile_scoped_runtime_at_source(
                request,
                Some(&self.generated_catalog),
                source,
            )?
        } else {
            compiler::compile_scoped_at_source(request, Some(&self.generated_catalog), source_site)?
        };
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
        self.attach_dynamic_catalogue(&mut expected)?;
        if invocation_context.is_none() && !source_bindings.calls.is_empty() {
            self.require_source_origins()?;
            invocation_context =
                super::invocation_context::FrozenInvocationContext::freeze_at_source_force(
                    retry_entry.as_ref().ok_or_else(|| {
                        error("complete source call lost its immutable entry", span)
                    })?,
                    self.parsed,
                    &self.index,
                    self.source_origins.as_ref(),
                    Some(&self.generated_catalog),
                    source_site,
                    true,
                    &mut self.budget,
                )?;
        }
        if delayed_original_freeze && invocation_context.is_none() {
            invocation_context =
                super::invocation_context::FrozenInvocationContext::freeze_at_source(
                    retry_entry
                        .as_ref()
                        .ok_or_else(|| error("source freezing lost its immutable entry", span))?,
                    self.parsed,
                    &self.index,
                    self.source_origins.as_ref(),
                    Some(&self.generated_catalog),
                    source_site,
                    &mut self.budget,
                )?;
        }
        if let Some(source) = runtime_source {
            invocation_context
                .as_mut()
                .and_then(Arc::get_mut)
                .ok_or_else(|| error("runtime CT requires its frozen original invocation", span))?
                .extend_runtime_entry(
                    source,
                    &source_bindings.runtime_frames,
                    &expected,
                    &self.index,
                    &mut self.budget,
                )?;
        }
        if !source_bindings.calls.is_empty() || !source_bindings.relocations.is_empty() {
            let context = invocation_context
                .as_mut()
                .and_then(std::sync::Arc::get_mut)
                .ok_or_else(|| {
                    vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "CT factory actuals require an authentic current invocation",
                        Some(span),
                    )]
                })?;
            context.extend_calls(
                &source_bindings.calls,
                &source_bindings.scopes,
                &expected,
                &self.index,
                &mut self.budget,
            )?;
            context.validate_record_relocations(
                &source_bindings.relocations,
                &source_bindings.calls,
                &expected,
                &mut self.budget,
            )?;
        }
        if !expected.ambient_life_scopes.is_empty()
            || !source_bindings.type_sites.is_empty()
            || !source_bindings.bindings.is_empty()
        {
            let context = invocation_context
                .as_mut()
                .and_then(std::sync::Arc::get_mut)
                .ok_or_else(|| {
                    error(
                        "contextual Type requires an authentic source invocation",
                        span,
                    )
                })?;
            context.validate_type_contexts(
                &source_bindings,
                &expected,
                &self.index,
                &mut self.budget,
            )?;
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
            invocation_context: invocation_context.as_deref(),
        };
        let mut meter =
            zil_verify::CtVerificationMeter::new(self.budget.remaining().max_analysis_work);
        let result =
            zil_verify::verify_ct_artifact_metered(raw, &selected_limits, &authority, &mut meter);
        self.budget.reserve_analysis_work(meter.spent())?;
        let verified = result?;
        let reader = FrozenReadView {
            record_schemas: &expected.record_schemas,
            enum_schemas: &expected.enum_schemas,
            ambient_life_scopes: &expected.ambient_life_scopes,
            invocation_context: invocation_context.as_deref(),
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
            stage_inventory: self.stage_inventory.as_deref(),
        };
        self.budget
            .reserve_analysis_work(source_bindings.calls.len() as u64)?;
        let has_host_calls = source_bindings
            .calls
            .iter()
            .any(|call| call.enclosing.is_some());
        let evaluated = if has_host_calls {
            zil_ct::evaluate_ct_artifact_with_completed_calls(
                &verified,
                arguments,
                &reader,
                &mut self.budget,
            )
        } else {
            zil_ct::evaluate_ct_artifact(&verified, arguments, &reader, &mut self.budget)
                .map(|(value, usage)| (value, usage, Vec::new()))
        };
        let (value, _usage, completed_calls) = evaluated.map_err(|mut errors| {
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
        })?;
        self.budget.release_usage_receipt(_usage);
        let result_ty =
            &verified.program().functions[verified.program().entry.0 as usize].result_type;
        let context = if retain_context
            && (matches!(
                result_ty,
                CtTy::Record(_)
                    | CtTy::Enum(_)
                    | CtTy::AnonymousSum(_)
                    | CtTy::Tuple(_)
                    | CtTy::Array { .. }
                    | CtTy::List(_)
            ) || matches!(&value, CtValue::Type(graph) if !graph.ambient_classes.is_empty()))
        {
            value_context::ValueContext::capture(
                verified.program(),
                &source_bindings.bindings,
                invocation_context.clone(),
                &mut self.budget,
            )
            .map(Some)
        } else {
            Ok(None)
        };
        // Capture can allocate while the independently verified candidate is
        // still live. Reclaim its charge only after every consumer finishes,
        // including the error path from capture.
        drop(verified);
        self.budget.release_temporary_bytes(copied_bytes);
        let context = context?;
        self.budget.reserve_analysis_work(
            (source_bindings.calls.len() as u64).saturating_mul(completed_calls.len() as u64),
        )?;
        let retain_host = source_bindings.calls.iter().any(|call| {
            call.enclosing.is_some()
                && call.execution.is_some_and(|(block, instruction)| {
                    completed_calls.iter().any(|event| {
                        event.caller == call.caller
                            && event.block == block
                            && event.instruction == instruction
                            && event.callee == call.instance
                    })
                })
        });
        if retain_host {
            // Only a successful independently admitted, verified AND evaluated
            // request reaches this private event constructor.
            self.retain_static_host_invocation(
                expected,
                source_bindings.calls,
                source_bindings.scopes,
                invocation_context
                    .as_ref()
                    .ok_or_else(|| error("CT namespace lost its authentic invocation", span))?
                    .clone(),
                completed_calls,
            )?;
        } else {
            let completed_bytes = completed_calls.capacity() as u64
                * std::mem::size_of::<zil_ct::CtCompletedCall>() as u64;
            drop(completed_calls);
            self.budget.release_temporary_bytes(completed_bytes);
        }
        if context.is_none()
            && !retain_host
            && let Some(invocation_context) = invocation_context
        {
            self.budget
                .release_temporary_bytes(invocation_context.bytes);
        }
        self.budget.release_temporary_bytes(runtime_module_bytes);
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: self.stage_inventory.as_deref(),
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
    fn complete_core_dynamic_inventory_is_data_not_a_provider_certificate() {
        let text = r#"
struct Provider { n:i32 }
trait Read { fn read(self:&Self)->i32; }
impl Read for Provider { fn read(self:&Self)->i32 { return self.n } }
fn use_provider()->i32 {
    match owned_dyn::<dyn Read>(Provider{n:42}) {
      .err(_)=>{return 99}, .ok(owner)=>{return owner.read()-42}
    }
}
fn identity(t:Type)->Type {return t}
"#;
        let sources = vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "complete.zil",
            text,
        )];
        let parsed = vec![Arc::new(zil_syntax::parse(&sources[0]))];
        assert!(
            parsed[0].diagnostics.is_empty(),
            "{:?}",
            parsed[0].diagnostics
        );
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let paths = BTreeMap::new();
        let core = zil_expand::expand_with_map_limits_target_context(
            &sources[0],
            &parsed[0].module,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        assert!(!core.dyn_declarations.is_empty());
        assert!(!core.dyn_interfaces.is_empty());
        let program = zil_ir::CoreProgram {
            root: ModuleId(0),
            source_fingerprint: sources[0].fingerprint(),
            modules: vec![zil_ir::ProgramModule {
                core,
                imports: vec![],
                exports: vec![],
            }],
        };
        let mut environment = Environment::new(
            &sources,
            &parsed,
            &paths,
            &[vec![]],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        assert!(environment.dynamic_catalogue.is_none()); // Pure shapes are not a full inventory.
        environment.bind_frozen_core_inventory(&program).unwrap();
        let catalogue = environment.dynamic_catalogue.as_ref().unwrap();
        assert_eq!(
            catalogue.declarations.len(),
            program.modules[0].core.dyn_declarations.len()
        );
        assert_eq!(
            catalogue.interfaces.len(),
            program.modules[0].core.dyn_interfaces.len()
        );
        assert_eq!(
            catalogue.vtables.len(),
            program.modules[0].core.dyn_vtables.len()
        );
        assert!(!catalogue.types.is_empty());
        let item=parsed[0].module.items.iter().find(|item|matches!(&item.kind,zil_syntax::ast::ItemKind::Function(function) if function.name=="identity")).unwrap();
        let symbol = environment.index.symbol_at(ModuleId(0), item.span).unwrap();
        let input = environment.empty_input(item.span).unwrap();
        let argument = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: vec![],
        });
        // The full raw catalogue travels through real Artifact Wire and the
        // independent CT verifier. A changed original declaration cannot reuse
        // the frozen source authority of the actual provider function.
        let syntax = parsed
            .iter()
            .map(|parsed| &parsed.module)
            .collect::<Vec<_>>();
        let modules = environment.index.modules(&sources, &syntax);
        let (mut expected, _) = compiler::compile_scoped(CompileRequest {
            target_pointer_width: target.pointer_width,
            modules: &modules,
            source_fingerprint: sources[0].fingerprint(),
            entry: Entry::Function(symbol.clone()),
            input_tokens: &input,
            snapshot: &environment.snapshot,
            limits: &limits,
        })
        .unwrap();
        environment.attach_dynamic_catalogue(&mut expected).unwrap();
        let authority = FrozenAuthority {
            source_fingerprint: sources[0].fingerprint(),
            snapshot: &environment.snapshot,
            input: &input,
            index: &environment.index,
            expected_functions: &expected.functions,
            expected_artifact: &expected,
            invocation_context: None,
        };
        let bytes = zil_ir::ct_wire::encode_program(&expected, &limits).unwrap();
        let decoded = zil_ir::ct_wire::decode_program(&bytes, &limits).unwrap();
        zil_verify::verify_ct_artifact(decoded, &limits, &authority).unwrap();
        let mut forged = expected.clone();
        forged.dynamic_catalogue.as_mut().unwrap().declarations[0]
            .identity
            .package
            .push_str("-forged");
        let bytes = zil_ir::ct_wire::encode_program(&forged, &limits).unwrap();
        let decoded = zil_ir::ct_wire::decode_program(&bytes, &limits).unwrap();
        assert!(zil_verify::verify_ct_artifact(decoded, &limits, &authority).is_err());
        let output = environment
            .evaluate(
                Entry::Function(symbol),
                &input,
                None,
                std::slice::from_ref(&argument),
                item.span,
            )
            .unwrap();
        assert_eq!(output, argument);
        assert!(environment.bind_frozen_core_inventory(&program).is_err());
    }

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
        let paths = BTreeMap::new();
        let mut environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Nominal {
                symbol: subject,
                arguments: Vec::new(),
            }],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        });
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let mut reader = FrozenReadView {
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Nominal {
                symbol: subject,
                arguments: Vec::new(),
            }],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        });
        let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let reader = FrozenReadView {
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let arg = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Builtin(CtBuiltinType::ByteBuffer)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let i32_type = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
                    scoped_catalogue: None,
                    nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                    root: CtTypeNodeId(0),
                    ambient_classes: Vec::new(),
                })],
                &mut budget,
            )
            .unwrap();
        assert_eq!(
            info.ty, direct,
            "reflected and directly applied Type must have one canonical DAG"
        );
        let bool_type = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::Bool)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let i32_value = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        });
        let bool_value = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::Bool)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let arg = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        assert_eq!(
            reader.type_name(&requester, &number, &mut budget).unwrap(),
            CtTypeRef {
                scoped_catalogue: None,
                nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                root: CtTypeNodeId(0),
                ambient_classes: Vec::new()
            }
        );
        assert_eq!(
            reader.type_name(&requester, &top, &mut budget).unwrap(),
            CtTypeRef {
                scoped_catalogue: None,
                nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                root: CtTypeNodeId(0),
                ambient_classes: Vec::new()
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
                    scoped_catalogue: None,
                    nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                    root: CtTypeNodeId(0),
                    ambient_classes: Vec::new(),
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
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
                    scoped_catalogue: None,
                    nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                    root: CtTypeNodeId(0),
                    ambient_classes: Vec::new(),
                })],
                &mut budget,
            )
            .unwrap();
        assert_eq!(
            chained, direct,
            "generic aliases must normalize through checked alias chains"
        );
        let i32_value = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        });
        let bool_value = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::Bool)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        assert_eq!(
            reader.type_name(&requester, &alias, &mut budget).unwrap(),
            CtTypeRef {
                scoped_catalogue: None,
                nodes: vec![
                    CtTypeNode::Primitive(CtScalarTy::I32),
                    CtTypeNode::Primitive(CtScalarTy::I64),
                    CtTypeNode::Tuple(vec![CtTypeNodeId(0), CtTypeNodeId(1)])
                ],
                root: CtTypeNodeId(2),
                ambient_classes: Vec::new()
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
        let paths = BTreeMap::new();
        let environment = Environment::new(
            &sources,
            &parsed,
            &paths,
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
            record_schemas: &[],
            enum_schemas: &[],
            ambient_life_scopes: &[],
            invocation_context: None,
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
            stage_inventory: None,
        };
        let mut budget = CtExecutionBudget::new(environment.budget.remaining());
        let i32_type = CtValue::Type(CtTypeRef {
            scoped_catalogue: None,
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
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
    #[test]
    fn ct5_failure_after_successful_scope_freeze_releases_invocation_bytes() {
        let sources = vec![SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "scope-failure.zil",
            "struct Phantom<life a>{n:i32} fn failing<life a>()->i32{return #(missing())}",
        )];
        let parsed = vec![Arc::new(zil_syntax::parse(&sources[0]))];
        assert!(parsed[0].diagnostics.is_empty());
        let syntax = vec![&parsed[0].module];
        let paths = BTreeMap::new();
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let (origins, _) = zil_expand::source_context::capture_original(
            &sources,
            &syntax,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let paths = BTreeMap::new();
        let mut environment = Environment::new(
            &sources,
            &parsed,
            &paths,
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        environment.bind_source_origins(origins.as_deref()).unwrap();
        environment.budget.reserve_temporary_bytes(37).unwrap();
        let before = environment.budget.live_temporary_bytes();
        let symbol = CtSymbolId {
            module: ModuleId(0),
            declaration: zil_base::NodeId(1),
            generation_path: None,
        };
        let context = super::super::invocation_context::FrozenInvocationContext::freeze(
            &Entry::Function(symbol.clone()),
            &parsed,
            &environment.index,
            environment.source_origins.as_ref(),
            &mut environment.budget,
        )
        .unwrap()
        .unwrap();
        assert!(context.bytes > 0);
        environment.budget.release_temporary_bytes(context.bytes);
        drop(context);
        let zil_syntax::ast::ItemKind::Function(function) = &parsed[0].module.items[1].kind else {
            unreachable!()
        };
        let zil_syntax::ast::FunctionBody::Block(block) = &function.body else {
            unreachable!()
        };
        let zil_syntax::ast::StmtKind::Return(Some(expression)) = &block.statements[0].kind else {
            unreachable!()
        };
        let zil_syntax::ast::ExprKind::MetaEval(expression) = &expression.kind else {
            unreachable!()
        };
        let input = environment.empty_input(expression.span).unwrap();
        let result = environment.evaluate_data(
            Entry::Expression {
                module: ModuleId(0),
                expression,
                lexical_owner: Some(symbol.clone()),
                definition: symbol,
                expected: None,
                parameters: &[],
            },
            &input,
            None,
            &[],
            expression.span,
        );
        assert!(result.is_err());
        assert_eq!(environment.budget.live_temporary_bytes(), before);
    }
    #[test]
    fn ct5_actual_definition_scope_raw_wire_and_independent_budget_controls() {
        for (text, definition_ordinal, sibling_ordinal) in [
            (
                "struct Phantom<life a>{n:i32} fn scalar(t:Type,n:i32)->i32{return n} fn first<T:Copy,life a,life b>(p:&'a i32)->i32{let value:Phantom<'a>=#(Phantom::<'a>{n:scalar(i32,42)});return value.n} fn second<T:Copy,life a,life b>(p:&'a i32)->i32{return 0}",
                2,
                3,
            ),
            (
                "struct Phantom<life a>{n:i32} fn scalar(t:Type,n:i32)->i32{return n} struct Reader{fn first<T:Copy,life a,life b>(p:&'a i32)->i32{let value:Phantom<'a>=#(Phantom::<'a>{n:scalar(i32,42)});return value.n} fn second<T:Copy,life a,life b>(p:&'a i32)->i32{return 0}}",
                4,
                5,
            ),
        ] {
            check_actual_scope_control(text, definition_ordinal, sibling_ordinal);
        }
    }
    fn check_actual_scope_control(text: &str, definition_ordinal: u32, sibling_ordinal: u32) {
        let sources = vec![SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "scope-raw.zil",
            text,
        )];
        let parsed = vec![Arc::new(zil_syntax::parse(&sources[0]))];
        assert!(
            parsed[0].diagnostics.is_empty(),
            "{:?}",
            parsed[0].diagnostics
        );
        let syntax = vec![&parsed[0].module];
        let paths = BTreeMap::new();
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let (origins, _) = zil_expand::source_context::capture_original(
            &sources,
            &syntax,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let origins = Arc::new(origins.unwrap());
        let paths = BTreeMap::new();
        let mut environment = Environment::new(
            &sources,
            &parsed,
            &paths,
            &[Vec::new()],
            ModuleId(0),
            sources[0].fingerprint(),
            &target,
            &limits,
        )
        .unwrap();
        let definition = CtSymbolId {
            module: ModuleId(0),
            declaration: zil_base::NodeId(definition_ordinal),
            generation_path: None,
        };
        let original = super::super::compiler::source_scope::original_function(
            &parsed[0].module,
            definition.declaration,
            limits.max_call_depth,
            |_| Ok(()),
        )
        .unwrap()
        .unwrap();
        let function = original.function;
        let zil_syntax::ast::FunctionBody::Block(block) = &function.body else {
            unreachable!()
        };
        let zil_syntax::ast::StmtKind::Binding { value, .. } = &block.statements[0].kind else {
            unreachable!()
        };
        let zil_syntax::ast::ExprKind::MetaEval(expression) = &value.kind else {
            unreachable!()
        };
        let mut invocation = super::super::invocation_context::FrozenInvocationContext::freeze(
            &Entry::Function(definition.clone()),
            &parsed,
            &environment.index,
            Some(&origins),
            &mut environment.budget,
        )
        .unwrap()
        .unwrap();
        let input = environment.empty_input(expression.span).unwrap();
        let modules = environment.index.modules(&sources, &syntax);
        let unavailable_entry = || Entry::Expression {
            module: ModuleId(0),
            expression,
            definition: definition.clone(),
            lexical_owner: None,
            expected: None,
            parameters: &[],
        };
        assert!(
            super::super::invocation_context::FrozenInvocationContext::freeze(
                &unavailable_entry(),
                &parsed,
                &environment.index,
                Some(&origins),
                &mut environment.budget,
            )
            .unwrap()
            .is_none(),
            "a naming owner is not an authentic lexical binder"
        );
        let errors = compiler::compile_scoped(CompileRequest {
            target_pointer_width: target.pointer_width,
            modules: &modules,
            source_fingerprint: sources[0].fingerprint(),
            entry: unavailable_entry(),
            input_tokens: &input,
            snapshot: &environment.snapshot,
            limits: &limits,
        })
        .expect_err("missing original scope must not bind a source lifetime");
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("actual lexical source owner")),
            "{errors:?}"
        );
        let (expected, source_bindings) = compiler::compile_scoped(CompileRequest {
            target_pointer_width: target.pointer_width,
            modules: &modules,
            source_fingerprint: sources[0].fingerprint(),
            entry: Entry::Expression {
                module: ModuleId(0),
                expression,
                lexical_owner: Some(definition.clone()),
                definition,
                expected: None,
                parameters: &[],
            },
            input_tokens: &input,
            snapshot: &environment.snapshot,
            limits: &limits,
        })
        .unwrap();
        Arc::get_mut(&mut invocation)
            .unwrap()
            .validate_type_contexts(
                &source_bindings,
                &expected,
                &environment.index,
                &mut environment.budget,
            )
            .unwrap();
        assert_eq!(expected.ambient_life_scopes.len(), 1);
        assert_eq!(
            expected.record_schemas[0]
                .life_context
                .as_ref()
                .unwrap()
                .ambient_classes[0]
                .source
                .parameter,
            1
        );
        // Private legacy attribution is checked against its own authenticated
        // schema before the result is split into independently reified children.
        let capture_budget = environment.budget.remaining();
        let capture = |bindings: &[compiler::SourceLifeBinding], work: u64| {
            let mut meter = zil_ct::CtExecutionBudget::new(CtDeclaredBudget {
                max_analysis_work: work,
                ..capture_budget
            });
            meter.reserve_temporary_bytes(37).unwrap();
            let result = value_context::ValueContext::capture(
                &expected,
                bindings,
                Some(invocation.clone()),
                &mut meter,
            );
            let used = meter.usage().analysis_work;
            if result.is_ok() {
                let allocated = meter.live_temporary_bytes() - 37;
                meter.release_temporary_bytes(allocated);
            }
            assert_eq!(meter.live_temporary_bytes(), 37);
            (result, used)
        };
        let (_, capture_work) =
            capture(&source_bindings.bindings, capture_budget.max_analysis_work);
        for (work, good) in [
            (capture_work, true),
            (capture_work - 1, false),
            (capture_work, true),
        ] {
            let (result, _) = capture(&source_bindings.bindings, work);
            assert_eq!(result.is_ok(), good);
            if !good {
                assert_eq!(result.err().unwrap()[0].code, "E-RESOURCE-LIMIT");
            }
        }
        for attack in 0..4 {
            let mut bindings = source_bindings.bindings.clone();
            match attack {
                0 => bindings[0].schema = u32::MAX,
                1 => bindings[0].enumeration = true,
                2 => bindings[0].parameters[0] = 2,
                _ => bindings[0].definition.declaration = zil_base::NodeId(sibling_ordinal),
            }
            let (result, _) = capture(&bindings, capture_budget.max_analysis_work);
            assert_eq!(result.err().unwrap()[0].code, "E-CT-ARTIFACT");
        }
        eprintln!(
            "CT5 actual-source owner {definition_ordinal} schema capture N={capture_work}/N-1/fresh"
        );
        let authority = FrozenAuthority {
            source_fingerprint: sources[0].fingerprint(),
            snapshot: &environment.snapshot,
            input: &input,
            index: &environment.index,
            expected_functions: &expected.functions,
            expected_artifact: &expected,
            invocation_context: Some(&invocation),
        };
        let encode = |raw: &CtArtifactProgram| {
            zil_ir::ct_wire::decode_program(
                &zil_ir::ct_wire::encode_program(raw, &limits).unwrap(),
                &limits,
            )
            .unwrap()
        };
        let mut meter = zil_verify::CtVerificationMeter::new(limits.max_analysis_steps as u64);
        let verified = zil_verify::verify_ct_artifact_metered(
            encode(&expected),
            &limits,
            &authority,
            &mut meter,
        )
        .unwrap();
        let exact = meter.spent();
        for work in [exact, exact - 1, exact] {
            let mut meter = zil_verify::CtVerificationMeter::new(work);
            let result = zil_verify::verify_ct_artifact_metered(
                expected.clone(),
                &limits,
                &authority,
                &mut meter,
            );
            if work == exact {
                assert!(result.is_ok());
            } else {
                assert_eq!(result.unwrap_err()[0].code, "E-RESOURCE-LIMIT");
            }
        }
        for attack in 0..10 {
            let mut raw = expected.clone();
            match attack {
                0 => {
                    raw.ambient_life_scopes[0].declaration.declaration =
                        zil_base::NodeId(sibling_ordinal)
                }
                1 => {
                    raw.record_schemas[0]
                        .life_context
                        .as_mut()
                        .unwrap()
                        .ambient_classes[0]
                        .source
                        .parameter = 0
                }
                2 => {
                    raw.record_schemas[0]
                        .life_context
                        .as_mut()
                        .unwrap()
                        .ambient_classes[0]
                        .source
                        .parameter = 99
                }
                3 => raw.record_schemas[0]
                    .life_context
                    .as_mut()
                    .unwrap()
                    .ambient_classes
                    .clear(),
                4 => {
                    let graph = raw.record_schemas[0].life_context.as_mut().unwrap();
                    graph.ambient_classes.push(graph.ambient_classes[0].clone());
                }
                5 => {
                    raw.record_schemas[0]
                        .life_context
                        .as_mut()
                        .unwrap()
                        .ambient_classes[0]
                        .owner = CtTypeNodeId(0)
                }
                6 => raw
                    .ambient_life_scopes
                    .push(raw.ambient_life_scopes[0].clone()),
                7 => raw.ambient_life_scopes[0].binder_site.push(0),
                8 => {
                    raw.record_schemas[0]
                        .life_context
                        .as_mut()
                        .unwrap()
                        .ambient_classes[0]
                        .source
                        .scope = CtAmbientScopeId(1)
                }
                _ => raw.ambient_life_scopes[0].declaration.declaration = zil_base::NodeId(0),
            }
            for candidate in [raw.clone(), encode(&raw)] {
                let errors =
                    zil_verify::verify_ct_artifact(candidate, &limits, &authority).unwrap_err();
                assert!(
                    errors.iter().all(|error| !error
                        .message
                        .contains("source or frozen snapshot is not authenticated")),
                    "attack {attack}: {errors:?}"
                );
            }
        }
        let reader = FrozenReadView {
            record_schemas: &expected.record_schemas,
            enum_schemas: &expected.enum_schemas,
            ambient_life_scopes: &expected.ambient_life_scopes,
            invocation_context: Some(&invocation),
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
            stage_inventory: None,
        };
        let (value, usage) =
            zil_ct::evaluate_ct_artifact(&verified, &[], &reader, &mut environment.budget).unwrap();
        assert!(matches!(value, CtValue::Record { .. }));
        environment.budget.release_value(&value);
        environment.budget.release_usage_receipt(usage);
        eprintln!(
            "CT5 actual-source owner {definition_ordinal} raw/Wire independent work boundary {exact}"
        );
    }
}

#[path = "value_context.rs"]
pub(super) mod value_context;
pub(super) struct EvaluatedData {
    pub value: CtValue,
    pub context: Option<value_context::ValueContext>,
}

#[cfg(test)]
#[path = "mixed_instance_tests.rs"]
mod mixed_instance_tests;

#[cfg(test)]
#[path = "record_relocation_tests.rs"]
mod record_relocation_tests;
