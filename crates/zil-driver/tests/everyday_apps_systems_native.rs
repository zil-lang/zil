//! Dual-backend (`clang-o0`, `clang-o2`, and `llvmapi-o0`/`llvmapi-o2` when
//! `llvm-api` is enabled) end-to-end native tests for the three systems everyday
//! ZIL multi-module applications:
//! 1. `examples/everyday-apps/http-mini-service-and-client`
//! 2. `examples/everyday-apps/file-batch-processor`
//! 3. `examples/everyday-apps/graph-dependency-analyzer`
//!
//! Each application is loaded via `ProjectManifest::open`, compiled through the
//! full verified pipeline, executed as a real native ELF binary against real
//! sockets/files/directories/CLI arguments (including error & cleanup paths),
//! and paired with nearest domain safety-negative controls.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization,
    ProjectManifest, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zil-everyday-apps-systems-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
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

fn build_project_dual(app_dir: &Path, out_dir: &Path, tag: &str) -> Vec<(&'static str, PathBuf)> {
    let manifest = ProjectManifest::open(app_dir, &CompileOptions::default().limits)
        .unwrap_or_else(|e| panic!("ProjectManifest::open({}) failed: {e:?}", app_dir.display()));
    let manifest = Arc::new(manifest);
    let mut built = Vec::new();
    for (label, backend, opt) in dual_configs() {
        let exe = out_dir.join(format!("bin-{tag}-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            project: Some(Arc::clone(&manifest)),
            build_inputs: manifest.fingerprint,
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(&manifest.entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            app_dir.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);
        built.push((label, exe));
    }
    built
}

fn run_exe(exe: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
    )
}

fn assert_safe_reject(dir: &Path, filename: &str, src: &str, expected_code_prefix: &str) {
    let entry = dir.join(filename);
    std::fs::write(&entry, src).expect("write negative control");
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected {filename} to SafeReject, got {:?} with diagnostics: {:?}",
        report.status,
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(expected_code_prefix)),
        "expected diagnostic prefix `{expected_code_prefix}` for {filename}, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn http_mini_service_and_client_dual_backend_native() {
    let app_dir = repo_root().join("examples/everyday-apps/http-mini-service-and-client");
    let work = temp_dir("http-mini-service");
    let built = build_project_dual(&app_dir, &work, "http-mini-service");

    let expected_stdout = concat!(
        "GET /health -> 200 ok:v1\n",
        "GET /echo?msg=zil-alpha -> 200 echo:zil-alpha\n",
        "GET /echo?a=1&msg=&b=2 -> 200 echo:\n",
        "GET /echo -> 400 missing msg\n",
        "POST /upper -> 200 HELLO ZIL 42\n",
        "GET /missing -> 404 not found\n",
        "BAD -> 400 bad request\n",
        "BAD_CL -> 400 bad request\n",
        "BAD_DUP_CL -> 400 bad request\n",
        "BAD_VER -> 400 bad request\n",
        "BAD_HDR -> 400 bad request\n",
        "SERVED 11\n",
    );

    for (label, exe) in built {
        let (code, stdout, stderr) = run_exe(&exe, &[]);
        assert_eq!(
            code, 0,
            "http app exited with {code} on {label}; stdout={stdout:?}; stderr={stderr:?}"
        );
        assert!(stderr.is_empty(), "unexpected stderr on {label}: {stderr}");
        assert_eq!(stdout, expected_stdout, "stdout mismatch on {label}");
    }
}

