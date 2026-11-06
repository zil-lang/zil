//! Independent Source consumers for the G2.4.1 lending trait profile.
use super::{project, NEXT};
use std::{fs, path::PathBuf, process::Command, sync::atomic::Ordering};
use zil_base::{Phase, ResultCategory};
use zil_driver::{compile_project, CompileOptions, NativeBackend, Optimization, Stage};

const MAIN: &str = include_str!("project/main.zil");
const API: &str = include_str!("project/api.zil");
const CURSOR: &str = include_str!("project/cursor.zil");
const CONSUMER: &str = include_str!("project/consumer.zil");
const OTHER: &str = include_str!("project/other.zil");

fn files<'a>(
    main: &'a str,
    api: &'a str,
    cursor: &'a str,
    consumer: &'a str,
    other: &'a str,
) -> [(&'a str, &'a str); 5] {
    [
        ("main", main),
        ("api", api),
        ("cursor", cursor),
        ("consumer", consumer),
        ("other", other),
    ]
}

fn source_report(
    main: &str,
    api: &str,
    cursor: &str,
    consumer: &str,
    other: &str,
) -> zil_driver::CompileReport {
    compile_project(
        &project(&files(main, api, cursor, consumer, other)),
        &CompileOptions::default(),
    )
}

fn canonical_success() {
    let report = source_report(MAIN, API, CURSOR, CONSUMER, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs >= 1, "{}", report.to_json());
}

fn safety_reject(
    main: &str,
    api: &str,
    cursor: &str,
    consumer: &str,
    other: &str,
    code_prefix: &str,
    phases: &[Phase],
) {
    // A rejected mutation is meaningful only after the unmodified Source succeeds.
    canonical_success();
    let report = source_report(main, api, cursor, consumer, other);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        phases.contains(&report.phase),
        "unexpected rejection phase: {}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(code_prefix)),
        "expected {code_prefix}: {}",
        report.to_json()
    );
}

#[test]
fn g241_cross_module_source_and_all_native_modes() {
    canonical_success();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let dir = root.join(format!(
                "g241-source-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let options = CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                entry: Some("main".into()),
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            };
            let report = compile_project(
                &project(&files(MAIN, API, CURSOR, CONSUMER, OTHER)),
                &options,
            );
            fs::write(dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1, "{}", report.to_json());
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none(), "{}", report.to_json());
            if backend == NativeBackend::LlvmApi {
                assert!(!codegen.object_reports.is_empty());
                assert!(codegen
                    .object_reports
                    .iter()
                    .all(|x| x.contains("\"input_kind\":\"builder\"")));
            }
            let output = Command::new(&executable).output().unwrap();
            fs::write(dir.join("stdout"), &output.stdout).unwrap();
            fs::write(dir.join("stderr"), &output.stderr).unwrap();
            // main checks 20 + 22, both exhausted states, and the nominal Token provider.
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, b"42\n", "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn g241_live_result_blocks_overlapping_next_and_parent_overwrite() {
    let live_next = "import api;import cursor;fn main()->i32{var c=cursor.make(20,22);let first=c.next();let second=c.next();match first{Option.Some(v)=>{return *v},Option.None()=>{return 0}}}";
    safety_reject(
        live_next,
        API,
        CURSOR,
        CONSUMER,
        OTHER,
        "E-BORROW",
        &[Phase::Analyze, Phase::Verify],
    );
    let overwrite = "import api;import cursor;fn main()->i32{var c=cursor.make(20,22);let first=c.next();c=cursor.make(1,2);match first{Option.Some(v)=>{return *v},Option.None()=>{return 0}}}";
    safety_reject(
        overwrite,
        API,
        CURSOR,
        CONSUMER,
        OTHER,
        "E-BORROW",
        &[Phase::Analyze, Phase::Verify],
    );
}

