//! Joint role claims are checked from actual CFGs, including coordinated attacks.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{
    CallableCleanup, CallableKind, CallableTraces, CandidatePlan, InstructionKind, TypedModule,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = include_str!("impl196_fixtures/10_nested_role_permutation.zil");
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "raw196-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn baseline(source: &str) -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw196.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    let dir = directory("baseline");
    fs::write(dir.join("source.zil"), source).unwrap();
    fs::write(dir.join("candidate.wire"), bytes).unwrap();
    (module, candidate)
}
fn reject(
    label: &str,
    module: TypedModule,
    mut candidate: CandidatePlan,
    analyze_reject: bool,
    code: &str,
) {
    candidate.module_fingerprint = module.fingerprint();
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let diagnostics = [
        zil_verify::verify_module(
            module.clone(),
            candidate.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err(),
    ];
    for result in &diagnostics {
        assert!(
            result.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == code),
            "{label}: {result:#?}"
        );
    }
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    if analyze_reject {
        assert!(
            analyzed.as_ref().is_err_and(|ds| ds
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject)),
            "{label}: {analyzed:#?}"
        );
    }
    let dir = directory(label);
    fs::write(dir.join("candidate.wire"), bytes).unwrap();
    fs::write(
        dir.join("diagnostics.txt"),
        format!("Verify raw/Wire: {diagnostics:#?}\nAnalyze: {analyzed:#?}\n"),
    )
    .unwrap();
}
fn outer(module: &TypedModule) -> usize {
    module
        .functions
        .iter()
        .position(|f| {
            f.callable_contract.as_ref().is_some_and(|c| {
                c.traces
                    .as_finite()
                    .is_some_and(|paths| paths.iter().any(|p| p.calls.len() == 4))
            })
        })
        .unwrap()
}
#[test]
fn role_deletion_duplication_and_non_environment_position_are_rejected() {
    let (module, candidate) = baseline(SOURCE);
    let index = outer(&module);
    for mutation in 0..3 {
        let mut changed = module.clone();
        let contract = changed.functions[index].callable_contract.as_mut().unwrap();
        match mutation {
            0 => {
                contract.roles.pop();
            }
            1 => {
                contract.roles[1] = contract.roles[0].clone();
            }
            _ => {
                contract.roles[1].parameter = 90;
            }
        }
        reject(
            &format!("role-shape-{mutation}"),
            changed,
            candidate.clone(),
            true,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}
#[test]
fn same_closure_id_does_not_authorize_event_role_swaps() {
    let (module, candidate) = baseline(SOURCE);
    let index = outer(&module);
    let mut changed = module.clone();
    let contract = changed.functions[index].callable_contract.as_mut().unwrap();
    assert_eq!(contract.roles[0].closure, contract.roles[1].closure);
    let path = &mut contract.traces.as_finite_mut().unwrap()[0];
    path.calls.swap(0, 1);
    reject(
        "same-id-event-swap",
        changed,
        candidate,
        true,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}
#[test]
fn separate_child_sites_do_not_share_one_role_substitution() {
    let (module, candidate) = baseline(SOURCE);
    let index = outer(&module);
    let mut changed = module.clone();
    let calls = &mut changed.functions[index]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
        .as_finite_mut()
        .unwrap()[0]
        .calls;
    calls[2].parameter = 0;
    calls[3].parameter = 1;
    reject(
        "child-site-map",
        changed,
        candidate,
        true,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}
#[test]
fn parent_and_child_coordinated_omission_is_rejected() {
    let (module, candidate) = baseline(SOURCE);
    let mut changed = module.clone();
    for function in &mut changed.functions {
        if let Some(contract) = &mut function.callable_contract {
            for path in contract.traces.as_finite_mut().unwrap() {
                path.calls.retain(|event| event.parameter == 0);
            }
        }
    }
    reject(
        "coordinated-second-omission",
        changed,
        candidate,
        true,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}
#[test]
fn second_role_mode_cleanup_and_writes_cannot_be_forged() {
    let source = "resource R{n:i32}fn pair<F:Fn()->i32,G:Fn()->i32>(a:&F,b:&G)->i32{return a()+b()}fn main()->i32{let origin=R{n:2};let edit=origin;let a=move()=>1;let b=()=>{edit.n+=1;return edit.n};return pair(&a,&b)-4}";
    let (module, candidate) = baseline(source);
    let index = module
        .functions
        .iter()
        .position(|f| f.callable_contract.is_some())
        .unwrap();
    assert!(
        !module.functions[index]
            .callable_contract
            .as_ref()
            .unwrap()
            .roles[1]
            .writes
            .is_empty()
    );
    for mutation in 0..3 {
        let mut changed = module.clone();
        let role = &mut changed.functions[index]
            .callable_contract
            .as_mut()
            .unwrap()
            .roles[1];
        match mutation {
            0 => role.kind = CallableKind::FnOnce,
            1 => role.cleanup = CallableCleanup::Owned,
            _ => role.writes.clear(),
        }
        reject(
            &format!("role-envelope-{mutation}"),
            changed,
            candidate.clone(),
            true,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}
#[test]
fn finite_paths_cannot_erase_branch_correlation() {
    let source = "fn make(n:i32)->impl Fn()->i32{return move()=>n}fn choose<F:Fn()->i32>(a:&F,b:&F,flag:bool)->i32{if flag{return a()}else{return b()}}fn main()->i32{let a=make(1);let b=make(2);return choose(&a,&b,true)}";
    let (module, candidate) = baseline(source);
    let mut changed = module.clone();
    let c = changed
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    let paths = c.traces.as_finite_mut().unwrap();
    let second = paths[1].calls[0];
    paths[0].calls.push(second);
    paths[1].calls.clear();
    paths.sort();
    reject(
        "branch-cartesian",
        changed,
        candidate,
        true,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}
#[test]
fn cyclic_second_role_event_and_exits_are_independently_rebuilt() {
    let source = "fn make(n:i32)->impl Fn()->i32{return move()=>n}fn run<F:Fn()->i32>(a:&F,b:&F,n:i32)->i32{var i=0;var total=0;while i<n{total+=a()+b();i+=1};return total}fn main()->i32{let a=make(1);let b=make(2);return run(&a,&b,2)-6}";
    let (module, candidate) = baseline(source);
    let mut changed = module.clone();
    let c = changed
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    let CallableTraces::Flow(flow) = &mut c.traces else {
        panic!("expected cycle")
    };
    for node in &mut flow.nodes {
        node.calls.retain(|event| event.parameter != 1)
    }
    reject(
        "cyclic-second-omission",
        changed,
        candidate,
        true,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}
#[test]
fn actual_duplicate_exclusive_arguments_do_not_gain_role_separation() {
    let source = include_str!("impl196_fixtures/04_same_f_mut_distinct.zil");
    let (module, candidate) = baseline(source);
    let helper = module
        .functions
        .iter()
        .find(|f| f.callable_contract.is_some())
        .unwrap()
        .id;
    let mut changed = module.clone();
    let arguments = changed
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            InstructionKind::Call {
                function,
                arguments,
            } if *function == helper => Some(arguments),
            _ => None,
        })
        .unwrap();
    arguments[1] = arguments[0];
    reject(
        "aliased-exclusive-actuals",
        changed,
        candidate,
        false,
        "E-BORROW",
    );
}
fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let mut high = Limits::default().max_analysis_steps;
    assert!(run(high).is_ok());
    let mut low = 0;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if run(mid).is_ok() {
            high = mid
        } else {
            low = mid
        }
    }
    assert!(run(high).is_ok());
    let rejected = run(high - 1).unwrap_err();
    assert!(
        rejected
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{rejected:#?}"
    );
    assert!(run(high).is_ok());
    high
}
#[test]
fn role_events_have_stage_local_work_limits_and_fresh_retry() {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget196.zil", SOURCE);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let types = boundary(|n| {
        zil_types::check(
            &core,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let (module, candidate) = baseline(SOURCE);
    let analyze = boundary(|n| {
        zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let verify = boundary(|n| {
        zil_verify::verify_module(
            module.clone(),
            candidate.clone(),
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let dir = directory("budgets");
    fs::write(dir.join("boundaries.json"),format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\"}}\n")).unwrap();
}
