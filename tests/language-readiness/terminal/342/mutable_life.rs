//! Ordinary Copy-T mutable life preserves actual modes, roots and windows.
use super::*;
use zil_driver::{NativeBackend, Optimization};
use zil_ir::Type;

const FORWARD: &str = "fn forward<T:Copy,life a>(p:&'a mut T)->&'a mut T{return p}fn result()->i64{var x:i64=40;let p=forward(&mut x);*p=*p+2;return x}";
const SELECT: &str = "fn select<life b,T:Copy,life a>(q:&'b mut T,p:&'a mut T)->&'a mut T{return p}fn relay<T:Copy,life x,life y>(p:&'x mut T,q:&'y mut T)->&'x mut T{let r=select(q,p);let ignored=*q;return r}fn result()->i64{var x:i64=40;var y:i64=1;let r=relay(&mut x,&mut y);y=99;*r=*r+2;return x}";

#[test]
fn terminal342_mixed_mutable_life_sources_have_real_native_addresses() {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") { backends.push(NativeBackend::LlvmApi); }
    for text in [
        FORWARD,
        SELECT,
        "fn again<T:Copy,life T>(p:&'T mut T)->&'T mut T{let q=&mut *p;return q}fn result()->i64{var x:i64=40;let p=again::<i64>(&mut x);*p=*p+2;return x}",
        "fn tail<T:Copy,life a>(p:&'a mut [T])->&'a mut [T]{return &mut p[1..]}fn result()->i64{var x:[i64;3]=[7,20,20];let p=tail(&mut x[..]);p[1]=22;return p[0]+p[1]}",
        "struct Pair<T:Copy>{value:T}fn field<T:Copy,life a>(p:&'a mut Pair<T>)->&'a mut T{return &mut p.value}fn result()->i64{var x=Pair::<i64>{value:40};let p=field(&mut x);*p=*p+2;return x.value}",
        "fn read<T:Copy,life a>(p:&'a mut T)->&'a T{return &*p}fn result()->i64{var x:i64=40;let p=read(&mut x);let n=*p;x=2;return n+x}",
        "fn choose<T:Copy,life a>(p:&'a mut T,q:&'a mut T,flag:bool)->&'a mut T{if flag{return p}return q}fn result()->i64{var x:i64=1;var y:i64=40;let p=choose(&mut x,&mut y,false);*p=*p+2;return y}",
        "fn select<T:Copy,life a>(q:&'a T,p:&'a mut T)->&'a mut T{return p}fn relay<T:Copy,life a>(q:&'a T,p:&'a mut T)->&'a mut T{return select(q,p)}fn result()->i64{var q:i64=1;var x:i64=40;let p=relay(&q,&mut x);q=99;*p=*p+2;return x}",
        "fn forward<T:Copy,life a>(p:&'a mut T)->&'a mut T{return p}fn adjust(p:&mut i64){let q=forward(p);defer{*q=*q+2}}fn result()->i64{var x:i64=40;adjust(&mut x);return x}",
    ] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(&source(text), &CompileOptions {
                    stop_after: Stage::Build,
                    backend: *backend,
                    optimization,
                    entry: Some("result".into()),
                    output: Some(temp.0.join("program")),
                    ..CompileOptions::default()
                });
                assert_eq!(report.status, ResultCategory::Success, "{text}: {}", report.to_json());
                execute(&report.artifacts["executable"], 42);
            }
        }
    }
}

#[test]
fn terminal342_mixed_mutable_unused_definitions_reject_wrong_roots_and_aliases() {
    for text in [
        "fn bad<T:Copy,life a,life b>(p:&'a mut T,q:&'b mut T)->&'a mut T{return q}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a mut T)->&'a mut T{var x=*p;return &mut x}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a T)->&'a mut T{return &mut *p}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a mut T)->&'a mut T{let q=&mut *p;let r=&mut *p;*q=*r;return p}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a mut T,flag:bool)->&'a mut T{let q=&*p;if flag{let r=&mut *p;}let value=*q;return p}fn main(){}",
        "fn pair<T:Copy,life a>(p:&'a mut T,q:&'a mut T){}fn bad<T:Copy,life a>(p:&'a mut T){pair(p,p)}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(report.status, ResultCategory::SafeReject, "{text}: {}", report.to_json());
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(report.artifacts.is_empty());
    }
}

#[test]
fn terminal342_instantiated_contracts_need_actual_cfg_proof_in_both_kernels_and_wire() {
    let module = typed(SELECT);
    let index = module.functions.iter().position(|f| f.reference_return_bound == Some(vec![1])).unwrap();
    assert_eq!(module.functions[index].reference_return_bound, Some(vec![1]));
    assert!(matches!(module.functions[index].return_type, Type::Address { mutable: true, .. }));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    for bound in [vec![], vec![0], vec![1,1], vec![99]] {
        let mut changed = module.clone();
        changed.functions[index].reference_return_bound = Some(bound);
        assert!(zil_analyze::analyze(&changed, &Limits::default()).unwrap_err().iter().any(|d| d.code == "E-LIFE-RETURN"));
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default()).unwrap();
        for result in [zil_verify::verify_module(changed.clone(), candidate, &changed.target, &Limits::default()), zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default())] {
            assert!(result.unwrap_err().iter().any(|d| d.code == "E-VERIFY-LIFE"));
        }
    }
}
