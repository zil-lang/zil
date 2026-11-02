//! Public Source CT trust-boundary controls. Raw shapes remain untrusted even
//! when they are not CT-serializable or appear after the function being walked.
use std::cell::Cell;
use zil_base::{
    Diagnostic, FileId, Fingerprint, Limits, ModuleId, NodeId, Phase, ResultCategory,
    SCHEMA_VERSION, SourceUnit, Span, TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 10)
}
fn expression(kind: CoreExprKind) -> CoreExpr {
    CoreExpr {
        id: NodeId(2),
        kind,
        span: span(),
    }
}
fn callback(count: usize) -> Type {
    Type::ForeignCallback {
        parameters: vec![Type::I32; count],
        result: Box::new(Type::I32),
    }
}
fn function() -> CoreFunction {
    CoreFunction {
        callable_return: None,
        state_parameters: Vec::new(),
        type_parameters: Vec::new(),
        callable_parameters: Vec::new(),
        result_mode: PassingMode::Infer,
        foreign: None,
        id: NodeId(0),
        name: "f".into(),
        parameters: vec![],
        return_type: Type::Unit,
        receiver: None,
        span: span(),
        body: CoreBlock {
            span: span(),
            statements: vec![],
        },
    }
}
fn module() -> CoreModule {
    CoreModule {
        aggregate_templates: Vec::new(),
        aggregate_applications: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        functions: vec![function()],
        aggregates: vec![],
        enums: vec![],
        unsupported: vec![],
    }
}
fn parameter(ty: Type) -> CoreParameter {
    CoreParameter {
        name: "p".into(),
        ty,
        mode: PassingMode::Infer,
        span: span(),
    }
}
fn statement(kind: CoreStatementKind) -> CoreStatement {
    CoreStatement {
        id: NodeId(1),
        kind,
        span: span(),
    }
}
fn limited(core: &CoreModule, limits: &Limits) {
    let errors =
        zil_ct::evaluate_source_constants_with_usage(core, &TargetProfile::host_llvm(), limits)
            .unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::ResourceLimit,
        "{errors:?}"
    );
}

#[test]
fn public_headers_are_bounded_before_any_type_clone() {
    let limits = Limits {
        max_nodes: 12,
        ..Limits::default()
    };
    for position in 0..3 {
        let mut core = module();
        match position {
            0 => core.functions[0].parameters.push(parameter(callback(32))),
            1 => core.functions[0].return_type = callback(32),
            _ => {
                core.functions[0].callable_parameters = vec![CoreCallableParameter {
                    index: 0,
                    generic_index: 0,
                    generic_name: "F".into(),
                    kind: CallableKind::Fn,
                    parameters: vec![callback(32)],
                    result: Type::I32,
                }]
            }
        }
        limited(&core, &limits);
    }
}

#[test]
fn aggregate_and_enum_tables_do_not_bypass_public_preflight() {
    let limits = Limits {
        max_nodes: 12,
        ..Limits::default()
    };
    let mut core = module();
    core.aggregates.push(CoreAggregate {
        id: NodeId(3),
        name: "A".into(),
        kind: AggregateKind::Struct,
        teardown: None,
        fields: vec![CoreField {
            name: "cb".into(),
            ty: callback(32),
            public: false,
            manages: false,
            span: span(),
        }],
        span: span(),
    });
    limited(&core, &limits);
    core.aggregates.clear();
    core.enums.push(CoreEnum {
        id: NodeId(4),
        name: "E".into(),
        variants: vec![("V".into(), vec![callback(32)])],
        span: span(),
    });
    limited(&core, &limits);
}

#[test]
fn local_annotations_records_and_closures_bound_callback_trees_before_clone() {
    let limits = Limits {
        max_nodes: 16,
        ..Limits::default()
    };
    for position in 0..4 {
        let mut core = module();
        core.functions[0]
            .body
            .statements
            .push(statement(match position {
                0 => CoreStatementKind::Binding {
                    name: "cb".into(),
                    mutable: false,
                    ty: Some(callback(40)),
                    initializer: None,
                },
                1 => CoreStatementKind::Expression(expression(CoreExprKind::Record {
                    ty: callback(40),
                    fields: vec![],
                })),
                2 => CoreStatementKind::Expression(expression(CoreExprKind::RecordUpdate {
                    ty: Some(callback(40)),
                    base: Box::new(expression(CoreExprKind::Unit)),
                    fields: vec![],
                })),
                _ => CoreStatementKind::Expression(expression(CoreExprKind::Closure {
                    move_capture: false,
                    parameters: vec![CoreClosureParameter {
                        name: "cb".into(),
                        ty: Some(callback(40)),
                        mode: PassingMode::Infer,
                        span: span(),
                    }],
                    return_type: None,
                    body: CoreBlock {
                        span: span(),
                        statements: vec![],
                    },
                })),
            }));
        limited(&core, &limits);
    }
}

