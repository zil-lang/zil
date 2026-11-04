//! Stage-level controls use actual immutable Source -> compiler call events.
//! The inventory proxy below isolates independent semantic rejection only; it
//! is never used to establish the Source-positive invocation authority.
use super::*;
use zil_verify::{CtArtifactAuthority, CtScopeParameterKind, CtSymbolUsage};

struct InventoryOnly<'a> {
    source: &'a FrozenAuthority<'a>,
}
impl InventoryOnly<'_> {
    fn prototype(&self, scope: &CtAmbientLifeScope) -> Option<&CtAmbientLifeScope> {
        self.source
            .expected_artifact
            .ambient_life_scopes
            .iter()
            .find(|original| {
                original.declaration == scope.declaration
                    && original.binder_site == scope.binder_site
            })
    }
}
impl CtArtifactAuthority for InventoryOnly<'_> {
    fn active_scope_count(&self) -> Option<usize> {
        self.source.active_scope_count()
    }
    fn scope_parameter_count(&self, scope: &CtAmbientLifeScope) -> Option<usize> {
        self.source.scope_parameter_count(self.prototype(scope)?)
    }
    fn scope_parameter_kind(
        &self,
        scope: &CtAmbientLifeScope,
        ordinal: usize,
    ) -> Option<CtScopeParameterKind> {
        self.source
            .scope_parameter_kind(self.prototype(scope)?, ordinal)
    }
    fn source_fingerprint(&self) -> Fingerprint {
        self.source.source_fingerprint()
    }
    fn snapshot(&self) -> &CtSnapshotRequest {
        self.source.snapshot()
    }
    fn authenticate_input_table(&self, input: &CtInputTable) -> bool {
        self.source.authenticate_input_table(input)
    }
    fn authenticate_artifact(&self, raw: &CtArtifactProgram) -> bool {
        self.source.authenticate_artifact(raw)
    }
    fn authenticate_function(&self, f: &CtFunction) -> bool {
        self.source.authenticate_function(f)
    }
    fn authorize_symbol(
        &self,
        requester: &CtSymbolId,
        symbol: &CtSymbolId,
        usage: CtSymbolUsage,
    ) -> bool {
        self.source.authorize_symbol(requester, symbol, usage)
    }
    fn authorize_query(&self, requester: &CtSymbolId, kind: CtQueryKind) -> bool {
        self.source.authorize_query(requester, kind)
    }
}

