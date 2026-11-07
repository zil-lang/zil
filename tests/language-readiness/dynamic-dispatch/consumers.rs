//! Source-to-native dyn consumers and source-derived independent raw/Wire controls.
use super::*;
use std::io::Write;
use std::process::Stdio;
use zil_base::TargetProfile;
use zil_driver::{CodegenOptions, CompilationSession, DiskCacheStatus, NativeBackend};
use zil_ir::{InstructionKind as I, Type};

const TWO: &str = include_str!("dyn-two-provider.zil");
const INPUT: &str = include_str!("dyn-input.zil");
const WIDE_ARGUMENT: &str = include_str!("dyn-wide-argument.zil");
const UNIT_RESULT: &str = include_str!("dyn-unit-result.zil");

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "shared_dynamic_dispatch-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile(text: &str, options: &CompileOptions) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "shared_dynamic_dispatch.zil", text),
        options,
    )
}

fn backends() -> Vec<NativeBackend> {
    let mut result = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        result.push(NativeBackend::LlvmApi);
    }
    result
}

#[test]
fn runtime_input_selects_two_real_vtables_in_both_backends() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("input");
            let executable = dir.join("program");
            let report = compile(
                INPUT,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
            assert_eq!(typed.matches("MakeDynBorrow {").count(), 2);
            assert!(typed.contains("CallDynSlot {"));
            for (input, expected) in [(b"1\n".as_slice(), b"20\n".as_slice()), (b"0\n", b"22\n")] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(result.status.code(), Some(0));
                assert_eq!(result.stdout, expected);
                assert!(result.stderr.is_empty(), "{result:?}");
            }
        }
    }
}

#[test]
fn indirect_scalar_argument_preserves_full_u64_result_in_both_backends() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("wide-argument");
            let executable = dir.join("program");
            let report = compile(
                WIDE_ARGUMENT,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1, "{}", report.to_json());
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
            assert_eq!(typed.matches("MakeDynBorrow {").count(), 2);
            assert!(typed.contains("CallDynSlot {") && typed.contains("arguments: ["));
            for (input, expected) in [
                (b"1\n".as_slice(), b"18446744073709551615\n".as_slice()),
                (b"0\n", b"42\n"),
            ] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(result.status.code(), Some(0), "{result:?}");
                assert_eq!(result.stdout, expected, "{result:?}");
                assert!(result.stderr.is_empty(), "{result:?}");
            }
        }
    }
}

#[test]
fn unit_result_slot_remains_outside_the_body_proven_profile() {
    let report = compile(UNIT_RESULT, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(report.commands.is_empty());
    assert_eq!(report.verification_runs, 0);
}

fn project_files(main: &'static str) -> [(&'static str, &'static str); 3] {
    [
        ("main", main),
        ("a", include_str!("project/a.zil")),
        ("b", include_str!("project/b.zil")),
    ]
}

#[test]
fn cross_module_default_provider_and_nominal_interface_separation() {
    let files = project_files(include_str!("project/main.zil"));
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("project");
            let executable = dir.join("program");
            let options = CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                entry: Some("main".into()),
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                cache_dir: Some(dir.join("parsed-cache")),
                ..CompileOptions::default()
            };
            for warm in [false, true] {
                let report = compile_project(&project(&files), &options);
                fs::write(
                    dir.join(if warm { "warm.json" } else { "cold.json" }),
                    report.to_json(),
                )
                .unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1, "{}", report.to_json());
                if warm {
                    assert_eq!(
                        report.disk_cache,
                        DiskCacheStatus::Hit,
                        "{}",
                        report.to_json()
                    );
                }
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let result = Command::new(&executable).output().unwrap();
                assert_eq!(result.status.code(), Some(0));
                assert_eq!(result.stdout, b"29\n");
                assert!(result.stderr.is_empty());
            }
        }
    }
    let wrong = project_files(include_str!("project/wrong-interface.zil"));
    let report = compile_project(&project(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Expand | Phase::Types),
        "{}",
        report.to_json()
    );
}

