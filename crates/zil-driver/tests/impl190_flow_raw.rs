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
    CallableCleanup, CallableExit, CallableFlow, CallableFlowNext, CallableFlowNode, CallableKind,
    CallableTrace, CallableTraces, CandidatePlan, Constant, InstructionKind, Terminator,
    TypedModule,
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

#[test]
fn actual_instructions_and_cfg_are_checked_against_the_graph_independently() {
    let (module, candidate) = baseline(SOURCE);
    for mutation in 0..2 {
        let mut changed = module.clone();
        let function = changed
            .functions
            .iter_mut()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        if mutation == 0 {
            let instruction = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::CallClosureBorrowed { .. }))
                .unwrap();
            instruction.kind = InstructionKind::Constant(Constant::I32(0));
        } else {
            let header = function
                .blocks
                .iter()
                .find(|b| matches!(b.terminator, Terminator::Branch { .. }))
                .unwrap()
                .id;
            let body = function
                .blocks
                .iter_mut()
                .find(|b| {
                    matches!(b.terminator, Terminator::Jump(target) if target == header)
                        && b.instructions
                            .iter()
                            .any(|i| matches!(i.kind, InstructionKind::CallClosureBorrowed { .. }))
                })
                .unwrap();
            body.terminator = Terminator::Trap;
        }
        reject(
            &format!("actual-body-{mutation}"),
            changed,
            candidate.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}

#[test]
fn nested_flow_copies_do_not_share_continuations_or_trust_coordinated_claims() {
    let source = r#"
fn leaf<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}
fn outer<F:Fn()->i32>(f:&F,n:i32)->i32{let first=leaf(f,n);let middle=f();let second=leaf(f,n+1);return first+middle+second}
fn main()->i32{let f=move()=>1;return outer(&f,3)-8}
"#;
    let (module, candidate) = baseline(source);
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|f| matches!(
                f.callable_contract.as_ref().map(|c| &c.traces),
                Some(CallableTraces::Flow(_))
            ))
            .count(),
        2
    );
    let mut changed = module.clone();
    for function in &mut changed.functions {
        if let Some(contract) = &mut function.callable_contract {
            let CallableTraces::Flow(flow) = &mut contract.traces else {
                panic!("both helpers are cyclic");
            };
            for node in &mut flow.nodes {
                node.calls.clear();
            }
        }
    }
    reject(
        "coordinated-empty-flow",
        changed,
        candidate.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
    let mut changed = module;
    let graph = changed
        .functions
        .iter_mut()
        .filter_map(|f| f.callable_contract.as_mut())
        .filter_map(|c| match &mut c.traces {
            CallableTraces::Flow(flow) => Some(flow),
            _ => None,
        })
        .max_by_key(|f| f.nodes.len())
        .unwrap();
    let last = graph
        .nodes
        .iter()
        .position(|n| n.next == CallableFlowNext::Exit(CallableExit::Return))
        .unwrap() as u32;
    let next = graph.nodes.iter_mut().enumerate().find(|(index, n)| matches!(&n.next, CallableFlowNext::Edges(edges) if edges.len() == 1 && edges[0] as usize > *index && edges[0] != last)).unwrap().1;
    next.next = CallableFlowNext::Edges(vec![last]);
    reject(
        "stolen-child-continuation",
        changed,
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}

#[test]
fn a_same_typed_factory_instance_cannot_replace_the_parameter_on_a_backedge() {
    let source = r#"
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{let unrelated=make(99);let ignored=&unrelated;ignored;var i=0;var sum=0;while i<n{sum+=leaf(f);i+=1}return sum}
fn main()->i32{let f=make(4);return run(&f,3)-12}
"#;
    let (mut module, candidate) = baseline(source);
    let leaf = module
        .functions
        .iter()
        .find(|f| {
            matches!(
                f.callable_contract.as_ref().map(|c| &c.traces),
                Some(CallableTraces::Finite(_))
            )
        })
        .unwrap()
        .id;
    let function = module
        .functions
        .iter_mut()
        .find(|f| {
            matches!(
                f.callable_contract.as_ref().map(|c| &c.traces),
                Some(CallableTraces::Flow(_))
            )
        })
        .unwrap();
    let local = function
        .locals
        .iter()
        .find(|l| l.name == "unrelated")
        .unwrap()
        .id;
    let other = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::BorrowLocal { local: actual } if local == actual)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let call = function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(
            |i| matches!(i.kind, InstructionKind::Call { function: callee, .. } if callee == leaf),
        )
        .unwrap();
    let InstructionKind::Call { arguments, .. } = &mut call.kind else {
        unreachable!()
    };
    assert_eq!(
        function.values[arguments[0].0 as usize],
        function.values[other.0 as usize]
    );
    arguments[0] = other;
    reject(
        "same-closure-id-on-loop",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}

#[test]
fn a_nonreturning_cycle_keeps_its_reachable_call_prefix() {
    let (mut module, _) = baseline(SOURCE);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.callable_contract.is_some())
        .unwrap();
    // Form a legitimate raw CFG with an unconditional runtime backedge. The
    // original return block becomes unreachable; no infinite machine is run.
    let header = function
        .blocks
        .iter_mut()
        .find(|b| matches!(b.terminator, Terminator::Branch { .. }))
        .unwrap();
    let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = header.terminator
    else {
        unreachable!()
    };
    header.terminator = Terminator::Jump(then_block);
    // Every declared SSA value still needs a typed definition, and the normal
    // initialization checker also checks disconnected blocks. Its obsolete
    // return load is replaced by a same-typed constant; no safety gate changes.
    for instruction in &mut function.blocks[else_block.0 as usize].instructions {
        if matches!(instruction.kind, InstructionKind::Load(_)) {
            instruction.kind = InstructionKind::Constant(Constant::I32(0));
        }
    }
    let CallableTraces::Flow(graph) = &mut function.callable_contract.as_mut().unwrap().traces
    else {
        unreachable!()
    };
    let exit = graph
        .nodes
        .iter()
        .position(|n| n.next == CallableFlowNext::Exit(CallableExit::Return))
        .unwrap();
    graph.nodes.remove(exit);
    for node in &mut graph.nodes {
        if let CallableFlowNext::Edges(edges) = &mut node.next {
            edges.retain(|target| *target as usize != exit);
            for target in edges {
                if *target as usize > exit {
                    *target -= 1;
                }
            }
        }
    }
    assert!(graph.nodes.iter().any(|n| !n.calls.is_empty()));
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| !matches!(n.next, CallableFlowNext::Exit(_)))
    );
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
    fs::write(
        directory("nonreturning-positive").join("candidate.wire"),
        bytes,
    )
    .unwrap();
    let graph = flow(&mut module);
    for node in &mut graph.nodes {
        node.calls.clear();
    }
    reject(
        "nonreturning-prefix-omission",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}

