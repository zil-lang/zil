//! Exercise the production private adapter with hostile raw Core without adding
//! a public compilation bypass just for tests.
#[path = "../src/ct_program.rs"]
mod adapter;

use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, NodeId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ct::SourceCompiler;
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn expr(kind: CoreExprKind) -> CoreExpr {
    CoreExpr {
        id: NodeId(3),
        span: span(),
        kind,
    }
}
fn statement(kind: CoreStatementKind) -> CoreStatement {
    CoreStatement {
        id: NodeId(2),
        span: span(),
        kind,
    }
}
fn empty() -> CoreBlock {
    CoreBlock {
        span: span(),
        statements: vec![],
    }
}
fn callback() -> Type {
    Type::ForeignCallback {
        parameters: vec![Type::I32; 128],
        result: Box::new(Type::I32),
    }
}
fn program() -> CoreProgram {
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: vec![ProgramModule {
            imports: vec![],
            exports: vec![NodeId(1)],
            core: CoreModule {
                schema_version: SCHEMA_VERSION,
                module_id: ModuleId(0),
                source_fingerprint: Fingerprint::ZERO,
                aggregates: vec![],
                enums: vec![],
                unsupported: vec![],
                functions: vec![CoreFunction {
                    id: NodeId(1),
                    span: span(),
                    name: "helper".into(),
                    parameters: vec![],
                    return_type: Type::I32,
                    result_mode: PassingMode::Infer,
                    receiver: None,
                    callable_return: None,
                    state_parameters: Vec::new(),
                    type_parameters: Vec::new(),
                    callable_parameters: Vec::new(),
                    foreign: None,
                    body: CoreBlock {
                        span: span(),
                        statements: vec![statement(CoreStatementKind::Return(Some(expr(
                            CoreExprKind::Integer("1".into()),
                        ))))],
                    },
                }],
            },
        }],
    }
}
fn query() -> CoreExpr {
    expr(CoreExprKind::Call {
        callee: Box::new(expr(CoreExprKind::Name("helper".into()))),
        arguments: vec![],
    })
}
fn reject(
    program: &CoreProgram,
    query: &CoreExpr,
    expected: Option<&Type>,
    limits: &Limits,
) -> Vec<zil_base::Diagnostic> {
    let target = TargetProfile::host_llvm();
    let compiler = adapter::Compiler {
        program,
        module: program.root,
        target: &target,
    };
    compiler.compile(query, expected, limits).unwrap_err()
}
fn limited(program: &CoreProgram, query: &CoreExpr, expected: Option<&Type>) {
    let errors = reject(
        program,
        query,
        expected,
        &Limits {
            max_nodes: 64,
            ..Limits::default()
        },
    );
    assert_eq!(
        errors[0].category,
        ResultCategory::ResourceLimit,
        "{errors:?}"
    );
    assert_eq!(errors[0].phase, Phase::Ct);
}
fn annotation(position: usize) -> CoreExpr {
    match position {
        0 => expr(CoreExprKind::Record {
            ty: callback(),
            fields: vec![],
        }),
        1 => expr(CoreExprKind::RecordUpdate {
            ty: Some(callback()),
            base: Box::new(expr(CoreExprKind::Unit)),
            fields: vec![],
        }),
        2 => expr(CoreExprKind::Closure {
            move_capture: false,
            parameters: vec![CoreClosureParameter {
                name: "cb".into(),
                ty: Some(callback()),
                mode: PassingMode::Infer,
                span: span(),
            }],
            return_type: None,
            body: empty(),
        }),
        _ => expr(CoreExprKind::Closure {
            move_capture: false,
            parameters: vec![],
            return_type: Some(callback()),
            body: empty(),
        }),
    }
}

