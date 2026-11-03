//! Concrete life-only mutable returns. Included by the existing borrow target.
use super::*;
use zil_driver::{NativeBackend, Optimization};
use zil_ir::Type;

const FORWARD: &str = include_str!("forward.zil");
const SELECT: &str = "fn select<life a,life b>(p:&'a mut i64,q:&'b mut i64)->&'a mut i64{return p}fn result()->i64{var x:i64=40;var y:i64=2;let p=select(&mut x,&mut y);y=99;*p=*p+2;return x}";

fn native_profiles(text: &str, expected: i64) {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    entry: Some("result".into()),
                    output: Some(temp.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
            execute(&report.artifacts["executable"], expected);
        }
    }
}

#[test]
fn mutable_life_real_sources_keep_actual_origins_modes_and_native_abi() {
    for text in [
        FORWARD,
        SELECT,
        "fn reborrow<life a>(p:&'a mut i64)->&'a mut i64{return &mut *p}fn result()->i64{var x:i64=40;let p=reborrow(&mut x);*p=*p+2;return x}",
        "fn tail<life a>(p:&'a mut [i64])->&'a mut [i64]{return &mut p[1..]}fn result()->i64{var x:[i64;3]=[7,20,20];let p=tail(&mut x[..]);p[1]=22;return p[0]+p[1]}",
        "struct Pair{value:i64}impl Pair{fn field<life a>(self:&'a mut Self)->&'a mut i64{return &mut self.value}}fn result()->i64{var x=Pair{value:40};let p=x.field();*p=*p+2;return x.value}",
        "fn choose<life a>(p:&'a mut i64,q:&'a mut i64,first:bool)->&'a mut i64{if first{return p}return q}fn result()->i64{var x:i64=1;var y:i64=40;let p=choose(&mut x,&mut y,false);*p=*p+2;return y}",
        "fn select<life a>(ignored:&'a i64,p:&'a mut i64)->&'a mut i64{return p}fn result()->i64{var ignored:i64=1;var x:i64=40;let p=select(&ignored,&mut x);ignored=99;*p=*p+2;return x}",
        "fn read<life a>(p:&'a mut i64)->&'a i64{return &*p}fn result()->i64{var x:i64=40;let p=read(&mut x);let n=*p;x=2;return n+x}",
        "fn right<life b,life a>(q:&'b mut i64,p:&'a mut i64)->&'a mut i64{return p}fn relay<life x,life y>(p:&'x mut i64,q:&'y mut i64)->&'x mut i64{return right(q,p)}fn result()->i64{var x:i64=40;var y:i64=1;let p=relay(&mut x,&mut y);y=100;*p=*p+2;return x}",
        "fn forward<life a>(p:&'a mut i64)->&'a mut i64{return p}fn result()->i64{var x:i64=40;let p=forward(&mut x);defer{*p=*p+2}return *p+2}",
    ] {
        native_profiles(text, 42);
    }
    let module = typed(SELECT);
    let select = module.functions.iter().find(|f| f.name == "select").unwrap();
    assert_eq!(select.reference_return_bound, Some(vec![0]));
    assert!(matches!(select.return_type, Type::Address { mutable: true, .. }));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn mutable_life_source_origins_exclusive_windows_and_cleanup_reject_near_misses() {
    for text in [
        "fn bad<life a,life b>(p:&'a mut i64,q:&'b mut i64)->&'a mut i64{return q}fn main(){}",
        "fn bad<life a>(p:&'a mut i64)->&'a mut i64{var x:i64=1;return &mut x}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
        assert!(report.diagnostics.iter().any(|d| d.code == "E-LIFE-RETURN" || d.code == "E-BORROW-ESCAPE"), "{}", report.to_json());
    }
    let report = compile_source(&source("fn bad<life a>(p:&'a i64)->&'a mut i64{return &mut *p}fn main(){}"), &CompileOptions::default());
    assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
    assert!(report.diagnostics.iter().any(|d| d.code == "E-ADDRESS-PERMISSION" || d.code == "E-IMMUTABLE-ASSIGNMENT"), "{}", report.to_json());
    for body in [
        "let p=forward(&mut x);x=99;return *p",
        "let p=forward(&mut x);let q=forward(&mut x);*p=1;return *q",
        "let p=forward(&mut x);let q=&x;*p=1;return *q",
        "let p=forward(&mut x);defer{*p=2}x=99;return 0",
    ] {
        let text = format!("fn forward<life a>(p:&'a mut i64)->&'a mut i64{{return p}}fn result()->i64{{var x:i64=40;{body}}}");
        reject(&text, ResultCategory::SafeReject, Phase::Verify, "E-BORROW-CONFLICT");
    }
    reject(
        "fn first<life a>(p:&'a mut i64,q:&'a mut i64)->&'a mut i64{return p}fn result()->i64{var x:i64=40;return *first(&mut x,&mut x)}",
        ResultCategory::SafeReject, Phase::Verify, "E-BORROW-CONFLICT",
    );
    for text in [
        "fn forward<T:Copy,life a>(p:&'a mut T)->&'a mut T{return p}fn main(){}",
        "fn forward<T:Copy,life a>(p:&'a mut T)->&'a mut T{return p}fn result()->i64{var x:i64=40;return *forward(&mut x)}",
        "fn local<life a>(p:&'a mut i64)->&'a mut i64{let q:&'a mut i64=p;return q}fn main(){}",
    ] {
        reject(text, ResultCategory::Unsupported, Phase::Expand, "E-UNSUPPORTED");
    }
    let incomplete = include_str!("known-incomplete-unused-generic.zil");
    let called = incomplete.replace("fn main(){}", "fn result()->i64{var x:i64=40;return *bad(&mut x)}");
    reject(&called, ResultCategory::SafeReject, Phase::Verify, "E-BORROW-CONFLICT");
}

#[test]
fn mutable_life_raw_and_wire_return_bounds_need_independent_cfg_proofs() {
    let module = typed(SELECT);
    let index = module.functions.iter().position(|f| f.name == "select").unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for bound in [vec![], vec![1], vec![0, 0], vec![1, 0], vec![99]] {
        let mut changed = module.clone();
        changed.functions[index].reference_return_bound = Some(bound);
        let errors = zil_analyze::analyze(&changed, &Limits::default()).unwrap_err();
        assert!(errors.iter().any(|d| d.code == "E-LIFE-RETURN"), "{errors:?}");
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default()).unwrap();
        for result in [
            zil_verify::verify_module(changed.clone(), candidate, &changed.target, &Limits::default()),
            zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()),
        ] {
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|d| d.code == "E-VERIFY-LIFE"), "{errors:?}");
        }
    }
}