#[test]
fn child_copy_size_is_checked_before_allocating_a_forged_short_parent_claim() {
    let mut source = "fn leaf0<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}".to_owned();
    for i in 1..=5 {
        source.push_str(&format!(
            "fn leaf{i}<F:Fn()->i32>(f:&F,n:i32)->i32=>leaf{}(f,n)+leaf{}(f,n)\n",
            i - 1,
            i - 1
        ));
    }
    source.push_str("fn main()->i32{let f=move()=>1;return leaf5(&f,1)-32}");
    let (module, candidate) = baseline(&source);
    let (parent, count) = module
        .functions
        .iter()
        .enumerate()
        .filter_map(
            |(i, f)| match f.callable_contract.as_ref().map(|c| &c.traces) {
                Some(CallableTraces::Flow(flow)) => Some((i, flow.nodes.len())),
                _ => None,
            },
        )
        .max_by_key(|(_, count)| *count)
        .unwrap();
    let exact = Limits {
        max_nodes: count,
        ..Limits::default()
    };
    zil_analyze::analyze(&module, &exact).unwrap();
    zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &exact).unwrap();
    let mut changed = module;
    changed.functions[parent]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Flow(CallableFlow {
        entry: 0,
        nodes: vec![CallableFlowNode {
            calls: Vec::new(),
            next: CallableFlowNext::Edges(vec![0]),
        }],
    });
    let mut candidate = candidate;
    candidate.module_fingerprint = changed.fingerprint();
    let limits = Limits {
        max_nodes: count - 1,
        ..Limits::default()
    };
    for errors in [
        zil_analyze::analyze(&changed, &limits).unwrap_err(),
        zil_verify::verify_module(changed.clone(), candidate, &changed.target, &limits)
            .unwrap_err(),
    ] {
        assert!(has_limit(&errors), "{errors:?}");
        assert!(
            errors.iter().any(|d| d.message.contains("helper flow")),
            "{errors:?}"
        );
    }
}

#[test]
fn a_flow_does_not_waive_hold_call_write_or_cleanup_obligations() {
    let (module, candidate) = baseline(SOURCE);
    for mutation in 0..4 {
        let mut changed = module.clone();
        let claim = changed
            .functions
            .iter_mut()
            .find_map(|f| f.callable_contract.as_mut())
            .unwrap();
        match mutation {
            0 => claim.hold.push(0),
            1 => claim.call.push(0),
            2 => claim.writes.push(0),
            _ => claim.cleanup = CallableCleanup::Owned,
        }
        reject(
            &format!("envelope-{mutation}"),
            changed,
            candidate.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}

#[test]
fn a_repeatable_label_does_not_authorize_consuming_an_owned_capture() {
    let (mut module, candidate) = baseline(
        "resource R{n:i32}fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}fn main()->i32{let owned=R{n:4};let f=move()=>{let scratch=R{n:99};return owned.n};return run(&f,3)-12}",
    );
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.environment.is_some())
        .unwrap();
    let make = function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
        .unwrap();
    make.kind = InstructionKind::TakeCapture { index: 0 };
    reject(
        "repeatable-capture-consumption",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-CLOSURE-CALL",
    );
}
