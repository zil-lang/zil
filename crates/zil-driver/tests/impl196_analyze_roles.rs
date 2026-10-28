//! Production source -> Types -> independent Analyze/Verify and raw/Wire attacks.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{
    CallEvent, CallableExit, CallableKind, CallableTrace, CallableTraces, CandidatePlan,
    TypedModule,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const NESTED: &str = r#"
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn leaf<F:Fn()->i32>(a:&F,b:&F)->i32{return a()*10+b()}
fn parent<F:Fn()->i32>(a:&F,b:&F)->i32{return leaf(a,b)+leaf(b,a)}
fn main()->i32{let a=make(2);let b=make(7);return parent(&a,&b)-99}
"#;

fn save(
    label: &str,
    source: Option<&str>,
    module: &TypedModule,
    plan: &CandidatePlan,
    extra: &str,
) {
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir = PathBuf::from(root).join(format!(
            "analyze196-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        if let Some(source) = source {
            fs::write(dir.join("source.zil"), source).unwrap();
        }
        fs::write(dir.join("module.txt"), format!("{module:#?}")).unwrap();
        fs::write(dir.join("candidate.txt"), format!("{plan:#?}")).unwrap();
        fs::write(dir.join("result.txt"), extra).unwrap();
        fs::write(
            dir.join("candidate.wire"),
            zil_ir::wire::encode_module_candidate(module, plan, &Limits::default()).unwrap(),
        )
        .unwrap();
    }
}
fn baseline(label: &str, text: &str) -> (TypedModule, CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "analyze196.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    save(
        label,
        Some(text),
        &module,
        &plan,
        "Actual source, Analyze, raw Verify and Wire Verify accepted; native not run by this test.",
    );
    (module, plan)
}
fn reject(label: &str, module: TypedModule, mut plan: CandidatePlan, analyze_message: &str) {
    let analysis = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        analysis.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::SafeReject
            && d.message.contains(analyze_message)),
        "{label}: {analysis:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap_err();
    let wire =
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err();
    for errors in [&raw, &wire] {
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject),
            "{label}: {errors:?}"
        );
    }
    save(
        label,
        None,
        &module,
        &plan,
        &format!("Analyze: {analysis:#?}\nVerify: {raw:#?}\nWire: {wire:#?}\n"),
    );
}

#[test]
fn actual_nested_same_type_instances_keep_both_static_role_mappings() {
    let (module, plan) = baseline("nested", NESTED);
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        match zil_analyze::analyze(&module, &limits) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert!(
                    errors.iter().any(|d| d.phase == Phase::Analyze
                        && d.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = middle + 1;
            }
        }
    }
    assert!(low > 0);
    let at = Limits {
        max_analysis_steps: low,
        ..Limits::default()
    };
    let below = Limits {
        max_analysis_steps: low - 1,
        ..Limits::default()
    };
    assert!(zil_analyze::analyze(&module, &at).is_ok());
    assert!(
        zil_analyze::analyze(&module, &below)
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    assert!(zil_analyze::analyze(&module, &at).is_ok());
    println!(
        "source-derived Analyze196 joint child frontier: {low}; N-1 rejected and fresh N accepted"
    );
    let parent = module.functions.iter().position(|function| function.callable_contract.as_ref().is_some_and(|c|
        matches!(&c.traces, CallableTraces::Finite(paths) if paths.len()==1 && paths[0].calls.len()==4))).unwrap();
    let contract = module.functions[parent].callable_contract.as_ref().unwrap();
    assert_eq!(contract.roles.len(), 2);
    assert_eq!(contract.roles[0].closure, contract.roles[1].closure);
    let CallableTraces::Finite(paths) = &contract.traces else {
        unreachable!()
    };
    assert_eq!(
        paths[0]
            .calls
            .iter()
            .map(|event| event.parameter)
            .collect::<Vec<_>>(),
        [0, 1, 1, 0]
    );
    let mut wrong = module.clone();
    let CallableTraces::Finite(paths) = &mut wrong.functions[parent]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
    else {
        unreachable!()
    };
    paths[0].calls.swap(2, 3);
    reject(
        "reuse-first-child-mapping",
        wrong,
        plan,
        "trace claim differs",
    );
}

