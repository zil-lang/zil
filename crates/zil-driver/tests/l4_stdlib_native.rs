//! End-to-end dual-backend (Clang + LLVM-API, O0 + O2) native stdlib suite,
//! cross-module field privacy soundness tests, out-of-repo embedded stdlib
//! resolution, and `std.*` hijack protection tests (Batch L4-1 / G9.3).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization,
    ProjectSources, Stage, compile_project,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn library_fixture(rel: &str) -> PathBuf {
    repo_root()
        .join("tests/language-readiness/terminal/library-and-ui")
        .join(rel)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l4-1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(&path, src).expect("write source");
    path
}

fn dual_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    configs
}

fn build_and_run_dual(
    dir: &Path,
    tag: &str,
    entry: &Path,
    stdin_bytes: Option<&[u8]>,
) -> (i32, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{tag}-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let mut cmd = Command::new(&exe);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let out = if let Some(input) = stdin_bytes {
            cmd.stdin(Stdio::piped());
            let mut child = cmd
                .spawn()
                .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", exe.display()));
            child
                .stdin
                .as_mut()
                .expect("piped stdin")
                .write_all(input)
                .expect("write stdin");
            child
                .wait_with_output()
                .unwrap_or_else(|e| panic!("failed to wait on {}: {e}", exe.display()))
        } else {
            cmd.stdin(Stdio::null())
                .output()
                .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()))
        };

        assert!(
            out.stderr.is_empty(),
            "unexpected stderr for {} ({label}): {}",
            entry.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let code = out.status.code().unwrap_or(-1);
        if let Some((prev_code, ref prev_stdout)) = expected {
            assert_eq!(
                code,
                prev_code,
                "exit code mismatch for {} on {label}",
                entry.display()
            );
            assert_eq!(
                out.stdout,
                *prev_stdout,
                "stdout mismatch for {} on {label}",
                entry.display()
            );
        } else {
            expected = Some((code, out.stdout));
        }
    }
    expected.expect("at least one backend ran")
}

#[test]
fn stdlib_consumers_execute_natively_on_dual_backends() {
    let dir = temp_dir("stdlib-consumers");
    let fixtures = [
        ("utf8", "utf8.zil"),
        ("int", "int.zil"),
        ("string", "string.zil"),
        ("string_scalar_index", "string_scalar_index.zil"),
        ("string_unicode", "string_unicode.zil"),
        ("store", "store.zil"),
        ("store_insert", "store-insert.zil"),
        ("collections", "collections.zil"),
        ("i64_hash_map", "i64_hash_map.zil"),
        ("i64_hash_set", "i64_hash_set.zil"),
        ("pod_hash_map", "pod_hash_map.zil"),
        ("string_map", "string_map.zil"),
        ("string_map_index", "string_map_index.zil"),
        ("ui_input", "ui_input.zil"),
        ("ui_layout", "ui_layout.zil"),
        ("ui_raster", "ui_raster.zil"),
        ("growing_map", "growing-hash-collections/map.zil"),
        ("growing_set", "growing-hash-collections/set.zil"),
    ];

    for (tag, rel) in fixtures {
        let path = library_fixture(rel);
        let (code, _stdout) = build_and_run_dual(&dir, tag, &path, None);
        assert_eq!(code, 0, "fixture {rel} exited with {code}");
    }

    // `standard_io.zil` copies stdin to stdout using `std.io`.
    let io_path = library_fixture("standard_io.zil");
    let payload = b"hello zil std.io\n";
    let (code, stdout) = build_and_run_dual(&dir, "standard_io", &io_path, Some(payload));
    assert_eq!(code, 0, "standard_io.zil exited with {code}");
    assert_eq!(stdout, payload);
}