#[test]
fn g241_last_use_and_distinct_roots_are_legal() {
    canonical_success();
    let main = "import api;import cursor;fn main()->i32{var left=cursor.make(20,22);var right=cursor.make(7,11);let first=left.next();let other=right.next();var total=0;match other{Option.Some(v)=>{total+=*v},Option.None()=>{return 1}}match first{Option.Some(v)=>{total+=*v},Option.None()=>{return 2}}let second=left.next();match second{Option.Some(v)=>{total+=*v},Option.None()=>{return 3}}return total}";
    let report = source_report(main, API, CURSOR, CONSUMER, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn g241_same_spelling_binders_in_distinct_providers() {
    canonical_success();
    let other = OTHER.replace("life q", "life p").replace("'q", "'p");
    assert_ne!(other, OTHER);
    let report = source_report(MAIN, API, CURSOR, CONSUMER, &other);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn g241_impl_return_must_use_receiver_root_even_when_unused() {
    let local = CURSOR.replace(
        "return Option::<&'p T>.Some(&self.values[i]);",
        "let local=self.values[i];return Option::<&'p T>.Some(&local);",
    );
    assert_ne!(local, CURSOR);
    safety_reject(
        MAIN,
        API,
        &local,
        CONSUMER,
        OTHER,
        "E-",
        &[Phase::Expand, Phase::Analyze, Phase::Verify],
    );
    let bad = "import api;struct Pair{left:i32;right:i32}impl api.LendingIterator for Pair{type Item=i32;fn next<life p>(self:&'p mut Self)->Option<&'p i32>{let local=42;return Option::<&'p i32>.Some(&local)}}fn main(){}";
    let report = source_report(bad, API, CURSOR, CONSUMER, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Expand | Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
}

#[test]
fn g241_unused_method_rejects_wrong_input_root() {
    canonical_success();
    let bad = "trait Wrong{type Item:Copy;fn pick<life a,life b>(self:&'a mut Self,other:&'b i32)->Option<&'a Self.Item>;}struct Pair{n:i32}impl Wrong for Pair{type Item=i32;fn pick<life x,life y>(self:&'x mut Self,other:&'y i32)->Option<&'x i32>{return Option::<&'x i32>.Some(other)}}fn main(){}";
    let report = super::check(bad);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify | Phase::Expand),
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("E-LIFE") || d.code.starts_with("E-BORROW")),
        "{}",
        report.to_json()
    );
}

#[test]
fn g241_live_result_blocks_resource_parent_move() {
    canonical_success();
    let provider = "trait Lend{type Item:Copy;fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;}resource Cursor{values:[i32;2];index:usize}impl Lend for Cursor{type Item=i32;fn next<life p>(self:&'p mut Self)->Option<&'p i32>{if self.index>=2usize{return Option::<&'p i32>.None()}let i=self.index;self.index+=1usize;return Option::<&'p i32>.Some(&self.values[i])}}fn consume(give c:Cursor)->i32{return c.values[0]}";
    let legal = format!("{provider}fn main()->i32{{var c=Cursor{{values:[20,22],index:0usize}};let held=c.next();var seen=0;match held{{Option.Some(v)=>{{seen=*v}},Option.None()=>{{return 0}}}}return seen+consume(give c)}}");
    let good = super::check(&legal);
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let invalid = format!("{provider}fn main()->i32{{var c=Cursor{{values:[20,22],index:0usize}};let held=c.next();let retired=consume(give c);match held{{Option.Some(v)=>{{return *v+retired}},Option.None()=>{{return 0}}}}}}");
    let report = super::check(&invalid);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("E-BORROW") || d.code.starts_with("E-ADDRESS")),
        "{}",
        report.to_json()
    );
}

#[test]
fn g241_copy_item_does_not_grant_arithmetic() {
    let consumer = format!("{CONSUMER}\npub fn invalid_arithmetic<I:api.LendingIterator>(x:I.Item)->I.Item{{return x+1}}");
    safety_reject(
        MAIN,
        API,
        CURSOR,
        &consumer,
        OTHER,
        "E-",
        &[Phase::Expand, Phase::Types],
    );
}
