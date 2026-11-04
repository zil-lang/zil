//! Shared argument/result contracts: real source, native O0/O2, and forged facts.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{HoldRoot, InstructionKind, LocalId, TypedModule};

#[path = "../../../tests/language-readiness/review331/cache.rs"]
mod independent331_cache;

#[path = "../../../tests/language-readiness/mutable337/regressions.rs"]
mod mutable337;

#[path = "../../../tests/language-readiness/terminal/342/mutable_life.rs"]
mod terminal342_mutable;

#[path = "../../../tests/language-readiness/terminal/342/borrowed_fp.rs"]
mod terminal342_borrowed_fp;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-call-borrow-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "shared.zil", text)
}
fn execute(path: &Path, expected: i64) {
    let mut child = Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native shared-address program timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
    assert!(output.stderr.is_empty());
}
fn native(text: &str, expected: i64) -> String {
    let temp = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("program")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(
        llvm.contains("getelementptr i8, ptr %local."),
        "missing real source address: {llvm}"
    );
    execute(&report.artifacts["executable"], expected);
    let optimized = temp.0.join("optimized");
    let clang = Command::new("clang")
        .args(["--target=x86_64-unknown-linux-gnu", "-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        clang.status.success(),
        "{}",
        String::from_utf8_lossy(&clang.stderr)
    );
    execute(&optimized, expected);
    llvm
}
fn reject(text: &str, category: ResultCategory, phase: Phase, code: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}
fn typed(text: &str) -> TypedModule {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

const LIFE_IDENTITY322: &str =
    include_str!("../../../tests/language-readiness/recovery/pending/life-parameter-return.zil");

const TYPE_LIFE331: &str = "fn first<T:Copy,life a>(p:&'a T)->&'a T{return p}fn result()->i64{let value:i64=42;let flag=true;if *first::<bool>(&flag){return *first(&value)}return 0}";

#[test]
fn type_life_instances_keep_binders_dense_maps_and_native_copy_types() {
    native(TYPE_LIFE331, 42);
    let syntax = zil_syntax::parse(&source(TYPE_LIFE331))
        .into_result()
        .unwrap();
    let core = zil_expand::expand(&source(TYPE_LIFE331), &syntax).unwrap();
    let instances: Vec<_> = core.functions.iter().filter(|f| f.life.is_some()).collect();
    assert_eq!(instances.len(), 2, "{core:?}");
    for function in instances {
        let life = function.life.as_ref().unwrap();
        assert_eq!(life.binders, ["a"]);
        assert_eq!(life.parameter_lives, [Some(0)]);
        assert_eq!(life.result_life, Some(0));
        assert!(function.type_parameters.is_empty());
    }
    for text in [
        "fn select<life b,T:Copy,life a>(right:&'b T,left:&'a T)->&'a T{return left}fn relay<T:Copy,life x,life y>(left:&'x T,right:&'y T)->&'x T{return select(right,left)}fn result()->i64{let a:i64=40;let b:i64=2;return *relay(&a,&b)+2}",
        "fn choose<T:Copy,life a>(p:&'a T,q:&'a T,flag:bool)->&'a T{var r=p;if flag{r=q}return r}fn result()->i64{let a:i64=1;let b:i64=42;return *choose(&a,&b,true)}",
        "fn reborrow<T:Copy,life a>(p:&'a T)->&'a T{let q=&*p;return q}fn result()->i64{let a:i64=42;return *reborrow(&a)}",
        "fn tail<T:Copy,life a>(p:&'a[T])->&'a[T]{return &p[1..]}fn result()->i64{let a:[i64;3]=[7,20,22];let p=tail(&a[..]);return p[0]+p[1]}",
        "struct Pair<T:Copy>{value:T}fn project<T:Copy,life a>(p:&'a Pair<T>)->&'a T{return &p.value}fn result()->i64{let p=Pair::<i64>{value:42};return *project(&p)}",
        "fn first<T:Copy,life T>(p:&'T T)->&'T T{return p}fn result()->i64{let value:i64=42;return *first(&value)}",
        "fn first<T:Copy,life a>(p:&'a T,n:i32)->&'a T{if n==0{return p}return first(p,n-1)}fn result()->i64{let value:i64=42;return *first(&value,3)}",
        "struct Point{value:i64}impl Point{fn read<life b>(self:&'b Self)->&'b i64{return &self.value}}fn first<T:Copy,life a>(p:&'a T)->&'a T{return p}fn constant()->i64{return 2}fn result()->i64{let p=Point{value:40};let extra=#(constant());return *first::<i64>(p.read())+extra}",
    ] {
        native(text, 42);
    }
}

#[test]
fn type_life_unused_templates_check_symbolic_copy_and_actual_return_roots() {
    for text in [
        "fn bad<T:Copy,life a,life b>(p:&'a T,q:&'b T)->&'a T{return q}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a T)->&'a T{let q=*p;return &q}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a T,q:T)->&'a T{return &q}fn main(){}",
        "fn bad<T:Copy,life a,life b>(p:&'a T,q:&'b T,flag:bool)->&'a T{var r=p;if flag{r=q}return r}fn main(){}",
        "fn bad<T:Copy,life a>(p:&T,q:&'a T)->&'a T{return p}fn main(){}",
        "fn last<T:Copy,life a,life b>(p:&'a T,q:&'b T)->&'b T{return q}fn bad<T:Copy,life a,life b>(p:&'a T,q:&'b T)->&'a T{return last(p,q)}fn main(){}",
        "fn bad<T:Copy,life a,life b>(p:&'a T,q:&'b T)->&'a T{return q}fn result()->i64{let value:i64=42;return *bad(&value,&value)}",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-LIFE-ORIGIN",
        );
    }
    for text in [
        "fn bad<T:Copy,life a>(p:&'a T)->T{return *p+*p}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a T)->T{return p.value}fn main(){}",
        "fn bad<T,life a>(p:&'a T)->&'a T{return p}fn main(){}",
        "fn bad<T:Copy,U:Copy,life a>(p:&'a T,q:U)->&'a T{return p}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'a T)->&'a T where T:Copy{return p}fn main(){}",
        "struct Point{value:i64}impl Point{fn bad<T:Copy,life a>(self:&'a Self,p:&'a T)->&'a T{return p}}fn main(){}",
        "fn old(p:&i32)->&i32{return p}fn bad<T:Copy,life a>(p:&'a T,q:&i32)->&i32{return old(q)}fn main(){}",
    ] {
        reject(
            text,
            ResultCategory::Unsupported,
            Phase::Expand,
            "E-UNSUPPORTED",
        );
    }
    for text in [
        "fn bad<T:Copy,life a,life a>(p:&'a T)->&'a T{return p}fn main(){}",
        "fn bad<T:Copy,life a>(p:&'b T)->&'a T{return p}fn main(){}",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-LIFE-DECLARATION",
        );
    }
    for text in [
        "fn first<T:Copy,life a>(p:&'a T)->&'a T{return p}fn result()->i64{let p:i64=42;return *first::<i64,bool>(&p)}",
        "fn first<T:Copy,life a>(p:&'a T)->&'a T{return p}fn result()->i64{let p:i64=42;return *first::<&i64>(&(&p))}",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        );
    }
}

