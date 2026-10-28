//! Parent claims compose only independently checked callee bodies and origins.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{CallableExit, CallableKind, CandidatePlan, Constant, InstructionKind, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = r#"
fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn middle<F:Fn()->i32>(f:&F)->i32=>leaf(f)
fn outer<F:Fn()->i32>(f:&F)->i32{let n=middle(f);return leaf(f)+n}
fn main()->i32{let f=move()=>4;return outer(&f)-8}
"#;
const BRANCH: &str = r#"
fn leaf<F:Fn()->i32>(f:&F,flag:bool)->i32 {
    match flag {true=>{return f()},false=>{return 0}}
}
fn middle<F:Fn()->i32>(f:&F,flag:bool)->i32{let n=leaf(f,flag);return f()+n}
fn outer<F:Fn()->i32>(f:&F,flag:bool)->i32=>middle(f,flag)
fn main()->i32{let f=move()=>4;return outer(&f,true)-8}
"#;

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "raw188-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn baseline(source: &str) -> (TypedModule, CandidatePlan) {
    let s = SourceUnit::new(FileId(0), ModuleId(0), "nested188.zil", source);
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &ast).unwrap();
    let m = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
    let path = directory("baseline");
    fs::write(path.join("source.zil"), source).unwrap();
    fs::write(path.join("module.txt"), format!("{m:#?}")).unwrap();
    fs::write(path.join("candidate.txt"), format!("{p:#?}")).unwrap();
    fs::write(path.join("candidate.wire"), bytes).unwrap();
    (m, p)
}
fn reject(name: &str, m: TypedModule, mut p: CandidatePlan, category: ResultCategory, code: &str) {
    // Rebind only the untrusted hash. Never run Analyze on a forged module to
    // turn producer metadata into permission for the independent verifier.
    p.module_fingerprint = m.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (m.clone(), p.clone())
    );
    let results: [Vec<Diagnostic>; 2] = [
        zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err(),
    ];
    let path = directory(name);
    fs::write(path.join("module.txt"), format!("{m:#?}")).unwrap();
    fs::write(path.join("candidate.txt"), format!("{p:#?}")).unwrap();
    fs::write(path.join("candidate.wire"), bytes).unwrap();
    fs::write(path.join("results.txt"), format!("{results:#?}")).unwrap();
    for ds in results {
        assert!(
            ds.iter()
                .any(|d| d.phase == Phase::Verify && d.category == category && d.code == code),
            "{name}: {ds:?}"
        );
    }
}
fn helper(m: &TypedModule, nested_calls: usize) -> usize {
    m.functions.iter().position(|f| {
        f.callable_contract.is_some() && f.blocks.iter().flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind, InstructionKind::Call { function, .. } if m.functions[function.0 as usize].callable_contract.is_some()))
            .count() == nested_calls
    }).unwrap()
}

