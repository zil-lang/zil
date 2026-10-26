//! 005: source and project entries must retain lexical receiver diagnostics
//! before later type or implementation diagnostics.
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Stage, compile_path, compile_source};

const CASES: [(&str, &str, &str); 4] = [
    (
        "RX05",
        include_str!("../../../conformance/cases/receiver/rx05-static-implicit-receiver.zil"),
        "E-RECEIVER-NONE",
    ),
    (
        "RX14",
        include_str!("../../../conformance/cases/receiver/rx14-trait-static-implicit-member.zil"),
        "E-RECEIVER-NONE",
    ),
    (
        "RX18",
        include_str!("../../../conformance/cases/receiver/rx18-local-self-shadow.zil"),
        "E-RECEIVER-RESERVED-NAME",
    ),
    (
        "RX19",
        include_str!("../../../conformance/cases/receiver/rx19-arrow-self-shadow.zil"),
        "E-RECEIVER-RESERVED-NAME",
    ),
];
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-receiver-005-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn put(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn options() -> CompileOptions {
    CompileOptions {
        stop_after: Stage::Types,
        ..CompileOptions::default()
    }
}
fn receiver_error(report: &CompileReport, case: &str, code: &str) {
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{case}: {}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{case}: {}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        code,
        "{case}: {}",
        report.to_json()
    );
}

#[test]
fn r005_01_four_frozen_receiver_cases_match_source_and_file_entries() {
    let directory = Directory::new();
    for (case, text, code) in CASES {
        let path = directory.put("main.zil", text);
        let source = SourceUnit::new(FileId(0), ModuleId(0), path.to_string_lossy(), text);
        let single = compile_source(&source, &options());
        receiver_error(&single, case, code);
        let project = compile_path(&path, &options());
        receiver_error(&project, case, code);
        assert_eq!(
            project.diagnostics, single.diagnostics,
            "{case}: phase, code, message and original spans must agree"
        );
    }
}

#[test]
fn r005_02_non_root_modules_keep_lexical_errors_and_original_spans() {
    let directory = Directory::new();
    // A record in the root must not change a dependency's lexical receiver
    // diagnostic or replace its original source-file provenance.
    let root = directory.put(
        "main.zil",
        "import dependency\nstruct Root { value: i32 }\nfn main() -> i32 => 0\n",
    );
    for (case, text, code) in CASES {
        let path = directory.put("dependency.zil", text);
        let source = SourceUnit::new(FileId(1), ModuleId(1), path.to_string_lossy(), text);
        let single = compile_source(&source, &options());
        receiver_error(&single, case, code);
        let project = compile_path(&root, &options());
        receiver_error(&project, case, code);
        assert_eq!(
            project.diagnostics, single.diagnostics,
            "{case}: dependency provenance must not become root provenance"
        );
        let span = project.diagnostics[0].span.unwrap();
        assert_eq!(span.file, FileId(1));
        assert_eq!(PathBuf::from(&project.source_files[&span.file]), path);
    }
}

#[test]
fn r005_03_programs_and_shared_receivers_work_while_trait_impls_stay_unsupported() {
    let directory = Directory::new();
    let input = directory.put(
        "main.zil",
        "import dependency\nfn main() -> i32 => dependency.value()\n",
    );
    directory.put("dependency.zil", "pub fn value() -> i32 => 42\n");
    let report = compile_path(&input, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify);
    assert_eq!(report.verification_runs, 1);
    let receiver = "struct Point { x: i32 }\nimpl Point(self: &Self) { fn valid() -> i32 => .x }\n";
    directory.put("main.zil", receiver);
    let source = SourceUnit::new(FileId(0), ModuleId(0), input.to_string_lossy(), receiver);
    for report in [
        compile_source(&source, &options()),
        compile_path(&input, &options()),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types);
        assert!(report.diagnostics.is_empty());
    }

    // The same shared receiver must also perform its field read in native code.
    let callable = format!(
        "{receiver}fn result() -> i32 {{ let point = Point {{ x: 42 }}; return point.valid() }}\n"
    );
    directory.put("main.zil", &callable);
    let source = SourceUnit::new(FileId(0), ModuleId(0), input.to_string_lossy(), &callable);
    for from_file in [false, true] {
        let build = CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(directory.0.join(if from_file { "file" } else { "source" })),
            ..CompileOptions::default()
        };
        let report = if from_file {
            compile_path(&input, &build)
        } else {
            compile_source(&source, &build)
        };
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let observed = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(observed.status.success(), "{observed:?}");
        assert_eq!(observed.stdout, b"42\n");
        assert!(observed.stderr.is_empty());
    }

    let unsupported = "struct Point { x: i32 }\ntrait Zero { fn zero() -> i32; }\nimpl Zero for Point { fn zero() -> i32 => 0 }\n";
    directory.put("main.zil", unsupported);
    let source = SourceUnit::new(FileId(0), ModuleId(0), input.to_string_lossy(), unsupported);
    let single = compile_source(&source, &options());
    let project = compile_path(&input, &options());
    for report in [&single, &project] {
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types);
        assert!(!report.diagnostics.is_empty());
        assert!(report.diagnostics.iter().all(|d| d.code == "E-UNSUPPORTED"));
    }
    assert_eq!(project.diagnostics, single.diagnostics);
}

#[test]
fn r005_04_program_metadata_budgets_still_precede_lexical_walk() {
    let source = SourceUnit::new(FileId(9), ModuleId(7), "raw-receiver.zil", CASES[0].1);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let module = zil_expand::expand_program_module(&source, &ast, Vec::new())
        .unwrap()
        .0;
    let program = zil_ir::CoreProgram {
        root: ModuleId(7),
        source_fingerprint: Fingerprint::of(b"005-budget"),
        modules: vec![module],
    };
    for limits in [
        Limits {
            max_nodes: 0,
            ..Limits::default()
        },
        Limits {
            max_functions: 0,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        },
    ] {
        let errors = zil_types::check_program(&program, &options().target, &limits).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].code, "E-RESOURCE-LIMIT");
    }
    // With sufficient budgets, the same raw source-derived input must expose
    // the receiver error.
    let errors =
        zil_types::check_program(&program, &options().target, &Limits::default()).unwrap_err();
    assert_eq!(errors[0].code, "E-RECEIVER-NONE");
}
