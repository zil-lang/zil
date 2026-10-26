use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, ProjectSources, QueryDisposition, Stage,
    compile_path, compile_project, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-project-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn put(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
    fn build_options(&self) -> CompileOptions {
        CompileOptions {
            stop_after: Stage::Build,
            output: Some(self.0.join("program")),
            ..CompileOptions::default()
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}
fn code(report: &CompileReport, category: ResultCategory, code: &str) {
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
}
fn memory(root: &str, library: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(10),
        modules: vec![
            SourceUnit::new(FileId(20), ModuleId(10), "root.zil", root),
            SourceUnit::new(FileId(21), ModuleId(11), "library.zil", library),
        ],
        paths: BTreeMap::from([(vec!["math".into()], ModuleId(11))]),
    }
}
fn exit(report: &CompileReport) -> i32 {
    success(report);
    Command::new(&report.artifacts["executable"])
        .status()
        .unwrap()
        .code()
        .unwrap()
}

#[test]
fn m001_real_native_modules_keep_same_names_and_only_root_main() {
    let directory = Directory::new();
    let input = directory.put("main.zil", "import math.arithmetic as numbers\nimport other\nfn main() -> i32 => numbers.answer() + other.answer()\n");
    directory.put("math/arithmetic.zil", "fn helper(x: i32) -> i32 => x + 1\npub fn answer() -> i32 => helper(40)\npub fn main() -> i32 => 99\n");
    directory.put("other.zil", "pub fn answer() -> i32 => 1\n");
    let options = CompileOptions {
        emit_dir: Some(directory.0.join("ir")),
        ..directory.build_options()
    };
    let report = compile_path(&input, &options);
    assert_eq!(exit(&report), 42);
    assert_eq!(report.source_files.len(), 3);
    assert_eq!(report.scalar_interfaces.len(), 3);
    assert_eq!(report.verification_runs, 1);
    let interfaces = fs::read_to_string(&report.artifacts["interfaces"]).unwrap();
    assert!(interfaces.contains("answer"));
    assert!(!interfaces.contains("helper"));
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("call i32 @zil.fn."), "{llvm}");
    // Explicit entries are also resolved in the root module alone.
    let report = compile_path(
        &input,
        &CompileOptions {
            entry: Some("answer".into()),
            ..options
        },
    );
    code(&report, ResultCategory::SafeReject, "E-ENTRY-NOT-FOUND");
}

#[test]
fn m002_visibility_alias_and_call_contract_rejections() {
    let options = CompileOptions::default();
    for (root, library, expected) in [
        (
            "import math\nfn main() -> i32 => math.value()",
            "fn value() -> i32 => 42",
            "E-MODULE-PRIVATE",
        ),
        (
            "import math\nfn main() -> i32 => math.absent()",
            "pub fn value() -> i32 => 42",
            "E-MODULE-MEMBER",
        ),
        (
            "import math\nimport math\nfn main() -> i32 => 0",
            "pub fn value() -> i32 => 42",
            "E-MODULE-ALIAS",
        ),
        (
            "import math\nfn math() -> i32 => 0\nfn main() -> i32 => 0",
            "pub fn value() -> i32 => 42",
            "E-MODULE-ALIAS",
        ),
        (
            "import math\nfn main() -> i32 => math.value()",
            "pub fn value(x: i32) -> i32 => x",
            "E-CALL-ARITY",
        ),
        (
            "import math\nfn main() -> i32 => math.value(true)",
            "pub fn value(x: i32) -> i32 => x",
            "E-TYPE-MISMATCH",
        ),
    ] {
        let report = compile_project(&memory(root, library), &options);
        code(&report, ResultCategory::SafeReject, expected);
    }
    let project = memory(
        "import math as left\nimport math as right\nfn main() -> i32 => left.value() + right.value()",
        "pub fn value() -> i32 => 21",
    );
    success(&compile_project(&project, &options));
}

