//! Draft definition classification. This compiler/verifier path never executes
//! a VM or exposes the candidate index to a query.
use super::*;

pub(super) struct DefinitionInventory<'a, 'b> {
    pub sources: &'a [SourceUnit],
    pub parsed: &'a [Arc<ParseResult>],
    pub index: &'a SourceIndex,
    pub generated_catalog: &'a zil_expand::source_context::GeneratedSourceCatalog,
    pub source_fingerprint: Fingerprint,
    pub target: &'a TargetProfile,
    pub limits: &'a Limits,
    pub snapshot: &'a CtSnapshotRequest,
    pub dynamic_catalogue: Option<&'a CtDynamicCatalogue>,
    pub budget: &'b mut CtExecutionBudget,
}
impl DefinitionInventory<'_, '_> {
    pub(super) fn classify(&mut self) -> CompileResult<Vec<CtSymbolId>> {
        let syntax = self.parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        self.check_original_definitions(&modules)?;
        let mut limits = self.remaining_limits();
        let before = self.budget.remaining();
        let catalog = (!self.generated_catalog.trees.is_empty()).then_some(self.generated_catalog);
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
        let mut ct_only = generic_definitions;
        let Some(entry) = entry else {
            return Ok(ct_only);
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
        ct_only.extend(classified);
        Ok(ct_only)
    }
    fn check_original_definitions(
        &mut self,
        modules: &[compiler::SourceModule<'_>],
    ) -> CompileResult<()> {
        use zil_verify::{CtOriginalImport, CtOriginalImportTarget, CtOriginalModule};
        // These are borrowed views of the full immutable source trees, before
        // classification can discard a CT-only declaration from runtime syntax.
        // Keep every generated tree, including consumed intermediate emitters.
        let imports_count = modules.iter().try_fold(0usize, |count, module| {
            count
                .checked_add(module.imports.len())
                .ok_or_else(|| error("CT original import view size overflow", Span::default()))
        })?;
        let bytes = imports_count
            .checked_mul(std::mem::size_of::<CtOriginalImport<'_>>())
            .and_then(|bytes| {
                bytes.checked_add(modules.len().checked_mul(
                    std::mem::size_of::<CtOriginalModule<'_>>()
                        + std::mem::size_of::<Vec<CtOriginalImport<'_>>>(),
                )?)
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    self.generated_catalog
                        .trees
                        .len()
                        .checked_mul(std::mem::size_of::<&zil_syntax::ast::Module>())?,
                )
            })
            .ok_or_else(|| error("CT original source view size overflow", Span::default()))?;
        self.budget.reserve_analysis_work(
            imports_count
                .saturating_add(modules.len())
                .saturating_add(self.generated_catalog.trees.len()) as u64,
        )?;
        self.budget.reserve_temporary_bytes(bytes as u64)?;
        let imports = modules
            .iter()
            .map(|module| {
                module
                    .imports
                    .iter()
                    .map(|import| CtOriginalImport {
                        alias: &import.alias,
                        target: match import.target {
                            compiler::ImportTarget::Module(module) => {
                                CtOriginalImportTarget::Module(module)
                            }
                            compiler::ImportTarget::Meta => CtOriginalImportTarget::Meta,
                            compiler::ImportTarget::Syntax => CtOriginalImportTarget::Syntax,
                        },
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let originals = modules
            .iter()
            .zip(&imports)
            .map(|(module, imports)| CtOriginalModule {
                syntax: module.syntax,
                imports,
            })
            .collect::<Vec<_>>();
        let generated = self
            .generated_catalog
            .trees
            .iter()
            .map(|tree| tree.module.as_ref())
            .collect::<Vec<_>>();
        let limits = self.remaining_limits();
        let mut meter =
            zil_verify::CtVerificationMeter::new(self.budget.remaining().max_analysis_work);
        let result =
            zil_verify::verify_ct_original_definitions(&originals, &generated, self.target, &limits, &mut meter);
        let charged = self.budget.reserve_analysis_work(meter.spent());
        drop(generated);
        drop(originals);
        drop(imports);
        self.budget.release_temporary_bytes(bytes as u64);
        charged?;
        result
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

    fn attach_dynamic_catalogue(&mut self, expected: &mut CtArtifactProgram) -> CompileResult<()> {
        expected.dynamic_catalogue = if let Some(catalogue) = self.dynamic_catalogue {
            let cost =
                crate::ct_artifact::dynamic_catalogue::copy_cost(catalogue, &mut self.budget)?;
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
    fn empty_input(&self, span: Span) -> CompileResult<CtInputTable> {
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
}
