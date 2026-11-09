//! Independent Source consumers for the trait shared lending lending trait profile.
use super::{NEXT, project};
use std::{fs, path::PathBuf, process::Command, sync::atomic::Ordering};
use zil_base::{Phase, ResultCategory};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_project};

const MAIN: &str = include_str!("project/main.zil");
const API: &str = include_str!("project/api.zil");
const CURSOR: &str = include_str!("project/cursor.zil");
const CONSUMER: &str = include_str!("project/consumer.zil");
const OTHER: &str = include_str!("project/other.zil");

#[path = "generic_composition.rs"]
mod generic_composition;

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
    (phase, code, message): (Phase, &str, &str),
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
        report.phase == phase,
        "unexpected rejection phase: {}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == code && d.message == message),
        "expected {code}: {message}: {}",
        report.to_json()
    );
}

#[test]
fn cross_module_source_and_all_native_modes() {
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
                "borrowed-interface-source-{}-{}",
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
                assert!(
                    codegen
                        .object_reports
                        .iter()
                        .all(|x| x.contains("\"input_kind\":\"builder\""))
                );
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
fn live_result_blocks_overlapping_next_and_parent_overwrite() {
    let live_next = "import api;import cursor;fn main()->i32{var c=cursor.make(20,22);let first=c.next();let second=c.next();match first{Option.Some(v)=>{return *v},Option.None()=>{return 0}}}";
    safety_reject(
        live_next,
        API,
        CURSOR,
        CONSUMER,
        OTHER,
        (
            Phase::Analyze,
            "E-ANALYZE-IR",
            "stored target proof: simultaneously live access overlaps an actual stored target/parent loan",
        ),
    );
    let overwrite = "import api;import cursor;fn main()->i32{var c=cursor.make(20,22);let first=c.next();c=cursor.make(1,2);match first{Option.Some(v)=>{return *v},Option.None()=>{return 0}}}";
    safety_reject(
        overwrite,
        API,
        CURSOR,
        CONSUMER,
        OTHER,
        (
            Phase::Analyze,
            "E-ANALYZE-IR",
            "stored target proof: simultaneously live access overlaps an actual stored target/parent loan",
        ),
    );
}

#[test]
fn last_use_and_distinct_roots_are_legal() {
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
fn same_spelling_binders_in_distinct_providers() {
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
fn impl_return_must_use_receiver_root_even_when_unused() {
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
        (
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
            "retained reference has no actual declared source class",
        ),
    );
    let bad = "import api;struct Pair{left:i32;right:i32}impl api.LendingIterator for Pair{type Item=i32;fn next<life p>(self:&'p mut Self)->Option<&'p i32>{let local=42;return Option::<&'p i32>.Some(&local)}}fn main(){}";
    let report = source_report(bad, API, CURSOR, CONSUMER, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-GENERIC-ARGUMENT"
                && d.message == "retained reference has no actual declared source class"),
        "{}",
        report.to_json()
    );
}

#[test]
fn unused_method_rejects_wrong_input_root() {
    canonical_success();
    let bad = "trait Wrong{type Item:Copy;fn pick<life a,life b>(self:&'a mut Self,other:&'b i32)->Option<&'a Self.Item>;}struct Pair{n:i32}impl Wrong for Pair{type Item=i32;fn pick<life x,life y>(self:&'x mut Self,other:&'y i32)->Option<&'x i32>{return Option::<&'x i32>.Some(other)}}fn main(){}";
    let report = super::check(bad);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-GENERIC-ARGUMENT"
                && d.message
                    == "source annotation cannot equate distinct declared lifetime classes"),
        "{}",
        report.to_json()
    );
}

#[test]
fn resource_head_lending_remains_explicitly_unsupported() {
    canonical_success();
    // A resource receiver reaches the existing head-address Types boundary.
    // This does not establish a parent-move safety result for trait shared lending.
    // A shared resource receiver with associated Item also lacks a supported
    // resource trait impl, so no resource transfer pair proves a move window here.
    let provider = "trait Lend{type Item:Copy;fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>;}resource Cursor{values:[i32;2];index:usize}impl Lend for Cursor{type Item=i32;fn next<life p>(self:&'p mut Self)->Option<&'p i32>{if self.index>=2usize{return Option::<&'p i32>.None()}let i=self.index;self.index+=1usize;return Option::<&'p i32>.Some(&self.values[i])}}fn consume(give c:Cursor)->i32{return c.values[0]}";
    let legal = format!(
        "{provider}fn main()->i32{{var c=Cursor{{values:[20,22],index:0usize}};let held=c.next();var seen=0;match held{{Option.Some(v)=>{{seen=*v}},Option.None()=>{{return 0}}}}return seen+consume(give c)}}"
    );
    let report = super::check(&legal);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-UNSUPPORTED" && d.message == "head addresses require a shared finite user-resource head or a supported Store element; byte-storage and other Package heads are not implemented"),
        "{}",
        report.to_json()
    );
}

