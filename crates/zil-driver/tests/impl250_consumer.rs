//! Predeclared 250 consumer. This authors source tests, not production safety.
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
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../")
}
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = base.join(format!(
        "consumer250-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn backend() -> NativeBackend {
    if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    }
}
const COUNTERS: [&str; 6] = [
    "alloc_calls",
    "allocated",
    "frees",
    "failures",
    "live",
    "peak_live",
];
struct Expected {
    input: Vec<u8>,
    stdout: String,
    exit: i64,
    fail_at: Option<u64>,
    counters: BTreeMap<String, u64>,
    json: String,
}
// Python's standard JSON reader reads the already frozen oracle. It does not
// calculate expectations or inspect compiler output; the Rust harness executes
// and compares the actual consumer. No new crate dependency is needed.
fn rows(path: impl AsRef<Path>, selection: &str) -> (bool, Vec<Expected>) {
    let script = r#"
import json,sys
with open(sys.argv[1]) as f: data=json.load(f)
selection=sys.argv[2]
if selection.startswith('owned-error:'):
    data={'cases':data['owned_error']}
    selection=selection.split(':',1)[1]
negative=False
if selection.startswith('frozen:'):
    case=next(c for c in data['frozen'] if c['name']==selection[7:])
    negative=False
    rows=[(c['input'].encode().hex(),c) for c in case['native_if_source_verified']]
elif selection=='normal':
    rows=[(c['input'].encode().hex(),c['normal']) for c in data['cases']]
else:
    assert selection=='allocator'
    rows=[(c['input'].encode().hex(),e) for c in data['cases'] for e in c['allocator']]
print(int(negative))
for stdin,e in rows:
    keys=['alloc_calls','allocated','frees','failures','live','peak_live']
    fields=[stdin,str(e['exit']),e['stdout'],str(e.get('fail_at','-'))]
    fields += [str(e.get(k,'-')) for k in keys]
    fields += [json.dumps(e,sort_keys=True)]
    print('\t'.join(fields))
"#;
    let output = Command::new("python3")
        .args(["-c", script])
        .arg(path.as_ref())
        .arg(selection)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    let negative = lines.next().unwrap() == "1";
    let rows = lines
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), 11);
            let hex = fields[0];
            Expected {
                input: (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect(),
                exit: fields[1].parse().unwrap(),
                stdout: fields[2].into(),
                fail_at: (fields[3] != "-").then(|| fields[3].parse().unwrap()),
                counters: COUNTERS
                    .iter()
                    .zip(&fields[4..10])
                    .filter(|(_, value)| **value != "-")
                    .map(|(key, value)| ((*key).into(), value.parse().unwrap()))
                    .collect(),
                json: fields[10].into(),
            }
        })
        .collect();
    (negative, rows)
}
fn project(entry: &str) -> ProjectSources {
    let names = [entry, "model", "dispatch"];
    ProjectSources {
        root: ModuleId(0),
        modules: names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let path = root().join(format!("tests/impl250/project/{name}.zil"));
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy().into_owned(),
                    fs::read_to_string(&path).unwrap(),
                )
            })
            .collect(),
        paths: names
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, n)| (vec![(*n).into()], ModuleId(i as u32)))
            .collect(),
    }
}
fn assert_builder(report: &zil_driver::CompileReport) {
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
    let input = &expected.input;
    fs::create_dir_all(out).unwrap();
    fs::write(out.join("stdin"), input).unwrap();
    let mut command = Command::new(binary);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if allocator {
        command.env("ZIL_TEST_FAIL_AT", expected.fail_at.unwrap().to_string());
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{}: consumer timed out", out.display());
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
        result.status.code().map(i64::from),
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
        let actual = stderr[1..stderr.len() - 1]
            .split(',')
            .map(|item| {
                let (key, value) = item.split_once(':').unwrap();
                (
                    key.trim().trim_matches('"'),
                    value.trim().parse::<u64>().unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for field in COUNTERS {
            assert_eq!(
                actual[field],
                expected.counters[field],
                "{} {field}: {actual:?}",
                out.display()
            );
        }
    } else {
        assert!(result.stderr.is_empty(), "{}: {result:?}", out.display());
    }
}
fn frozen_case(name: &str) {
    let (negative, expected) = rows(
        root().join("tests/impl250/ORACLE.json"),
        &format!("frozen:{name}"),
    );
    let path = root().join(format!("tests/impl250/frozen-R/{name}/source.zil"));
    let text = fs::read_to_string(&path).unwrap();
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory(name);
        fs::write(out.join("source.zil"), &text).unwrap();
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                path.to_string_lossy().into_owned(),
                text.as_str(),
            ),
            &CompileOptions {
                stop_after: if negative {
                    Stage::Verify
                } else {
                    Stage::Build
                },
                backend: backend(),
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        if negative {
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
                    .any(|d| d.phase == Phase::Analyze && d.code == "E-UNINITIALIZED"),
                "{}",
                report.to_json()
            );
            assert!(!report.artifacts.contains_key("executable"));
        } else {
            assert_builder(&report);
            for (i, case) in expected.iter().enumerate() {
                execute(
                    &report.artifacts["executable"],
                    &out.join(format!("case-{i}")),
                    case,
                    false,
                );
            }
        }
    }
}
#[test]
fn original_r_event_reply_dispatch() {
    frozen_case("event-reply")
}
#[test]
fn original_r_owned_event_copy_result() {
    frozen_case("event-copy-result")
}
#[test]
fn original_r_owned_reply_from_once_capture() {
    frozen_case("owned-result")
}

#[test]
fn three_module_predeclared_native_matrix() {
    native_matrix("main", "normal");
}
#[test]
fn three_module_owned_error_payload_is_prepared_before_cleanup() {
    native_matrix("owned_error_main", "owned-error:normal");
}
fn native_matrix(entry: &str, selection: &str) {
    let p = project(entry);
    let (_, expected) = rows(root().join("tests/impl250/ORACLE.json"), selection);
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory("matrix");
        for module in &p.modules {
            fs::write(
                out.join(format!("module{}.zil", module.module_id.0)),
                &module.text,
            )
            .unwrap();
        }
        let report = compile_project(
            &p,
            &CompileOptions {
                backend: backend(),
                optimization,
                stop_after: Stage::Build,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_builder(&report);
        for (index, case) in expected.iter().enumerate() {
            execute(
                &report.artifacts["executable"],
                &out.join(format!("case-{index}")),
                case,
                false,
            );
        }
    }
}

#[cfg(all(unix, feature = "llvm-api"))]
#[test]
fn three_module_actual_allocator_failures_follow_the_frozen_source_oracle() {
    allocator_matrix("main", "allocator");
}
#[cfg(all(unix, feature = "llvm-api"))]
#[test]
fn three_module_owned_error_actual_allocator_failures() {
    allocator_matrix("owned_error_main", "owned-error:allocator");
}
#[cfg(all(unix, feature = "llvm-api"))]
fn allocator_matrix(entry: &str, selection: &str) {
    use std::os::unix::fs::PermissionsExt;
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let p = project(entry);
    let (_, expected) = rows(root().join("tests/impl250/ORACLE.json"), selection);
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory("allocator");
        for module in &p.modules {
            fs::write(
                out.join(format!("module{}.zil", module.module_id.0)),
                &module.text,
            )
            .unwrap();
        }
        let wrapper = out.join("capture-object.sh");
        fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\ncase \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&out.join("api.o")),quote(&clang))).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let report = compile_project(
            &p,
            &CompileOptions {
                backend: NativeBackend::LlvmApi,
                optimization,
                clang: Some(wrapper),
                stop_after: Stage::Build,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
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
        for case in &expected {
            let input = std::str::from_utf8(&case.input).unwrap();
            let fail = case.fail_at.unwrap();
            execute(
                &probe,
                &out.join(format!("input-{input}-fail-{fail}")),
                case,
                true,
            );
        }
    }
}
