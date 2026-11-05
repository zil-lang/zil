//! Request-local successful calls retained for fresh static namespace checking.
//! This owns actual facts and frozen syntax; no public permission row exists.
use super::super::invocation_context::FrozenInvocationContext;
use super::*;
use zil_expand::runtime_ct::RuntimeDefinition;
use zil_syntax::ast as s;

pub(super) struct Invocation {
    program: CtArtifactProgram,
    calls: Vec<compiler::SourceInstanceCall>,
    scopes: Vec<CtAmbientLifeScope>,
    context: Arc<FrozenInvocationContext>,
}

impl Environment<'_> {
    pub(in crate::ct_artifact) fn has_static_host_uses(&self) -> bool {
        !self.static_host_invocations.is_empty()
    }
    pub(super) fn retain_static_host_invocation(
        &mut self,
        program: CtArtifactProgram,
        calls: Vec<compiler::SourceInstanceCall>,
        scopes: Vec<CtAmbientLifeScope>,
        context: Arc<FrozenInvocationContext>,
    ) -> CompileResult<()> {
        // Payloads move from the already prepaid successful producer request.
        // Reserve the entire replacement backing before Vec can grow.
        let bytes = (self.static_host_invocations.len() + 1) as u64
            * std::mem::size_of::<Invocation>() as u64;
        self.budget
            .reserve_analysis_work(self.static_host_invocations.len() as u64 + 1)?;
        self.budget.reserve_temporary_bytes(bytes)?;
        self.static_host_invocations.reserve_exact(1);
        self.static_host_invocations.push(Invocation {
            program,
            calls,
            scopes,
            context,
        });
        Ok(())
    }
    pub(in crate::ct_artifact) fn check_static_host(
        &mut self,
        definition: &RuntimeDefinition,
        actual: &s::Item,
    ) -> CompileResult<()> {
        if definition.member.is_some() || definition.origin.retained_members.is_some() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Expand,
                "CT namespace transformed member inventory needs complete source correspondence",
                Some(actual.span),
            )]);
        }
        let generation_path = definition
            .origin
            .generated
            .as_ref()
            .and_then(|g| g.tree.generation_fingerprint());
        let symbol = CtSymbolId {
            module: definition.module,
            declaration: definition.origin.declaration,
            generation_path,
        };
        self.budget
            .reserve_analysis_work(self.static_host_invocations.len() as u64)?;
        let mut source = None;
        for invocation in &self.static_host_invocations {
            self.budget
                .reserve_analysis_work(invocation.calls.len() as u64)?;
            for call in &invocation.calls {
                if let Some((host, identity)) = invocation.context.static_host_source(
                    call,
                    &invocation.scopes,
                    &invocation.program,
                    &symbol,
                    &self.index,
                    &mut self.budget,
                )? {
                    let identity_work = super::super::invocation_context::identity_cost(
                        identity,
                        &mut self.budget,
                    )?;
                    self.budget.reserve_analysis_work(identity_work)?;
                    if *identity != definition.origin.identity {
                        return Err(error(
                            "CT namespace origin differs from its actual source host",
                            actual.span,
                        ));
                    }
                    if let Some(generated) = &definition.origin.generated {
                        self.budget
                            .reserve_analysis_work(self.generated_catalog.trees.len() as u64 + 2)?;
                        if !self
                            .generated_catalog
                            .trees
                            .iter()
                            .any(|tree| Arc::ptr_eq(tree, &generated.tree))
                            || generated.tree.module.id != definition.module
                            || generated.tree.declarations.get(generated.root as usize)
                                != Some(&symbol.declaration)
                            || !generated
                                .tree
                                .module
                                .items
                                .get(generated.root as usize)
                                .is_some_and(|item| std::ptr::eq(item, host))
                        {
                            return Err(error(
                                "CT namespace generated root is outside its actual event catalog",
                                actual.span,
                            ));
                        }
                    }
                    source = Some(host);
                    break;
                }
            }
            if source.is_some() {
                break;
            }
        }
        let source = source.ok_or_else(|| {
            vec![Diagnostic::unsupported(
                Phase::Expand,
                "generic resource or methods require capability resolution",
                Some(actual.span),
            )]
        })?;
        let limits = self.remaining_limits_for_input();
        let cost = zil_expand::ct_selection::item_cost(source, &limits)?;
        let actual_cost = zil_expand::ct_selection::item_cost(actual, &limits)?;
        self.budget
            .reserve_analysis_work(cost.work.saturating_add(actual_cost.work) as u64)?;
        let bytes = cost
            .nodes
            .saturating_mul(
                std::mem::size_of::<s::Item>()
                    + std::mem::size_of::<s::Stmt>()
                    + std::mem::size_of::<s::Expr>()
                    + std::mem::size_of::<s::TypeRef>()
                    + std::mem::size_of::<s::Pattern>(),
            )
            .saturating_add(cost.work) as u64;
        self.budget.reserve_temporary_bytes(bytes)?;
        self.budget.reserve_generated_nodes(cost.nodes as u64)?;
        let mut syntax = zil_syntax::GeneratedSyntax::ItemList(vec![source.clone()]);
        if definition.origin.generated.is_some() {
            let work = zil_expand::remap_generated_spans(
                &mut syntax,
                actual.span,
                &self.remaining_limits_for_input(),
            )?;
            self.budget.reserve_analysis_work(work as u64)?;
        }
        let zil_syntax::GeneratedSyntax::ItemList(expected) = syntax else {
            unreachable!()
        };
        self.budget
            .reserve_analysis_work(cost.work.saturating_add(actual_cost.work) as u64)?;
        if expected[0] != *actual {
            return Err(error(
                "CT namespace retained whole host differs from its actual frozen source",
                actual.span,
            ));
        }
        drop(expected);
        self.budget.release_temporary_bytes(bytes);
        let module_bytes = (self.parsed.len() * std::mem::size_of::<&s::Module>()
            + self.sources.len().min(self.parsed.len())
                * std::mem::size_of::<compiler::SourceModule<'_>>())
            as u64;
        self.budget
            .reserve_analysis_work((self.parsed.len() + self.sources.len()) as u64)?;
        self.budget.reserve_temporary_bytes(module_bytes)?;
        let syntax = self
            .parsed
            .iter()
            .map(|parsed| &parsed.module)
            .collect::<Vec<_>>();
        let modules = self.index.modules(self.sources, &syntax);
        let before = self.remaining_limits_for_input();
        let mut remaining = before.clone();
        let result = compiler::static_namespace::check(
            &modules,
            Some(&self.generated_catalog),
            source,
            definition.module,
            &mut remaining,
        );
        self.budget.reserve_analysis_work(
            before
                .max_analysis_steps
                .saturating_sub(remaining.max_analysis_steps) as u64,
        )?;
        self.budget
            .reserve_generated_nodes(before.max_nodes.saturating_sub(remaining.max_nodes) as u64)?;
        self.budget.reserve_temporary_bytes(
            before
                .max_source_bytes
                .saturating_sub(remaining.max_source_bytes) as u64,
        )?;
        for _ in remaining.max_functions..before.max_functions {
            self.budget.reserve_instance()?;
        }
        result?;
        self.budget.release_temporary_bytes(module_bytes);
        Ok(())
    }
}
