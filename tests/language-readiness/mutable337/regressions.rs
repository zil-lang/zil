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
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{text}: {}",
                report.to_json()
            );
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
        "fn forward<life a>(p:&'a mut i64)->&'a mut i64{return p}fn adjust(p:&mut i64){let q=forward(p);defer{*q=*q+2}}fn result()->i64{var x:i64=40;adjust(&mut x);return x}",
    ] {
        native_profiles(text, 42);
    }
    let module = typed(SELECT);
    let select = module
        .functions
        .iter()
        .find(|f| f.name == "select")
        .unwrap();
    assert_eq!(select.reference_return_bound, Some(vec![0]));
    assert!(matches!(
        select.return_type,
        Type::Address { mutable: true, .. }
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn mutable_life_source_origins_exclusive_windows_and_cleanup_reject_near_misses() {
    for text in [
        "fn bad<life a,life b>(p:&'a mut i64,q:&'b mut i64)->&'a mut i64{return q}fn main(){}",
        "fn bad<life a,life b>(p:&'a mut i64,q:&'b mut i64,first:bool)->&'a mut i64{if first{return p}return q}fn main(){}",
        "fn bad<life a>(p:&'a mut i64)->&'a mut i64{var x:i64=1;return &mut x}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
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
                .any(|d| d.code == "E-LIFE-RETURN" || d.code == "E-BORROW-ESCAPE"),
            "{}",
            report.to_json()
        );
    }
    let report = compile_source(
        &source("fn bad<life a>(p:&'a i64)->&'a mut i64{return &mut *p}fn main(){}"),
        &CompileOptions::default(),
    );
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
            .any(|d| d.code == "E-ADDRESS-PERMISSION" || d.code == "E-IMMUTABLE-ASSIGNMENT"),
        "{}",
        report.to_json()
    );
    for body in [
        "let p=forward(&mut x);x=99;return *p",
        "let p=forward(&mut x);let q=forward(&mut x);*p=1;return *q",
        "let p=forward(&mut x);let q=&x;*p=1;return *q",
    ] {
        let text = format!(
            "fn forward<life a>(p:&'a mut i64)->&'a mut i64{{return p}}fn result()->i64{{var x:i64=40;{body}}}"
        );
        reject(
            &text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
    reject(
        "fn forward<life a>(p:&'a mut i64)->&'a mut i64{return p}fn result()->i64{var x:i64=40;let p=forward(&mut x);defer{*p=2}x=99;return 0}",
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-BORROW-CONFLICT",
    );
    reject(
        "fn first<life a>(p:&'a mut i64,q:&'a mut i64)->&'a mut i64{return p}fn result()->i64{var x:i64=40;return *first(&mut x,&mut x)}",
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    for text in [
        "fn forward<T:Copy,life a>(p:&'a mut T)->&'a mut T{return p}fn main(){}",
        "fn forward<T:Copy,life a>(p:&'a mut T)->&'a mut T{return p}fn result()->i64{var x:i64=40;return *forward(&mut x)}",
        "fn local<life a>(p:&'a mut i64)->&'a mut i64{let q:&'a mut i64=p;return q}fn main(){}",
    ] {
        reject(
            text,
            ResultCategory::Unsupported,
            Phase::Expand,
            "E-UNSUPPORTED",
        );
    }
    let incomplete = include_str!("known-incomplete-unused-generic.zil");
    let called = incomplete.replace(
        "fn main(){}",
        "fn result()->i64{var x:i64=40;return *bad(&mut x)}",
    );
    reject(
        &called,
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn mutable_life_raw_and_wire_return_bounds_need_independent_cfg_proofs() {
    let module = typed(SELECT);
    let index = module
        .functions
        .iter()
        .position(|f| f.name == "select")
        .unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for bound in [vec![], vec![1], vec![0, 0], vec![1, 0], vec![99]] {
        let mut changed = module.clone();
        changed.functions[index].reference_return_bound = Some(bound);
        let errors = zil_analyze::analyze(&changed, &Limits::default()).unwrap_err();
        assert!(
            errors.iter().any(|d| d.code == "E-LIFE-RETURN"),
            "{errors:?}"
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
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|d| d.code == "E-VERIFY-LIFE"),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn mutable_life_raw_and_wire_cannot_upgrade_shared_authority() {
    let module = typed(
        "fn forward<life a>(p:&'a mut i64)->&'a mut i64{return &mut *p}fn result()->i64{var x:i64=42;return *forward(&mut x)}",
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for mode in 0..2 {
        let mut changed = module.clone();
        let function = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "forward")
            .unwrap();
        if mode == 0 {
            let parameter = function.parameters[0];
            let Type::Address { mutable, .. } = &mut function.locals[parameter.0 as usize].ty
            else {
                panic!()
            };
            *mutable = false;
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                if matches!(instruction.kind, InstructionKind::Load(local) if local == parameter) {
                    let Type::Address { mutable, .. } =
                        &mut function.values[instruction.result.unwrap().0 as usize]
                    else {
                        panic!()
                    };
                    *mutable = false;
                }
            }
        } else {
            let Type::Address { mutable, .. } = &mut function.return_type else {
                panic!()
            };
            *mutable = false;
        }
        let analyzed = zil_analyze::analyze(&changed, &Limits::default());
        if mode == 0 {
            assert!(
                analyzed
                    .as_ref()
                    .unwrap_err()
                    .iter()
                    .any(|d| d.code == "E-LIFE-RETURN")
            );
        }
        println!(
            "337 raw authority attack={mode}: Analyze={:?}",
            analyzed
                .as_ref()
                .map(|_| "Success")
                .map_err(|errors| errors.iter().map(|d| d.code.as_str()).collect::<Vec<_>>())
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
            let errors = result.unwrap_err();
            assert!(!errors.is_empty());
            assert!(
                errors.iter().all(|d| d.phase == Phase::Verify),
                "{errors:?}"
            );
            assert!(
                errors.iter().any(|d| d.code
                    == if mode == 0 {
                        "E-VERIFY-ADDRESS"
                    } else {
                        "E-VERIFY-RETURN"
                    }),
                "{errors:?}"
            );
            println!("337 raw authority attack={mode}: Verify={errors:?}");
        }
    }
}

#[test]
fn mutable_life_raw_core_metadata_and_permissions_are_rechecked() {
    let unit = source(FORWARD);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    for mode in 0..4 {
        let mut changed = core.clone();
        let function = &mut changed.functions[0];
        let life = function.life.as_mut().unwrap();
        match mode {
            0 => life.binders.clear(),
            1 => life.parameter_lives.clear(),
            2 => life.parameter_lives[0] = Some(99),
            _ => life.result_life = Some(99),
        }
        let errors =
            zil_types::check(&changed, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert!(
            errors.iter().any(|d| d.code == "E-LIFE-CONTRACT"),
            "{errors:?}"
        );
    }
}

#[test]
fn mutable_life_raw_wire_windows_ignore_declared_origin_permissions() {
    let module = typed(
        "fn forward<life a>(p:&'a mut i64)->&'a mut i64{return p}fn result()->i64{var x:i64=40;let p=forward(&mut x);let q=forward(&mut x);*p=1;return *q}",
    );
    // Analyze's candidate is not a certificate of exclusive permission. The
    // independent verifier must reconstruct the actual two conflicting loans.
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    for result in [
        zil_verify::verify_module(
            module.clone(),
            candidate,
            &module.target,
            &Limits::default(),
        ),
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
            "{errors:?}"
        );
    }
}

#[test]
fn mutable_life_modules_ct_and_source_caches_recheck_current_actual_windows() {
    let temp = Temp::new();
    let api = "pub fn first<life a>(p:&'a mut i64,q:&'a mut i64)->&'a mut i64{return p}";
    let main = "import api\nfn constant()->i64{return 2}fn result()->i64{var x:i64=40;var y:i64=1;let extra=#(constant());let p=api.first(&mut x,&mut y);y=99;*p=*p+extra;return x}";
    let project = |api: &str| zil_driver::ProjectSources {
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
    let cold = session.compile_project(&project(api), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    let export = cold
        .scalar_interfaces
        .iter()
        .flat_map(|i| &i.exports)
        .find(|f| f.name == "first")
        .unwrap();
    assert_eq!(export.reference_return_bound, Some(vec![0, 1]));
    let actual = cold
        .scalar_interfaces
        .iter()
        .flat_map(|i| i.result_origins.as_deref().unwrap_or_default())
        .find(|f| f.name == "first")
        .unwrap();
    assert_eq!(actual.parameters, [0]);
    for warm in [
        session.compile_project(&project(api), &options),
        zil_driver::compile_project(&project(api), &options),
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
            session.compile_project(&project(&changed), &options),
            zil_driver::compile_project(&project(&changed), &options),
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
    // The caller also participates in cache identity: the callee's valid bound
    // cannot authorize a new root write while its actual result remains live.
    let mut bad_caller = project(api);
    bad_caller.modules[0] = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "main.zil",
        main.replace("y=99", "x=99"),
    );
    for report in [
        session.compile_project(&bad_caller, &options),
        zil_driver::compile_project(&bad_caller, &options),
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

#[test]
fn mutable_life_changed_phases_keep_exact_fresh_request_budgets() {
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
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{phase:?}: {errors:?}"
                    );
                    low = n;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        let errors = run(high - 1).unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
            "{phase:?}: {errors:?}"
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
    let module = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let analyze = boundary(Phase::Analyze, |n| zil_analyze::analyze(&module, &limit(n)));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limit(n))
            .map(|v| v.fingerprint())
    });
    println!(
        "337 mutable phase N: expand={expand} types={types} analyze={analyze} verify={verify}; N-1 ResourceLimit, fresh N Success"
    );
}
