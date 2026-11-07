// VM-only controls over independently verified raw CFG; this is not Source evidence.
use super::*;
struct TraceReader;
impl CtReadView for TraceReader {
    fn authenticate_argument(&self, _: u32, _: &CtTy, _: &CtValue) -> CompileResult<()> {
        Ok(())
    }
    fn type_name(
        &self,
        _: &CtSymbolId,
        _: &CtSymbolId,
        _: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef> {
        Err(error("unused test reader method"))
    }
    fn type_apply(
        &self,
        _: &CtSymbolId,
        _: &CtSymbolId,
        _: &[CtValue],
        _: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef> {
        Err(error("unused test reader method"))
    }
    fn definition_ident(
        &self,
        _: &CtSymbolId,
        _: &CtSymbolId,
        _: &mut CtExecutionBudget,
    ) -> CompileResult<CtIdent> {
        Err(error("unused test reader method"))
    }
    fn parse(
        &self,
        _: CtSyntaxCategory,
        _: &CtTokenTree,
        _: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        Err(error("unused test reader method"))
    }
    fn parse_ident(&self, _: &CtTokenTree, _: &mut CtExecutionBudget) -> CompileResult<CtIdent> {
        Err(error("unused test reader method"))
    }
    fn query(
        &self,
        _: &CtSymbolId,
        kind: CtQueryKind,
        _: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(CtValue, CtQueryDependency)> {
        budget.reserve_analysis_work(1)?;
        budget.reserve_temporary_bytes(std::mem::size_of::<CtShapeInfo>() as u64)?;
        Ok((
            CtValue::ShapeInfo(CtShapeInfo {
                nominal: symbol(),
                visible_fields: Vec::new(),
                complete: true,
            }),
            CtQueryDependency {
                kind,
                subject_fingerprint: Fingerprint::of(b"actual trace query subject"),
                result_fingerprint: Fingerprint::of(b"actual trace query result"),
            },
        ))
    }
    fn build_syntax(
        &self,
        _: CtSyntaxBuilder,
        _: &[CtValue],
        _: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        Err(error("unused test reader method"))
    }
    fn instantiate_quote(
        &self,
        _: &CtTemplate,
        _: &[CtValue],
        _: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        Err(error("unused test reader method"))
    }
    fn token_inspect(
        &self,
        _: &CtValue,
        _: CtTokenProjection,
        _: Option<i64>,
        _: &mut CtExecutionBudget,
    ) -> CompileResult<CtValue> {
        Err(error("unused test reader method"))
    }
}

fn program() -> CtArtifactProgram {
    let mut p = raw();
    p.budget.max_temporary_bytes = 1_000_000;
    p.budget.max_analysis_work = 1_000_000;
    p.snapshot.ready = CtReadyStage::Shape;
    p.snapshot.requested_queries = vec![CtQueryKind::Shape];
    let span = Span::default();
    let mut child = p.functions[0].clone();
    child.id = CtFunctionId(1);
    child.definition.declaration = NodeId(2);
    child.result_type = CtTy::Scalar(CtScalarTy::Unit);
    child.values.clear();
    child.blocks = vec![
        CtBlock {
            id: CtBlockId(0),
            instructions: Vec::new(),
            terminator: CtTerminator::Jump(CtBlockId(1)),
        },
        CtBlock {
            id: CtBlockId(1),
            instructions: Vec::new(),
            terminator: CtTerminator::Return(None),
        },
    ];
    let f = &mut p.functions[0];
    f.result_type = CtTy::Scalar(CtScalarTy::Unit);
    f.values = vec![CtTy::Type, CtTy::ShapeInfo];
    f.blocks = vec![CtBlock {
        id: CtBlockId(0),
        instructions: vec![
            CtInstruction {
                result: Some(CtValueId(0)),
                kind: CtInstructionKind::TypePrimitive(CtScalarTy::I32),
                span,
            },
            CtInstruction {
                result: Some(CtValueId(1)),
                kind: CtInstructionKind::Query {
                    kind: CtQueryKind::Shape,
                    arguments: vec![CtValueId(0)],
                },
                span,
            },
        ],
        terminator: CtTerminator::Return(None),
    }];
    for _ in 0..8 {
        f.blocks[0].instructions.push(CtInstruction {
            result: None,
            kind: CtInstructionKind::Call {
                function: CtFunctionId(1),
                arguments: Vec::new(),
            },
            span,
        });
    }
    p.functions.push(child);
    p
}
#[test]
fn completed_unit_calls_and_query_receipt_keep_exact_byte_neighbors() {
    let p = program();
    let authority = Authority {
        snapshot: p.snapshot.clone(),
    };
    let checked = verify_ct_artifact(p.clone(), &Limits::default(), &authority).unwrap();
    let mut budget = CtExecutionBudget::new(p.budget);
    let (value, usage, calls) =
        evaluate_ct_artifact_with_completed_calls(&checked, &[], &TraceReader, &mut budget)
            .unwrap();
    assert_eq!(value, CtValue::Scalar(Constant::Unit));
    assert_eq!(usage.queries.len(), 1);
    assert_eq!(calls.len(), 8);
    for (i, call) in calls.iter().enumerate() {
        assert_eq!(
            *call,
            CtCompletedCall {
                caller: CtFunctionId(0),
                block: CtBlockId(0),
                instruction: i as u32 + 2,
                callee: CtFunctionId(1),
                result: None
            }
        );
    }
    let trace_bytes = calls.capacity() as u64 * std::mem::size_of::<CtCompletedCall>() as u64;
    assert!(budget.live_temporary_bytes() >= trace_bytes);
    let (_, old_usage) = evaluate_ct_artifact(
        &checked,
        &[],
        &TraceReader,
        &mut CtExecutionBudget::new(p.budget),
    )
    .unwrap();
    assert_eq!(usage.steps, old_usage.steps);
    assert!(
        usage.analysis_work > old_usage.analysis_work,
        "only the observing API pays for actual trace collection"
    );
    let peak = usage.peak_temporary_bytes;
    let exact = CtDeclaredBudget {
        max_temporary_bytes: peak,
        ..p.budget
    };
    for bytes in [peak, peak - 1, peak] {
        let result = evaluate_ct_artifact_with_completed_calls(
            &checked,
            &[],
            &TraceReader,
            &mut CtExecutionBudget::new(CtDeclaredBudget {
                max_temporary_bytes: bytes,
                ..p.budget
            }),
        );
        if bytes == peak {
            let (_, receipt, events) = result.unwrap();
            assert_eq!(receipt.peak_temporary_bytes, peak);
            assert_eq!(events, calls);
        } else {
            assert_eq!(result.unwrap_err()[0].code, "E-RESOURCE-LIMIT");
        }
    }
    let mut failed = p.clone();
    failed.functions[0].blocks[0].terminator = CtTerminator::Trap;
    let checked = verify_ct_artifact(failed, &Limits::default(), &authority).unwrap();
    let mut failed_budget = CtExecutionBudget::new(exact);
    assert_eq!(
        evaluate_ct_artifact_with_completed_calls(&checked, &[], &TraceReader, &mut failed_budget)
            .unwrap_err()[0]
            .code,
        "E-CT-ARTIFACT"
    );
    let query_bytes = failed_budget.usage().queries.capacity() as u64
        * std::mem::size_of::<CtQueryDependency>() as u64;
    assert_eq!(
        failed_budget.live_temporary_bytes(),
        query_bytes,
        "failed invocation publishes no pending/completed call storage"
    );
}

#[test]
fn unit_no_result_preserves_call_checks_in_raw_and_wire() {
    struct ClosedAuthority(CtSnapshotRequest);
    impl CtArtifactAuthority for ClosedAuthority {
        fn source_fingerprint(&self) -> Fingerprint {
            Fingerprint::ZERO
        }
        fn snapshot(&self) -> &CtSnapshotRequest {
            &self.0
        }
        fn authenticate_input_table(&self, input: &CtInputTable) -> bool {
            input.tokens.is_empty()
        }
        fn authenticate_artifact(&self, _: &CtArtifactProgram) -> bool {
            true
        }
        fn authenticate_function(&self, _: &CtFunction) -> bool {
            true
        }
        fn authorize_symbol(&self, _: &CtSymbolId, target: &CtSymbolId, _: CtSymbolUsage) -> bool {
            target.module == ModuleId(0) && matches!(target.declaration, NodeId(1) | NodeId(2))
        }
        fn authorize_query(&self, _: &CtSymbolId, kind: CtQueryKind) -> bool {
            kind == CtQueryKind::Shape
        }
    }
    let limits = Limits::default();
    let positive = program();
    assert!(
        verify_ct_artifact(
            positive.clone(),
            &limits,
            &ClosedAuthority(positive.snapshot.clone())
        )
        .is_ok()
    );
    for attack in 0..6 {
        let mut p = positive.clone();
        match attack {
            0 => {
                // A real non-Unit return still requires an SSA result.
                let child = &mut p.functions[1];
                child.result_type = CtTy::Scalar(CtScalarTy::I32);
                child.values = vec![child.result_type.clone()];
                child.blocks[1].instructions = vec![CtInstruction {
                    result: Some(CtValueId(0)),
                    kind: CtInstructionKind::Constant(CtConstantId(0)),
                    span: Span::default(),
                }];
                child.blocks[1].terminator = CtTerminator::Return(Some(CtValueId(0)));
            }
            1 => {
                p.functions[0].blocks[0].instructions[2].kind = CtInstructionKind::Call {
                    function: CtFunctionId(99),
                    arguments: Vec::new(),
                };
            }
            2 => {
                p.functions[1].definition.declaration = NodeId(3);
            }
            3 | 4 => {
                let child = &mut p.functions[1];
                child.parameters = vec![CtLocalId(0)];
                child.parameter_types = vec![CtTy::Scalar(CtScalarTy::I32)];
                child.locals = child.parameter_types.clone();
                if attack == 4 {
                    // The actual operand is Type, not the callee's i32.
                    p.functions[0].blocks[0].instructions[2].kind = CtInstructionKind::Call {
                        function: CtFunctionId(1),
                        arguments: vec![CtValueId(0)],
                    };
                }
            }
            5 => {
                // The exception does not apply to arbitrary Unit instructions.
                let id = CtConstantId(p.constants.len() as u32);
                p.constants.push(CtConstant::Scalar(Constant::Unit));
                p.functions[0].blocks[0].instructions.push(CtInstruction {
                    result: None,
                    kind: CtInstructionKind::Constant(id),
                    span: Span::default(),
                });
            }
            _ => unreachable!(),
        }
        let wire = zil_ir::ct_wire::encode_program(&p, &limits).unwrap();
        let decoded = zil_ir::ct_wire::decode_program(&wire, &limits).unwrap();
        for raw in [p, decoded] {
            let snapshot = raw.snapshot.clone();
            let errors = verify_ct_artifact(raw, &limits, &ClosedAuthority(snapshot)).unwrap_err();
            assert!(
                errors.iter().all(|e| e.code != "E-RESOURCE-LIMIT"),
                "attack {attack}: {errors:?}"
            );
        }
    }
}