#[test]
fn stdlib_field_privacy_and_type_negatives_reject_cleanly() {
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };

    // 1. `i64_hash_set_field_negative.zil` accesses private `set.state` field across module boundary.
    let field_neg = library_fixture("i64_hash_set_field_negative.zil");
    let report = CompilationSession::new().compile_path(&field_neg, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected SafeReject for i64_hash_set_field_negative.zil, got: {:?}",
        report.diagnostics
    );
    assert_eq!(report.phase, Phase::Types);
    assert!(
        report.diagnostics.iter().any(|d| d.code == "E-VISIBILITY"),
        "expected E-VISIBILITY for i64_hash_set_field_negative.zil, got: {:?}",
        report.diagnostics
    );

    // 2. `i64_hash_set_type_negative.zil` passes `&mut I64HashSet` where `&mut I64HashMap` is expected.
    let type_neg = library_fixture("i64_hash_set_type_negative.zil");
    let report = CompilationSession::new().compile_path(&type_neg, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected SafeReject for i64_hash_set_type_negative.zil, got: {:?}",
        report.diagnostics
    );
    assert_eq!(report.phase, Phase::Types);

    // 3. Cross-module private field read, write, literal construction, and pattern destructuring.
    let dir = temp_dir("field-privacy");
    write_source(
        &dir,
        "secret.zil",
        r#"
pub struct Boxed {
    pub visible: i32
    hidden: i32
}

pub fn make(v: i32, h: i32) -> Boxed {
    return Boxed { visible: v, hidden: h }
}

pub fn hidden_val(b: &Boxed) -> i32 {
    return b.hidden
}
"#,
    );

    // Positive: accessing public field and calling helper works natively.
    let pos = write_source(
        &dir,
        "pos_main.zil",
        r#"
import secret

fn main() -> i32 {
    var b = secret.make(10, 32)
    b.visible = b.visible + 1
    if b.visible != 11 { return 1 }
    if secret.hidden_val(&b) != 32 { return 2 }
    return 0
}
"#,
    );
    let (code, _) = build_and_run_dual(&dir, "field_privacy_pos", &pos, None);
    assert_eq!(code, 0);

    // Negative cases: read, write, construction, destructuring of private `hidden`.
    let neg_cases = [
        (
            "neg_read.zil",
            "import secret\nfn main() -> i32 { let b = secret.make(1, 2); return b.hidden }\n",
        ),
        (
            "neg_write.zil",
            "import secret\nfn main() -> i32 { var b = secret.make(1, 2); b.hidden = 9; return 0 }\n",
        ),
        (
            "neg_construct.zil",
            "import secret\nfn main() -> i32 { let b = secret.Boxed { visible: 1, hidden: 2 }; return b.visible }\n",
        ),
        (
            "neg_destructure.zil",
            "import secret\nfn main() -> i32 { let b = secret.make(1, 2); match b { secret.Boxed { visible: v, hidden: h } => { return v + h } } }\n",
        ),
    ];

    for (name, src) in neg_cases {
        let entry = write_source(&dir, name, src);
        let rep = CompilationSession::new().compile_path(&entry, &opts);
        assert_eq!(
            rep.status,
            ResultCategory::SafeReject,
            "expected SafeReject for {name}, got: {:?}",
            rep.diagnostics
        );
        assert_eq!(rep.phase, Phase::Types, "expected Phase::Types for {name}");
        assert!(
            rep.diagnostics.iter().any(|d| d.code == "E-VISIBILITY"),
            "expected E-VISIBILITY for {name}, got: {:?}",
            rep.diagnostics
        );
    }
}