#[test]
fn type_life_module_alias_ct_and_source_caches_preserve_the_relation() {
    let temp = Temp::new();
    let api =
        "type A=i64;pub fn first<life A,T:Copy,life b>(p:&'A T,q:&'b T,extra:A)->&'A T{return p}";
    let project = |api: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import api\nfn constant()->i64{return 2}fn result()->i64{let p:i64=40;let q:i64=2;let extra=#(constant());return *api.first(&p,&q,q)+extra}",
            ),
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
    for warm in [
        session.compile_project(&project(api), &options),
        zil_driver::compile_project(&project(api), &options),
    ] {
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.verification_runs, 1);
        assert_eq!(warm.scalar_interfaces, cold.scalar_interfaces);
    }
    assert_eq!(
        zil_driver::compile_project(&project(api), &options).disk_cache,
        zil_driver::DiskCacheStatus::Hit
    );
    let wrong = api.replace("return p", "return q");
    assert_eq!(wrong.len(), api.len());
    for report in [
        session.compile_project(&project(&wrong), &options),
        zil_driver::compile_project(&project(&wrong), &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, "E-LIFE-ORIGIN");
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn type_life_instantiated_raw_bounds_still_need_two_independent_cfg_checks() {
    let module = typed(
        "fn select<life b,T:Copy,life a>(right:&'b T,left:&'a T)->&'a T{return left}fn result()->i64{let p:i64=40;let q:i64=2;return *select(&p,&q)}",
    );
    let index = module
        .functions
        .iter()
        .position(|f| f.reference_return_bound.is_some())
        .unwrap();
    assert_eq!(
        module.functions[index].reference_return_bound,
        Some(vec![1])
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for bound in [vec![], vec![0], vec![1, 1], vec![1, 0], vec![99]] {
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
                    .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-LIFE")
            );
        }
    }
}

