//! Raw responsibility-component tests. Stored target and lifetime admission is
//! deliberately a separate proof; these fixtures never claim whole IR safety.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::{PlaceProjection, StorageLifeShape, StorageLifeSlot, StoredBorrowMode};

fn qualified() -> Type {
    Type::BorrowedStorage {
        storage: Box::new(Type::Package {
            name: "BorrowedOwner".into(),
            fields: vec![
                ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("target".into(), address()),
            ],
        }),
        life: StorageLifeShape {
            binder_count: 1,
            slots: vec![StorageLifeSlot {
                path: vec![PlaceProjection::Field(1)],
                binder: 0,
                mode: StoredBorrowMode::ExclusiveAddress,
            }],
            retained: vec![],
            nominal_arguments: vec![],
        },
    }
}
fn address() -> Type {
    Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    }
}
fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: Span::new(FileId(0), 0, 1),
    }
}
fn fixture() -> TypedModule {
    let span = Span::new(FileId(0), 0, 1);
    let owner = Type::Managed(Box::new(Type::ByteBuffer));
    let function = TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
        reference_return_bound: None,
        id: FunctionId(0),
        name: "assemble".into(),
        foreign: None,
        environment: None,
        callable_contract: None,
        protocol_contract: None,
        callable_return_contract: None,
        parameters: vec![LocalId(0), LocalId(1)],
        return_type: qualified(),
        result_mode: PassingMode::Infer,
        locals: vec![owner.clone(), address(), qualified()]
            .into_iter()
            .enumerate()
            .map(|(index, ty)| LocalDecl {
                id: LocalId(index as u32),
                name: format!("l{index}"),
                ty,
                mutable: true,
                span,
            })
            .collect(),
        values: vec![owner, address(), qualified(), qualified()],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                instruction(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
                instruction(Some(1), InstructionKind::Load(LocalId(1))),
                instruction(
                    Some(2),
                    InstructionKind::MakePackage {
                        fields: vec![ValueId(0), ValueId(1)],
                    },
                ),
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(2),
                    },
                ),
                instruction(Some(3), InstructionKind::TakeLocal { local: LocalId(2) }),
            ],
            terminator: Terminator::Return(Some(ValueId(3))),
        }],
        entry: BlockId(0),
        span,
    };
    TypedModule {
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![function],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "ByteBuffer".into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn check(module: &TypedModule) -> CompileResult<Effect> {
    analyze_function(
        module,
        &module.functions[0],
        &[true],
        &[Effect::default()],
        true,
        &mut Budget::new(&Limits::default()),
    )
}

#[test]
fn qualified_package_assembly_move_and_return_conserve_real_owner() {
    let module = fixture();
    check(&module).unwrap();
    let f = &module.functions[0];
    let mut budget = Budget::new(&Limits::default());
    let cells = shapes(&module, f, &mut budget).unwrap();
    assert!(cells[&Place::Local(LocalId(2))].owned);
    assert!(
        cells[&Place::PackageField {
            local: LocalId(2),
            index: 0
        }]
            .owned
    );
    assert!(!cells.contains_key(&Place::PackageField {
        local: LocalId(2),
        index: 1
    }));
}

#[test]
fn qualified_package_cleanup_discharges_fields_and_cannot_repeat() {
    let mut module = fixture();
    let f = &mut module.functions[0];
    f.return_type = Type::Unit;
    f.blocks[0].instructions.pop();
    f.blocks[0].instructions.push(instruction(
        None,
        InstructionKind::DropPackage { local: LocalId(2) },
    ));
    f.blocks[0].terminator = Terminator::Return(None);
    check(&module).unwrap();
    module.functions[0].blocks[0].instructions.push(instruction(
        None,
        InstructionKind::DropPackage { local: LocalId(2) },
    ));
    assert!(
        check(&module)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("duplicate automatic cleanup"))
    );
}

#[test]
fn qualified_package_cannot_duplicate_or_discard_transferred_debt() {
    let mut duplicate = fixture();
    duplicate.functions[0].blocks[0].instructions.insert(
        4,
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(2),
                value: ValueId(2),
            },
        ),
    );
    assert!(check(&duplicate).is_err());
    let mut leak = fixture();
    let f = &mut leak.functions[0];
    f.return_type = Type::Unit;
    f.blocks[0].instructions.pop();
    f.blocks[0].terminator = Terminator::Return(None);
    assert!(
        check(&leak)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("unpaid"))
    );
}

#[test]
fn qualified_package_partial_take_cannot_move_whole_shell() {
    let mut module = fixture();
    let f = &mut module.functions[0];
    f.values.push(Type::Managed(Box::new(Type::ByteBuffer)));
    f.blocks[0].instructions.insert(
        4,
        instruction(
            Some(4),
            InstructionKind::TakePackageField {
                local: LocalId(2),
                index: 0,
            },
        ),
    );
    assert!(
        check(&module)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("every owned element or field live"))
    );
}

#[test]
fn borrowed_field_qualification_does_not_grant_copy_or_fresh_birth() {
    let mut budget = Budget::new(&Limits::default());
    product::admit(&qualified(), &mut budget).unwrap();
    assert!(
        product::admit(
            package_storage(&qualified()),
            &mut Budget::new(&Limits::default())
        )
        .is_err()
    );
    let mut module = fixture();
    module.functions[0].result_mode = PassingMode::Create;
    assert!(
        check(&module)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("incoming responsibility"))
    );
}
