//! Untrusted Core controls for request-local sparse declaration views.
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, NodeId, Phase, ResultCategory, SCHEMA_VERSION, Span,
};
use zil_ir::*;

fn span(module: u32, start: usize) -> Span {
    Span::new(FileId(module), start, start + 1)
}

fn module(id: u32) -> ProgramModule {
    ProgramModule {
        core: CoreModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(id),
            source_fingerprint: Fingerprint::ZERO,
            functions: Vec::new(),
            aggregates: Vec::new(),
            enums: Vec::new(),
            unsupported: Vec::new(),
        },
        imports: Vec::new(),
        exports: Vec::new(),
    }
}

fn program(modules: Vec<ProgramModule>) -> CoreProgram {
    CoreProgram {
        root: modules[0].core.module_id,
        source_fingerprint: Fingerprint::ZERO,
        modules,
    }
}

fn tag(owner: u32, name: &str) -> CoreEnum {
    CoreEnum {
        id: NodeId(1),
        name: name.into(),
        variants: vec![("Some".into(), vec![Type::I64])],
        span: span(owner, 2),
    }
}

#[test]
fn empty_modules_preserve_the_frozen_per_declaration_budget() {
    let mut modules = Vec::new();
    for i in 0..8 {
        let mut m = module(i);
        m.core.aggregates.push(CoreAggregate {
            id: NodeId(0),
            name: format!("$module{i}_Cell"),
            kind: AggregateKind::Struct,
            teardown: None,
            fields: vec![CoreField {
                name: "value".into(),
                ty: Type::I64,
                public: true,
                manages: false,
                span: span(i, 1),
            }],
            span: span(i, 0),
        });
        let mut e = tag(i, &format!("$module{i}_Tag"));
        e.variants[0].1 = vec![Type::Named(format!("$module{i}_Cell"))];
        m.core.enums.push(e);
        modules.push(m);
        modules.extend((0..16).map(|n| module(100 + i * 16 + n)));
    }
    let input = program(modules);
    // CP10 froze 1568/1567 for these eight declaration-bearing modules.
    // Inserting empty tables has never added logical resolution charges.
    let limits = Limits {
        max_analysis_steps: 1568,
        ..Limits::default()
    };
    let accepted = zil_types::resolve_program_shapes(&input, &limits).unwrap();
    let errors = zil_types::resolve_program_shapes(
        &input,
        &Limits {
            max_analysis_steps: 1567,
            ..limits.clone()
        },
    )
    .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Types);
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].message, "aggregate resolution budget exceeded");
    assert_eq!(errors[0].span, Some(span(7, 1)));
    assert_eq!(
        zil_types::resolve_program_shapes(&input, &limits).unwrap(),
        accepted
    );
    for m in accepted.modules.iter().filter(|m| !m.core.enums.is_empty()) {
        let Type::Record { name, .. } = &m.core.enums[0].variants[0].1[0] else {
            panic!("enum payload was not resolved");
        };
        assert_eq!(
            name,
            &format!(
                "@module{}:$module{}_Cell",
                m.core.module_id.0, m.core.module_id.0
            )
        );
    }
}

#[test]
fn local_enums_with_the_same_name_remain_module_scoped() {
    let mut modules = vec![module(5), module(8), module(2), module(11), module(9)];
    modules[0].core.enums.push(tag(5, "Tag"));
    modules[2].core.enums.push(tag(2, "Tag"));
    let input = program(modules);
    let accepted = zil_types::resolve_program_shapes(&input, &Limits::default()).unwrap();
    assert_eq!(accepted, input);
    zil_types::check_program(
        &input,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
}

#[test]
fn sparse_enum_conflicts_follow_input_order_and_do_not_leak_between_requests() {
    let mut modules = vec![module(0), module(9), module(8), module(1)];
    for i in [1, 3] {
        let owner = modules[i].core.module_id.0;
        modules[i].core.enums.push(tag(owner, "$module0_Tag"));
    }
    let mut input = program(modules);
    for expected in [1, 9, 1] {
        let errors = zil_types::resolve_program_shapes(&input, &Limits::default()).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "E-NAME-DUPLICATE");
        assert_eq!(errors[0].message, "duplicate enum identity in project");
        assert_eq!(errors[0].span, Some(span(expected, 2)));
        input.modules.swap(1, 3);
    }
    let fresh = program(vec![module(0), module(1)]);
    assert_eq!(
        zil_types::resolve_program_shapes(&fresh, &Limits::default()).unwrap(),
        fresh
    );
}

#[test]
fn modules_without_shapes_still_validate_schema_imports_and_module_limits() {
    let mut input = program(vec![module(0), module(1), module(2)]);
    input.modules[1].core.schema_version = SCHEMA_VERSION + 1;
    assert_eq!(
        zil_types::resolve_program_shapes(&input, &Limits::default()).unwrap_err()[0].code,
        "E-SCHEMA-VERSION"
    );
    input.modules[1].core.schema_version = SCHEMA_VERSION;
    input.modules[1].imports.push(CoreImport {
        alias: "missing".into(),
        module: ModuleId(7),
        span: span(1, 5),
    });
    let errors = zil_types::resolve_program_shapes(&input, &Limits::default()).unwrap_err();
    assert_eq!(errors[0].code, "E-MODULE-NOT-FOUND");
    assert_eq!(errors[0].span, Some(span(1, 5)));
    input.modules[1].imports.clear();
    let errors = zil_types::resolve_program_shapes(
        &input,
        &Limits {
            max_nodes: 2,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].message, "source closure module budget exceeded");
}