#[test]
fn type_life_raw_scalar_live_and_restored_reuse_recheck_the_current_header() {
    let unit = source(
        "fn scalar<T:Copy,life a,life b>(p:T)->T{let n0=p;let n1=n0;let n2=n1;return n2}fn result()->i32{return scalar(42)}",
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let original = zil_ir::CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    let limits = Limits::default();
    let target = TargetProfile::default();
    let prepared = zil_types::infer_program_for_ct(&original, &limits)
        .unwrap()
        .into_prepared();
    let mut seed = zil_types::FunctionSession::default();
    let fresh = seed
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    let encoded = seed.encode_work_products(&target, &limits).unwrap();
    for restored in [false, true] {
        let mut session = seed.clone();
        if restored {
            session = zil_types::FunctionSession::default();
            assert!(session.restore_work_products(&encoded, &target, &limits));
        }
        assert_eq!(
            session
                .check_prepared_program(&prepared, &target, &limits)
                .unwrap(),
            fresh
        );
        assert!(session.last_stats().reused_functions > 0);
    }
    let index = original.modules[0]
        .core
        .functions
        .iter()
        .position(|f| f.life.is_some())
        .unwrap();
    for attack in 0..6 {
        let mut changed = original.clone();
        let life = changed.modules[0].core.functions[index]
            .life
            .as_mut()
            .unwrap();
        match attack {
            0 => life.binders.clear(),
            1 => life.binders[1] = life.binders[0].clone(),
            2 => life.parameter_lives.clear(),
            3 => life.parameter_lives[0] = Some(0),
            4 => life.result_life = Some(0),
            _ => life.result_life = Some(99),
        }
        assert!(
            zil_types::check_program(&changed, &target, &limits)
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-LIFE-CONTRACT")
        );
        let prepared = zil_types::infer_program_for_ct(&changed, &limits)
            .unwrap()
            .into_prepared();
        for restored in [false, true] {
            let mut session = seed.clone();
            if restored {
                session = zil_types::FunctionSession::default();
                assert!(session.restore_work_products(&encoded, &target, &limits));
            }
            assert!(
                session
                    .check_prepared_program(&prepared, &target, &limits)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.code == "E-LIFE-CONTRACT")
            );
        }
    }
}

#[test]
fn type_life_symbolic_alias_and_module_work_share_exact_fresh_expand_budget() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        label: &str,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(!errors.is_empty());
                    assert!(
                        errors.iter().all(|d| d.phase == Phase::Expand
                            && d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        let errors = run(high - 1).unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|d| d.phase == Phase::Expand && d.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        assert_eq!(run(high).unwrap(), expected);
        println!("331 {label} Expand N={high}; N-1 ResourceLimit; fresh N identical");
    }
    let limits = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    for (label, text) in [
        ("original", TYPE_LIFE331),
        (
            "symbolic-callee-positions",
            "fn right<life b,T:Copy,life a>(p:&'b T,q:&'a T)->&'a T{return q}fn first<T:Copy,life x,life y>(p:&'x T,q:&'y T)->&'x T{let r=right(q,p);return r}fn result()->i64{let p:i64=42;let q:i64=1;return *first(&p,&q)}",
        ),
        (
            "unused-symbolic",
            "fn first<T:Copy,life a>(p:&'a T)->&'a T{let r=&*p;return r}fn main(){}",
        ),
    ] {
        let unit = source(text);
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        boundary(label, |n| {
            zil_expand::expand_with_map_limits(&unit, &ast, &limits(n)).map(|m| m.core)
        });
    }
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import api;fn result()->i64{let p:i64=42;return *api.first(&p,p)}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "api.zil",
            "type A=i64;pub fn first<life A,T:Copy>(p:&'A T,extra:A)->&'A T{return p}",
        ),
    ];
    let asts = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = [
        vec![zil_ir::CoreImport {
            alias: "api".into(),
            module: ModuleId(1),
            span: zil_base::Span::default(),
        }],
        vec![],
    ];
    boundary("module-alias", |n| {
        zil_expand::expand_program_with_limits(
            &sources,
            &asts.iter().collect::<Vec<_>>(),
            &imports,
            &limits(n),
        )
        .map(|m| m.into_iter().map(|(m, _)| m).collect::<Vec<_>>())
    });
}

