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
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
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
fn project() -> ProjectSources {
    let names = ["main", "model", "bridge"];
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
        .arg(fixtures().join("ORACLE.json"))
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
fn compile(temporary: &Temp, optimization: Optimization) -> PathBuf {
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
    for (source, output) in [
        (fixtures().join("driver.c"), &driver),
        (root().join("tools/language/173/allocator_probe.c"), &probe),
    ] {
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
        &project(),
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
                    && r.contains("\"verify_before_after\":true"))
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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&[expected.input])
        .unwrap();
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
    let report = compile_project(&project(), &CompileOptions::default());
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
        let binary = compile(&temporary, optimization);
        for row in &expected {
            observe(&binary, &temporary, row);
        }
    }
}