#[test]
fn explicit_local_coercion_and_live_receiver_conflicts() {
    let explicit = TWO.replace(
        "return read_it(&a)+read_it(&b)",
        "let d:&dyn Read=&a;return read_it(d)+read_it(&b)",
    );
    let report = compile(&explicit, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let conflicting = TWO.replace(
        "fn main()->i32{let a=A{n:20};let b=B{n:22};return read_it(&a)+read_it(&b)}",
        "fn main()->i32{var a=A{n:20};let d:&dyn Read=&a;a=A{n:21};return read_it(d)}",
    );
    assert_ne!(conflicting, TWO);
    let report = compile(&conflicting, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
}

fn source_candidate() -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    // This fixture has no CT expressions. Use the same closed-project identity
    // and program expansion entry as Driver, so the original trait/impl origins
    // survive specialization instead of manufacturing a raw Core inventory.
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "shared_dynamic_dispatch.zil", INPUT);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let aliases = BTreeMap::new();
    let expanded = zil_expand::expand_program_with_limits_target_context(
        std::slice::from_ref(&unit),
        &[&syntax],
        &[Vec::new()],
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &aliases,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    let core = zil_ir::CoreProgram {
        root: unit.module_id,
        source_fingerprint: unit.fingerprint(),
        modules: expanded.into_iter().map(|(module, _)| module).collect(),
    };
    let typed = zil_types::check_program(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze_program(&typed, &limits).unwrap();
    let module = typed.module;
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn rejected(
    mut module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    mutate: impl FnOnce(&mut zil_ir::TypedModule),
) {
    mutate(&mut module);
    plan.module_fingerprint = module.fingerprint();
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    let target = module.target.clone();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for errors in [
        zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors
                .iter()
                .any(|e| e.phase == Phase::Verify && e.code == "E-VERIFY-DYN"),
            "{errors:?}"
        );
    }
}

#[test]
fn source_derived_raw_wire_contract_identity_and_slot_controls() {
    let (module, plan) = source_candidate();
    let target = TargetProfile::host_llvm();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default()).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).unwrap();
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_interfaces[0].slots[0].contract.effects = None
    });
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_vtables[0].self_type = m.dyn_vtables[1].self_type.clone()
    });
    rejected(module.clone(), plan.clone(), |m| {
        let instruction = m
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, I::CallDynSlot { .. }))
            .unwrap();
        if let I::CallDynSlot { slot, .. } = &mut instruction.kind {
            *slot = u32::MAX;
        }
    });
    // The raw table is a declaration of its selected target, not a certificate
    // that an arbitrary raw program still implements the original Source.
    let mut other = module.clone();
    let spare = other
        .functions
        .iter()
        .find(|f| f.name.ends_with("spare"))
        .unwrap()
        .id;
    assert!(
        other
            .dyn_vtables
            .iter()
            .all(|t| t.entries[0].function != spare)
    );
    let table = other
        .dyn_vtables
        .iter_mut()
        .find(|t| matches!(&t.self_type,Type::Record{ name,.. } if name.ends_with('A')))
        .unwrap();
    table.entries[0].function = spare;
    let mut other_plan = plan;
    other_plan.module_fingerprint = other.fingerprint();
    zil_analyze::analyze(&other, &Limits::default()).unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&other, &other_plan, &Limits::default()).unwrap();
    zil_verify::verify_module(other, other_plan, &target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).unwrap();
}

