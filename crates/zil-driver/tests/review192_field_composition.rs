//! Independent physical field, parent, and helper composition probes for192.
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
            "review192-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn unit(s: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review192.zil", s)
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
fn aligned_and_zero_sized_fields_preserve_inline_storage() {
    let s = "struct Empty{} struct Pair{large:i64;small:u8} resource R{n:i32} package P{manages owner:R;empty:Empty;byte:u8;pair:Pair;tail:i64} fn pick(p:&mut Pair)->&mut i64{return &mut p.large} fn main()->i32{var state=P{owner:R{n:1},empty:Empty{},byte:7u8,pair:Pair{large:5000000000i64,small:9u8},tail:11i64};let z=&mut state.empty;*z=Empty{};let p=pick(&mut state.pair);let t=&mut state.tail;*p+=*t;*t+=i64(state.byte);return i32(*p-5000000011i64)+i32(*t-18i64)}";
    native("alignment", s, b"", 0);
    conflict(
        "projection-parent",
        &s.replace("*p+=*t;", "let illegal=state.pair.large;*p+=*t+illegal;"),
    );
}
#[test]
fn an_owning_sibling_can_be_taken_and_replaced_during_a_copy_window() {
    let s = "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package P{manages owner:R;count:i64}fn main()->i32{var state=P{owner:R{n:65},count:2i64};let p=&mut state.count;{let old=give state.owner;state.owner=R{n:66};*p+=i64(old.n)}return i32(*p)-67}";
    native("sibling-take-replace", s, b"AB", 0);
}
#[test]
fn nested_helpers_keep_owned_environment_and_explicit_field_arguments_separate() {
    let s = "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package P{manages owner:R;count:i64}fn leaf<F:FnMut(&mut i64)->i64>(f:&mut F,p:&mut i64)->i64{return f(p)}fn twice<F:FnMut(&mut i64)->i64>(f:&mut F,p:&mut i64)->i64{return leaf(f,p)+leaf(f,p)}fn main()->i32{var state=P{owner:R{n:66},count:20i64};var captured=P{owner:R{n:65},count:10i64};var f=move(p:&mut i64)=>{let c=&mut captured.count;*c+=*p;*p+=1i64;return *c};let p=&mut state.count;let total=twice(&mut f,p);return i32(total+*p)-103}";
    native("owned-helper-field", s, b"AB", 0);
}
#[test]
fn raw_read_retargeting_keeps_actual_parent_identity() {
    let s = "resource R{n:i32}package P{manages r:R;x:i32}fn main()->i32{var a=P{r:R{n:1},x:20};let b=P{r:R{n:2},x:1};let p=&mut a.x;let old=b.x;*p+=old;return *p-21}";
    let (mut m, mut p) = baseline(s);
    let f = m.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let borrowed = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::BorrowPackageField { local, .. } = i.kind {
                Some(local)
            } else {
                None
            }
        })
        .unwrap();
    let mut changes = 0;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::ReadPackageField { local, index } = &mut i.kind {
            assert_ne!(*local, borrowed);
            assert_eq!(*index, 1);
            *local = borrowed;
            changes += 1;
        }
    }
    assert_eq!(changes, 1);
    p.module_fingerprint = m.fingerprint();
    let limits = Limits::default();
    let wire = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    let d = dir("retarget-parent-raw");
    fs::write(d.join("source.zil"), s).unwrap();
    fs::write(d.join("module.txt"), format!("{m:#?}")).unwrap();
    fs::write(d.join("candidate.txt"), format!("{p:#?}")).unwrap();
    fs::write(d.join("candidate.bin"), &wire).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(m.clone(), p, &m.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &m.target, &limits),
        ),
    ] {
        let errors = result.unwrap_err();
        fs::write(d.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
        assert_eq!(errors[0].phase, Phase::Verify);
        assert_eq!(errors[0].category, ResultCategory::SafeReject);
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT");
    }
}
