//! Universally declared callback operations and invocation-local returned loans.
use super::*;
use zil_driver::{NativeBackend, Optimization};

#[test]
fn terminal342_hrtb_callbacks_use_actual_fresh_arguments_and_fixed_environment() {
    let temp=Temp::new();
    let mut backends=vec![NativeBackend::Clang];
    if cfg!(feature="llvm-api") { backends.push(NativeBackend::LlvmApi); }
    for text in [
        "fn apply<F:for<life a> Fn(&'a i64)->i64>(f:&F,p:&i64)->i64{return f(p)}fn result()->i64{let n:i64=40;let offset:i64=2;let f=(p:&i64)=>*p+offset;return apply(&f,&n)}",
        "fn apply<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}fn result()->i64{let n:i64=42;let f=(p:&i64)=>p;let p=apply(&f,&n);return *p}",
        "fn apply<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->&mut i64{return f(p)}fn result()->i64{var n:i64=40;var f=(p:&mut i64)=>{*p+=2;return p};let p=apply(&mut f,&mut n);return *p}",
    ] {
        for backend in &backends { for optimization in [Optimization::O0,Optimization::O2] {
            let report=compile_source(&source(text),&CompileOptions{stop_after:Stage::Build,backend:*backend,optimization,entry:Some("result".into()),output:Some(temp.0.join("program")),..CompileOptions::default()});
            assert_eq!(report.status,ResultCategory::Success,"{text}: {}",report.to_json());
            execute(&report.artifacts["executable"],42);
        } }
    }
}
