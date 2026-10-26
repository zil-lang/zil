use super::*;
use graph::{Edge, Node, Slot};

const SOURCE: &str = "resource Token { n:i32 }\nfn read(t:Token)->i32{return t.n}\nfn f(flag:bool)->i32{let t=Token{n:7};let alias=t;var n=1;n+=1;if flag {return read(alias)+n}return read(alias)}\nfn main()->i32{return f(true)}";
fn fixture() -> (TypedModule, Graph) {
    let m = typed("test.zil", SOURCE).unwrap();
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    let g = graph::extract(&m, f).unwrap();
    (m, g)
}
fn expected(m: &TypedModule, g: &Graph) -> Vec<Row> {
    oracle_rows(m, &m.functions[g.function as usize], g).unwrap()
}

#[test]
fn real_source_all_methods_equal_independent_typed_oracle() {
    let (m, g) = fixture();
    production(m.clone()).unwrap();
    oracle::audit_extraction(&m, &m.functions[g.function as usize], &g).unwrap();
    let want = expected(&m, &g);
    for cut in [false, true] {
        let p = graph::plan(&g, cut).unwrap();
        assert_eq!(graph::solve(&g, &p).unwrap().0, want);
    }
    assert_eq!(
        block::solve(&g, &m.functions[g.function as usize])
            .unwrap()
            .0,
        want
    );
}
#[test]
fn removed_conditions_and_failure_exits_are_detected() {
    let (m, g) = fixture();
    let rows = expected(&m, &g);
    let flat = graph::plan(&g, false).unwrap();
    let good = snapshot(&g, &flat, &rows).unwrap();
    let mut bad = flat.clone();
    let branch = g
        .nodes
        .iter()
        .position(|n| n.edges.iter().any(|e| e.guard.is_some()))
        .unwrap();
    bad.edges[branch].pop();
    assert_ne!(snapshot(&g, &bad, &rows).unwrap(), good);
    let mut bad = flat.clone();
    let trap = g
        .nodes
        .iter()
        .position(|n| n.label.starts_with("conservative-failure"))
        .unwrap();
    bad.active[trap] = false;
    assert!(snapshot(&g, &bad, &rows).is_err());
}
#[test]
fn captured_use_and_full_may_rows_cannot_be_omitted() {
    let m = typed(
        "capture.zil",
        "resource T{n:i32}\nfn main()->i32{let a=T{n:1};let b=T{n:2};let f=()=>a.n+b.n;return f()}",
    )
    .unwrap();
    let f = m.functions.iter().find(|f| f.name == "main").unwrap();
    let g = graph::extract(&m, f).unwrap();
    let want = oracle_rows(&m, f, &g).unwrap();
    let mut bad = g.clone();
    let p = bad
        .nodes
        .iter()
        .position(|n| n.label.starts_with("MakeClosure") && !n.uses.is_empty())
        .unwrap();
    bad.nodes[p].uses.clear();
    assert_ne!(
        graph::solve(&bad, &graph::plan(&bad, false).unwrap())
            .unwrap()
            .0,
        want
    );
    let mut missing = want.clone();
    missing.pop();
    assert!(snapshot(&g, &graph::plan(&g, false).unwrap(), &missing).is_err());
}
#[test]
fn restore_summary_and_result_bit_tampering_are_detected() {
    let (m, g) = fixture();
    let want = expected(&m, &g);
    let mut p = graph::plan(&g, true).unwrap();
    let (i, _) = p
        .removed
        .iter()
        .enumerate()
        .find(|(_, x)| want[x.0].iter().any(|&w| w != 0))
        .unwrap();
    p.removed[i].1.clear();
    assert_ne!(graph::solve(&g, &p).unwrap().0, want);
    let mut forged = want.clone();
    let i = forged.iter().position(|r| !r.is_empty()).unwrap();
    forged[i][0] ^= 1;
    assert_ne!(forged, want);
}
#[test]
fn cleanup_order_and_function_identity_remain_observable() {
    let m = typed(
        "cleanup.zil",
        "resource T{n:i32}\nfn main()->i32{let a=T{n:1};let b=T{n:2};return a.n+b.n}",
    )
    .unwrap();
    let f = m.functions.iter().find(|f| f.name == "main").unwrap();
    let mut g = graph::extract(&m, f).unwrap();
    let rows = oracle_rows(&m, f, &g).unwrap();
    let p = graph::plan(&g, true).unwrap();
    let good = snapshot(&g, &p, &rows).unwrap();
    let drops = g
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.label.starts_with("DropManaged"))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(drops.len() >= 2);
    let a = g.nodes[drops[0]].label.clone();
    g.nodes[drops[0]].label = g.nodes[drops[1]].label.clone();
    g.nodes[drops[1]].label = a;
    assert_ne!(snapshot(&g, &p, &rows).unwrap(), good);
    g.function += 1;
    assert_ne!(snapshot(&g, &p, &rows).unwrap(), good);
}
#[test]
fn loop_backedge_and_error_tag_are_retained() {
    let m=typed("loop.zil","resource T{n:i32}\nenum E{Ok(i32),Err(i32)}\nfn f(stop:bool)->E{let t=T{n:1};let a=t;var n=0;while n<4{n+=1;if n==2{continue}if stop{break}a.n}if stop{return E.Err(a.n)}return E.Ok(a.n)}").unwrap();
    for f in &m.functions {
        let g = graph::extract(&m, f).unwrap();
        let rows = oracle_rows(&m, f, &g).unwrap();
        let p = graph::plan(&g, true).unwrap();
        assert_eq!(graph::solve(&g, &p).unwrap().0, rows);
        assert_eq!(
            graph::boundaries(&g, &p).unwrap(),
            graph::boundaries(&g, &graph::plan(&g, false).unwrap()).unwrap()
        );
    }
}
#[test]
fn bounded_fill_retains_point_and_exact_answer() {
    // Algebra-only stress, explicitly not part of the real-source timing corpus.
    let make = |i, observable, edges| Node {
        block: 0,
        ordinal: i,
        uses: vec![],
        kill: vec![],
        observable,
        label: format!("n{i}"),
        edges,
    };
    let mut nodes = (0..65)
        .map(|i| {
            make(
                i,
                true,
                vec![Edge {
                    to: 65,
                    guard: None,
                }],
            )
        })
        .collect::<Vec<_>>();
    nodes.push(make(
        65,
        false,
        vec![Edge {
            to: 66,
            guard: None,
        }],
    ));
    let mut end = make(66, true, vec![]);
    end.uses = vec![0];
    nodes.push(end);
    let g = Graph {
        function: 0,
        slots: vec![Slot::Local(0)],
        reachable: vec![true; 67],
        starts: vec![0],
        entry: 0,
        nodes,
        implicit_exits: 0,
        interaction: vec![],
    };
    let p = graph::plan(&g, true).unwrap();
    assert_eq!(p.metrics.retained_for_fill, 1);
    assert!(p.active[65]);
    let used = p.metrics.work;
    for limit in [used - 1, used, used + 1] {
        let attempt = graph::plan_with_limit(&g, true, limit);
        if limit < used {
            assert!(attempt.unwrap_err().starts_with("ResourceLimit: plan"));
        } else {
            let exact = attempt.unwrap();
            assert_eq!(exact.metrics.work, used);
            assert_eq!(exact.metrics.retained_for_fill, 1);
            assert!(exact.active[65]);
        }
    }
    assert_eq!(
        graph::solve(&g, &p).unwrap().0,
        graph::solve(&g, &graph::plan(&g, false).unwrap())
            .unwrap()
            .0
    );
}
#[test]
fn transparent_cycle_minimum_solution_is_not_fabricated() {
    let make = |i, edges| Node {
        block: 0,
        ordinal: i,
        uses: vec![],
        kill: vec![],
        observable: i == 0,
        label: format!("n{i}"),
        edges,
    };
    let g = Graph {
        function: 0,
        slots: vec![Slot::Local(0)],
        nodes: vec![
            make(0, vec![Edge { to: 1, guard: None }]),
            make(1, vec![Edge { to: 1, guard: None }]),
        ],
        starts: vec![0],
        entry: 0,
        reachable: vec![true; 2],
        implicit_exits: 0,
        interaction: vec![],
    };
    assert_eq!(
        graph::solve(&g, &graph::plan(&g, true).unwrap()).unwrap().0,
        vec![vec![0], vec![0]]
    );
    // TypedIR extraction separately retains every backward jump, so an actual
    // source divergence cannot be erased by this algebra-only identity example.
}

