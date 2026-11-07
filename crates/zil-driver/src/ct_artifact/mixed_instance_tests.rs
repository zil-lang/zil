//! Stage-level controls use actual immutable Source -> compiler call events.
//! The inventory proxy below isolates independent semantic rejection only; it
//! is never used to establish the Source-positive invocation authority.
use super::*;
use zil_verify::{CtArtifactAuthority, CtScopeParameterKind, CtSymbolUsage};

#[test]
fn contextual_record_relocation_actual_source_record_relocation_has_independent_call_and_life_proofs()
 {
    for (text, separated) in [
        (
            include_str!(
                "../../../../tests/language-readiness/terminal/constraint-budgets/anonymous/host-identity-review/good-three-layer-reordered.zil"
            ),
            false,
        ),
        (
            include_str!(
                "../../../../tests/language-readiness/terminal/constraint-budgets/anonymous/ct5-cross-factory-lifetime-separation.zil"
            ),
            true,
        ),
    ] {
        let sources = vec![SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "actual-return.zil",
            text,
        )];
        let parsed = vec![Arc::new(zil_syntax::parse(&sources[0]))];
        assert!(parsed[0].diagnostics.is_empty());
        let syntax = vec![&parsed[0].module];
        let paths = BTreeMap::new();
        let imports = vec![Vec::new()];
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
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
            declaration: zil_base::NodeId(4),
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
        let input = environment.empty_input(expression.span).unwrap();
        let modules = environment.index.modules(&sources, &syntax);
        let (expected, facts) = compiler::compile_scoped(CompileRequest {
            target_pointer_width: 64,
            modules: &modules,
            source_fingerprint: sources[0].fingerprint(),
            entry: Entry::Expression {
                module: ModuleId(0),
                expression,
                definition: definition.clone(),
                lexical_owner: Some(definition.clone()),
                expected: None,
                parameters: &[],
            },
            input_tokens: &input,
            snapshot: &environment.snapshot,
            limits: &limits,
        })
        .unwrap();
        assert_eq!(expected.record_schemas.len(), 3);
        assert_eq!(facts.calls.len(), 3);
        assert_eq!(facts.bindings.len(), 3);
        assert_eq!(facts.relocations.len(), 2);
        assert_eq!(facts.scopes.len(), 4);
        assert_ne!(facts.bindings[0].definition, facts.bindings[1].definition);
        assert_eq!(
            facts
                .bindings
                .iter()
                .map(|row| row.parameters.clone())
                .collect::<Vec<_>>(),
            [vec![1], vec![0], vec![2]]
        );
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
        let entry = || Entry::Expression {
            module: ModuleId(0),
            expression,
            definition: definition.clone(),
            lexical_owner: Some(definition.clone()),
            expected: None,
            parameters: &[],
        };
        let mut invocation = super::super::invocation_context::FrozenInvocationContext::freeze(
            &entry(),
            &parsed,
            &environment.index,
            Some(&origins),
            &mut CtExecutionBudget::new(expected.budget),
        )
        .unwrap()
        .unwrap();
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
        Arc::get_mut(&mut invocation)
            .unwrap()
            .validate_type_contexts(
                &facts,
                &expected,
                &environment.index,
                &mut environment.budget,
            )
            .unwrap();
        invocation
            .validate_record_relocations(
                &facts.relocations,
                &facts.calls,
                &expected,
                &mut environment.budget,
            )
            .unwrap();
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
        for raw in [expected.clone(), wire(&expected)] {
            invocation
                .validate_record_relocations(
                    &facts.relocations,
                    &facts.calls,
                    &raw,
                    &mut CtExecutionBudget::new(expected.budget),
                )
                .unwrap();
            let verified = zil_verify::verify_ct_artifact(raw, &limits, &authority).unwrap();
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
                shape_failure: environment.shape_failure.as_deref(),
                input: None,
                limits: &limits,
                target: &target,
                snapshot: &environment.snapshot,
            };
            let (value, _) = zil_ct::evaluate_ct_artifact(
                &verified,
                &[],
                &reader,
                &mut CtExecutionBudget::new(expected.budget),
            )
            .unwrap();
            assert!(matches!(value, CtValue::Record { fields, .. }
            if fields == [CtValue::Scalar(zil_ir::Constant::I32(42))]));
        }
        let context = value_context::ValueContext::capture(
            &expected,
            &facts.bindings,
            Some(invocation.clone()),
            &mut CtExecutionBudget::new(expected.budget),
        )
        .unwrap();
        let (_, sources) = context
            .type_with_scopes(&context.ty, &mut CtExecutionBudget::new(expected.budget))
            .unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].definition, definition);
        assert_eq!(sources[0].parameters, [0]);
        for attack in 0..5 {
            let mut rows = facts.relocations.clone();
            match attack {
                0 => rows[0].input = rows[0].output,
                1 => rows[0].target = rows[0].source,
                2 => rows[0].caller = CtFunctionId(0),
                3 => rows[0].output = rows[0].input,
                _ => rows.push(rows[0]),
            }
            assert!(
                invocation
                    .validate_record_relocations(
                        &rows,
                        &facts.calls,
                        &expected,
                        &mut CtExecutionBudget::new(expected.budget)
                    )
                    .is_err(),
                "private relocation attack {attack}"
            );
        }
        for attack in 0..3 {
            let mut raw = expected.clone();
            let row = facts.relocations[0];
            let function = &mut raw.functions[row.caller.0 as usize];
            let block = function
                .blocks
                .iter_mut()
                .find(|block| block.id == row.block)
                .unwrap();
            let at = block
                .instructions
                .iter()
                .position(|instruction| instruction.result == Some(row.input))
                .unwrap();
            match attack {
                0 => {
                    let CtInstructionKind::ExtractField { index, .. } =
                        &mut block.instructions[at + 1].kind
                    else {
                        unreachable!()
                    };
                    *index = 1;
                }
                1 => {
                    let CtInstructionKind::MakeRecord { fields, .. } =
                        &mut block.instructions[at + 2].kind
                    else {
                        unreachable!()
                    };
                    fields.clear();
                }
                _ => {
                    let CtInstructionKind::MakeRecord { fields, .. } =
                        &mut block.instructions[at + 2].kind
                    else {
                        unreachable!()
                    };
                    fields[0] = row.input;
                }
            }
            for raw in [raw.clone(), wire(&raw)] {
                assert!(
                    invocation
                        .validate_record_relocations(
                            &facts.relocations,
                            &facts.calls,
                            &raw,
                            &mut CtExecutionBudget::new(expected.budget)
                        )
                        .is_err(),
                    "raw relocation attack {attack}"
                );
            }
        }
        if separated {
            let row = facts.relocations[0];
            for target in [false, true] {
                let mut raw = expected.clone();
                let schema = if target { row.target } else { row.source };
                raw.record_schemas[schema.0 as usize]
                    .life_context
                    .as_mut()
                    .unwrap()
                    .ambient_classes[0]
                    .source
                    .parameter = if target { 2 } else { 1 };
                // Another genuine Life ordinal in the same authentic caller or
                // callee instance, without changing any full type/value argument.
                for raw in [raw.clone(), wire(&raw)] {
                    let errors = invocation
                        .validate_record_relocations(
                            &facts.relocations,
                            &facts.calls,
                            &raw,
                            &mut CtExecutionBudget::new(expected.budget),
                        )
                        .unwrap_err();
                    assert!(
                        errors.iter().any(|d| d.phase == Phase::Ct
                            && d.code == "E-UNSUPPORTED"
                            && d.message == "CT relocation changes the original source lifetime endpoint/full ordinal"),
                        "changed target={target} lifetime endpoint: {errors:?}"
                    );
                }
            }
        }
        for type_actual in [false, true] {
            let mut calls = facts.calls.clone();
            if type_actual {
                let CtScopeActual::Type(graph) =
                    &mut calls[1].actuals[if separated { 3 } else { 2 }]
                else {
                    unreachable!()
                };
                graph.nodes[graph.root.0 as usize] = CtTypeNode::Primitive(CtScalarTy::Bool);
            } else {
                calls[1].actuals[if separated { 2 } else { 1 }] =
                    CtScopeActual::Scalar(zil_ir::Constant::I32(43));
            }
            let errors = invocation
                .validate_record_relocations(
                    &facts.relocations,
                    &calls,
                    &expected,
                    &mut CtExecutionBudget::new(expected.budget),
                )
                .unwrap_err();
            assert!(
                errors.iter().any(|d| d
                    .message
                    .contains("source full instance differs from actual call")),
                "{errors:?}"
            );
        }

        for attack in 0..2 {
            let mut raw = expected.clone();
            let row = facts.relocations[0];
            let schema = &mut raw.record_schemas[row.target.0 as usize];
            if attack == 0 {
                schema.arguments[2] = CtSchemaArgument::Scalar(zil_ir::Constant::I32(43));
            } else {
                schema.life_context.as_mut().unwrap().nodes[0] =
                    CtTypeNode::Primitive(CtScalarTy::Bool);
            }
            for raw in [raw.clone(), wire(&raw)] {
                let errors = invocation
                    .validate_record_relocations(
                        &facts.relocations,
                        &facts.calls,
                        &raw,
                        &mut CtExecutionBudget::new(expected.budget),
                    )
                    .unwrap_err();
                let expected_message = if attack == 0 {
                    "CT relocation changes the complete ordered nominal actual vector"
                } else {
                    "CT relocation changes the complete nominal context graph"
                };
                assert!(
                    errors.iter().any(|d| d.phase == Phase::Ct
                        && d.code == "E-UNSUPPORTED"
                        && d.message == expected_message),
                    "attack {attack}: {errors:?}"
                );
            }
        }
    }
}

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
fn mixed_compile_time_actual_actual_source_instances_raw_wire_ledger_and_independent_checks() {
    let values = include_str!(
        "../../../../tests/language-readiness/terminal/constraint-budgets/anonymous/ct5-mixed-ct-instance-values.zil"
    );
    let usize_source = include_str!(
        "../../../../tests/language-readiness/terminal/constraint-budgets/anonymous/ct5-mixed-ct-factory.zil"
    )
    .replace("#N:i32", "#N:usize");
    // The returned scalar ignores every generic actual. Vary each slot alone
    // and repeat the first call to distinguish identity from occurrence count.
    let unused = "struct Anchor<life a>{n:i32}fn choose<T:Copy,#N:i32,life a>(marker:Type)->i32{return 42}fn result<life s,life t>(p:&'s i32,q:&'t i32)->i32{let value=#((choose::<i32,7,'s>(i32),choose::<bool,7,'s>(i32),choose::<i32,35,'s>(i32),choose::<i32,7,'t>(i32),choose::<i32,7,'s>(i32)));return value.0}fn main()->i32{let n=0;let m=0;return result(&n,&m)}";
    for (text, definition_ordinal, domain, unused_actuals) in [
        (values, 3, CtScalarTy::I32, false),
        (usize_source.as_str(), 2, CtScalarTy::Usize, false),
        (unused, 2, CtScalarTy::I32, true),
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
                if unused_actuals {
                    5
                } else if domain == CtScalarTy::I32 {
                    2
                } else {
                    1
                }
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
            assert_eq!(
                invocation.active_scope_count(),
                if unused_actuals {
                    5
                } else {
                    facts.calls.len() + 1
                }
            );
            if unused_actuals {
                assert_eq!(facts.calls[0].instance, facts.calls[4].instance);
                assert_eq!(facts.calls[0].actuals, facts.calls[4].actuals);
                for (changed, slot) in [(1, 0), (2, 1), (3, 2)] {
                    assert_eq!(facts.calls[0].callee, facts.calls[changed].callee);
                    assert_eq!(facts.calls[0].caller, facts.calls[changed].caller);
                    assert_ne!(facts.calls[0].instance, facts.calls[changed].instance);
                    for ordinal in 0..3 {
                        assert_eq!(
                            facts.calls[0].actuals[ordinal]
                                == facts.calls[changed].actuals[ordinal],
                            ordinal != slot,
                        );
                    }
                    for prior in 1..changed {
                        assert_ne!(facts.calls[prior].instance, facts.calls[changed].instance);
                    }
                }
            }
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
            if unused_actuals {
                assert!(expected.ambient_life_scopes.is_empty());
                eprintln!(
                    "mixed static member actuals actual Source {width}: each unused T/N/Life slot alone selects a distinct instance; repeated full vector reuses it; 5 authentic active scopes; raw/Wire exact N={exact}/N-1"
                );
                continue;
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
            if width == 64 && domain == CtScalarTy::I32 {
                // Semantic-only proxy control: first complete suffix 2->1->0,
                // then enter 4->3->2. Reusing completed 2 still has height 2.
                // Neither this forged DAG nor its nearest control is Source
                // authority; a valid inner check reaches snapshot rejection.
                for type_edge in [false, true] {
                    let mut dag = expected.clone();
                    let contextual = dag.record_schemas[0].life_context.as_ref().unwrap().clone();
                    dag.ambient_life_scopes.truncate(1);
                    for index in 1..=4 {
                        let mut row = expected.ambient_life_scopes[instance].clone();
                        let CtScopeInstance::Instantiated(actuals) = &mut row.instance else {
                            unreachable!()
                        };
                        actuals[1] = CtScopeActual::Scalar(zil_ir::Constant::I32(100 + index));
                        let dependency = CtAmbientLifeReference {
                            scope: CtAmbientScopeId((index - 1) as u32),
                            parameter: if index == 1 { 0 } else { 2 },
                        };
                        if type_edge {
                            let mut graph = contextual.clone();
                            for ambient in &mut graph.ambient_classes {
                                ambient.source = dependency;
                            }
                            actuals[0] = CtScopeActual::Type(graph);
                            actuals[2] = CtScopeActual::Lifetime(CtAmbientLifeReference {
                                scope: CtAmbientScopeId(0),
                                parameter: 0,
                            });
                        } else {
                            actuals[2] = CtScopeActual::Lifetime(dependency);
                        }
                        dag.ambient_life_scopes.push(row);
                    }
                    for schema in &mut dag.record_schemas {
                        if let Some(graph) = &mut schema.life_context {
                            for ambient in &mut graph.ambient_classes {
                                ambient.source.scope = CtAmbientScopeId(
                                    if ambient.source.scope.0 == instance as u32 {
                                        2
                                    } else {
                                        4
                                    },
                                );
                            }
                        }
                    }
                    for (depth, wanted) in [
                        (3, "dependency depth exceeded"),
                        (4, "snapshot is not authenticated"),
                    ] {
                        dag.budget.max_call_depth = depth;
                        for candidate in [dag.clone(), wire(&dag)] {
                            let errors = zil_verify::verify_ct_artifact(candidate, &limits, &proxy)
                                .unwrap_err();
                            assert!(
                                errors.iter().any(|d| d.message.contains(wanted)),
                                "shared DAG type_edge={type_edge}, depth={depth}: {errors:?}"
                            );
                        }
                    }
                }
            }
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
                "mixed static member actuals actual compiler ledger {width}/{domain:?}: raw+Wire verifier N={exact}/N-1; 5 private ledger attacks; 7 independent semantic attacks"
            );
        }
    }
}

