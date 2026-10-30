//! Raw untrusted Core controls; source-to-native evidence is separate.
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, NodeId, Phase, ResultCategory, SCHEMA_VERSION, Span,
};
use zil_ir::*;

const STATE: &str = "$generic_$module0_State_F_G";

fn span(module: u32, start: usize) -> Span {
    Span::new(FileId(module), start, start + 1)
}

fn function(module: u32, name: &str) -> CoreFunction {
    CoreFunction {
        id: NodeId(10),
        name: name.into(),
        type_parameters: Vec::new(),
        state_parameters: Vec::new(),
        callable_return: None,
        callable_parameters: Vec::new(),
        foreign: None,
        parameters: Vec::new(),
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        body: CoreBlock {
            statements: Vec::new(),
            span: span(module, 10),
        },
        receiver: None,
        span: span(module, 10),
    }
}

fn parameter(module: u32, name: &str, ty: Type) -> CoreParameter {
    CoreParameter {
        name: name.into(),
        ty,
        mode: PassingMode::Infer,
        span: span(module, 11),
    }
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

fn project(callable: bool, state: bool) -> CoreProgram {
    let mut modules: Vec<_> = (0..4).map(module).collect();
    modules[0].core.aggregates.push(CoreAggregate {
        id: NodeId(0),
        name: STATE.into(),
        kind: AggregateKind::Package,
        teardown: None,
        fields: vec![
            CoreField {
                name: "storage".into(),
                ty: Type::Named("ByteBuffer".into()),
                public: true,
                manages: true,
                span: span(0, 1),
            },
            CoreField {
                name: "first".into(),
                ty: Type::Named("F".into()),
                public: true,
                manages: false,
                span: span(0, 2),
            },
            CoreField {
                name: "second".into(),
                ty: Type::Named("G".into()),
                public: true,
                manages: false,
                span: span(0, 3),
            },
        ],
        span: span(0, 0),
    });
    modules[0].core.enums.push(CoreEnum {
        id: NodeId(1),
        name: "$module0_Tag".into(),
        variants: vec![
            ("Some".into(), vec![Type::I64]),
            ("None".into(), Vec::new()),
        ],
        span: span(0, 4),
    });
    if callable {
        let mut f = function(1, "declare_callbacks");
        for (i, name) in ["F", "G"].into_iter().enumerate() {
            f.parameters
                .push(parameter(1, name, Type::Named(name.into())));
            f.callable_parameters.push(CoreCallableParameter {
                index: i as u32,
                generic_index: i as u32,
                generic_name: name.into(),
                kind: CallableKind::Fn,
                parameters: Vec::new(),
                result: Type::I32,
            });
        }
        modules[1].core.functions.push(f);
    }
    if state {
        let mut f = function(2, "declare_state");
        f.parameters
            .push(parameter(2, "owner", Type::Named(STATE.into())));
        f.state_parameters.push(CoreStateParameter {
            index: 0,
            bindings: ["F", "G"]
                .into_iter()
                .enumerate()
                .map(|(i, name)| CoreStateBinding {
                    generic_index: i as u32,
                    generic_name: name.into(),
                    kind: if i == 0 {
                        CallableKind::FnMut
                    } else {
                        CallableKind::FnOnce
                    },
                    parameters: Vec::new(),
                    result: Type::I32,
                    fields: vec![i as u32 + 1],
                })
                .collect(),
        });
        modules[2].core.functions.push(f);
    }
    let mut f = function(3, "untrusted_store_shape");
    f.return_type = Type::Desc(Box::new(Type::OwnedStore {
        element: Box::new(Type::Named(STATE.into())),
    }));
    modules[3].core.functions.push(f);
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules,
    }
}

fn assert_store_shape(program: &CoreProgram) {
    let module = program
        .modules
        .iter()
        .find(|module| module.core.module_id == ModuleId(3))
        .unwrap();
    let Type::Desc(payload) = &module.core.functions[0].return_type else {
        panic!("missing Store descriptor");
    };
    let Type::OwnedStore { element } = payload.as_ref() else {
        panic!("missing Store payload");
    };
    let Type::Package { fields, .. } = element.as_ref() else {
        panic!("missing resolved State");
    };
    assert_eq!(fields[0].1, Type::Managed(Box::new(Type::ByteBuffer)));
    assert_eq!(fields[1].1, Type::Named("F".into()));
    assert_eq!(fields[2].1, Type::Named("G".into()));
}

