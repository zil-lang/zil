//! Non-author source review of signature admission, environment modes and
//! separately closed profiles. This does not stand in for native execution.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{CallableKind, TypedModule};

const DATA: &str = "resource R{n:i32}package Event{manages r:R;n:i32}";
fn typed(source: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review246.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn output(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review246-{label}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn consuming_new_data_does_not_consume_fn_or_fnmut_environment() {
    for (label, body, kind) in [
        (
            "fn",
            "let f=(event:Event)->Event=>{return event}",
            CallableKind::Fn,
        ),
        (
            "fnmut",
            "var n=0;var f=move(event:Event)->Event=>{n+=1;return event}",
            CallableKind::FnMut,
        ),
        (
            "once",
            "let r=R{n:1};let f=move(event:Event)->Event=>{let held=give r;return event}",
            CallableKind::FnOnce,
        ),
    ] {
        let source = format!("{DATA}fn main(){{{body}}}");
        let out = output(label);
        fs::write(out.join("source.zil"), &source).unwrap();
        let module = typed(&source);
        fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
        assert_eq!(module.closures.len(), 1);
        assert_eq!(module.closures[0].kind, kind, "{label}");
        assert_eq!(module.closures[0].parameters.len(), 1);
        assert_eq!(module.closures[0].parameters[0], module.closures[0].result);
    }
}

#[test]
fn unused_public_single_once_definitions_check_the_complete_owned_signature() {
    let source = format!(
        "{DATA}fn dispatch<F:FnOnce(Event)->Event>(f:F,event:Event)->Event{{return f(event)}}fn main(){{}}"
    );
    typed(&source);
    typed(
        "resource F{n:i32}package Event{manages leaf:F}fn dispatch<F:FnOnce(Event)->Event>(f:F,event:Event)->Event{return f(event)}fn main(){let f=(event:Event)->Event=>{return event};let result=dispatch(f,Event{leaf:F{n:7}})}",
    );
}

#[test]
fn owning_data_does_not_open_address_borrowed_multi_f_or_capture_profiles() {
    let cases = [
        (
            "address-mixed",
            "fn main(){let f=(event:Event,p:&i32)->Event=>{return event}}",
        ),
        (
            "borrowed-public",
            "fn dispatch<F:Fn(Event)->Event>(f:&F,event:Event)->Event{return f(event)}fn main(){}",
        ),
        (
            "two-public-roles",
            "fn dispatch<F:FnOnce(Event)->Event,G:FnOnce(Event)->Event>(f:F,g:G,event:Event)->Event{return g(f(event))}fn main(){}",
        ),
        (
            "enum-capture",
            "enum Choice{Empty,Data(Event)}fn main(){let value=Choice.Empty;let f=move(event:Event)->Event=>{let held=value;return event}}",
        ),
        (
            "store-capture-owning-result",
            "fn main(){let values:Store<Event>=store_new();let f=move(event:Event)->Event=>{store_len(values);return event}}",
        ),
        (
            "shared-product-owning-result",
            "fn main(){let parent=Event{r:R{n:1},n:2};let view=&parent;let f=move(event:Event)->Event=>{let n=view.n;return event}}",
        ),
    ];
    for (label, tail) in cases {
        let source = format!("{DATA}{tail}");
        let out = output(label);
        fs::write(out.join("source.zil"), &source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "review246.zil", source),
            &CompileOptions {
                stop_after: Stage::Types,
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        let (phase, reason) = match label {
            "address-mixed" => (Phase::Types, "address arguments require a Copy result"),
            "borrowed-public" => (Phase::Types, "finite nonescaping callable roles"),
            "two-public-roles" => (Phase::Expand, "one callable type"),
            "enum-capture" => (Phase::Types, "capture requires a finite Copy value"),
            "store-capture-owning-result" => (
                Phase::Types,
                "Store capture environments require Copy results",
            ),
            "shared-product-owning-result" => (Phase::Types, "shared Product capture"),
            _ => unreachable!(),
        };
        assert!(
            report.diagnostics.iter().any(|d| d.phase == phase
                && d.code == "E-UNSUPPORTED"
                && d.message.contains(reason)),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