#[test]
fn explicit_life_identity_keeps_core_ct_and_actual_native_origins() {
    let r = compile_source(&source(LIFE_IDENTITY322), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    let module = typed(LIFE_IDENTITY322);
    assert_eq!(
        module
            .functions
            .iter()
            .find(|f| f.name == "identity")
            .unwrap()
            .reference_return_bound,
        Some(vec![0])
    );
    native(&LIFE_IDENTITY322.replace("fn main()", "fn result()"), 0);
    native(
        "fn select<life a,life b>(left:&'a i64,right:&'b i64)->&'a i64{return left}fn result()->i64{var left:i64=40;var right:i64=2;let p=select(&left,&right);right=99;return *p+right}",
        139,
    );
    native(
        "fn select<life a>(left:&'a i64,right:&'a i64,first:bool)->&'a i64{if first{return left}return right}fn result()->i64{var left:i64=40;var right:i64=2;let p=select(&left,&right,false);let n=*p;left=100;right=200;return n+left+right}",
        302,
    );
    native(
        "struct Point{value:i64}impl Point{fn read<life a>(self:&'a Self)->&'a i64{return &self.value}}fn result()->i64{let p=Point{value:42};return *p.read()}",
        42,
    );
    native(
        "fn identity<life a>(s:&'a[i64])->&'a[i64]{return s}fn result()->i64{let values:[i64;2]=[20,22];let view=identity(&values[..]);return view[0]+view[1]}",
        42,
    );
}

#[test]
fn explicit_life_definitions_reject_wrong_binders_and_unsupported_positions() {
    for text in [
        include_str!("../../../tests/language-readiness/recovery/probes/undeclared-life.zil"),
        include_str!(
            "../../../tests/language-readiness/recovery/probes/mismatched-undeclared-life.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/recovery/probes/receiver-undeclared-life.zil"
        ),
        "fn f<life a,life a>(p:&'a i32)->&'a i32{return p}fn main(){}",
        "fn f<life a>()->&'a i32{let x=1;return &x}fn main(){}",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-LIFE-DECLARATION",
        );
    }
    for text in [
        "fn f<life a,life b>(p:&'a i32,q:&'b i32)->&'a i32{return q}fn main(){}",
        "fn f<life a>(p:&'a i32)->&'a i32{let x=1;return &x}fn main(){}",
        "fn inner(p:&i32,q:&i32)->&i32{return q}fn outer<life a,life b>(p:&'a i32,q:&'b i32)->&'a i32{return inner(p,q)}fn main(){}",
        "fn f<life a>(p:&i32,q:&'a i32)->&'a i32{return p}fn main(){}",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-LIFE-RETURN",
        );
    }
    // 337 admits the mutable life header. Returning its mutable value directly
    // into a shared result still requires an explicit shared reborrow.
    reject(
        "fn f<life a>(p:&'a mut i32)->&'a i32{return p}fn main(){}",
        ResultCategory::SafeReject,
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
    for text in [
        "fn f<life a>(p:&'a i32)->&'a i32{let local:&'a i32=p;return local}fn main(){}",
        "fn f<life a>(p:fn(&'a i32)->i32){}fn main(){}",
        include_str!("../../../tests/language-readiness/recovery/pending/life-higher-rank.zil"),
    ] {
        reject(
            text,
            ResultCategory::Unsupported,
            Phase::Expand,
            "E-UNSUPPORTED",
        );
    }
    native(
        "fn f<life a>(p:&'a i64,q:&i64)->&i64{return q}fn result()->i64{let p:i64=1;let q:i64=42;return *f(&p,&q)}",
        42,
    );
}

