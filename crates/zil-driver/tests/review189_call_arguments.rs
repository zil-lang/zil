//! Independent invocation/affinity and error-path controls for189.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, InstructionKind as I, TypedModule};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "review189-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn unit(s: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review189.zil", s)
}
fn native(label: &str, s: &str, stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let p = dir(label);
        fs::write(p.join("main.zil"), s).unwrap();
        let executable = p.join("program");
        let report = compile_source(
            &unit(s),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(p.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&executable).output().unwrap();
        fs::write(p.join("stdout"), &output.stdout).unwrap();
        fs::write(p.join("stderr"), &output.stderr).unwrap();
        fs::write(
            p.join("run.txt"),
            format!(
                "{optimization:?}\n{executable:?}\nexit {:?}; expected {exit}\n",
                output.status.code()
            ),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(exit));
        assert_eq!(output.stdout, stdout);
        assert!(output.stderr.is_empty());
    }
}
fn conflict(label: &str, s: &str) {
    let p = dir(label);
    fs::write(p.join("main.zil"), s).unwrap();
    let report = compile_source(&unit(s), &CompileOptions::default());
    fs::write(p.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}
fn baseline(s: &str) -> (TypedModule, CandidatePlan) {
    let u = unit(s);
    let a = zil_syntax::parse(&u).into_result().unwrap();
    let c = zil_expand::expand(&u, &a).unwrap();
    let m = zil_types::check(&c, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    (m, p)
}
#[test]
fn reborrowed_aggregate_arguments_keep_actual_parent_and_restore_after_call() {
    let s = "struct Pair{x:i32;y:i32}fn pass(p:&mut Pair)->&mut Pair{return p}fn main()->i32{var a=Pair{x:2,y:3};var b=Pair{x:5,y:7};let pa=pass(&mut a);let pb=pass(&mut b);let exchange=(p:&mut Pair,q:&mut Pair)=>{let n=p.x;p.x=q.y;q.y=n};exchange(pa,pb);exchange(pa,pb);return a.x+a.y+b.x+b.y-17}";
    native("aggregate-parents", s, b"", 0);
    conflict(
        "same-aggregate",
        "struct Pair{x:i32;y:i32}fn pass(p:&mut Pair)->&mut Pair{return p}fn main()->i32{var a=Pair{x:2,y:3};let pa=pass(&mut a);let edit=(p:&mut Pair,q:&mut Pair)=>{p.x+=1;q.y+=1};edit(pa,pa);return 0}",
    );
}
#[test]
fn consuming_error_callback_settles_owned_capture_after_mutating_argument() {
    let base = "extern \"C\" fn putchar(n:i32)->i32;enum E{No}resource R{n:i32;code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}fn eat(give r:R){}fn main()->E!i32{let guard=R{n:0,code:90};let r=R{n:2,code:65};var n=40;let f=move(p:&mut i32,fail:bool)->E!i32=>{*p+=r.n;eat(give r);if fail{return E.No};return *p};let result=f(&mut n,FAIL)?;return result+n-84}";
    for (fail, exit) in [(false, 0), (true, 1)] {
        native(
            "once-error",
            &base.replace("FAIL", if fail { "true" } else { "false" }),
            b"AZ",
            exit,
        );
    }
}
#[test]
fn descriptor_capture_is_substituted_at_ordinary_caller_with_extra_reference() {
    let base = "resource R{n:i32}fn apply<F:Fn()->i32>(f:&F,p:&i32)->i32{return f()+*p}fn outer(d:R,p:&i32)->i32{let f=()=>{d.n=9;return 1};return apply(&f,p)}fn main()->i32{let a=R{n:4};let b=R{n:4};let h=&b;let p=&h.n;return outer(a,p)-5}";
    native("ordinary-other", base, b"", 0);
    conflict("ordinary-same", &base.replace("outer(a,p)", "outer(b,p)"));
}
#[test]
fn raw_and_wire_reject_two_current_exclusive_parameters_pointing_at_one_value() {
    let source = "fn main()->i32{var a=2;var b=3;let f=(p:&mut i32,q:&mut i32)=>{*p+=1;*q+=2};f(&mut a,&mut b);return a+b-8}";
    let (mut m, mut p) = baseline(source);
    let mut changed = 0;
    for f in &mut m.functions {
        for b in &mut f.blocks {
            for i in &mut b.instructions {
                if let I::CallClosure { arguments, .. } = &mut i.kind {
                    assert_eq!(arguments.len(), 2);
                    arguments[1] = arguments[0];
                    changed += 1;
                }
            }
        }
    }
    assert_eq!(changed, 1);
    p.module_fingerprint = m.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(decoded, (m.clone(), p.clone()));
    let results = [
        zil_verify::verify_module(m.clone(), p.clone(), &m.target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &m.target, &limits).unwrap_err(),
    ];
    let out = dir("raw-double-exclusive");
    fs::write(out.join("source.zil"), source).unwrap();
    fs::write(out.join("module.txt"), format!("{m:#?}")).unwrap();
    fs::write(out.join("candidate.txt"), format!("{p:#?}")).unwrap();
    fs::write(out.join("diagnostics.txt"), format!("{results:#?}")).unwrap();
    for errors in results {
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject
                    && d.phase == Phase::Verify
                    && d.code == "E-BORROW-CONFLICT"),
            "{errors:?}"
        );
    }
}