#[test]
fn actual_branch_paths_cannot_be_replaced_with_independent_role_counts() {
    let text = r#"fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn choose<F:Fn()->i32>(a:&F,b:&F,flag:bool)->i32{if flag{return a()}else{return b()}}
fn main()->i32{let a=make(2);let b=make(7);return choose(&a,&b,true)-2}"#;
    let (mut module, plan) = baseline("branch", text);
    let c = module
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    assert_eq!(
        c.traces,
        CallableTraces::Finite(vec![
            CallableTrace {
                calls: vec![CallEvent {
                    parameter: 0,
                    kind: CallableKind::Fn
                }],
                exit: CallableExit::Return
            },
            CallableTrace {
                calls: vec![CallEvent {
                    parameter: 1,
                    kind: CallableKind::Fn
                }],
                exit: CallableExit::Return
            },
        ])
    );
    c.traces = CallableTraces::Finite(vec![
        CallableTrace {
            calls: vec![],
            exit: CallableExit::Return,
        },
        CallableTrace {
            calls: vec![
                CallEvent {
                    parameter: 0,
                    kind: CallableKind::Fn,
                },
                CallEvent {
                    parameter: 1,
                    kind: CallableKind::Fn,
                },
            ],
            exit: CallableExit::Return,
        },
    ]);
    reject(
        "branch-cartesian-product",
        module,
        plan,
        "trace claim differs",
    );
}

#[test]
fn actual_loop_control_preserves_roles_in_the_flow_contract() {
    let text = r#"fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn run<F:Fn()->i32>(a:&F,b:&F,n:i32)->i32{var i=0;var total=0;while i<n{total+=a();if i==1{i+=1;continue}total+=b();if i==3{break}i+=1}return total}
fn main()->i32{let a=make(1);let b=make(2);return run(&a,&b,3)-7}"#;
    let (mut module, plan) = baseline("flow", text);
    let c = module
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    let CallableTraces::Flow(flow) = &mut c.traces else {
        panic!("runtime loop must remain Flow")
    };
    let event = flow
        .nodes
        .iter_mut()
        .flat_map(|node| &mut node.calls)
        .find(|event| event.parameter == 1)
        .unwrap();
    event.parameter = 0;
    reject(
        "second-flow-role-erased",
        module,
        plan,
        "omits or changes calls",
    );
}

#[test]
fn two_owned_once_roles_have_separate_finite_consumption() {
    let text = r#"fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn pair<F:FnOnce()->i32>(left:F,right:F)->i32{return left()+right()}
fn main()->i32{let left=make(1);let right=make(2);return pair(left,right)-3}"#;
    let (mut module, plan) = baseline("once", text);
    let c = module
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    assert_eq!(c.roles.len(), 2);
    assert!(c.roles.iter().all(|role| role.kind == CallableKind::FnOnce));
    let CallableTraces::Finite(paths) = &mut c.traces else {
        unreachable!()
    };
    paths[0].calls[1].parameter = paths[0].calls[0].parameter;
    reject("once-role-collapsed", module, plan, "trace claim differs");
}

#[test]
fn second_role_write_is_projected_into_the_actual_owned_environment() {
    let text = r#"resource R{n:i32}
fn only_second<F:Fn()->i32,G:Fn()->i32>(first:&F,second:&G)->i32{return second()}
fn make()->impl FnMut()->i32{let owner=R{n:2};return move()=>{let edit=owner;let first=move()=>1;let second=()=>{edit.n+=1;return edit.n};return only_second(&first,&second)}}
fn main()->i32{var f=make();return f()-3}"#;
    let (mut module, mut plan) = baseline("second-write", text);
    let outer = module
        .closures
        .iter()
        .find(|c| {
            c.kind == CallableKind::FnMut
                && c.captures
                    .iter()
                    .any(|ty| matches!(ty, zil_ir::Type::Managed(_)))
        })
        .unwrap()
        .id;
    module.closures[outer.0 as usize].kind = CallableKind::Fn;
    for function in &mut module.functions {
        if let Some(contract) = &mut function.callable_return_contract
            && contract.closure == outer
        {
            contract.kind = CallableKind::Fn;
        }
    }
    plan.module_fingerprint = module.fingerprint();
    reject(
        "second-role-owned-write",
        module,
        plan,
        "shared owned capture is written",
    );
}
