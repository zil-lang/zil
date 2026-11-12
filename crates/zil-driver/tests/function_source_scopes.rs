//! Raw metadata transport/prepay controls. These tests do not certify a source
//! declaration or use the metadata as any lifetime or capability proof.
use zil_base::{FileId, Limits, ModuleId, NodeId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;
fn input() -> CoreModule {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "scope-transport.zil",
        "fn main()->i32{return 0}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &syntax).unwrap()
}
fn scope() -> CoreFunctionSourceScope {
    CoreFunctionSourceScope {
        declaration: LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(7),
            compiler: false,
        },
        identity: LifeDeclarationIdentity {
            package: "scope_transport".into(),
            module_path: vec!["model".into()],
            declaration_path: vec!["original".into()],
            generation_path: vec![],
        },
        parameters: vec![
            LifeParameterKind::Lifetime,
            LifeParameterKind::Type,
            LifeParameterKind::Const(Type::Tuple(vec![Type::I32, Type::Usize])),
        ],
        inputs: vec![],
    }
}
#[test]
fn raw_original_scope_survives_ct_and_uses_an_exact_node_budget() {
    let mut core = input();
    core.functions[0].source_scope = Some(scope());
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let (output, usage) =
        zil_ct::evaluate_source_constants_with_usage(&core, &target, &limits).unwrap();
    assert_eq!(
        output.functions[0].source_scope,
        core.functions[0].source_scope
    );
    zil_types::check(&output, &target, &limits).unwrap();
    let exact = Limits {
        max_nodes: usage.nodes,
        ..limits.clone()
    };
    assert_eq!(
        zil_ct::evaluate_source_constants_with_usage(&core, &target, &exact).unwrap(),
        (output, usage)
    );
    assert_eq!(
        zil_ct::evaluate_source_constants(
            &core,
            &target,
            &Limits {
                max_nodes: usage.nodes - 1,
                ..limits
            }
        )
        .unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
}
#[test]
fn original_scope_directories_and_const_types_are_bounded_before_copy() {
    let limits = Limits {
        max_nodes: 64,
        max_call_depth: 16,
        ..Limits::default()
    };
    let target = TargetProfile::host_llvm();
    for deep in [false, true] {
        let mut core = input();
        let mut metadata = scope();
        if deep {
            let mut ty = Type::I32;
            for _ in 0..32 {
                ty = Type::Array {
                    element: Box::new(ty),
                    length: 1,
                };
            }
            metadata.parameters.push(LifeParameterKind::Const(ty));
        } else {
            metadata.inputs = vec![
                CoreClosureInputAnchor {
                    parameter: 0,
                    source_parameter: Some(0),
                    header_binder: StorageBinder::Invocation(0),
                    position: CoreClosureHeaderPosition::Borrow(StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![]
                    })
                };
                65
            ];
        }
        core.functions[0].source_scope = Some(metadata);
        assert_eq!(
            zil_ct::evaluate_source_constants(&core, &target, &limits).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        assert_eq!(
            zil_types::check(&core, &target, &limits).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
    }
}
#[test]
fn scope_structural_cost_stops_at_the_actual_remaining_work_and_nodes() {
    let scope = scope();
    let (nodes, work) = core_function_scope_metadata_cost(&scope, 100, 16, 10000).unwrap();
    assert_eq!(
        core_function_scope_metadata_cost(&scope, nodes, 16, work),
        Some((nodes, work))
    );
    assert_eq!(
        core_function_scope_metadata_cost(&scope, nodes - 1, 16, work),
        None
    );
    assert_eq!(
        core_function_scope_metadata_cost(&scope, nodes, 16, work - 1),
        None
    );
    assert_eq!(
        core_function_scope_metadata_cost(&scope, nodes, 16, 0),
        None
    );
}