#[test]
fn raw_recursive_callback_depth_and_text_copy_work_are_bounded() {
    let mut core = module();
    let mut ty = Type::I32;
    for _ in 0..8 {
        ty = Type::ForeignCallback {
            parameters: vec![ty],
            result: Box::new(Type::I32),
        };
    }
    core.functions[0].parameters.push(parameter(ty));
    limited(
        &core,
        &Limits {
            max_call_depth: 3,
            ..Limits::default()
        },
    );
    let mut names = module();
    names.functions[0].name = "x".repeat(2048);
    limited(
        &names,
        &Limits {
            max_analysis_steps: 32,
            ..Limits::default()
        },
    );
}

struct UnexpectedCompiler(Cell<usize>);
impl zil_ct::SourceCompiler for UnexpectedCompiler {
    fn compile(
        &self,
        _: &CoreExpr,
        _: Option<&Type>,
        _: &Limits,
    ) -> zil_base::CompileResult<CtProgram> {
        self.0.set(self.0.get() + 1);
        Err(vec![Diagnostic::error(
            Phase::Ct,
            "E-UNEXPECTED-COMPILER",
            None,
            "header preflight must precede this query",
        )])
    }
}

#[test]
fn later_headers_are_preflighted_before_an_earlier_ct_query() {
    let mut core = module();
    core.functions[0]
        .body
        .statements
        .push(statement(CoreStatementKind::Expression(expression(
            CoreExprKind::CompileTime(Box::new(expression(CoreExprKind::Call {
                callee: Box::new(expression(CoreExprKind::Name("later".into()))),
                arguments: vec![],
            }))),
        ))));
    let mut later = function();
    later.name = "later".into();
    later.id = NodeId(3);
    later.parameters.push(parameter(callback(32)));
    core.functions.push(later);
    let compiler = UnexpectedCompiler(Cell::new(0));
    let errors = zil_ct::evaluate_source_constants_with_compiler(
        &core,
        &TargetProfile::host_llvm(),
        &Limits {
            max_nodes: 16,
            ..Limits::default()
        },
        None,
        &compiler,
    )
    .unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::ResourceLimit,
        "{errors:?}"
    );
    assert_eq!(compiler.0.get(), 0);
}

fn source_core(text: &str) -> CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "impl088.zil", text);
    let parsed = zil_syntax::parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    zil_expand::expand(&source, &parsed.module).unwrap()
}

#[test]
fn real_source_header_limits_do_not_depend_on_declaration_order() {
    let params = (0..24)
        .map(|i| format!("p{i}: i32"))
        .collect::<Vec<_>>()
        .join(", ");
    let signature = vec!["i32"; 24].join(", ");
    let first = "fn first() { accepts(c_callback(handler)) }";
    let handler = format!("fn handler({params}) -> i32 {{ return 1 }}");
    let accepts = format!("fn accepts(cb: extern \"C\" fn({signature}) -> i32) {{ }}");
    for text in [
        format!("{first}\n{handler}\n{accepts}"),
        format!("{accepts}\n{handler}\n{first}"),
    ] {
        let core = source_core(&text);
        limited(
            &core,
            &Limits {
                max_nodes: 20,
                ..Limits::default()
            },
        );
    }
}

