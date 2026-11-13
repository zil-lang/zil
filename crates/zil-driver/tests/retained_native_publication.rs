//! Explicit-profile, resident real-source contract for final executable reuse.
//! This is an ignored infrastructure contract, not a compile-time benchmark.
#![cfg(all(feature = "llvm-api", target_os = "linux", target_arch = "x86_64"))]

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization,
    ProjectSources, Stage,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MAIN: &str = "import left\nimport right\nfn main()->i32{return left.value()+right.value();}";
const RIGHT: &str = "pub fn value()->i32{return 23;}";
const INVALID: &str = "fn hidden()->i32{let x=1;var p=&x;if true{let inner=2;p=&inner;}return *p;}\npub fn value()->i32{return hidden();}";

struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-retained-publication-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let case = Self(path);
        fs::write(case.0.join("main.zil"), MAIN).unwrap();
        fs::write(case.0.join("right.zil"), RIGHT).unwrap();
        case.private_value(19);
        case
    }
    fn private_value(&self, value: i32) {
        fs::write(
            self.0.join("left.zil"),
            format!("fn hidden()->i32{{return {value};}}\npub fn value()->i32{{return hidden();}}"),
        )
        .unwrap();
    }
    fn project(&self) -> ProjectSources {
        ProjectSources {
            root: ModuleId(0),
            modules: ["main", "left", "right"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    let path = self.0.join(format!("{name}.zil"));
                    SourceUnit::new(
                        FileId(index as u32),
                        ModuleId(index as u32),
                        path.to_string_lossy(),
                        fs::read_to_string(&path).unwrap(),
                    )
                })
                .collect(),
            paths: BTreeMap::from([
                (vec!["left".into()], ModuleId(1)),
                (vec!["right".into()], ModuleId(2)),
            ]),
        }
    }
    fn options(&self, profile: PathBuf) -> CompileOptions {
        CompileOptions {
            backend: NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            output: Some(self.0.join("program")),
            emit_dir: Some(self.0.join("dumps")),
            native_link_profile: Some(profile),
            clang: None,
            push_reuse: true,
            cache_dir: Some(self.0.join("source-cache")),
            object_cache: Some(self.0.join("objects")),
            optimization: Optimization::O2,
            codegen: CodegenOptions { units: 2, jobs: 1 },
            ..CompileOptions::default()
        }
    }
    fn compile(
        &self,
        session: &mut CompilationSession,
        options: &CompileOptions,
        label: &str,
    ) -> CompileReport {
        let report = session.compile_project(&self.project(), options);
        eprintln!("{label}: {}", report.to_json());
        report
    }
    fn paths(&self) -> (PathBuf, PathBuf) {
        (self.0.join("program"), self.0.join("program.ll"))
    }
    fn dump(&self) -> PathBuf {
        self.0.join("dumps/llvm.ll")
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    ctime: (i64, i64),
    mode: u32,
    length: u64,
}
fn stamp(path: &Path) -> Stamp {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    Stamp {
        device: metadata.dev(),
        inode: metadata.ino(),
        ctime: (metadata.ctime(), metadata.ctime_nsec()),
        mode: metadata.mode(),
        length: metadata.len(),
    }
}
fn successful(report: &CompileReport, expected: i32) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "current independent Verify");
    let native = report.native_codegen.as_ref().expect("actual native build");
    assert_eq!(native.requested_units, 2);
    assert_eq!(native.actual_units, 2);
    assert_eq!(native.jobs, 1);
    let output = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
fn reused(report: &CompileReport, retained_files: bool, profile: &str) {
    let native = report.native_codegen.as_ref().unwrap();
    let final_value = native
        .final_artifact
        .as_ref()
        .expect("explicit reuse receipt");
    assert!(final_value.reused_program);
    assert!(final_value.before_codegen);
    assert_eq!(final_value.retained_files, retained_files);
    assert_eq!(final_value.profile_id, profile);
    let checked = final_value
        .current_preflight_checked_units
        .expect("current checked count");
    let reused = final_value
        .current_preflight_reused_units
        .expect("current bound receipt count");
    assert_eq!(checked + reused, native.actual_units);
    assert!(
        report.commands.is_empty(),
        "reuse must execute no native subprocess: {}",
        report.to_json()
    );
}
fn same_owned_files(case: &Case, program: &[u8], diagnostic: &[u8], mode: u32) {
    let (executable, llvm) = case.paths();
    assert_eq!(fs::read(&executable).unwrap(), program);
    assert_eq!(fs::read(&llvm).unwrap(), diagnostic);
    assert_eq!(stamp(&executable).mode & 0o7777, mode);
    assert_ne!(mode & 0o111, 0, "restored program must remain executable");
    assert_eq!(fs::read(case.dump()).unwrap(), diagnostic);
}