#[test]
fn static_member_factory_original_static_members_keep_host_identity_and_actual_values() {
    let text = include_str!(
        "../../../../tests/language-readiness/terminal/constraint-budgets/anonymous/ct5-static-member-host-identity.zil"
    );
    let sources = vec![SourceUnit::new(
        zil_base::FileId(0),
        ModuleId(0),
        "static-members.zil",
        text,
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
    environment.bind_source_origins(origins.as_deref()).unwrap();
    environment.classify_definitions().unwrap();
    let definition = CtSymbolId {
        module: ModuleId(0),
        declaration: zil_base::NodeId(3),
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
    let mut identities = Vec::new();
    let mut actuals = Vec::new();
    for (position, number) in [(0, 7), (1, 35)] {
        let zil_syntax::ast::StmtKind::Binding { value, .. } = &block.statements[position].kind
        else {
            unreachable!()
        };
        let zil_syntax::ast::ExprKind::MetaEval(expression) = &value.kind else {
            unreachable!()
        };
        let input = environment.empty_input(expression.span).unwrap();
        // This executes actual Source attribution, independent CT Verify and
        // the VM before the separately blocked ordinary materialization path.
        let result = environment
            .evaluate_data(
                Entry::Expression {
                    module: ModuleId(0),
                    expression,
                    definition: definition.clone(),
                    lexical_owner: Some(definition.clone()),
                    expected: None,
                    parameters: &[],
                },
                &input,
                None,
                &[],
                expression.span,
            )
            .unwrap();
        assert!(matches!(&result.value, CtValue::Record { fields, .. }
            if matches!(fields.as_slice(), [CtValue::Scalar(zil_ir::Constant::I32(n))] if *n == number)));
        let context = result.context.unwrap();
        let instances: Vec<_> = context
            .ambient_life_scopes
            .iter()
            .filter(|scope| matches!(scope.instance, CtScopeInstance::Instantiated(_)))
            .collect();
        assert_eq!(instances.len(), 1);
        let CtScopeInstance::Instantiated(vector) = &instances[0].instance else {
            unreachable!()
        };
        assert!(matches!(
            vector.as_slice(),
            [
                CtScopeActual::Type(_),
                CtScopeActual::Scalar(zil_ir::Constant::I32(7)),
                CtScopeActual::Lifetime(_)
            ]
        ));
        actuals.push(vector.clone());
        identities.push(
            context
                .invocation_context
                .as_ref()
                .unwrap()
                .symbol_key(&instances[0].declaration, &mut environment.budget)
                .unwrap(),
        );
    }
    assert_eq!(actuals[0], actuals[1]);
    assert_ne!(
        identities[0], identities[1],
        "same member spelling/vector merged original hosts"
    );
}

#[test]
fn nested_static_member_nested_source_calls_keep_private_vectors_and_published_graph_dependencies()
{
    for (text, intermediate) in [
        (
            include_str!(
                "../../../../tests/language-readiness/terminal/constraint-budgets/anonymous/ct5-nested-mixed-scalar.zil"
            ),
            false,
        ),
        (
            include_str!(
                "../../../../tests/language-readiness/terminal/constraint-budgets/anonymous/ct5-nested-mixed-intermediate.zil"
            ),
            true,
        ),
    ] {
        let sources = vec![SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "nested.zil",
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
            declaration: zil_base::NodeId(if intermediate { 4 } else { 3 }),
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
            target_pointer_width: 64,
            modules: &modules,
            source_fingerprint: sources[0].fingerprint(),
            entry: entry(),
            input_tokens: &input,
            snapshot: &environment.snapshot,
            limits: &limits,
        })
        .unwrap();
        assert_eq!(facts.calls.len(), 3);
        assert_eq!(facts.scopes.len(), 3);
        assert_eq!(expected.ambient_life_scopes.is_empty(), !intermediate);
        for (index, call) in facts.calls.iter().enumerate() {
            assert_eq!(call.actuals.len(), 3);
            assert!(matches!(
                call.actuals[1],
                CtScopeActual::Scalar(zil_ir::Constant::I32(42))
            ));
            let CtScopeActual::Lifetime(reference) = call.actuals[2] else {
                unreachable!()
            };
            assert_eq!(reference.scope, call.caller_scope_id);
            assert_eq!(reference.parameter, if index == 0 { 0 } else { 2 });
            if index > 0 {
                assert_eq!(call.caller, facts.calls[index - 1].instance);
                assert!(matches!(
                    call.caller_scope.instance,
                    CtScopeInstance::Instantiated(_)
                ));
            }
        }
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
        assert_eq!(invocation.active_scope_count(), 4);
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
        zil_verify::verify_ct_artifact_metered(expected.clone(), &limits, &authority, &mut meter)
            .unwrap();
        let n = meter.spent();
        for raw in [expected.clone(), wire(&expected)] {
            for (budget, success) in [(n - 1, false), (n, true), (n, true)] {
                let result = zil_verify::verify_ct_artifact_metered(
                    raw.clone(),
                    &limits,
                    &authority,
                    &mut zil_verify::CtVerificationMeter::new(budget),
                );
                assert_eq!(
                    result.is_ok(),
                    success,
                    "intermediate={intermediate}: {result:?}"
                );
                if !success {
                    assert_eq!(result.unwrap_err()[0].code, "E-RESOURCE-LIMIT");
                }
            }
        }
        for attack in 0..3 {
            let mut calls = facts.calls.clone();
            match attack {
                0 => {
                    calls[2].actuals[2] = CtScopeActual::Lifetime(CtAmbientLifeReference {
                        scope: calls[0].caller_scope_id,
                        parameter: 0,
                    })
                }
                1 => {
                    calls[2].actuals[2] = CtScopeActual::Lifetime(CtAmbientLifeReference {
                        scope: calls[2].caller_scope_id,
                        parameter: 1,
                    })
                }
                _ => {
                    calls.remove(1);
                }
            }
            let mut context = freeze();
            let result = Arc::get_mut(&mut context).unwrap().extend_calls(
                &calls,
                &facts.scopes,
                &expected,
                &environment.index,
                &mut CtExecutionBudget::new(expected.budget),
            );
            assert!(result.is_err(), "nested private ledger attack {attack}");
        }
        if intermediate {
            // This proxy bypasses whole-artifact equality only. Independent
            // graph/scope admission must reject erased dependencies by itself.
            let proxy = InventoryOnly { source: &authority };
            let mut raw = expected.clone();
            raw.ambient_life_scopes.clear();
            for raw in [raw.clone(), wire(&raw)] {
                let errors = zil_verify::verify_ct_artifact(raw, &limits, &proxy).unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("outside inventory")),
                    "{errors:?}"
                );
            }
        }
        eprintln!(
            "nested static member actual 3-level source chain intermediate={intermediate}: private vectors retained; raw/Wire independent N={n}/N-1"
        );
    }
}
