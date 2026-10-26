use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use zil_ir::{Constant, InstructionKind};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-driver-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "test.zil", text)
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{:?}",
        report.diagnostics
    );
}
fn disposition(report: &CompileReport, phase: Phase) -> Option<QueryDisposition> {
    report
        .query_events
        .iter()
        .find(|event| event.phase == phase)
        .map(|event| event.disposition)
}
fn no_staging(directory: &Path) {
    assert!(fs::read_dir(directory).unwrap().all(|entry| {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        !name.starts_with(".zil-driver-") && !name.starts_with(".zil-build-")
    }));
}

#[test]
fn cache_session_reuses_raw_phases_but_always_runs_independent_verification() {
    let input = source("fn main() -> i32 { return 42 }");
    let mut session = CompilationSession::default();
    let cold = session.compile_source(&input, &CompileOptions::default());
    success(&cold);
    let warm = session.compile_source(&input, &CompileOptions::default());
    success(&warm);
    for phase in [
        Phase::Parse,
        Phase::Expand,
        Phase::Ct,
        Phase::Types,
        Phase::Analyze,
    ] {
        assert_eq!(disposition(&cold, phase), Some(QueryDisposition::Executed));
        assert_eq!(disposition(&warm, phase), Some(QueryDisposition::Reused));
    }
    assert_eq!(warm.verification_runs, 1);
    assert_eq!(cold.verification_runs, 1);
    assert_eq!(
        disposition(&warm, Phase::Verify),
        Some(QueryDisposition::Executed)
    );
    assert_eq!(session.active_query_count(), 0);
}
#[test]
fn cache_target_change_reuses_frontend_and_rechecks_target_dependent_phases() {
    let input = source("fn main() -> i32 { return 0 }");
    let mut session = CompilationSession::default();
    success(&session.compile_source(&input, &CompileOptions::default()));
    let mut options = CompileOptions::default();
    options.target.id = "another-explicit-profile".into();
    let report = session.compile_source(&input, &options);
    success(&report);
    assert_eq!(
        disposition(&report, Phase::Parse),
        Some(QueryDisposition::Reused)
    );
    assert_eq!(
        disposition(&report, Phase::Expand),
        Some(QueryDisposition::Reused)
    );
    for phase in [Phase::Ct, Phase::Types, Phase::Analyze, Phase::Verify] {
        assert_eq!(
            disposition(&report, phase),
            Some(QueryDisposition::Executed)
        );
    }
}
#[test]
fn cache_changed_source_limits_build_inputs_and_file_identity_cannot_reuse_stale_snapshots() {
    let input = source("fn main() -> i32 { return 0 }");
    let mut session = CompilationSession::default();
    success(&session.compile_source(&input, &CompileOptions::default()));
    let changed = source("fn main() -> i32 { return 1 }");
    let report = session.compile_source(&changed, &CompileOptions::default());
    success(&report);
    assert!(
        report
            .query_events
            .iter()
            .all(|event| event.disposition == QueryDisposition::Executed)
    );
    let mut options = CompileOptions::default();
    options.limits.max_tokens = 1;
    let report = session.compile_source(&input, &options);
    assert_eq!(report.status, ResultCategory::ResourceLimit);
    assert_eq!(
        disposition(&report, Phase::Parse),
        Some(QueryDisposition::Executed)
    );
    let options = CompileOptions {
        build_inputs: Fingerprint::of(b"different locked configuration"),
        ..CompileOptions::default()
    };
    let report = session.compile_source(&input, &options);
    success(&report);
    assert_eq!(
        disposition(&report, Phase::Parse),
        Some(QueryDisposition::Executed)
    );
    let different_file = SourceUnit::new(FileId(9), ModuleId(8), "other.zil", &input.text);
    let report = session.compile_source(&different_file, &CompileOptions::default());
    success(&report);
    assert_eq!(
        disposition(&report, Phase::Parse),
        Some(QueryDisposition::Executed)
    );
}
#[test]
fn cache_disk_corruption_is_rebuilt_and_never_substitutes_for_phase_execution() {
    let directory = Directory::new();
    let input = source("fn main() -> i32 { return 0 }");
    let options = CompileOptions {
        cache_dir: Some(directory.0.clone()),
        ..CompileOptions::default()
    };
    let first = compile_source(&input, &options);
    success(&first);
    assert_eq!(first.disk_cache, DiskCacheStatus::Missing);
    let hit = compile_source(&input, &options);
    success(&hit);
    assert_eq!(hit.disk_cache, DiskCacheStatus::Hit);
    assert!(
        hit.query_events
            .iter()
            .all(|event| event.disposition == QueryDisposition::Executed)
    );
    let path = fs::read_dir(&directory.0)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    for corruption in [vec![0; 3], b"ZILRAW01bad version".to_vec(), vec![0; 81]] {
        fs::write(&path, corruption).unwrap();
        let rebuilt = compile_source(&input, &options);
        success(&rebuilt);
        assert_eq!(rebuilt.disk_cache, DiskCacheStatus::Rebuilt);
        assert_eq!(rebuilt.verification_runs, 1);
        assert_eq!(
            disposition(&rebuilt, Phase::Parse),
            Some(QueryDisposition::Executed)
        );
    }
}
#[test]
fn cache_tampered_raw_typed_snapshot_is_rejected_by_a_new_verifier_run() {
    let input = source("fn main() -> i32 { return 0 }");
    let mut session = CompilationSession::default();
    success(&session.compile_source(&input, &CompileOptions::default()));
    for raw in session.raw.values_mut() {
        if let session::RawSnapshot::Typed(typed) = raw {
            Arc::make_mut(typed).functions[0].blocks[0].instructions[0].kind =
                InstructionKind::Constant(Constant::I64(7));
        }
    }
    let report = session.compile_source(&input, &CompileOptions::default());
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert_eq!(report.phase, Phase::Verify);
    assert_eq!(report.verification_runs, 1);
    assert_eq!(
        disposition(&report, Phase::Types),
        Some(QueryDisposition::Reused)
    );
    assert!(report.diagnostics[0].code.starts_with("E-VERIFY-"));
}