#[test]
fn file_batch_processor_dual_backend_native() {
    let app_dir = repo_root().join("examples/everyday-apps/file-batch-processor");
    let work = temp_dir("file-batch");
    let built = build_project_dual(&app_dir, &work, "file-batch");

    // Prepare a real input directory with target (.txt/.log) and non-target entries.
    let in_dir = work.join("input");
    std::fs::create_dir_all(in_dir.join("nested_subdir")).expect("create nested");
    std::fs::write(
        in_dir.join("alpha.txt"),
        "  hello world  \r\n\r\nmy secret key and SECRET token\n",
    )
    .expect("write alpha.txt");
    std::fs::write(in_dir.join("beta.log"), "event ok\n  warn secret \n").expect("write beta.log");
    std::fs::write(in_dir.join("gamma.txt"), "\n   \nfinal line\n").expect("write gamma.txt");
    std::fs::write(in_dir.join("ignored.bin"), [0xffu8, 0x00u8, 0xfeu8]).expect("write bin");
    std::fs::write(in_dir.join(".hidden.txt"), "should not be processed").expect("write dot");

    // Prepare a bad input directory containing invalid UTF-8 in a target .txt file.
    let bad_utf8_dir = work.join("bad_utf8_input");
    std::fs::create_dir_all(&bad_utf8_dir).expect("create bad_utf8_dir");
    std::fs::write(bad_utf8_dir.join("broken.txt"), [0xffu8, 0xfeu8, 0x80u8])
        .expect("write broken.txt");

    // Prepare a regular file where an output directory is expected (error path).
    let file_as_out = work.join("not_a_dir");
    std::fs::write(&file_as_out, "occupied").expect("write file_as_out");

    let missing_dir = work.join("does_not_exist");

    let mut baseline_stdout: Option<String> = None;
    for (label, exe) in built {
        let out_dir = work.join(format!("out-{label}"));
        let (code, stdout, stderr) = run_exe(
            &exe,
            &[
                in_dir.to_str().expect("utf8"),
                out_dir.to_str().expect("utf8"),
            ],
        );
        assert_eq!(
            code, 0,
            "file-batch-processor failed on {label}: stdout={stdout:?} stderr={stderr:?}"
        );
        assert!(stderr.is_empty(), "unexpected stderr on {label}: {stderr}");

        // Verify exact transformed contents and that non-targets / temp files do not exist.
        assert_eq!(
            std::fs::read_to_string(out_dir.join("alpha.txt")).expect("read alpha"),
            "HELLO WORLD\nMY [REDACTED] KEY AND [REDACTED] TOKEN\n"
        );
        assert_eq!(
            std::fs::read_to_string(out_dir.join("beta.log")).expect("read beta"),
            "EVENT OK\nWARN [REDACTED]\n"
        );
        assert_eq!(
            std::fs::read_to_string(out_dir.join("gamma.txt")).expect("read gamma"),
            "FINAL LINE\n"
        );
        assert!(!out_dir.join("ignored.bin").exists());
        assert!(!out_dir.join(".hidden.txt").exists());
        assert!(!out_dir.join("nested_subdir").exists());
        for entry in std::fs::read_dir(&out_dir).expect("read out_dir") {
            let name = entry.expect("entry").file_name();
            let name_str = name.to_string_lossy();
            assert!(
                !name_str.ends_with(".tmp"),
                "stale temp file left behind on {label}: {name_str}"
            );
        }

        // Verify exact report lines and cross-backend determinism.
        assert!(stdout.contains("FILE alpha.txt lines=2 words=8 redacted=2 bytes=51 "));
        assert!(stdout.contains("FILE beta.log lines=2 words=4 redacted=1 bytes=25 "));
        assert!(stdout.contains("FILE gamma.txt lines=1 words=2 redacted=0 bytes=11 "));
        assert!(stdout.contains("TOTAL files=3 lines=5 words=14 redacted=3 bytes=87\n"));
        if let Some(ref prev) = baseline_stdout {
            assert_eq!(&stdout, prev, "stdout mismatch across backends ({label})");
        } else {
            baseline_stdout = Some(stdout);
        }

        // Error path 1: missing CLI arguments -> exit 2.
        let (c_args, _, e_args) = run_exe(&exe, &[]);
        assert_eq!(c_args, 2, "missing args exit mismatch on {label}");
        assert!(e_args.contains("usage:"));

        // Error path 2: missing input directory -> exit 3, output dir not created.
        let unused_out = work.join(format!("unused-out-{label}"));
        let (c_miss, _, e_miss) = run_exe(
            &exe,
            &[
                missing_dir.to_str().expect("utf8"),
                unused_out.to_str().expect("utf8"),
            ],
        );
        assert_eq!(c_miss, 3, "missing input dir exit mismatch on {label}");
        assert!(e_miss.contains("missing or invalid input directory"));
        assert!(!unused_out.exists());

        // Error path 3: output path is an existing regular file -> exit 4.
        let (c_out, _, e_out) = run_exe(
            &exe,
            &[
                in_dir.to_str().expect("utf8"),
                file_as_out.to_str().expect("utf8"),
            ],
        );
        assert_eq!(c_out, 4, "invalid output dir exit mismatch on {label}");
        assert!(e_out.contains("cannot prepare output directory"));

        // Error path 4: invalid UTF-8 input file -> exit 5, no partial .tmp files left behind.
        let bad_out = work.join(format!("bad-out-{label}"));
        let (c_bad, _, e_bad) = run_exe(
            &exe,
            &[
                bad_utf8_dir.to_str().expect("utf8"),
                bad_out.to_str().expect("utf8"),
            ],
        );
        assert_eq!(c_bad, 5, "invalid utf8 exit mismatch on {label}");
        assert!(e_bad.contains("batch processing failed"));
        if let Some(entry) = std::fs::read_dir(&bad_out).expect("read bad_out").next() {
            let name = entry.expect("entry").file_name();
            panic!(
                "unexpected partial file left on error path ({label}): {}",
                name.to_string_lossy()
            );
        }
    }
}

