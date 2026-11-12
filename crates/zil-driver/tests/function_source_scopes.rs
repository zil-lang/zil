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

// Raw Core supplies its own original-scope record and an independent public
// bound claim. This is a Core boundary control; it does not replace the original
// explicitly annotated Source fixture, whose producer is tested separately.
fn raw_return_bridge() -> CoreModule {
    let text = "fn make<life a,life b>(value:&'a mut i32,other:&'b mut i32)->impl FnOnce()->&mut i32{return move()=>give value} fn main()->i32{var value=20;var spare=0;let f=make(&mut value,&mut spare);let pointer=f();*pointer+=2;return *pointer-22}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw-return-bridge.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let mut core = zil_expand::expand(&source, &syntax).unwrap();
    let baseline =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let actual = baseline
        .functions
        .iter()
        .find(|f| f.name == "make")
        .unwrap();
    let mut header = actual.storage_life.clone().unwrap();
    header
        .classes
        .retain(|row| row.position.base != StorageBase::Result);
    let inputs = header
        .bindings
        .iter()
        .filter_map(|row| {
            let StorageBase::Parameter(parameter) = row.position.base else {
                return None;
            };
            Some(CoreClosureInputAnchor {
                parameter,
                source_parameter: Some(parameter),
                header_binder: row.binder,
                position: CoreClosureHeaderPosition::Borrow(row.position.clone()),
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(inputs.len(), 2);
    let mut owner = scope();
    owner.parameters = vec![LifeParameterKind::Lifetime, LifeParameterKind::Lifetime];
    owner.inputs = inputs;
    let function = core
        .functions
        .iter_mut()
        .find(|f| f.name == "make")
        .unwrap();
    // Raw Core selects one complete rich relation, not a second independent
    // flat authority alongside it. Original formals remain in the owner scope.
    function.life = None;
    function.storage_life = Some(header);
    function.source_scope = Some(owner.clone());
    let bound = function.callable_return.as_mut().unwrap();
    bound.life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 0,
        bindings: vec![StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
            binder: StorageBinder::Fixed(0),
        }],
        classes: vec![],
    });
    bound.source_life = Some(CoreCallableReturnLifeContext {
        declaration: owner.declaration,
        identity: owner.identity.clone(),
        parameters: owner.parameters.clone(),
        fixed: vec![CoreCallableReturnFixed {
            fixed: 0,
            input: owner.inputs[0].clone(),
        }],
    });
    core
}
#[test]
fn raw_return_bridge_matches_the_actual_owner_scope_and_all_original_input_occurrences() {
    let core = raw_return_bridge();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let f = module.functions.iter().find(|f| f.name == "make").unwrap();
    assert!(f.callable_return_contract.as_ref().unwrap().life.is_some());
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    zil_analyze::analyze(&decoded, &limits).unwrap();
    for attack in 0..10 {
        let mut bad = core.clone();
        let f = bad.functions.iter_mut().find(|f| f.name == "make").unwrap();
        let claim = f
            .callable_return
            .as_mut()
            .unwrap()
            .source_life
            .as_mut()
            .unwrap();
        match attack {
            0 => claim.declaration.declaration = NodeId(8),
            1 => claim.identity.package = "other_original".into(),
            2 => claim.parameters.push(LifeParameterKind::Lifetime),
            3 => {
                let mut other = f.source_scope.as_ref().unwrap().inputs[1].clone();
                other.source_parameter = Some(0);
                claim.fixed[0].input = other;
            }
            4 => claim.fixed[0].input = f.source_scope.as_ref().unwrap().inputs[1].clone(),
            5 => f.source_scope.as_mut().unwrap().inputs.clear(),
            6 => {
                let scope = f.source_scope.as_mut().unwrap();
                scope.inputs.push(scope.inputs[0].clone());
            }
            7 => f.source_scope = None,
            8 => {
                f.source_scope.as_mut().unwrap().inputs[0].position =
                    CoreClosureHeaderPosition::Class(SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![],
                        class: 0,
                    })
            }
            _ => {
                f.source_scope.as_mut().unwrap().inputs[0].header_binder =
                    StorageBinder::Invocation(1);
                claim.fixed[0].input.header_binder = StorageBinder::Invocation(1);
            }
        }
        let errors = zil_types::check(&bad, &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| matches!(
                e.code.as_str(),
                "E-CALLABLE-RETURN-SCOPE" | "E-CALLABLE-RETURN-LIFE"
            )),
            "attack {attack}: {errors:?}"
        );
    }
}
