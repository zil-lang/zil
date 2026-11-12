use super::*;
use zil_base::{FileId, Span};
use zil_ir::{BasicBlock, BlockId, ClosureId, LocalDecl};

#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
    ));
}

fn module() -> TypedModule {
    let mut module = fixture::module();
    let input = Type::Address {
        mutable: false,
        inner: Box::new(Type::I64),
    };
    let bytes = Type::Managed(Box::new(Type::ByteBuffer));
    let result = Type::Package {
        name: "Reply".into(),
        fields: vec![("data".into(), bytes.clone())],
    };
    let mut entry = module.functions[0].clone();
    entry.storage_life = None;
    entry.environment = Some(ClosureId(0));
    entry.return_type = result.clone();
    entry.parameters = vec![LocalId(0)];
    entry.locals = vec![LocalDecl {
        id: LocalId(0),
        name: "input".into(),
        ty: input.clone(),
        mutable: false,
        span: Span::new(FileId(0), 0, 1),
    }]
    .into();
    entry.values = vec![bytes, result.clone()].into();
    entry.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            fixture::instruction(Some(0), InstructionKind::ByteBufferNew),
            fixture::instruction(
                Some(1),
                InstructionKind::MakePackage {
                    fields: vec![ValueId(0)],
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(1))),
    }]
    .into();
    module.functions = vec![entry];
    module.closures = vec![ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(0),
        captures: vec![],
        parameters: vec![input],
        result,
        kind: zil_ir::CallableKind::FnMut,
        lending_captures: vec![],
    }];
    module.resources = vec![zil_ir::ResourceDefinition {
        name: "@builtin:ByteBuffer".into(),
        payload: Type::ByteBuffer,
        teardown: None,
    }];
    module.life_declarations.clear();
    module.life_instances.clear();
    module
}

#[test]
fn zero_option_masked_owned_entry_rechecks_actual_bounds_and_parameters() {
    let module = module();
    // This near control authenticates the entry, not the later executable
    // Package profile. No Option lane or producer analysis grants authority.
    super::super::stored_callables::owned_entry(
        &module,
        &module.closures[0],
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    assert!(module.functions.iter().all(|f| {
        f.locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
            .all(|ty| !matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }))
    }));
    let graph = FunctionGraph {
        successors: vec![vec![]],
        predecessors: vec![vec![]],
        reachable: vec![true],
        package_views: packages::CaptureViews::default(),
        role_data_uses: vec![],
    };
    let mut scheduling = Budget::new(&Limits::default());
    assert!(!scheduling.store_option_results);
    let mask = module
        .functions
        .iter()
        .map(|f| qualified_function(f, &mut scheduling))
        .collect::<Check<Vec<_>>>()
        .unwrap();
    assert_eq!(mask, [true]);
    let mut ordinary = module.functions[0].clone();
    ordinary.environment = None;
    assert!(!qualified_function(&ordinary, &mut scheduling).unwrap());
    assert_eq!(module.functions[0].environment, Some(ClosureId(0)));
    for attack in [
        "result-mode",
        "reference-bound",
        "storage-bound",
        "callable-bound",
        "parameter",
        "result",
        "environment",
    ] {
        let mut raw = module.clone();
        let entry = &mut raw.functions[0];
        match attack {
            "result-mode" => entry.result_mode = PassingMode::Give,
            "reference-bound" => entry.reference_return_bound = Some(vec![0]),
            "storage-bound" => {
                entry.storage_life = Some(zil_ir::StorageLifeContract {
                    fixed_binder_count: 0,
                    invocation_binder_count: 0,
                    bindings: vec![],
                    classes: vec![],
                });
            }
            "callable-bound" => {
                entry.callable_contract = Some(zil_ir::CallableParameterContract {
                    roles: vec![],
                    traces: zil_ir::CallableTraces::Finite(vec![]),
                });
            }
            "parameter" => entry.locals[0].ty = Type::Bool,
            "result" => {
                entry.return_type = Type::Package {
                    name: "OtherReply".into(),
                    fields: vec![("data".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
                };
            }
            "environment" => entry.environment = Some(ClosureId(1)),
            _ => unreachable!(),
        }
        let mut budget = Budget::new(&Limits::default());
        assert!(!budget.store_option_results);
        let mask = raw
            .functions
            .iter()
            .map(|f| qualified_function(f, &mut budget))
            .collect::<Check<Vec<_>>>()
            .unwrap();
        assert_eq!(mask, [true], "actual selected entry at {attack}");
        let plan = CandidatePlan::empty_for(&raw);
        let error = check_mixed(
            &raw,
            std::slice::from_ref(&graph),
            &plan,
            &mask,
            &mut budget,
        )
        .unwrap_err();
        if attack == "environment" {
            assert_eq!(error.code, "E-VERIFY-STORED");
            assert_eq!(
                error.message,
                "stored target proof: callback function environment is absent"
            );
        } else {
            assert_eq!(
                error.code, "E-VERIFY-QUALIFIED-CALLABLE",
                "{attack}: {error:?}"
            );
            assert!(
                error.message == "stored owning callback actual entry differs"
                    || error.message == "stored owning callback actual parameter differs",
                "{attack}: {error:?}"
            );
        }
    }
}
