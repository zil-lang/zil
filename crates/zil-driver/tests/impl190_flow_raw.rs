//! A cyclic public call graph is a claim, never permission. Mutations below
//! retain the original independently derived candidate and only rebind its
//! untrusted identity; Analyze is never used to legitimize a forged claim.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{
    CallableExit, CallableFlow, CallableFlowNext, CallableFlowNode, CallableKind, CallableTrace,
    CallableTraces, CandidatePlan, TypedModule,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = r#"
fn run<F:Fn()->i32>(f:&F,n:i32)->i32 {
    var left=n;var total=0;
    while left>0 { total+=f();left-=1 }
    return total
}
fn main()->i32{let f=move()=>7;return run(&f,3)-21}
"#;

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "raw190-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn baseline(source: &str) -> (TypedModule, CandidatePlan) {
    let dir = directory("baseline");
    fs::write(dir.join("source.zil"), source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "flow190.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
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
    fs::write(dir.join("candidate.wire"), bytes).unwrap();
    (module, candidate)
}

fn flow(module: &mut TypedModule) -> &mut CallableFlow {
    module
        .functions
        .iter_mut()
        .find_map(
            |f| match f.callable_contract.as_mut().map(|c| &mut c.traces) {
                Some(CallableTraces::Flow(flow)) => Some(flow),
                _ => None,
            },
        )
        .unwrap()
}

fn reject(
    label: &str,
    module: TypedModule,
    mut candidate: CandidatePlan,
    category: ResultCategory,
    code: &str,
) {
    candidate.module_fingerprint = module.fingerprint();
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let actual = [
        zil_verify::verify_module(
            module.clone(),
            candidate,
            &module.target,
            &Limits::default(),
        )
        .unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err(),
    ];
    let dir = directory(label);
    fs::write(dir.join("candidate.wire"), bytes).unwrap();
    fs::write(dir.join("diagnostics.txt"), format!("{actual:#?}")).unwrap();
    for diagnostics in actual {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == category && d.code == code),
            "{label}: {diagnostics:?}"
        );
    }
    let diagnostics = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.category == category),
        "{label} Analyze: {diagnostics:?}"
    );
}

#[test]
fn runtime_helper_has_an_explicit_verified_flow_and_wire_identity() {
    let (mut module, _) = baseline(SOURCE);
    let graph = flow(&mut module);
    assert_eq!(graph.entry, 0);
    assert!(graph.nodes.iter().any(|n| !n.calls.is_empty()));
    assert!(graph.nodes.iter().enumerate().any(|(index, n)| matches!(&n.next, CallableFlowNext::Edges(edges) if edges.iter().any(|next| *next as usize <= index))));
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.next == CallableFlowNext::Exit(CallableExit::Return))
    );
}