#[test]
fn out_of_repo_embedded_stdlib_and_hijack_protection() {
    let dir = temp_dir("embedded-and-hijack");

    // 1. Out-of-repo compilation importing `std.utf8`, `std.string`, and `std.store`.
    let out_of_repo = write_source(
        &dir,
        "out_of_repo_main.zil",
        r#"
import std.utf8
import std.string
import std.store

fn main() -> string.Error!i32 {
    let bytes = [65u8, 66u8, 67u8]
    let bounds = utf8.locate(&bytes[..], 0, 2)
    if bounds.error >= 0 || bounds.count != 3 { return 1 }

    let s = string.from_utf8("hello", 32)?
    if string.byte_len(&s) != 5 { return 2 }

    let st: Store<i64> = store_new()
    if !store_reserve(st, 4) { return 3 }
    store_push(st, 10)
    store_push(st, 20)
    if store.len(st) != 2 { return 4 }
    return 0
}
"#,
    );
    let (code, _) = build_and_run_dual(&dir, "out_of_repo", &out_of_repo, None);
    assert_eq!(code, 0);

    // 2. Local `std/utf8.zil` hijack attempt next to entry file must NOT hijack canonical `std.utf8`.
    let hijack_dir = temp_dir("local-std-hijack");
    write_source(
        &hijack_dir,
        "std/utf8.zil",
        r#"
pub fn hijacked_marker() -> i32 {
    return 99
}
"#,
    );
    let hijack_probe = write_source(
        &hijack_dir,
        "probe_hijack.zil",
        r#"
import std.utf8
fn main() -> i32 {
    return utf8.hijacked_marker()
}
"#,
    );
    let verify_opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let hijack_rep = CompilationSession::new().compile_path(&hijack_probe, &verify_opts);
    assert_eq!(
        hijack_rep.status,
        ResultCategory::SafeReject,
        "local std/utf8.zil must not hijack canonical std.utf8: {:?}",
        hijack_rep.diagnostics
    );

    // Meanwhile, importing canonical `std.utf8` from the same directory still resolves canonical stdlib!
    let canonical_use = write_source(
        &hijack_dir,
        "canonical_use.zil",
        r#"
import std.utf8
fn main() -> i32 {
    let bytes = [65u8, 66u8]
    let bounds = utf8.locate(&bytes[..], 0, 1)
    if bounds.error >= 0 || bounds.count != 2 { return 1 }
    return 0
}
"#,
    );
    let (code, _) = build_and_run_dual(&hijack_dir, "canonical_with_decoy", &canonical_use, None);
    assert_eq!(code, 0);

    // 3. Forged `std.*` module in `ProjectSources` must fail with `SafeReject` / `E-STD-RESERVED`.
    let canonical_utf8 = std::fs::read_to_string(repo_root().join("lib/std/utf8.zil"))
        .expect("read lib/std/utf8.zil");
    let entry_src = r#"
import std.utf8
fn main() -> i32 {
    let bytes = [65u8]
    let b = utf8.locate(&bytes[..], 0, 0)
    if b.error >= 0 { return 1 }
    return 0
}
"#;

    // 3a. Exact canonical `std.utf8` in `ProjectSources` is accepted.
    let valid_proj = ProjectSources {
        entry: ModuleId(0),
        sources: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main", entry_src),
            SourceUnit::new(FileId(1), ModuleId(1), "std.utf8", canonical_utf8.clone()),
        ],
        paths: BTreeMap::from([(vec!["std".into(), "utf8".into()], ModuleId(1))]),
    };
    let valid_rep = compile_project(&valid_proj, &verify_opts);
    assert_eq!(
        valid_rep.status,
        ResultCategory::Success,
        "canonical std.utf8 in ProjectSources should succeed: {:?}",
        valid_rep.diagnostics
    );

    // 3b. Even an innocuous modification to `std.utf8` in `ProjectSources` is rejected with `E-STD-RESERVED`.
    let mut forged_utf8 = canonical_utf8;
    forged_utf8.push_str("\n// forged comment\n");
    let forged_proj = ProjectSources {
        entry: ModuleId(0),
        sources: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main", entry_src),
            SourceUnit::new(FileId(1), ModuleId(1), "std.utf8", forged_utf8),
        ],
        paths: BTreeMap::from([(vec!["std".into(), "utf8".into()], ModuleId(1))]),
    };
    let forged_rep = compile_project(&forged_proj, &verify_opts);
    assert_eq!(forged_rep.status, ResultCategory::SafeReject);
    assert_eq!(forged_rep.phase, Phase::Driver);
    assert!(
        forged_rep
            .diagnostics
            .iter()
            .any(|d| d.code == "E-STD-RESERVED"),
        "expected E-STD-RESERVED, got: {:?}",
        forged_rep.diagnostics
    );

    // 3c. Invented `std.fake` module in `ProjectSources` is also rejected with `E-STD-RESERVED`.
    let fake_proj = ProjectSources {
        entry: ModuleId(0),
        sources: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main",
                "import std.fake\nfn main() -> i32 { return fake.v() }\n",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "std.fake",
                "pub fn v() -> i32 { return 0 }\n",
            ),
        ],
        paths: BTreeMap::from([(vec!["std".into(), "fake".into()], ModuleId(1))]),
    };
    let fake_rep = compile_project(&fake_proj, &verify_opts);
    assert_eq!(fake_rep.status, ResultCategory::SafeReject);
    assert_eq!(fake_rep.phase, Phase::Driver);
    assert!(
        fake_rep
            .diagnostics
            .iter()
            .any(|d| d.code == "E-STD-RESERVED"),
        "expected E-STD-RESERVED for std.fake, got: {:?}",
        fake_rep.diagnostics
    );

    // 3d. Forged `lib/std/utf8.zil` unit path in `ProjectSources` is rejected with `E-STD-RESERVED`.
    let forged_lib_std_proj = ProjectSources {
        entry: ModuleId(0),
        sources: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main", entry_src),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "lib/std/utf8.zil",
                "pub fn hijacked_marker() -> i32 { return 99 }\n",
            ),
        ],
        paths: BTreeMap::new(),
    };
    let forged_lib_std_rep = compile_project(&forged_lib_std_proj, &verify_opts);
    assert_eq!(forged_lib_std_rep.status, ResultCategory::SafeReject);
    assert_eq!(forged_lib_std_rep.phase, Phase::Driver);
    assert!(
        forged_lib_std_rep
            .diagnostics
            .iter()
            .any(|d| d.code == "E-STD-RESERVED"),
        "expected E-STD-RESERVED for forged lib/std/utf8.zil, got: {:?}",
        forged_lib_std_rep.diagnostics
    );

    // 4. Local AND ancestor `lib/std/utf8.zil` / `lib/std/fake.zil` MUST NOT override embedded
    // canonical stdlib in `compile_path`.
    let lib_std_hijack_dir = temp_dir("lib-std-ancestor-hijack");
    write_source(
        &lib_std_hijack_dir,
        "lib/std/utf8.zil",
        r#"
pub fn hijacked_marker() -> i32 {
    return 99
}
"#,
    );
    write_source(
        &lib_std_hijack_dir,
        "lib/std/fake.zil",
        r#"
pub fn forged_std() -> i32 {
    return 7
}
"#,
    );
    for rel in ["probe_direct.zil", "nested/sub/probe_ancestor.zil"] {
        let probe = write_source(
            &lib_std_hijack_dir,
            rel,
            r#"
import std.utf8
fn main() -> i32 {
    return utf8.hijacked_marker()
}
"#,
        );
        let rep = CompilationSession::new().compile_path(&probe, &verify_opts);
        assert_eq!(
            rep.status,
            ResultCategory::SafeReject,
            "lib/std/utf8.zil must not override embedded canonical std.utf8 ({rel}): {:?}",
            rep.diagnostics
        );
        assert_eq!(rep.phase, Phase::Types);
    }

    let fake_probe = write_source(
        &lib_std_hijack_dir,
        "nested/sub/probe_fake.zil",
        r#"
import std.fake
fn main() -> i32 {
    return fake.forged_std()
}
"#,
    );
    let fake_path_rep = CompilationSession::new().compile_path(&fake_probe, &verify_opts);
    assert_eq!(
        fake_path_rep.status,
        ResultCategory::SafeReject,
        "lib/std/fake.zil must not be loaded by compile_path: {:?}",
        fake_path_rep.diagnostics
    );
    assert_eq!(fake_path_rep.phase, Phase::Ct);

    // Canonical `std.utf8` still compiles and runs natively from inside the decoy tree!
    let canonical_under_decoy = write_source(
        &lib_std_hijack_dir,
        "nested/sub/canonical_under_lib_std.zil",
        r#"
import std.utf8
fn main() -> i32 {
    let bytes = [65u8, 66u8, 67u8]
    let bounds = utf8.locate(&bytes[..], 0, 2)
    if bounds.error >= 0 || bounds.count != 3 { return 1 }
    return 0
}
"#,
    );
    let (code, _) = build_and_run_dual(
        &lib_std_hijack_dir,
        "canonical_under_lib_std",
        &canonical_under_decoy,
        None,
    );
    assert_eq!(code, 0);
}