#[test]
fn graph_dependency_analyzer_dual_backend_native() {
    let app_dir = repo_root().join("examples/everyday-apps/graph-dependency-analyzer");
    let work = temp_dir("graph-analyzer");
    let built = build_project_dual(&app_dir, &work, "graph-analyzer");

    let dag_spec = work.join("workspace.deps");
    std::fs::write(
        &dag_spec,
        concat!(
            "# Diamond workspace dependency graph\n",
            "node base\n",
            "node solo\n",
            "dep auth -> base\n",
            "dep net -> base\n",
            "dep api -> auth\n",
            "dep api -> net\n",
            "dep cli -> api\n",
            "dep cli -> net\n",
        ),
    )
    .expect("write dag spec");

    let cyclic_spec = work.join("cyclic.deps");
    std::fs::write(
        &cyclic_spec,
        concat!(
            "dep alpha -> beta\n",
            "dep beta -> gamma\n",
            "dep gamma -> alpha\n",
            "dep leaf -> base\n",
        ),
    )
    .expect("write cyclic spec");

    let malformed_spec = work.join("malformed.deps");
    std::fs::write(&malformed_spec, "not_a_valid_directive foo bar\n")
        .expect("write malformed spec");

    let missing_spec = work.join("missing.deps");

    let expected_builtin = concat!(
        "STATUS ACYCLIC\n",
        "NODES 5 EDGES 5\n",
        "ORDER core,net,tls,http,app\n",
        "LEVELS core=0,net=1,tls=1,http=2,app=3\n",
        "CLOSURE app core,http,net,tls\n",
        "PATH app -> core app -> http -> net -> core\n",
    );

    let expected_dag = concat!(
        "STATUS ACYCLIC\n",
        "NODES 6 EDGES 6\n",
        "ORDER base,auth,net,api,cli,solo\n",
        "LEVELS base=0,auth=1,net=1,api=2,cli=3,solo=0\n",
        "CLOSURE cli api,auth,base,net\n",
        "PATH cli -> base cli -> net -> base\n",
    );

    let expected_unreachable = concat!(
        "STATUS ACYCLIC\n",
        "NODES 6 EDGES 6\n",
        "ORDER base,auth,net,api,cli,solo\n",
        "LEVELS base=0,auth=1,net=1,api=2,cli=3,solo=0\n",
        "CLOSURE cli api,auth,base,net\n",
        "PATH cli -> solo NONE\n",
    );

    let expected_cycle = concat!("STATUS CYCLIC\n", "CYCLE alpha -> beta -> gamma -> alpha\n",);

    for (label, exe) in built {
        // 1. Built-in default DAG mode.
        let (c0, out0, err0) = run_exe(&exe, &[]);
        assert_eq!(c0, 0, "builtin mode failed on {label}: stderr={err0:?}");
        assert!(err0.is_empty());
        assert_eq!(out0, expected_builtin, "builtin stdout mismatch on {label}");

        // 2. File-based DAG with transitive closure and shortest path (`cli -> net -> base`).
        let (c1, out1, err1) = run_exe(&exe, &[dag_spec.to_str().expect("utf8"), "cli", "base"]);
        assert_eq!(c1, 0, "dag mode failed on {label}: stderr={err1:?}");
        assert!(err1.is_empty());
        assert_eq!(out1, expected_dag, "dag stdout mismatch on {label}");

        // 3. Unreachable shortest path query returns `NONE`.
        let (c2, out2, err2) = run_exe(&exe, &[dag_spec.to_str().expect("utf8"), "cli", "solo"]);
        assert_eq!(c2, 0, "unreachable path query failed on {label}");
        assert!(err2.is_empty());
        assert_eq!(
            out2, expected_unreachable,
            "unreachable mismatch on {label}"
        );

        // 4. Cyclic graph exits 1 and reports exact closed cycle path.
        let (c3, out3, err3) = run_exe(&exe, &[cyclic_spec.to_str().expect("utf8")]);
        assert_eq!(c3, 1, "cyclic graph should exit 1 on {label}");
        assert!(err3.is_empty());
        assert_eq!(out3, expected_cycle, "cycle report mismatch on {label}");

        // 5. Error paths: missing file (exit 2), malformed spec (exit 3), unknown query node (exit 4).
        let (c_miss, _, e_miss) = run_exe(&exe, &[missing_spec.to_str().expect("utf8")]);
        assert_eq!(c_miss, 2, "missing spec should exit 2 on {label}");
        assert!(e_miss.contains("cannot read graph spec file"));

        let (c_bad, _, e_bad) = run_exe(&exe, &[malformed_spec.to_str().expect("utf8")]);
        assert_eq!(c_bad, 3, "malformed spec should exit 3 on {label}");
        assert!(e_bad.contains("invalid graph specification"));

        let (c_unk, _, e_unk) = run_exe(&exe, &[dag_spec.to_str().expect("utf8"), "nonexistent"]);
        assert_eq!(c_unk, 4, "unknown query node should exit 4 on {label}");
        assert!(e_unk.contains("unknown query node"));
    }
}

