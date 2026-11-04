//! General Task38 source consumers; scalar worker admission remains explicit.
use super::*;
const SOURCE: &str = include_str!("task-general-i32.zil");
#[test]
fn general_task343_native_ordinary_poll_and_once_ready() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, text, expected) in [
                ("yield", SOURCE, b"PRC".as_slice()),
                (
                    "user-poll",
                    include_str!("task-general-user-poll.zil"),
                    b"PRC".as_slice(),
                ),
                (
                    "direct",
                    include_str!("task-general-direct.zil"),
                    b"RC".as_slice(),
                ),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "zil343-general-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
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
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                assert_eq!(actual.stdout, expected);
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
#[test]
fn general_task343_raw_constructor_result_and_pc0_boundaries() {
    use zil_ir::{InstructionKind as I, Type};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(SOURCE, &limits).unwrap().core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    for case in 0..6 {
        let mut raw = typed.clone();
        let caller = raw
            .functions
            .iter()
            .position(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, I::TaskCreate { .. }))
            })
            .unwrap();
        match case {
            0 => {
                let i = raw.functions[caller]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreate { .. }))
                    .unwrap();
                if let I::TaskCreate { arguments, .. } = &mut i.kind {
                    arguments.clear();
                }
            }
            1 => {
                let i = raw.functions[caller]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreate { .. }))
                    .unwrap();
                if let I::TaskCreate { worker, .. } = &mut i.kind {
                    *worker = zil_ir::FunctionId(u32::MAX);
                }
            }
            2 => {
                let f = &mut raw.functions[caller];
                let i = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreate { .. }))
                    .unwrap();
                let out = i.result.unwrap();
                if let Type::ErrorUnion { result, .. } = &mut f.values[out.0 as usize] {
                    **result = Type::Managed(Box::new(Type::Task {
                        result: Box::new(Type::Bool),
                    }));
                }
            }
            3 => {
                let r = raw
                    .resources
                    .iter_mut()
                    .find(|r| matches!(r.payload, Type::Task { .. }))
                    .unwrap();
                r.name.push_str("forged");
            }
            4 => {
                let f = raw
                    .functions
                    .iter_mut()
                    .find(|f| {
                        f.blocks
                            .iter()
                            .flat_map(|b| &b.instructions)
                            .any(|i| matches!(i.kind, I::TaskPoll { .. }))
                    })
                    .unwrap();
                let out = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::TaskPoll { .. }))
                    .unwrap()
                    .result
                    .unwrap();
                if let Type::ErrorUnion { result, .. } = &mut f.values[out.0 as usize]
                    && let Type::Enum { name, .. } = result.as_mut()
                {
                    name.push_str("forged");
                }
            }
            5 => {
                let worker = raw.functions[caller]
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| {
                        if let I::TaskCreate { worker, .. } = i.kind {
                            Some(worker)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let f = &mut raw.functions[worker.0 as usize];
                f.locals[f.parameters[0].0 as usize].ty = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                };
            }
            _ => unreachable!(),
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze case {case}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let input = if wire {
                zil_ir::wire::decode_module_candidate(
                    &zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap(),
                    &limits,
                )
                .unwrap()
                .0
            } else {
                raw.clone()
            };
            let errors =
                zil_verify::verify_module(input, claim.clone(), &target, &limits).unwrap_err();
            assert_ne!(
                errors[0].code, "E-VERIFY-PLAN-IDENTITY",
                "case {case} wire={wire}: {errors:?}"
            );
            eprintln!(
                "general raw case={case} wire={wire}: {} {}",
                errors[0].code, errors[0].message
            );
            let expected = [
                "E-VERIFY-TASK",
                "E-VERIFY-TASK",
                "E-UNSUPPORTED",
                "E-VERIFY-TASK",
                "E-VERIFY-TYPE",
                "E-VERIFY-TASK",
            ][case];
            assert_eq!(
                errors[0].code, expected,
                "case {case} wire={wire}: {errors:?}"
            );
            if case == 2 {
                assert!(
                    errors[0]
                        .message
                        .contains("general owning task result lowering")
                );
            }
        }
    }
}

