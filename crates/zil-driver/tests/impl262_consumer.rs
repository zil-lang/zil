//! Fixed Type/Syntax source consumers; this test grants no generator authority.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, ProjectSources,
    Stage, SyntaxLock, compile_path, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const COUNTERS: [&str; 6] = [
    "alloc_calls",
    "allocated",
    "frees",
    "failures",
    "live",
    "peak_live",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../")
}

fn backend() -> NativeBackend {
    if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    }
}

fn directory(label: &str) -> PathBuf {
    let parent = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    parent.join(format!(
        "consumer262-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn names() -> Vec<String> {
    let result = Command::new("python3")
        .args(["-c", "import json,sys;print('\\n'.join(c['name'] for c in json.load(open(sys.argv[1]))['positive']))"])
        .arg(root().join("tests/impl262/ORACLE.json"))
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let names = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 9);
    names
}

struct Input {
    project: ProjectSources,
    entry: PathBuf,
    lock: Option<SyntaxLock>,
}

fn materialize(name: &str, variant: &str, out: &Path) -> Input {
    let result = Command::new("python3")
        .arg(root().join("tools/language/262/materialize.py"))
        .args(["--case", name, "--variant", variant, "--out"])
        .arg(out)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    fs::write(out.join("materialize.stdout"), &result.stdout).unwrap();
    fs::write(out.join("materialize.stderr"), &result.stderr).unwrap();
    let text = String::from_utf8(result.stdout).unwrap();
    let mut lines = text.lines();
    let entry = PathBuf::from(lines.next().unwrap());
    let lock = match lines.next().unwrap() {
        "-" => None,
        path => Some(SyntaxLock::parse(&fs::read_to_string(path).unwrap()).unwrap()),
    };
    let mut modules = Vec::new();
    let mut paths = BTreeMap::new();
    for line in lines {
        let columns = line.split('\t').collect::<Vec<_>>();
        assert_eq!(columns.len(), 3);
        let index: u32 = columns[0].parse().unwrap();
        assert_eq!(index as usize, modules.len());
        modules.push(SourceUnit::new(
            FileId(index),
            ModuleId(index),
            columns[2],
            fs::read_to_string(columns[2]).unwrap(),
        ));
        if index != 0 {
            paths.insert(
                columns[1].split('.').map(str::to_owned).collect(),
                ModuleId(index),
            );
        }
    }
    Input {
        project: ProjectSources {
            root: ModuleId(0),
            modules,
            paths,
        },
        entry,
        lock,
    }
}

struct Expected {
    stdout: String,
    exit: i32,
    fail_at: Option<u64>,
    counters: BTreeMap<String, u64>,
    json: String,
}

fn rows(name: &str, allocator: bool) -> Vec<Expected> {
    let script = r#"
import json,sys
case=next(c for c in json.load(open(sys.argv[1]))['positive'] if c['name']==sys.argv[2])
rows=case['allocator'] if sys.argv[3]=='allocator' else [case['normal']]
for e in rows:
    keys=['alloc_calls','allocated','frees','failures','live','peak_live']
    print('\t'.join([str(e['exit']),e['stdout'],str(e.get('fail_at','-'))]+[str(e[k]) for k in keys]+[json.dumps(e,sort_keys=True)]))
"#;
    let result = Command::new("python3")
        .args(["-c", script])
        .arg(root().join("tests/impl262/ORACLE.json"))
        .args([name, if allocator { "allocator" } else { "normal" }])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), 10);
            Expected {
                exit: fields[0].parse().unwrap(),
                stdout: fields[1].into(),
                fail_at: (fields[2] != "-").then(|| fields[2].parse().unwrap()),
                counters: COUNTERS
                    .iter()
                    .zip(&fields[3..9])
                    .map(|(name, value)| ((*name).into(), value.parse().unwrap()))
                    .collect(),
                json: fields[9].into(),
            }
        })
        .collect()
}