#[test]
fn explicit_life_raw_bounds_are_checked_independently_after_the_fixed_point() {
    let module = typed(
        "fn first<life a,life b>(p:&'a i32,q:&'b i32,flag:bool)->&'a i32{return p}fn result()->i32{let x=1;let y=2;return *first(&x,&y,false)}",
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for bound in [vec![], vec![1], vec![0, 0], vec![1, 0], vec![2], vec![99]] {
        let mut changed = module.clone();
        changed
            .functions
            .iter_mut()
            .find(|f| f.name == "first")
            .unwrap()
            .reference_return_bound = Some(bound);
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
                errors
                    .iter()
                    .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-LIFE"),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn explicit_life_project_ct_and_warm_interface_keep_the_declared_upper_bound() {
    let temp = Temp::new();
    let main = "import api\nfn constant()->i32{return 0}fn result()->i32{let x=42;let ignored=#(constant());return *api.identity(&x)+ignored-42}";
    let api = "pub fn identity<life a>(p:&'a i32)->&'a i32{return p}";
    let project = zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "api.zil", api),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    };
    let options = CompileOptions {
        semantic_cache: Some(temp.0.join("cache")),
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let cold = zil_driver::compile_project(&project, &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    let warm = zil_driver::compile_project(&project, &options);
    assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
    assert_eq!(
        warm.disk_cache,
        zil_driver::DiskCacheStatus::Hit,
        "{}",
        warm.to_json()
    );
    assert_eq!(warm.verification_runs, 1);
    assert_eq!(cold.scalar_interfaces, warm.scalar_interfaces);
    assert_eq!(
        warm.scalar_interfaces
            .iter()
            .flat_map(|i| &i.exports)
            .find(|f| f.name == "identity")
            .unwrap()
            .reference_return_bound,
        Some(vec![0])
    );
}

#[test]
fn explicit_life_core_metadata_is_not_a_type_proof() {
    let unit = source("fn first<life a,life b>(p:&'a i32,q:&'b i32)->&'a i32{return p}");
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    for attack in 0..5 {
        let mut changed = core.clone();
        let life = changed.functions[0].life.as_mut().unwrap();
        match attack {
            0 => life.binders[1] = life.binders[0].clone(),
            1 => life.parameter_lives[0] = Some(99),
            2 => life.result_life = Some(99),
            3 => {
                life.parameter_lives.pop();
            }
            _ => life.binders.clear(),
        }
        let errors =
            zil_types::check(&changed, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == "E-LIFE-CONTRACT"),
            "{errors:?}"
        );
    }
}

#[test]
fn reviewer322_scalar_reuse_must_recheck_current_raw_life_declarations() {
    let unit = source(
        "fn scalar<life a,life b>(p:i32)->i32{let n0=p+1;let n1=n0+1;let n2=n1+1;return n2}",
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let original = zil_ir::CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    let limits = Limits::default();
    let target = TargetProfile::default();
    let prepared = zil_types::infer_program_for_ct(&original, &limits)
        .unwrap()
        .into_prepared();
    let mut seeded = zil_types::FunctionSession::default();
    seeded
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    assert_eq!(seeded.last_stats().executed_functions, 1);
    assert_eq!(seeded.last_stats().reused_functions, 0);
    let encoded = seeded.encode_work_products(&target, &limits).unwrap();
    let valid = seeded
        .clone()
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    assert_eq!(
        valid,
        zil_types::check_program(&original, &target, &limits).unwrap()
    );
    let mut bypassed = Vec::new();
    for attack in 0..6 {
        let mut changed = original.clone();
        let life = changed.modules[0].core.functions[0].life.as_mut().unwrap();
        match attack {
            0 => life.binders.clear(),
            1 => life.binders[1] = life.binders[0].clone(),
            2 => life.parameter_lives.clear(),
            3 => life.parameter_lives[0] = Some(0),
            4 => life.result_life = Some(0),
            _ => life.result_life = Some(99),
        }
        let fresh = zil_types::check_program(&changed, &target, &limits).unwrap_err();
        assert!(
            fresh.iter().any(|d| d.code == "E-LIFE-CONTRACT"),
            "{fresh:?}"
        );
        for restored in [false, true] {
            let mut session = if restored {
                let mut session = zil_types::FunctionSession::default();
                assert!(session.restore_work_products(&encoded, &target, &limits));
                session
            } else {
                seeded.clone()
            };
            let prepared = zil_types::infer_program_for_ct(&changed, &limits)
                .unwrap()
                .into_prepared();
            match session.check_prepared_program(&prepared, &target, &limits) {
                Ok(_) => bypassed.push((attack, restored, session.last_stats())),
                Err(errors) => assert!(
                    errors.iter().any(|d| d.code == "E-LIFE-CONTRACT"),
                    "{errors:?}"
                ),
            }
        }
    }
    assert!(
        bypassed.is_empty(),
        "current life metadata bypassed on scalar reuse: {bypassed:?}"
    );
}

#[test]
fn reviewer322_equal_length_source_life_error_still_precedes_reuse() {
    let original =
        "fn scalar<life a,life b>(p:i32)->i32{let n0=p+1;let n1=n0+1;let n2=n1+1;return n2}";
    let changed = original.replace("life a,life b", "life a,life a");
    assert_eq!(original.len(), changed.len());
    let project = |text: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![source(text)],
        paths: std::collections::BTreeMap::new(),
    };
    let options = CompileOptions::default();
    let mut session = zil_driver::CompilationSession::default();
    let seed = session.compile_project(&project(original), &options);
    assert_eq!(seed.status, ResultCategory::Success, "{}", seed.to_json());
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
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, "E-LIFE-DECLARATION");
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn reviewer322_scalar_work_products_reject_a_typed_return_life_bound() {
    let unit = source("fn scalar(p:i32)->i32{let n0=p+1;let n1=n0+1;let n2=n1+1;return n2}");
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let original = zil_ir::CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    let limits = Limits::default();
    let target = TargetProfile::default();
    let prepared = zil_types::infer_program_for_ct(&original, &limits)
        .unwrap()
        .into_prepared();
    let mut seeded = zil_types::FunctionSession::default();
    let expected = seeded
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    let bytes = seeded.encode_work_products(&target, &limits).unwrap();
    assert_eq!(&bytes[..8], b"ZILFUN01");
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 1);
    let boundary = 12 + 80;
    let (module, candidate) =
        zil_ir::wire::decode_module_candidate(&bytes[boundary..], &limits).unwrap();
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].reference_return_bound, None);
    let mut restored = zil_types::FunctionSession::default();
    assert!(restored.restore_work_products(&bytes, &target, &limits));
    assert_eq!(
        restored
            .check_prepared_program(&prepared, &target, &limits)
            .unwrap(),
        expected
    );
    assert_eq!(restored.last_stats().reused_functions, 1);
    for bound in [vec![], vec![0], vec![99]] {
        let mut changed = module.clone();
        changed.functions[0].reference_return_bound = Some(bound);
        let changed_wire =
            zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
        let mut tampered = bytes[..boundary].to_vec();
        tampered.extend_from_slice(&changed_wire);
        let mut session = zil_types::FunctionSession::default();
        assert!(!session.restore_work_products(&tampered, &target, &limits));
        assert!(session.is_empty());
        assert_eq!(
            session
                .check_prepared_program(&prepared, &target, &limits)
                .unwrap(),
            expected
        );
        assert_eq!(session.last_stats().reused_functions, 0);
        assert_eq!(session.last_stats().executed_functions, 1);
    }
}

