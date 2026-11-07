//! Independent new Core classification at the real private CT adapter.
//! Pure ordinary functions are positive controls, so TypeApply stripping cannot
//! accidentally pass by hiding behind unrelated resource rejection.
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
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let p = root.join(format!(
            "independent202-ct-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p, retained.is_some())
    }
    fn write(&self, n: &str, b: impl AsRef<[u8]>) {
        fs::write(self.0.join(n), b).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn source_program() -> CoreProgram {
    let source = "fn pure()->i32{return 7}fn wrapper()->i32{return pure()}fn main()->i32{return wrapper()-7}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "ct202.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            imports: vec![],
            exports: vec![],
            core,
        }],
    }
}
fn name(program: &CoreProgram, text: &str) -> CoreExpr {
    CoreExpr {
        id: NodeId(900_001),
        span: program.modules[0].core.functions[0].span,
        kind: CoreExprKind::Name(text.into()),
    }
}
fn call(program: &CoreProgram, text: &str) -> CoreExpr {
    CoreExpr {
        id: NodeId(900_002),
        span: program.modules[0].core.functions[0].span,
        kind: CoreExprKind::Call {
            callee: Box::new(name(program, text)),
            arguments: vec![],
        },
    }
}
fn application(program: &CoreProgram, text: &str) -> CoreExpr {
    CoreExpr {
        id: NodeId(900_003),
        span: program.modules[0].core.functions[0].span,
        kind: CoreExprKind::TypeApply {
            callee: Box::new(name(program, text)),
            type_arguments: vec![Type::Package {
                name: "ActualFiniteElement".into(),
                fields: vec![("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
            }],
        },
    }
}
fn ordinary(program: &mut CoreProgram) {
    let f = program.modules[0]
        .core
        .functions
        .iter_mut()
        .find(|f| f.name == "pure")
        .unwrap();
    f.type_parameters = vec![CoreTypeParameter {
        index: 0,
        name: "T".into(),
        bound: CoreTypeBound::StoreElement,
    }];
}
fn forbidden(label: &str, program: &CoreProgram, query: &CoreExpr) {
    let case = Case::new(label);
    case.write("program.txt", format!("{program:#?}"));
    case.write("query.txt", format!("{query:#?}"));
    let target = TargetProfile::host_llvm();
    let compiler = adapter::Compiler {
        program,
        module: program.root,
        target: &target,
    };
    let result = compiler.compile(query, Some(&Type::I32), &Limits::default());
    case.write("result.txt", format!("{result:#?}"));
    let ds = result.unwrap_err();
    assert!(
        ds.iter()
            .any(|d| d.phase == Phase::Ct && d.code == "E-CT-FORBIDDEN"),
        "{label}: {ds:#?}"
    );
}

#[test]
fn pure_control_compiles_but_new_type_applications_never_execute_in_ct() {
    let original = source_program();
    let target = TargetProfile::host_llvm();
    let compiler = adapter::Compiler {
        program: &original,
        module: original.root,
        target: &target,
    };
    let valid = compiler.compile(
        &call(&original, "wrapper"),
        Some(&Type::I32),
        &Limits::default(),
    );
    let c = Case::new("pure-control");
    c.write("program.txt", format!("{original:#?}"));
    c.write("result.txt", format!("{valid:#?}"));
    let verified =
        zil_verify::verify_ct_program(valid.unwrap(), &target, &Limits::default()).unwrap();
    let evaluation = zil_ct::evaluate_program(&verified).unwrap();
    c.write("evaluation.txt", format!("{evaluation:#?}"));
    assert_eq!(
        evaluation.value,
        zil_ct::CompileTimeValue::Scalar(Constant::I32(7))
    );
    // A new application is forbidden even around an otherwise pure, concrete
    // callee. Removing the wrapper and executing the callee would be a bypass.
    let mut query = call(&original, "pure");
    let CoreExprKind::Call { callee, .. } = &mut query.kind else {
        unreachable!()
    };
    **callee = application(&original, "pure");
    forbidden("application-around-concrete-pure", &original, &query);
    forbidden(
        "application-as-value",
        &original,
        &application(&original, "pure"),
    );
    let mut generic = original.clone();
    ordinary(&mut generic);
    forbidden("bare-ordinary-template", &generic, &call(&generic, "pure"));
    forbidden("applied-ordinary-template", &generic, &query);
    let nested = application(&original, "pure");
    let mut twice = query.clone();
    let CoreExprKind::Call { callee, .. } = &mut twice.kind else {
        unreachable!()
    };
    let CoreExprKind::TypeApply { callee: inner, .. } = &mut callee.kind else {
        unreachable!()
    };
    **inner = nested;
    forbidden("nested-type-application", &generic, &twice);
}

#[test]
fn a_regular_wrapper_cannot_hide_template_or_application_execution() {
    let original = source_program();
    let mut generic = original.clone();
    ordinary(&mut generic);
    forbidden(
        "wrapper-to-bare-template",
        &generic,
        &call(&generic, "wrapper"),
    );
    let application = application(&generic, "pure");
    let f = generic.modules[0]
        .core
        .functions
        .iter_mut()
        .find(|f| f.name == "wrapper")
        .unwrap();
    let CoreStatementKind::Return(Some(expr)) = &mut f.body.statements[0].kind else {
        unreachable!()
    };
    let CoreExprKind::Call { callee, .. } = &mut expr.kind else {
        unreachable!()
    };
    **callee = application;
    forbidden(
        "wrapper-to-applied-template",
        &generic,
        &call(&generic, "wrapper"),
    );
    // The same applied wrapper also stays forbidden if its target is made
    // concrete again; the public TypeApply node itself cannot disappear.
    generic.modules[0]
        .core
        .functions
        .iter_mut()
        .find(|f| f.name == "pure")
        .unwrap()
        .type_parameters
        .clear();
    forbidden(
        "wrapper-to-applied-concrete",
        &generic,
        &call(&generic, "wrapper"),
    );
}

struct ObservedCompiler(Cell<usize>);
impl SourceCompiler for ObservedCompiler {
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
            "independent observer records whether query compilation occurred",
        )])
    }
}

