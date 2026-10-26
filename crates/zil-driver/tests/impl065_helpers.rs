//! Source-to-independent-verifier coverage of concrete callable helpers.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, ProjectSources, Stage, compile_project, compile_source,
};

fn checked(text: &str) {
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "helpers065.zil", text),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
}

#[test]
fn helper065_shared_and_copy_arguments() {
    checked(
        "fn twice<F:Fn(i32)->i32>(f:&F,n:i32)->i32{return f(n)+f(n)} fn main()->i32{let x=20;let f=(n:i32)=>x+n;return twice(&f,1)-42}",
    );
}

#[test]
fn helper065_owned_environment_shared_borrow_and_reuse() {
    checked(
        "resource R{n:i32} fn twice<F:Fn()->i32>(f:&F)->i32{return f()+f()} fn main()->i32{let r=R{n:7};let f=move ()=>r.n;let n=twice(&f);return n+f()-21}",
    );
}

#[test]
fn helper065_mutable_borrow_updates_actual_environment() {
    checked(
        "fn twice<F:FnMut()->i32>(f:&mut F)->i32{return f()+f()} fn main()->i32{var n=0;var f=()=>{n+=1;return n};let first=twice(&mut f);return first+f()-6}",
    );
}

#[test]
fn helper065_value_condition_and_consume_adaptation() {
    checked(
        "resource R{n:i32} fn run<F:FnOnce()->i32>(f:F,yes:bool)->i32{if yes{return f()}return 0} fn main()->i32{let r=R{n:7};let f=move ()=>r.n;return run(f,true)-7}",
    );
}

#[test]
fn helper065_error_callback_stops_trace_and_late_argument_is_guarded() {
    checked(
        "resource R{n:i32} fn twice<F:Fn()->bool!i32>(f:&F)->bool!i32{let a=f()?;return a+f()?} fn run<F:FnOnce()->i32>(f:F,give r:R,n:i32)->i32{return f()+n} fn fail()->bool!i32{return true} fn main()->bool!i32{let r=R{n:7};let f=move ()=>r.n;return run(f,R{n:8},fail()?)}",
    );
}

fn project(main: &str, helpers: &str) -> CompileReport {
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(10),
            modules: vec![
                SourceUnit::new(FileId(20), ModuleId(10), "main.zil", main),
                SourceUnit::new(FileId(21), ModuleId(11), "helpers.zil", helpers),
            ],
            paths: std::collections::BTreeMap::from([(vec!["helpers".into()], ModuleId(11))]),
        },
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    report
}

#[test]
fn helper065_cross_module_interfaces_include_actual_environment_layout() {
    let helpers = "pub fn twice<F:Fn()->i32>(f:&F)->i32{return f()+f()} pub fn run<F:FnOnce()->i32>(f:F)->i32{return f()}";
    let first = project(
        "import helpers;fn main()->i32{let x=1;let f=()=>x;let n=helpers.twice(&f);return n+helpers.run(f)}",
        helpers,
    );
    let same_shape = project(
        "import helpers;fn main()->i32{let x=2;let f=()=>x;let n=helpers.twice(&f);return n+helpers.run(f)}",
        helpers,
    );
    let other_shape = project(
        "import helpers;fn main()->i32{let x=true;let f=()=>{if x{return 1}return 0};let n=helpers.twice(&f);return n+helpers.run(f)}",
        helpers,
    );
    let interface = |report: &CompileReport| {
        report
            .scalar_interfaces
            .iter()
            .find(|i| i.module == ModuleId(11))
            .unwrap()
            .clone()
    };
    let a = interface(&first);
    let b = interface(&same_shape);
    let c = interface(&other_shape);
    assert_eq!(a.exports.len(), 2);
    assert_eq!(a.callable_contracts.as_ref().unwrap().len(), 2);
    assert_eq!(a.signature_fingerprint, b.signature_fingerprint);
    assert_ne!(a.signature_fingerprint, c.signature_fingerprint);
    assert_eq!(a.implementation_fingerprint, c.implementation_fingerprint);
}

