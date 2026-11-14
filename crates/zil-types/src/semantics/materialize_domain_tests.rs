use super::*;

fn evaluate(recipe: SemanticTypeRecipe) -> SemanticTypeValue {
    let scope = SemanticInstanceId::default();
    let definition = SemanticDefinitionId::default();
    let program = SemanticModule {
        definitions: vec![SemanticDefinition {
            id: definition,
            identity: LifeDeclarationIdentity {
                package: "test".into(),
                module_path: vec![],
                declaration_path: vec!["joint".into()],
                generation_path: vec![],
            },
            parent: None,
            kind: SemanticDefinitionKind::Function,
            parameters: vec![SemanticFormal::Lifetime],
            implicit_lifetimes: vec![],
            requirements: vec![],
            environment: vec![],
            body: SemanticDefinitionBody::Function {
                signature: SemanticSignatureRecipe {
                    bound_lifetimes: 0,
                    invocation_lifetimes: 0,
                    parameters: vec![],
                    result: SemanticTypeRecipe::Atom(Type::Unit),
                    result_mode: PassingMode::Infer,
                    call_kind: CallKind::Direct,
                },
                sites: vec![],
            },
        }],
        instances: vec![SemanticInstance {
            id: scope,
            definition,
            target: SemanticInstanceTarget::Scope,
            lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
            parent: None,
            actuals: vec![],
            header_embedding: vec![],
            environment: None,
            self_type: None,
        }],
        sites: vec![],
        dyn_scopes: vec![],
    };
    let module = TypedModule {
        semantic: None,
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: zil_base::ModuleId(0),
        source_fingerprint: zil_base::Fingerprint::ZERO,
        target: TargetProfile::default(),
        functions: vec![],
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    };
    let ledger = Ledger::default();
    let limits = Limits::default();
    let mut left = limits.max_analysis_steps;
    Eval {
        input: &program,
        module: &ActualDirectory::from_module(&module),
        ledger: &ledger,
        limits: &limits,
        left: &mut left,
        active: BTreeSet::new(),
    }
    .ty(scope, &recipe, None, 0)
    .unwrap()
}
fn address(inner: SemanticTypeRecipe) -> SemanticTypeRecipe {
    SemanticTypeRecipe::Address {
        mutable: false,
        lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        })),
        inner: Box::new(inner),
    }
}

#[test]
fn fresh_tuple_shares_original_outer_life_and_keeps_both_places() {
    let value = evaluate(SemanticTypeRecipe::Tuple(vec![
        address(SemanticTypeRecipe::Atom(Type::I32)),
        address(SemanticTypeRecipe::Atom(Type::Bool)),
    ]));
    let SemanticTypeValue::Resolved { life, .. } = value else {
        panic!()
    };
    assert_eq!(life.binder_count, 1);
    assert_eq!(
        life.slots,
        vec![
            StorageLifeSlot {
                path: vec![PlaceProjection::Field(0)],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress
            },
            StorageLifeSlot {
                path: vec![PlaceProjection::Field(1)],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress
            }
        ]
    );
}

#[test]
fn sibling_pointee_boundaries_keep_their_complete_occurrences() {
    let nested = address(address(SemanticTypeRecipe::Atom(Type::I32)));
    let value = evaluate(SemanticTypeRecipe::Tuple(vec![nested.clone(), nested]));
    let SemanticTypeValue::Resolved { life, .. } = value else {
        panic!()
    };
    assert_eq!(life.binder_count, 3);
    assert_eq!(
        life.slots.iter().map(|row| row.binder).collect::<Vec<_>>(),
        vec![0, 0]
    );
    assert_eq!(
        life.retained
            .iter()
            .map(|row| row.binder)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn a_frozen_domain_keeps_two_indices_with_one_outside_image() {
    let frozen_life = StorageLifeShape {
        binder_count: 2,
        slots: (0..2)
            .map(|binder| StorageLifeSlot {
                path: vec![PlaceProjection::Field(binder)],
                binder,
                mode: StoredBorrowMode::SharedAddress,
            })
            .collect(),
        retained: vec![],
        nominal_arguments: vec![],
    };
    let ty = Type::BorrowedStorage {
        storage: Box::new(Type::Tuple(vec![
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Bool),
            },
        ])),
        life: frozen_life.clone(),
    };
    let exact = SemanticTypeRecipe::Exact(Box::new(SemanticExactTypeRecipe {
        ty,
        life: frozen_life,
        classes: vec![
            SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: 0
            });
            2
        ],
    }));
    let value = evaluate(SemanticTypeRecipe::Tuple(vec![
        exact,
        SemanticTypeRecipe::Atom(Type::I32),
    ]));
    let SemanticTypeValue::Resolved { life, classes, .. } = value else {
        panic!()
    };
    assert_eq!(life.binder_count, 2);
    assert_eq!(classes[0], classes[1]);
    assert_eq!(
        life.retained
            .iter()
            .map(|row| (row.class, row.binder))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 1)]
    );
}