#[test]
fn explicit_life_scalar_reuse_keeps_valid_hits_ordered_keys_and_exact_budget() {
    let unit = source(
        "fn scalar<life a,life b>(p:i32)->i32{let n0=p+1;let n1=n0+1;let n2=n1+1;return n2}",
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let original = zil_ir::CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    };
    let target = TargetProfile::default();
    let run = |n, restored| -> Result<zil_ir::TypedProgram, Vec<zil_base::Diagnostic>> {
        let limits = Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let prepared = zil_types::infer_program_for_ct(&original, &limits)?.into_prepared();
        let expected = zil_types::check_prepared_program(&prepared, &target, &limits)?;
        let mut seeded = zil_types::FunctionSession::default();
        assert_eq!(
            seeded.check_prepared_program(&prepared, &target, &limits)?,
            expected
        );
        let mut warm = if restored {
            let bytes = seeded.encode_work_products(&target, &limits).unwrap();
            let mut warm = zil_types::FunctionSession::default();
            assert!(warm.restore_work_products(&bytes, &target, &limits));
            warm
        } else {
            seeded
        };
        let actual = warm.check_prepared_program(&prepared, &target, &limits)?;
        assert_eq!(actual, expected);
        assert_eq!(warm.last_stats().reused_functions, 1);
        assert_eq!(warm.last_stats().executed_functions, 0);
        Ok(actual)
    };
    for restored in [false, true] {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high, restored).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle, restored) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(
                        errors.iter().all(|d| d.phase == Phase::Types
                            && d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high, restored).unwrap(), expected);
        let errors = run(high - 1, restored).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.phase == Phase::Types && d.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        assert_eq!(run(high, restored).unwrap(), expected);
        println!(
            "322 valid scalar life reuse restored={restored} N={high}; fresh/live/restored agree"
        );
    }
    let limits = Limits::default();
    let prepared = zil_types::infer_program_for_ct(&original, &limits)
        .unwrap()
        .into_prepared();
    let mut seeded = zil_types::FunctionSession::default();
    seeded
        .check_prepared_program(&prepared, &target, &limits)
        .unwrap();
    let encoded = seeded.encode_work_products(&target, &limits).unwrap();
    for remove in [false, true] {
        let mut changed = original.clone();
        let function = &mut changed.modules[0].core.functions[0];
        if remove {
            function.life = None;
        } else {
            function.life.as_mut().unwrap().binders.swap(0, 1);
        }
        let expected = zil_types::check_program(&changed, &target, &limits).unwrap();
        let prepared = zil_types::infer_program_for_ct(&changed, &limits)
            .unwrap()
            .into_prepared();
        for restored in [false, true] {
            let mut warm = if restored {
                let mut warm = zil_types::FunctionSession::default();
                assert!(warm.restore_work_products(&encoded, &target, &limits));
                warm
            } else {
                seeded.clone()
            };
            assert_eq!(
                warm.check_prepared_program(&prepared, &target, &limits)
                    .unwrap(),
                expected
            );
            assert_eq!(warm.last_stats().reused_functions, 0);
            assert_eq!(warm.last_stats().executed_functions, 1);
        }
    }
}

#[test]
fn explicit_life_upper_bound_and_actual_origins_have_distinct_interface_identity() {
    let run = |head: &str, body: &str| {
        let project = zil_driver::ProjectSources {
            root: ModuleId(0),
            modules: vec![source(&format!(
                "pub fn selected<life a,life b>(left:&'a i32,right:&'{head} i32)->&'a i32{{return {body}}}fn result()->i32{{let x=1;let y=2;return *selected(&x,&y)}}"
            ))],
            paths: std::collections::BTreeMap::new(),
        };
        let report = zil_driver::compile_project(&project, &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        report.scalar_interfaces[0].clone()
    };
    let narrow = run("b", "left");
    let wide_left = run("a", "left");
    let wide_right = run("a", "right");
    assert_ne!(
        narrow.signature_fingerprint,
        wide_left.signature_fingerprint
    );
    assert_eq!(narrow.result_origins, wide_left.result_origins);
    assert_eq!(
        wide_left.signature_fingerprint,
        wide_right.signature_fingerprint
    );
    assert_ne!(wide_left.result_origins, wide_right.result_origins);
    assert_ne!(
        wide_left.contract_fingerprint,
        wide_right.contract_fingerprint
    );
}

#[test]
fn explicit_life_each_changed_semantic_phase_has_exact_fresh_budget() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(!errors.is_empty());
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{phase:?}: {errors:?}"
                    );
                    low = middle;
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
    let unit = source(LIFE_IDENTITY322);
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
        "322 original phase N: expand={expand} types={types} analyze={analyze} verify={verify}; N-1 ResourceLimit, fresh N Success"
    );
}