#[test]
fn unusable_context_callback_headers_are_still_structurally_bounded() {
    let mut core = module();
    core.functions[0].parameters.push(parameter(callback(32)));
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: vec![ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    assert!(
        matches!(zil_ct::SourceProgramContext::new(&program, &Limits { max_analysis_steps: 12, ..Limits::default() }), Err(errors) if errors[0].category == ResultCategory::ResourceLimit)
    );
}

#[test]
fn source_node_and_depth_limits_accept_exact_boundaries_and_accumulate() {
    let target = TargetProfile::host_llvm();
    let mut core = module();
    core.functions[0].parameters.push(parameter(callback(2)));
    core.functions[0]
        .body
        .statements
        .push(statement(CoreStatementKind::Expression(expression(
            CoreExprKind::Literal(Constant::WideString(vec![65, 0xd83d, 0xde00, 0])),
        ))));
    let (output, usage) =
        zil_ct::evaluate_source_constants_with_usage(&core, &target, &Limits::default()).unwrap();
    assert_eq!(output, core);
    let exact = Limits {
        max_nodes: usage.nodes,
        ..Limits::default()
    };
    assert!(zil_ct::evaluate_source_constants(&core, &target, &exact).is_ok());
    limited(
        &core,
        &Limits {
            max_nodes: usage.nodes - 1,
            ..Limits::default()
        },
    );
    let mut twice = core.clone();
    let mut second = core.functions[0].clone();
    second.name = "second".into();
    second.id = NodeId(9);
    twice.functions.push(second);
    limited(&twice, &exact);

    let mut nested = Type::I32;
    for _ in 0..3 {
        nested = Type::ForeignCallback {
            parameters: vec![nested],
            result: Box::new(Type::I32),
        };
    }
    let mut raw = module();
    raw.functions[0].parameters.push(parameter(nested));
    assert!(
        zil_ct::evaluate_source_constants(
            &raw,
            &target,
            &Limits {
                max_call_depth: 3,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    limited(
        &raw,
        &Limits {
            max_call_depth: 2,
            ..Limits::default()
        },
    );
    println!("impl088 source callback+UTF16 exact nodes: {}", usage.nodes);
}

#[test]
fn metadata_work_is_cumulative_and_exact_boundary_is_not_off_by_one() {
    let target = TargetProfile::host_llvm();
    let mut core = module();
    core.functions[0].name = "f".repeat(128);
    let mut lower = 0;
    let mut upper = 1024;
    while lower < upper {
        let middle = (lower + upper) / 2;
        match zil_ct::evaluate_source_constants(
            &core,
            &target,
            &Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            },
        ) {
            Ok(_) => upper = middle,
            Err(errors) => {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                lower = middle + 1;
            }
        }
    }
    let exact = Limits {
        max_analysis_steps: upper,
        ..Limits::default()
    };
    assert!(zil_ct::evaluate_source_constants(&core, &target, &exact).is_ok());
    limited(
        &core,
        &Limits {
            max_analysis_steps: upper - 1,
            ..Limits::default()
        },
    );
    let mut twice = core.clone();
    twice.functions.push(core.functions[0].clone());
    limited(&twice, &exact);
    println!("impl088 source text exact analysis work: {upper}");
}

#[test]
fn hint_lookahead_does_not_clone_unvisited_ascription_trees() {
    for range in [false, true] {
        let ascribed = expression(CoreExprKind::Ascribed {
            ty: callback(128),
            value: Box::new(expression(CoreExprKind::Unit)),
        });
        let mut core = module();
        core.functions[0].body.statements.push(statement(if range {
            CoreStatementKind::ForRange {
                name: "n".into(),
                start: ascribed,
                end: expression(CoreExprKind::Integer("1".into())),
                body: CoreBlock {
                    span: span(),
                    statements: vec![],
                },
            }
        } else {
            CoreStatementKind::Expression(expression(CoreExprKind::Binary {
                op: BinaryOp::Add,
                left: Box::new(ascribed),
                right: Box::new(expression(CoreExprKind::Integer("1".into()))),
            }))
        }));
        limited(
            &core,
            &Limits {
                max_nodes: 32,
                ..Limits::default()
            },
        );
    }
}

#[test]
fn foreign_metadata_and_untyped_closure_parameters_cannot_hide_unbounded_tables() {
    let mut core = module();
    core.functions[0].foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "f".into(),
        noescape_parameters: vec![0; 128],
        static_callbacks: true,
    });
    limited(
        &core,
        &Limits {
            max_nodes: 32,
            ..Limits::default()
        },
    );
    core.functions[0].foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "f".repeat(128),
        noescape_parameters: vec![],
        static_callbacks: true,
    });
    limited(
        &core,
        &Limits {
            max_analysis_steps: 32,
            ..Limits::default()
        },
    );
    core.functions[0].foreign = None;
    core.functions[0]
        .body
        .statements
        .push(statement(CoreStatementKind::Expression(expression(
            CoreExprKind::Closure {
                move_capture: false,
                parameters: vec![
                    CoreClosureParameter {
                        name: String::new(),
                        ty: None,
                        mode: PassingMode::Infer,
                        span: span()
                    };
                    128
                ],
                return_type: None,
                body: CoreBlock {
                    span: span(),
                    statements: vec![],
                },
            },
        ))));
    limited(
        &core,
        &Limits {
            max_nodes: 32,
            ..Limits::default()
        },
    );
}

#[test]
fn real_source_ct_helpers_keep_static_foreign_values_forbidden_in_both_orders() {
    let query = "fn first()->i32 { return #(helper()) }";
    let long_text = "x".repeat(4096);
    for body in [
        "let h=c_null(); return 1".to_owned(),
        "let w=c_wstr_null(); return 1".to_owned(),
        format!("let w=c_wstr(\"{long_text}\"); return 1"),
        "let cb=c_callback(handler); return 1".to_owned(),
    ] {
        let helper = format!("fn helper()->i32 {{ {body} }} fn handler(x:i32)->i32 {{ return x }}");
        for text in [format!("{query}\n{helper}"), format!("{helper}\n{query}")] {
            let source = SourceUnit::new(FileId(0), ModuleId(0), "impl088-static-ct.zil", text);
            let report =
                zil_driver::compile_source(&source, &zil_driver::CompileOptions::default());
            assert_ne!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.phase == Phase::Ct && d.code == "E-CT-FORBIDDEN"),
                "{}",
                report.to_json()
            );
            assert!(!report.artifacts.contains_key("executable"));
        }
    }
}
