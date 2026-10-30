//! Untrusted-Core cost equivalence against the unchanged recursive checker.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, ResultCategory};

fn empty() -> CoreModule {
    CoreModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        functions: Vec::new(),
        aggregates: Vec::new(),
        enums: Vec::new(),
        unsupported: Vec::new(),
    }
}

fn walker<'a>(core: &'a CoreModule, target: &'a TargetProfile, limits: &'a Limits) -> Walker<'a> {
    Walker {
        core,
        target,
        limits,
        nodes: 0,
        work_left: limits.max_analysis_steps,
        next_node: None,
        steps_left: limits.max_ct_steps,
        context: None,
        compiler: None,
        scopes: Vec::new(),
        type_shapes: BTreeMap::new(),
    }
}

fn compound() -> Type {
    Type::Record {
        name: "Repeated".into(),
        fields: vec![
            ("first".into(), Type::Tuple(vec![Type::I64; 4])),
            (
                "second".into(),
                Type::Enum {
                    name: "Tagged".into(),
                    variants: vec![
                        ("Empty".into(), Vec::new()),
                        ("Pair".into(), vec![Type::I64, Type::Bool]),
                    ],
                },
            ),
        ],
    }
}

#[test]
fn compound_replay_preserves_all_node_work_edges_and_current_error_span() {
    let core = empty();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let ty = compound();
    let mut walker = walker(&core, &target, &limits);
    walker.pattern_type_shape(&ty, 0, Span::default()).unwrap();
    assert_eq!(walker.type_shapes.len(), 1);
    let cost_nodes = walker.nodes;
    let cost_work = limits.max_analysis_steps - walker.work_left;
    let span = Span::new(FileId(9), 100, 120);
    for available_nodes in 0..=cost_nodes + 1 {
        for available_work in 0..=cost_work + 1 {
            let initial_nodes = limits.max_nodes - available_nodes;
            walker.nodes = initial_nodes;
            walker.work_left = available_work;
            let mut reference_nodes = initial_nodes;
            let mut reference_work = available_work;
            let reference = hint_type(&ty, 0, &mut reference_nodes, &mut reference_work, &limits)
                .map(|_| ())
                .map_err(|mut errors| {
                    for error in &mut errors {
                        error.span = Some(span);
                    }
                    errors
                });
            let actual = walker.pattern_type_shape(&ty, 0, span);
            assert_eq!(
                actual, reference,
                "nodes={available_nodes} work={available_work}"
            );
            assert_eq!(
                (walker.nodes, walker.work_left),
                (reference_nodes, reference_work)
            );
            assert_eq!(walker.type_shapes.len(), 1);
        }
    }
}

#[test]
fn deeper_call_site_and_parameter_limits_cannot_reuse_a_shallower_success() {
    let core = empty();
    let target = TargetProfile::default();
    let limits = Limits {
        max_call_depth: 2,
        max_locals: 2,
        ..Limits::default()
    };
    let ty = compound();
    let invalid = Type::FunctionPointer {
        parameters: vec![Type::I64; 3],
        result: Box::new(Type::Unit),
    };
    let mut walker = walker(&core, &target, &limits);
    walker.pattern_type_shape(&ty, 0, Span::default()).unwrap();
    let span = Span::new(FileId(1), 7, 13);
    for (shape, depth) in [(&ty, 1), (&invalid, 0)] {
        let errors = walker.pattern_type_shape(shape, depth, span).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].message, "CT header type budget exceeded");
        assert_eq!(errors[0].span, Some(span));
        assert_eq!(walker.type_shapes.len(), 1, "failures must not be cached");
    }
}

#[test]
fn equal_types_have_separate_borrowed_identities_and_leaves_have_no_entries() {
    let core = empty();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let first = compound();
    let second = first.clone();
    let small = Type::Array {
        element: Box::new(Type::I64),
        length: 4,
    };
    let mut walker = walker(&core, &target, &limits);
    for _ in 0..3 {
        walker
            .pattern_type_shape(&Type::I64, 0, Span::default())
            .unwrap();
        walker
            .pattern_type_shape(&small, 0, Span::default())
            .unwrap();
    }
    assert!(walker.type_shapes.is_empty());
    walker
        .pattern_type_shape(&first, 0, Span::default())
        .unwrap();
    walker
        .pattern_type_shape(&second, 0, Span::default())
        .unwrap();
    assert_eq!(walker.type_shapes.len(), 2);
}
