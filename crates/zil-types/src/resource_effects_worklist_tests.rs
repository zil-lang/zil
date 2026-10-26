//! Hand-built Core tests isolate scheduling; driver tests cover source admission.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, NodeId};

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn expr(kind: CoreExprKind) -> CoreExpr {
    CoreExpr {
        id: NodeId(0),
        kind,
        span: span(),
    }
}
fn name(value: &str) -> CoreExpr {
    expr(CoreExprKind::Name(value.into()))
}
fn statement(kind: CoreStatementKind) -> CoreStatement {
    CoreStatement {
        id: NodeId(0),
        kind,
        span: span(),
    }
}
fn desc() -> Type {
    Type::Desc(Box::new(Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    }))
}
fn call(callee: CoreExpr, swap: bool) -> CoreStatement {
    statement(CoreStatementKind::Expression(expr(CoreExprKind::Call {
        callee: Box::new(callee),
        arguments: if swap {
            vec![name("b"), name("a")]
        } else {
            vec![name("a"), name("b")]
        },
    })))
}
fn graph(edges: &[Vec<usize>], writers: usize) -> CoreModule {
    CoreModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        functions: edges
            .iter()
            .enumerate()
            .map(|(index, callees)| {
                let mut statements = callees
                    .iter()
                    .map(|callee| call(name(&format!("f{callee}")), (index + callee) % 2 != 0))
                    .collect::<Vec<_>>();
                if writers & (1 << (index % usize::BITS as usize)) != 0 {
                    statements.push(statement(CoreStatementKind::Assign {
                        target: expr(CoreExprKind::Member {
                            object: Box::new(name("a")),
                            member: "n".into(),
                        }),
                        value: expr(CoreExprKind::Integer("1".into())),
                    }));
                }
                CoreFunction {
                    id: NodeId(index as u32),
                    name: format!("f{index}"),
                    callable_return: None,
                    callable_parameter: None,
                    foreign: None,
                    parameters: ["a", "b"]
                        .map(|name| CoreParameter {
                            name: name.into(),
                            ty: desc(),
                            mode: PassingMode::Describe,
                            span: span(),
                        })
                        .to_vec(),
                    return_type: Type::Unit,
                    result_mode: PassingMode::Infer,
                    body: CoreBlock {
                        statements,
                        span: span(),
                    },
                    receiver: None,
                    span: span(),
                }
            })
            .collect(),
        aggregates: vec![],
        enums: vec![],
        unsupported: vec![],
    }
}
fn table(core: &CoreModule) -> BTreeMap<String, Signature> {
    core.functions
        .iter()
        .enumerate()
        .map(|(i, f)| {
            (
                f.name.clone(),
                Signature {
                    id: FunctionId(i as u32),
                    callable: None,
                    callable_return: None,
                    template: None,
                    foreign: false,
                    cleanup: false,
                    parameters: f.parameters.iter().map(|p| p.ty.clone()).collect(),
                    result: Type::Unit,
                    public: false,
                    receiver: None,
                },
            )
        })
        .collect()
}
fn inputs<'a>(core: &'a CoreModule, table: &'a BTreeMap<String, Signature>) -> Vec<Input<'a>> {
    core.functions
        .iter()
        .map(|function| Input {
            function,
            signatures: table,
            imports: &[],
        })
        .collect()
}
fn enough() -> Limits {
    Limits {
        max_analysis_steps: usize::MAX,
        ..Limits::default()
    }
}

#[test]
fn exhaustive_three_node_graphs_and_seeds_match_sweeps() {
    for mask in 0..512u32 {
        let edges = (0..3)
            .map(|i| (0..3).filter(|j| mask & (1 << (i * 3 + j)) != 0).collect())
            .collect::<Vec<_>>();
        for seeds in 0..8 {
            let core = graph(&edges, seeds);
            let signatures = table(&core);
            let list = inputs(&core, &signatures);
            assert_eq!(
                solve_worklist(&list, None, &enough()).unwrap(),
                solve_sweeps(&list, None, &enough()).unwrap(),
                "graph={mask} seeds={seeds}"
            );
        }
    }
}

