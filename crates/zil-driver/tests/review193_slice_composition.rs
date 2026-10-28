//! Independent slice argument composition with actual Package fields and helper roots.
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
            "review193-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn unit(s: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review193.zil", s)
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
fn a_state_callback_receives_its_buffer_and_counter_siblings() {
    let s = "extern \"C\" fn putchar(n:i32)->i32;resource Ticket{n:i32}impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.n)}}}package State<F>{callback:F;manages bytes:ByteBuffer;count:i32}fn make()->impl FnMut(&mut [u8],&mut i32)->i32{let ticket=Ticket{n:65};var calls=0;return move(s:&mut [u8],p:&mut i32)=>{ticket.n;calls+=1;s[0]+=u8(calls);*p+=i32(s[0]);return *p}}fn pack<F:FnMut(&mut [u8],&mut i32)->i32>(callback:F)->State<F>{return State::<F>{callback:callback,bytes:bytes_new(),count:0}}fn run<F:FnMut(&mut [u8],&mut i32)->i32>(state:State<F>)->i32{var current=state;if !bytes_reserve(current.bytes,1){return 90};bytes_push(current.bytes,3u8);let callback=&mut current.callback;let window=bytes_view_mut(current.bytes);let count=&mut current.count;let a=callback(window,count);let b=callback(window,count);return a+b+*count}fn main()->i32{return run(pack(make()))-24}";
    native("state-slice-and-counter", s, b"A", 0);
}
#[test]
fn nested_helpers_preserve_mutable_slice_parent_and_large_copy_values() {
    let prefix = "fn first(s:&mut [i64])->&mut i64{return &mut s[0]}fn leaf<F:Fn(&mut [i64])->i64>(f:&F,s:&mut [i64])->i64{return f(s)}fn outer<F:Fn(&mut [i64])->i64>(f:&F,s:&mut [i64])->i64{return leaf(f,s)+leaf(f,s)}fn main()->i32{var values:[i64;2]=[5000000000,2];let f=(s:&mut [i64])=>{let step=s[1];let p=first(s);*p+=step;return *p};";
    native(
        "nested-wide-slice",
        &format!(
            "{prefix}let total=outer(&f,&mut values[..]);return i32(total-10000000006)+i32(values[0]-5000000004)}}"
        ),
        b"",
        0,
    );
    conflict(
        "nested-parent-current-call",
        &format!(
            "{prefix}let p=&mut values[..];let child=first(p);let total=outer(&f,p);return i32(*child)+i32(total)}}"
        ),
    );
}
#[test]
fn nested_borrowed_descriptors_use_the_actual_last_call_backing() {
    let s = "fn leaf<F:Fn(&[u8])->i32>(f:&F,s:&[u8])->i32{return f(s)}fn outer<F:Fn(&[u8])->i32>(f:&F,s:&[u8])->i32{return leaf(f,s)}fn main()->i32{let data=bytes_new();let other=bytes_new();if !bytes_reserve(data,1){return 90};if !bytes_reserve(other,1){return 91};bytes_push(data,4u8);bytes_push(other,7u8);let f=(s:&[u8])=>{bytes_reserve(data,32);return i32(s[0])};return outer(&f,bytes_view(other))-7}";
    native("nested-other-backing", s, b"", 0);
    conflict(
        "nested-same-backing",
        &s.replace("outer(&f,bytes_view(other))", "outer(&f,bytes_view(data))"),
    );
}
#[test]
fn two_distinct_slice_operands_cannot_relabel_one_actual_backing() {
    let source = "fn main()->i32{var a=[1u8];var b=[2u8];let f=(p:&mut [u8],q:&mut [u8])=>{p[0]+=1u8;q[0]+=2u8;return i32(p[0])+i32(q[0])};return f(&mut a[..],&mut b[..])-6}";
    let (mut m, mut p) = baseline(source);
    let index = m.functions.iter().position(|f| f.name == "main").unwrap();
    let origins = m.functions[index]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| {
            if let I::BorrowLocalMut { local } = i.kind {
                Some(local)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(origins.len(), 2);
    assert_ne!(origins[0], origins[1]);
    let mut changed = 0;
    for i in m.functions[index]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if let I::BorrowLocalMut { local } = &mut i.kind
            && *local == origins[1]
        {
            *local = origins[0];
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    for hold in &mut p.functions[index].holds {
        for root in &mut hold.roots {
            if *root == zil_ir::HoldRoot::Local(origins[1]) {
                *root = zil_ir::HoldRoot::Local(origins[0]);
            }
        }
        hold.roots.sort();
        hold.roots.dedup();
    }
    p.module_fingerprint = m.fingerprint();
    let limits = Limits::default();
    let wire = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    let d = dir("raw-actual-slice-backing");
    fs::write(d.join("source.zil"), source).unwrap();
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