#[test]
fn general_task343_borrowed_initial_frame_is_not_erased_by_scalar_result() {
    let original = include_str!("task-general-borrowed-pc0.zil");
    let report = compile_source(&source(original), &CompileOptions::default());
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Analyze,
        "{}",
        report.to_json()
    );
    assert!(report.diagnostics.iter().any(|d| d.code == "E-TASK-ABI"));
    // Build an actual benign plan on the same source-derived CFG/SSA inventory:
    // only the TaskCreate target and input are changed to a closed scalar
    // neighbor. Rebind that untrusted plan to the original borrowed candidate.
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(original, &limits).unwrap().core;
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    assert!(
        zil_analyze::analyze(&raw, &limits)
            .unwrap_err()
            .iter()
            .any(|d| d.code == "E-TASK-ABI")
    );
    let mut benign = raw.clone();
    let safe = benign
        .functions
        .iter()
        .find(|f| f.name == "safe")
        .unwrap()
        .id;
    let factory = benign
        .functions
        .iter_mut()
        .find(|f| f.name == "escape")
        .unwrap();
    let scalar = factory
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(
                i.kind,
                zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(_))
            )
            .then_some(i.result)
            .flatten()
        })
        .unwrap();
    let creation = factory
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, zil_ir::InstructionKind::TaskCreate { .. }))
        .unwrap();
    creation.kind = zil_ir::InstructionKind::TaskCreate {
        worker: safe,
        arguments: vec![scalar],
    };
    let mut claim = zil_analyze::analyze(&benign, &limits).unwrap();
    zil_verify::verify_module(benign, claim.clone(), &target, &limits).unwrap();
    claim.module_fingerprint = raw.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    for wire in [false, true] {
        let (input, claim) = if wire {
            zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
        } else {
            (raw.clone(), claim.clone())
        };
        let errors = zil_verify::verify_module(input, claim, &target, &limits).unwrap_err();
        assert_eq!(
            errors[0].code, "E-VERIFY-TASK",
            "source PC0 wire={wire}: {errors:?}"
        );
        assert!(
            errors[0]
                .message
                .contains("closed scalar profile independently"),
            "{errors:?}"
        );
    }
}
#[test]
fn general_task343_native_creation_failure_and_default_cleanup() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let observed = SOURCE.replace("putchar", "task_general_observe");
    let default = observed.replace("    task_finish_destroy(give task);", "");
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil343-general-fault-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let object = dir.join("observer.o");
            let observer = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/language-readiness/terminal/343/task-general-observer.c");
            let output = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                .arg(observer)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            for text in [observed.as_str(), default.as_str()] {
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(wrapper.clone()),
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                for fail in [false, true] {
                    let mut command = Command::new(&report.artifacts["executable"]);
                    if fail {
                        command.env("ZIL343_TASK_ALLOC_FAIL", "1");
                    }
                    let actual = command.output().unwrap();
                    assert_eq!(actual.status.code(), Some(i32::from(fail)), "{actual:?}");
                    assert_eq!(
                        actual.stdout,
                        if fail {
                            b"A".as_slice()
                        } else {
                            b"APRCF".as_slice()
                        },
                        "{actual:?}"
                    );
                }
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn general_task343_exact_kernel_work_boundary() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(SOURCE, &limits).unwrap().core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for verify in [false, true] {
        let accepts = |steps| {
            let bounds = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &bounds).map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &bounds).map(|_| ())
            }
        };
        let (mut lo, mut hi) = (1, limits.max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if accepts(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        accepts(lo).unwrap();
        assert_eq!(
            accepts(lo - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        accepts(lo).unwrap();
        eprintln!("general Task343 exact kernel verify={verify} N={lo}");
    }
}

#[test]
fn general_task343_user_poll_shape_does_not_replace_standard_identity() {
    let source_text = include_str!("task-general-user-poll.zil");
    let wrong = source_text.replace(
        "let outcome = task_poll(task)?;",
        "let outcome:Poll<i32> = task_poll(task)?;",
    );
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
}
