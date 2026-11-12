//! Generic Product method lookup follows the complete resolved receiver.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/iterator-adapters/product-methods.zil");
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "product-methods.zil", text)
}

#[test]
fn actual_product_receiver_reaches_both_kernels_and_native_cleanup() {
    let syntax = zil_syntax::parse(&unit(SOURCE)).into_result().unwrap();
    let core = zil_expand::expand(&unit(SOURCE), &syntax).unwrap();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &typed.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (typed.clone(), plan)
    );
    zil_verify::verify_module_bytes(&wire, &typed.target, &limits).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let dir = root.join(format!(
                "product-methods-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("main.zil"), SOURCE).unwrap();
            let executable = dir.join("program");
            let report = compile_source(
                &unit(SOURCE),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(executable.clone()),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            let out = Command::new(executable).output().unwrap();
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            assert_eq!(out.stdout, b"BA");
            assert!(out.stderr.is_empty());
            if std::env::var_os("ZIL_TEST_REPORT_DIR").is_none() {
                fs::remove_dir_all(dir).unwrap()
            }
        }
    }
}

#[test]
fn raw_and_wire_reject_a_same_layout_different_product_method_receiver() {
    use zil_ir::{InstructionKind, Type};
    let source = format!(
        "{SOURCE}\npackage Other{{manages mark:Mark;value:i32}}\nfn other()->i32{{let values:Store<Other>=store_new();if !store_reserve(values,1){{return 1}}store_push(values,Other{{mark:Mark{{code:67}},value:4}});var cursor=drain::<Other>(give values);match advance(&mut cursor){{.ok(value)=>{{return value.value-4}},.err(exhausted)=>{{return 2}}}}}}\n"
    );
    let syntax = zil_syntax::parse(&unit(&source)).into_result().unwrap();
    let core = zil_expand::expand(&unit(&source), &syntax).unwrap();
    let limits = Limits::default();
    let mut typed = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let mut plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let methods = typed
        .functions
        .iter()
        .filter(|f| f.name.ends_with(".next"))
        .collect::<Vec<_>>();
    assert_eq!(methods.len(), 2);
    assert_ne!(methods[0].return_type, methods[1].return_type);
    let (Type::ErrorUnion { result: first, .. }, Type::ErrorUnion { result: second, .. }) =
        (&methods[0].return_type, &methods[1].return_type)
    else {
        panic!("owning results required")
    };
    let (
        Type::Package {
            name: a,
            fields: af,
        },
        Type::Package {
            name: b,
            fields: bf,
        },
    ) = (first.as_ref(), second.as_ref())
    else {
        panic!("owning packages required")
    };
    assert_ne!(a, b);
    assert_eq!(
        af, bf,
        "the nominal counterexample deliberately has equal layout"
    );
    let (first, second) = (methods[0].id, methods[1].id);
    let mut changed = 0;
    for f in &mut typed.functions {
        for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::Call { function, .. } = &mut i.kind
                && *function == first
            {
                *function = second;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    // Keep the plan identity current, forcing checks of the actual call.
    plan.module_fingerprint = typed.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(typed.clone(), plan, &typed.target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &typed.target, &limits).unwrap_err(),
    ] {
        assert!(
            errors.iter().any(|e| e.code == "E-VERIFY-CALL"),
            "{errors:#?}"
        );
    }
}

#[test]
fn canonical_receiver_index_has_a_fresh_exact_types_budget() {
    let syntax = zil_syntax::parse(&unit(SOURCE)).into_result().unwrap();
    let core = zil_expand::expand(&unit(SOURCE), &syntax).unwrap();
    let run = |n| {
        zil_types::check(
            &core,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let expected = run(Limits::default().max_analysis_steps).unwrap();
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = middle;
            }
            Err(errors) => {
                assert!(!errors.is_empty());
                assert!(
                    errors
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                low = middle;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    assert!(
        run(high - 1)
            .unwrap_err()
            .iter()
            .all(|e| e.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(high).unwrap(), expected);
    println!("canonical Product receiver Types N={high}; N-1 rejected; fresh N equal");
}
