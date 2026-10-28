//! Independent CT transport checks. These raw headers never stand in for
//! declared-body checking, concrete ownership, or native execution.
#[path = "../src/ct_program.rs"]
#[allow(dead_code)]
mod adapter;
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ct::SourceCompiler;
use zil_ir::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn record(label: &str, original: impl std::fmt::Debug, result: impl std::fmt::Debug) {
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let path = PathBuf::from(root).join(format!(
            "independent205-ct-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("input.txt"), format!("{original:#?}\n")).unwrap();
        fs::write(path.join("result.txt"), format!("{result:#?}\n")).unwrap();
    }
}
fn module(id: u32, source: &str) -> CoreModule {
    let unit = SourceUnit::new(
        FileId(id),
        ModuleId(id),
        format!("ct205-independent-{id}.zil"),
        source,
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand(&unit, &syntax).unwrap()
}
fn program(core: CoreModule) -> CoreProgram {
    CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            imports: vec![],
            exports: core.functions.iter().map(|f| f.id).collect(),
            core,
        }],
    }
}
fn mixed(f: &mut CoreFunction, kind: CallableKind) {
    let span = f.span;
    f.parameters = vec![
        CoreParameter {
            name: "replacement".into(),
            mode: PassingMode::Infer,
            span,
            ty: Type::Named("F".into()),
        },
        CoreParameter {
            name: "flag".into(),
            mode: PassingMode::Copy,
            span,
            ty: Type::Bool,
        },
        CoreParameter {
            name: "count".into(),
            mode: PassingMode::Copy,
            span,
            ty: Type::I64,
        },
        CoreParameter {
            name: "state".into(),
            mode: PassingMode::Infer,
            span,
            ty: Type::Package {
                name: "State@phantom:module1".into(),
                fields: vec![
                    ("first".into(), Type::Named("F".into())),
                    ("owner".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                    ("tag".into(), Type::I64),
                    ("second".into(), Type::Named("F".into())),
                ],
            },
        },
    ];
    f.return_type = f.parameters[3].ty.clone();
    f.state_parameters = vec![CoreStateParameter {
        index: 3,
        bindings: vec![CoreStateBinding {
            generic_index: 0,
            generic_name: "F".into(),
            kind,
            parameters: vec![Type::I64],
            result: Type::I32,
            fields: vec![0, 3],
        }],
    }];
    f.callable_parameters = vec![CoreCallableParameter {
        index: 0,
        generic_index: 0,
        generic_name: "F".into(),
        kind,
        parameters: vec![Type::I64],
        result: Type::I32,
    }];
}
fn scalar_call_argument<'a>(core: &'a CoreModule, function: &str) -> &'a str {
    let f = core.functions.iter().find(|f| f.name == function).unwrap();
    let CoreStatementKind::Return(Some(expr)) = &f.body.statements[0].kind else {
        panic!("{f:#?}")
    };
    let CoreExprKind::Call { arguments, .. } = &expr.kind else {
        panic!("{expr:#?}")
    };
    let CoreExprKind::Integer(n) = &arguments[2].kind else {
        panic!("{arguments:#?}")
    };
    n
}

#[test]
fn independent205_ct_reversed_nonadjacent_roles_preserve_actual_scalar_context() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let mut core = module(
            0,
            "fn revise(replacement:i32,flag:bool,count:i64,state:i32)->i32{return state}fn marker()->i32{return revise(2,true,#(4294967296),4)}",
        );
        mixed(&mut core.functions[0], kind);
        let p = program(core.clone());
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let context = zil_ct::SourceProgramContext::new(&p, &limits).unwrap();
        for (name, result) in [
            (
                "plain",
                zil_ct::evaluate_source_constants(&core, &target, &limits),
            ),
            (
                "context",
                zil_ct::evaluate_source_constants_with_context(&core, &target, &limits, &context)
                    .map(|(m, _)| m),
            ),
            ("driver", adapter::single(&core, &target, &limits)),
        ] {
            record(&format!("nonadjacent-{kind:?}-{name}"), &core, &result);
            let result = result.unwrap();
            assert_eq!(result.functions[0], core.functions[0]);
            assert_eq!(scalar_call_argument(&result, "marker"), "4294967296i64");
        }
        let root = module(
            0,
            "fn marker()->i32{return child.revise(2,true,#(4294967296),4)}",
        );
        let mut child = module(
            1,
            "pub fn revise(replacement:i32,flag:bool,count:i64,state:i32)->i32{return state}",
        );
        mixed(&mut child.functions[0], kind);
        let mut p = program(root.clone());
        p.modules[0].imports.push(CoreImport {
            alias: "child".into(),
            module: ModuleId(1),
            span: root.functions[0].span,
        });
        p.modules.push(ProgramModule {
            imports: vec![],
            exports: child.functions.iter().map(|f| f.id).collect(),
            core: child,
        });
        let context = zil_ct::SourceProgramContext::new(&p, &limits).unwrap();
        let result =
            zil_ct::evaluate_source_constants_with_context(&root, &target, &limits, &context);
        record(&format!("imported-context-{kind:?}"), &p, &result);
        assert_eq!(
            scalar_call_argument(&result.unwrap().0, "marker"),
            "4294967296i64"
        );
    }
}

