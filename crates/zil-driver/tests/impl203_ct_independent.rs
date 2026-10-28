//! Independent CT checks distinguish runtime metadata preservation from actual
//! query execution. These tests use the real private driver CT adapter.
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
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let path = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "independent203-ct-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn module(id: u32, source: &str) -> CoreModule {
    let unit = SourceUnit::new(FileId(id), ModuleId(id), format!("ct203-{id}.zil"), source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand(&unit, &ast).unwrap()
}
fn state(function: &mut CoreFunction) {
    function.parameters = vec![CoreParameter {
        name: "state".into(),
        mode: PassingMode::Infer,
        span: function.span,
        ty: Type::Package {
            name: "ReversedState".into(),
            fields: vec![
                ("late".into(), Type::Named("G".into())),
                ("owner".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("early".into(), Type::Named("F".into())),
            ],
        },
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
                result: Type::I64,
                fields: vec![0],
            },
        ],
    }];
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
fn query(program: &CoreProgram, name: &str) -> CoreExpr {
    let span = program.modules[0].core.functions[0].span;
    CoreExpr {
        id: NodeId(980001),
        span,
        kind: CoreExprKind::Call {
            callee: Box::new(CoreExpr {
                id: NodeId(980002),
                span,
                kind: CoreExprKind::Name(name.into()),
            }),
            arguments: vec![],
        },
    }
}

#[test]
fn independent203_imported_wrapper_cannot_hide_new_state_execution() {
    let root = module(
        0,
        "fn wrapper()->i32{return child.pure()}fn main()->i32{return wrapper()}",
    );
    let dependency = module(1, "pub fn pure()->i32{return 7}");
    let import = CoreImport {
        alias: "child".into(),
        module: ModuleId(1),
        span: root.functions[0].span,
    };
    let mut p = program(root);
    p.modules[0].imports.push(import);
    p.modules.push(ProgramModule {
        imports: vec![],
        exports: dependency.functions.iter().map(|f| f.id).collect(),
        core: dependency,
    });
    let target = TargetProfile::host_llvm();
    let case = Case::new("imported-pure-positive");
    case.write("program.txt", format!("{p:#?}"));
    let positive = adapter::Compiler {
        program: &p,
        module: p.root,
        target: &target,
    }
    .compile(&query(&p, "wrapper"), Some(&Type::I32), &Limits::default());
    case.write("compile.txt", format!("{positive:#?}"));
    let verified =
        zil_verify::verify_ct_program(positive.unwrap(), &target, &Limits::default()).unwrap();
    let result = zil_ct::evaluate_program(&verified).unwrap();
    case.write("evaluation.txt", format!("{result:#?}"));
    assert_eq!(
        result.value,
        zil_ct::CompileTimeValue::Scalar(Constant::I32(7))
    );
    state(&mut p.modules[1].core.functions[0]);
    for called in ["wrapper", "main"] {
        let case = Case::new(&format!("imported-state-through-{called}"));
        let q = query(&p, called);
        let result = adapter::Compiler {
            program: &p,
            module: p.root,
            target: &target,
        }
        .compile(&q, Some(&Type::I32), &Limits::default());
        case.write("program.txt", format!("{p:#?}"));
        case.write("query.txt", format!("{q:#?}"));
        case.write("result.txt", format!("{result:#?}"));
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Ct && d.code == "E-CT-FORBIDDEN"),
            "{errors:#?}"
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
            "record whether query preparation was reached",
        )])
    }
}