#[test]
fn raw_and_wire_reject_omitted_added_or_changed_callback_events() {
    let (module, candidate) = baseline(SOURCE);
    for mutation in 0..3 {
        let mut changed = module.clone();
        let graph = flow(&mut changed);
        match mutation {
            0 => graph
                .nodes
                .iter_mut()
                .find(|n| !n.calls.is_empty())
                .unwrap()
                .calls
                .clear(),
            1 => graph.nodes[0].calls.push(CallableKind::Fn),
            _ => {
                graph
                    .nodes
                    .iter_mut()
                    .find(|n| !n.calls.is_empty())
                    .unwrap()
                    .calls[0] = CallableKind::FnMut
            }
        }
        assert_ne!(module.fingerprint(), changed.fingerprint());
        reject(
            &format!("events-{mutation}"),
            changed,
            candidate.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}

#[test]
fn raw_and_wire_reject_missing_backedges_changed_exits_and_noncanonical_numbering() {
    let (module, candidate) = baseline(SOURCE);
    for mutation in 0..10 {
        let mut changed = module.clone();
        let graph = flow(&mut changed);
        match mutation {
            0 => graph.entry = 1,
            1 => graph.nodes.clear(),
            2 => graph.nodes.push(CallableFlowNode {
                calls: Vec::new(),
                next: CallableFlowNext::Exit(CallableExit::Return),
            }),
            3 => graph.nodes[0].next = CallableFlowNext::Edges(vec![u32::MAX]),
            4 => {
                let node = graph
                    .nodes
                    .iter_mut()
                    .find(|n| matches!(n.next, CallableFlowNext::Edges(_)))
                    .unwrap();
                let CallableFlowNext::Edges(edges) = &mut node.next else {
                    unreachable!()
                };
                edges.push(edges[0]);
            }
            5 => graph.nodes[0].next = CallableFlowNext::Edges(Vec::new()),
            6 => {
                let node = graph.nodes.iter_mut().enumerate().find(|(index, n)| matches!(&n.next, CallableFlowNext::Edges(edges) if edges.iter().any(|next| *next as usize <= *index))).unwrap().1;
                node.next = CallableFlowNext::Exit(CallableExit::Return);
            }
            7 => {
                graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.next == CallableFlowNext::Exit(CallableExit::Return))
                    .unwrap()
                    .next = CallableFlowNext::Exit(CallableExit::Trap)
            }
            8 => {
                let node = graph
                    .nodes
                    .iter_mut()
                    .find(|n| matches!(&n.next, CallableFlowNext::Edges(edges) if edges.len() == 2))
                    .unwrap();
                let CallableFlowNext::Edges(edges) = &mut node.next else {
                    unreachable!()
                };
                edges.swap(0, 1);
            }
            _ => {
                // Isomorphic graph with changed IDs is still noncanonical.
                graph.nodes.swap(1, 2);
                for node in &mut graph.nodes {
                    if let CallableFlowNext::Edges(edges) = &mut node.next {
                        for target in edges {
                            *target = match *target {
                                1 => 2,
                                2 => 1,
                                other => other,
                            };
                        }
                    }
                }
            }
        }
        reject(
            &format!("shape-{mutation}"),
            changed,
            candidate.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}

#[test]
fn cyclic_cfg_cannot_claim_an_empty_or_truncated_finite_language() {
    let (module, candidate) = baseline(SOURCE);
    for paths in [
        Vec::new(),
        vec![CallableTrace {
            calls: vec![CallableKind::Fn],
            exit: CallableExit::Return,
        }],
    ] {
        let mut changed = module.clone();
        changed
            .functions
            .iter_mut()
            .find_map(|f| f.callable_contract.as_mut())
            .unwrap()
            .traces = CallableTraces::Finite(paths);
        reject(
            "finite-for-cycle",
            changed,
            candidate.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}

#[test]
fn zero_event_fn_once_cycle_stays_an_explicit_scope_limit() {
    let (mut module, candidate) = baseline(
        "fn idle<F:Fn()->i32>(f:F,n:i32)->i32{var i=n;while i>0{i-=1}return i}fn main()->i32{let f=move()=>7;return idle(f,3)}",
    );
    let claim = module
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    let CallableTraces::Flow(graph) = &claim.traces else {
        panic!("zero event loop must preserve a Flow");
    };
    assert!(graph.nodes.iter().all(|node| node.calls.is_empty()));
    claim.kind = CallableKind::FnOnce;
    reject(
        "zero-event-once",
        module,
        candidate,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
}

fn has_limit(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|d| d.category == ResultCategory::ResourceLimit)
}

#[test]
fn static_helper_depth_is_bounded_independently_of_runtime_iteration_count() {
    let count = 12;
    let mut source = "fn loop0<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}".to_owned();
    for index in 1..count {
        source.push_str(&format!(
            "fn loop{index}<F:Fn()->i32>(f:&F,n:i32)->i32=>loop{}(f,n)\n",
            index - 1
        ));
    }
    source.push_str(&format!(
        "fn main()->i32{{let f=move()=>1;return loop{}(&f,10000)-10000}}",
        count - 1
    ));
    let (module, candidate) = baseline(&source);
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    for depth in [count - 1, count] {
        let limits = Limits {
            max_call_depth: depth,
            ..Limits::default()
        };
        let analyzed = zil_analyze::analyze(&module, &limits);
        let raw =
            zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits);
        let wire = zil_verify::verify_module_bytes(&bytes, &module.target, &limits);
        if depth == count {
            analyzed.unwrap();
            raw.unwrap();
            wire.unwrap();
        } else {
            for errors in [analyzed.unwrap_err(), raw.unwrap_err(), wire.unwrap_err()] {
                assert!(has_limit(&errors), "{errors:?}");
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("helper dependency call depth")),
                    "{errors:?}"
                );
            }
        }
    }
    zil_verify::verify_module(
        module.clone(),
        candidate,
        &module.target,
        &Limits::default(),
    )
    .unwrap();
}

#[test]
fn exact_verification_work_frontier_rejects_without_poisoning_a_fresh_request() {
    let (module, candidate) = baseline(SOURCE);
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    for wire in [false, true] {
        let attempt = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if wire {
                zil_verify::verify_module_bytes(&bytes, &module.target, &limits).map(|_| ())
            } else {
                zil_verify::verify_module(
                    module.clone(),
                    candidate.clone(),
                    &module.target,
                    &limits,
                )
                .map(|_| ())
            }
        };
        let (mut low, mut high) = (0usize, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match attempt(mid) {
                Ok(()) => high = mid,
                Err(diagnostics) => {
                    assert!(has_limit(&diagnostics), "{diagnostics:?}");
                    low = mid;
                }
            }
        }
        assert!(has_limit(&attempt(high - 1).unwrap_err()));
        attempt(high).unwrap();
        attempt(Limits::default().max_analysis_steps).unwrap();
        fs::write(directory("work-frontier").join("boundary.json"), format!("{{\"wire\":{wire},\"minimum_steps\":{high},\"n_minus_1_rejected\":true,\"fresh_retry\":true}}\n")).unwrap();
    }
}