#[test]
fn m003_dependency_edit_changes_runtime_without_changing_public_signature() {
    let directory = Directory::new();
    let input = directory.put(
        "main.zil",
        "import math\nfn main() -> i32 => math.value()\n",
    );
    directory.put(
        "math.zil",
        "fn hidden() -> i32 => 41\npub fn value() -> i32 => hidden()\n",
    );
    let options = directory.build_options();
    let mut session = CompilationSession::default();
    let first = session.compile_path(&input, &options);
    assert_eq!(exit(&first), 41);
    let hot = session.compile_path(&input, &options);
    assert_eq!(exit(&hot), 41);
    assert_eq!(hot.verification_runs, 1);
    assert!(
        hot.query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Reused)
    );
    directory.put(
        "math.zil",
        "fn hidden() -> i32 => 42\npub fn value() -> i32 => hidden()\n",
    );
    let edited = session.compile_path(&input, &options);
    assert_eq!(exit(&edited), 42);
    assert_ne!(first.input_fingerprint, edited.input_fingerprint);
    assert_eq!(
        first.scalar_interfaces[1].signature_fingerprint,
        edited.scalar_interfaces[1].signature_fingerprint
    );
    assert_ne!(
        first.scalar_interfaces[1].implementation_fingerprint,
        edited.scalar_interfaces[1].implementation_fingerprint
    );
    assert!(
        edited
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Parse && e.disposition == QueryDisposition::Reused)
    );
    assert!(
        edited
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
    );
    directory.put("math.zil", "pub fn value() -> bool => true\n");
    code(
        &session.compile_path(&input, &options),
        ResultCategory::SafeReject,
        "E-TYPE-MISMATCH",
    );
    assert_eq!(session.active_query_count(), 0);
    // A rejected dependency edit does not overwrite the last good executable.
    assert_eq!(
        Command::new(options.output.as_ref().unwrap())
            .status()
            .unwrap()
            .code(),
        Some(42)
    );
}

#[test]
fn m004_signature_and_implementation_hashes_track_different_inputs() {
    let options = CompileOptions::default();
    let first = compile_project(
        &memory(
            "import math\nfn main() -> i32 => 0",
            "pub fn value() -> i32 => 1",
        ),
        &options,
    );
    let changed = compile_project(
        &memory(
            "import math\nfn main() -> i32 => 0",
            "pub fn value() -> bool => true",
        ),
        &options,
    );
    success(&first);
    success(&changed);
    assert_ne!(
        first.scalar_interfaces[1].signature_fingerprint,
        changed.scalar_interfaces[1].signature_fingerprint
    );
    let mut remapped = memory(
        "import math\nfn main() -> i32 => 0",
        "pub fn value() -> i32 => 1",
    );
    remapped.modules.reverse();
    let remapped = compile_project(&remapped, &options);
    success(&remapped);
    assert_eq!(
        first.scalar_interfaces[1].signature_fingerprint,
        remapped.scalar_interfaces[0].signature_fingerprint
    );
    assert_ne!(
        first.scalar_interfaces[1].exports[0].function,
        remapped.scalar_interfaces[0].exports[0].function
    );
}

#[test]
fn m005_memory_entry_has_no_implicit_filesystem_imports() {
    let directory = Directory::new();
    let path = directory.put("main.zil", "import math\nfn main() -> i32 => math.value()");
    directory.put("math.zil", "pub fn value() -> i32 => 42");
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        path.to_string_lossy(),
        fs::read_to_string(&path).unwrap(),
    );
    let report = compile_source(&source, &CompileOptions::default());
    code(&report, ResultCategory::Unsupported, "E-UNSUPPORTED");
    let mut project = memory(&source.text, "pub fn value() -> i32 => 42");
    project.paths.clear();
    code(
        &compile_project(&project, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-NOT-FOUND",
    );
}

