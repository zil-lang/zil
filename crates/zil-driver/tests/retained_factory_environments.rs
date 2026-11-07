//! Real source factory environments, public contracts, and nearest boundaries.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, ProjectSources, Stage, compile_project, compile_source,
};
use zil_ir::{CallableKind, InstructionKind, Type};

fn source(text: &str, stage: Stage) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "retained069.zil", text),
        &CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        },
    )
}
fn checked(text: &str) -> CompileReport {
    let report = source(text, Stage::Verify);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    report
}
fn rejected(text: &str, phase: Phase, code: &str) {
    let report = source(text, Stage::Verify);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "unexpected success: {text}"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == phase && d.code == code),
        "{}",
        report.to_json()
    );
}

#[test]
fn retained069_copy_snapshot_factory_before_and_after_caller() {
    let factory = "fn make(n:i32)->impl Fn(i32)->i32{return move (x)=>n+x}";
    let caller = "fn main()->i32{let f=make(20);let first=f(1);return first+f(1)-42}";
    checked(&format!("{factory} {caller}"));
    checked(&format!("{caller} {factory}"));
}

#[test]
fn retained069_owned_fn_zero_multiple_and_move_calls() {
    checked(
        "resource R{n:i32} fn make(give r:R)->impl Fn()->i32{return move ()=>r.n} fn main()->i32{let unused=make(R{n:1});let f=make(R{n:21});let n=f();let g=f;return n+g()-42}",
    );
}

#[test]
fn retained069_fnmut_survives_an_event_loop() {
    checked(
        "fn make(n:i32)->impl FnMut()->i32{var count=n;return move ()=>{count+=1;return count}} fn main()->i32{var f=make(0);var events=0;var sum=0;while events<3{sum+=f();events+=1}return sum-6}",
    );
}

#[test]
fn retained069_owned_fnmut_updates_retained_resource() {
    checked(
        "resource R{n:i32} fn make(n:i32)->impl FnMut()->i32{let r=R{n:n};return move ()=>{r.n+=1;return r.n}} fn main()->i32{var f=make(19);let first=f();return first+f()-41}",
    );
}

#[test]
fn retained069_owned_fnonce_consumes_once() {
    checked(
        "resource R{n:i32} fn consume(give r:R)->i32{return r.n} fn make(n:i32)->impl FnOnce()->i32{let r=R{n:n};return move ()=>consume(give r)} fn main()->i32{let f=make(42);return f()-42}",
    );
}

#[test]
fn retained069_public_factory_result_can_enter_a_public_helper() {
    checked(
        "fn twice<F:Fn(i32)->i32>(f:&F,n:i32)->i32{return f(n)+f(n)} fn main()->i32{let f=make(20);return twice(&f,1)-42} fn make(n:i32)->impl Fn(i32)->i32{return move (n2)=>n+n2}",
    );
}

#[test]
fn retained069_factory_result_and_local_closure_share_source_inference() {
    checked(
        "fn make(n:i32)->impl Fn(i32)->i32{return move (x)=>n+x} fn main()->i32{let f=make(20);let local=(x)=>x+1;return f(local(20))-41}",
    );
}

#[test]
fn retained069_return_same_local_on_multiple_paths_and_unrelated_closure() {
    checked(
        "fn make(n:i32,yes:bool)->impl Fn(i32)->i32{let local=(x:i32)=>x+1;let ignored=local(0);let f=move (x:i32)=>x+n;if yes{return f}return f} fn main()->i32{let f=make(41,true);return f(1)-42}",
    );
}

#[test]
fn retained069_returned_error_callback_and_early_cleanup() {
    checked(
        "resource R{n:i32} fn make(n:i32)->impl Fn()->bool!i32{let r=R{n:n};return move ()=>{if r.n<0{return true}return r.n}} fn main()->bool!i32{let f=make(21);let first=f()?;return first+f()?-42}",
    );
}

fn project(main: &str, factory: &str) -> CompileReport {
    compile_project(
        &ProjectSources {
            root: ModuleId(10),
            modules: vec![
                SourceUnit::new(FileId(10), ModuleId(10), "main.zil", main),
                SourceUnit::new(FileId(20), ModuleId(20), "factory.zil", factory),
            ],
            paths: std::collections::BTreeMap::from([(vec!["factory".into()], ModuleId(20))]),
        },
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    )
}

