//! Actual complete Enum continuation source, machine and request controls.
use super::*;
const OWNED_ENUM861: &str = include_str!("owned-enum-saved.zil");

#[test]
fn g861_complete_enum_real_source_reaches_analyze() {
    let cancelled = OWNED_ENUM861.replace(
        "    drive(&mut task);\n    drive(&mut task);",
        "    drive(&mut task);",
    );
    let pc0 = OWNED_ENUM861.replace("    drive(&mut task);\n    drive(&mut task);\n", "");
    let empty = OWNED_ENUM861.replace("Packet.Item(give token)", "Packet.Empty");
    for input in [
        OWNED_ENUM861,
        &cancelled,
        &pc0,
        &empty,
        include_str!("owned-enum-failed-prepare.zil"),
        include_str!("owned-enum-prepared.zil"),
    ] {
        let report = compile_source(
            &source(input),
            &CompileOptions {
                stop_after: zil_driver::Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn g861_complete_enum_scope_fences_stay_closed() {
    let copy_error_result = OWNED_ENUM861
        .replace(
            "fn worker(give token:Token)->i32",
            "fn worker(give token:Token)->TaskPollState!i32",
        )
        .replace("Task<i32>", "Task<TaskPollState!i32>");
    let enum_input = OWNED_ENUM861
        .replace("fn worker(give token:Token)", "fn worker(packet:Packet)")
        .replace("    let packet=Packet.Item(give token);\n", "")
        .replace(
            "task_create(worker,give input)",
            "task_create(worker,Packet.Item(give input))",
        );
    for (input, phase) in [
        (include_str!("owned-enum-task.zil"), zil_base::Phase::Expand),
        (
            include_str!("owned-result-task.zil"),
            zil_base::Phase::Expand,
        ),
        (copy_error_result.as_str(), zil_base::Phase::Analyze),
        (enum_input.as_str(), zil_base::Phase::Analyze),
    ] {
        let report = compile_source(&source(input), &CompileOptions::default());
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn g861_complete_enum_native_whole_slot_lifecycle() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let pair = include_str!("owned-enum-two-leaves.zil");
    let prepared = include_str!("owned-enum-prepared.zil");
    let empty = OWNED_ENUM861
        .replace(
            "fn worker(give token:Token)->i32 {",
            "fn discard(give token:Token){}fn worker(give token:Token)->i32 {",
        )
        .replace(
            "let packet=Packet.Item(give token);",
            "let packet=Packet.Empty;discard(give token);",
        );
    let inputs = [
        ("complete", OWNED_ENUM861.to_owned(), "PAR", 0),
        (
            "cancel",
            OWNED_ENUM861.replace(
                "    drive(&mut task);\n    drive(&mut task);",
                "    drive(&mut task);",
            ),
            "PA",
            0,
        ),
        (
            "pc0",
            OWNED_ENUM861.replace("    drive(&mut task);\n    drive(&mut task);\n", ""),
            "A",
            0,
        ),
        ("empty-complete", empty.clone(), "APR", 0),
        (
            "empty-cancel",
            empty.replace(
                "    drive(&mut task);\n    drive(&mut task);",
                "    drive(&mut task);",
            ),
            "AP",
            0,
        ),
        ("pair-complete", pair.to_owned(), "PABR", 0),
        (
            "pair-cancel",
            pair.replace(
                "    drive(&mut task);\n    drive(&mut task);",
                "    drive(&mut task);",
            ),
            "PAB",
            0,
        ),
        (
            "pair-pc0",
            pair.replace("    drive(&mut task);\n    drive(&mut task);\n", ""),
            "BA",
            0,
        ),
        ("prepared-complete", prepared.to_owned(), "PPAR", 0),
        (
            "prepared-inner-cancel",
            prepared.replace(
                "    drive(&mut task);\n    drive(&mut task);\n    drive(&mut task);",
                "    drive(&mut task);",
            ),
            "PA",
            0,
        ),
        (
            "prepared-outer-cancel",
            prepared.replace(
                "    drive(&mut task);\n    drive(&mut task);\n    drive(&mut task);",
                "    drive(&mut task);\n    drive(&mut task);",
            ),
            "PPA",
            0,
        ),
        (
            "failed-later-prepare",
            include_str!("owned-enum-failed-prepare.zil").to_owned(),
            "BA",
            1,
        ),
    ];
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, input, expected, exit) in &inputs {
                let dir = std::env::temp_dir().join(format!(
                    "zil861-enum-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(input),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{case}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none(), "{native:?}");
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(*exit), "{case}: {actual:?}");
                assert_eq!(actual.stdout, expected.as_bytes(), "{case}: {actual:?}");
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn g861_complete_enum_request_budgets_and_semantic_cache() {
    use zil_driver::{CompilationSession, Stage};
    let bounds = |ledger: &str| {
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                "bytes" => options.limits.max_source_bytes = n,
                "depth" => options.limits.max_call_depth = n,
                _ => unreachable!(),
            }
            options
        };
        let limits = Limits::default();
        let (mut low, mut high) = (
            1,
            match ledger {
                "work" => limits.max_analysis_steps,
                "nodes" => limits.max_nodes,
                "bytes" => limits.max_source_bytes,
                "depth" => limits.max_call_depth,
                _ => unreachable!(),
            },
        );
        let input = source(OWNED_ENUM861);
        assert_eq!(
            compile_source(&input, &options(high)).status,
            ResultCategory::Success
        );
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&input, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{ledger}/{n}: {}", report.to_json()),
            }
        }
        let mut warm = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                warm.compile_source(&input, &options(n)),
                CompilationSession::default().compile_source(&input, &options(n)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!("G8.6.1 whole Source {ledger} N={low}/N-1/cold/warm/fresh");
    };
    for ledger in ["work", "nodes", "bytes", "depth"] {
        bounds(ledger);
    }
    let dir = std::env::temp_dir().join(format!("zil861-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut session = CompilationSession::default();
    let pair = include_str!("owned-enum-two-leaves.zil");
    let cancelled = pair.replace(
        "    drive(&mut task);\n    drive(&mut task);",
        "    drive(&mut task);",
    );
    for (ordinal, (text, expected)) in [
        (OWNED_ENUM861, "PAR"),
        (OWNED_ENUM861, "PAR"),
        (pair, "PABR"),
        (cancelled.as_str(), "PAB"),
        (OWNED_ENUM861, "PAR"),
    ]
    .into_iter()
    .enumerate()
    {
        let report = session.compile_source(
            &source(text),
            &CompileOptions {
                backend: zil_driver::NativeBackend::Clang,
                stop_after: Stage::Build,
                output: Some(dir.join(format!("program-{ordinal}"))),
                semantic_cache: Some(dir.join("cache")),
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
        assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
        let observed = std::process::Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(observed.status.code(), Some(0));
        assert_eq!(observed.stdout, expected.as_bytes());
    }
    let store = dir.join("cache/semantic-v4");
    let mut candidates = 0;
    for entry in std::fs::read_dir(&store).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "candidate") {
            continue;
        }
        let key =
            zil_base::Fingerprint::from_hex(path.file_stem().unwrap().to_str().unwrap()).unwrap();
        let zil_db::CacheRead::Candidate(payload) =
            zil_db::ArtifactCache::with_limit(&store, Limits::default().max_source_bytes)
                .inspect_candidate(key)
                .unwrap()
        else {
            panic!("missing actual semantic candidate")
        };
        assert_eq!(&payload[..8], b"ZILSEM04");
        let length = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
        let bytes = &payload[12..12 + length];
        let (typed, plan) =
            zil_ir::wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        zil_verify::verify_module(typed.clone(), plan, &typed.target, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(bytes, &typed.target, &Limits::default()).unwrap();
        candidates += 1;
    }
    assert!(candidates > 0);
    eprintln!(
        "G8.6.1 warm/change/cancel/revert native and {candidates} actual cache raw/Wire controls"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn g861_complete_enum_kernel_current_request_boundaries() {
    let defaults = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(OWNED_ENUM861, &defaults).unwrap().core;
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
    for phase in [
        zil_base::Phase::Types,
        zil_base::Phase::Analyze,
        zil_base::Phase::Verify,
    ] {
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..defaults.clone()
            };
            match phase {
                zil_base::Phase::Types => zil_types::check(&core, &target, &limits).map(|_| ()),
                zil_base::Phase::Analyze => zil_analyze::analyze(&typed, &limits).map(|_| ()),
                zil_base::Phase::Verify => {
                    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
                        .map(|_| ())
                }
                _ => unreachable!(),
            }
        };
        let (mut low, mut high) = (1, defaults.max_analysis_steps);
        run(high).unwrap();
        while low < high {
            let n = low + (high - low) / 2;
            if run(n).is_ok() {
                high = n;
            } else {
                low = n + 1;
            }
        }
        run(low).unwrap();
        let errors = run(low - 1).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{phase:?}: {errors:?}"
        );
        assert_eq!(errors[0].phase, phase, "{errors:?}");
        run(low).unwrap();
        eprintln!("G8.6.1 actual Source kernel {phase:?} N={low}/N-1/fresh");
    }
}

#[test]
fn g861_complete_enum_actual_backend_growth() {
    let source_for = |width: usize| {
        let fields = std::iter::repeat_n("manages Token", width)
            .collect::<Vec<_>>()
            .join(",");
        let parameters = (0..width)
            .map(|n| format!("give a{n}:Token"))
            .collect::<Vec<_>>()
            .join(",");
        let arguments = (0..width)
            .map(|n| format!("give a{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let bindings = (0..width)
            .map(|n| format!("x{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let inputs = (0..width)
            .map(|n| format!("let a{n}=Token{{code:65}};"))
            .collect::<String>();
        format!(
            "extern \"C\" fn putchar(n:i32)->i32;resource Token{{code:i32}}impl Drop for Token{{fn drop(self:Self){{unsafe{{putchar(.code);}}}}}}enum Packet{{Empty,Many({fields})}}fn worker({parameters})->i32{{let packet=Packet.Many({arguments});task_yield();match packet{{Packet.Empty()=>{{return 0;}},Packet.Many({bindings})=>{{return 0;}}}}}}fn main()->TaskCreateError!i32{{{inputs}var task=task_create(worker,{arguments})?;task_poll(&mut task);task_request_cancel(&mut task);task_poll_close(&mut task);task_finish_destroy(give task);return 0;}}"
        )
    };
    let defaults = Limits::default();
    let target = zil_base::TargetProfile::default();
    let mut growth = Vec::new();
    for width in [2, 4, 8] {
        let text = source_for(width);
        let typed =
            zil_types::check(&expand(&text, &defaults).unwrap().core, &target, &defaults).unwrap();
        let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
        let verified =
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &defaults).unwrap();
        let worker = typed
            .functions
            .iter()
            .find(|f| f.name == "worker")
            .unwrap()
            .id;
        assert!(
            verified
                .task_ownership(worker)
                .unwrap()
                .actions()
                .any(|actions| actions.after().len() >= width)
        );
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..defaults.clone()
            };
            let verified =
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)?;
            let lir = zil_lower::lower(&verified)?;
            zil_backend_llvm::emit_with_options(
                &verified,
                &lir,
                &zil_backend_llvm::EmitOptions {
                    entry: zil_backend_llvm::EntryPoint::Main,
                },
            )
            .map(|_| ())
        };
        let (mut low, mut high) = (1, defaults.max_analysis_steps);
        run(high).unwrap();
        while low < high {
            let n = low + (high - low) / 2;
            if run(n).is_ok() {
                high = n;
            } else {
                low = n + 1;
            }
        }
        run(low).unwrap();
        let errors = run(low - 1).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{errors:?}"
        );
        assert_eq!(errors[0].phase, zil_base::Phase::Codegen, "{errors:?}");
        run(low).unwrap();
        growth.push(low);
        eprintln!("G8.6.1 complete Enum {width} leaves Codegen N={low}/N-1/fresh");
    }
    assert!(growth[0] < growth[1] && growth[1] < growth[2], "{growth:?}");
}