#[test]
fn incoming_pointer_and_exact_return_origin_use_native_abi() {
    let llvm = native(
        "fn first(a: &i64, b: &i64) -> &i64 { return a; } fn read(p: &i64) -> i64 { return *p; } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; let p = first(&x, &y); y = 99; let answer = read(p); x = 100; return answer + x + y; }",
        239,
    );
    assert!(llvm.contains("define ptr @zil.fn."), "{llvm}");
    assert!(llvm.contains("call ptr @zil.fn."), "{llvm}");
    assert!(!llvm.contains("malloc"), "references must not allocate");
}

#[test]
fn branch_union_alias_substitution_and_rebinding_preserve_last_use() {
    native(
        "fn choose(a: &i64, b: &i64, flag: bool) -> &i64 { return if flag { a } else { b }; } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; var p = choose(&x, &y, false); let first = *p; p = &y; x = 100; let second = *p; y = 200; let same = choose(&x, &x, true); return first + second + y + *same; }",
        304,
    );
}

#[test]
fn recursive_and_mutually_recursive_forwarding_find_grounded_origins() {
    native(
        "fn relay(a: &i64, b: &i64, n: i32) -> &i64 { if n == 0 { return b; } return again(a, b, n - 1); } fn again(a: &i64, b: &i64, n: i32) -> &i64 { return relay(a, b, n); } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; let p = relay(&x, &y, 8); x = 100; return *p + x; }",
        102,
    );
    native(
        "fn rotate(a: &i64, b: &i64, n: i32) -> &i64 { if n == 0 { return a; } return rotate(b, a, n - 1); } fn result() -> i64 { let x: i64 = 40; let y: i64 = 2; return *rotate(&x, &y, 7); }",
        2,
    );
}

#[test]
fn returned_pointer_composes_with_closure_and_inner_calls() {
    native(
        "fn choose(a: &i64, b: &i64) -> &i64 { return b; } fn read(p: &i64) -> i64 { return *p; } fn combine(a: &i64, b: &i64) -> i64 { let f = move () => { let p = choose(a, b); return read(p) + *a; }; return f(); } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; let p = choose(&x, &y); let snapshot: i64 = 3; let f = move () => read(p) + snapshot; let g = f; let total = f() + g() + combine(&x, &y); y = 100; return total + y; }",
        152,
    );
}

#[test]
fn bool_unit_and_i32_incoming_references_work() {
    native(
        "fn flag(p: &bool) -> &bool { return p; } fn unit(p: &()) -> &() { return p; } fn read(p: &i32) -> i32 { return *p; } fn result() -> i32 { let u = (); let yes = true; let value = 42; *unit(&u); if *flag(&yes) { return read(&value); } return 0; }",
        42,
    );
}

