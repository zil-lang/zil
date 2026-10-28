//! Adversarial Core starts from the real, previously failing helper source.
//! Unused definition checks are compiler-contract gates, not runtime attacks.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, NodeId, Phase, SourceUnit, Span, TargetProfile};
use zil_ir::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = include_str!("impl202_fixtures/01_original_push.zil");

fn source() -> CoreModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "original_push.zil", SOURCE);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand(&unit, &syntax).unwrap()
}
fn expr(kind: CoreExprKind) -> CoreExpr {
    CoreExpr {
        id: NodeId(900_000),
        kind,
        span: Span::new(FileId(0), 1, 2),
    }
}
fn name(value: &str) -> CoreExpr {
    expr(CoreExprKind::Name(value.into()))
}
fn statement(kind: CoreStatementKind) -> CoreStatement {
    CoreStatement {
        id: NodeId(900_001),
        kind,
        span: Span::new(FileId(0), 1, 2),
    }
}
fn unused_core() -> CoreModule {
    let mut core = source();
    core.functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap()
        .body
        .statements = vec![statement(CoreStatementKind::Return(Some(expr(
        CoreExprKind::Integer("0".into()),
    ))))];
    core
}
fn append_identity(core: &mut CoreModule) -> usize {
    let mut function = core
        .functions
        .iter()
        .find(|f| !f.type_parameters.is_empty())
        .unwrap()
        .clone();
    function.id = NodeId(core.functions.iter().map(|f| f.id.0).max().unwrap() + 1);
    function.name = "unused_identity".into();
    function.parameters = vec![CoreParameter {
        name: "value".into(),
        ty: Type::Named("T".into()),
        mode: PassingMode::Infer,
        span: function.span,
    }];
    function.return_type = Type::Named("T".into());
    function.result_mode = PassingMode::Infer;
    function.body.statements = vec![statement(CoreStatementKind::Return(Some(name("value"))))];
    let index = core.functions.len();
    core.functions.push(function);
    index
}
fn inspect(label: &str, core: &CoreModule, code: Option<&str>) {
    let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
    let out = retained
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "raw202-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("original.zil"), SOURCE).unwrap();
    fs::write(out.join("core.txt"), format!("{core:#?}")).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let single = zil_types::check(core, &target, &limits);
    let program = zil_types::check_program(
        &CoreProgram {
            root: core.module_id,
            source_fingerprint: core.source_fingerprint,
            modules: vec![ProgramModule {
                core: core.clone(),
                imports: vec![],
                exports: vec![],
            }],
        },
        &target,
        &limits,
    );
    fs::write(out.join("single.txt"), format!("{single:#?}")).unwrap();
    fs::write(out.join("program.txt"), format!("{program:#?}")).unwrap();
    let program = program.map(|p| p.module);
    for (route, result) in [("single", single), ("program", program)] {
        match code {
            Some(code) => {
                let errors = result.unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|e| e.phase == Phase::Types && e.code == code),
                    "{label}/{route}: {errors:?}"
                );
            }
            None => {
                result.unwrap_or_else(|e| panic!("{label}/{route}: {e:?}"));
            }
        }
    }
    if retained.is_none() {
        let _ = fs::remove_dir_all(out);
    }
}

#[test]
fn unused_ordinary_binder_is_local_to_its_definition() {
    let mut core = unused_core();
    let index = append_identity(&mut core);
    inspect("unused-identity-control", &core, None);
    core.functions[index].type_parameters[0].name = "U".into();
    // T still belongs to the other retained template. No actual instance of
    // this identity exists that could accidentally supply its missing binder.
    inspect("unused-identity-foreign-T", &core, Some("E-UNSUPPORTED"));
}

