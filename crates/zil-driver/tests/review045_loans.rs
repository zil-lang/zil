//! Independent review of root integration delta 64406b9 at exact 088a389.
use std::{
    collections::BTreeMap,
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, ProjectSources, Stage, compile_project, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review045-loans.zil", text)
}
fn native(build: impl FnOnce(&CompileOptions) -> CompileReport, want: i64) {
    let temp = std::env::temp_dir().join(format!(
        "zil-review045-loans-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&temp).unwrap();
    let report = build(&CompileOptions {
        stop_after: Stage::Build,
        entry: Some("answer".into()),
        output: Some(temp.join("o0")),
        emit_dir: Some(temp.join("ir")),
        ..CompileOptions::default()
    });
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    let ir = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(!ir.contains("noalias") && !ir.contains("@malloc("));
    let output = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(temp.join("o2"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for path in [temp.join("o0"), temp.join("o2")] {
        let output = Command::new(path).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, format!("{want}\n").as_bytes());
        assert!(output.stderr.is_empty());
    }
    fs::remove_dir_all(temp).unwrap();
}
fn one(text: &str, want: i64) {
    native(|options| compile_source(&source(text), options), want);
}
fn project(main: &str, model: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(10),
        modules: vec![
            SourceUnit::new(FileId(101), ModuleId(10), "main.zil", main),
            SourceUnit::new(FileId(205), ModuleId(30), "model.zil", model),
        ],
        paths: BTreeMap::from([(vec!["model".into()], ModuleId(30))]),
    }
}
fn reject(report: CompileReport, code: &str) {
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn mixed_windows_repeated_calls_and_restored_parents_execute() {
    one(
        "fn answer()->i32{var x=1;let y=2;var f=()=>{x=x+y;x};let a=f();let b=f();return a+b+x+y}",
        15,
    );
    one(
        "fn answer()->i32{var x=1;let y=2;let z=3;var f=()=>{x+=y+z;x};let a=f();let b=f();return a+b+x+y+z}",
        33,
    );
    one(
        "fn answer()->i32{var x=1;var y=2;let z=3;var f=()=>{x+=z;y+=x;x+y};let a=f();let b=f();return a+b+x+y+z}",
        53,
    );
    one(
        "fn answer()->i32{var x=1;var y=2;let z=3;let p=&mut x;let q=&mut y;var f=()=>{*p+=z;*q+=*p;*p+*q};let a=f();let b=f();*p+=1;*q+=1;return a+b+x+y+z}",
        55,
    );
}

#[test]
fn multiple_shared_descendants_and_unrelated_mutable_capture_execute() {
    one(
        "fn answer()->i32{var x=10;var y=0;let p=&mut x;let a=&*p;let b=&*p;var f=()=>{y+=*a+*b;y};let first=f();let second=f();return first+second+x+y}",
        110,
    );
    one(
        "fn answer()->i32{var x=21;var y=0;let p=&mut x;var f=()=>{y+=*p;y};let first=f();let second=f();*p=0;return first+second+y+x}",
        105,
    );
}

const CONFLICTS: &[&str] = &[
    // A relevant sibling cannot borrow the delegated status of an unrelated root.
    "fn answer()->i32{var x=1;let y=2;let p=&mut x;let sibling=&mut *p;var f=()=>{*p+=y;*p};let a=f();return a+*sibling}",
    // A live shared child prevents a parent's exclusive capture.
    "fn answer()->i32{var x=1;let y=2;let p=&mut x;let child=&*p;var f=()=>{*p+=y;*p};let a=f();return a+*child}",
    // Parent use between mixed-environment calls remains suspended.
    "fn answer()->i32{var x=1;var y=2;let z=3;let p=&mut x;let q=&mut y;var f=()=>{*p+=z;*q+=*p;*p+*q};let a=f();*q=40;return a+f()}",
    // The unrelated shared capture remains a real address dependency.
    "fn answer()->i32{var x=1;var y=2;var f=()=>{x+=y;x};let a=f();y=40;return a+f()}",
    // Direct reads do not obtain a vacuous parent delegation from an empty own set.
    "fn answer()->i32{var x=1;let y=2;var f=()=>{x+=y;x};let a=f();let n=x;return a+n+f()}",
    // Two relevant overlapping mutable captures cannot pass an any-parent test.
    "fn answer()->i32{var x=1;let z=2;let p=&mut x;let q=&mut *p;var f=()=>{*p+=z;*q+=z;*p+*q};return f()}",
    // Shared grandchild liveness must remain transitive through a mixed environment.
    "fn answer()->i32{var x=1;let y=2;let p=&mut x;let q=&*p;let r=&*q;var f=()=>{*p+=y;*p};let n=f();return n+*r}",
];

#[test]
fn mixed_root_source_conflicts_still_reject() {
    for text in CONFLICTS {
        reject(
            compile_source(&source(text), &CompileOptions::default()),
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn mixed_root_raw_conflicts_reject_with_matching_analyzer_candidates() {
    for text in CONFLICTS {
        let s = source(text);
        let ast = zil_syntax::parse(&s).into_result().unwrap();
        let core = zil_expand::expand(&s, &ast).unwrap();
        let limits = Limits::default();
        let module = zil_types::check(&core, &TargetProfile::default(), &limits).unwrap();
        let plan = zil_analyze::analyze(&module, &limits).unwrap();
        assert_eq!(plan.module_fingerprint, module.fingerprint());
        let errors =
            zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err();
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{text}\n{errors:?}");
    }
}

const MODEL: &str = "pub enum Value<T>{Empty,Item(T)} pub struct Row{pub value:i64;secret:i64;} pub fn make()->Row{return Row{value:5000000042,secret:7}} pub fn hidden(p:&Row)->Value<i64>{return Value.Item(p.secret)}";

#[test]
fn projected_generic_payloads_compose_across_modules_at_native_width() {
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item=model.Value.Item(p.value);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    native(|options| compile_project(&p, options), 5000000042);
    let p = project(
        "import model\nfn answer()->i64{var row:(i64,[i64;2])=(3,[7,5000000042]);let p=&mut row;let item=model.Value.Item(p.1[1]);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    native(|options| compile_project(&p, options), 5000000042);
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item=model.hidden(p);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    native(|options| compile_project(&p, options), 7);
}

#[test]
fn projected_generic_hints_grant_neither_private_access_nor_different_payload_type() {
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item=model.Value.Item(p.secret);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    reject(
        compile_project(&p, &CompileOptions::default()),
        "E-FIELD-PRIVATE",
    );
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item:model.Value<bool>=model.Value.Item(p.value);return 0}",
        MODEL,
    );
    reject(
        compile_project(&p, &CompileOptions::default()),
        "E-GENERIC-ARGUMENT",
    );
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let pp=&p;let item=model.Value.Item(pp.value);return 0}",
        MODEL,
    );
    let report = compile_project(&p, &CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "nested address hint was silently accepted: {}",
        report.to_json()
    );
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
