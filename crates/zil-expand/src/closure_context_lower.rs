//! Actual lowering events paired with the same request's elaboration ledger.
//! Neither SourceMap spans nor a parallel AST walk identify the creator.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct ClosureContextEmission {
    pub(super) marker: u32,
    pub(super) creator: NodeId,
    pub(super) expression: NodeId,
    pub(super) item: usize,
}

impl Expander<'_> {
    pub(super) fn emit_closure_context(&mut self, marker: u32, expression: NodeId, span: Span) {
        let Some(creator) = self.current_function else {
            self.unsupported("source closure context has no actual function creator", span);
            return;
        };
        if self.member_owner_work >= self.member_owner_limit || self.member_owner_nodes == 0 {
            self.diagnostics.push(Diagnostic::limit(
                Phase::Expand,
                "closure lowering context ledger budget exhausted",
                Some(span),
            ));
            return;
        }
        self.member_owner_work += 1;
        self.member_owner_nodes -= 1;
        self.closure_context_emissions.push(ClosureContextEmission {
            marker,
            creator,
            expression,
            item: self.source_item_ordinal,
        });
    }
}

impl generics::products::Products {
    pub(crate) fn bind_closure_contexts(
        &mut self,
        module: &s::Module,
        expanded: &mut ExpandedModule,
        limits: &Limits,
    ) -> CompileResult<()> {
        if expanded.closure_context_emissions.is_empty() {
            return Ok(());
        }
        let allowance = self.allowance.as_ref().unwrap_or(limits);
        let mut budget = generics::Budget::new(allowance);
        let result = (|| {
            let origins = self.origins.as_ref().ok_or_else(|| {
                Diagnostic::unsupported(Phase::Expand, "closure source lineage is absent", None)
            })?;
            for emission in std::mem::take(&mut expanded.closure_context_emissions) {
                budget.node(module.span, 0)?;
                let item = module.items.get(emission.item).ok_or_else(|| {
                    Diagnostic::unsupported(Phase::Expand, "closure creator placement is absent", None)
                })?;
                if !matches!(item.kind, s::ItemKind::Function(_)) {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "closure context requires an original free-function Definition",
                        Some(item.span),
                    ));
                }
                let origin = origins.in_module(module.id, emission.item, &mut budget, item.span)?;
                let row = self.closure_contexts.consume(
                    emission.marker, origin, emission.creator, emission.expression,
                    |work| budget.identity_work(work, item.span),
                )?;
                let owner = self.closure_contexts.owner(row.owner).ok_or_else(|| {
                    Diagnostic::unsupported(Phase::Expand, "closure owner is absent", None)
                })?;
                let identity = &owner.source.identity;
                budget.text(&identity.package, item.span)?;
                for part in identity.module_path.iter().chain(&identity.declaration_path) {
                    budget.node(item.span, 0)?;
                    budget.text(part, item.span)?;
                }
                budget.identity_work(identity.generation_path.len(), item.span)?;
                let mut parameters = Vec::new();
                for parameter in &owner.parameters {
                    budget.node(item.span, 0)?;
                    if !matches!(parameter.kind, s::GenericParamKind::Lifetime) {
                        return Err(Diagnostic::unsupported(
                            Phase::Expand, "closure creator has a non-life Definition parameter", Some(item.span),
                        ));
                    }
                    parameters.push(zil_ir::LifeParameterKind::Lifetime);
                }
                budget.identity_work(row.named_fixed.len().saturating_add(row.inputs.len()), item.span)?;
                expanded.core.closure_life_contexts.push(zil_ir::CoreClosureLifeContext {
                    creator: emission.creator,
                    expression: emission.expression,
                    declaration: zil_ir::LifeDeclarationId {
                        module: origin.declaration.module,
                        declaration: owner.source.declaration,
                        compiler: false,
                    },
                    identity: identity.clone(),
                    parameters,
                    occurrence: row.occurrence,
                    signature: row.signature,
                    named_fixed: row.named_fixed.into_iter().map(|named| zil_ir::CoreClosureNamedFixed {
                        fixed: named.fixed, parameter: named.parameter,
                    }).collect(),
                    inputs: row.inputs.into_iter().map(|input| zil_ir::CoreClosureInputAnchor {
                        parameter: input.parameter,
                        source_parameter: input.source_parameter,
                        header_binder: input.header_binder,
                        position: match input.selector {
                            generics::closure_context::InputSelector::Borrow(position) => zil_ir::CoreClosureHeaderPosition::Borrow(position),
                            generics::closure_context::InputSelector::Class(position) => zil_ir::CoreClosureHeaderPosition::Class(position),
                        },
                    }).collect(),
                });
            }
            Ok(())
        })();
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|error| vec![error])
    }

    pub(crate) fn finish_closure_contexts(&mut self, limits: &Limits) -> CompileResult<()> {
        let mut budget = generics::Budget::new(self.allowance.as_ref().unwrap_or(limits));
        let result = self.closure_contexts.finish(|work| budget.identity_work(work, Span::default()));
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|error| vec![error])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory};

    const SOURCE: &str = "struct Tag<life s>{p:&'s i64}fn apply<life s>(seed:&Tag<'s>)->i32{let f=(p:&Tag<'s>)->i32=>i32(*p.p+*seed.p);return f(seed)}";

    fn expand_source(text: &str, work: usize) -> CompileResult<ExpandedModule> {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "closure-core.zil", text);
        let module = zil_syntax::parse(&source).into_result()?;
        expand_with_map_limits_target(
            &source, &module,
            &Limits { max_analysis_steps: work, ..Limits::default() },
            &zil_base::TargetProfile::host_llvm(),
        )
    }

    #[test]
    fn actual_core_context_binds_creator_outer_expression_and_complete_header() {
        let expanded = expand_source(SOURCE, Limits::default().max_analysis_steps).unwrap();
        assert!(expanded.core.unsupported.is_empty(), "{:?}", expanded.core.unsupported);
        let [context] = expanded.core.closure_life_contexts.as_slice() else { panic!("one actual context") };
        let creator = expanded.core.functions.iter().find(|function| function.id == context.creator).unwrap();
        let statement = &creator.body.statements[0];
        let CoreStatementKind::Binding { initializer: Some(value), .. } = &statement.kind else { panic!("actual source binding") };
        assert!(matches!(value.kind, CoreExprKind::Closure { .. }));
        assert_eq!(context.expression, value.id);
        assert_eq!(context.parameters, [zil_ir::LifeParameterKind::Lifetime]);
        assert_eq!(context.named_fixed, [zil_ir::CoreClosureNamedFixed { fixed: 0, parameter: 0 }]);
        assert!(creator.storage_life.is_some());
        assert!(context.inputs.iter().any(|input| input.parameter == 0 && input.source_parameter == Some(0)));
        assert_eq!(context.declaration.module, expanded.core.module_id);
        assert!(context.identity.generation_path.is_empty());
    }

    #[test]
    fn actual_contexts_have_distinct_creators_and_live_lowering_budget() {
        let second = SOURCE.split_once("fn apply").unwrap().1;
        let text = format!("{SOURCE}fn other{second}");
        let expanded = expand_source(&text, Limits::default().max_analysis_steps).unwrap();
        let [first, second] = expanded.core.closure_life_contexts.as_slice() else { panic!("two source contexts") };
        assert_ne!(first.creator, second.creator);
        assert_ne!(first.expression, second.expression);
        assert_ne!(first.occurrence, second.occurrence);
        assert_ne!(first.declaration, second.declaration);
        assert_ne!(first.identity, second.identity);
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match expand_source(SOURCE, middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert!(errors.iter().all(|error| error.category == ResultCategory::ResourceLimit), "{errors:?}");
                    low = middle;
                }
            }
        }
        expand_source(SOURCE, high).unwrap();
        assert!(expand_source(SOURCE, high - 1).unwrap_err().iter().all(|error| error.category == ResultCategory::ResourceLimit));
        expand_source(SOURCE, high).unwrap();
        println!("actual Source to Core closure context N={high}; N-1/fresh retry verified");
    }
}