#[test]
fn m006_missing_cycles_and_invalid_paths_have_honest_categories() {
    let directory = Directory::new();
    let input = directory.put("main.zil", "import missing\nfn main() -> i32 => 0");
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-NOT-FOUND",
    );
    directory.put("main.zil", "import math\nfn main() -> i32 => 0");
    directory.put("math.zil", "import main\npub fn value() -> i32 => 1");
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    let mut project = memory("fn main() -> i32 => 0", "pub fn value() -> i32 => 1");
    project.paths = BTreeMap::from([(vec!["..".into(), "escape".into()], ModuleId(11))]);
    code(
        &compile_project(&project, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-PATH",
    );
    project.paths.clear();
    project.modules[1].module_id = project.root;
    code(
        &compile_project(&project, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-ID",
    );
}

#[test]
fn m007_every_dependency_is_protected_from_output_overwrite() {
    let directory = Directory::new();
    let input = directory.put("main.zil", "import math\nfn main() -> i32 => math.value()");
    let library = directory.put("math.zil", "pub fn value() -> i32 => 42");
    let bytes = fs::read(&library).unwrap();
    let report = compile_path(
        &input,
        &CompileOptions {
            output: Some(library.clone()),
            ..directory.build_options()
        },
    );
    code(&report, ResultCategory::InfraError, "E-OUTPUT-SOURCE");
    assert_eq!(fs::read(&library).unwrap(), bytes);
    assert!(report.query_events.is_empty());
    let dump_source = directory.put("dump/core.txt", "pub fn value() -> i32 => 42");
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                input.to_string_lossy(),
                "fn main() -> i32 => 0",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                dump_source.to_string_lossy(),
                fs::read_to_string(&dump_source).unwrap(),
            ),
        ],
        paths: BTreeMap::new(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            emit_dir: Some(directory.0.join("dump")),
            ..CompileOptions::default()
        },
    );
    code(&report, ResultCategory::InfraError, "E-OUTPUT-SOURCE");
    assert_eq!(fs::read(&dump_source).unwrap(), bytes);
}

#[test]
fn m008_source_and_frontend_function_and_block_quotas_are_cumulative() {
    let project = memory(
        "import math\nfn main() -> i32 => math.value()",
        "pub fn value() -> i32 => 42",
    );
    let tokens: Vec<_> = project
        .modules
        .iter()
        .map(|s| zil_syntax::parse(s).tokens.len())
        .collect();
    for budget in 0..5 {
        let mut options = CompileOptions::default();
        match budget {
            0 => {
                options.limits.max_source_bytes =
                    project.modules.iter().map(|s| s.text.len()).max().unwrap()
            }
            1 => options.limits.max_tokens = *tokens.iter().max().unwrap(),
            2 => options.limits.max_nodes = *tokens.iter().max().unwrap(),
            3 => options.limits.max_functions = 1,
            4 => options.limits.max_blocks = 1,
            _ => unreachable!(),
        }
        code(
            &compile_project(&project, &options),
            ResultCategory::ResourceLimit,
            "E-RESOURCE-LIMIT",
        );
    }
}

#[test]
fn m009_second_module_receives_only_remaining_ct_steps_before_execution() {
    let project = memory(
        "import math\nfn main() -> i32 => #(6 * 7) + math.value()",
        "pub fn value() -> i32 => #(5 + 7)",
    );
    let defaults = CompileOptions::default();
    let usage: Vec<_> = project
        .modules
        .iter()
        .map(|source| {
            let parsed = zil_syntax::parse(source).into_result().unwrap();
            let core = zil_expand::expand(source, &parsed).unwrap();
            zil_ct::evaluate_source_constants_with_usage(&core, &defaults.target, &defaults.limits)
                .unwrap()
                .1
                .steps
        })
        .collect();
    assert!(usage.iter().all(|n| *n > 0));
    let mut limited = CompileOptions::default();
    limited.limits.max_ct_steps = *usage.iter().max().unwrap();
    let report = compile_project(&project, &limited);
    code(&report, ResultCategory::ResourceLimit, "E-RESOURCE-LIMIT");
    assert_eq!(report.phase, Phase::Ct);
    limited.limits.max_ct_steps = usage.iter().sum();
    success(&compile_project(&project, &limited));
    let poisoned = memory(
        &project.modules[0].text,
        "pub fn value() -> i32 => #(1 / 0)",
    );
    limited.limits.max_ct_steps = usage[0];
    // No remaining step allowance: the second expression must be stopped
    // before arithmetic evaluation, rather than report division by zero.
    code(
        &compile_project(&poisoned, &limited),
        ResultCategory::ResourceLimit,
        "E-RESOURCE-LIMIT",
    );
}

#[test]
fn m010_dependency_diagnostics_preserve_file_identity() {
    let project = memory(
        "import math\nfn main() -> i32 => math.value()",
        "pub fn value() -> i32 => true",
    );
    let report = compile_project(&project, &CompileOptions::default());
    code(&report, ResultCategory::SafeReject, "E-TYPE-MISMATCH");
    let span = report.diagnostics[0].span.unwrap();
    assert_eq!(span.file, FileId(21));
    assert_eq!(report.source_files[&span.file], "library.zil");
}

