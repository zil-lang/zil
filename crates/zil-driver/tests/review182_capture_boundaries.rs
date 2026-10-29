//! Independent capture-source and capability boundary probes for language182.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir() -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "review182-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn check(source: &str, success: bool) {
    let p = dir();
    fs::write(p.join("input.zil"), source).unwrap();
    let s = SourceUnit::new(FileId(0), ModuleId(0), "review182.zil", source);
    let r = compile_source(&s, &CompileOptions::default());
    fs::write(p.join("report.json"), r.to_json()).unwrap();
    if success {
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
    } else {
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.phase == Phase::Verify && d.code == "E-BORROW-CONFLICT"),
            "{}",
            r.to_json()
        );
    }
}
fn native(source: &str, stdout: &[u8]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let p = dir();
        fs::write(p.join("input.zil"), source).unwrap();
        let s = SourceUnit::new(FileId(0), ModuleId(0), "review182.zil", source);
        let binary = p.join("program");
        let r = compile_source(
            &s,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(binary.clone()),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("report.json"), r.to_json()).unwrap();
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        let o = Command::new(&binary).output().unwrap();
        fs::write(
            p.join("native.json"),
            format!(
                "{{\"exit\":{:?},\"stdout\":{:?},\"stderr\":{:?},\"command\":{:?}}}",
                o.status.code().unwrap(),
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr),
                binary
            ),
        )
        .unwrap();
        assert_eq!(o.status.code(), Some(0));
        assert_eq!(o.stdout, stdout);
        assert!(o.stderr.is_empty());
    }
}
const MODEL: &str = r#"extern "C" fn putchar(n:i32)->i32;
resource R{code:i32;n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32=>f();
"#;
#[test]
fn mutable_local_capture_and_readonly_resource_borrow_do_not_share_authority() {
    let source = format!(
        "{MODEL}fn main()->i32{{let a=R{{code:65,n:20}};let d=a;let h=&a;var n=0;var f=()=>{{n+=1;return d.n+n}};let answer=invoke(&mut f);return answer+h.n+n-42}}"
    );
    native(&source, b"A");
    let bad = source.replace("n+=1;return d.n+n", "n+=1;d.n+=1;return d.n+n");
    check(&bad, false);
}
#[test]
fn headless_ordinary_forwarding_reconstructs_the_captured_alias() {
    let source = format!(
        "{MODEL}fn hidden(edit:R){{let f=()=>{{edit.n+=1}};f()}}fn second(edit:R){{hidden(edit)}}fn main()->i32{{let a=R{{code:65,n:20}};let b=R{{code:66,n:21}};let h=&a;second(b);return h.n+b.n-42}}"
    );
    native(&source, b"BA");
    check(&source.replace("second(b)", "second(a)"), false);
}
#[test]
fn unrelated_resource_head_in_an_unused_function_does_not_change_a_rejection() {
    let source = format!(
        "{MODEL}fn main()->i32{{let a=R{{code:65,n:20}};let d=a;let h=&a;let q=&h.n;let f=()=>{{d.n=42;return *q}};return f()}}"
    );
    check(&source, false);
    check(
        &(source + "fn unrelated()->i32{let r=R{code:90,n:1};let p=&r;return p.n}"),
        false,
    );
}
#[test]
fn moved_shared_capture_stays_readonly_across_two_helper_reborrows() {
    let source = format!(
        "{MODEL}fn read<F:Fn()->i32>(f:&F)->i32=>f();fn main()->i32{{let a=R{{code:65,n:21}};let d=a;let h=&a;let q=&h.n;let f=move()=>d.n+*q;let p=&f;let one=read(p);let two=read(p);return one+two-84}}"
    );
    native(&source, b"A");
    check(
        &source.replace("move()=>d.n+*q", "move()=>{d.n=42;return *q}"),
        false,
    );
}

#[test]
fn repaired_weak_capture_keeps_the_actual_field_instance_without_a_whole_package_hold() {
    let source = format!(
        "{MODEL}package Pair{{manages left:R;manages right:R}}fn main()->i32{{var p=Pair{{left:R{{code:65,n:21}},right:R{{code:66,n:20}}}};let d=p.left;let f=()=>d.n;p.right=R{{code:67,n:1}};return f()-21}}"
    );
    check(&source, true);
    let bad = source.replace("p.right=R{code:67,n:1}", "p.left=R{code:67,n:1}");
    let p = dir();
    fs::write(p.join("stale-field.zil"), &bad).unwrap();
    let r = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "stale-field.zil", bad.clone()),
        &CompileOptions::default(),
    );
    fs::write(p.join("report.json"), r.to_json()).unwrap();
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.phase, Phase::Analyze, "{}", r.to_json());
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "E-ANALYZE-IR" && d.message.contains("description required later"))
    );
    // Bypass the producer rejection: the independently checked raw and wire
    // inputs must also reject the old field instance, with the good candidate.
    let target = zil_base::TargetProfile::host_llvm();
    let limits = zil_base::Limits::default();
    let typed = |text: &str| {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "stale-field.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &syntax).unwrap();
        zil_types::check(&core, &target, &limits).unwrap()
    };
    let good = typed(&source);
    let mut candidate = zil_analyze::analyze(&good, &limits).unwrap();
    let forged = typed(&bad);
    candidate.module_fingerprint = forged.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&forged, &candidate, &limits).unwrap();
    for (label, result) in [
        (
            "raw",
            zil_verify::verify_module(forged, candidate, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ),
    ] {
        let errors = result.unwrap_err();
        fs::write(p.join(format!("{label}.txt")), format!("{errors:#?}")).unwrap();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-VERIFY-RESOURCE"),
            "{errors:#?}"
        );
    }
}

#[test]
fn restored_local_nested_borrow_ends_at_last_call_but_cannot_follow_a_move() {
    let prefix =
        format!("{MODEL}fn make()->impl Fn()->i32{{let r=R{{code:65,n:42}};return move ()=>r.n}}");
    let source = format!(
        "{prefix}fn main()->i32{{let f=make();let g=()=>f();let n=g();let moved=f;return n+moved()-84}}"
    );
    native(&source, b"A");
    let bad = format!(
        "{prefix}fn main()->i32{{let f=make();let g=()=>f();let moved=f;return g()+moved()-84}}"
    );
    let p = dir();
    fs::write(p.join("moved-nested.zil"), &bad).unwrap();
    let r = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "moved-nested.zil", bad),
        &CompileOptions::default(),
    );
    fs::write(p.join("report.json"), r.to_json()).unwrap();
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.phase, Phase::Verify, "{}", r.to_json());
}
