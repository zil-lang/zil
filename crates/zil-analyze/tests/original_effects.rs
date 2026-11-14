//! The complete original body is checked even without a selected instance.
//! These raw/Wire programs test the ABI theorem and its actual opcode bridge;
//! they are not receipts for a Source producer or for other intrinsic schemes.
use zil_analyze::analyze;
use zil_base::{Fingerprint, Limits, ModuleId, NodeId, Phase, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

fn definition(index: u32, name: &str, kind: CallKind) -> SemanticDefinition {
    SemanticDefinition {
        id: SemanticDefinitionId {
            module: ModuleId(0),
            index,
        },
        identity: LifeDeclarationIdentity {
            package: "effect-test".into(),
            module_path: vec!["original".into()],
            declaration_path: vec![name.into()],
            generation_path: vec![],
        },
        parent: None,
        kind: SemanticDefinitionKind::Function,
        parameters: vec![],
        implicit_lifetimes: vec![],
        requirements: vec![],
        environment: vec![],
        body: SemanticDefinitionBody::FunctionWithBody {
            signature: SemanticSignatureRecipe {
                bound_lifetimes: 0,
                invocation_lifetimes: 0,
                parameters: vec![],
                result: SemanticTypeRecipe::Atom(Type::Unit),
                result_mode: PassingMode::Infer,
                call_kind: kind,
            },
            parameters: vec![],
            receiver_parameter: None,
            names: vec![],
            sites: vec![],
            site_nodes: vec![],
            body: SemanticOriginalFunctionBody::Defined(CoreBlock {
                statements: vec![],
                span: Span::default(),
            }),
        },
    }
}
fn module(definitions: Vec<SemanticDefinition>) -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![],
        resources: vec![],
        closures: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        unsupported: vec![],
        semantic: Some(SemanticModule {
            definitions,
            instances: vec![],
            sites: vec![],
            dyn_scopes: vec![],
        }),
    }
}
fn call(definition: &mut SemanticDefinition, name: &str) {
    let SemanticDefinitionBody::FunctionWithBody {
        sites,
        site_nodes,
        body,
        ..
    } = &mut definition.body
    else {
        unreachable!()
    };
    sites.push(SemanticSiteRecipe {
        id: SemanticSiteId(0),
        occurrence_path: vec![0],
        kind: SemanticSiteRecipeKind::OriginalCall,
    });
    site_nodes.push(SemanticOriginalSiteNode {
        site: SemanticSiteId(0),
        node: SemanticOriginalNode::Expression(NodeId(2)),
    });
    *body = SemanticOriginalFunctionBody::Defined(CoreBlock {
        statements: vec![CoreStatement {
            id: NodeId(1),
            span: Span::default(),
            kind: CoreStatementKind::Expression(CoreExpr {
                id: NodeId(2),
                span: Span::default(),
                kind: CoreExprKind::Call {
                    callee: Box::new(CoreExpr {
                        id: NodeId(3),
                        span: Span::default(),
                        kind: CoreExprKind::Name(name.into()),
                    }),
                    arguments: vec![],
                },
            }),
        }],
        span: Span::default(),
    });
}
fn both(module: &TypedModule, expected: Option<&str>) {
    let limits = Limits::default();
    let bytes =
        wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits).unwrap();
    for raw in [
        module.clone(),
        wire::decode_module_candidate(&bytes, &limits).unwrap().0,
    ] {
        match expected {
            None => {
                analyze(&raw, &limits).unwrap();
            }
            Some(message) => {
                let errors = analyze(&raw, &limits).expect_err("original ABI relation must reject");
                assert!(
                    errors
                        .iter()
                        .any(|e| e.phase == Phase::Analyze && e.message.contains(message)),
                    "{errors:#?}"
                );
            }
        }
    }
}
#[test]
fn unused_fixed_suspend_empty_and_direct_yield_are_distinct() {
    both(
        &module(vec![definition(0, "worker", CallKind::Suspend)]),
        None,
    );
    let mut worker = definition(0, "worker", CallKind::Direct);
    call(&mut worker, "task_yield");
    both(
        &module(vec![worker.clone()]),
        Some("Direct body executes a Suspend"),
    );
    let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &mut worker.body else {
        unreachable!()
    };
    signature.call_kind = CallKind::Suspend;
    both(&module(vec![worker]), None);
}
#[test]
fn an_unselected_branch_and_defer_still_contribute_original_effects() {
    let mut worker = definition(0, "worker", CallKind::Direct);
    call(&mut worker, "task_yield");
    let SemanticDefinitionBody::FunctionWithBody { body, .. } = &mut worker.body else {
        unreachable!()
    };
    let SemanticOriginalFunctionBody::Defined(original) = body else {
        unreachable!()
    };
    let yield_body = original.clone();
    original.statements = vec![CoreStatement {
        id: NodeId(4),
        span: Span::default(),
        kind: CoreStatementKind::If {
            condition: CoreExpr {
                id: NodeId(5),
                span: Span::default(),
                kind: CoreExprKind::Bool(false),
            },
            then_body: yield_body,
            else_body: None,
        },
    }];
    both(
        &module(vec![worker.clone()]),
        Some("Direct body executes a Suspend"),
    );
    let SemanticDefinitionBody::FunctionWithBody {
        body: SemanticOriginalFunctionBody::Defined(original),
        ..
    } = &mut worker.body
    else {
        unreachable!()
    };
    let deferred = original.clone();
    original.statements = vec![CoreStatement {
        id: NodeId(6),
        span: Span::default(),
        kind: CoreStatementKind::Defer(deferred),
    }];
    both(
        &module(vec![worker]),
        Some("Direct body executes a Suspend"),
    );
}
#[test]
fn real_source_shadow_wins_and_cannot_disappear_from_names() {
    let target = definition(1, "task_yield", CallKind::Direct);
    let mut caller = definition(0, "caller", CallKind::Direct);
    call(&mut caller, "task_yield");
    let SemanticDefinitionBody::FunctionWithBody { names, .. } = &mut caller.body else {
        unreachable!()
    };
    names.push(SemanticOriginalNameBinding {
        path: vec!["task_yield".into()],
        namespace: SemanticOriginalNamespace::Value,
        value: SemanticOriginalNameValue::Definition(target.id),
    });
    both(&module(vec![caller.clone(), target.clone()]), None);
    let mut suspend = target.clone();
    let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &mut suspend.body else {
        unreachable!()
    };
    signature.call_kind = CallKind::Suspend;
    both(
        &module(vec![caller.clone(), suspend]),
        Some("Direct body executes a Suspend"),
    );
    let SemanticDefinitionBody::FunctionWithBody { names, .. } = &mut caller.body else {
        unreachable!()
    };
    names.clear();
    both(
        &module(vec![caller, target]),
        Some("hides an unbound real source declaration"),
    );
}
fn executed_yield() -> TypedModule {
    let mut original = definition(0, "worker", CallKind::Suspend);
    call(&mut original, "task_yield");
    let definition = original.id;
    let mut module = module(vec![original]);
    module.functions.push(TypedFunction {
        id: FunctionId(0),
        name: "worker".into(),
        parameters: vec![],
        return_type: Type::Unit,
        locals: vec![],
        values: vec![Type::Unit],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::TaskYield,
                span: Span::default(),
            }],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: Span::default(),
        result_mode: PassingMode::Infer,
        call_kind: CallKind::Suspend,
        resume_bound: TaskResumeBound::Local,
        foreign: None,
        environment: None,
        storage_life: None,
        dyn_instance: None,
        reference_return_bound: None,
        retained_return_sources: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contracts: vec![],
    });
    let instance = SemanticInstanceId {
        module: ModuleId(0),
        index: 0,
    };
    let program = module.semantic.as_mut().unwrap();
    program.instances.push(SemanticInstance {
        id: instance,
        definition,
        target: SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(FunctionId(0))),
        lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
        parent: None,
        actuals: vec![],
        header_embedding: vec![],
        environment: None,
        self_type: None,
    });
    program.sites.push(SemanticSite {
        instance,
        recipe: SemanticSiteId(0),
        at: SemanticSiteAddress::Typed {
            block: BlockId(0),
            instruction: 0,
            edge: SemanticSiteEdge::After,
        },
        action: SemanticSiteAction::BodyCall { actuals: vec![] },
    });
    module
}
#[test]
fn actual_yield_requires_its_exact_original_operation_and_one_site() {
    let original = executed_yield();
    both(&original, None);
    let mut changed = original.clone();
    changed.functions[0].blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::Unit);
    both(
        &changed,
        Some("original body call has no actual call opcode"),
    );
    let mut missing = original;
    missing.semantic.as_mut().unwrap().sites.clear();
    both(
        &missing,
        Some("original emission recipe has no actual operation"),
    );
}