#[cfg(unix)]
#[test]
fn m011_symlink_escape_rejected_and_internal_aliases_share_one_identity() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let outside = Directory::new();
    outside.put("external.zil", "pub fn value() -> i32 => 1");
    let input = directory.put("main.zil", "import math\nfn main() -> i32 => math.value()");
    symlink(outside.0.join("external.zil"), directory.0.join("math.zil")).unwrap();
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-PATH",
    );
    fs::remove_file(directory.0.join("math.zil")).unwrap();
    directory.put("math.zil", "pub fn value() -> i32 => 21");
    symlink(Path::new("math.zil"), directory.0.join("alias.zil")).unwrap();
    directory.put(
        "main.zil",
        "import math\nimport alias\nfn main() -> i32 => math.value() + alias.value()",
    );
    let report = compile_path(&input, &directory.build_options());
    assert_eq!(exit(&report), 42);
    assert_eq!(report.source_files.len(), 2);
}

#[test]
fn m012_graph_mapping_edits_invalidate_even_with_identical_source_bytes() {
    let mut project = memory(
        "import math\nfn main() -> i32 => math.value()",
        "pub fn value() -> i32 => 41",
    );
    project.modules.push(SourceUnit::new(
        FileId(22),
        ModuleId(12),
        "second.zil",
        "pub fn value() -> i32 => 42",
    ));
    let mut session = CompilationSession::default();
    let directory = Directory::new();
    let options = directory.build_options();
    let first = session.compile_project(&project, &options);
    assert_eq!(exit(&first), 41);
    project.paths.insert(vec!["math".into()], ModuleId(12));
    let second = session.compile_project(&project, &options);
    assert_eq!(exit(&second), 42);
    assert_ne!(first.input_fingerprint, second.input_fingerprint);
    assert!(
        second
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
    );
}

#[test]
fn m013_raw_program_calls_use_explicit_ids_and_validate_export_references() {
    use zil_base::{Fingerprint, NodeId};
    use zil_ir::{CoreImport, CoreProgram, InstructionKind};
    let project = memory(
        "import math\nfn main() -> i32 => math.value()",
        "pub fn value() -> i32 => 42",
    );
    let modules = project
        .modules
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let ast = zil_syntax::parse(source).into_result().unwrap();
            let imports = if index == 0 {
                vec![CoreImport {
                    alias: "math".into(),
                    module: ModuleId(11),
                    span: ast.items[0].span,
                }]
            } else {
                Vec::new()
            };
            zil_expand::expand_program_module(source, &ast, imports)
                .unwrap()
                .0
        })
        .collect();
    let program = CoreProgram {
        root: project.root,
        source_fingerprint: Fingerprint::of(b"raw-program-test"),
        modules,
    };
    let options = CompileOptions::default();
    let typed = zil_types::check_program(&program, &options.target, &options.limits).unwrap();
    let exported_id = typed.interfaces[1].exports[0].function;
    assert!(typed.module.functions[0].blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(&instruction.kind, InstructionKind::Call { function, .. } if *function == exported_id)));
    let mut missing = program.clone();
    missing.modules[0].imports[0].module = ModuleId(999);
    assert_eq!(
        zil_types::check_program(&missing, &options.target, &options.limits).unwrap_err()[0].code,
        "E-MODULE-NOT-FOUND"
    );
    let mut forged = program.clone();
    forged.modules[1].exports.push(NodeId(u32::MAX));
    assert_eq!(
        zil_types::check_program(&forged, &options.target, &options.limits).unwrap_err()[0].code,
        "E-MODULE-EXPORT"
    );
    let mut private = program;
    private.modules[1].exports.clear();
    assert_eq!(
        zil_types::check_program(&private, &options.target, &options.limits).unwrap_err()[0].code,
        "E-MODULE-PRIVATE"
    );
}

