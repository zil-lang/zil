//! Independently authored source attacks on owning argument/result authority.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};
const DATA: &str = "resource R{n:i32}package Event{manages r:R;n:i32}";

fn rejects(label: &str, source: String) {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review247-{label}-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("source.zil"), &source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "review247.zil", source),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{label}: {}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.phase == Phase::Analyze),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn argument_and_once_environment_each_transfer_only_once() {
    for (label, tail) in [
        (
            "argument-twice",
            "fn main(){let value=Event{r:R{n:1},n:2};let f=(event:Event)->Event=>{return event};let a=f(value);let b=f(value)}",
        ),
        (
            "field-after-call",
            "fn main()->i32{let value=Event{r:R{n:1},n:2};let f=(event:Event)->Event=>{return event};let a=f(value);return value.n}",
        ),
        (
            "once-twice",
            "fn main(){let active=R{n:1};let f=move(event:Event)->Event=>{let held=give active;return event};let a=f(Event{r:R{n:2},n:3});let b=f(Event{r:R{n:4},n:5})}",
        ),
    ] {
        rejects(label, format!("{DATA}{tail}"));
    }
}

#[test]
fn callback_and_public_dispatch_never_reexport_private_created_leaf_authority() {
    for (label, helper, expression) in [
        (
            "concrete-identity",
            "",
            "let f=(event:Event)->Event=>{return event};let boxed=f(Event{r:R{n:7},n:1});",
        ),
        (
            "concrete-factory",
            "",
            "let f=()->Event=>{return Event{r:R{n:7},n:1}};let boxed=f();",
        ),
        (
            "public-identity",
            "fn dispatch<F:FnOnce(Event)->Event>(f:F,event:Event)->Event{return f(event)}",
            "let f=(event:Event)->Event=>{return event};let boxed=dispatch(f,Event{r:R{n:7},n:1});",
        ),
    ] {
        rejects(
            label,
            format!(
                "{DATA}{helper}fn extract()->R contract{{result(create)}}{{{expression}return give boxed.r}}fn main(){{}}"
            ),
        );
    }
}

#[test]
fn active_captured_view_cannot_survive_owning_argument_transfer() {
    rejects(
        "loan-in-callee",
        format!(
            "{DATA}fn main()->i32{{let event=Event{{r:R{{n:7}},n:2}};let view=event.r;let f=(value:Event)->Event=>{{let n=view.n;return value}};let result=f(event);return result.n-2}}"
        ),
    );
}