#[test]
fn identical_may_rows_do_not_hide_cross_resource_branch_correlation() {
    let source = "resource T{n:i32}\nenum E{Ok(i32),Err(i32)}\nfn f(flag:bool)->E{let a=T{n:1};let b=T{n:2};let fa=()=>a.n;let fb=()=>b.n;if flag{return E.Ok(fa())}return E.Err(fb())}";
    let m = typed("correlation.zil", source).unwrap();
    production(m.clone()).unwrap();
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    let g = graph::extract(&m, f).unwrap();
    oracle::audit_extraction(&m, f, &g).unwrap();
    let good = graph::plan(&g, true).unwrap();
    let want = oracle_rows(&m, f, &g).unwrap();
    let mut swapped = good.clone();
    let branch = g
        .nodes
        .iter()
        .position(|n| n.edges.iter().any(|e| e.guard.is_some()))
        .unwrap();
    for edge in &mut swapped.edges[branch] {
        edge.guard.as_mut().unwrap().1 ^= true;
    }
    // A single may set cannot distinguish which resource corresponds to true.
    assert_eq!(graph::solve(&g, &swapped).unwrap().0, want);
    // The shared, labeled event graph can. No resource-wise marginal is used.
    assert_ne!(
        snapshot(&g, &swapped, &want).unwrap(),
        snapshot(&g, &good, &want).unwrap()
    );
}