#[test]
fn m014_imported_main_cannot_supply_a_missing_root_entry() {
    let directory = Directory::new();
    let project = memory(
        "import math\nfn alternative() -> i32 => math.main()",
        "pub fn main() -> i32 => 42",
    );
    code(
        &compile_project(&project, &directory.build_options()),
        ResultCategory::SafeReject,
        "E-CODEGEN-ENTRY",
    );
    let report = compile_project(
        &project,
        &CompileOptions {
            entry: Some("alternative".into()),
            ..directory.build_options()
        },
    );
    success(&report);
    let output = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "42");
}

#[test]
fn m015_cross_module_ct_preserves_context_visibility_and_verified_calls() {
    let directory = Directory::new();
    let root = "import math as wide\nfn value() -> i64 => wide.identity(#(2147483647 + 1))";
    let library = "pub fn identity(x: i64) -> i64 => x";
    let options = CompileOptions {
        entry: Some("value".into()),
        ..directory.build_options()
    };
    let report = compile_project(&memory(root, library), &options);
    success(&report);
    let output = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "2147483648"
    );

    // A header hint does not grant visibility, including for a large literal.
    let report = compile_project(
        &memory(root, "fn identity(x: i64) -> i64 => x"),
        &CompileOptions::default(),
    );
    code(&report, ResultCategory::SafeReject, "E-MODULE-PRIVATE");
    assert_eq!(report.phase, Phase::Types);
    for (root, library, expected) in [
        (
            "import math as wide\nfn value() -> i64 => wide.identity(#(1), 2)",
            library,
            "E-CALL-ARITY",
        ),
        (
            "import math as wide\nfn value() -> i64 => wide.identity(true)",
            library,
            "E-TYPE-MISMATCH",
        ),
        (
            "import math as wide\nfn value() -> i64 => math.identity(#(1))",
            library,
            "E-UNKNOWN-NAME",
        ),
        (
            "import math as wide\nimport math as wide\nfn value() -> i64 => wide.identity(#(1))",
            library,
            "E-MODULE-ALIAS",
        ),
    ] {
        code(
            &compile_project(&memory(root, library), &CompileOptions::default()),
            ResultCategory::SafeReject,
            expected,
        );
    }
    // 027 added actual verified CT function execution. Header hints alone still
    // grant no authority; this query must survive ordinary types and CT verify.
    let query = "import math as wide\nfn value() -> i64 => #(wide.identity(2147483648))";
    let report = compile_project(
        &memory(query, library),
        &CompileOptions {
            emit_dir: Some(directory.0.join("ct-call-ir")),
            ..options
        },
    );
    success(&report);
    let output = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"2147483648\n");
    assert!(
        !fs::read_to_string(&report.artifacts["ct-core"])
            .unwrap()
            .contains("CompileTime")
    );
    let private_query = compile_project(
        &memory(query, "fn identity(x: i64) -> i64 => x"),
        &CompileOptions::default(),
    );
    code(
        &private_query,
        ResultCategory::SafeReject,
        "E-MODULE-PRIVATE",
    );
    assert_eq!(private_query.phase, Phase::Ct);
}

#[test]
fn m016_ct_header_index_charges_graph_and_signature_work() {
    let project = memory("fn main() -> i32 => 0", "pub fn value(x: i64) -> i64 => x");
    let program = zil_ir::CoreProgram {
        root: project.root,
        source_fingerprint: zil_base::Fingerprint::of(b"CT-header-budget"),
        modules: project
            .modules
            .iter()
            .map(|source| {
                let ast = zil_syntax::parse(source).into_result().unwrap();
                zil_expand::expand_program_module(source, &ast, Vec::new())
                    .unwrap()
                    .0
            })
            .collect(),
    };
    for axis in 0..3 {
        let mut limits = zil_base::Limits::default();
        match axis {
            0 => limits.max_nodes = 2,
            1 => limits.max_functions = 1,
            _ => limits.max_analysis_steps = 1,
        }
        let errors = zil_ct::SourceProgramContext::new(&program, &limits)
            .err()
            .expect("index must be bounded");
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    }
}

