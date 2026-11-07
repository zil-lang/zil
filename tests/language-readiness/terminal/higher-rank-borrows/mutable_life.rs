//! Ordinary Copy-T mutable life preserves actual modes, roots and windows.
use super::*;
use zil_driver::{NativeBackend, Optimization};
use zil_ir::Type;

const FORWARD: &str = "fn forward<T:Copy,life a>(p:&'a mut T)->&'a mut T{return p}fn result()->i64{var x:i64=40;let p=forward(&mut x);*p=*p+2;return x}";
const SELECT: &str = "fn select<life b,T:Copy,life a>(q:&'b mut T,p:&'a mut T)->&'a mut T{return p}fn relay<T:Copy,life x,life y>(p:&'x mut T,q:&'y mut T)->&'x mut T{let r=select(q,p);let ignored=*q;return r}fn result()->i64{var x:i64=40;var y:i64=1;let r=relay(&mut x,&mut y);y=99;*r=*r+2;return x}";

#[test]
fn mixed_mutable_life_sources_have_real_native_addresses() {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
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
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend: *backend,
                        optimization,
                        entry: Some("result".into()),
                        output: Some(temp.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{text}: {}",
                    report.to_json()
                );
                execute(&report.artifacts["executable"], 42);
            }
        }
    }
}

#[test]
fn mixed_mutable_unused_definitions_reject_wrong_roots_and_aliases() {
    for text in [
        "fn bad<T:Copy,life a,life b>(p:&'a mut T,q:&'b mut T)->&'a mut T{return q}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a mut T)->&'a mut T{var x=*p;return &mut x}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a T)->&'a mut T{return &mut *p}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a mut T)->&'a mut T{let q=&mut *p;let r=&mut *p;*q=*r;return p}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a mut T,flag:bool)->&'a mut T{let q=&*p;if flag{let r=&mut *p;}let value=*q;return p}fn main(){}",
        "fn pair<T:Copy,life a>(p:&'a mut T,q:&'a mut T){}fn bad<T:Copy,life a>(p:&'a mut T){pair(p,p)}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(report.artifacts.is_empty());
    }
}

#[test]
fn instantiated_contracts_need_actual_cfg_proof_in_both_kernels_and_wire() {
    let module = typed(SELECT);
    let index = module
        .functions
        .iter()
        .position(|f| f.reference_return_bound == Some(vec![1]))
        .unwrap();
    assert_eq!(
        module.functions[index].reference_return_bound,
        Some(vec![1])
    );
    assert!(matches!(
        module.functions[index].return_type,
        Type::Address { mutable: true, .. }
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    for bound in [vec![], vec![0], vec![1, 1], vec![99]] {
        let mut changed = module.clone();
        changed.functions[index].reference_return_bound = Some(bound);
        assert!(
            zil_analyze::analyze(&changed, &Limits::default())
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-LIFE-RETURN")
        );
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default())
            .unwrap();
        for result in [
            zil_verify::verify_module(
                changed.clone(),
                candidate,
                &changed.target,
                &Limits::default(),
            ),
            zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()),
        ] {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.code == "E-VERIFY-LIFE")
            );
        }
    }
}

#[test]
fn mixed_mutable_phases_keep_exact_fresh_request_budgets() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap();
        while low + 1 < high {
            let n = low + (high - low) / 2;
            match run(n) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = n;
                }
                Err(errors) => {
                    assert!(!errors.is_empty());
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{errors:?}"
                    );
                    low = n;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), expected);
        high
    }
    let unit = source(SELECT);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limit = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let expand = boundary(Phase::Expand, |n| {
        zil_expand::expand_with_map_limits(&unit, &syntax, &limit(n)).map(|m| m.core)
    });
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let types = boundary(Phase::Types, |n| {
        zil_types::check(&core, &TargetProfile::default(), &limit(n))
    });
    let module = typed(SELECT);
    let analyze = boundary(Phase::Analyze, |n| zil_analyze::analyze(&module, &limit(n)));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limit(n))
            .map(|v| v.fingerprint())
    });
    println!(
        "342 mixed mutable phase N: expand={expand} types={types} analyze={analyze} verify={verify}; N-1 ResourceLimit, fresh N identical"
    );
}

#[test]
fn module_ct_live_and_restored_caches_recheck_mixed_mutable_sources() {
    let temp = Temp::new();
    let api = "pub fn first<T:Copy,life a>(p:&'a mut T,q:&'a mut T)->&'a mut T{return p}";
    let main = "import api;fn constant()->i64{return 2}fn result()->i64{var x:i64=40;var y:i64=1;let extra=#(constant());let p=api.first(&mut x,&mut y);y=99;*p=*p+extra;return x}";
    let project = |api: &str, main: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "api.zil", api),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    };
    let options = CompileOptions {
        semantic_cache: Some(temp.0.join("cache")),
        ..CompileOptions::default()
    };
    let mut session = zil_driver::CompilationSession::default();
    let cold = session.compile_project(&project(api, main), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    for warm in [
        session.compile_project(&project(api, main), &options),
        zil_driver::compile_project(&project(api, main), &options),
    ] {
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.verification_runs, 1);
        assert!(warm.cache_hit, "{}", warm.to_json());
        assert_eq!(warm.scalar_interfaces, cold.scalar_interfaces);
    }
    for changed in [
        api.replace("return p", "return q"),
        api.replace("life a>", "life a,life a>"),
        api.replace("p:&'a mut", "p:&'a    "),
    ] {
        for report in [
            session.compile_project(&project(&changed, main), &options),
            zil_driver::compile_project(&project(&changed, main), &options),
        ] {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert!(!report.artifacts.contains_key("executable"));
        }
    }
    let bad = main.replace("y=99", "x=99");
    for report in [
        session.compile_project(&project(api, &bad), &options),
        zil_driver::compile_project(&project(api, &bad), &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-BORROW-CONFLICT"),
            "{}",
            report.to_json()
        );
    }
}