#[test]
fn independent203_second_binding_is_bounded_before_any_query_preparation() {
    let mut original = module(
        0,
        "fn symbolic()->i32{return 0}fn pure()->i64{return 4294967296}fn main()->i64{return #(pure())}",
    );
    state(&mut original.functions[0]);
    for oversized in 0..3 {
        let mut core = original.clone();
        let limits = match oversized {
            0 => Limits::default(),
            1 => {
                let long = "g".repeat(4096);
                core.functions[0].state_parameters[0].bindings[1].generic_name = long.clone();
                let Type::Package { fields, .. } = &mut core.functions[0].parameters[0].ty else {
                    unreachable!()
                };
                fields[0].1 = Type::Named(long);
                Limits {
                    max_analysis_steps: 512,
                    ..Limits::default()
                }
            }
            _ => {
                let mut nested = Type::I64;
                for _ in 0..12 {
                    nested = Type::Tuple(vec![nested]);
                }
                core.functions[0].state_parameters[0].bindings[1].result = nested;
                Limits {
                    max_call_depth: 4,
                    ..Limits::default()
                }
            }
        };
        let observer = Observer(Cell::new(0));
        let target = TargetProfile::host_llvm();
        let result = zil_ct::evaluate_source_constants_with_compiler(
            &core, &target, &limits, None, &observer,
        );
        let case = Case::new(&format!("second-binding-before-query-{oversized}"));
        case.write("core.txt", format!("{core:#?}"));
        case.write("limits.txt", format!("{limits:#?}"));
        case.write("result.txt", format!("{result:#?}"));
        case.write(
            "observer.json",
            format!(
                "{{\"compiler_calls\":{},\"metadata_variant\":{oversized}}}\n",
                observer.0.get()
            ),
        );
        let errors = result.unwrap_err();
        if oversized == 0 {
            assert_eq!(observer.0.get(), 1);
            assert!(
                errors.iter().any(|d| d.code == "E-REVIEW-QUERY-REACHED"),
                "{errors:#?}"
            );
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
fn independent203_runtime_folding_preserves_both_selectors_and_modes_exactly() {
    let mut core = module(
        0,
        "fn symbolic()->i32{return #(6*7)}fn main()->i32{return 0}",
    );
    state(&mut core.functions[0]);
    let p = program(core.clone());
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let context = zil_ct::SourceProgramContext::new(&p, &limits).unwrap();
    let plain = zil_ct::evaluate_source_constants(&core, &target, &limits).unwrap();
    let contextual =
        zil_ct::evaluate_source_constants_with_context(&core, &target, &limits, &context)
            .unwrap()
            .0;
    let driver = adapter::single(&core, &target, &limits).unwrap();
    let case = Case::new("runtime-fold-preserves-exact-table");
    case.write("original.txt", format!("{core:#?}"));
    for (label, out) in [
        ("plain", plain),
        ("contextual", contextual),
        ("driver", driver),
    ] {
        case.write(&format!("{label}.txt"), format!("{out:#?}"));
        assert_eq!(
            out.functions[0].state_parameters,
            core.functions[0].state_parameters
        );
        assert_eq!(out.functions[0].parameters, core.functions[0].parameters);
        assert!(
            matches!(&out.functions[0].body.statements[0].kind,CoreStatementKind::Return(Some(CoreExpr{kind:CoreExprKind::Integer(n),..})) if n=="42i32")
        );
    }
}

#[test]
fn independent203_imported_value_template_hints_do_not_execute_or_ignore_shadowing() {
    let root = module(
        0,
        "fn entry(value:i32)->i64{return child.accept(value,7)}fn main()->i32{return 0}",
    );
    let child = module(1, "pub fn accept(value:i32,count:i64)->i64{return count}");
    let span = root.functions[0].span;
    let mut p = program(root);
    p.modules[0].imports.push(CoreImport {
        alias: "child".into(),
        module: ModuleId(1),
        span,
    });
    p.modules.push(ProgramModule {
        exports: child.functions.iter().map(|f| f.id).collect(),
        imports: vec![],
        core: child,
    });
    let mut q = query(&p, "entry");
    let CoreExprKind::Call { arguments, .. } = &mut q.kind else {
        unreachable!()
    };
    arguments.push(CoreExpr {
        id: NodeId(980003),
        span,
        kind: CoreExprKind::Integer("1".into()),
    });
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let case = Case::new("imported-value-argument-concrete-ct-control");
    case.write("program.txt", format!("{p:#?}"));
    case.write("query.txt", format!("{q:#?}"));
    let positive = adapter::Compiler {
        program: &p,
        module: p.root,
        target: &target,
    }
    .compile(&q, Some(&Type::I64), &limits);
    case.write("compile.txt", format!("{positive:#?}"));
    let verified = zil_verify::verify_ct_program(positive.unwrap(), &target, &limits).unwrap();
    let evaluation = zil_ct::evaluate_program(&verified).unwrap();
    case.write("evaluation.txt", format!("{evaluation:#?}"));
    assert_eq!(
        evaluation.value,
        zil_ct::CompileTimeValue::Scalar(Constant::I64(7))
    );

    let function = &mut p.modules[1].core.functions[0];
    function.type_parameters = vec![CoreTypeParameter {
        index: 0,
        name: "T".into(),
        bound: CoreTypeBound::StoreElement,
    }];
    function.parameters[0].ty = Type::Named("T".into());
    let case = Case::new("imported-value-inferred-template-ct-forbidden");
    let forbidden = adapter::Compiler {
        program: &p,
        module: p.root,
        target: &target,
    }
    .compile(&q, Some(&Type::I64), &limits);
    case.write("program.txt", format!("{p:#?}"));
    case.write("query.txt", format!("{q:#?}"));
    case.write("compile.txt", format!("{forbidden:#?}"));
    assert!(
        forbidden
            .unwrap_err()
            .iter()
            .any(|d| d.phase == Phase::Ct && d.code == "E-CT-FORBIDDEN")
    );

    p.modules[0].core = module(
        0,
        "fn entry(value:i32)->i64{return child.accept(value,#(4294967296))}fn main()->i32{return 0}",
    );
    let context = zil_ct::SourceProgramContext::new(&p, &limits).unwrap();
    let normalized = zil_ct::evaluate_source_constants_with_context(
        &p.modules[0].core,
        &target,
        &limits,
        &context,
    );
    let case = Case::new("imported-ordinary-runtime-i64-hint");
    case.write("program.txt", format!("{p:#?}"));
    case.write("normalized.txt", format!("{normalized:#?}"));
    let normalized = normalized.unwrap().0;
    let CoreStatementKind::Return(Some(CoreExpr {
        kind: CoreExprKind::Call { callee, arguments },
        ..
    })) = &normalized.functions[0].body.statements[0].kind
    else {
        unreachable!()
    };
    assert!(
        matches!(&callee.kind,CoreExprKind::Member{object,member} if member=="accept" && matches!(&object.kind,CoreExprKind::Name(name) if name=="child"))
    );
    assert_eq!(
        arguments[1].kind,
        CoreExprKind::Integer("4294967296i64".into())
    );
    assert_eq!(
        normalized.functions[0].parameters,
        p.modules[0].core.functions[0].parameters
    );

    for local in [false, true] {
        let mut shadow = p.clone();
        let function = &mut shadow.modules[0].core.functions[0];
        if local {
            function.body.statements.insert(
                0,
                CoreStatement {
                    id: NodeId(980010),
                    span,
                    kind: CoreStatementKind::Binding {
                        name: "child".into(),
                        mutable: false,
                        ty: Some(Type::I32),
                        initializer: Some(CoreExpr {
                            id: NodeId(980011),
                            span,
                            kind: CoreExprKind::Integer("0".into()),
                        }),
                    },
                },
            );
        } else {
            function.parameters.push(CoreParameter {
                name: "child".into(),
                ty: Type::I32,
                mode: PassingMode::Infer,
                span,
            });
        }
        let context = zil_ct::SourceProgramContext::new(&shadow, &limits).unwrap();
        let result = zil_ct::evaluate_source_constants_with_context(
            &shadow.modules[0].core,
            &target,
            &limits,
            &context,
        );
        let case = Case::new(&format!("runtime-header-shadow-local-{local}"));
        case.write("program.txt", format!("{shadow:#?}"));
        case.write("normalized.txt", format!("{result:#?}"));
        // Only a CT normalization assertion: the shadowed call is not claimed
        // to be a well-typed runtime program. It cannot borrow an import hint.
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|d| d.phase == Phase::Ct && d.code == "E-INTEGER-RANGE")
        );
    }
}