#[test]
fn dyn_request_work_nodes_and_cache_recheck_current_verify() {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "shared_dynamic_dispatch-budget.zil", INPUT);
    let baseline = CompileOptions::default();
    let initial = CompilationSession::default().compile_source(&unit, &baseline);
    assert_eq!(
        initial.status,
        ResultCategory::Success,
        "{}",
        initial.to_json()
    );
    assert_eq!(initial.verification_runs, 1);
    for ledger in ["work", "nodes"] {
        let options = |n: usize| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => unreachable!(),
            }
            options
        };
        let maximum = match ledger {
            "work" => baseline.limits.max_analysis_steps,
            "nodes" => baseline.limits.max_nodes,
            _ => unreachable!(),
        };
        let (mut low, mut high) = (1, maximum);
        while low < high {
            let n = low + (high - low) / 2;
            let report = CompilationSession::default().compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{ledger} N={n}: {}", report.to_json()),
            }
        }
        assert!(low > 1, "{ledger} must have a meaningful N-1 control");
        let mut warm = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                warm.compile_source(&unit, &options(n)),
                CompilationSession::default().compile_source(&unit, &options(n)),
            ] {
                assert_eq!(
                    report.status,
                    expected,
                    "{ledger} N={low}: {}",
                    report.to_json()
                );
                if expected == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!("G5.2.1 dyn {ledger} N={low}; N-1/cold/warm/fresh agree");
    }

    let cache = directory("raw-cache");
    let options = CompileOptions {
        cache_dir: Some(cache.join("raw")),
        ..CompileOptions::default()
    };
    let cold = CompilationSession::default().compile_source(&unit, &options);
    let warm = CompilationSession::default().compile_source(&unit, &options);
    for report in [&cold, &warm] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    }
    assert_eq!(warm.disk_cache, DiskCacheStatus::Hit, "{}", warm.to_json());

    // semantic-v4 charge_type excludes Dyn. A trusted-local candidate cannot
    // authorize this type; both cold and warm requests rebuild before Verify.
    let options = CompileOptions {
        semantic_cache: Some(cache.join("semantic")),
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::default();
    let reports = [
        session.compile_source(&unit, &options),
        session.compile_source(&unit, &options),
        CompilationSession::default().compile_source(&unit, &options),
    ];
    for report in &reports {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    }
    for phase in [Phase::Types, Phase::Analyze] {
        assert!(
            reports[2]
                .query_events
                .iter()
                .any(|event| event.phase == phase
                    && event.disposition == zil_driver::QueryDisposition::Executed),
            "{}",
            reports[2].to_json()
        );
    }
    let candidates = cache.join("semantic").join("semantic-v4");
    if candidates.exists() {
        assert!(fs::read_dir(candidates).unwrap().all(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_none_or(|ext| ext != "candidate")
        }));
    }
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn project_request_eight_units_object_cache_reuses_actual_dyn_groups() {
    if !cfg!(feature = "llvm-api") {
        return;
    }
    let files = project_files(include_str!("project/main.zil"));
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory("project-objects");
        let executable = dir.join("program");
        let options = CompileOptions {
            backend: NativeBackend::LlvmApi,
            optimization,
            stop_after: Stage::Build,
            entry: Some("main".into()),
            output: Some(executable.clone()),
            object_cache: Some(dir.join("objects")),
            codegen: CodegenOptions { units: 8, jobs: 4 },
            ..CompileOptions::default()
        };
        let mut baseline_units = None;
        for warm in [false, true] {
            let report = compile_project(&project(&files), &options);
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1, "{}", report.to_json());
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, NativeBackend::LlvmApi);
            assert!(native.fallback.is_none(), "{}", report.to_json());
            assert_eq!(native.requested_units, 8);
            assert!((1..=8).contains(&native.actual_units));
            if let Some(units) = baseline_units {
                assert_eq!(native.actual_units, units);
            } else {
                baseline_units = Some(native.actual_units);
            }
            if warm {
                assert_eq!(
                    native.object_cache.hits,
                    native.actual_units,
                    "{}",
                    report.to_json()
                );
                assert_eq!(native.object_cache.misses, 0);
            } else {
                assert_eq!(native.object_cache.hits, 0, "{}", report.to_json());
                assert_eq!(
                    native.object_cache.misses,
                    native.actual_units,
                    "{}",
                    report.to_json()
                );
            }
            let result = Command::new(&executable).output().unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert_eq!(result.stdout, b"29\n");
            assert!(result.stderr.is_empty(), "{result:?}");
        }
        eprintln!(
            "G5.2.1 dyn project requested 8, actual {} units at {optimization:?}",
            baseline_units.unwrap()
        );
    }
}