#[test]
fn copy_item_arithmetic_requires_an_additional_declared_bound() {
    // This is an explicit generic capability boundary, not a loan-safety result.
    let consumer = format!(
        "{CONSUMER}\npub fn invalid_arithmetic<I:api.LendingIterator>(x:I.Item)->I.Item{{return x+1}}"
    );
    canonical_success();
    let report = source_report(MAIN, API, CURSOR, &consumer, OTHER);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"
            && d.message
                == "operation on an unconstrained type parameter requires a declared bound"),
        "{}",
        report.to_json()
    );
}

#[test]
fn unused_generic_option_patterns_preserve_the_receiver_loan() {
    canonical_success();
    let load = "fn load<T:Copy>(p:&T)->T{return *p}";
    let pairs = [
        (
            "if let",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{if let Option.Some(v)=cursor.next(){return load(v)}return fallback}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{if let Option.Some(v)=cursor.next(){cursor.next();return load(v)}return fallback}",
        ),
        (
            "while let",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{while let Option.Some(v)=cursor.next(){return load(v)}return fallback}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{while let Option.Some(v)=cursor.next(){cursor.next();return load(v)}return fallback}",
        ),
        (
            "held Option then match",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();match held{Option.Some(v)=>{return load(v)},Option.None()=>{return fallback}}}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();cursor.next();match held{Option.Some(v)=>{return load(v)},Option.None()=>{return fallback}}}",
        ),
    ];
    for (name, legal, invalid) in pairs {
        let good_source = format!("{CONSUMER}\n{load}\n{legal}");
        let good = source_report(MAIN, API, CURSOR, &good_source, OTHER);
        assert_eq!(
            good.status,
            ResultCategory::Success,
            "{name}: {}",
            good.to_json()
        );
        let bad_source = format!("{CONSUMER}\n{load}\n{invalid}");
        let bad = source_report(MAIN, API, CURSOR, &bad_source, OTHER);
        assert_eq!(
            bad.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            bad.to_json()
        );
        assert_eq!(bad.phase, Phase::Expand, "{name}: {}", bad.to_json());
        assert!(
            bad.diagnostics
                .iter()
                .any(|d| d.code == "E-GENERIC-ARGUMENT"
                    && d.message == "State alias outlives a moved or replaced field"),
            "{name}: {}",
            bad.to_json()
        );
    }
    for (name, body) in [
        (
            "if let wildcard",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{if let Option.Some(_)=cursor.next(){cursor.next();}return fallback}",
        ),
        (
            "match wildcard",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();match held{Option.Some(_)=>{cursor.next();},Option.None()=>{}}return fallback}",
        ),
    ] {
        let text = format!("{CONSUMER}\n{body}");
        let report = source_report(MAIN, API, CURSOR, &text, OTHER);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
    }
}