#[test]
fn helper065_same_name_in_distinct_modules_keeps_definition_identity() {
    let mut project = ProjectSources {
        root: ModuleId(10),
        modules: vec![
            SourceUnit::new(
                FileId(10),
                ModuleId(10),
                "main.zil",
                "import a;import b;fn local()->i32{return 99} fn main()->i32{let n=3;let f=()=>n;return a.run(&f)+b.run(&f)+a.run(&f)}",
            ),
            SourceUnit::new(
                FileId(20),
                ModuleId(20),
                "a.zil",
                "fn local()->i32{return 11} pub fn run<F:Fn()->i32>(f:&F)->i32{return f()+local()}",
            ),
            SourceUnit::new(
                FileId(30),
                ModuleId(30),
                "b.zil",
                "fn local()->i32{return 22} pub fn run<F:Fn()->i32>(f:&F)->i32{return f()+local()}",
            ),
        ],
        paths: std::collections::BTreeMap::from([
            (vec!["a".into()], ModuleId(20)),
            (vec!["b".into()], ModuleId(30)),
        ]),
    };
    for _ in 0..2 {
        let report = compile_project(&project, &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.verification_runs > 0);
        let interface = |module| {
            report
                .scalar_interfaces
                .iter()
                .find(|i| i.module == module)
                .unwrap()
        };
        let a = interface(ModuleId(20));
        let b = interface(ModuleId(30));
        assert_eq!(a.exports.len(), 1);
        assert_eq!(b.exports.len(), 1);
        assert_eq!(a.exports[0].name, "run::<env0>");
        assert_eq!(a.exports[0].name, b.exports[0].name);
        assert_ne!(a.exports[0].function, b.exports[0].function);
        assert_eq!(a.signature_fingerprint, b.signature_fingerprint);
        let a_contracts = a.callable_contracts.as_ref().unwrap();
        let b_contracts = b.callable_contracts.as_ref().unwrap();
        assert_eq!(a_contracts.len(), 1);
        assert_eq!(b_contracts.len(), 1);
        assert_eq!(a_contracts[0].name, a.exports[0].name);
        assert_eq!(b_contracts[0].name, b.exports[0].name);
        assert_eq!(a_contracts[0].contract, b_contracts[0].contract);
        project.modules.reverse();
    }
}

#[test]
fn helper065_generic_body_reads_known_resource_field() {
    checked(
        "resource R{n:i32} fn accept<F:FnOnce()->i32>(f:F,give item:R,code:i32)->i32{return f()+item.n+code} fn main()->i32{let owner=R{n:7};let f=move ()=>owner.n;let item=R{n:3};return accept(f,give item,2)-12}",
    );
}

#[test]
fn helper065_mutable_target_loan_ends_after_owned_zero_or_one_call() {
    for enabled in ["true", "false"] {
        checked(&format!(
            "fn run<F:FnOnce()->i32>(f:F,yes:bool)->i32{{if yes{{return f()}}return 0}} fn main()->i32{{var n=0;var f=()=>{{n+=1;return n}};let result=run(f,{enabled});n+=10;return n+result}}"
        ));
        checked(&format!(
            "fn run<F:FnOnce()->i32>(f:F,yes:bool)->i32{{if yes{{return f()}}return 0}} fn main()->i32{{var n=0;let pointer=&mut n;var f=()=>{{*pointer+=1;return *pointer}};let result=run(f,{enabled});n+=10;return n+result}}"
        ));
    }
}

#[test]
fn helper065_owned_fn_can_adapt_to_borrowed_fnmut_then_be_reused() {
    checked(
        "resource R{n:i32} fn twice<F:FnMut()->i32>(f:&mut F)->i32{return f()+f()} fn main()->i32{let owner=R{n:7};var f=move ()=>owner.n;let first=twice(&mut f);return first+f()-21}",
    );
}

#[test]
fn helper065_copy_error_callbacks_are_instantiated_and_verified() {
    checked(
        "fn twice<F:Fn()->bool!i32>(f:&F)->bool!i32{let first=f()?;return first+f()?} fn main()->bool!i32{let yes=true;let f=()->bool!i32=>{if yes{return true}return 1};return twice(&f)}",
    );
    checked(
        "fn twice<F:FnMut()->bool!i32>(f:&mut F)->bool!i32{let first=f()?;return first+f()?} fn main()->bool!i32{var n=0;var f=()->bool!i32=>{n+=1;if n==2{return true}return n};return twice(&mut f)}",
    );
}

fn rejected(text: &str, category: ResultCategory, phase: Phase, message: &str) {
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "negative065.zil", text),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(report.status, category, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == phase && d.message.contains(message)),
        "{}",
        report.to_json()
    );
}

#[test]
fn helper065_nearest_callable_admission_rejections() {
    rejected(
        "fn run<F>(f:F)->i32{return f()} fn main()->i32{return 0}",
        ResultCategory::Unsupported,
        Phase::Expand,
        "bound",
    );
    rejected(
        "fn run<F:Fn()->i32>(f:&F)->i32{return f.unknown} fn main()->i32{return 0}",
        ResultCategory::Unsupported,
        Phase::Expand,
        "bound",
    );
    rejected(
        "fn run<F:FnOnce()->i32>(f:F)->i32{return f()+f()} fn main()->i32{return 0}",
        ResultCategory::SafeReject,
        Phase::Expand,
        "consumed",
    );
    rejected(
        "fn run<F:FnMut()->i32>(f:&F)->i32{return f()} fn main()->i32{return 0}",
        ResultCategory::SafeReject,
        Phase::Expand,
        "mutable",
    );
    rejected(
        "fn run<F:Fn()->i32>(f:&F)->i32{return f()} fn main()->i32{var n=0;var f=()=>{n+=1;return n};return run(&f)}",
        ResultCategory::SafeReject,
        Phase::Types,
        "callable mode",
    );
}