#[test]
fn m017_file_parse_stop_only_checks_entry_syntax() {
    let directory = Directory::new();
    let options = CompileOptions {
        stop_after: Stage::Parse,
        ..CompileOptions::default()
    };
    directory.put("math.zil", "this is not valid module syntax");
    for text in [
        "import absent\nfn main() -> i32 => 0",
        "syntax use absent as macro\nfn main() -> i32 => 0",
        "import math\nfn main() -> i32 => 0",
    ] {
        let input = directory.put("main.zil", text);
        let source = SourceUnit::new(FileId(0), ModuleId(0), input.to_string_lossy(), text);
        let memory = compile_source(&source, &options);
        let report = compile_path(&input, &options);
        success(&memory);
        success(&report);
        assert_eq!(report.phase, Phase::Parse);
        assert_eq!(report.input_fingerprint, memory.input_fingerprint);
        assert_eq!(report.source_files.len(), 1);
        assert_eq!(report.query_events.len(), 1);
        assert_eq!(report.query_events[0].phase, Phase::Parse);
        assert_eq!(report.verification_runs, 0);
    }
    // The stop still checks the requested source; it does not suppress errors.
    let input = directory.put("main.zil", "this is not valid module syntax");
    let report = compile_path(&input, &options);
    code(
        &report,
        ResultCategory::SafeReject,
        "E-PARSE-MODULE-STATEMENT",
    );
    assert_eq!(report.phase, Phase::Parse);

    let options = CompileOptions {
        stop_after: Stage::Core,
        ..options
    };
    directory.put("main.zil", "import absent\nfn main() -> i32 => 0");
    code(
        &compile_path(&input, &options),
        ResultCategory::SafeReject,
        "E-MODULE-NOT-FOUND",
    );
    directory.put(
        "main.zil",
        "syntax use absent as macro\nfn main() -> i32 => 0",
    );
    code(
        &compile_path(&input, &options),
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    directory.put("main.zil", "import math\nfn main() -> i32 => 0");
    code(
        &compile_path(&input, &options),
        ResultCategory::SafeReject,
        "E-PARSE-MODULE-STATEMENT",
    );
}

#[cfg(unix)]
#[test]
fn m018_file_parse_stop_does_not_resolve_escaping_symlinks() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let outside = Directory::new();
    outside.put("external.zil", "pub fn value() -> i32 => 42");
    symlink(outside.0.join("external.zil"), directory.0.join("math.zil")).unwrap();
    let input = directory.put("main.zil", "import math\nfn main() -> i32 => 0");
    let options = CompileOptions {
        stop_after: Stage::Parse,
        ..CompileOptions::default()
    };
    let report = compile_path(&input, &options);
    success(&report);
    assert_eq!(report.phase, Phase::Parse);
    assert_eq!(report.source_files.len(), 1);
    code(
        &compile_path(
            &input,
            &CompileOptions {
                stop_after: Stage::Core,
                ..options
            },
        ),
        ResultCategory::SafeReject,
        "E-MODULE-PATH",
    );
}

#[test]
fn m019_file_core_stop_resolves_graph_without_executing_ct() {
    let directory = Directory::new();
    let input = directory.put("main.zil", "import math\nfn main() -> i32 => math.value()");
    directory.put("math.zil", "pub fn value() -> i32 => #(1 / 0)");
    let options = CompileOptions {
        stop_after: Stage::Core,
        ..CompileOptions::default()
    };
    let report = compile_path(&input, &options);
    success(&report);
    assert_eq!(report.phase, Phase::Expand);
    assert_eq!(report.source_files.len(), 2);
    assert!(
        report
            .query_events
            .iter()
            .all(|event| matches!(event.phase, Phase::Parse | Phase::Expand))
    );
    assert_eq!(report.verification_runs, 0);
    let report = compile_path(
        &input,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..options
        },
    );
    code(&report, ResultCategory::SafeReject, "E-CT-ARITHMETIC");
    assert_eq!(report.phase, Phase::Ct);
}

#[test]
fn m020_directory_entry_is_a_single_module_with_explicit_dependencies() {
    let directory = Directory::new();
    let input = directory.put(
        "main.zil",
        "import tools.number\nfn main() -> i32 => number.answer()",
    );
    directory.put(
        "tools/number/mod.zil",
        "import tools.number.detail as core\npub fn answer() -> i32 => core.value() + 1",
    );
    directory.put("tools/number/detail.zil", "pub fn value() -> i32 => 41");
    directory.put("tools/number/unused.zil", "this must never be parsed");
    let report = compile_path(&input, &directory.build_options());
    assert_eq!(exit(&report), 42);
    assert_eq!(report.source_files.len(), 3);
    assert_eq!(report.scalar_interfaces.len(), 3);
}

