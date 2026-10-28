//! Transport and execution boundaries for raw State metadata. These fixtures
//! deliberately test CT preflight, not source-level State admission.
#[path = "../src/ct_program.rs"]
#[allow(dead_code)]
mod adapter;

use zil_base::{
    FileId, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ct::SourceCompiler;
use zil_ir::*;

fn program(source: &str) -> CoreProgram {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "ct203.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    }
}

fn state(function: &mut CoreFunction) {
    function.parameters = vec![CoreParameter {
        name: "state".into(),
        ty: Type::Package {
            name: "State".into(),
            fields: vec![
                ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("second".into(), Type::Named("G".into())),
                ("first".into(), Type::Named("F".into())),
            ],
        },
        mode: PassingMode::Infer,
        span: function.span,
    }];
    function.state_parameters = vec![CoreStateParameter {
        index: 0,
        bindings: vec![
            CoreStateBinding {
                generic_index: 0,
                generic_name: "F".into(),
                kind: CallableKind::Fn,
                parameters: vec![Type::I32],
                result: Type::I32,
                fields: vec![2],
            },
            CoreStateBinding {
                generic_index: 1,
                generic_name: "G".into(),
                kind: CallableKind::FnOnce,
                parameters: vec![Type::U8],
                result: Type::I32,
                fields: vec![1],
            },
        ],
    }];
}

fn raw_state() -> CoreProgram {
    let mut p = program("fn state()->i32{return #(6*7)}");
    state(&mut p.modules[0].core.functions[0]);
    p
}

fn limited(errors: Vec<zil_base::Diagnostic>) {
    assert_eq!(errors[0].phase, Phase::Ct, "{errors:#?}");
    assert_eq!(
        errors[0].category,
        ResultCategory::ResourceLimit,
        "{errors:#?}"
    );
}

#[test]
fn ct203_preserves_two_declared_modes_and_reversed_selectors_during_runtime_folding() {
    let p = raw_state();
    let input = &p.modules[0].core;
    let target = TargetProfile::default();
    let limits = Limits::default();
    let context = zil_ct::SourceProgramContext::new(&p, &limits).unwrap();
    let (output, usage) =
        zil_ct::evaluate_source_constants_with_usage(input, &target, &limits).unwrap();
    let contextual =
        zil_ct::evaluate_source_constants_with_context(input, &target, &limits, &context).unwrap();
    let collected = adapter::single(input, &target, &limits).unwrap();
    for result in [&output, &contextual, &collected] {
        assert_eq!(
            result.functions[0].state_parameters,
            input.functions[0].state_parameters
        );
        assert_eq!(
            result.functions[0].parameters,
            input.functions[0].parameters
        );
        assert!(matches!(&result.functions[0].body.statements[0].kind,
            CoreStatementKind::Return(Some(CoreExpr { kind: CoreExprKind::Integer(n), .. })) if n == "42"));
    }
    let exact = Limits {
        max_nodes: usage.nodes,
        ..limits.clone()
    };
    for _ in 0..2 {
        zil_ct::evaluate_source_constants(input, &target, &exact).unwrap();
    }
    limited(
        zil_ct::evaluate_source_constants(
            input,
            &target,
            &Limits {
                max_nodes: usage.nodes - 1,
                ..limits
            },
        )
        .unwrap_err(),
    );
    println!("ct203 runtime State transport nodes={}", usage.nodes);
}

#[test]
fn ct203_bounds_unused_second_binding_before_all_public_copy_paths() {
    let target = TargetProfile::default();
    for deep in [false, true] {
        let mut p = raw_state();
        let bound = &mut p.modules[0].core.functions[0].state_parameters[0].bindings[1];
        let limits = if deep {
            bound.result = Type::Tuple(vec![Type::Tuple(vec![Type::Tuple(vec![Type::I32])])]);
            Limits {
                max_call_depth: 1,
                ..Limits::default()
            }
        } else {
            bound.generic_name = "second".repeat(4096);
            Limits {
                max_analysis_steps: 256,
                ..Limits::default()
            }
        };
        let input = &p.modules[0].core;
        limited(zil_ct::evaluate_source_constants(input, &target, &limits).unwrap_err());
        limited(
            zil_ct::SourceProgramContext::new(&p, &limits)
                .err()
                .unwrap(),
        );
        limited(adapter::single(input, &target, &limits).unwrap_err());
    }
    for mutation in 0..5 {
        let mut p = raw_state();
        let f = &mut p.modules[0].core.functions[0];
        match mutation {
            0 => f.state_parameters[0].index = 1,
            1 => f.state_parameters[0].bindings.pop().map(|_| ()).unwrap(),
            2 => f.state_parameters[0].bindings[1].generic_index = 0,
            3 => f.state_parameters[0].bindings[1].fields = vec![2],
            _ => {
                f.type_parameters = vec![CoreTypeParameter {
                    index: 0,
                    name: "T".into(),
                    bound: CoreTypeBound::StoreElement,
                }]
            }
        }
        let core = &p.modules[0].core;
        for errors in [
            zil_ct::evaluate_source_constants(core, &target, &Limits::default()).unwrap_err(),
            zil_ct::SourceProgramContext::new(&p, &Limits::default())
                .err()
                .unwrap(),
            adapter::single(core, &target, &Limits::default()).unwrap_err(),
        ] {
            assert_eq!(errors[0].phase, Phase::Ct);
            assert_eq!(errors[0].category, ResultCategory::Unsupported);
        }
    }
}

#[test]
fn ct203_direct_and_wrapper_queries_cannot_execute_a_state_template() {
    let mut p = program("fn pure()->i32{return 42}fn wrapper()->i32{return pure()}");
    let target = TargetProfile::default();
    let query = |name: &str| CoreExpr {
        id: NodeId(800),
        span: Default::default(),
        kind: CoreExprKind::Call {
            callee: Box::new(CoreExpr {
                id: NodeId(801),
                span: Default::default(),
                kind: CoreExprKind::Name(name.into()),
            }),
            arguments: vec![],
        },
    };
    let positive = adapter::Compiler {
        program: &p,
        module: ModuleId(0),
        target: &target,
    }
    .compile(&query("wrapper"), Some(&Type::I32), &Limits::default())
    .unwrap();
    let verified = zil_verify::verify_ct_program(positive, &target, &Limits::default()).unwrap();
    assert_eq!(
        zil_ct::evaluate_program(&verified).unwrap().value,
        zil_ct::CompileTimeValue::Scalar(Constant::I32(42))
    );
    state(
        p.modules[0]
            .core
            .functions
            .iter_mut()
            .find(|f| f.name == "pure")
            .unwrap(),
    );
    for name in ["pure", "wrapper"] {
        let errors = adapter::Compiler {
            program: &p,
            module: ModuleId(0),
            target: &target,
        }
        .compile(&query(name), Some(&Type::I32), &Limits::default())
        .unwrap_err();
        assert_eq!(errors[0].phase, Phase::Ct);
        assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
    }
}