#[test]
fn mutable_life_raw_and_wire_cannot_upgrade_shared_authority() {
    let module = typed("fn forward<life a>(p:&'a mut i64)->&'a mut i64{return &mut *p}fn result()->i64{var x:i64=42;return *forward(&mut x)}");
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for mode in 0..2 {
        let mut changed = module.clone();
        let function = changed.functions.iter_mut().find(|f| f.name == "forward").unwrap();
        if mode == 0 {
            let parameter = function.parameters[0];
            let Type::Address { mutable, .. } = &mut function.locals[parameter.0 as usize].ty else { panic!() };
            *mutable = false;
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                if matches!(instruction.kind, InstructionKind::Load(local) if local == parameter) {
                    let Type::Address { mutable, .. } = &mut function.values[instruction.result.unwrap().0 as usize] else { panic!() };
                    *mutable = false;
                }
            }
        } else {
            let Type::Address { mutable, .. } = &mut function.return_type else { panic!() };
            *mutable = false;
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default()).unwrap();
        for result in [
            zil_verify::verify_module(changed.clone(), candidate, &changed.target, &Limits::default()),
            zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()),
        ] {
            let errors = result.unwrap_err();
            assert!(errors.iter().all(|d| d.phase == Phase::Verify), "{errors:?}");
        }
    }
}

#[test]
fn mutable_life_raw_core_metadata_and_permissions_are_rechecked() {
    let unit = source(FORWARD);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    for mode in 0..5 {
        let mut changed = core.clone();
        let function = &mut changed.functions[0];
        let life = function.life.as_mut().unwrap();
        match mode {
            0 => life.binders.clear(),
            1 => life.parameter_lives.clear(),
            2 => life.parameter_lives[0] = Some(99),
            3 => life.result_life = Some(99),
            _ => function.parameters[0].ty = Type::I64,
        }
        let errors = zil_types::check(&changed, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert!(errors.iter().any(|d| d.code == "E-LIFE-CONTRACT"), "{errors:?}");
    }
}
