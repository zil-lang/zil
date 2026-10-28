//! CT transport checks for one actual State owner plus an independent owned F.
//! Raw Core fixtures are not source ownership proofs or executable bad programs.
#[path = "../src/ct_program.rs"]
#[allow(dead_code)]
mod adapter;

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ct::SourceCompiler;
use zil_ir::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn record(label: &str, core: &CoreModule, value: impl std::fmt::Debug) {
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let path = PathBuf::from(root).join(format!(
            "ct205-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("core.txt"), format!("{core:#?}\n")).unwrap();
        fs::write(path.join("result.txt"), format!("{value:#?}\n")).unwrap();
    }
}

fn program(source: &str) -> CoreProgram {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "ct205.zil", source);
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

fn mixed(reverse: bool, fields: usize, kind: CallableKind) -> CoreProgram {
    let mut p = program(
        "fn identity(state:i32,replacement:i32,tag:i64)->i32{return state} fn marker()->i32{return #(6*7)}",
    );
    let f = &mut p.modules[0].core.functions[0];
    let mut members = vec![("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer)))];
    for i in 0..fields {
        members.push((format!("callback{i}"), Type::Named("F".into())));
    }
    f.parameters[0].ty = Type::Package {
        name: "State@module0:copy-i64".into(),
        fields: members,
    };
    f.parameters[1].ty = Type::Named("F".into());
    f.return_type = f.parameters[0].ty.clone();
    if reverse {
        f.parameters.swap(0, 1);
    }
    let owner = u32::from(reverse);
    let direct = 1 - owner;
    f.state_parameters = vec![CoreStateParameter {
        index: owner,
        bindings: vec![CoreStateBinding {
            generic_index: 0,
            generic_name: "F".into(),
            kind,
            parameters: vec![Type::I64],
            result: Type::I32,
            fields: (1..=fields as u32).collect(),
        }],
    }];
    f.callable_parameters = vec![CoreCallableParameter {
        index: direct,
        generic_index: 0,
        generic_name: "F".into(),
        kind,
        parameters: vec![Type::I64],
        result: Type::I32,
    }];
    p
}

fn results(p: &CoreProgram, limits: &Limits) -> [Result<CoreModule, Vec<zil_base::Diagnostic>>; 3] {
    let core = &p.modules[0].core;
    let target = TargetProfile::default();
    [
        zil_ct::evaluate_source_constants(core, &target, limits),
        zil_ct::SourceProgramContext::new(p, limits).and_then(|context| {
            zil_ct::evaluate_source_constants_with_context(core, &target, limits, &context)
                .map(|(core, _)| core)
        }),
        adapter::single(core, &target, limits),
    ]
}

fn check_category(label: &str, p: &CoreProgram, limits: &Limits, category: ResultCategory) {
    for (entry, result) in results(p, limits).into_iter().enumerate() {
        record(&format!("{label}-{entry}"), &p.modules[0].core, &result);
        let errors = result.unwrap_err();
        assert_eq!(errors[0].phase, Phase::Ct, "{label}/{entry}: {errors:#?}");
        assert_eq!(errors[0].category, category, "{label}/{entry}: {errors:#?}");
    }
}

#[test]
fn ct205_preserves_one_or_two_fields_and_separate_replacement_in_both_orders() {
    for reverse in [false, true] {
        for count in [1, 2] {
            for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
                let p = mixed(reverse, count, kind);
                for (entry, result) in results(&p, &Limits::default()).into_iter().enumerate() {
                    record(
                        &format!("preserve-{reverse}-{count}-{kind:?}-{entry}"),
                        &p.modules[0].core,
                        &result,
                    );
                    let output = result.unwrap();
                    assert_eq!(output.functions[0], p.modules[0].core.functions[0]);
                    assert!(matches!(&output.functions[1].body.statements[0].kind,
                CoreStatementKind::Return(Some(CoreExpr { kind: CoreExprKind::Integer(n), .. })) if n == "42i32"));
                }
            }
        }
    }
}

#[test]
fn ct205_unused_mixed_headers_reject_wrong_roles_signatures_return_and_selectors() {
    for attack in 0..21 {
        let mut p = mixed(false, 2, CallableKind::FnOnce);
        let f = &mut p.modules[0].core.functions[0];
        match attack {
            0 => f.state_parameters.push(f.state_parameters[0].clone()),
            1 => f.state_parameters[0].bindings[0].generic_index = 1,
            2 => f.state_parameters[0].bindings[0].fields.clear(),
            3 => f.state_parameters[0].bindings[0].fields = vec![1, 1],
            4 => f.state_parameters[0].bindings[0].fields = vec![2, 1],
            5 => f.callable_parameters[0].index = 0,
            6 => f.parameters[1].mode = PassingMode::Borrow,
            7 => f.callable_parameters[0].generic_name = "Other".into(),
            8 => f.callable_parameters[0].kind = CallableKind::Fn,
            9 => f.callable_parameters[0].parameters = vec![Type::I32],
            10 => f.callable_parameters[0].result = Type::I64,
            11 => f.callable_parameters.push(f.callable_parameters[0].clone()),
            12 => {
                let Type::Package { name, .. } = &mut f.return_type else {
                    unreachable!()
                };
                *name = "OtherState".into();
            }
            13 => f.parameters[1].ty = Type::Named("Other".into()),
            14 => f.parameters[0].ty = Type::I32,
            15 => f.state_parameters[0].bindings[0].fields = vec![3],
            16 => f.state_parameters[0].bindings[0].fields = vec![1],
            17 => f.type_parameters.push(CoreTypeParameter {
                index: 0,
                name: "T".into(),
                bound: CoreTypeBound::StoreElement,
            }),
            18 => {
                f.callable_return = Some(CoreCallableReturn {
                    kind: CallableKind::FnOnce,
                    parameters: vec![],
                    result: Type::I32,
                })
            }
            19 => {
                f.receiver = Some(ReceiverShape {
                    ty: Type::I32,
                    mode: PassingMode::Copy,
                    implicit: true,
                })
            }
            20 => f.parameters[0].mode = PassingMode::Borrow,
            _ => unreachable!(),
        }
        check_category(
            &format!("bad-head-{attack}"),
            &p,
            &Limits::default(),
            ResultCategory::Unsupported,
        );
    }
}

#[test]
fn ct205_both_heads_are_bounded_before_deep_signature_equality() {
    for direct in [false, true] {
        for large in 0..3 {
            let mut p = mixed(false, 2, CallableKind::Fn);
            let f = &mut p.modules[0].core.functions[0];
            let (name, parameters, result) = if direct {
                let b = &mut f.callable_parameters[0];
                (&mut b.generic_name, &mut b.parameters, &mut b.result)
            } else {
                let b = &mut f.state_parameters[0].bindings[0];
                (&mut b.generic_name, &mut b.parameters, &mut b.result)
            };
            let limits = match large {
                0 => {
                    *name = "unbounded".repeat(4096);
                    Limits {
                        max_analysis_steps: 2048,
                        ..Limits::default()
                    }
                }
                1 => {
                    *result = Type::I32;
                    for _ in 0..16 {
                        *result = Type::Tuple(vec![result.clone()]);
                    }
                    Limits {
                        max_call_depth: 8,
                        ..Limits::default()
                    }
                }
                _ => {
                    *parameters = vec![Type::I32; 65];
                    Limits {
                        max_locals: 64,
                        ..Limits::default()
                    }
                }
            };
            check_category(
                &format!("bound-{direct}-{large}"),
                &p,
                &limits,
                ResultCategory::ResourceLimit,
            );
        }
    }
}

#[test]
fn ct205_exact_metadata_node_budget_recovers_on_fresh_retry() {
    let p = mixed(true, 2, CallableKind::FnOnce);
    let core = &p.modules[0].core;
    let target = TargetProfile::default();
    let initial = zil_ct::evaluate_source_constants_with_usage(core, &target, &Limits::default());
    record("node-budget-initial", core, &initial);
    let (_, usage) = initial.unwrap();
    let exact = Limits {
        max_nodes: usage.nodes,
        ..Limits::default()
    };
    let before = zil_ct::evaluate_source_constants(core, &target, &exact);
    record("node-budget-n", core, &before);
    before.unwrap();
    let below = zil_ct::evaluate_source_constants(
        core,
        &target,
        &Limits {
            max_nodes: usage.nodes - 1,
            ..exact.clone()
        },
    );
    record("node-budget-below", core, &below);
    assert_eq!(
        below.unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    let fresh = zil_ct::evaluate_source_constants(core, &target, &exact);
    record("node-budget-fresh", core, &fresh);
    fresh.unwrap();
    println!("ct205 exact runtime metadata nodes={}", usage.nodes);
}

#[test]
fn ct205_mixed_templates_and_wrappers_never_execute_in_ct() {
    let mut p = program("fn identity()->i32{return 42}fn wrapper()->i32{return identity()}");
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
    let before = adapter::Compiler {
        program: &p,
        module: ModuleId(0),
        target: &target,
    }
    .compile(&query("wrapper"), Some(&Type::I32), &Limits::default())
    .unwrap();
    let verified = zil_verify::verify_ct_program(before, &target, &Limits::default()).unwrap();
    assert_eq!(
        zil_ct::evaluate_program(&verified).unwrap().value,
        zil_ct::CompileTimeValue::Scalar(Constant::I32(42))
    );
    p.modules[0].core.functions[0] = mixed(false, 2, CallableKind::Fn)
        .modules
        .remove(0)
        .core
        .functions
        .remove(0);
    for name in ["identity", "wrapper"] {
        let result = adapter::Compiler {
            program: &p,
            module: ModuleId(0),
            target: &target,
        }
        .compile(&query(name), Some(&Type::I32), &Limits::default());
        record(&format!("ct-forbidden-{name}"), &p.modules[0].core, &result);
        let errors = result.unwrap_err();
        assert_eq!(errors[0].phase, Phase::Ct);
        assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
    }
}