#[test]
#[ignore = "requires an explicitly installed native link profile; run with ZIL_TEST_NATIVE_LINK_PROFILE"]
fn resident_final_values_retain_restore_and_reject_current_invalid_source() {
    let profile = PathBuf::from(
        std::env::var_os("ZIL_TEST_NATIVE_LINK_PROFILE")
            .expect("ZIL_TEST_NATIVE_LINK_PROFILE must name the installed native link manifest"),
    );
    assert!(
        profile.is_absolute() && profile.is_file(),
        "explicit profile manifest must exist"
    );
    let case = Case::new();
    let options = case.options(profile);
    let mut session = CompilationSession::default();
    let first = case.compile(&mut session, &options, "first-actual-build");
    successful(&first, 42);
    assert!(!first.commands.is_empty(), "cold build must actually link");
    let native = first.native_codegen.as_ref().unwrap();
    assert_eq!(native.object_reports.len(), native.actual_units);
    assert!(
        !native
            .final_artifact
            .as_ref()
            .is_some_and(|value| value.reused_program)
    );
    let (executable, llvm) = case.paths();
    let program = fs::read(&executable).unwrap();
    let diagnostic = fs::read(&llvm).unwrap();
    let executable_mode = stamp(&executable).mode & 0o7777;
    let before = (stamp(&executable), stamp(&llvm));

    let noop = case.compile(&mut session, &options, "retained-noop");
    successful(&noop, 42);
    let profile_id = noop
        .native_codegen
        .as_ref()
        .unwrap()
        .final_artifact
        .as_ref()
        .unwrap()
        .profile_id
        .clone();
    assert!(!profile_id.is_empty());
    reused(&noop, true, &profile_id);
    assert_eq!(
        (stamp(&executable), stamp(&llvm)),
        before,
        "no-op must preserve both inodes and ctimes"
    );
    same_owned_files(&case, &program, &diagnostic, executable_mode);

    for (label, path, remove) in [
        ("missing-executable", &executable, true),
        ("overwritten-executable", &executable, false),
        ("missing-sidecar", &llvm, true),
        ("overwritten-sidecar", &llvm, false),
    ] {
        if remove {
            fs::remove_file(path).unwrap();
        } else {
            fs::write(path, b"externally overwritten\xff").unwrap();
            if path == &executable {
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
            }
        }
        let restored = case.compile(&mut session, &options, label);
        successful(&restored, 42);
        reused(&restored, false, &profile_id);
        same_owned_files(&case, &program, &diagnostic, executable_mode);
    }

    // Race after backend reuse: the driver must dump the owned diagnostic, then
    // restore BOTH members privately instead of trusting the earlier file stamp.
    let mut raced = false;
    let raced_report =
        session.compile_project_with_observer(&case.project(), &options, &mut |event| {
            if event.phase == Phase::Link {
                assert!(!raced, "one native link event per request");
                raced = true;
                fs::write(&executable, b"raced executable").unwrap();
                fs::set_permissions(&executable, fs::Permissions::from_mode(0o600)).unwrap();
                fs::write(&llvm, b"raced diagnostic\xff").unwrap();
            }
        });
    eprintln!("after-backend-file-race: {}", raced_report.to_json());
    assert!(raced);
    successful(&raced_report, 42);
    reused(&raced_report, false, &profile_id);
    same_owned_files(&case, &program, &diagnostic, executable_mode);

    case.private_value(20);
    let edited = case.compile(&mut session, &options, "private-function-edit");
    successful(&edited, 43);
    assert!(
        !edited.commands.is_empty(),
        "changed observable program must really link"
    );
    assert!(
        !edited
            .native_codegen
            .as_ref()
            .unwrap()
            .final_artifact
            .as_ref()
            .is_some_and(|value| value.reused_program)
    );
    let edited_program = fs::read(&executable).unwrap();
    let edited_diagnostic = fs::read(&llvm).unwrap();
    assert_ne!(edited_diagnostic, diagnostic);
    same_owned_files(&case, &edited_program, &edited_diagnostic, executable_mode);
    let before_error = (stamp(&executable), stamp(&llvm));

    // This exact address-lifetime counterexample is rejected by independent
    // Verify (also covered by shared_addresses), after the warm successful edit.
    fs::write(case.0.join("left.zil"), INVALID).unwrap();
    let rejected = case.compile(&mut session, &options, "current-verify-rejection");
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    assert_eq!(rejected.phase, Phase::Verify, "{}", rejected.to_json());
    assert_eq!(rejected.verification_runs, 1);
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.phase == Phase::Verify
                && diagnostic.code == "E-BORROW-CONFLICT")
    );
    assert!(rejected.native_codegen.is_none());
    assert!(rejected.commands.is_empty());
    assert!(
        !rejected.artifacts.contains_key("executable"),
        "old on-disk executable cannot be reported as this rejected request's result"
    );
    assert_eq!((stamp(&executable), stamp(&llvm)), before_error);
    assert_eq!(fs::read(&executable).unwrap(), edited_program);
    assert_eq!(fs::read(&llvm).unwrap(), edited_diagnostic);

    case.private_value(20);
    let recovered = case.compile(&mut session, &options, "valid-source-recovery");
    successful(&recovered, 43);
    reused(&recovered, true, &profile_id);
    assert_eq!((stamp(&executable), stamp(&llvm)), before_error);
    same_owned_files(&case, &edited_program, &edited_diagnostic, executable_mode);
}
