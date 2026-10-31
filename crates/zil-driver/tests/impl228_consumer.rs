//! Independent fixed-layout event consumers and their handwritten equivalents.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit, json_string};
use zil_driver::{
    CompileOptions, CompileReport, NativeBackend, Optimization, ProjectSources, Stage,
    compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MAIN: &str = include_str!("../../../examples/mainline/generated_events228/plain/main.zil");
const PLAIN: &str = include_str!("../../../examples/mainline/generated_events228/plain/events.zil");
const GENERATED: &str =
    include_str!("../../../examples/mainline/generated_events228/generated/events.zil");
const WHERE: &str = include_str!("../../../examples/mainline/generated_events228/where/events.zil");
const COMBINED: &str =
    include_str!("../../../examples/mainline/generated_events228/combined/events.zil");

struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let report_dir = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = report_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "consumer228-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, report_dir.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn project(main: &str, events: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "events.zil", events),
        ],
        paths: BTreeMap::from([(vec!["events".into()], ModuleId(1))]),
    }
}

fn execute(executable: &Path, case: &Case, input: u8, expected: &[u8], exit: i32) {
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Every entry consumes exactly one byte before choosing a branch.
    child.stdin.take().unwrap().write_all(&[input]).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            fs::write(case.0.join(format!("{input}.stdout")), output.stdout).unwrap();
            fs::write(case.0.join(format!("{input}.stderr")), output.stderr).unwrap();
            fs::write(case.0.join(format!("{input}.timeout")), "5 seconds\n").unwrap();
            panic!("native event consumer timed out: {}", executable.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    fs::write(case.0.join(format!("{input}.stdout")), &output.stdout).unwrap();
    fs::write(case.0.join(format!("{input}.stderr")), &output.stderr).unwrap();
    fs::write(
        case.0.join(format!("{input}.run.json")),
        format!(
            "{{\"input_byte\":{input},\"exit_code\":{},\"expected_exit\":{exit},\"stdout\":{},\"expected_stdout\":{},\"stderr\":{}}}\n",
            output.status.code().map_or("null".into(), |n| n.to_string()),
            json_string(&String::from_utf8_lossy(&output.stdout)),
            json_string(&String::from_utf8_lossy(expected)),
            json_string(&String::from_utf8_lossy(&output.stderr)),
        ),
    )
    .unwrap();
    assert_eq!(output.status.code(), Some(exit), "{output:?}");
    assert_eq!(output.stdout, expected, "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

fn events(label: &str, source: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(&format!("{label}-{optimization:?}"));
        fs::write(case.0.join("main.zil"), MAIN).unwrap();
        fs::write(case.0.join("events.zil"), source).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let executable = case.0.join("events");
        let report = compile_project(
            &project(MAIN, source),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                emit_dir: Some(case.0.join("phases")),
                output: Some(executable.clone()),
                ..CompileOptions::default()
            },
        );
        fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}/{optimization:?}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let native = report.native_codegen.as_ref().unwrap();
        assert_eq!(native.backend, backend);
        assert!(native.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            assert!(!native.object_reports.is_empty());
            assert!(native.object_reports.iter().all(|r| {
                r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"verify_before_after\":true")
            }));
        }
        for (input, stdout, exit) in [
            (b'0', &b"J"[..], 0),
            (b'1', &b"W"[..], 0),
            (b'x', &b""[..], 7),
        ] {
            execute(&executable, &case, input, stdout, exit);
        }
    }
}

#[test]
fn plain_event_consumer() {
    events("plain", PLAIN);
}

#[test]
fn generated_event_consumer() {
    events("generated", GENERATED);
}

#[test]
fn where_event_consumer() {
    events("where", WHERE);
}

#[test]
fn combined_event_consumer() {
    events("combined", COMBINED);
}

#[test]
fn paired_sources_change_only_the_requested_declaration_forms() {
    let inline = "fn duplicate<T: Copy>(value: T) -> (T, T)";
    let trailing = "fn duplicate<T>(value: T) -> (T, T) where T: Copy";
    assert_eq!(WHERE.replace(trailing, inline), PLAIN);
    assert_eq!(COMBINED.replace(trailing, inline), GENERATED);
    for main in [
        include_str!("../../../examples/mainline/generated_events228/generated/main.zil"),
        include_str!("../../../examples/mainline/generated_events228/where/main.zil"),
        include_str!("../../../examples/mainline/generated_events228/combined/main.zil"),
    ] {
        assert_eq!(MAIN, main);
    }
}

fn rejection(label: &str, main: &str, events: &str) -> CompileReport {
    let case = Case::new(label);
    fs::write(case.0.join("main.zil"), main).unwrap();
    fs::write(case.0.join("events.zil"), events).unwrap();
    let report = compile_project(
        &project(main, events),
        &CompileOptions {
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
    report
}

#[test]
fn generated_event_shapes_keep_distinct_nominal_identities() {
    let main = "import events\nfn main()->i32{return events.accept(events.first())}";
    let plain = rejection(
        "nominal-plain",
        main,
        "struct Pointer{kind:i32}struct Touch{kind:i32}\n\
         pub fn first()->Pointer=>Pointer{kind:74}\n\
         pub fn accept(event:Touch)->i32=>event.kind",
    );
    let generated = rejection(
        "nominal-generated",
        main,
        "#for(mode in [0,1]){struct Event{kind:i32}\n\
         #if(mode==0){pub fn first()->Event=>Event{kind:74}}\n\
         else{pub fn accept(event:Event)->i32=>event.kind}}",
    );
    assert_eq!(plain.phase, generated.phase);
    assert_eq!(plain.diagnostics[0].code, generated.diagnostics[0].code);
}

#[test]
fn trailing_copy_bound_cannot_duplicate_an_owning_event_resource() {
    let main = "import events\nresource Marker{code:i32}\n\
                fn main()->i32{let owner=Marker{code:65};events.duplicate(owner);return 0}";
    let inline = rejection(
        "resource-inline",
        main,
        "pub fn duplicate<T:Copy>(value:T)->(T,T)=>(value,value)",
    );
    let trailing = rejection(
        "resource-where",
        main,
        "pub fn duplicate<T>(value:T)->(T,T) where T:Copy =>(value,value)",
    );
    for report in [&inline, &trailing] {
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("Copy")),
            "{}",
            report.to_json()
        );
    }
    assert_eq!(inline.diagnostics[0].code, trailing.diagnostics[0].code);
}