#[test]
fn every_returned_root_blocks_writes_until_last_use() {
    for text in [
        "fn id(a: &i32) -> &i32 { return a; } fn result() -> i32 { var x = 1; let p = id(&x); x = 2; return *p; }",
        "fn pick(a: &i32, b: &i32, flag: bool) -> &i32 { return if flag { a } else { b }; } fn result() -> i32 { var x = 1; var y = 2; let p = pick(&x, &y, true); y = 3; return *p; }",
        "fn id(a: &i32) -> &i32 { return a; } fn result() -> i32 { var x = 1; var p = &x; if true { let inner = 2; p = id(&inner); } return *p; }",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
    reject(
        "fn id(a: &i32) -> &i32 { return a; } fn result() -> i32 { var x = 1; let p = id(&x); let f = move () => *p; let g = f; f(); x = 2; return g(); }",
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn local_storage_and_value_parameters_cannot_escape() {
    for text in [
        "fn bad() -> &i32 { let x = 1; return &x; } fn result() -> i32 { return 0; }",
        "fn bad(x: i32) -> &i32 { return &x; } fn result() -> i32 { return 0; }",
        "fn id(a: &i32) -> &i32 { return a; } fn bad(a: &i32) -> &i32 { let x = 1; return id(&x); } fn result() -> i32 { return 0; }",
        "fn bad(a: &i32, flag: bool) -> &i32 { let x = 1; return if flag { a } else { &x }; } fn result() -> i32 { return 0; }",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-ESCAPE",
        );
    }
    reject(
        "fn loop_ref(a: &i32) -> &i32 { return loop_ref(a); } fn result() -> i32 { return 0; }",
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-ORIGIN",
    );
}

#[test]
fn forged_argument_roots_and_missing_return_holds_fail() {
    let module = typed(
        "fn pick(a: &i64, b: &i64, flag: bool) -> &i64 { return if flag { a } else { b }; } fn result() -> i64 { let x: i64 = 40; let y: i64 = 2; return *pick(&x, &y, false); }",
    );
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let call = module.functions[1]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap()
        .result
        .unwrap();
    assert_eq!(
        plan.functions[1].holds[call.0 as usize].roots,
        [HoldRoot::Local(LocalId(0)), HoldRoot::Local(LocalId(1))]
    );
    assert!(
        plan.functions[0]
            .holds
            .iter()
            .any(|h| h.roots == [HoldRoot::Parameter(0), HoldRoot::Parameter(1)])
    );
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    for mode in 0..3 {
        let mut altered = plan.clone();
        match mode {
            0 => altered.functions[1].holds[call.0 as usize].roots.clear(),
            1 => {
                altered.functions[1].holds[call.0 as usize].roots =
                    vec![HoldRoot::Local(LocalId(0))]
            }
            _ => {
                altered.functions[0]
                    .holds
                    .iter_mut()
                    .find(|h| !h.roots.is_empty())
                    .unwrap()
                    .roots = vec![HoldRoot::Parameter(99)]
            }
        }
        let error = zil_verify::verify_module(module.clone(), altered, &module.target, &limits)
            .unwrap_err();
        assert_eq!(error[0].code, "E-VERIFY-HOLD");
    }
}

#[test]
fn source_module_boundaries_preserve_pointer_signatures_and_return_roots() {
    let temp = Temp::new();
    let root = temp.0.join("main.zil");
    fs::write(
        &root,
        include_str!("../../../examples/call-borrows/main.zil"),
    )
    .unwrap();
    fs::write(
        temp.0.join("select.zil"),
        include_str!("../../../examples/call-borrows/select.zil"),
    )
    .unwrap();
    let report = zil_driver::compile_path(
        &root,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("answer".into()),
            output: Some(temp.0.join("program")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.source_files.len(), 2);
    execute(&report.artifacts["executable"], 305);
    let optimized = temp.0.join("optimized");
    let output = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    execute(&optimized, 305);
}

#[test]
fn low_budget_never_turns_recursive_unknown_origins_into_success() {
    let module = typed(
        "fn id(a: &i32, n: i32) -> &i32 { if n == 0 { return a; } return id(a, n - 1); } fn result() -> i32 { let x = 42; return *id(&x, 4); }",
    );
    let defaults = Limits::default();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    let limits = Limits {
        max_analysis_steps: 1,
        ..defaults
    };
    assert_eq!(
        zil_analyze::analyze(&module, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    assert_eq!(
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
}

#[test]
fn contract_identity_tracks_return_origin_changes_only_after_verify() {
    let temp = Temp::new();
    let root = temp.0.join("main.zil");
    let library = temp.0.join("library.zil");
    fs::write(
        &root,
        "import library; fn main() -> i32 { let x = 1; let y = 2; return *library.pick(&x, &y); }",
    )
    .unwrap();
    fs::write(
        &library,
        "pub fn pick(a: &i32, b: &i32) -> &i32 { return a; }",
    )
    .unwrap();
    let report = zil_driver::compile_path(
        &root,
        &CompileOptions {
            stop_after: Stage::Types,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .scalar_interfaces
            .iter()
            .all(|i| i.result_origins.is_none() && i.contract_fingerprint.is_none())
    );
    let checked = |directory: &str| {
        let report = zil_driver::compile_path(
            &root,
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(temp.0.join(directory)),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.artifacts.contains_key("verified-interfaces"));
        let interface = report
            .scalar_interfaces
            .iter()
            .find(|i| i.exports.iter().any(|f| f.name == "pick"))
            .unwrap();
        (
            interface.signature_fingerprint,
            interface.contract_fingerprint.unwrap(),
            interface.result_origins.clone().unwrap(),
        )
    };
    let before = checked("first");
    fs::write(
        &library,
        "pub fn pick(a: &i32, b: &i32) -> &i32 { return b; }",
    )
    .unwrap();
    let after = checked("second");
    assert_eq!(before.0, after.0);
    assert_ne!(before.1, after.1);
    assert_eq!(before.2[0].parameters, [0]);
    assert_eq!(after.2[0].parameters, [1]);
}