#[test]
fn m021_two_entries_are_rejected_including_preseeded_root_identity() {
    let directory = Directory::new();
    let input = directory.put("main.zil", "import math\nfn main() -> i32 => math.value()");
    directory.put("math.zil", "pub fn value() -> i32 => 1");
    directory.put("math/mod.zil", "pub fn value() -> i32 => 2");
    let report = compile_path(&input, &CompileOptions::default());
    code(&report, ResultCategory::SafeReject, "E-MODULE-AMBIGUOUS");
    assert!(report.diagnostics[0].message.contains("math.zil"));
    assert!(report.diagnostics[0].message.contains("mod.zil"));
    directory.put("main.zil", "import main as root\nfn main() -> i32 => 0");
    directory.put("main/mod.zil", "pub fn value() -> i32 => 2");
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-AMBIGUOUS",
    );
}

#[test]
fn m022_directory_without_entry_is_not_a_glob_or_merged_namespace() {
    let directory = Directory::new();
    let input = directory.put("main.zil", "import values\nfn main() -> i32 => 0");
    directory.put("values/one.zil", "pub fn one() -> i32 => 1");
    directory.put("values/invalid.zil", "this must never be parsed");
    let report = compile_path(&input, &CompileOptions::default());
    code(&report, ResultCategory::SafeReject, "E-MODULE-NOT-FOUND");
    assert!(
        report.diagnostics[0]
            .message
            .contains("directories are not scanned")
    );
    directory.put("values/mod.zil", "pub fn two() -> i32 => 2");
    directory.put(
        "main.zil",
        "import values\nfn main() -> i32 => values.one()",
    );
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-MEMBER",
    );
}

#[test]
fn m023_zil_suffix_is_a_logical_segment_and_never_stripped() {
    let directory = Directory::new();
    let input = directory.put(
        "main.zil",
        "import a.zil as entry\nfn main() -> i32 => entry.value()",
    );
    directory.put("a.zil", "pub fn value() -> i32 => 1");
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-NOT-FOUND",
    );
    directory.put("a/zil.zil", "pub fn value() -> i32 => 43");
    assert_eq!(exit(&compile_path(&input, &directory.build_options())), 43);
    fs::remove_file(directory.0.join("a/zil.zil")).unwrap();
    directory.put("a/zil/mod.zil", "pub fn value() -> i32 => 44");
    assert_eq!(exit(&compile_path(&input, &directory.build_options())), 44);
}

#[test]
#[cfg(unix)]
fn m024_directory_aliases_share_identity_and_cannot_escape_the_root() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let outside = Directory::new();
    outside.put("mod.zil", "pub fn value() -> i32 => 1");
    let input = directory.put("main.zil", "import math\nfn main() -> i32 => math.value()");
    symlink(&outside.0, directory.0.join("math")).unwrap();
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-PATH",
    );
    fs::remove_file(directory.0.join("math")).unwrap();
    directory.put("math/mod.zil", "pub fn value() -> i32 => 21");
    symlink(Path::new("math"), directory.0.join("alias")).unwrap();
    directory.put(
        "main.zil",
        "import math\nimport alias\nfn main() -> i32 => math.value() + alias.value()",
    );
    let report = compile_path(&input, &directory.build_options());
    assert_eq!(exit(&report), 42);
    assert_eq!(report.source_files.len(), 2);
    // Even two names for the same physical entry do not remove ambiguity.
    symlink(Path::new("math/mod.zil"), directory.0.join("math.zil")).unwrap();
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-AMBIGUOUS",
    );
}