fn chain(n: usize, reverse: bool) -> CoreModule {
    let edges = (0..n)
        .map(|i| {
            if reverse {
                if i + 1 < n { vec![i + 1] } else { vec![] }
            } else if i > 0 {
                vec![i - 1]
            } else {
                vec![]
            }
        })
        .collect::<Vec<_>>();
    let mut core = graph(&edges, 0);
    let writer = if reverse { n - 1 } else { 0 };
    core.functions[writer]
        .body
        .statements
        .push(statement(CoreStatementKind::Assign {
            target: expr(CoreExprKind::Member {
                object: Box::new(name("a")),
                member: "n".into(),
            }),
            value: expr(CoreExprKind::Integer("1".into())),
        }));
    core
}
fn minimum(mut run: impl FnMut(usize) -> R<Writes>) -> usize {
    let (mut lo, mut hi) = (0, 100_000_000);
    assert!(run(hi).is_ok());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if run(mid).is_ok() {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}
#[test]
fn forward_and_reverse_chain_growth_is_measured_without_time_claims() {
    for n in [16, 32, 64, 128] {
        for reverse in [false, true] {
            let core = chain(n, reverse);
            let signatures = table(&core);
            let list = inputs(&core, &signatures);
            let limit = |steps| Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            let new = minimum(|s| solve_worklist(&list, None, &limit(s)));
            let old = minimum(|s| solve_sweeps(&list, None, &limit(s)));
            assert_eq!(
                solve_worklist(&list, None, &enough()).unwrap(),
                solve_sweeps(&list, None, &enough()).unwrap()
            );
            if reverse {
                assert!(new * 2 < old);
            }
            println!("functions={n} reverse={reverse} old_work={old} new_work={new}");
        }
    }
}
#[test]
fn small_dispatch_preserves_every_old_budget_boundary() {
    let core = chain(6, true);
    let signatures = table(&core);
    let work = minimum(|s| {
        solve_sweeps(
            &inputs(&core, &signatures),
            None,
            &Limits {
                max_analysis_steps: s,
                ..Limits::default()
            },
        )
    });
    for s in 0..=work {
        let limits = Limits {
            max_analysis_steps: s,
            ..Limits::default()
        };
        assert_eq!(
            format!("{:?}", solve(inputs(&core, &signatures), None, &limits)),
            format!(
                "{:?}",
                solve_sweeps(&inputs(&core, &signatures), None, &limits)
            )
        );
    }
}
#[test]
fn worklist_every_failure_prefix_and_recovery() {
    let core = chain(16, true);
    let signatures = table(&core);
    let list = inputs(&core, &signatures);
    let work = minimum(|s| {
        solve_worklist(
            &list,
            None,
            &Limits {
                max_analysis_steps: s,
                ..Limits::default()
            },
        )
    });
    for s in 0..work {
        let error = solve_worklist(
            &list,
            None,
            &Limits {
                max_analysis_steps: s,
                ..Limits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(error.phase, Phase::Types);
    }
    assert_eq!(
        solve_worklist(
            &list,
            None,
            &Limits {
                max_analysis_steps: work,
                ..Limits::default()
            }
        )
        .unwrap(),
        solve_sweeps(&list, None, &enough()).unwrap()
    );
}
#[test]
fn empty_worklist_and_seedless_cycles_have_no_effects() {
    assert!(
        solve_worklist(
            &[],
            None,
            &Limits {
                max_analysis_steps: 0,
                ..Limits::default()
            }
        )
        .unwrap()
        .is_empty()
    );
    let core = graph(&(0..32).map(|i| vec![(i + 1) % 32]).collect::<Vec<_>>(), 0);
    let signatures = table(&core);
    let actual = solve(inputs(&core, &signatures), None, &enough()).unwrap();
    assert_eq!(actual.len(), 32);
    assert!(actual.values().all(BTreeSet::is_empty));
}
#[test]
fn shadowed_calls_remain_conservative_and_imports_register_real_ids() {
    let mut core = graph(&[vec![], vec![]], 2);
    core.functions[0].body.statements = vec![
        statement(CoreStatementKind::Binding {
            name: "f1".into(),
            mutable: false,
            ty: None,
            initializer: Some(expr(CoreExprKind::Integer("0".into()))),
        }),
        call(name("f1"), false),
    ];
    let signatures = table(&core);
    let actual = solve_worklist(&inputs(&core, &signatures), None, &enough()).unwrap();
    assert_eq!(
        actual,
        solve_sweeps(&inputs(&core, &signatures), None, &enough()).unwrap()
    );
    assert_eq!(actual[&FunctionId(0)], Origins::from([0, 1]));
    core.functions[0].body.statements = vec![call(
        expr(CoreExprKind::Member {
            object: Box::new(name("dep")),
            member: "f1".into(),
        }),
        false,
    )];
    let imports = vec![CoreImport {
        alias: "dep".into(),
        module: ModuleId(1),
        span: span(),
    }];
    let tables = BTreeMap::from([(ModuleId(1), signatures.clone())]);
    let make_inputs = || -> Vec<Input<'_>> {
        core.functions
            .iter()
            .map(|function| Input {
                function,
                signatures: &signatures,
                imports: &imports,
            })
            .collect()
    };
    let actual = solve_worklist(&make_inputs(), Some(&tables), &enough()).unwrap();
    assert_eq!(
        actual,
        solve_sweeps(&make_inputs(), Some(&tables), &enough()).unwrap()
    );
    assert_eq!(actual[&FunctionId(0)], Origins::from([0]));
}
fn all_summary_values(core: &CoreModule) -> Vec<Writes> {
    (0..1usize << (2 * core.functions.len()))
        .map(|mask| {
            (0..core.functions.len())
                .filter_map(|f| {
                    let set: Origins = (0..2).filter(|p| mask & (1 << (2 * f + p)) != 0).collect();
                    // Also test a genuinely missing entry, not just empty sets.
                    (!set.is_empty()).then_some((FunctionId(f as u32), set))
                })
                .collect()
        })
        .collect()
}

// A summary perturbation may change writes, but must not reveal a previously
// absent dependency. This checks the actual reused Core transfer, not a model.
fn compare_transfers(core: &CoreModule, label: &str) -> usize {
    let signatures = table(core);
    let list = inputs(core, &signatures);
    let empty = Writes::new();
    let mut compared = 0;
    for input in &list {
        let mut work = usize::MAX;
        let (_, dependencies) =
            scan_once::<Tracked>(input, None, &empty, &mut work, &enough()).unwrap();
        for summaries in all_summary_values(core) {
            let mut work = usize::MAX;
            let (actual, observed) =
                scan_once::<Tracked>(input, None, &summaries, &mut work, &enough()).unwrap();
            let (expected, _) =
                scan_once::<Untracked>(input, None, &summaries, &mut work, &enough()).unwrap();
            assert_eq!(actual, expected, "{label}");
            assert_eq!(
                dependencies, observed,
                "dependency changed with summary: {label}"
            );
            for callee in 0..core.functions.len() {
                let mut changed = summaries.clone();
                changed.insert(FunctionId(callee as u32), Origins::from([0, 1]));
                let (next, _) =
                    scan_once::<Untracked>(input, None, &changed, &mut work, &enough()).unwrap();
                if next != actual {
                    assert!(
                        dependencies.contains(&FunctionId(callee as u32)),
                        "missing edge: {label}"
                    );
                }
            }
            compared += 1;
        }
    }
    assert_eq!(
        solve_worklist(&list, None, &enough()).unwrap(),
        solve_sweeps(&list, None, &enough()).unwrap(),
        "{label}"
    );
    compared
}
#[test]
fn every_two_node_transfer_and_summary_assignment_matches_local_relation() {
    let mut compared = 0;
    for mask in 0..16 {
        let edges: Vec<Vec<usize>> = (0..2)
            .map(|i| (0..2).filter(|j| mask & (1 << (2 * i + j)) != 0).collect())
            .collect();
        for seeds in 0..4 {
            compared += compare_transfers(&graph(&edges, seeds), "two-node relation");
        }
    }
    assert_eq!(compared, 2048);
}

fn block(statements: Vec<CoreStatement>) -> CoreBlock {
    CoreBlock {
        statements,
        span: span(),
    }
}
fn call_expr() -> CoreExpr {
    expr(CoreExprKind::Call {
        callee: Box::new(name("f1")),
        arguments: vec![name("a"), name("b")],
    })
}
fn alias_assign() -> CoreStatement {
    statement(CoreStatementKind::Assign {
        target: name("alias"),
        value: name("b"),
    })
}
fn alias_body() -> CoreBlock {
    block(vec![
        statement(CoreStatementKind::Expression(expr(CoreExprKind::Call {
            callee: Box::new(name("f1")),
            arguments: vec![name("alias"), name("b")],
        }))),
        alias_assign(),
    ])
}
fn pattern(kind: zil_ir::CorePatternKind) -> zil_ir::CorePattern {
    zil_ir::CorePattern { kind, span: span() }
}

// Exhaustive matches make a future schema extension require an explicit update
// to this differential matrix instead of silently passing through a wildcard.
fn expression_variant(kind: &CoreExprKind) -> &'static str {
    use CoreExprKind::*;
    match kind {
        Literal(_) => "Literal",
        Ascribed { .. } => "Ascribed",
        Integer(_) => "Integer",
        IntegerConvert { .. } => "IntegerConvert",
        Bool(_) => "Bool",
        String(_) => "String",
        Unit => "Unit",
        Name(_) => "Name",
        Unary { .. } => "Unary",
        Binary { .. } => "Binary",
        Call { .. } => "Call",
        Tuple(_) => "Tuple",
        Array(_) => "Array",
        Index { .. } => "Index",
        Slice { .. } => "Slice",
        Member { .. } => "Member",
        ReceiverMember(_) => "ReceiverMember",
        Record { .. } => "Record",
        RecordUpdate { .. } => "RecordUpdate",
        Sequence { .. } => "Sequence",
        CompileTime(_) => "CompileTime",
        Try(_) => "Try",
        Closure { .. } => "Closure",
        If { .. } => "If",
        Unsupported(_) => "Unsupported",
    }
}
fn statement_variant(kind: &CoreStatementKind) -> &'static str {
    use CoreStatementKind::*;
    match kind {
        Binding { .. } => "Binding",
        Assign { .. } => "Assign",
        Expression(_) => "Expression",
        Return(_) => "Return",
        If { .. } => "If",
        IfLet { .. } => "IfLet",
        While { .. } => "While",
        WhileLet { .. } => "WhileLet",
        ForRange { .. } => "ForRange",
        Match { .. } => "Match",
        Block(_) => "Block",
        Unsafe(_) => "Unsafe",
        Break => "Break",
        Continue => "Continue",
        Unsupported(_) => "Unsupported",
    }
}

