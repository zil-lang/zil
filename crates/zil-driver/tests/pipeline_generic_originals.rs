use std::{collections::BTreeMap, fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
use zil_ir::{InstructionKind, Type, TypedFunction};

const RESOURCE: &str = r#"
extern "C" fn putchar(value:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn make(code:i32)->Token contract {result(create)} {return Token{code}}
fn read_generic<T:Copy>(token:Token)->i32 {return token.code}
fn consume(give token:Token) {}
"#;

fn source(text: impl Into<String>) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "generic_pipeline.zil", text)
}

#[test]
fn generic_pipeline_calls_run_through_ownership_and_grouped_inference() {
    let text = format!(
        r#"{RESOURCE}
fn identity<T:Copy>(value:T)->T {{return value}}
fn main()->i32 {{
    make(65) |> read_generic::<i32>;
    let owner=make(66);
    give owner |> consume;
    return ((42 |> identity) |> (identity::<i32>))-42
}}
"#,
    );
    let program = source(text);
    let output = std::env::temp_dir().join(format!("zil-generic-pipeline-{}", std::process::id()));
    fs::create_dir_all(&output).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let binary = output.join(format!("{backend:?}-{optimization:?}"));
            let report = compile_source(
                &program,
                &CompileOptions {
                    backend,
                    optimization,
                    output: Some(binary.clone()),
                    stop_after: Stage::Build,
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let outcome = Command::new(binary).output().unwrap();
            assert!(outcome.status.success(), "{outcome:?}");
            assert_eq!(outcome.stdout, b"AB");
            assert!(outcome.stderr.is_empty(), "{outcome:?}");
        }
    }
    fs::remove_dir_all(output).unwrap();
}

fn parameter_types(function: &TypedFunction) -> Vec<&Type> {
    function
        .parameters
        .iter()
        .map(|id| {
            &function
                .locals
                .iter()
                .find(|local| local.id == *id)
                .unwrap()
                .ty
        })
        .collect()
}

#[test]
fn pipeline_call_rejects_another_same_signature_generic_actual_in_raw_and_wire() {
    let program = source(format!(
        r#"{RESOURCE}
fn main()->i32 {{make(65) |> read_generic::<i32>;make(66) |> read_generic::<u32>;return 0}}
"#,
    ));
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&program).into_result().unwrap();
    let context = SourceIdentityContext::ClosedSnapshot {
        root: program.module_id,
        aliases: &BTreeMap::new(),
        declarations: DeclarationPlacement::Original,
    };
    let expanded =
        zil_expand::expand_with_map_semantics(&program, &syntax, &limits, &target, context)
            .unwrap();
    let mut typed = zil_types::check(&expanded.core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    let actuals: Vec<_> = typed
        .functions
        .iter()
        .filter(|function| function.name.contains("read_generic"))
        .collect();
    let [first, second] = actuals.as_slice() else {
        panic!("expected exact i32 and u32 generic applications");
    };
    assert_eq!(parameter_types(first), parameter_types(second));
    assert_eq!(first.result_mode, second.result_mode);
    assert_eq!(first.return_type, second.return_type);
    assert_eq!(first.call_kind, second.call_kind);
    let targets = [first.id, second.id];
    let main = typed
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .unwrap();
    let selected = main
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match &mut instruction.kind {
            InstructionKind::Call { function, .. } if targets.contains(function) => Some(function),
            _ => None,
        })
        .unwrap();
    *selected = *targets.iter().find(|id| *id != selected).unwrap();

    let analysis = zil_analyze::analyze(&typed, &limits).unwrap_err();
    plan.module_fingerprint = typed.fingerprint();
    let raw = zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let serialized = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert_eq!(analysis[0].code, "E-ANALYZE-IR");
    assert!(analysis[0].message.contains(
        "actual BodyCall target changes its independent original rigid mixed context"
    ));
    assert_eq!(raw[0].code, "E-VERIFY-SEMANTIC");
    assert_eq!(
        raw[0].message,
        "actual Call has no complete source instance in its exact lexical world"
    );
    assert_eq!(serialized, raw);
}