#[test]
fn retained069_cross_module_factory_publishes_actual_return_contract() {
    let report = project(
        "import factory;fn main()->i32{let f=factory.make(21);return f()+f()-42}",
        "resource R{n:i32} pub fn make(n:i32)->impl Fn()->i32{let r=R{n:n};return move ()=>r.n}",
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let interface = report
        .scalar_interfaces
        .iter()
        .find(|i| i.module == ModuleId(20))
        .unwrap();
    assert_eq!(interface.exports.len(), 1);
    assert!(matches!(interface.exports[0].result, Type::Closure(_)));
    let contracts = interface
        .callable_returns
        .as_ref()
        .expect("Verify publishes factory contracts");
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].name, "make");
    assert_eq!(contracts[0].contract.kind, CallableKind::Fn);
    assert_eq!(contracts[0].contract.cleanup, vec![0]);
    assert!(contracts[0].contract.hold.is_empty());
    assert!(contracts[0].contract.call.is_empty());
    assert!(contracts[0].contract.writes.is_empty());
}

#[test]
fn retained069_nearest_source_boundaries_fail_closed() {
    checked(
        "fn inner()->impl Fn()->i32{return move ()=>1} fn make()->impl Fn()->i32{return inner()} fn main()->i32{return 0}",
    );
    for text in [
        "fn make(n:i32)->impl Fn()->i32{return ()=>n} fn main()->i32{return 0}",
        "resource R{n:i32} fn make()->impl Fn()->i32{let r=R{n:1};return ()=>r.n} fn main()->i32{return 0}",
        "fn make()->impl FnOnce()->i32{return move ()=>1} fn main()->i32{return 0}",
        "fn make()->impl Fn()->i32{return make()} fn main()->i32{return 0}",
        "fn inner()->impl Fn()->i32{return make()} fn make()->impl Fn()->i32{return inner()} fn main()->i32{return 0}",
    ] {
        rejected(text, Phase::Types, "E-UNSUPPORTED");
    }
    rejected(
        "fn make(yes:bool)->impl Fn()->i32{if yes{return move ()=>1}return move ()=>2} fn main()->i32{return 0}",
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
}

#[test]
fn retained069_managed_use_after_move_and_once_reuse_rejected() {
    for text in [
        "resource R{n:i32} fn make()->impl Fn()->i32{let r=R{n:1};return move ()=>r.n} fn main()->i32{let f=make();let g=f;return f()+g()}",
        "resource R{n:i32} fn consume(give r:R)->i32{return r.n} fn make()->impl FnOnce()->i32{let r=R{n:1};return move ()=>consume(give r)} fn main()->i32{let f=make();return f()+f()}",
    ] {
        let report = source(text, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Analyze),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn retained069_hidden_fields_do_not_become_caller_projection_rights() {
    let report = project(
        "import factory;fn main()->i32{let f=factory.make(1);return f.n}",
        "resource R{n:i32} pub fn make(n:i32)->impl Fn()->i32{let r=R{n:n};return move ()=>r.n}",
    );
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.phase == Phase::Types),
        "{}",
        report.to_json()
    );
}

#[test]
fn retained069_new_raw_metadata_is_bounded_before_cloning() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "budget069.zil",
        "fn make()->impl Fn()->i32{return move ()=>1}",
    );
    let limits = Limits::default();
    let ast = zil_syntax::parse_with_limits(&unit, &limits)
        .into_result()
        .unwrap();
    let mut core = zil_expand::expand(&unit, &ast).unwrap();
    core.functions[0]
        .callable_return
        .as_mut()
        .unwrap()
        .parameters = vec![Type::I32; 128];
    let low = Limits {
        max_analysis_steps: 64,
        ..limits
    };
    let errors = zil_types::check(&core, &zil_base::TargetProfile::default(), &low).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-RESOURCE-LIMIT"),
        "{errors:?}"
    );
}

