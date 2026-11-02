//! Ordinary multi-module ZIL -> verified compiler -> controlled C event loop.
#![cfg(target_os = "linux")]
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::{fs::PermissionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn fixtures() -> PathBuf {
    root().join("tests/foreign-scoped-callback")
}
fn preparation_project(group: &str, name: &str) -> ProjectSources {
    fixture_project("local-preparation", group, name)
}
fn fixture_project(directory: &str, group: &str, name: &str) -> ProjectSources {
    let path = fixtures()
        .join(directory)
        .join(group)
        .join(format!("{name}.zil"));
    ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            path.to_string_lossy().into_owned(),
            fs::read_to_string(path).unwrap(),
        )],
        paths: BTreeMap::new(),
    }
}
struct Temp(PathBuf, bool);
impl Temp {
    fn new(label: &str) -> Self {
        let configured = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let parent = configured
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = parent.join(format!(
            "foreign-scoped-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, configured.is_some())
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn project(entry: &str) -> ProjectSources {
    let names = match entry {
        "disjoint_body_main" => vec![entry, "bridge"],
        "local_body_main" => vec![entry, "local_body_bridge"],
        _ => vec![entry, "model", "bridge"],
    };
    ProjectSources {
        root: ModuleId(0),
        modules: names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let path = fixtures().join(format!("project/{name}.zil"));
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy().into_owned(),
                    fs::read_to_string(path).unwrap(),
                )
            })
            .collect(),
        paths: names
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, name)| (vec![(*name).into()], ModuleId(i as u32)))
            .collect(),
    }
}
fn backend() -> NativeBackend {
    if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    }
}
struct Expected {
    input: u8,
    fail_at: u64,
    exit: Option<i32>,
    signal: Option<i32>,
    stdout: String,
    counters: BTreeMap<String, u64>,
}
fn rows() -> Vec<Expected> {
    oracle_rows("ORACLE.json")
}
fn oracle_rows(name: &str) -> Vec<Expected> {
    // Read the frozen oracle, not the generator or observed compiler output.
    let python = r#"
import json,sys
for case in json.load(open(sys.argv[1]))['cases']:
 for row in [case['normal'],*case['allocator']]:
  fields=[row['input_hex'],str(row['fail_at']),str(row['exit']),str(row['signal']),row['stdout']]
  fields += [str(row.get(k,'-')) for k in ['alloc_calls','allocated','frees','failures','live','peak_live']]
  print('\t'.join(fields))
"#;
    let output = Command::new("python3")
        .args(["-c", python])
        .arg(fixtures().join(name))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), 11);
            Expected {
                input: u8::from_str_radix(fields[0], 16).unwrap(),
                fail_at: fields[1].parse().unwrap(),
                exit: (fields[2] != "None").then(|| fields[2].parse().unwrap()),
                signal: (fields[3] != "None").then(|| fields[3].parse().unwrap()),
                stdout: fields[4].into(),
                counters: COUNTERS
                    .iter()
                    .zip(&fields[5..])
                    .filter(|(_, value)| **value != "-")
                    .map(|(name, value)| ((*name).into(), value.parse().unwrap()))
                    .collect(),
            }
        })
        .collect()
}
fn checked(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(output.status.success(), "{command:?}: {output:?}");
}
fn compile(temporary: &Temp, optimization: Optimization, entry: &str) -> PathBuf {
    compile_native(
        temporary,
        optimization,
        &project(entry),
        &fixtures().join("driver.c"),
        &root().join("tools/language/173/allocator_probe.c"),
    )
}
fn compile_native(
    temporary: &Temp,
    optimization: Optimization,
    sources: &ProjectSources,
    driver_source: &Path,
    probe_source: &Path,
) -> PathBuf {
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let opt = if optimization == Optimization::O0 {
        "-O0"
    } else {
        "-O2"
    };
    let driver = temporary.0.join("driver.o");
    let probe = temporary.0.join("allocator.o");
    for (source, output) in [(driver_source, &driver), (probe_source, &probe)] {
        checked(
            Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", opt, "-c"])
                .arg(source)
                .arg("-o")
                .arg(output),
        );
    }
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    // Add only explicit foreign test objects at the production native link.
    // -x none ends the reference backend's -x ir before binary object inputs.
    let wrapper = temporary.0.join("link-foreign.sh");
    fs::write(&wrapper, format!(
        "#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} {} -Wl,--wrap=realloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",
        quote(&clang), quote(&driver), quote(&probe), quote(&clang)
    )).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let report = compile_project(
        sources,
        &CompileOptions {
            backend: backend(),
            optimization,
            clang: Some(wrapper),
            stop_after: Stage::Build,
            output: Some(temporary.0.join("program")),
            ..CompileOptions::default()
        },
    );
    if temporary.1 {
        fs::write(temporary.0.join("compile.json"), report.to_json()).unwrap();
    }
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let codegen = report.native_codegen.as_ref().unwrap();
    assert_eq!(codegen.backend, backend());
    assert!(codegen.fallback.is_none());
    if cfg!(feature = "llvm-api") {
        assert!(!codegen.object_reports.is_empty());
        assert!(
            codegen
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"verify_before_after\":true")
                    && r.contains("\"parse_ns\":0,"))
        );
    }
    report.artifacts["executable"].clone()
}
fn observe(program: &Path, temporary: &Temp, expected: &Expected) {
    let mut command = Command::new(program);
    command
        .env("ZIL_TEST_FAIL_AT", expected.fail_at.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let write = child.stdin.take().unwrap().write_all(&[expected.input]);
    if let Err(error) = write {
        // Nearby controls need no input and can exit before this write. Their
        // exact exit status, events and counters are still checked below.
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("foreign callback event loop timed out");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let output = child.wait_with_output().unwrap();
    if temporary.1 {
        let path = temporary.0.join(format!(
            "input-{:02x}-fail-{}",
            expected.input, expected.fail_at
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("stdout"), &output.stdout).unwrap();
        fs::write(path.join("stderr"), &output.stderr).unwrap();
        fs::write(path.join("status"), output.status.to_string()).unwrap();
    }
    assert_eq!(
        output.status.code(),
        expected.exit,
        "input {:02x} fail {}: {output:?}",
        expected.input,
        expected.fail_at
    );
    assert_eq!(output.status.signal(), expected.signal, "{output:?}");
    assert_eq!(
        output.stdout,
        expected.stdout.as_bytes(),
        "input {:02x} fail {}",
        expected.input,
        expected.fail_at
    );
    if expected.signal.is_some() {
        assert!(output.stderr.is_empty(), "{output:?}");
        return;
    }
    let text = std::str::from_utf8(&output.stderr).unwrap().trim();
    let inner = text
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .expect("allocator JSON counters");
    let mut counters = BTreeMap::new();
    for field in inner.split(',') {
        let (name, value) = field.split_once(':').unwrap();
        let name = name
            .trim()
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap();
        assert!(COUNTERS.contains(&name) || name == "peak_bytes");
        assert!(
            counters
                .insert(name.to_string(), value.trim().parse::<u64>().unwrap())
                .is_none()
        );
    }
    for name in COUNTERS {
        assert_eq!(
            counters[name], expected.counters[name],
            "{name}: input {:02x} fail {}",
            expected.input, expected.fail_at
        );
    }
}

#[test]
fn scoped_event_loop_source_reaches_independent_verification() {
    let report = compile_project(&project("main"), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn scoped_event_loop_matches_source_oracle_at_o0_and_o2() {
    let expected = rows();
    assert_eq!(expected.len(), 33);
    for optimization in [Optimization::O0, Optimization::O2] {
        let temporary = Temp::new(if optimization == Optimization::O0 {
            "o0"
        } else {
            "o2"
        });
        let binary = compile(&temporary, optimization, "main");
        for row in &expected {
            observe(&binary, &temporary, row);
        }
    }
}

#[test]
fn scoped_callback_source_rejections_bind_the_actual_risk() {
    // Bind the source phase and code as well as rejection category. These
    // expectations describe the risk, never an early parser/Unsupported result.
    let cases = [
        (
            "fn_once_callback",
            Phase::Types,
            "E-FFI-SCOPED-CALLBACK",
            "FnOnce",
        ),
        (
            "borrowed_callback_environment",
            Phase::Types,
            "E-FFI-SCOPED-CALLBACK",
            "self-contained",
        ),
        (
            "body_captures_same_callback",
            Phase::Analyze,
            "E-BORROW-CONFLICT",
            "",
        ),
        (
            "body_moves_same_callback",
            Phase::Analyze,
            "E-UNINITIALIZED",
            "",
        ),
        (
            "caller_keeps_live_callback_alias",
            Phase::Analyze,
            "E-BORROW-CONFLICT",
            "",
        ),
        (
            "missing_unsafe_region",
            Phase::Types,
            "E-FFI-UNSAFE",
            "unsafe",
        ),
        ("swapped_roles", Phase::Types, "E-FFI-SCOPED-ROLE", "role"),
        (
            "missing_register_role",
            Phase::Types,
            "E-FFI-SCOPED-ROLE",
            "role",
        ),
        (
            "wrong_register_signature",
            Phase::Types,
            "E-FFI-DECLARATION",
            "signature",
        ),
        (
            "ordinary_role_call",
            Phase::Types,
            "E-FFI-SCOPED-ROLE",
            "only",
        ),
        (
            "wrong_body_result",
            Phase::Types,
            "E-TYPE-MISMATCH",
            "closure type constraints",
        ),
        (
            "wrong_callback_signature",
            Phase::Types,
            "E-TYPE-MISMATCH",
            "closure type constraints",
        ),
        (
            "body_used_after_consumption",
            Phase::Analyze,
            "E-UNINITIALIZED",
            "",
        ),
    ];
    let mut failures = Vec::new();
    for (name, phase, code, reason) in cases {
        let path = fixtures().join(format!("negative/{name}.zil"));
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            path.to_string_lossy().into_owned(),
            fs::read_to_string(path).unwrap(),
        );
        let report = compile_source(&source, &CompileOptions::default());
        let exact = report.status == ResultCategory::SafeReject
            && report.verification_runs == 0
            && report.diagnostics.iter().any(|d| {
                d.phase == phase
                    && d.code == code
                    && d.category == ResultCategory::SafeReject
                    && d.message.contains(reason)
                    && d.span.is_none_or(|span| span.file == FileId(0))
            });
        if !exact {
            failures.push(format!(
                "{name}: expected {phase:?}/{code}/{reason:?}; {}",
                report.to_json()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn scoped_body_can_borrow_a_disjoint_scalar_through_an_event_loop() {
    let report = compile_project(&project("disjoint_body_main"), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    // This nearby program has exactly mode 0's external events and no allocation.
    // It additionally mutates a disjoint caller scalar twice and checks it after
    // body consumption. The original matrix and frozen oracle stay unchanged.
    let expected = rows()
        .into_iter()
        .find(|r| r.input == b'0' && r.fail_at == 0)
        .unwrap();
    for optimization in [Optimization::O0, Optimization::O2] {
        let temporary = Temp::new(if optimization == Optimization::O0 {
            "disjoint-o0"
        } else {
            "disjoint-o2"
        });
        let program = compile(&temporary, optimization, "disjoint_body_main");
        observe(&program, &temporary, &expected);
    }
}

#[test]
fn scoped_f_only_wrapper_creates_its_own_concrete_body() {
    let report = compile_project(&project("local_body_main"), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let expected = oracle_rows("LOCAL_BODY_ORACLE.json");
    assert_eq!(expected.len(), 1);
    for optimization in [Optimization::O0, Optimization::O2] {
        let temporary = Temp::new(if optimization == Optimization::O0 {
            "local-body-o0"
        } else {
            "local-body-o2"
        });
        let program = compile(&temporary, optimization, "local_body_main");
        observe(&program, &temporary, &expected[0]);
    }
}

#[test]
fn scoped_local_preparation_frozen_sources_reach_independent_verification() {
    let mut failures = Vec::new();
    for (group, name) in [
        ("frozen", "generic-local"),
        ("frozen", "concrete-local"),
        ("frozen", "generic-owned-parameter"),
        ("observed", "generic-local"),
    ] {
        let report = compile_project(
            &preparation_project(group, name),
            &CompileOptions::default(),
        );
        if report.status != ResultCategory::Success || report.verification_runs != 1 {
            failures.push(format!("{group}/{name}: {}", report.to_json()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn preparation_native_group(group: &str, names: &[&str], oracle: &str) {
    native_fixture_group(
        "local-preparation",
        group,
        names,
        oracle,
        "local_preparation_probe.c",
    );
}
fn native_fixture_group(directory: &str, group: &str, names: &[&str], oracle: &str, probe: &str) {
    let expected = oracle_rows(&format!("{directory}/{oracle}"));
    assert_eq!(expected.len(), 6);
    for name in names {
        for optimization in [Optimization::O0, Optimization::O2] {
            let temporary = Temp::new(&format!("{directory}-{group}-{name}-{optimization:?}"));
            let program = compile_native(
                &temporary,
                optimization,
                &fixture_project(directory, group, name),
                &fixtures().join(directory).join("driver.c"),
                &root()
                    .join("tools/language/foreign-scoped-callback")
                    .join(probe),
            );
            for row in &expected {
                observe(&program, &temporary, row);
            }
        }
    }
}

#[test]
fn scoped_local_preparation_frozen_controls_match_native_oracle() {
    preparation_native_group(
        "frozen",
        &["concrete-local", "generic-owned-parameter"],
        "FROZEN_ORACLE.json",
    );
}

#[test]
fn scoped_local_preparation_generic_source_matches_native_oracle() {
    preparation_native_group("frozen", &["generic-local"], "FROZEN_ORACLE.json");
}

#[test]
fn scoped_local_preparation_preserves_cleanup_and_caller_callback() {
    preparation_native_group("observed", &["generic-local"], "OBSERVED_ORACLE.json");
}

const COMPOSITION_CONTROLS: [&str; 4] = [
    "body-dispatch-local-concrete",
    "body-dispatch-capture-concrete",
    "body-factory-simple-concrete",
    "nested-helper-propagate",
];
const COMPOSITION_HANDLERS: [&str; 2] = ["body-dispatch-local", "body-dispatch-capture"];
const COMPOSITION_FACTORIES: [&str; 2] = ["body-factory-simple", "body-dispatch-factory"];

#[test]
fn scoped_concrete_composition_sources_reach_independent_verification() {
    let mut failures = Vec::new();
    for (group, names) in [
        ("frozen", COMPOSITION_CONTROLS.as_slice()),
        ("frozen", COMPOSITION_HANDLERS.as_slice()),
        ("frozen", COMPOSITION_FACTORIES.as_slice()),
        ("observed", COMPOSITION_HANDLERS.as_slice()),
        ("observed", COMPOSITION_FACTORIES.as_slice()),
        ("frozen", ["handler-without-with"].as_slice()),
    ] {
        for name in names {
            let report = compile_project(
                &fixture_project("callable-composition", group, name),
                &CompileOptions::default(),
            );
            if report.status != ResultCategory::Success || report.verification_runs != 1 {
                failures.push(format!("{group}/{name}: {}", report.to_json()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn composition_native_group(group: &str, names: &[&str], oracle: &str) {
    native_fixture_group(
        "callable-composition",
        group,
        names,
        oracle,
        "composition_probe.c",
    );
}

#[test]
fn scoped_concrete_composition_existing_controls_match_native_oracle() {
    composition_native_group("frozen", &COMPOSITION_CONTROLS, "FROZEN_ORACLE.json");
}

#[test]
fn scoped_concrete_composition_local_handler_matches_native_oracle() {
    composition_native_group("frozen", &["body-dispatch-local"], "FROZEN_ORACLE.json");
}

#[test]
fn scoped_concrete_composition_captured_handler_matches_native_oracle() {
    composition_native_group("frozen", &["body-dispatch-capture"], "FROZEN_ORACLE.json");
}

#[test]
fn scoped_concrete_composition_factory_bodies_match_native_oracle() {
    composition_native_group("frozen", &COMPOSITION_FACTORIES, "FROZEN_ORACLE.json");
}

#[test]
fn scoped_concrete_composition_local_handler_preserves_remaining_cleanup_and_callback_access() {
    composition_native_group("observed", &["body-dispatch-local"], "OBSERVED_ORACLE.json");
}

#[test]
fn scoped_concrete_composition_captured_handler_preserves_remaining_cleanup_and_callback_access() {
    composition_native_group(
        "observed",
        &["body-dispatch-capture"],
        "OBSERVED_ORACLE.json",
    );
}

#[test]
fn scoped_concrete_composition_factory_bodies_preserve_remaining_cleanup_and_callback_access() {
    composition_native_group("observed", &COMPOSITION_FACTORIES, "OBSERVED_ORACLE.json");
}

#[test]
fn concrete_factory_only_helper_reaches_independent_verification() {
    let report = compile_project(
        &fixture_project("callable-composition", "frozen", "factory-only"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn concrete_factory_only_helper_runs_without_foreign_callbacks() {
    // This Copy-only source has no FFI, allocation or input. Keep its native
    // check independent of the C registration/allocator harness.
    for optimization in [Optimization::O0, Optimization::O2] {
        let temporary = Temp::new(&format!("factory-only-{optimization:?}"));
        let program = temporary.0.join("program");
        let report = compile_project(
            &fixture_project("callable-composition", "frozen", "factory-only"),
            &CompileOptions {
                backend: backend(),
                optimization,
                stop_after: Stage::Build,
                output: Some(program.clone()),
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
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend());
        assert!(codegen.fallback.is_none());
        if cfg!(feature = "llvm-api") {
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| {
                r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"parse_ns\":0,")
                    && r.contains("\"verify_before_after\":true")
            }));
        }
        let output = Command::new(program).output().unwrap();
        // make()(2)=5, caller F(1)=2, run returns 7 and main returns 0.
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}