#[test]
fn raw_and_wire_do_not_trust_parent_or_coordinated_child_traces() {
    let (m, p) = baseline(SOURCE);
    let outer = helper(&m, 2);
    let leaf = helper(&m, 0);
    let mut changed = m.clone();
    changed.functions[outer]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
        .as_finite_mut()
        .unwrap()[0]
        .calls
        .clear();
    reject(
        "parent-omits-nested-calls",
        changed,
        p.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
    let mut changed = m.clone();
    for f in &mut changed.functions {
        if let Some(c) = &mut f.callable_contract {
            for trace in c.traces.as_finite_mut().unwrap() {
                trace.calls.clear();
            }
        }
    }
    reject(
        "coordinated-parent-child-omission",
        changed,
        p.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
    let mut changed = m;
    changed.functions[leaf]
        .callable_contract
        .as_mut()
        .unwrap()
        .kind = CallableKind::FnMut;
    changed.functions[leaf]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
        .as_finite_mut()
        .unwrap()[0]
        .calls[0] = CallableKind::FnMut;
    reject(
        "child-kind-forgery",
        changed,
        p,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}

#[test]
fn raw_and_wire_check_actual_nested_edges_and_child_instructions() {
    let (m, p) = baseline(SOURCE);
    for nested_calls in [0, 1] {
        let mut changed = m.clone();
        let index = helper(&m, nested_calls);
        let instruction = changed.functions[index]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| {
                if nested_calls == 0 {
                    matches!(i.kind, InstructionKind::CallClosureBorrowed { .. })
                } else {
                    matches!(i.kind, InstructionKind::Call { .. })
                }
            })
            .unwrap();
        instruction.kind = InstructionKind::Constant(Constant::I32(0));
        reject(
            if nested_calls == 0 {
                "removed-child-invocation"
            } else {
                "removed-parent-edge"
            },
            changed,
            p.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
    let leaf = helper(&m, 0);
    let mut changed = m.clone();
    changed.functions[leaf].callable_contract = None;
    reject(
        "removed-child-contract",
        changed,
        p.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
    let mut changed = m;
    changed.functions[leaf]
        .callable_contract
        .as_mut()
        .unwrap()
        .parameter = 1;
    reject(
        "wrong-child-parameter",
        changed,
        p,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}

#[test]
fn raw_and_wire_reject_recursive_actual_edges_despite_finite_claims() {
    let (mut m, p) = baseline(SOURCE);
    let middle = helper(&m, 1);
    let id = m.functions[middle].id;
    let instruction = m.functions[middle]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    let InstructionKind::Call { function, .. } = &mut instruction.kind else {
        unreachable!()
    };
    *function = id;
    reject(
        "recursive-actual-edge",
        m,
        p,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
}

#[test]
fn raw_and_wire_keep_zero_calls_and_stop_at_a_nested_trap() {
    let (m, p) = baseline(BRANCH);
    let leaf = helper(&m, 0);
    let middle = m
        .functions
        .iter()
        .position(|f| {
            f.callable_contract.is_some()
                && f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::CallClosureBorrowed { .. }))
                && f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::Call { .. }))
        })
        .unwrap();
    let actual: Vec<_> = m.functions[middle]
        .callable_contract
        .as_ref()
        .unwrap()
        .traces
        .as_finite()
        .unwrap()
        .iter()
        .map(|t| (t.calls.len(), t.exit))
        .collect();
    assert_eq!(
        actual,
        vec![
            (0, CallableExit::Trap),
            (1, CallableExit::Return),
            (2, CallableExit::Return)
        ]
    );
    for mutation in [
        "lost-zero-call-return",
        "lost-trap-exit",
        "continued-after-child-trap",
    ] {
        let mut changed = m.clone();
        match mutation {
            "lost-zero-call-return" => changed.functions[leaf]
                .callable_contract
                .as_mut()
                .unwrap()
                .traces
                .as_finite_mut()
                .unwrap()
                .retain(|t| !(t.calls.is_empty() && t.exit == CallableExit::Return)),
            "lost-trap-exit" => changed.functions[leaf]
                .callable_contract
                .as_mut()
                .unwrap()
                .traces
                .as_finite_mut()
                .unwrap()
                .retain(|t| t.exit != CallableExit::Trap),
            _ => {
                let trace = changed.functions[middle]
                    .callable_contract
                    .as_mut()
                    .unwrap()
                    .traces
                    .as_finite_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|t| t.exit == CallableExit::Trap)
                    .unwrap();
                trace.calls.push(CallableKind::Fn);
                changed.functions[middle]
                    .callable_contract
                    .as_mut()
                    .unwrap()
                    .traces
                    .as_finite_mut()
                    .unwrap()
                    .sort();
            }
        }
        reject(
            mutation,
            changed,
            p.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}

#[test]
fn another_factory_instance_with_the_same_closure_id_is_not_the_parameter() {
    let source = r#"
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn middle<F:Fn()->i32>(f:&F)->i32{let unrelated=make(99);let ignored=&unrelated;ignored;return leaf(f)}
fn main()->i32{let f=make(4);return middle(&f)-4}
"#;
    let (mut m, p) = baseline(source);
    let middle = helper(&m, 1);
    let leaf = m.functions[helper(&m, 0)].id;
    let f = &mut m.functions[middle];
    let local = f.locals.iter().find(|l| l.name == "unrelated").unwrap().id;
    let other = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::BorrowLocal { local: actual } if actual == local)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let instruction = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Call { function, .. } if function == leaf));
    // The factory precedes this ordinary helper call. Locate the call by its
    // actual address-typed argument rather than by source spelling or ID order.
    let instruction = instruction.unwrap();
    let InstructionKind::Call { arguments, .. } = &mut instruction.kind else {
        unreachable!()
    };
    assert_eq!(
        f.values[arguments[0].0 as usize],
        f.values[other.0 as usize]
    );
    arguments[0] = other;
    reject(
        "same-type-different-instance",
        m,
        p,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}

#[test]
fn empty_child_traces_do_not_authorize_reusing_an_owned_environment() {
    let source = r#"
resource R { n:i32 }
fn leaf<F:FnOnce()->i32>(f:F)->i32=>0
fn outer<F:FnOnce()->i32>(f:F)->i32{let n=leaf(f);return n+1}
fn main()->i32{let r=R{n:4};let f=move()=>r.n;return outer(f)-1}
"#;
    let (mut m, p) = baseline(source);
    let outer = helper(&m, 1);
    let f = &mut m.functions[outer];
    assert!(
        f.callable_contract
            .as_ref()
            .unwrap()
            .traces
            .as_finite()
            .unwrap()
            .iter()
            .all(|t| t.calls.is_empty())
    );
    let call = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| matches!(i.kind, InstructionKind::Call { .. }).then_some(i.kind.clone()))
        .unwrap();
    let constant = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Constant(Constant::I32(1))))
        .unwrap();
    constant.kind = call;
    reject(
        "owned-environment-forwarded-twice-with-zero-calls",
        m,
        p,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
    );
}