#[test]
fn helper065_helper_does_not_extend_resource_or_address_lifetimes() {
    rejected(
        "resource R{n:i32} fn run<F:FnOnce()->i32>(f:F)->i32{return f()} fn main()->i32{let owner=R{n:7};let f=move ()=>owner.n;let first=run(f);return first+f()}",
        ResultCategory::SafeReject,
        Phase::Analyze,
        "initialized",
    );
    rejected(
        "fn run<F:Fn()->i32>(f:&F)->i32{return f()} fn main()->i32{var n=0;var f=()=>{n+=1;return n};n=10;return run(&f)}",
        ResultCategory::SafeReject,
        Phase::Types,
        "callable mode",
    );
    rejected(
        "fn run<F:FnMut()->i32>(f:&mut F)->i32{return f()} fn main()->i32{var n=0;var f=()=>{n+=1;return n};n=10;return run(&mut f)}",
        ResultCategory::SafeReject,
        Phase::Verify,
        "address still required",
    );
}

#[test]
fn helper065_scope_is_explicit_for_extra_borrows_and_unbounded_traces() {
    rejected(
        "fn run<F:Fn()->i32>(f:&F,other:&i32)->i32{return f()} fn main()->i32{let n=7;let f=()=>n;return run(&f,&n)}",
        ResultCategory::Unsupported,
        Phase::Types,
        "finite Copy",
    );
    rejected(
        "fn run<F:FnMut()->i32>(f:F)->i32{return 0} fn main()->i32{return 0}",
        ResultCategory::Unsupported,
        Phase::Expand,
        "explicit mutable binding",
    );
    checked(
        "fn thrice<F:Fn()->i32>(f:&F)->i32{return f()+f()+f()} fn main()->i32{let n=7;let f=()=>n;return thrice(&f)}",
    );
}

fn core(text: &str) -> zil_ir::CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw065.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}

#[test]
fn helper065_instances_are_cached_and_new_identities_are_bounded() {
    let input = core(
        "fn run<F:Fn()->i32>(f:&F)->i32{return f()} fn main()->i32{let x=7;let f=()=>x;return run(&f)+run(&f)+run(&f)}",
    );
    let limits = Limits {
        max_functions: 3,
        ..Limits::default()
    };
    let typed = zil_types::check(&input, &TargetProfile::default(), &limits).unwrap();
    assert_eq!(typed.functions.len(), 3);
    assert_eq!(
        typed
            .functions
            .iter()
            .filter(|f| f.callable_contract.is_some())
            .count(),
        1
    );
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, plan, &TargetProfile::default(), &limits).unwrap();

    let input = core(
        "fn run<F:Fn()->i32>(f:&F)->i32{return f()} fn main()->i32{let x=7;let f=()=>x;let y=8;let g=()=>y;return run(&f)+run(&g)}",
    );
    let limits = Limits {
        max_functions: 4,
        ..Limits::default()
    };
    let errors = zil_types::check(&input, &TargetProfile::default(), &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == ResultCategory::ResourceLimit
                && e.phase == Phase::Types
                && e.message.contains("generated function budget")),
        "{errors:?}"
    );
}

#[test]
fn helper065_raw_callable_metadata_is_bounded_before_instantiation() {
    let mut input = core("fn run<F:Fn()->i32>(f:&F)->i32{return f()} fn main()->i32{return 0}");
    let mut nested = zil_ir::Type::I32;
    for _ in 0..32 {
        nested = zil_ir::Type::Tuple(vec![nested]);
    }
    input.functions[0]
        .callable_parameter
        .as_mut()
        .unwrap()
        .result = nested;
    let limits = Limits {
        max_call_depth: 8,
        ..Limits::default()
    };
    let errors = zil_types::check(&input, &TargetProfile::default(), &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == ResultCategory::ResourceLimit
                && e.phase == Phase::Types
                && e.message.contains("depth")),
        "{errors:?}"
    );
}

#[test]
fn helper065_analyze_reconstructs_hold_and_call_paths() {
    let input = core(
        "fn run<F:FnMut()->i32>(f:&mut F)->i32{return f()+f()} fn main()->i32{var n=0;var f=()=>{n+=1;return n};return run(&mut f)}",
    );
    let limits = Limits::default();
    let original = zil_types::check(&input, &TargetProfile::default(), &limits).unwrap();
    zil_analyze::analyze(&original, &limits).unwrap();
    for erase_hold in [true, false] {
        let mut changed = original.clone();
        let claim = changed
            .functions
            .iter_mut()
            .find_map(|f| f.callable_contract.as_mut())
            .unwrap();
        if erase_hold {
            assert!(!claim.hold.is_empty());
            claim.hold.clear();
        } else {
            assert!(!claim.traces.is_empty());
            claim.traces.clear();
        }
        let errors = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.phase == Phase::Analyze
                && e.category == ResultCategory::SafeReject
                && e.message.contains(if erase_hold {
                    "envelope"
                } else {
                    "trace claim"
                })),
            "{errors:?}"
        );
    }
}