#[test]
fn native_entry_cannot_select_a_synthesized_closure_with_or_without_captures() {
    for text in [
        "fn result() -> i64 { let callback = () => 42; return callback(); }",
        "fn result() -> i64 { let base: i64 = 40; let callback = move () => base + 2; return callback(); }",
    ] {
        let input = source(text);
        let mut session = CompilationSession::default();
        success(&session.compile_source(&input, &CompileOptions::default()));
        let typed = session
            .raw
            .values()
            .find_map(|raw| match raw {
                session::RawSnapshot::Typed(module) => Some(module),
                _ => None,
            })
            .expect("actual source compilation produced a typed closure module");
        let definition = &typed.closures[0];
        let closure = typed
            .functions
            .iter()
            .find(|function| function.id == definition.function)
            .unwrap();
        assert_eq!(closure.environment, Some(definition.id));
        let directory = Directory::new();
        let output = directory.0.join("preserved-program");
        let sidecar = directory.0.join("preserved-program.ll");
        fs::write(&output, b"existing executable").unwrap();
        fs::write(&sidecar, b"existing IR").unwrap();
        let options = CompileOptions {
            stop_after: Stage::Build,
            entry: Some(closure.name.clone()),
            output: Some(output.clone()),
            clang: Some(directory.0.join("no-such-compiler")),
            ..CompileOptions::default()
        };
        let report = session.compile_source(&input, &options);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Driver);
        assert_eq!(report.diagnostics[0].code, "E-ENTRY-ENVIRONMENT");
        assert_eq!(report.verification_runs, 1);
        assert!(report.commands.is_empty());
        assert!(!report.artifacts.contains_key("executable"));
        assert_eq!(fs::read(&output).unwrap(), b"existing executable");
        assert_eq!(fs::read(&sidecar).unwrap(), b"existing IR");
        no_staging(&directory.0);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
        let absent_parent = directory.0.join("uncreated-output");
        let options = CompileOptions {
            output: Some(absent_parent.join("program")),
            ..options
        };
        let report = session.compile_source(&input, &options);
        assert_eq!(report.diagnostics[0].code, "E-ENTRY-ENVIRONMENT");
        assert_eq!(report.verification_runs, 1);
        assert!(!absent_parent.exists());
    }
}