#[test]
fn m025_directory_edits_and_layout_changes_match_cold_compilation() {
    let directory = Directory::new();
    let input = directory.put(
        "main.zil",
        "import math.values\nfn main() -> i32 => values.value()",
    );
    directory.put("math/values/mod.zil", "pub fn value() -> i32 => 40");
    let options = directory.build_options();
    let mut session = CompilationSession::default();
    let first = session.compile_path(&input, &options);
    assert_eq!(exit(&first), 40);
    let hot = session.compile_path(&input, &options);
    assert_eq!(hot.verification_runs, 1);
    assert_eq!(hot.input_fingerprint, first.input_fingerprint);
    assert!(
        hot.query_events
            .iter()
            .any(|event| event.phase == Phase::Types
                && event.disposition == QueryDisposition::Reused)
    );
    directory.put("math/values/mod.zil", "pub fn value() -> i32 => 41");
    let edited = session.compile_path(&input, &options);
    let cold = compile_path(&input, &options);
    assert_eq!(exit(&edited), 41);
    assert_eq!(edited.status, cold.status);
    assert_eq!(edited.input_fingerprint, cold.input_fingerprint);
    assert_ne!(first.input_fingerprint, edited.input_fingerprint);
    assert_eq!(edited.verification_runs, 1);
    // A newly introduced competing entry invalidates acceptance, even hot.
    directory.put("math/values.zil", "pub fn value() -> i32 => 42");
    code(
        &session.compile_path(&input, &options),
        ResultCategory::SafeReject,
        "E-MODULE-AMBIGUOUS",
    );
    fs::remove_file(directory.0.join("math/values/mod.zil")).unwrap();
    let moved = session.compile_path(&input, &options);
    assert_eq!(exit(&moved), 42);
    assert_eq!(
        moved.input_fingerprint,
        compile_path(&input, &options).input_fingerprint
    );
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn m026_directory_imports_keep_parse_stops_and_cumulative_source_budgets() {
    let directory = Directory::new();
    let input = directory.put(
        "main.zil",
        "import math.values\nfn main() -> i32 => values.value()",
    );
    directory.put("math/values/mod.zil", "this is deliberately invalid");
    let parse_options = CompileOptions {
        stop_after: Stage::Parse,
        ..CompileOptions::default()
    };
    let report = compile_path(&input, &parse_options);
    success(&report);
    assert_eq!(report.source_files.len(), 1);
    directory.put("math/values.zil", "also invalid and ambiguous");
    success(&compile_path(&input, &parse_options));
    code(
        &compile_path(&input, &CompileOptions::default()),
        ResultCategory::SafeReject,
        "E-MODULE-AMBIGUOUS",
    );
    fs::remove_file(directory.0.join("math/values.zil")).unwrap();
    let source = "pub fn value() -> i32 => 42";
    directory.put("math/values/mod.zil", source);
    let mut options = CompileOptions::default();
    options.limits.max_source_bytes = fs::read(&input).unwrap().len() + source.len() - 1;
    let report = compile_path(&input, &options);
    assert_eq!(
        report.status,
        ResultCategory::ResourceLimit,
        "{}",
        report.to_json()
    );
    options.limits.max_source_bytes += 1;
    success(&compile_path(&input, &options));
    // Multiple logical edges to one physical module do not reread or recharge
    // the module's bytes. The root itself is still fully included.
    directory.put(
        "main.zil",
        "import math.values as first\nimport math.values as second\nfn main() -> i32 => first.value() + second.value()",
    );
    options.limits.max_source_bytes = fs::read(&input).unwrap().len() + source.len();
    let report = compile_path(&input, &options);
    success(&report);
    assert_eq!(report.source_files.len(), 2);
    options.limits.max_source_bytes -= 1;
    assert_eq!(
        compile_path(&input, &options).status,
        ResultCategory::ResourceLimit,
    );
}

#[test]
fn m027_numberlab_compiles_real_algorithms_through_file_and_directory_modules() {
    let directory = Directory::new();
    let input = directory.put(
        "main.zil",
        include_str!("../../../examples/numberlab/main.zil"),
    );
    directory.put(
        "math/gcd.zil",
        include_str!("../../../examples/numberlab/math/gcd.zil"),
    );
    directory.put(
        "math/primes/mod.zil",
        include_str!("../../../examples/numberlab/math/primes/mod.zil"),
    );
    assert_eq!(exit(&compile_path(&input, &directory.build_options())), 0);
    let report = compile_path(
        &input,
        &CompileOptions {
            entry: Some("result".into()),
            ..directory.build_options()
        },
    );
    success(&report);
    let output = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "76316");
    assert_eq!(report.source_files.len(), 3);
    assert_eq!(report.verification_runs, 1);
}