#[test]
fn nested_result_still_requires_the_actual_environment_hold() {
    let source = r#"
fn make(n:i32)->impl Fn()->&i32{return move()=>&n}
fn leaf<F:Fn()->&i32>(f:&F)->&i32=>f()
fn outer<F:Fn()->&i32>(f:&F)->&i32=>leaf(f)
fn main()->i32{let f=make(4);let view=outer(&f);return *view-4}
"#;
    let (m, mut p) = baseline(source);
    let mut changed = 0;
    for (function, plan) in m.functions.iter().zip(&mut p.functions) {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let InstructionKind::Call {
                function: target, ..
            } = instruction.kind
                && m.functions[target.0 as usize].callable_contract.is_some()
            {
                let hold = &mut plan.holds[instruction.result.unwrap().0 as usize];
                assert!(!hold.roots.is_empty());
                hold.roots.clear();
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 2);
    reject(
        "erased-nested-result-hold",
        m,
        p,
        ResultCategory::SafeReject,
        "E-VERIFY-HOLD",
    );
}

#[test]
fn analyze_reconstructs_its_own_callee_order_and_body_counts() {
    let (module, _) = baseline(SOURCE);
    let path = directory("independent-analyze");
    for mutation in ["all-claims-empty", "child-edge-cycle", "parent-claim-empty"] {
        let mut m = module.clone();
        let category = match mutation {
            "all-claims-empty" => {
                for f in &mut m.functions {
                    if let Some(c) = &mut f.callable_contract {
                        for t in c.traces.as_finite_mut().unwrap() {
                            t.calls.clear();
                        }
                    }
                }
                ResultCategory::SafeReject
            }
            "child-edge-cycle" => {
                let middle = helper(&m, 1);
                let id = m.functions[middle].id;
                for i in m.functions[middle]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                {
                    if let InstructionKind::Call { function, .. } = &mut i.kind {
                        *function = id;
                    }
                }
                ResultCategory::Unsupported
            }
            _ => {
                let outer = helper(&m, 2);
                m.functions[outer]
                    .callable_contract
                    .as_mut()
                    .unwrap()
                    .traces
                    .as_finite_mut()
                    .unwrap()[0]
                    .calls
                    .clear();
                ResultCategory::SafeReject
            }
        };
        let errors = zil_analyze::analyze(&m, &Limits::default()).unwrap_err();
        fs::write(path.join(format!("{mutation}.txt")), format!("{errors:#?}")).unwrap();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == category),
            "{mutation}: {errors:?}"
        );
    }
}

#[test]
fn dag_construction_and_trace_composition_pay_each_phase_budget() {
    let s = SourceUnit::new(FileId(0), ModuleId(0), "budget188.zil", BRANCH);
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let defaults = Limits::default();
    let core = zil_expand::expand(&s, &ast).unwrap();
    let target = TargetProfile::host_llvm();
    let m = zil_types::check(&core, &target, &defaults).unwrap();
    let p = zil_analyze::analyze(&m, &defaults).unwrap();
    let path = directory("budget");
    fs::write(path.join("source.zil"), BRANCH).unwrap();
    let mut results = Vec::new();
    for phase in [Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify] {
        let attempt = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            };
            match phase {
                Phase::Expand => zil_expand::expand_with_map_limits(&s, &ast, &limits).map(|_| ()),
                Phase::Types => zil_types::check(&core, &target, &limits).map(|_| ()),
                Phase::Analyze => zil_analyze::analyze(&m, &limits).map(|_| ()),
                Phase::Verify => {
                    zil_verify::verify_module(m.clone(), p.clone(), &target, &limits).map(|_| ())
                }
                _ => unreachable!(),
            }
            .map_err(|e| e[0].category)
        };
        let (mut low, mut high) = (0, defaults.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if attempt(mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(low > 0);
        assert!(attempt(low).is_ok());
        assert_eq!(attempt(low - 1), Err(ResultCategory::ResourceLimit));
        assert!(attempt(low).is_ok());
        results.push(format!("{{\"phase\":\"{phase:?}\",\"N\":{low},\"at_N\":\"Pass\",\"at_N_minus_1\":\"ResourceLimit\",\"retry\":\"Pass\"}}"));
    }
    fs::write(
        path.join("boundaries.json"),
        format!("[{}]\n", results.join(",")),
    )
    .unwrap();
}