#[test]
fn raw_application_metadata_is_charged_before_preparing_its_ct_argument() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "ct202-order.zil",
        "fn pure()->i64{return 7}fn sink(n:i64)->i32{return 0}fn main()->i32{return sink(#(pure()))}",
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let mut core = zil_expand::expand(&unit, &ast).unwrap();
    let sink = core
        .functions
        .iter_mut()
        .find(|f| f.name == "sink")
        .unwrap();
    sink.type_parameters = vec![CoreTypeParameter {
        index: 0,
        name: "T".into(),
        bound: CoreTypeBound::StoreElement,
    }];
    let main = core
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let CoreStatementKind::Return(Some(expr)) = &mut main.body.statements[0].kind else {
        unreachable!()
    };
    let CoreExprKind::Call { callee, .. } = &mut expr.kind else {
        unreachable!()
    };
    **callee = CoreExpr {
        id: NodeId(920_001),
        span: callee.span,
        kind: CoreExprKind::TypeApply {
            callee: callee.clone(),
            type_arguments: vec![Type::Package {
                name: "RuntimeElement".into(),
                fields: vec![("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
            }],
        },
    };
    let limits = Limits {
        max_analysis_steps: 2048,
        ..Limits::default()
    };
    for oversized in [false, true] {
        let case = Case::new(if oversized {
            "precharge-oversized-type"
        } else {
            "precharge-valid-type-query-control"
        });
        let mut changed = core.clone();
        if oversized {
            let main = changed
                .functions
                .iter_mut()
                .find(|f| f.name == "main")
                .unwrap();
            let CoreStatementKind::Return(Some(expr)) = &mut main.body.statements[0].kind else {
                unreachable!()
            };
            let CoreExprKind::Call { callee, .. } = &mut expr.kind else {
                unreachable!()
            };
            let CoreExprKind::TypeApply { type_arguments, .. } = &mut callee.kind else {
                unreachable!()
            };
            type_arguments[0] = Type::Named("wide".repeat(2048));
        }
        let observer = ObservedCompiler(Cell::new(0));
        let result = zil_ct::evaluate_source_constants_with_compiler(
            &changed,
            &TargetProfile::host_llvm(),
            &limits,
            None,
            &observer,
        );
        case.write("core.txt", format!("{changed:#?}"));
        case.write("result.txt", format!("{result:#?}"));
        case.write(
            "observer.json",
            format!(
                "{{\"compiler_calls\":{},\"oversized_type\":{oversized}}}\n",
                observer.0.get()
            ),
        );
        let errors = result.unwrap_err();
        if oversized {
            assert_eq!(observer.0.get(), 0);
            assert!(
                errors
                    .iter()
                    .any(|d| d.phase == Phase::Ct && d.category == ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
        } else {
            assert_eq!(observer.0.get(), 1);
            assert!(
                errors.iter().any(|d| d.code == "E-REVIEW-QUERY-REACHED"),
                "{errors:#?}"
            );
        }
    }
}