#[test]
fn every_current_core_variant_preserves_origin_support_and_transfer() {
    use zil_ir::{CoreClosureParameter, CoreMatchArm, CorePatternKind as P};
    let expressions = vec![
        CoreExprKind::Literal(Constant::I32(0)),
        CoreExprKind::Ascribed {
            ty: desc(),
            value: Box::new(name("a")),
        },
        CoreExprKind::Integer("0".into()),
        CoreExprKind::IntegerConvert {
            value: Box::new(name("a")),
            target: Type::I64,
        },
        CoreExprKind::Bool(false),
        CoreExprKind::String("x".into()),
        CoreExprKind::Unit,
        CoreExprKind::Name("a".into()),
        CoreExprKind::Unary {
            op: UnaryOp::Borrow,
            operand: Box::new(name("a")),
        },
        CoreExprKind::Binary {
            op: BinaryOp::Add,
            left: Box::new(call_expr()),
            right: Box::new(name("b")),
        },
        call_expr().kind,
        CoreExprKind::Tuple(vec![name("a"), call_expr()]),
        CoreExprKind::Array(vec![name("a"), call_expr()]),
        CoreExprKind::Index {
            object: Box::new(name("a")),
            index: Box::new(call_expr()),
        },
        CoreExprKind::Slice {
            object: Box::new(name("a")),
            start: Some(Box::new(call_expr())),
            end: Some(Box::new(name("b"))),
        },
        CoreExprKind::Member {
            object: Box::new(name("a")),
            member: "n".into(),
        },
        CoreExprKind::ReceiverMember("n".into()),
        CoreExprKind::Record {
            ty: desc(),
            fields: vec![("n".into(), call_expr())],
        },
        CoreExprKind::RecordUpdate {
            ty: None,
            base: Box::new(name("a")),
            fields: vec![("n".into(), call_expr())],
        },
        CoreExprKind::Sequence {
            bindings: vec![("local".into(), name("a"))],
            value: Box::new(name("local")),
        },
        CoreExprKind::CompileTime(Box::new(name("a"))),
        CoreExprKind::Try(Box::new(name("a"))),
        CoreExprKind::Closure {
            move_capture: false,
            parameters: vec![CoreClosureParameter {
                name: "a".into(),
                ty: Some(desc()),
                mode: PassingMode::Infer,
                span: span(),
            }],
            return_type: None,
            body: block(vec![statement(CoreStatementKind::Expression(call_expr()))]),
        },
        CoreExprKind::If {
            condition: Box::new(call_expr()),
            then_value: Box::new(name("a")),
            else_value: Box::new(name("b")),
        },
        CoreExprKind::Unsupported("test-only Core leaf".into()),
    ];
    assert_eq!(
        expressions
            .iter()
            .map(expression_variant)
            .collect::<BTreeSet<_>>()
            .len(),
        25
    );
    let mut compared = 0;
    for kind in expressions {
        let label = expression_variant(&kind);
        let mut core = graph(&[vec![], vec![]], 2);
        core.functions[0].body = block(vec![statement(CoreStatementKind::Expression(expr(
            CoreExprKind::Call {
                callee: Box::new(name("f1")),
                arguments: vec![expr(kind), name("b")],
            },
        )))]);
        compared += compare_transfers(&core, label);
    }
    let statements = vec![
        CoreStatementKind::Binding {
            name: "inner".into(),
            mutable: false,
            ty: None,
            initializer: Some(name("a")),
        },
        alias_assign().kind,
        CoreStatementKind::Expression(call_expr()),
        CoreStatementKind::Return(Some(call_expr())),
        CoreStatementKind::Return(None),
        CoreStatementKind::If {
            condition: call_expr(),
            then_body: alias_body(),
            else_body: Some(alias_body()),
        },
        CoreStatementKind::IfLet {
            pattern: pattern(P::Tuple(vec![
                pattern(P::Name("a".into())),
                pattern(P::Wildcard),
            ])),
            value: name("b"),
            then_body: alias_body(),
            else_body: Some(alias_body()),
        },
        CoreStatementKind::While {
            condition: expr(CoreExprKind::Bool(false)),
            body: alias_body(),
        },
        CoreStatementKind::WhileLet {
            pattern: pattern(P::Variant {
                ty: Type::I32,
                variant: "V".into(),
                fields: vec![pattern(P::Name("a".into()))],
            }),
            value: name("b"),
            body: alias_body(),
        },
        CoreStatementKind::ForRange {
            name: "a".into(),
            start: call_expr(),
            end: expr(CoreExprKind::Integer("2".into())),
            body: alias_body(),
        },
        CoreStatementKind::Match {
            value: call_expr(),
            arms: vec![
                CoreMatchArm {
                    pattern: pattern(P::Integer("1".into())),
                    body: alias_body(),
                    span: span(),
                },
                CoreMatchArm {
                    pattern: pattern(P::Bool(false)),
                    body: alias_body(),
                    span: span(),
                },
            ],
        },
        CoreStatementKind::Block(alias_body()),
        CoreStatementKind::Unsafe(alias_body()),
        CoreStatementKind::Break,
        CoreStatementKind::Continue,
        CoreStatementKind::Unsupported("test-only Core statement".into()),
    ];
    assert_eq!(
        statements
            .iter()
            .map(statement_variant)
            .collect::<BTreeSet<_>>()
            .len(),
        15
    );
    for kind in statements {
        let label = statement_variant(&kind);
        let mut core = graph(&[vec![], vec![]], 2);
        core.functions[0].body = block(vec![
            statement(CoreStatementKind::Binding {
                name: "alias".into(),
                mutable: true,
                ty: None,
                initializer: Some(name("a")),
            }),
            statement(kind),
        ]);
        compared += compare_transfers(&core, label);
    }
    assert_eq!(compared, (25 + 16) * 16 * 2);
}