#[test]
fn deep_finite_dependencies_pay_the_existing_instantiation_depth_bound() {
    let mut source = String::new();
    for index in 0..12 {
        source.push_str(&format!(
            "fn level{index}<F:Fn()->i32>(f:&F)->i32=>{}\n",
            if index == 11 {
                "f()".into()
            } else {
                format!("level{}(f)", index + 1)
            }
        ));
    }
    source.push_str("fn main()->i32{let f=move()=>4;return level0(&f)-4}");
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "depth188.zil", source.clone());
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let target = TargetProfile::host_llvm();
    zil_types::check(&core, &target, &Limits::default()).unwrap();
    let limits = Limits {
        max_call_depth: 8,
        ..Limits::default()
    };
    let errors = zil_types::check(&core, &target, &limits).unwrap_err();
    let path = directory("instantiation-depth");
    fs::write(path.join("source.zil"), source).unwrap();
    fs::write(path.join("results.txt"), format!("{errors:#?}")).unwrap();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Types
            && d.category == ResultCategory::ResourceLimit
            && d.message.contains("helper nesting")),
        "{errors:?}"
    );
    zil_types::check(&core, &target, &Limits::default()).unwrap();
}

#[test]
fn the_default_helper_depth_boundary_returns_a_diagnostic_without_host_recursion() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let path = directory("default-depth-boundary");
    for count in [limits.max_call_depth, limits.max_call_depth + 1] {
        let mut source = String::new();
        for i in 0..count {
            let call = if i + 1 == count {
                "f()".into()
            } else {
                format!("level{}(f)", i + 1)
            };
            source.push_str(&format!("fn level{i}<F:Fn()->i32>(f:&F)->i32=>{call}\n"));
        }
        source.push_str("fn main()->i32{let f=move()=>4;return level0(&f)-4}");
        fs::write(path.join(format!("depth-{count}.zil")), &source).unwrap();
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "default-depth188.zil", source);
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        let core = zil_expand::expand(&unit, &ast).unwrap();
        let result = zil_types::check(&core, &target, &limits);
        if count == limits.max_call_depth {
            let module = result.unwrap();
            let plan = zil_analyze::analyze(&module, &limits).unwrap();
            zil_verify::verify_module(module, plan, &target, &limits).unwrap();
            fs::write(
                path.join(format!("depth-{count}.txt")),
                "Types/Analyze/Verify Success\n",
            )
            .unwrap();
        } else {
            let errors = result.unwrap_err();
            fs::write(
                path.join(format!("depth-{count}.txt")),
                format!("{errors:#?}"),
            )
            .unwrap();
            assert!(
                errors.iter().any(|d| d.phase == Phase::Types
                    && d.category == ResultCategory::ResourceLimit
                    && d.message.contains("helper nesting")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn raw_core_dependency_cycles_do_not_rely_on_expand_or_the_host_stack() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "core-cycle188.zil", SOURCE);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let mut core = zil_expand::expand(&source, &ast).unwrap();
    let middle = core
        .functions
        .iter_mut()
        .find(|f| f.name == "middle")
        .unwrap();
    let zil_ir::CoreStatementKind::Return(Some(value)) = &mut middle.body.statements[0].kind else {
        panic!("middle return")
    };
    let zil_ir::CoreExprKind::Call { callee, .. } = &mut value.kind else {
        panic!("middle helper call")
    };
    callee.kind = zil_ir::CoreExprKind::Name("middle".into());
    let errors =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    let path = directory("core-cycle");
    fs::write(path.join("source.zil"), SOURCE).unwrap();
    fs::write(path.join("core.txt"), format!("{core:#?}")).unwrap();
    fs::write(path.join("results.txt"), format!("{errors:#?}")).unwrap();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Types
            && d.category == ResultCategory::Unsupported
            && d.message.contains("recursive concrete helper dependency")),
        "{errors:?}"
    );
}