#[test]
fn unused_type_application_checks_target_before_value_arity() {
    let mut core = unused_core();
    let index = core
        .functions
        .iter()
        .position(|f| !f.type_parameters.is_empty())
        .unwrap();
    inspect("unused-push-control", &core, None);
    core.functions[index].body.statements = vec![statement(CoreStatementKind::Return(Some(expr(
        CoreExprKind::Call {
            callee: Box::new(expr(CoreExprKind::TypeApply {
                callee: Box::new(name("putchar")),
                type_arguments: vec![Type::Named("T".into())],
            })),
            arguments: vec![name("items"), name("value")],
        },
    ))))];
    inspect(
        "unused-foreign-type-target",
        &core,
        Some("E-TYPE-APPLICATION"),
    );
}

#[test]
fn unused_store_element_bound_does_not_grant_fields_or_numeric_operations() {
    for (label, value, code) in [
        (
            "field",
            expr(CoreExprKind::Member {
                object: Box::new(name("value")),
                member: "value".into(),
            }),
            "E-FIELD-TYPE",
        ),
        (
            "numeric",
            expr(CoreExprKind::Binary {
                op: BinaryOp::Add,
                left: Box::new(name("value")),
                right: Box::new(name("value")),
            }),
            "E-TYPE-MISMATCH",
        ),
    ] {
        let mut core = unused_core();
        let index = append_identity(&mut core);
        core.functions[index]
            .body
            .statements
            .insert(0, statement(CoreStatementKind::Expression(value)));
        inspect(label, &core, Some(code));
    }
}

#[test]
fn ordinary_metadata_and_application_placement_are_checked_when_unused() {
    for shape in [
        "index",
        "empty-name",
        "two-binders",
        "bare-call",
        "standalone",
        "nested",
        "no-type",
        "two-types",
    ] {
        let mut core = unused_core();
        let index = append_identity(&mut core);
        let apply = || {
            expr(CoreExprKind::TypeApply {
                callee: Box::new(name("unused_identity")),
                type_arguments: vec![Type::Named("T".into())],
            })
        };
        let function = &mut core.functions[index];
        let code = match shape {
            "index" => {
                function.type_parameters[0].index = 1;
                "E-TYPE-PARAMETERS"
            }
            "empty-name" => {
                function.type_parameters[0].name.clear();
                "E-TYPE-PARAMETERS"
            }
            "two-binders" => {
                function
                    .type_parameters
                    .push(function.type_parameters[0].clone());
                "E-TYPE-PARAMETERS"
            }
            "bare-call" => {
                function.body.statements[0].kind =
                    CoreStatementKind::Return(Some(expr(CoreExprKind::Call {
                        callee: Box::new(name("unused_identity")),
                        arguments: vec![name("value")],
                    })));
                "E-TYPE-APPLICATION"
            }
            "standalone" => {
                function
                    .body
                    .statements
                    .insert(0, statement(CoreStatementKind::Expression(apply())));
                "E-TYPE-APPLICATION"
            }
            _ => {
                let (callee, type_arguments) = match shape {
                    "nested" => (apply(), vec![Type::Named("T".into())]),
                    "no-type" => (name("unused_identity"), vec![]),
                    "two-types" => (
                        name("unused_identity"),
                        vec![Type::Named("T".into()), Type::Named("T".into())],
                    ),
                    _ => unreachable!(),
                };
                function.body.statements[0].kind =
                    CoreStatementKind::Return(Some(expr(CoreExprKind::Call {
                        callee: Box::new(expr(CoreExprKind::TypeApply {
                            callee: Box::new(callee),
                            type_arguments,
                        })),
                        arguments: vec![name("value")],
                    })));
                "E-TYPE-APPLICATION"
            }
        };
        // 203 permits a static ordinary call whose value parameter uniquely
        // determines T. This unused definition supplies its own lexical T;
        // it does not create or complete a recursive runtime instance. The
        // instantiated recursion guard remains in impl202_independent.
        let expected = (shape != "bare-call").then_some(code);
        inspect(shape, &core, expected);
    }
}
