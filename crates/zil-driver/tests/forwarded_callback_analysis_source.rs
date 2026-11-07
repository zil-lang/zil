//! Real source -> Types -> Analyze. Verify/native are separate slice evidence.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "253.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn checked(text: &str) -> TypedModule {
    let module = typed(text);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let raw = zil_ir::wire::decode_module_candidate(&wire, &limits)
        .unwrap()
        .0;
    assert_eq!(zil_analyze::analyze(&raw, &limits).unwrap(), plan);
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir =
            std::path::PathBuf::from(root).join(format!("source-{}", module.source_fingerprint));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.zil"), text).unwrap();
        std::fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
        std::fs::write(dir.join("candidate.txt"), format!("{plan:#?}")).unwrap();
        std::fs::write(dir.join("candidate.wire"), wire).unwrap();
    }
    module
}
#[test]
fn original_and_copy_and_mutable_sources_reach_analyze() {
    for source in [
        include_str!("forwarded_callback_types_fixtures/original.zil"),
        "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}fn twice<F:Fn(i32)->i32>(f:F)->i32{let current=keep(f);return current(1)+current(2)}fn main()->i32{let offset=3;let f=move(n:i32)=>n+offset;let current=keep(f);return current(1)+current(2)+f(3)+twice(f)-24}",
        "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}fn step<F:FnMut(i32)->i32>(f:F)->F{var current=f;current(1);return keep(current)}fn main()->i32{var count=0;var f=move(n:i32)=>{count+=n;return count};var current=step(f);return current(2)-3}",
    ] {
        checked(source);
    }
}

#[test]
fn six_frozen_callback_signatures_reach_analyze() {
    for (name, source) in [
        (
            "fn-copy",
            include_str!("../../../tests/callable-instance-forwarding/frozen-prep/probes/fn-copy.zil"),
        ),
        (
            "fnmut-copy",
            include_str!("../../../tests/callable-instance-forwarding/frozen-prep/probes/fnmut-copy.zil"),
        ),
        (
            "fnonce-copy",
            include_str!("../../../tests/callable-instance-forwarding/frozen-prep/probes/fnonce-copy.zil"),
        ),
        (
            "fn-owned",
            include_str!("../../../tests/callable-instance-forwarding/frozen-prep/probes/fn-owned.zil"),
        ),
        (
            "fnmut-owned",
            include_str!("../../../tests/callable-instance-forwarding/frozen-prep/probes/fnmut-owned.zil"),
        ),
        (
            "fnonce-owned",
            include_str!("../../../tests/callable-instance-forwarding/frozen-prep/probes/fnonce-owned.zil"),
        ),
    ] {
        let module = typed(source);
        zil_analyze::analyze(&module, &Limits::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
}

#[test]
fn concrete_caller_can_replace_an_owning_event_callback_through_keep_in_a_loop() {
    checked(include_str!(
        "forwarded_callback_analysis_fixtures/owned_event_caller.zil"
    ));
}

#[test]
fn source_forward_has_an_exact_fresh_request_budget() {
    let module = typed(include_str!(
        "forwarded_callback_types_fixtures/original.zil"
    ));
    let defaults = Limits::default();
    let expected = zil_analyze::analyze(&module, &defaults).unwrap();
    let (mut low, mut high) = (0, defaults.max_analysis_steps);
    while low + 1 < high {
        let steps = low + (high - low) / 2;
        let r = zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            },
        );
        match r {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = steps;
            }
            Err(ds) => {
                assert!(
                    ds.iter()
                        .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
                );
                low = steps;
            }
        }
    }
    for steps in [high, high - 1, high] {
        let r = zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            },
        );
        if steps == high {
            assert_eq!(r.unwrap(), expected);
        } else {
            assert!(
                r.unwrap_err()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }
    let count = module
        .functions
        .iter()
        .filter(|f| f.callable_return_contract.is_some())
        .count();
    println!(
        "253 original source Analyze N={high}; Parameter contracts={count}; N-1 ResourceLimit only; fresh N identical"
    );
}