#[test]
fn project_collects_callable_and_state_names_from_all_modules() {
    for (callable, state) in [(true, false), (false, true), (true, true)] {
        let mut input = project(callable, state);
        for _ in 0..4 {
            let resolved = zil_types::resolve_program_shapes(&input, &Limits::default()).unwrap();
            assert_store_shape(&resolved);
            if state {
                let function = &resolved
                    .modules
                    .iter()
                    .find(|module| module.core.module_id == ModuleId(2))
                    .unwrap()
                    .core
                    .functions[0];
                assert!(matches!(function.parameters[0].ty, Type::Package { .. }));
                let bindings = &function.state_parameters[0].bindings;
                assert_eq!(bindings[0].kind, CallableKind::FnMut);
                assert_eq!(bindings[1].kind, CallableKind::FnOnce);
                assert_eq!(bindings[0].fields, vec![1]);
                assert_eq!(bindings[1].fields, vec![2]);
            }
            input.modules.rotate_left(1);
        }
    }
}

#[test]
fn standalone_inference_keeps_its_local_state_declarations() {
    let mut input = project(false, true);
    let mut core = input.modules.remove(0).core;
    core.functions = input.modules.remove(1).core.functions;
    let inferred = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    assert!(matches!(
        inferred.functions[0].parameters[0].ty,
        Type::Package { .. }
    ));
    assert_eq!(inferred.functions[0].state_parameters[0].bindings.len(), 2);
}

#[test]
fn a_previous_request_cannot_supply_missing_callable_declarations() {
    let accepted =
        zil_types::resolve_program_shapes(&project(true, true), &Limits::default()).unwrap();
    assert_store_shape(&accepted);
    let errors =
        zil_types::resolve_program_shapes(&project(false, false), &Limits::default()).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Types);
    assert_eq!(errors[0].category, ResultCategory::Unsupported);
    assert_eq!(
        errors[0].message,
        "aggregate field requires finite Copy, an explicit package manages resource, or a complete fixed Package array"
    );
    assert_eq!(errors[0].span, Some(span(0, 2)));
    assert_eq!(
        zil_types::resolve_program_shapes(&project(true, true), &Limits::default()).unwrap(),
        accepted
    );
}

#[test]
fn declaration_errors_keep_precedence_over_later_invalid_shapes() {
    let mut input = project(true, true);
    let repeated = input.modules[0].core.aggregates[0].clone();
    input.modules[0].core.aggregates.push(repeated);
    input.modules[3].core.functions[0].return_type = Type::Named("Missing".into());
    let errors = zil_types::resolve_program_shapes(&input, &Limits::default()).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Types);
    assert_eq!(errors[0].code, "E-NAME-DUPLICATE");
    assert_eq!(errors[0].message, "duplicate aggregate name");
    assert_eq!(errors[0].span, Some(span(0, 0)));
}

#[test]
fn symbolic_store_and_state_have_exact_fresh_analysis_budgets() {
    // Frozen on the unchanged Types implementation before sharing its lookup.
    for (callable, state, boundary) in [(true, false, 273), (false, true, 707), (true, true, 780)] {
        let input = project(callable, state);
        let limits = Limits {
            max_analysis_steps: boundary,
            ..Limits::default()
        };
        let accepted = zil_types::resolve_program_shapes(&input, &limits).unwrap();
        let errors = zil_types::resolve_program_shapes(
            &input,
            &Limits {
                max_analysis_steps: boundary - 1,
                ..limits.clone()
            },
        )
        .unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].phase, Phase::Types);
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].message, "factory dependency budget exceeded");
        assert_eq!(errors[0].span, Some(span(3, 10)));
        assert_eq!(
            zil_types::resolve_program_shapes(&input, &limits).unwrap(),
            accepted
        );
    }
}

#[test]
fn aggregate_and_enum_index_charges_remain_per_module() {
    let mut input = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: (0..8).map(module).collect(),
    };
    for (i, module) in input.modules.iter_mut().enumerate() {
        module.core.aggregates.push(CoreAggregate {
            id: NodeId(0),
            name: format!("$module{i}_Cell"),
            kind: AggregateKind::Struct,
            teardown: None,
            fields: vec![CoreField {
                name: "value".into(),
                ty: Type::I64,
                public: true,
                manages: false,
                span: span(i as u32, 1),
            }],
            span: span(i as u32, 0),
        });
        module.core.enums.push(CoreEnum {
            id: NodeId(1),
            name: format!("$module{i}_Tag"),
            variants: vec![("Some".into(), vec![Type::Named(format!("$module{i}_Cell"))])],
            span: span(i as u32, 2),
        });
    }
    // This boundary is dominated by aggregate resolution, including the
    // unchanged per-module project declaration/enum index charges.
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
    for module in &accepted.modules {
        assert!(
            matches!(&module.core.enums[0].variants[0].1[0], Type::Record { name, .. } if name == &format!("@module{}:$module{}_Cell", module.core.module_id.0, module.core.module_id.0))
        );
    }
    assert_eq!(
        zil_types::resolve_program_shapes(&input, &limits).unwrap(),
        accepted
    );
}