#[test]
fn g442_actual_source_instances_raw_wire_ledger_and_independent_checks() {
    let values = include_str!(
        "../../../../tests/language-readiness/terminal/344/anonymous/ct5-mixed-ct-instance-values.zil"
    );
    let usize_source = include_str!(
        "../../../../tests/language-readiness/terminal/344/anonymous/ct5-mixed-ct-factory.zil"
    )
    .replace("#N:i32", "#N:usize");
    for (text, definition_ordinal, domain) in [
        (values, 3, CtScalarTy::I32),
        (usize_source.as_str(), 2, CtScalarTy::Usize),
    ] {
        for width in [32, 64] {
            let sources = vec![SourceUnit::new(
                zil_base::FileId(0),
                ModuleId(0),
                "actual-instance.zil",
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
            let imports = vec![Vec::new()];
            let mut environment = Environment::new(
                &sources,
                &parsed,
                &paths,
                &imports,
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
            let original = compiler::source_scope::original_function(
                &parsed[0].module,
                definition.declaration,
                limits.max_call_depth,
                |_| Ok(()),
            )
            .unwrap()
            .unwrap();
            let zil_syntax::ast::FunctionBody::Block(block) = &original.function.body else {
                unreachable!()
            };
            let zil_syntax::ast::StmtKind::Binding { value, .. } = &block.statements[0].kind else {
                unreachable!()
            };
            let zil_syntax::ast::ExprKind::MetaEval(expression) = &value.kind else {
                unreachable!()
            };
            let entry = || Entry::Expression {
                module: ModuleId(0),
                expression,
                definition: definition.clone(),
                lexical_owner: Some(definition.clone()),
                expected: None,
                parameters: &[],
            };
            let input = environment.empty_input(expression.span).unwrap();
            let modules = environment.index.modules(&sources, &syntax);
            let (expected, facts) = compiler::compile_scoped(CompileRequest {
                target_pointer_width: width,
                modules: &modules,
                source_fingerprint: sources[0].fingerprint(),
                entry: entry(),
                input_tokens: &input,
                snapshot: &environment.snapshot,
                limits: &limits,
            })
            .unwrap();
            assert!(!facts.calls.is_empty());
            assert_eq!(
                facts.calls.len(),
                if domain == CtScalarTy::I32 { 2 } else { 1 }
            );
            let freeze = || {
                let mut budget = CtExecutionBudget::new(expected.budget);
                super::super::invocation_context::FrozenInvocationContext::freeze(
                    &entry(),
                    &parsed,
                    &environment.index,
                    Some(&origins),
                    &mut budget,
                )
                .unwrap()
                .unwrap()
            };
            let mut invocation = freeze();
            Arc::get_mut(&mut invocation)
                .unwrap()
                .extend_calls(
                    &facts.calls,
                    &facts.scopes,
                    &expected,
                    &environment.index,
                    &mut environment.budget,
                )
                .unwrap();
            assert_eq!(invocation.active_scope_count(), facts.calls.len() + 1);
            let authority = FrozenAuthority {
                source_fingerprint: sources[0].fingerprint(),
                snapshot: &environment.snapshot,
                input: &input,
                index: &environment.index,
                expected_functions: &expected.functions,
                expected_artifact: &expected,
                invocation_context: Some(&invocation),
            };
            let wire = |raw: &CtArtifactProgram| {
                zil_ir::ct_wire::decode_program(
                    &zil_ir::ct_wire::encode_program(raw, &limits).unwrap(),
                    &limits,
                )
                .unwrap()
            };
            let mut meter = zil_verify::CtVerificationMeter::new(expected.budget.max_analysis_work);
            zil_verify::verify_ct_artifact_metered(
                expected.clone(),
                &limits,
                &authority,
                &mut meter,
            )
            .unwrap();
            let exact = meter.spent();
            for raw in [expected.clone(), wire(&expected)] {
                for (work, good) in [(exact - 1, false), (exact, true), (exact, true)] {
                    let mut meter = zil_verify::CtVerificationMeter::new(work);
                    let result = zil_verify::verify_ct_artifact_metered(
                        raw.clone(),
                        &limits,
                        &authority,
                        &mut meter,
                    );
                    assert_eq!(
                        result.is_ok(),
                        good,
                        "{width}/{domain:?}/{work}: {result:?}"
                    );
                    if !good {
                        assert_eq!(result.unwrap_err()[0].code, "E-RESOURCE-LIMIT");
                    }
                }
            }
            // A row from the genuine expected artifact is insufficient without
            // its actual source-call events.
            let unbound = freeze();
            let unbound_authority = FrozenAuthority {
                invocation_context: Some(&unbound),
                ..authority
            };
            let errors =
                zil_verify::verify_ct_artifact(expected.clone(), &limits, &unbound_authority)
                    .unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.message.contains("actual active source binder")),
                "{errors:?}"
            );
            // Failed/forged private event ledgers cannot grant raw rows rights.
            for attack in 0..5 {
                let mut calls = facts.calls.clone();
                match attack {
                    0 => {
                        calls.push(calls[0].clone());
                    }
                    1 => {
                        calls.remove(0);
                    }
                    2 => calls[0].instance = calls[0].caller,
                    3 => calls[0].caller_scope.declaration.declaration = zil_base::NodeId(u32::MAX),
                    _ => calls[0].actuals[0] = CtScopeActual::Scalar(zil_ir::Constant::I32(0)),
                }
                let mut context = freeze();
                let mut budget = CtExecutionBudget::new(expected.budget);
                let result = Arc::get_mut(&mut context).unwrap().extend_calls(
                    &calls,
                    &facts.scopes,
                    &expected,
                    &environment.index,
                    &mut budget,
                );
                assert!(result.is_err(), "private ledger attack {attack}");
            }
            let instance = expected
                .ambient_life_scopes
                .iter()
                .position(|s| matches!(s.instance, CtScopeInstance::Instantiated(_)))
                .unwrap();
            let proxy = InventoryOnly { source: &authority };
            for attack in 0..7 {
                let mut raw = expected.clone();
                let CtScopeInstance::Instantiated(actuals) =
                    &mut raw.ambient_life_scopes[instance].instance
                else {
                    unreachable!()
                };
                let wanted = match attack {
                    0 => {
                        actuals.pop();
                        "full actual vector arity"
                    }
                    1 => {
                        actuals[0] = CtScopeActual::Scalar(zil_ir::Constant::I32(0));
                        "actual kind differs"
                    }
                    2 => {
                        actuals[1] = CtScopeActual::Scalar(zil_ir::Constant::Bool(true));
                        "scalar domain differs"
                    }
                    3 => {
                        actuals[2] = CtScopeActual::Lifetime(CtAmbientLifeReference {
                            scope: CtAmbientScopeId(instance as u32),
                            parameter: 0,
                        });
                        "actual lifetime ordinal"
                    }
                    4 => {
                        actuals[2] = CtScopeActual::Lifetime(CtAmbientLifeReference {
                            scope: CtAmbientScopeId(instance as u32),
                            parameter: 2,
                        });
                        "dependency cycle"
                    }
                    5 => {
                        let CtScopeActual::Type(graph) = &mut actuals[0] else {
                            unreachable!()
                        };
                        graph.root = CtTypeNodeId(u32::MAX);
                        "noncanonical root"
                    }
                    _ => {
                        let duplicate = raw.ambient_life_scopes[instance].clone();
                        raw.ambient_life_scopes.push(duplicate);
                        "scope is duplicated"
                    }
                };
                for candidate in [raw.clone(), wire(&raw)] {
                    let errors =
                        zil_verify::verify_ct_artifact(candidate, &limits, &proxy).unwrap_err();
                    assert!(
                        errors.iter().any(|d| d.message.contains(wanted)),
                        "independent {attack}/{width}/{domain:?}: {errors:?}"
                    );
                    assert!(
                        !errors
                            .iter()
                            .any(|d| d.message.contains("snapshot is not authenticated")),
                        "whole table equality masked {attack}"
                    );
                }
            }
            if width == 32 && domain == CtScalarTy::Usize {
                let mut raw = expected.clone();
                let CtScopeInstance::Instantiated(actuals) =
                    &mut raw.ambient_life_scopes[instance].instance
                else {
                    unreachable!()
                };
                actuals[1] =
                    CtScopeActual::Scalar(zil_ir::Constant::Usize(u64::from(u32::MAX) + 1));
                for candidate in [raw.clone(), wire(&raw)] {
                    let errors =
                        zil_verify::verify_ct_artifact(candidate, &limits, &proxy).unwrap_err();
                    assert!(
                        errors
                            .iter()
                            .any(|d| d.message.contains("usize exceeds target width")),
                        "{errors:?}"
                    );
                }
            }
            eprintln!(
                "G4.4.2 actual compiler ledger {width}/{domain:?}: raw+Wire verifier N={exact}/N-1; 5 private ledger attacks; 7 independent semantic attacks"
            );
        }
    }
}