#[test]
fn nested_option_reference_patterns_remain_explicitly_unsupported() {
    canonical_success();
    let load = "fn load<T:Copy>(p:&T)->T{return *p}";
    let cases = [
        (
            "match direct",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{match cursor.next(){Option.Some(value)=>{return load(value)},Option.None()=>{return fallback}}}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{match cursor.next(){Option.Some(ref slot)=>{return load(*slot)},Option.None()=>{return fallback}}}",
        ),
        (
            "if let direct",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{if let Option.Some(value)=cursor.next(){return load(value)}return fallback}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{if let Option.Some(ref slot)=cursor.next(){return load(*slot)}return fallback}",
        ),
        (
            "while let direct",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{while let Option.Some(value)=cursor.next(){return load(value)}return fallback}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{while let Option.Some(ref slot)=cursor.next(){return load(*slot)}return fallback}",
        ),
        (
            "match held alias",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();match held{Option.Some(value)=>{return load(value)},Option.None()=>{return fallback}}}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();match held{Option.Some(ref slot)=>{return load(*slot)},Option.None()=>{return fallback}}}",
        ),
        (
            "if let held alias",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();if let Option.Some(value)=held{return load(value)}return fallback}",
            "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();if let Option.Some(ref slot)=held{return load(*slot)}return fallback}",
        ),
    ];
    for (name, ordinary, nested) in cases {
        let good = source_report(
            MAIN,
            API,
            CURSOR,
            &format!("{CONSUMER}\n{load}\n{ordinary}"),
            OTHER,
        );
        assert_eq!(
            good.status,
            ResultCategory::Success,
            "{name}: {}",
            good.to_json()
        );
        let report = source_report(
            MAIN,
            API,
            CURSOR,
            &format!("{CONSUMER}\n{load}\n{nested}"),
            OTHER,
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{name}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"
                && d.message
                    == "nested Option reference payload pattern requires its borrowing consumer"),
            "{name}: {}",
            report.to_json()
        );
    }
}

#[test]
fn while_let_held_alias_has_an_existing_generic_loop_boundary() {
    canonical_success();
    let load = "fn load<T:Copy>(p:&T)->T{return *p}";
    for body in [
        "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();while let Option.Some(value)=held{return load(value)}return fallback}",
        "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{let held=cursor.next();while let Option.Some(ref slot)=held{return load(*slot)}return fallback}",
    ] {
        let report = source_report(
            MAIN,
            API,
            CURSOR,
            &format!("{CONSUMER}\n{load}\n{body}"),
            OTHER,
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"
                && d.message
                    == "generic loop must preserve outer carrier and non-Copy consumption state"),
            "{}",
            report.to_json()
        );
    }
    // This loop boundary prevents the nested-ref case from proving its own guard.
}

#[test]
fn unused_generic_option_join_retains_the_possible_receiver_loan() {
    canonical_success();
    let load = "fn load<T:Copy>(p:&T)->T{return *p}";
    let legal = "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item,flag:bool)->I.Item{var held=Option::<&I.Item>.None();if flag{held=cursor.next();} match held{Option.Some(value)=>{return load(value)},Option.None()=>{return fallback}}}";
    let invalid = "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item,flag:bool)->I.Item{var held=Option::<&I.Item>.None();if flag{held=cursor.next();}cursor.next(); match held{Option.Some(value)=>{return load(value)},Option.None()=>{return fallback}}}";
    let good = source_report(
        MAIN,
        API,
        CURSOR,
        &format!("{CONSUMER}\n{load}\n{legal}"),
        OTHER,
    );
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let bad = source_report(
        MAIN,
        API,
        CURSOR,
        &format!("{CONSUMER}\n{load}\n{invalid}"),
        OTHER,
    );
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert_eq!(bad.phase, Phase::Expand, "{}", bad.to_json());
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.code == "E-GENERIC-ARGUMENT"
                && d.message == "State alias outlives a moved or replaced field"),
        "{}",
        bad.to_json()
    );
}

#[test]
fn generic_option_rewrap_reports_explicit_symbolic_source_boundary() {
    canonical_success();
    let load = "fn load<T:Copy>(p:&T)->T{return *p}";
    let legal = "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{match cursor.next(){Option.Some(value)=>{let wrapped=Option::<&I.Item>.Some(value); match wrapped{Option.Some(inner)=>{return load(inner)},Option.None()=>{return fallback}}},Option.None()=>{return fallback}}}";
    let invalid = "fn unused<I:api.LendingIterator>(cursor:&mut I,fallback:I.Item)->I.Item{match cursor.next(){Option.Some(value)=>{let wrapped=Option::<&I.Item>.Some(value);cursor.next(); match wrapped{Option.Some(inner)=>{return load(inner)},Option.None()=>{return fallback}}},Option.None()=>{return fallback}}}";
    // Even the version without a second next is outside the current symbolic
    // source profile. The invalid version's rejection is not safety evidence.
    for (name, body) in [("no second next", legal), ("second next", invalid)] {
        let report = source_report(
            MAIN,
            API,
            CURSOR,
            &format!("{CONSUMER}\n{load}\n{body}"),
            OTHER,
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{name}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"
                && d.message == "Option payload has no actual symbolic source"),
            "{name}: {}",
            report.to_json()
        );
    }
}