#[test]
fn cancellation_during_a_phase_aborts_publication_and_next_request_recovers() {
    let directory = Directory::new();
    let input = source("fn main() -> i32 { return 0 }");
    let options = CompileOptions {
        stop_after: Stage::Build,
        output: Some(directory.0.join("must-not-exist")),
        ..CompileOptions::default()
    };
    let token = options.cancellation.clone();
    let mut session = CompilationSession::default();
    let report = session.compile_source_with_observer(&input, &options, &mut |event| {
        if event.phase == Phase::Types {
            token.cancel();
        }
    });
    assert_eq!(report.status, ResultCategory::Cancelled);
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(session.active_query_count(), 0);
    assert_eq!(report.verification_runs, 0);
    assert!(!options.output.as_ref().unwrap().exists());
    let recovered = session.compile_source(&input, &CompileOptions::default());
    success(&recovered);
    assert_eq!(
        disposition(&recovered, Phase::Expand),
        Some(QueryDisposition::Reused)
    );
    assert_eq!(
        disposition(&recovered, Phase::Types),
        Some(QueryDisposition::Executed)
    );
    assert_eq!(recovered.verification_runs, 1);
}
#[test]
fn cancellation_before_start_and_after_cache_hit_cannot_report_success() {
    let input = source("fn main() -> i32 { return 0 }");
    let mut session = CompilationSession::default();
    success(&session.compile_source(&input, &CompileOptions::default()));
    let options = CompileOptions::default();
    options.cancellation.cancel();
    let report = session.compile_source(&input, &options);
    assert_eq!(report.status, ResultCategory::Cancelled);
    assert!(report.query_events.is_empty());
    let options = CompileOptions::default();
    let token = options.cancellation.clone();
    let report = session.compile_source_with_observer(&input, &options, &mut |event| {
        if event.phase == Phase::Parse && event.disposition == QueryDisposition::Reused {
            token.cancel();
        }
    });
    assert_eq!(report.status, ResultCategory::Cancelled);
    assert_eq!(session.active_query_count(), 0);
}
#[test]
fn cancellation_after_native_link_removes_the_unpublished_executable() {
    let directory = Directory::new();
    let input = source("fn main() -> i32 { return 0 }");
    let options = CompileOptions {
        stop_after: Stage::Build,
        output: Some(directory.0.join("cancelled")),
        ..CompileOptions::default()
    };
    let token = options.cancellation.clone();
    let report = CompilationSession::default().compile_source_with_observer(
        &input,
        &options,
        &mut |event| {
            if event.phase == Phase::Link {
                token.cancel();
            }
        },
    );
    assert_eq!(report.status, ResultCategory::Cancelled);
    assert_eq!(
        disposition(&report, Phase::Link),
        Some(QueryDisposition::Executed)
    );
    assert!(!directory.0.join("cancelled").exists());
    assert!(!directory.0.join("cancelled.ll").exists());
    assert!(!report.artifacts.contains_key("executable"));
    no_staging(&directory.0);
}

