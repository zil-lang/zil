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