#[test]
fn fn_parent_can_forward_through_a_called_fnmut_child_and_recover_fn() {
    checked(
        "fn step<F:FnMut(i32)->i32>(f:F)->F{var current=f;current(1);return current} fn relay<F:Fn(i32)->i32>(f:F)->F{return step(f)} fn main()->i32{let f=(n:i32)=>n+1;let current=relay(f);return current(2)-3}",
    );
}

#[test]
fn source_derived_cleanup_and_return_metadata_cannot_discharge_or_replace_the_input() {
    let source = "resource R{n:i32} package Data{manages bytes:ByteBuffer} fn keep<F:FnOnce(i32)->i32>(data:Data,f:F)->F{return f} fn main()->i32{let r=R{n:3};let f=move(n:i32)=>r.n+n;let current=keep(Data{bytes:bytes_new()},f);return current(1)-4}";
    let module = checked(source);
    let helper = module
        .functions
        .iter()
        .position(|f| {
            f.callable_return_contract
                .as_ref()
                .is_some_and(|c| matches!(c.source, CallableReturnSource::Parameter(_)))
        })
        .unwrap();
    let mut missing = module.clone();
    let mut removed = 0;
    for block in &mut missing.functions[helper].blocks {
        let old = block.instructions.len();
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
        removed += old - block.instructions.len();
    }
    assert!(removed > 0);
    assert!(zil_analyze::analyze(&missing, &Limits::default()).is_err());
    let mut wrong = module.clone();
    wrong.functions[helper]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .source = CallableReturnSource::Parameter(0);
    assert!(zil_analyze::analyze(&wrong, &Limits::default()).is_err());
    let mut wrong = module;
    wrong.functions[helper]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[0]
        .cleanup = CallableCleanup::Owned;
    assert!(zil_analyze::analyze(&wrong, &Limits::default()).is_err());
}

#[test]
fn frozen_cross_module_source_reaches_analyze_without_verify() {
    checked_project("frozen-prep/project");
}
#[test]
fn extended_store_address_and_owning_loop_source_reaches_analyze_without_verify() {
    checked_project("extended");
}
fn checked_project(relative: &str) {
    use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/callable-instance-forwarding")
        .join(relative);
    let names = ["main", "model", "relay"];
    let project = ProjectSources {
        root: ModuleId(0),
        modules: names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let path = root.join(format!("{name}.zil"));
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    path.to_string_lossy().into_owned(),
                    std::fs::read_to_string(path).unwrap(),
                )
            })
            .collect(),
        paths: [
            (vec!["model".into()], ModuleId(1)),
            (vec!["relay".into()], ModuleId(2)),
        ]
        .into_iter()
        .collect(),
    };
    let report_dir = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .map(|root| root.join(relative.replace('/', "-")));
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Analyze,
            emit_dir: report_dir.as_ref().map(|dir| dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    if let Some(dir) = report_dir {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("report.json"), report.to_json()).unwrap();
    }
    assert_eq!(
        report.status,
        zil_base::ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn private_replacement_slots_need_no_helper_edge_even_in_a_closure_body() {
    let source = include_str!("forwarded_callback_types_fixtures/nested_original.zil");
    checked(source);
    // Keep the actual inner instance self-contained while exercising direct
    // storage rather than an Array1. Moving it must invalidate the old loan.
    let stale = source
        .replace("f=f;", "let view=&f;let moved=f;")
        .replace("let p=f(Event", "let p=view(Event");
    assert_ne!(source, stale);
    let module = typed(&stale);
    let error = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        format!("{error:?}").contains("invalidates a live address window"),
        "{error:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(
        &module,
        &CandidatePlan::empty_for(&module),
        &Limits::default(),
    )
    .unwrap();
    let raw = zil_ir::wire::decode_module_candidate(&wire, &Limits::default())
        .unwrap()
        .0;
    assert!(zil_analyze::analyze(&raw, &Limits::default()).is_err());
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir = std::path::PathBuf::from(root).join("nested-stale-source");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("source.zil"), &stale).unwrap();
        std::fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
        std::fs::write(dir.join("diagnostic.txt"), format!("{error:#?}")).unwrap();
        std::fs::write(dir.join("untrusted.wire"), wire).unwrap();
    }

    let valid = source
        .replace("f=f;", "let view=&f;")
        .replace("let p=f(Event", "let p=view(Event");
    checked(&valid);
}