fn options(input: &Input, out: &Path, optimization: Optimization, stage: Stage) -> CompileOptions {
    CompileOptions {
        syntax_lock: input.lock.clone(),
        stop_after: stage,
        backend: backend(),
        optimization,
        output: Some(out.join("program")),
        emit_dir: Some(out.join("phases")),
        ..CompileOptions::default()
    }
}

fn assert_builder(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let native = report.native_codegen.as_ref().unwrap();
    assert_eq!(native.backend, backend());
    assert!(native.fallback.is_none());
    if cfg!(feature = "llvm-api") {
        assert!(native.actual_units > 0);
        assert_eq!(native.object_reports.len(), native.actual_units);
        assert!(
            native
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"verify_before_after\":true"))
        );
    }
}

fn execute(binary: &Path, out: &Path, expected: &Expected, allocator: bool) {
    fs::create_dir_all(out).unwrap();
    fs::write(out.join("stdin"), b"").unwrap();
    let mut command = Command::new(binary);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if allocator {
        command.env("ZIL_TEST_FAIL_AT", expected.fail_at.unwrap().to_string());
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(&[]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{} timed out", binary.display());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let result = child.wait_with_output().unwrap();
    fs::write(out.join("stdout"), &result.stdout).unwrap();
    fs::write(out.join("stderr"), &result.stderr).unwrap();
    fs::write(
        out.join("exit.txt"),
        format!("{:?}\n", result.status.code()),
    )
    .unwrap();
    fs::write(out.join("expected.json"), &expected.json).unwrap();
    assert_eq!(
        result.status.code(),
        Some(expected.exit),
        "{}",
        out.display()
    );
    assert_eq!(
        result.stdout,
        expected.stdout.as_bytes(),
        "{}",
        out.display()
    );
    if allocator {
        let stderr = std::str::from_utf8(&result.stderr).unwrap().trim();
        assert!(stderr.starts_with('{') && stderr.ends_with('}'));
        let counters = stderr[1..stderr.len() - 1]
            .split(',')
            .map(|pair| {
                let (key, value) = pair.split_once(':').unwrap();
                (
                    key.trim().trim_matches('"'),
                    value.trim().parse::<u64>().unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for key in COUNTERS {
            assert_eq!(
                counters[key],
                expected.counters[key],
                "{} {key}",
                out.display()
            );
        }
    } else {
        assert!(result.stderr.is_empty(), "{}: {result:?}", out.display());
    }
}

#[test]
fn frozen_source_operations_and_explicit_provider_locks_are_unchanged() {
    let result = Command::new("python3")
        .arg(root().join("tools/language/262/oracle.py"))
        .arg("--check")
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
}

#[test]
fn all_generated_source_projects_reach_real_verify() {
    verify_sources("generated");
}

#[test]
fn handwritten_controls_use_valid_existing_zil_syntax_and_reach_verify() {
    verify_sources("handwritten");
}

fn verify_sources(variant: &str) {
    let mut failures = Vec::new();
    for name in names() {
        let out = directory(&format!("{name}-{variant}-verify"));
        let input = materialize(&name, variant, &out);
        let report = compile_project(
            &input.project,
            &options(&input, &out, Optimization::O0, Stage::Verify),
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert!(!report.artifacts.contains_key("executable"));
        if report.status != ResultCategory::Success || report.verification_runs != 1 {
            failures.push(format!("{name}: {}", report.to_json()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn rejection(entry: &str) -> (Phase, &'static str, &'static [&'static str]) {
    match entry {
        "neg_frozen_emit_scalar_main.zil"
        | "neg_emit_expr_main.zil"
        | "neg_splice_wrong_category_main.zil"
        | "neg_type_requires_type_value_main.zil"
        | "neg_type_splice_expr_main.zil"
        | "neg_item_emit_statements_main.zil" => (Phase::Ct, "E-CT-CATEGORY", &[]),
        "neg_syntax_is_not_automatic_main.zil"
        | "neg_type_in_runtime_main.zil"
        | "neg_ident_in_runtime_main.zil" => (Phase::Ct, "E-CT-RUNTIME", &[]),
        "neg_duplicate_export_main.zil" => (Phase::Expand, "E-NAME-DUPLICATE", &["duplicate"]),
        "neg_private_reflection_main.zil" => (Phase::Ct, "E-CT-VISIBILITY", &[]),
        "neg_nominal_identity_main.zil" => (Phase::Types, "E-TYPE-MISMATCH", &[]),
        "neg_duplicate_owned_input_main.zil" => {
            (Phase::Analyze, "E-UNINITIALIZED", &["not initialized"])
        }
        "neg_runtime_read_in_ct_main.zil" => (Phase::Ct, "E-CT-SOURCE", &["runtime"]),
        "neg_unknown_provider_export_main.zil" => (Phase::Expand, "E-SYNTAX-LOCK-ENTRY", &[]),
        "neg_member_owner_main.zil" => (Phase::Ct, "E-CT-MEMBER-OWNER", &[]),
        "neg_layout_cycle_main.zil" => (Phase::Ct, "E-CT-PHASE-CYCLE", &[]),
        "neg_export_cannot_capture_main.zil" => (Phase::Ct, "E-CT-EXPORT-CAPTURE", &[]),
        "neg_late_import_main.zil" => (Phase::Expand, "E-CT-LATE-IMPORT", &[]),
        _ => panic!("missing source-risk diagnostic obligation: {entry}"),
    }
}

#[test]
fn invalid_generated_sources_reach_their_real_category_stage_or_ownership_reason() {
    let script = "import json,sys;print('\\n'.join(c['entry'] for c in json.load(open(sys.argv[1]))['negative']))";
    let listed = Command::new("python3")
        .args(["-c", script])
        .arg(root().join("tests/impl262/ORACLE.json"))
        .output()
        .unwrap();
    assert!(listed.status.success(), "{listed:?}");
    let names = String::from_utf8(listed.stdout).unwrap();
    assert_eq!(names.lines().count(), 19);
    let mut failures = Vec::new();
    for entry in names.lines() {
        let out = directory(entry.trim_end_matches(".zil"));
        let input = materialize(entry, "generated", &out);
        let report = compile_project(
            &input.project,
            &options(&input, &out, Optimization::O0, Stage::Verify),
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        let (phase, code, fragments) = rejection(entry);
        fs::write(out.join("expected-diagnostic.json"),format!("{{\"category\":\"SafeReject\",\"phase\":\"{}\",\"code\":\"{code}\",\"message_fragments\":{fragments:?},\"obligation_from\":\"tests/impl262/ORACLE.json negative entry {entry}\"}}\n",phase.as_str())).unwrap();
        assert!(!report.artifacts.contains_key("executable"));
        assert!(!out.join("program").exists());
        assert!(report.native_codegen.is_none());
        if report.status != ResultCategory::SafeReject
            || !report.diagnostics.iter().any(|diagnostic| {
                diagnostic.category == ResultCategory::SafeReject
                    && diagnostic.phase == phase
                    && diagnostic.code == code
                    && fragments
                        .iter()
                        .all(|fragment| diagnostic.message.contains(fragment))
            })
        {
            failures.push(format!("{entry}: {}", report.to_json()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn explicit_provider_locks_fail_closed_without_repair_or_order_fallback() {
    let out = directory("lock-boundaries");
    let input = materialize("03_providers_hygiene", "generated", &out);
    let control = out.join("positive-control");
    fs::create_dir_all(&control).unwrap();
    let report = compile_project(
        &input.project,
        &options(&input, &control, Optimization::O0, Stage::Verify),
    );
    fs::write(control.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for (name, code) in [
        ("missing_lock", "E-SYNTAX-LOCK-MISSING"),
        ("missing_default", "E-SYNTAX-LOCK-MISSING"),
        ("stale_source", "E-SYNTAX-LOCK-STALE"),
        ("wrong_grammar", "E-SYNTAX-LOCK-GRAMMAR"),
        ("duplicate_entry", "E-SYNTAX-LOCK-DUPLICATE"),
        ("wrong_rule", "E-SYNTAX-LOCK-RULE"),
    ] {
        let directory = out.join(name);
        fs::create_dir_all(&directory).unwrap();
        let mut request = options(&input, &directory, Optimization::O0, Stage::Verify);
        if name == "missing_lock" {
            request.syntax_lock = None;
        } else {
            let generated = Command::new("python3")
                .arg(root().join("tools/language/262/lock_variant.py"))
                .arg(name)
                .output()
                .unwrap();
            assert!(generated.status.success(), "{generated:?}");
            fs::write(directory.join("request.lock"), &generated.stdout).unwrap();
            let text = String::from_utf8(generated.stdout).unwrap();
            let parsed = SyntaxLock::parse(&text);
            if matches!(name, "duplicate_entry" | "wrong_rule") {
                let diagnostics = parsed.unwrap_err();
                fs::write(
                    directory.join("parse-diagnostics.txt"),
                    format!("{diagnostics:#?}"),
                )
                .unwrap();
                assert!(
                    diagnostics.iter().any(|diagnostic| diagnostic.category
                        == ResultCategory::SafeReject
                        && diagnostic.phase == Phase::Expand
                        && diagnostic.code == code),
                    "{diagnostics:?}"
                );
                continue;
            }
            request.syntax_lock = Some(parsed.unwrap());
        }
        let report = compile_project(&input.project, &request);
        fs::write(directory.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.phase == Phase::Expand && diagnostic.code == code),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
        assert!(report.native_codegen.is_none());
    }
}

#[test]
fn explicit_file_and_memory_graphs_have_the_same_inputs_and_results() {
    for name in [
        "01_type_values",
        "03_providers_hygiene",
        "07_fresh_and_named",
    ] {
        let out = directory(&format!("{name}-input-equivalence"));
        let input = materialize(name, "generated", &out);
        let mut results = Vec::new();
        for filesystem in [false, true] {
            let directory = out.join(if filesystem { "filesystem" } else { "memory" });
            fs::create_dir_all(&directory).unwrap();
            let request = options(&input, &directory, Optimization::O0, Stage::Verify);
            let report = if filesystem {
                compile_path(&input.entry, &request)
            } else {
                compile_project(&input.project, &request)
            };
            fs::write(directory.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert!(!report.artifacts.contains_key("executable"));
            results.push((report.input_fingerprint, report.scalar_interfaces));
        }
        assert_eq!(results[0], results[1], "{name}");
    }
}

#[test]
fn one_request_ct_step_budget_has_exact_cold_warm_n_and_n_minus_one() {
    fn probe(input: &Input, out: &Path, session: &mut CompilationSession, steps: u64) -> bool {
        fs::create_dir_all(out).unwrap();
        let mut request = options(input, out, Optimization::O0, Stage::Verify);
        request.limits.max_ct_steps = steps;
        let report = session.compile_project(&input.project, &request);
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert!(!report.artifacts.contains_key("executable"));
        if report.status == ResultCategory::Success {
            assert_eq!(report.verification_runs, 1);
            true
        } else {
            assert_eq!(
                report.status,
                ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.phase == Phase::Ct
                        && diagnostic.code == "E-RESOURCE-LIMIT"),
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 0);
            false
        }
    }
    let out = directory("shared-ct-steps");
    let input = materialize("budget_single", "generated", &out);
    let mut session = CompilationSession::default();
    let mut high = CompileOptions::default().limits.max_ct_steps;
    // A missing language capability stops here. It cannot become a budget
    // success through repeated unsupported requests or a lowered expectation.
    assert!(probe(&input, &out.join("default"), &mut session, high));
    let mut low = 0;
    let mut sequence = 0;
    while low < high {
        let middle = low + (high - low) / 2;
        let pass = probe(
            &input,
            &out.join(format!("search-{sequence}-N{middle}")),
            &mut session,
            middle,
        );
        sequence += 1;
        if pass {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let n = low;
    assert!(n > 0);
    assert!(probe(
        &input,
        &out.join("cold-N"),
        &mut CompilationSession::default(),
        n
    ));
    assert!(!probe(
        &input,
        &out.join("cold-N-minus-1"),
        &mut CompilationSession::default(),
        n - 1
    ));
    assert!(probe(&input, &out.join("warm-N"), &mut session, n));
    assert!(!probe(
        &input,
        &out.join("warm-N-minus-1"),
        &mut session,
        n - 1
    ));
    assert!(probe(
        &input,
        &out.join("fresh-N"),
        &mut CompilationSession::default(),
        n
    ));
    let double_out = out.join("two-source-calls");
    let double = materialize("budget_double", "generated", &double_out);
    assert!(probe(
        &double,
        &double_out.join("default"),
        &mut CompilationSession::default(),
        CompileOptions::default().limits.max_ct_steps
    ));
    assert!(!probe(
        &double,
        &double_out.join("single-N"),
        &mut CompilationSession::default(),
        n
    ));
    fs::write(out.join("budget.json"), format!("{{\"axis\":\"max_ct_steps\",\"N\":{n},\"search_requests\":{sequence},\"cold_N\":\"Pass\",\"cold_N_minus_1\":\"ResourceLimit\",\"warm_N\":\"Pass\",\"warm_N_minus_1\":\"ResourceLimit\",\"fresh_N\":\"Pass\",\"two_actual_CT_calls_at_single_N\":\"ResourceLimit\",\"native_runs\":0,\"cache_hit_scope\":\"actual query_events in each raw report; no CT-cache-hit claim inferred from another phase\"}}\n")).unwrap();
}

#[test]
fn generated_and_handwritten_programs_execute_the_frozen_oracle_at_o0_o2() {
    for name in names() {
        for variant in ["generated", "handwritten"] {
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = directory(&format!("{name}-{variant}-normal-{optimization:?}"));
                let input = materialize(&name, variant, &out);
                let report = compile_project(
                    &input.project,
                    &options(&input, &out, optimization, Stage::Build),
                );
                fs::write(out.join("report.json"), report.to_json()).unwrap();
                assert_builder(&report);
                for expected in rows(&name, false) {
                    execute(
                        &report.artifacts["executable"],
                        &out.join("case-0"),
                        &expected,
                        false,
                    );
                }
            }
        }
    }
}

#[cfg(all(unix, feature = "llvm-api"))]
#[test]
fn generated_and_handwritten_owners_follow_actual_allocator_failures() {
    use std::os::unix::fs::PermissionsExt;
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    for name in ["05_generated_allocation", "05b_generated_early_error"] {
        for variant in ["generated", "handwritten"] {
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = directory(&format!("{name}-{variant}-allocator-{optimization:?}"));
                let input = materialize(name, variant, &out);
                let wrapper = out.join("capture-object.sh");
                fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\ncase \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&out.join("api.o")),quote(&clang))).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                let mut request = options(&input, &out, optimization, Stage::Build);
                request.clang = Some(wrapper);
                let report = compile_project(&input.project, &request);
                fs::write(out.join("report.json"), report.to_json()).unwrap();
                assert_builder(&report);
                assert_eq!(report.native_codegen.as_ref().unwrap().actual_units, 1);
                let probe = out.join("probe");
                let linked = Command::new(&clang)
                    .arg(out.join("api.o"))
                    .arg(root().join("tools/language/173/allocator_probe.c"))
                    .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
                    .arg(&probe)
                    .output()
                    .unwrap();
                fs::write(out.join("link.stdout"), &linked.stdout).unwrap();
                fs::write(out.join("link.stderr"), &linked.stderr).unwrap();
                assert!(linked.status.success(), "{linked:?}");
                for expected in rows(name, true) {
                    // Every input is empty bytes; its hexadecimal label is
                    // empty. No raw stdin byte appears in a path.
                    execute(
                        &probe,
                        &out.join(format!("input--fail-{}", expected.fail_at.unwrap())),
                        &expected,
                        true,
                    );
                }
            }
        }
    }
}