#[test]
fn collector_bounds_query_and_reachable_helper_annotations_before_copying() {
    for position in 0..4 {
        limited(&program(), &annotation(position), None);
        let mut p = program();
        p.modules[0].core.functions[0].body.statements.insert(
            0,
            statement(CoreStatementKind::Expression(annotation(position))),
        );
        limited(&p, &query(), None);
    }
    let mut p = program();
    p.modules[0].core.functions[0].body.statements.insert(
        0,
        statement(CoreStatementKind::Binding {
            name: "cb".into(),
            mutable: false,
            ty: Some(callback()),
            initializer: None,
        }),
    );
    limited(&p, &query(), None);
    limited(&program(), &query(), Some(&callback()));
}

#[test]
fn collector_bounds_retained_and_returned_callable_contracts() {
    for retained in [false, true] {
        let mut p = program();
        let f = &mut p.modules[0].core.functions[0];
        if retained {
            f.callable_parameters = vec![CoreCallableParameter {
                index: 0,
                generic_index: 0,
                generic_name: "F".into(),
                kind: CallableKind::Fn,
                parameters: vec![callback()],
                result: Type::I32,
            }];
        } else {
            f.callable_return = Some(CoreCallableReturn {
                kind: CallableKind::Fn,
                parameters: vec![callback()],
                result: Type::I32,
            });
        }
        limited(&p, &query(), None);
    }
}

#[test]
fn collector_rejects_static_foreign_values_and_callbacks_in_every_reachable_position() {
    for kind in [
        CoreExprKind::Literal(Constant::NullHandle),
        CoreExprKind::Literal(Constant::NullWideString),
        CoreExprKind::Literal(Constant::WideString(vec![65; 4096])),
        CoreExprKind::Call {
            callee: Box::new(expr(CoreExprKind::Name("c_callback".into()))),
            arguments: vec![expr(CoreExprKind::Name("helper".into()))],
        },
    ] {
        for in_helper in [false, true] {
            let mut p = program();
            let q = if in_helper {
                p.modules[0].core.functions[0].body.statements.insert(
                    0,
                    statement(CoreStatementKind::Expression(expr(kind.clone()))),
                );
                query()
            } else {
                expr(kind.clone())
            };
            let errors = reject(&p, &q, None, &Limits::default());
            assert_eq!(errors[0].code, "E-CT-FORBIDDEN", "{errors:?}");
            assert_eq!(errors[0].phase, Phase::Ct);
        }
    }
}

#[test]
fn single_runtime_copy_has_bounded_static_values_and_keeps_valid_literals() {
    let mut p = program();
    let f = &mut p.modules[0].core.functions[0];
    let mut units = vec![65; 128];
    units.push(0);
    f.body.statements.insert(
        0,
        statement(CoreStatementKind::Expression(expr(CoreExprKind::Literal(
            Constant::WideString(units),
        )))),
    );
    let core = &p.modules[0].core;
    let target = TargetProfile::host_llvm();
    let output = adapter::single(core, &target, &Limits::default()).unwrap();
    assert_eq!(output, *core);
    let errors = adapter::single(
        core,
        &target,
        &Limits {
            max_nodes: 64,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}

#[test]
fn collector_text_budget_precedes_declaration_and_query_name_copies() {
    let limits = Limits {
        max_analysis_steps: 64,
        ..Limits::default()
    };
    let mut p = program();
    p.modules[0].core.aggregates.push(CoreAggregate {
        id: NodeId(8),
        name: "A".repeat(1024),
        kind: AggregateKind::Struct,
        teardown: None,
        fields: vec![],
        span: span(),
    });
    assert_eq!(
        reject(&p, &query(), None, &limits)[0].category,
        ResultCategory::ResourceLimit
    );
    let q = expr(CoreExprKind::Call {
        callee: Box::new(expr(CoreExprKind::Name("x".repeat(1024)))),
        arguments: vec![],
    });
    assert_eq!(
        reject(&program(), &q, None, &limits)[0].category,
        ResultCategory::ResourceLimit
    );
}