#[test]
fn direct_byte_primitives_foreign_and_unknown_calls_remain_seeds() {
    let mut compared = 0;
    for primitive in [
        "bytes_new",
        "bytes_len",
        "bytes_capacity",
        "bytes_reserve",
        "bytes_push",
        "bytes_view",
        "bytes_view_mut",
        "not_resolved",
    ] {
        let mut core = graph(&[vec![], vec![]], 0);
        core.functions[0].body = block(vec![statement(CoreStatementKind::Expression(expr(
            CoreExprKind::Call {
                callee: Box::new(name(primitive)),
                arguments: vec![name("a"), name("b")],
            },
        )))]);
        compared += compare_transfers(&core, primitive);
    }
    let mut core = graph(&[vec![], vec![]], 0);
    core.functions[0].foreign = Some(zil_ir::ForeignDeclaration {
        abi: "C".into(),
        symbol: "model_only".into(),
        noescape_parameters: vec![],
        static_callbacks: false,
    });
    compared += compare_transfers(&core, "foreign model only");
    assert_eq!(compared, 9 * 16 * 2);
}

#[test]
fn larger_mixed_graphs_and_function_order_permutations_match() {
    let mut state = 0x133decafu64;
    for n in [16, 31, 64] {
        for _ in 0..20 {
            let edges = (0..n)
                .map(|_| {
                    (0..3)
                        .map(|_| {
                            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                            (state as usize) % n
                        })
                        .collect()
                })
                .collect::<Vec<_>>();
            let mut core = graph(&edges, 1);
            let signatures = table(&core);
            for _ in 0..3 {
                let list = inputs(&core, &signatures);
                assert_eq!(
                    solve_worklist(&list, None, &enough()).unwrap(),
                    solve_sweeps(&list, None, &enough()).unwrap()
                );
                core.functions.rotate_left(n / 3);
            }
        }
    }
}

#[test]
fn disabled_dependency_collection_is_zero_sized() {
    assert_eq!(std::mem::size_of::<Untracked>(), 0);
    assert!(std::mem::size_of::<Tracked>() > 0);
    assert_eq!(
        std::mem::size_of::<Scan<Tracked>>() - std::mem::size_of::<Scan<Untracked>>(),
        std::mem::size_of::<Tracked>()
    );
}