fn query(p: &CoreProgram, name: &str) -> CoreExpr {
    let span = p.modules[0].core.functions[0].span;
    CoreExpr {
        id: NodeId(975000),
        span,
        kind: CoreExprKind::Call {
            callee: Box::new(CoreExpr {
                id: NodeId(975001),
                span,
                kind: CoreExprKind::Name(name.into()),
            }),
            arguments: vec![],
        },
    }
}
#[test]
fn independent205_ct_imported_wrapper_cannot_execute_either_mixed_role() {
    let root = module(
        0,
        "fn wrapper()->i32{return child.pure()}fn main()->i32{return wrapper()}",
    );
    let child = module(1, "pub fn pure()->i32{return 42}");
    let mut p = program(root);
    let span = p.modules[0].core.functions[0].span;
    p.modules[0].imports.push(CoreImport {
        alias: "child".into(),
        module: ModuleId(1),
        span,
    });
    p.modules.push(ProgramModule {
        imports: vec![],
        exports: child.functions.iter().map(|f| f.id).collect(),
        core: child,
    });
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let control = adapter::Compiler {
        program: &p,
        module: p.root,
        target: &target,
    }
    .compile(&query(&p, "wrapper"), Some(&Type::I32), &limits);
    record("imported-pure-control", &p, &control);
    let verified = zil_verify::verify_ct_program(control.unwrap(), &target, &limits).unwrap();
    let result = zil_ct::evaluate_program(&verified).unwrap();
    assert_eq!(
        result.value,
        zil_ct::CompileTimeValue::Scalar(Constant::I32(42))
    );
    mixed(&mut p.modules[1].core.functions[0], CallableKind::FnOnce);
    for name in ["wrapper", "main"] {
        let result = adapter::Compiler {
            program: &p,
            module: p.root,
            target: &target,
        }
        .compile(&query(&p, name), Some(&Type::I32), &limits);
        record(&format!("imported-mixed-{name}"), &p, &result);
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|d| d.phase == Phase::Ct && d.code == "E-CT-FORBIDDEN")
        );
    }
}

struct Observer(Cell<usize>);
impl SourceCompiler for Observer {
    fn compile(
        &self,
        _: &CoreExpr,
        _: Option<&Type>,
        _: &Limits,
    ) -> zil_base::CompileResult<CtProgram> {
        self.0.set(self.0.get() + 1);
        Err(vec![Diagnostic::error(
            Phase::Ct,
            "E-REVIEW-QUERY-REACHED",
            None,
            "record whether any query compilation was reached",
        )])
    }
}
#[test]
fn independent205_ct_both_matching_heads_are_prepaid_before_query_preparation() {
    let mut original = module(
        0,
        "fn revise()->i32{return 0}fn marker()->i32{return #(6*7)}",
    );
    mixed(&mut original.functions[0], CallableKind::Fn);
    for variant in [
        "control",
        "matching-long-names",
        "owner-tree",
        "replacement-tree",
    ] {
        let mut core = original.clone();
        let f = &mut core.functions[0];
        let limits = if variant == "matching-long-names" {
            let name = "f".repeat(8192);
            f.callable_parameters[0].generic_name = name.clone();
            f.state_parameters[0].bindings[0].generic_name = name.clone();
            f.parameters[0].ty = Type::Named(name.clone());
            let Type::Package { fields, .. } = &mut f.parameters[3].ty else {
                unreachable!()
            };
            fields[0].1 = Type::Named(name.clone());
            fields[3].1 = Type::Named(name);
            f.return_type = f.parameters[3].ty.clone();
            Limits {
                max_analysis_steps: 1024,
                ..Limits::default()
            }
        } else if variant.ends_with("tree") {
            let mut nested = Type::I64;
            for _ in 0..18 {
                nested = Type::Array {
                    element: Box::new(nested),
                    length: 1,
                };
            }
            if variant == "owner-tree" {
                f.state_parameters[0].bindings[0].parameters = vec![nested]
            } else {
                f.callable_parameters[0].parameters = vec![nested]
            }
            Limits {
                max_call_depth: 8,
                ..Limits::default()
            }
        } else {
            Limits::default()
        };
        let observer = Observer(Cell::new(0));
        let result = zil_ct::evaluate_source_constants_with_compiler(
            &core,
            &TargetProfile::host_llvm(),
            &limits,
            None,
            &observer,
        );
        record(variant, (&core, &limits), (&result, observer.0.get()));
        let errors = result.unwrap_err();
        if variant == "control" {
            assert_eq!(observer.0.get(), 1);
            assert!(errors.iter().any(|d| d.code == "E-REVIEW-QUERY-REACHED"));
        } else {
            assert_eq!(observer.0.get(), 0);
            assert!(
                errors
                    .iter()
                    .any(|d| d.phase == Phase::Ct && d.category == ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
        }
    }
}

#[test]
fn independent205_ct_complete_reversed_metadata_has_exact_fresh_node_budget() {
    let mut core = module(0, "fn revise()->i32{return 0}fn main(){}");
    mixed(&mut core.functions[0], CallableKind::FnOnce);
    let target = TargetProfile::host_llvm();
    let initial = zil_ct::evaluate_source_constants_with_usage(&core, &target, &Limits::default());
    record("node-budget-initial", &core, &initial);
    let (expected, usage) = initial.unwrap();
    assert!(usage.nodes > 0);
    for (label, n) in [
        ("exact", usage.nodes),
        ("below", usage.nodes - 1),
        ("fresh", usage.nodes),
    ] {
        let limits = Limits {
            max_nodes: n,
            ..Limits::default()
        };
        let result = zil_ct::evaluate_source_constants(&core, &target, &limits);
        record(label, (&core, &limits), &result);
        if label == "below" {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            );
        } else {
            assert_eq!(result.unwrap(), expected);
        }
    }
    println!(
        "independent205 CT reversed metadata exact nodes={}",
        usage.nodes
    );
}
