//! Author controls for 221: source-derived raw/Wire, unmodified native output,
//! and separately linked allocator observation. This is not the independent review.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, Stage, compile_path, compile_source,
};
use zil_ir::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);
const FN: &str = include_str!("../../../tools/language/221/cases/fn-owned.zil");
const ONCE: &str = include_str!("../../../tools/language/221/cases/once-identities.zil");
const MUTABLE: &str = include_str!("../../../tools/language/221/cases/mut-owned.zil");
const DISCARD: &str = include_str!("../../../tools/language/221/cases/discard-and-eager.zil");
const PREPARE: &str = include_str!("../../../tools/language/221/cases/prepare-failure.zil");
const STORE: &str = include_str!("../../../tools/language/221/cases/callback-store.zil");

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "factory221-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn source_control() -> (TypedModule, CandidatePlan) {
    source_module("source-control", FN)
}

fn source_module(label: &str, text: &str) -> (TypedModule, CandidatePlan) {
    let path = directory(label);
    fs::write(path.join("source.zil"), text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "factory221.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    fs::write(path.join("core.txt"), format!("{core:#?}")).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
    fs::write(path.join("candidate.txt"), format!("{candidate:#?}")).unwrap();
    (module, candidate)
}

fn factory(module: &TypedModule) -> (usize, ClosureId, ClosureId) {
    module
        .functions
        .iter()
        .enumerate()
        .find_map(|(index, f)| {
            let Type::Closure(output) = f.return_type else {
                return None;
            };
            f.parameters
                .iter()
                .find_map(|p| match f.locals[p.0 as usize].ty {
                    Type::Closure(input) => Some((index, input, output)),
                    _ => None,
                })
        })
        .expect("source must instantiate an owning factory")
}

fn both(label: &str, module: &TypedModule, plan: &CandidatePlan, expected: Option<&str>) {
    let path = directory(label);
    let mut plan = plan.clone();
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    fs::write(path.join("input.wire"), &wire).unwrap();
    fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan, &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        match expected {
            Some(code) => {
                let errors =
                    result.expect_err("forged public factory candidate must fail independently");
                fs::write(path.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
                assert!(
                    errors
                        .iter()
                        .any(|e| e.phase == Phase::Verify && e.code == code),
                    "{label}: {errors:#?}"
                );
                assert!(errors.iter().all(|e| e.code != "E-VERIFY-PLAN-IDENTITY"
                    && e.category != ResultCategory::ResourceLimit));
            }
            None => {
                let verified = result.unwrap();
                fs::write(
                    path.join(format!("{route}.txt")),
                    format!("verified={}\n", verified.fingerprint()),
                )
                .unwrap();
                let lir = zil_lower::lower(&verified).unwrap();
                let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
                fs::write(path.join(format!("{route}.ll")), &llvm).unwrap();
                assert!(
                    llvm.contains("zil_env_"),
                    "actual inline closure types must reach LLVM"
                );
            }
        }
    }
}

#[test]
fn source_factory_reaches_independent_raw_wire_and_inline_machine_ir() {
    let (module, plan) = source_control();
    let (index, input, output) = factory(&module);
    assert_ne!(input, output);
    assert!(module.functions[index].callable_contract.is_none());
    assert_eq!(
        module.functions[index]
            .callable_return_contract
            .as_ref()
            .unwrap()
            .closure,
        output
    );
    both("valid-source", &module, &plan, None);
}

#[test]
fn source_derived_return_envelope_cannot_be_deleted_or_counterfeited() {
    let (base, plan) = source_control();
    let (index, input, _) = factory(&base);
    for mutation in 0..7 {
        let mut module = base.clone();
        let f = &mut module.functions[index];
        match mutation {
            0 => f.callable_return_contract = None,
            1 => {
                f.callable_contract = Some(CallableParameterContract {
                    roles: vec![],
                    traces: CallableTraces::Finite(vec![]),
                })
            }
            2 => f.callable_return_contract.as_mut().unwrap().cleanup.clear(),
            3 => f.callable_return_contract.as_mut().unwrap().kind = CallableKind::FnOnce,
            4 => f.callable_return_contract.as_mut().unwrap().hold.push(0),
            5 => f.parameters.push(f.parameters[0]),
            _ => {
                f.return_type = Type::Closure(input);
                f.callable_return_contract.as_mut().unwrap().closure = input;
            }
        }
        both(
            &format!("return-envelope-{mutation}"),
            &module,
            &plan,
            Some("E-VERIFY-CALLABLE-CONTRACT"),
        );
    }
}

#[test]
fn source_derived_environment_and_runtime_values_remain_independent() {
    let (base, plan) = source_control();
    let (index, input, output) = factory(&base);
    let mut module = base.clone();
    module.closures[output.0 as usize].captures[0] = Type::Closure(output);
    both(
        "recursive-output",
        &module,
        &plan,
        Some("E-VERIFY-CLOSURE-LAYOUT"),
    );

    let mut module = base.clone();
    module.closures[output.0 as usize]
        .captures
        .push(Type::Closure(input));
    let f = &mut module.functions[index];
    f.callable_return_contract.as_mut().unwrap().cleanup.push(1);
    let creation = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(
            |i| matches!(i.kind, InstructionKind::MakeClosure { closure, .. } if closure == output),
        )
        .unwrap();
    let InstructionKind::MakeClosure { captures, .. } = &mut creation.kind else {
        unreachable!()
    };
    captures.push(captures[0]);
    both(
        "duplicate-actual-input",
        &module,
        &plan,
        Some("E-VERIFY-RESOURCE"),
    );

    let mut module = base.clone();
    let entry = module.closures[output.0 as usize].function.0 as usize;
    let borrow = module.functions[entry]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::BorrowCapture { .. }))
        .unwrap();
    borrow.kind = InstructionKind::ReadCapture { index: 0 };
    both("copy-owned-child", &module, &plan, Some("E-VERIFY-CAPTURE"));

    let mut module = base;
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    for block in &mut main.blocks {
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::DropClosure { .. }));
    }
    both(
        "omit-caller-cleanup",
        &module,
        &plan,
        Some("E-VERIFY-RESOURCE"),
    );
}