#[test]
fn retained069_return_is_a_direct_call_with_environment_not_body_substitution() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "shape069.zil",
        "fn main()->i32{let f=make(40);return f(2)-42} fn make(n:i32)->impl Fn(i32)->i32{return move (x)=>n+x}",
    );
    let limits = Limits::default();
    let ast = zil_syntax::parse_with_limits(&unit, &limits)
        .into_result()
        .unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let typed = zil_types::check(&core, &zil_base::TargetProfile::default(), &limits).unwrap();
    let main = typed.functions.iter().find(|f| f.name == "main").unwrap();
    let factory = typed.functions.iter().find(|f| f.name == "make").unwrap();
    assert!(factory.callable_return_contract.is_some());
    assert!(main.blocks.iter().flat_map(|b| &b.instructions).any(
        |i| matches!(i.kind, InstructionKind::Call { function, .. } if function == factory.id)
    ));
    assert!(
        !main
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
    );
    zil_analyze::analyze(&typed, &limits).unwrap();
}

#[test]
fn retained069_analyze_reconstructs_cleanup_and_rejects_missing_contract() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "contract069.zil",
        "resource R{n:i32} fn make()->impl Fn()->i32{let r=R{n:21};return move ()=>r.n}",
    );
    let limits = Limits::default();
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let typed = zil_types::check(&core, &zil_base::TargetProfile::default(), &limits).unwrap();
    zil_analyze::analyze(&typed, &limits).unwrap();
    let mut missing = typed.clone();
    missing.functions[0].callable_return_contract = None;
    let errors = zil_analyze::analyze(&missing, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "E-ANALYZE-IR" && e.message.contains("no public"))
    );
    let mut forged = typed.clone();
    forged.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .clear();
    let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "E-ANALYZE-IR" && e.message.contains("cleanup"))
    );
    let mut corrupt = typed;
    corrupt.functions[0].blocks[0].instructions[0].result = Some(zil_ir::ValueId(u32::MAX));
    assert!(zil_analyze::analyze(&corrupt, &limits).is_err());
}

#[test]
fn retained069_cross_module_same_name_factories_and_module_order() {
    let mut inputs = ProjectSources {
        root: ModuleId(10),
        modules: vec![
            SourceUnit::new(
                FileId(10),
                ModuleId(10),
                "main.zil",
                "import a;import b;fn main()->i32{let first=a.make(20);let second=b.make(true);return first()+second()-42}",
            ),
            SourceUnit::new(
                FileId(20),
                ModuleId(20),
                "a.zil",
                "pub fn make(n:i32)->impl Fn()->i32{return move ()=>n}",
            ),
            SourceUnit::new(
                FileId(30),
                ModuleId(30),
                "b.zil",
                "pub fn make(yes:bool)->impl Fn()->i32{return move ()=>{if yes{return 22}return 0}}",
            ),
        ],
        paths: std::collections::BTreeMap::from([
            (vec!["a".into()], ModuleId(20)),
            (vec!["b".into()], ModuleId(30)),
        ]),
    };
    for _ in 0..2 {
        let report = compile_project(
            &inputs,
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
        let contracts: Vec<_> = report
            .scalar_interfaces
            .iter()
            .filter(|i| i.module != ModuleId(10))
            .map(|i| &i.callable_returns.as_ref().unwrap()[0])
            .collect();
        assert_eq!(contracts.len(), 2);
        assert_eq!(contracts[0].name, "make");
        assert_eq!(contracts[1].name, "make");
        assert_ne!(contracts[0].contract.closure, contracts[1].contract.closure);
        inputs.modules.reverse();
    }
}

#[test]
fn retained069_mixed_capture_cleanup_indices_and_owned_return_paths() {
    let report = project(
        "import factory;fn main()->i32{let f=factory.make(true);return f()-42}",
        "resource R{n:i32} pub fn make(yes:bool)->impl Fn()->i32{let first=R{n:20};let extra=2;let last=R{n:20};let f=move ()=>first.n+extra+last.n;if yes{return f}return f}",
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let interface = report
        .scalar_interfaces
        .iter()
        .find(|i| i.module == ModuleId(20))
        .unwrap();
    let contract = &interface.callable_returns.as_ref().unwrap()[0].contract;
    assert_eq!(contract.cleanup, vec![0, 2]);
}