#[test]
fn everyday_apps_paired_nearest_safety_negative_controls() {
    let work = temp_dir("safety-negatives");

    // Control 1 (HTTP mini service domain): returning a borrowed slice pointing into
    // a local request ByteBuffer dropped at function exit must be rejected.
    assert_safe_reject(
        &work,
        "http_dangling_request_slice.zil",
        r#"
fn dangling_request_line() -> &[u8] {
    let raw = bytes_new()
    bytes_push(raw, 71u8)
    bytes_push(raw, 69u8)
    bytes_push(raw, 84u8)
    return bytes_view(raw)
}

fn main() -> i32 {
    let line = dangling_request_line()
    if line.len() == 3 { return 0 }
    return 1
}
"#,
        "E-SAFETY-",
    );

    // Control 2 (HTTP mini service domain): moving a Mutex containing a foreign-Drop
    // `TcpListener` across `thread_scope` must be rejected by the independent verifier.
    assert_safe_reject(
        &work,
        "http_mutex_listener_thread_escape.zil",
        r#"
import std.net

package ServerState {
    manages listener: net.TcpListener
}

fn server_worker(cell: Mutex<ServerState>) -> i32 {
    var guard = mutex_lock(cell)
    let state = mutex_value(&mut guard)
    match state.listener.local_port() {
        .ok(_) => { return 0 },
        .err(_) => { return 1 }
    }
}

fn main() -> i32 {
    match net.TcpListener.bind(net.localhost(), 0) {
        .err(_) => { return 1 },
        .ok(listener) => {
            match mutex_create(ServerState { listener: give listener }) {
                .err(_) => { return 2 },
                .ok(cell) => {
                    let (ok, child, parent) = thread_scope(server_worker, cell, () -> i32 => 0)
                    if !ok { return 3 }
                    return child + parent
                }
            }
        }
    }
}
"#,
        "E-SAFETY-",
    );

    // Control 3 (File batch processor domain): sending a locked `MutexGuard`
    // across `thread_scope` to a worker thread must be rejected.
    assert_safe_reject(
        &work,
        "file_batch_mutex_guard_thread_escape.zil",
        r#"
package BatchQueue {
    manages work_buf: ByteBuffer;
    pub next_index: i64
}

fn bad_worker(guard: MutexGuard<BatchQueue>) -> i32 {
    return 0
}

fn main() -> MutexCreateError!i32 {
    let cell = mutex_create(BatchQueue {
        work_buf: bytes_new(),
        next_index: 0
    })?
    let guard = mutex_lock(cell)
    let (ok, child, parent) = thread_scope(bad_worker, guard, () -> i32 => 0)
    if !ok { return 1 }
    return child + parent
}
"#,
        "E-SAFETY-",
    );

    // Control 4 (Graph dependency analyzer domain): holding a borrowed node name
    // reference from `store_at` across a mutating `store_push` on the same `Store`
    // must be rejected (prevents reallocation dangling reference).
    assert_safe_reject(
        &work,
        "graph_store_borrow_across_push.zil",
        r#"
import std.string

fn main() -> i32 {
    let names: Store<string.String> = store_new()
    store_reserve(names, 4)
    match string.from_utf8("core", 64) {
        .err(_) => { return 1 },
        .ok(first) => {
            match store_push(names, first) {
                .err(_) => { return 2 },
                .ok(_) => {}
            }
        }
    }
    let borrowed = store_at(names, 0)
    match string.from_utf8("net", 64) {
        .err(_) => { return 3 },
        .ok(second) => {
            match store_push(names, second) {
                .err(_) => { return 4 },
                .ok(_) => {}
            }
        }
    }
    if string.byte_len(borrowed) == 4 { return 0 }
    return 5
}
"#,
        "E-SAFETY-",
    );
}
