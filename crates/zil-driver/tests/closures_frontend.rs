//! C1 frontend regressions start with real source, not hand-authored typed IR.
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project, compile_source};
use zil_ir::{CallableKind, InstructionKind, Type, TypedModule};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "closures_frontend.zil", text)
}
fn typed(text: &str) -> TypedModule {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn report(text: &str, limits: Limits) -> zil_driver::CompileReport {
    compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Types,
            limits,
            ..CompileOptions::default()
        },
    )
}
fn accepted(text: &str) {
    let report = report(text, Limits::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn c014_01_first_call_resolves_identity_and_repeated_calls_keep_one_environment() {
    let module = typed("fn main() -> i32 { let f = (x) => x; return f(20) + f(22); }");
    assert_eq!(module.closures.len(), 1);
    let closure = &module.closures[0];
    assert_eq!(closure.parameters, vec![Type::I32]);
    assert_eq!(closure.result, Type::I32);
    assert_eq!(closure.kind, CallableKind::Fn);
    assert!(closure.captures.is_empty());
    assert_eq!(closure.function.0, 1);
    assert_eq!(module.functions[1].environment, Some(closure.id));
    assert_eq!(module.functions[1].name, "$closure0");
    let instructions: Vec<_> = module.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .collect();
    assert_eq!(
        instructions
            .iter()
            .filter(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
            .count(),
        1
    );
    assert_eq!(
        instructions
            .iter()
            .filter(|i| matches!(i.kind, InstructionKind::CallClosure { .. }))
            .count(),
        2
    );
}

#[test]
fn c014_02_known_i64_capture_constrains_unannotated_parameter_and_literals() {
    let module = typed(
        "fn result() -> i64 { let base: i64 = 2147483648; let f = move (x) => x + base; return f(2); }",
    );
    assert_eq!(module.closures[0].parameters, vec![Type::I64]);
    assert_eq!(module.closures[0].result, Type::I64);
    assert_eq!(module.closures[0].captures, vec![Type::I64]);
    assert!(
        module.functions[1]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::ReadCapture { index: 0 }))
    );
}

#[test]
fn c014_03_parameter_and_inner_binding_shadow_outer_capture_candidates() {
    let module = typed(
        "fn main() -> i32 { let x: i64 = 2147483648; let f = (x) => x; let g = () => { let x = 42; return x; }; return f(g()); }",
    );
    assert!(module.closures.iter().all(|c| c.captures.is_empty()));
    assert_eq!(module.closures[0].parameters, vec![Type::I32]);
}

#[test]
fn c014_04_capture_identity_and_source_order_survive_later_shadowing() {
    let module = typed(
        "fn result() -> i64 { let a: i64 = 1; let b: i64 = 2; let f = move () => b - a; if true { let a: i64 = 100; return f() + a; } else { return 0; } }",
    );
    let instructions = &module.functions[0].blocks[0].instructions;
    let captures = instructions
        .iter()
        .find_map(|i| match &i.kind {
            InstructionKind::MakeClosure { captures, .. } => Some(captures),
            _ => None,
        })
        .unwrap();
    let local_names: Vec<_> = captures
        .iter()
        .map(|capture| {
            let instruction = instructions
                .iter()
                .find(|i| i.result == Some(*capture))
                .unwrap();
            let InstructionKind::Load(local) = instruction.kind else {
                panic!("capture was not materialized")
            };
            module.functions[0].locals[local.0 as usize].name.as_str()
        })
        .collect();
    assert_eq!(local_names, ["b", "a"]);
    assert_eq!(module.closures[0].captures, [Type::I64, Type::I64]);
}

#[test]
fn c014_05_local_copy_assignment_and_pending_capture_are_concrete_values() {
    let module = typed(
        "fn main() -> i32 { var flag = true; let f = move (x) => { if flag { return x; } else { return x + 1; } }; flag = false; var g = f; g = f; return g(41); }",
    );
    assert_eq!(module.closures[0].captures, [Type::Bool]);
    assert_eq!(module.closures[0].parameters, [Type::I32]);
    let closure_type = Type::Closure(module.closures[0].id);
    assert_eq!(
        module.functions[0]
            .locals
            .iter()
            .filter(|l| l.ty == closure_type)
            .count(),
        2
    );
    let instructions = &module.functions[0].blocks[0].instructions;
    let creation = instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    let changed = instructions
        .iter()
        .rposition(|i| matches!(i.kind, InstructionKind::Store { local, .. } if local.0 == 0))
        .unwrap();
    assert!(creation < changed);
}

#[test]
fn c014_06_rejects_ambiguous_conflicting_and_mutating_closures() {
    let cases = [
        (
            "fn main() -> i32 { let f = (x) => x; return 0; }",
            ResultCategory::SafeReject,
            "E-CLOSURE-INFERENCE",
        ),
        (
            "fn main() -> i32 { let f = (x) => x; f(1); f(true); return 0; }",
            ResultCategory::SafeReject,
            "E-TYPE-MISMATCH",
        ),
        (
            "fn main() -> i32 { let f = (x) => x; return f(); }",
            ResultCategory::SafeReject,
            "E-CALL-ARITY",
        ),
        (
            "fn main() -> i32 { var x = 1; let f = () => { x = 2; return x; }; return f(); }",
            ResultCategory::SafeReject,
            "E-CLOSURE-MUTABILITY",
        ),
        (
            "fn main() -> i32 { var x = 1; let f = move () => { x = 2; return x; }; return f(); }",
            ResultCategory::SafeReject,
            "E-CLOSURE-MUTABILITY",
        ),
        (
            "fn main() -> i32 { let f = () => 1; return f; }",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "fn take(x: i32) -> i32 => x; fn main() -> i32 { let f = () => 1; return take(f); }",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "fn main() -> i32 { let f = (x, x) => x; return f(1, 2); }",
            ResultCategory::SafeReject,
            "E-NAME-DUPLICATE",
        ),
    ];
    for (source, category, code) in cases {
        let report = report(source, Limits::default());
        assert_eq!(report.status, category, "{source}: {}", report.to_json());
        assert_eq!(
            report.diagnostics[0].code,
            code,
            "{source}: {}",
            report.to_json()
        );
    }
}

#[test]
fn c014_07_body_call_signature_and_boolean_constraints_infer_without_call_defaults() {
    let module = typed(
        "fn wide(x: i64) -> i64 => x; fn main() -> i32 { let f = (x) => wide(x); let g = (flag) => !flag; f(2147483648); g(true); return 0; }",
    );
    assert_eq!(module.closures[0].parameters, [Type::I64]);
    assert_eq!(module.closures[1].parameters, [Type::Bool]);
    assert_eq!(module.closures[0].function.0, 2);
    assert_eq!(module.closures[1].function.0, 3);
}

#[test]
fn c014_08_synthetic_functions_share_graph_budgets() {
    let text = "fn main() -> i32 { let base = 40; let f = move (x) => base + x; return f(2); }";
    let module = typed(text);
    let blocks: usize = module.functions.iter().map(|f| f.blocks.len()).sum();
    let locals: usize = module.functions.iter().map(|f| f.locals.len()).sum();
    let values: usize = module.functions.iter().map(|f| f.values.len()).sum();
    let instructions: usize = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .map(|b| b.instructions.len())
        .sum();
    for limits in [
        Limits {
            max_functions: 1,
            ..Limits::default()
        },
        Limits {
            max_blocks: blocks - 1,
            ..Limits::default()
        },
        Limits {
            max_locals: locals - 1,
            ..Limits::default()
        },
        Limits {
            max_values: values - 1,
            ..Limits::default()
        },
        Limits {
            max_instructions: instructions - 1,
            ..Limits::default()
        },
    ] {
        let report = report(text, limits);
        assert_eq!(
            report.status,
            ResultCategory::ResourceLimit,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn c014_09_closure_body_resolves_imports_without_exporting_environment_functions() {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            source("import math\nfn main() -> i32 { let f = (x) => math.plus(x); return f(40); }"),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "math.zil",
                "pub fn plus(x: i32) -> i32 => x + 2",
            ),
        ],
        paths: BTreeMap::from([(vec!["math".into()], ModuleId(1))]),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Types,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let exports: Vec<_> = report
        .scalar_interfaces
        .iter()
        .flat_map(|i| &i.exports)
        .collect();
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "plus");
    assert_eq!(exports[0].function.0, 1);
}

#[test]
fn c014_10_ct_parameter_shadowing_does_not_use_outer_i64_hint() {
    let report = report(
        "fn main() -> i32 { let x: i64 = 2147483648; let f = (x) => #(2147483647 + 1) + x; return f(0); }",
        Limits::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Ct);
}

#[test]
fn c014_11_captureless_unit_closure_and_scalar_control_flow_are_supported() {
    accepted(
        "fn main() -> i32 { let run = () => { var n = 0; while n < 2 { n = n + 1; } }; run(); return 42; }",
    );
    accepted(
        "fn main() -> i32 { let pick = (flag, a, b) => { if flag { return a; } else { return b; } }; return pick(true, 42, 0); }",
    );
}

#[test]
fn c014_12_callee_is_evaluated_once_before_arguments() {
    let module = typed(
        "fn argument() -> i32 => 42; fn main() -> i32 { let f = (x) => x; return f(argument()); }",
    );
    let instructions = &module.functions[1].blocks[0].instructions;
    let (callee, arguments) = instructions
        .iter()
        .find_map(|i| match &i.kind {
            InstructionKind::CallClosure { callee, arguments } => Some((*callee, arguments)),
            _ => None,
        })
        .unwrap();
    assert_eq!(arguments.len(), 1);
    let callee_definition = instructions
        .iter()
        .position(|i| i.result == Some(callee))
        .unwrap();
    let argument_definition = instructions
        .iter()
        .position(|i| i.result == Some(arguments[0]))
        .unwrap();
    assert!(callee_definition < argument_definition);
    assert!(matches!(
        instructions[callee_definition].kind,
        InstructionKind::Load(_)
    ));
    assert!(
        matches!(instructions[argument_definition].kind, InstructionKind::Call { function, .. } if function.0 == 0)
    );
    assert_eq!(
        instructions
            .iter()
            .filter(|i| i.result == Some(callee))
            .count(),
        1
    );
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct NativeDirectory(PathBuf);
impl NativeDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-c014-frontend-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for NativeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn c014_13_real_native_pending_capture_copy_and_i64_inference() {
    for (text, entry, answer) in [
        (
            "fn main() -> i32 { var flag = true; let f = move (x) => { if flag { return x; } else { return x + 1; } }; flag = false; var g = f; g = f; return g(42); }",
            "main",
            "42\n",
        ),
        (
            "fn value() -> i64 { let base: i64 = 2147483648; let f = move (x) => x + base; return f(2); }",
            "value",
            "2147483650\n",
        ),
        (
            "fn main() -> i32 { let unit = (); let f = move () => unit; f(); f(); return 42; }",
            "main",
            "42\n",
        ),
    ] {
        let directory = NativeDirectory::new();
        let options = CompileOptions {
            stop_after: Stage::Build,
            output: Some(directory.0.join("program")),
            entry: Some(entry.into()),
            ..CompileOptions::default()
        };
        let report = compile_source(&source(text), &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), answer);
    }
}

#[test]
fn c014_14_known_later_argument_constrains_earlier_unsuffixed_literal() {
    let module = typed(
        "fn main() -> i32 { let wide: i64 = 2147483648; let add = (a, b) => a + b; add(1, wide); return 0; }",
    );
    assert_eq!(module.closures[0].parameters, [Type::I64, Type::I64]);
    assert_eq!(module.closures[0].result, Type::I64);
}

fn native_answer(text: &str, entry: &str, answer: &str) {
    let directory = NativeDirectory::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(directory.0.join("program")),
            entry: Some(entry.into()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{text}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let output = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), answer);
}