#[test]
fn cancellation_after_native_link_preserves_both_previous_successful_outputs() {
    let directory = Directory::new();
    let executable = directory.0.join("program");
    let ir = directory.0.join("program.ll");
    let options = CompileOptions {
        stop_after: Stage::Build,
        output: Some(executable.clone()),
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::default();
    success(&session.compile_source(&source("fn main() -> i32 { return 7 }"), &options));
    let old_executable = fs::read(&executable).unwrap();
    let old_ir = fs::read(&ir).unwrap();
    let token = options.cancellation.clone();
    let report = session.compile_source_with_observer(
        &source("fn main() -> i32 { return 9 }"),
        &options,
        &mut |event| {
            if event.phase == Phase::Link {
                token.cancel();
            }
        },
    );
    assert_eq!(report.status, ResultCategory::Cancelled);
    assert_eq!(
        disposition(&report, Phase::Link),
        Some(QueryDisposition::Executed)
    );
    assert_eq!(fs::read(&executable).unwrap(), old_executable);
    assert_eq!(fs::read(&ir).unwrap(), old_ir);
    assert_eq!(
        std::process::Command::new(&executable)
            .status()
            .unwrap()
            .code(),
        Some(7)
    );
    assert!(!report.artifacts.contains_key("executable"));
    no_staging(&directory.0);

    let recovered_options = CompileOptions {
        cancellation: CancellationToken::default(),
        ..options
    };
    success(&session.compile_source(&source("fn main() -> i32 { return 9 }"), &recovered_options));
    assert_eq!(
        std::process::Command::new(&executable)
            .status()
            .unwrap()
            .code(),
        Some(9)
    );
    assert_ne!(fs::read(&ir).unwrap(), old_ir);
    no_staging(&directory.0);
}

#[test]
fn late_artifact_failure_preserves_both_previous_native_outputs() {
    let directory = Directory::new();
    let executable = directory.0.join("program");
    let ir = directory.0.join("program.ll");
    let options = CompileOptions {
        stop_after: Stage::Build,
        output: Some(executable.clone()),
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::default();
    success(&session.compile_source(&source("fn main() -> i32 { return 7 }"), &options));
    let old_executable = fs::read(&executable).unwrap();
    let old_ir = fs::read(&ir).unwrap();
    let emit_dir = directory.0.join("phases");
    let options = CompileOptions {
        emit_dir: Some(emit_dir.clone()),
        ..options
    };
    let report = session.compile_source_with_observer(
        &source("fn main() -> i32 { return 9 }"),
        &options,
        &mut |event| {
            if event.phase == Phase::Link {
                // The initial preflight succeeded; a later artifact write now
                // encounters a real filesystem obstruction after clang ran.
                fs::create_dir(emit_dir.join("llvm.ll")).unwrap();
            }
        },
    );
    assert_eq!(report.status, ResultCategory::InfraError);
    assert_eq!(
        disposition(&report, Phase::Link),
        Some(QueryDisposition::Executed)
    );
    assert_eq!(fs::read(&executable).unwrap(), old_executable);
    assert_eq!(fs::read(&ir).unwrap(), old_ir);
    assert_eq!(
        std::process::Command::new(&executable)
            .status()
            .unwrap()
            .code(),
        Some(7)
    );
    assert!(!report.artifacts.contains_key("executable"));
    no_staging(&directory.0);
    no_staging(&emit_dir);
}

#[test]
fn artifact_preflight_protects_source_from_every_phase_dump_and_native_destination() {
    let text = "fn main() -> i32 { return 0 }";
    for name in [
        "syntax.txt",
        "core.txt",
        "source-map.txt",
        "ct-core.txt",
        "typed.txt",
        "candidate.txt",
        "verified.txt",
        "lir.txt",
        "llvm.ll",
    ] {
        let directory = Directory::new();
        let source = directory.0.join(name);
        fs::write(&source, text).unwrap();
        let cache = directory.0.join("cache");
        let options = CompileOptions {
            stop_after: Stage::Build,
            emit_dir: Some(directory.0.clone()),
            output: Some(directory.0.join("program")),
            cache_dir: Some(cache.clone()),
            ..CompileOptions::default()
        };
        let report = compile_path(&source, &options);
        assert_eq!(
            report.diagnostics[0].code, "E-OUTPUT-SOURCE",
            "{name}: {report:?}"
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), text);
        assert!(report.query_events.is_empty());
        assert!(!cache.exists());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    for name in ["program", "program.ll"] {
        let directory = Directory::new();
        let source = directory.0.join(name);
        fs::write(&source, text).unwrap();
        let options = CompileOptions {
            stop_after: Stage::Build,
            output: Some(directory.0.join("program")),
            ..CompileOptions::default()
        };
        let report = compile_path(&source, &options);
        assert_eq!(report.diagnostics[0].code, "E-OUTPUT-SOURCE");
        assert_eq!(fs::read_to_string(&source).unwrap(), text);
        assert!(report.query_events.is_empty());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
}

#[test]
fn artifact_preflight_rejects_dump_native_collisions_before_any_write() {
    let directory = Directory::new();
    let source = directory.0.join("p.zil");
    fs::write(&source, "fn main() -> i32 { return 0 }").unwrap();
    for output in [directory.0.join("llvm.ll"), directory.0.join("llvm")] {
        let options = CompileOptions {
            stop_after: Stage::Build,
            output: Some(output),
            emit_dir: Some(directory.0.clone()),
            ..CompileOptions::default()
        };
        let report = compile_path(&source, &options);
        assert_eq!(report.diagnostics[0].code, "E-OUTPUT-COLLISION");
        assert!(report.query_events.is_empty());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
}

#[cfg(unix)]
#[test]
fn artifact_preflight_detects_hard_links_and_parent_symlink_aliases() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let source = directory.0.join("syntax.txt");
    let text = "fn main() -> i32 { return 0 }";
    fs::write(&source, text).unwrap();
    let alias = directory.0.join("directory-alias");
    symlink(&directory.0, &alias).unwrap();
    let options = CompileOptions {
        stop_after: Stage::Parse,
        emit_dir: Some(alias),
        ..CompileOptions::default()
    };
    let report = compile_path(&source, &options);
    assert_eq!(report.diagnostics[0].code, "E-OUTPUT-SOURCE");
    assert_eq!(fs::read_to_string(&source).unwrap(), text);

    for link_name in ["program", "program.ll", "dumps/syntax.txt"] {
        let directory = Directory::new();
        let source = directory.0.join("p.zil");
        fs::write(&source, text).unwrap();
        fs::create_dir(directory.0.join("dumps")).unwrap();
        let linked = directory.0.join(link_name);
        fs::hard_link(&source, &linked).unwrap();
        let options = CompileOptions {
            stop_after: Stage::Build,
            output: Some(directory.0.join("program")),
            emit_dir: Some(directory.0.join("dumps")),
            ..CompileOptions::default()
        };
        let report = compile_path(&source, &options);
        assert_eq!(
            report.diagnostics[0].code, "E-OUTPUT-SOURCE",
            "{link_name}: {report:?}"
        );
        assert!(report.query_events.is_empty());
        assert_eq!(fs::read_to_string(&source).unwrap(), text);
        assert_eq!(fs::read_to_string(&linked).unwrap(), text);
        no_staging(&directory.0);
    }
}
#[test]
fn explicit_main_contract_has_the_frozen_types_diagnostics() {
    for (text, code) in [
        (
            "fn main() -> i32 { return 0 }\nfn main() -> i32 { return 1 }",
            "E-NAME-DUPLICATE",
        ),
        ("fn main() -> bool => true", "E-ENTRY-SIGNATURE"),
        ("fn main(x: i32) -> i32 { return x }", "E-ENTRY-SIGNATURE"),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(report.status, ResultCategory::SafeReject);
        assert_eq!(report.phase, Phase::Types);
        assert_eq!(report.diagnostics[0].code, code);
    }
    success(&compile_source(
        &source("fn main() {}"),
        &CompileOptions::default(),
    ));
}
#[test]
fn source_ct_keeps_original_core_and_propagates_ct_errors() {
    let directory = Directory::new();
    let input = source("fn check() -> i64 { return #(6 * 7) }");
    let options = CompileOptions {
        stop_after: Stage::Core,
        emit_dir: Some(directory.0.clone()),
        ..CompileOptions::default()
    };
    let report = compile_source(&input, &options);
    success(&report);
    assert!(
        fs::read_to_string(&report.artifacts["core"])
            .unwrap()
            .contains("CompileTime")
    );
    assert!(disposition(&report, Phase::Ct).is_none());
    let options = CompileOptions {
        emit_dir: Some(directory.0.clone()),
        ..CompileOptions::default()
    };
    let report = compile_source(&input, &options);
    success(&report);
    assert!(
        !fs::read_to_string(&report.artifacts["ct-core"])
            .unwrap()
            .contains("CompileTime")
    );
    let report = compile_source(
        &source("fn check() -> i64 { return #(1 / 0) }"),
        &CompileOptions::default(),
    );
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert_eq!(report.phase, Phase::Ct);
    assert_eq!(report.diagnostics[0].code, "E-CT-ARITHMETIC");
}
#[test]
fn unsupported_profile_and_invalid_source_have_explicit_categories() {
    let options = CompileOptions {
        profile: ExecutionProfile::Managed,
        ..CompileOptions::default()
    };
    let report = compile_source(&source("fn main() {}"), &options);
    assert_eq!(report.status, ResultCategory::Unsupported);
    let directory = Directory::new();
    let path = directory.0.join("invalid.zil");
    fs::write(&path, [0xff]).unwrap();
    let report = compile_path(&path, &CompileOptions::default());
    assert_eq!(report.diagnostics[0].code, "E-UTF8");
    let mut options = CompileOptions::default();
    options.limits.max_source_bytes = 1;
    let report = compile_source(&source("too long"), &options);
    assert_eq!(report.status, ResultCategory::ResourceLimit);
}