#[test]
fn source_derived_raw_and_wire_work_has_exact_fresh_limits() {
    let (module, plan) = source_control();
    let path = directory("source-budgets");
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for serialized in [false, true] {
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if serialized {
                zil_verify::verify_module_bytes(&wire, &module.target, &limits)
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap().fingerprint();
        while low + 1 < high {
            let n = low + (high - low) / 2;
            match run(n) {
                Ok(actual) => {
                    assert_eq!(actual.fingerprint(), expected);
                    high = n;
                }
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit),
                        "{errors:#?}"
                    );
                    low = n;
                }
            }
        }
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        let below = run(high - 1).unwrap_err();
        assert!(
            below
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        fs::write(path.join(format!("wire-{serialized}.json")), format!("{{\"N\":{high},\"N_minus_1\":\"ResourceLimit\",\"fresh_N\":\"Pass\",\"fingerprint\":\"{expected}\"}}\n")).unwrap();
        fs::write(
            path.join(format!("wire-{serialized}-N-1.txt")),
            format!("{below:#?}"),
        )
        .unwrap();
        println!("source221 wire={serialized} N={high}; N-1 ResourceLimit; fresh N identical");
    }
}

#[test]
fn unused_factory_definitions_keep_declared_consuming_obligations() {
    for (label, body) in [
        ("twice", "return move()=>f()+f()"),
        ("consume-before-capture", "let n=f();return move()=>f()"),
        ("moved-alias", "let alias=f;return move()=>f()+alias()"),
        (
            "loop",
            "return move()=>{var n=0;while n<2{f();n+=1};return n}",
        ),
    ] {
        let source =
            format!("fn invalid<F:FnOnce()->i32>(f:F)->impl FnOnce()->i32{{{body}}}fn main(){{}}");
        let path = directory(label);
        fs::write(path.join("source.zil"), &source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "unused221.zil", source),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("compile.json"), report.to_json()).unwrap();
        assert_ne!(report.status, ResultCategory::Success, "{label}");
        assert!(
            matches!(report.phase, Phase::Expand | Phase::Types),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

fn build(label: &str, source: &str, opt: Optimization, project: bool) -> PathBuf {
    let path = directory(&format!("{label}-{opt:?}"));
    fs::write(path.join("main.zil"), source).unwrap();
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    let options = CompileOptions {
        backend,
        stop_after: Stage::Build,
        optimization: opt,
        output: Some(path.join("program")),
        emit_dir: Some(path.join("phases")),
        ..CompileOptions::default()
    };
    let report = if project {
        fs::write(
            path.join("api.zil"),
            include_str!("../../../tools/language/221/cases/cross-module/api.zil"),
        )
        .unwrap();
        compile_path(&path.join("main.zil"), &options)
    } else {
        compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "native221.zil", source),
            &options,
        )
    };
    fs::write(path.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let codegen = report
        .native_codegen
        .as_ref()
        .expect("an actual native build report");
    assert_eq!(codegen.backend, backend);
    assert!(
        codegen.fallback.is_none(),
        "no implicit backend substitution"
    );
    path
}

fn observe(
    program: &Path,
    path: &Path,
    label: &str,
    expected: &str,
    exit: i32,
    fail: Option<usize>,
) -> String {
    let mut command = Command::new(program);
    if let Some(fail) = fail {
        command.env("ZIL_TEST_FAIL_AT", fail.to_string());
    }
    let result = command.output().unwrap();
    fs::write(path.join(format!("{label}.stdout")), &result.stdout).unwrap();
    fs::write(path.join(format!("{label}.stderr")), &result.stderr).unwrap();
    let actual_exit = result
        .status
        .code()
        .map_or_else(|| "null".into(), |n| n.to_string());
    let fail_at = fail.map_or_else(|| "null".into(), |n| n.to_string());
    fs::write(
        path.join(format!("{label}.json")),
        format!(
            "{{\"actual_exit\":{actual_exit},\"expected_exit\":{exit},\"fail_at\":{fail_at}}}\n"
        ),
    )
    .unwrap();
    assert_eq!(result.status.code(), Some(exit), "{label}: {result:?}");
    assert_eq!(result.stdout, expected.as_bytes(), "{label}: {result:?}");
    String::from_utf8(result.stderr).unwrap()
}

fn probe(path: &Path, opt: Optimization) -> PathBuf {
    let source = include_str!("../../../tools/language/173/allocator_probe.c").replace(
        "#include \"../../research/142/allocator_probe.c\"",
        include_str!("../../../tools/research/142/allocator_probe.c"),
    );
    fs::write(path.join("allocator_probe.c"), source).unwrap();
    let output = path.join("probe");
    let result = Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
        .arg(format!("-{opt:?}"))
        .arg(path.join("phases/llvm.ll"))
        .arg(path.join("allocator_probe.c"))
        .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    fs::write(path.join("probe-link.stdout"), &result.stdout).unwrap();
    fs::write(path.join("probe-link.stderr"), &result.stderr).unwrap();
    assert!(result.status.success(), "{result:?}");
    output
}

#[test]
fn native_owning_modes_and_distinct_values_clean_exactly_once_without_allocation() {
    for (label, source, expected) in [
        ("fn-owned", FN, "XAB"),
        ("once-identities", ONCE, "AXB"),
        ("mut-owned", MUTABLE, "XC"),
        ("discard-and-eager", DISCARD, "ABX"),
    ] {
        for opt in [Optimization::O0, Optimization::O2] {
            let path = build(label, source, opt, false);
            assert!(
                observe(&path.join("program"), &path, "ordinary", expected, 0, None).is_empty()
            );
            let observer = probe(&path, opt);
            let allocation = observe(&observer, &path, "instrumented", expected, 0, Some(0));
            assert_eq!(
                allocation.trim(),
                "{\"alloc_calls\":0,\"allocated\":0,\"frees\":0,\"failures\":0,\"live\":0,\"peak_live\":0,\"peak_bytes\":0}"
            );
        }
    }
}

#[test]
fn native_cross_module_nested_factory_keeps_complete_input_identity() {
    let source = include_str!("../../../tools/language/221/cases/cross-module/main.zil");
    for opt in [Optimization::O0, Optimization::O2] {
        let path = build("cross-module", source, opt, true);
        assert!(observe(&path.join("program"), &path, "ordinary", "XA", 0, None).is_empty());
    }
}

#[test]
fn native_argument_preparation_failure_cleans_the_prepared_input() {
    for opt in [Optimization::O0, Optimization::O2] {
        let path = build("prepare-failure", PREPARE, opt, false);
        assert!(observe(&path.join("program"), &path, "ordinary", "PSAX", 0, None).is_empty());
        let observer = probe(&path, opt);
        for fail in 0..=2 {
            let failed = fail == 1;
            let allocation = observe(
                &observer,
                &path,
                &format!("fail-{fail}"),
                if failed { "PAX" } else { "PSAX" },
                0,
                Some(fail),
            );
            let expected = if failed {
                "{\"alloc_calls\":1,\"allocated\":0,\"frees\":0,\"failures\":1,\"live\":0,\"peak_live\":0,\"peak_bytes\":0}"
            } else {
                "{\"alloc_calls\":1,\"allocated\":1,\"frees\":1,\"failures\":0,\"live\":0,\"peak_live\":1,\"peak_bytes\":32}"
            };
            assert_eq!(allocation.trim(), expected);
        }
    }
}

#[test]
fn native_callback_store_uses_inline_wrapper_stride_and_cleans_each_failure() {
    for opt in [Optimization::O0, Optimization::O2] {
        let path = build("callback-store", STORE, opt, false);
        assert!(observe(&path.join("program"), &path, "ordinary", "AB", 0, None).is_empty());
        let observer = probe(&path, opt);
        for fail in 0..=3 {
            let (stdout, exit, calls, allocated, failures, peak) = match fail {
                1 => ("BA", 1, 1, 0, 1, 0),
                2 => ("AB", 2, 2, 1, 1, 80),
                _ => ("AB", 0, 2, 1, 0, 160),
            };
            let allocation = observe(
                &observer,
                &path,
                &format!("fail-{fail}"),
                stdout,
                exit,
                Some(fail),
            );
            let expected = format!(
                "{{\"alloc_calls\":{calls},\"allocated\":{allocated},\"frees\":{allocated},\"failures\":{failures},\"live\":0,\"peak_live\":{allocated},\"peak_bytes\":{peak}}}"
            );
            assert_eq!(allocation.trim(), expected);
        }
    }
}

#[test]
fn extended_signature_controls_keep_raw_wire_and_native_contracts() {
    for (label, source, expected) in [
        (
            "address-signatures",
            include_str!("../../../tools/language/221/cases/address-signatures.zil"),
            "",
        ),
        (
            "copy-error-result",
            include_str!("../../../tools/language/221/cases/copy-error-result.zil"),
            "ABXCDY",
        ),
        (
            "lending-child",
            include_str!("../../../tools/language/221/cases/lending-child.zil"),
            "",
        ),
    ] {
        let (module, plan) = source_module(label, source);
        let (_, input, output) = factory(&module);
        match label {
            "address-signatures" => {
                let factories = module.functions.iter().filter(|f| {
                    matches!(f.return_type, Type::Closure(_))
                        && f.parameters
                            .iter()
                            .any(|p| matches!(f.locals[p.0 as usize].ty, Type::Closure(_)))
                });
                assert_eq!(factories.count(), 3);
                assert!(matches!(
                    module.closures[input.0 as usize].parameters[0],
                    Type::Address { .. }
                ));
            }
            "copy-error-result" => {
                assert!(matches!(
                    module.closures[input.0 as usize].result,
                    Type::ErrorUnion { .. }
                ));
                assert!(module.closures[input.0 as usize].result.is_copy_value());
                assert_eq!(module.closures[input.0 as usize].kind, CallableKind::Fn);
                assert_eq!(
                    module.closures[output.0 as usize].kind,
                    CallableKind::FnOnce
                );
            }
            "lending-child" => {
                assert!(module.closures[input.0 as usize].result.is_copy_value());
                let child = module.closures[input.0 as usize]
                    .captures
                    .iter()
                    .find_map(|ty| match ty {
                        Type::Closure(id) => Some(*id),
                        _ => None,
                    })
                    .expect("an actual inner lending environment");
                assert!(matches!(
                    module.closures[child.0 as usize].result,
                    Type::Address { .. }
                ));
                assert!(
                    !module.closures[child.0 as usize]
                        .lending_captures
                        .is_empty()
                );
            }
            _ => unreachable!(),
        }
        both(label, &module, &plan, None);
        for opt in [Optimization::O0, Optimization::O2] {
            let path = build(label, source, opt, false);
            assert!(
                observe(&path.join("program"), &path, "ordinary", expected, 0, None).is_empty()
            );
        }
    }
}