#[test]
fn c014_15_whole_function_constraints_are_preserved_and_call_order_independent() {
    let bodies = [
        "let add = (x: i64) => { let one = 1; return x + one; }; return add(41);",
        "let id = x => x; id(1); let value: i64 = 42; return id(value);",
        "let id = x => x; let value: i64 = 42; id(value); return id(42);",
        "let get = () => 42; get(); let answer: i64 = get(); return answer;",
        "let get = () => 42; let answer: i64 = get(); get(); return answer;",
        "let base = 40; let f = move (x: i64) => base + x; return f(2);",
    ];
    for body in bodies {
        let source = format!("fn probe() -> i64 {{ {body} }}\nfn main() -> i32 => 0");
        native_answer(&source, "probe", "42\n");
    }
}

#[test]
fn c014_16_ct_uses_solved_context_before_verified_evaluation() {
    for body in [
        "let f = (x: i64) => #(1) + x; return f(41);",
        "let f = (x: i64) => x + #(1); return f(41);",
        "let f = () => #(42); f(); let answer: i64 = f(); return answer;",
    ] {
        let source = format!("fn probe() -> i64 {{ {body} }}\nfn main() -> i32 => 0");
        native_answer(&source, "probe", "42\n");
    }
    native_answer(
        "fn probe() -> i64 { let f = (x: i64) => #(2147483647 + 1) + x; return f(1); }\nfn main() -> i32 => 0",
        "probe",
        "2147483649\n",
    );
    let report = report(
        "fn probe() -> i64 { let f = () => #(1 / 0); return f(); }\nfn main() -> i32 => 0",
        Limits::default(),
    );
    assert_eq!(report.phase, zil_base::Phase::Ct, "{}", report.to_json());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn c014_17_explicit_types_still_conflict_and_return_annotations_constrain() {
    let module = typed("fn probe() -> i64 { let f = (x) -> i64 => x; return f(42); }");
    assert_eq!(module.closures[0].parameters, [Type::I64]);
    assert_eq!(module.closures[0].result, Type::I64);
    for text in [
        "fn main() -> i32 { let f = x => x; let a: i32 = 1; f(a); let b: i64 = 2; f(b); return 0; }",
        "fn main() -> i32 { let f = x => x; let b: i64 = 2; f(b); let a: i32 = 1; f(a); return 0; }",
        "fn main() -> i32 { let f = (x: i32) -> i64 => x; return 0; }",
    ] {
        let report = report(text, Limits::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.diagnostics[0].code,
            "E-TYPE-MISMATCH",
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn c014_18_lexical_capture_identity_does_not_follow_later_bindings() {
    for text in [
        "fn plus(x: i32) -> i32 => x + 1\nfn main() -> i32 { let f = x => plus(x); let plus = 100; return f(41); }",
        "fn main() -> i32 { let x = 40; let f = move () => { let x = x + 1; return x + 1; }; return f(); }",
        "fn main() -> i32 { let x = 40; let f = move () => { var n = 0; for x in 0..2 { n = n + x; } return x + 2; }; return f(); }",
    ] {
        native_answer(text, "main", "42\n");
    }
}

#[test]
fn c014_19_ct_unit_equality_remains_available_without_runtime_admission() {
    for (operator, answer) in [("==", "42\n"), ("!=", "0\n")] {
        let text = format!(
            "fn main() -> i32 {{ let f = () => #(() {operator} ()); if f() {{ return 42; }} else {{ return 0; }} }}"
        );
        native_answer(&text, "main", answer);
        let runtime = report(
            &format!(
                "fn main() -> i32 {{ let f = () => () {operator} (); if f() {{ return 42; }} else {{ return 0; }} }}"
            ),
            Limits::default(),
        );
        assert_eq!(
            runtime.status,
            ResultCategory::SafeReject,
            "{}",
            runtime.to_json()
        );
        assert_eq!(
            runtime.phase,
            zil_base::Phase::Types,
            "{}",
            runtime.to_json()
        );
    }
}

fn raw_core(text: &str) -> zil_ir::CoreModule {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}

fn deep_type() -> Type {
    let mut ty = Type::I32;
    for _ in 0..64 {
        ty = Type::Address {
            mutable: false,
            inner: Box::new(ty),
        };
    }
    ty
}

fn resource_limited<T: std::fmt::Debug>(result: zil_base::CompileResult<T>) {
    let errors = result.unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::ResourceLimit,
        "{errors:?}"
    );
}

#[test]
fn c014_20_raw_type_and_metadata_budgets_precede_recursive_cloning() {
    let limits = Limits {
        max_call_depth: 4,
        max_nodes: 8,
        max_analysis_steps: 100,
        ..Limits::default()
    };
    let mut core = raw_core("fn main() -> i32 => 0");
    core.functions[0].return_type = deep_type();
    resource_limited(zil_types::infer_source_closures(&core, &limits));
    resource_limited(zil_types::check(&core, &TargetProfile::default(), &limits));
    let mut program = zil_ir::CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    resource_limited(zil_types::infer_program_closures(&program, &limits));
    resource_limited(zil_types::check_program(
        &program,
        &TargetProfile::default(),
        &limits,
    ));
    program.modules[0].core.functions[0].return_type = Type::I32;
    program.modules[0].exports = (0..10_000).map(zil_base::NodeId).collect();
    resource_limited(zil_types::infer_program_closures(&program, &limits));
    resource_limited(zil_types::check_program(
        &program,
        &TargetProfile::default(),
        &limits,
    ));
}

#[test]
fn c014_21_budget_preflight_visits_tail_after_first_closure_and_charges_names() {
    let mut core = raw_core("fn main() -> i32 { let f = () => 1; let tail = 0; return f(); }");
    let zil_ir::CoreStatementKind::Binding { ty, .. } =
        &mut core.functions[0].body.statements[1].kind
    else {
        panic!("fixture is a binding");
    };
    *ty = Some(deep_type());
    resource_limited(zil_types::infer_source_closures(
        &core,
        &Limits {
            max_call_depth: 8,
            ..Limits::default()
        },
    ));
    let mut core = raw_core("fn main() -> i32 => 0");
    core.functions[0].return_type = Type::Named("T".repeat(1_000));
    resource_limited(zil_types::infer_source_closures(
        &core,
        &Limits {
            max_analysis_steps: 100,
            ..Limits::default()
        },
    ));
}

#[test]
fn c014_22_synthetic_function_budget_is_global_and_checked_before_ct() {
    let text = "fn other() -> i32 { let f = () => #(1 / 0); return f(); }\nfn main() -> i32 { let f = () => 42; return f(); }";
    let result = report(
        text,
        Limits {
            max_functions: 3,
            ..Limits::default()
        },
    );
    assert_eq!(
        result.status,
        ResultCategory::ResourceLimit,
        "{}",
        result.to_json()
    );
    assert_eq!(result.phase, zil_base::Phase::Types, "{}", result.to_json());
}
