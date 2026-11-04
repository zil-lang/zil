//! Universally declared callback operations and invocation-local returned loans.
use super::*;
use zil_driver::{NativeBackend, Optimization};
const PAIR:&str="fn pair<F:for<life a> for<life b> Fn(&'a i64,&'b i64)->&'a i64,G:for<life a> Fn(&'a i64,&'a i64)->&'a i64>(f:&F,g:&G,p:&i64,q:&i64)->i64{let left=f(p,q);let right=g(p,q);return *left+*right}fn result()->i64{let n:i64=21;let other:i64=41;let f=(p:&i64,q:&i64)=>p;return pair(&f,&f,&n,&other)}";


#[test]
fn terminal342_hrtb_callbacks_use_actual_fresh_arguments_and_fixed_environment() {
    let temp=Temp::new();
    let mut backends=vec![NativeBackend::Clang];
    if cfg!(feature="llvm-api") { backends.push(NativeBackend::LlvmApi); }
    for text in [
        "fn apply<F:for<life a> Fn(&'a i64)->i64>(f:&F,p:&i64)->i64{return f(p)}fn result()->i64{let n:i64=40;let offset:i64=2;let f=(p:&i64)=>*p+offset;return apply(&f,&n)}",
        "fn apply<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}fn result()->i64{let n:i64=42;let f=(p:&i64)=>p;let p=apply(&f,&n);return *p}",
        "fn apply<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->&mut i64{return f(p)}fn result()->i64{var n:i64=40;var f=(p:&mut i64)=>{*p+=2;return p};let p=apply(&mut f,&mut n);return *p}",
        "fn apply<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}fn lift(p:&i64)->&i64{let offset:i64=0;let f=(q:&i64)=>{let seen=offset;return q};return apply(&f,p)}fn result()->i64{let n:i64=42;return *lift(&n)}",
        "fn apply<F:for<life a> FnOnce(&'a i64)->&'a i64>(f:F,p:&i64)->&i64{return f(p)}fn result()->i64{let n:i64=42;let f=(p:&i64)=>p;let p=apply(f,&n);return *p}",
        "fn apply<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->&mut i64{return f(p)}fn result()->i64{var a:i64=20;var b:i64=20;var f=(p:&mut i64)=>{*p+=1;return p};let left=apply(&mut f,&mut a);let right=apply(&mut f,&mut b);return *left+*right}",
        PAIR,
    ] {
        for backend in &backends { for optimization in [Optimization::O0,Optimization::O2] {
            let report=compile_source(&source(text),&CompileOptions{stop_after:Stage::Build,backend:*backend,optimization,entry:Some("result".into()),output:Some(temp.0.join("program")),..CompileOptions::default()});
            assert_eq!(report.status,ResultCategory::Success,"{text}: {}",report.to_json());
            execute(&report.artifacts["executable"],42);
        } }
    }
}

#[test]
fn terminal342_unused_hrtb_definitions_cannot_invent_invocation_loans() {
    for text in [
        "fn bad<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->i64{let first=f(&mut *p);let second=f(&mut *p);return *first+*second}fn main(){}",
        "fn bad<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F)->&i64{let n:i64=0;return f(&n)}fn main(){}",
        "fn bad<F:for<life a> FnOnce(&'a i64)->i64>(f:F,p:&i64)->i64{let one=f(p);return one+f(p)}fn main(){}",
        "fn narrow<H:for<life a> for<life b> Fn(&'a i64,&'b i64)->&'a i64>(h:&H,p:&i64,q:&i64)->&i64{return h(p,q)}fn bad<F:for<life a> for<life b> Fn(&'a i64,&'b i64)->&'b i64>(f:&F,p:&i64,q:&i64)->&i64{return narrow(f,p,q)}fn main(){}",
    ] {
        let report=compile_source(&source(text),&CompileOptions::default());
        assert_eq!(report.status,ResultCategory::SafeReject,"{text}: {}",report.to_json());
        assert_eq!(report.phase,Phase::Expand,"{}",report.to_json());
    }
}
#[test]
fn terminal342_each_hrtb_role_checks_actual_callback_roots_on_raw_and_wire() {
    let module=typed(PAIR);
    let plan=zil_analyze::analyze(&module,&Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(),plan.clone(),&module.target,&Limits::default()).unwrap();
    for fault in [0,1,2] {
        let mut changed=module.clone();
        let roles=&mut changed.functions.iter_mut().find(|f|f.callable_contract.as_ref().is_some_and(|c|c.roles.len()==2)).unwrap().callable_contract.as_mut().unwrap().roles;
        assert_eq!(roles[0].closure,roles[1].closure);
        let life=roles[1].life.as_mut().unwrap();
        match fault {
            0=>{life.binder_count=2;life.parameter_lives=vec![Some(0),Some(1)];life.result_life=Some(1);},
            1=>{life.binder_count=2;life.parameter_lives=vec![Some(1),Some(1)];life.result_life=Some(1);},
            _=>{life.binder_count=u32::MAX;},
        }
        assert!(zil_analyze::analyze(&changed,&Limits::default()).is_err());
        let mut fake=plan.clone();fake.module_fingerprint=changed.fingerprint();
        let raw=zil_verify::verify_module(changed.clone(),fake.clone(),&changed.target,&Limits::default()).unwrap_err();
        if fault==0 {assert!(raw.iter().any(|d|d.code=="E-VERIFY-CALLABLE-LIFE"),"{raw:?}");}
        if fault==2 {assert!(raw.iter().all(|d|d.category==ResultCategory::ResourceLimit),"{raw:?}");continue;}
        let wire=zil_ir::wire::encode_module_candidate(&changed,&fake,&Limits::default()).unwrap();
        let decoded=zil_verify::verify_module_bytes(&wire,&changed.target,&Limits::default()).unwrap_err();
        assert!(decoded.iter().all(|d|d.category==ResultCategory::SafeReject),"{decoded:?}");
    }
}