#[test]
fn raw_ir_audit_rejects_extractor_identity_and_transfer_mutants() {
    let (m, g) = fixture();
    let f = &m.functions[g.function as usize];
    let mut bad = g.clone();
    bad.slots.swap(0, 1);
    assert!(oracle::audit_extraction(&m, f, &bad).is_err());
    let mut bad = g.clone();
    let branch = bad
        .nodes
        .iter()
        .position(|n| n.edges.iter().any(|e| e.guard.is_some()))
        .unwrap();
    bad.nodes[branch].edges.pop();
    assert!(oracle::audit_extraction(&m, f, &bad).is_err());
    let mut bad = g.clone();
    let use_point = bad.nodes.iter().position(|n| !n.uses.is_empty()).unwrap();
    bad.nodes[use_point].uses.clear();
    assert!(oracle::audit_extraction(&m, f, &bad).is_err());
    let mut bad = g.clone();
    let event = bad
        .nodes
        .iter()
        .position(|n| n.label.starts_with("DropManaged"))
        .unwrap();
    bad.nodes[event].observable = false;
    assert!(oracle::audit_extraction(&m, f, &bad).is_err());
    let mut bad = g.clone();
    bad.reachable[0] = !bad.reachable[0];
    assert!(oracle::audit_extraction(&m, f, &bad).is_err());
}

#[test]
fn malformed_bitset_shape_is_rejected() {
    let (m, g) = fixture();
    let mut rows = expected(&m, &g);
    let plan = graph::plan(&g, true).unwrap();
    rows[0].push(0);
    assert!(snapshot(&g, &plan, &rows).is_err());
    let mut rows = expected(&m, &g);
    rows[0][0] |= 1u64 << 63;
    assert!(snapshot(&g, &plan, &rows).is_err());
}

#[test]
fn production_rejection_is_never_overridden_by_a_shadow_answer() {
    // Existing borrowed/mixed closure negative-source obligations; this tool
    // cannot manufacture Verified even when its narrower liveness query ends.
    for body in [
        "var x=1;let f=()=>x;x=2;return f();",
        "var x=1;let f=()=>x;let g=f;f();x=2;return g();",
        "var x=1;let p=&x;let f=move ()=>*p;x=2;return f();",
    ] {
        let source = format!("fn answer()->i32{{{body}}}");
        let m = typed("reject.zil", &source).unwrap();
        let error = production(m).unwrap_err();
        assert!(error.contains("E-BORROW-CONFLICT"), "{error}");
    }
}

#[test]
fn raw_audit_and_shape_enforce_exact_phase_boundaries() {
    let (m, g) = fixture();
    let f = &m.functions[g.function as usize];
    let used = oracle::audit_with_limit(&m, f, &g, budget::LIMIT).unwrap();
    for limit in [used - 1, used, used + 1] {
        let got = oracle::audit_with_limit(&m, f, &g, limit);
        if limit < used {
            assert!(
                got.unwrap_err()
                    .starts_with("ResourceLimit: raw extraction audit")
            );
        } else {
            assert_eq!(got.unwrap(), used);
        }
    }
    let (shape, used) = graph::shape_with_limit(&g, budget::LIMIT).unwrap();
    for limit in [used - 1, used, used + 1] {
        let got = graph::shape_with_limit(&g, limit);
        if limit < used {
            assert!(got.unwrap_err().starts_with("ResourceLimit: shape census"));
        } else {
            assert_eq!(got.unwrap(), (shape, used));
        }
    }
}
