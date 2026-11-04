// Shared untrusted raw fixture; contains no proof implementation.
use zil_base::{FileId, Fingerprint, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;
pub(super) fn address(t: Type, mutable: bool) -> Type {
    Type::Address {
        mutable,
        inner: Box::new(t),
    }
}
fn storage() -> Type {
    Type::BorrowedStorage {
        storage: Box::new(Type::Record {
            name: "View".into(),
            fields: vec![("pointer".into(), address(Type::I64, false))],
        }),
        life: StorageLifeShape {
            binder_count: 1,
            slots: vec![StorageLifeSlot {
                path: vec![PlaceProjection::Field(0)],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress,
            }],
            retained: vec![],
            nominal_arguments: vec![NominalLifeArgument {
                path: vec![],
                declaration: declaration(),
                argument: 0,
                binder: 0,
            }],
        },
    }
}
fn declaration() -> LifeDeclarationId {
    LifeDeclarationId {
        module: ModuleId(0),
        declaration: NodeId(1),
        compiler: false,
    }
}
fn s() -> Span {
    Span::new(FileId(0), 0, 1)
}
pub(super) fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: s(),
    }
}
fn function(
    id: u32,
    parameters: Vec<Type>,
    ret: Type,
    values: Vec<Type>,
    instructions: Vec<Instruction>,
    result: u32,
) -> TypedFunction {
    TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(id),
        name: format!("function{id}"),
        foreign: None,
        environment: None,
        parameters: (0..parameters.len()).map(|i| LocalId(i as u32)).collect(),
        return_type: ret,
        result_mode: PassingMode::Infer,
        locals: parameters
            .into_iter()
            .enumerate()
            .map(|(i, ty)| LocalDecl {
                id: LocalId(i as u32),
                name: format!("parameter{i}"),
                ty,
                mutable: false,
                span: s(),
            })
            .collect(),
        values,
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions,
            terminator: Terminator::Return(Some(ValueId(result))),
        }],
        span: s(),
    }
}
fn row(base: StorageBase, path: Vec<PlaceProjection>, binder: StorageBinder) -> StorageLifeBinding {
    StorageLifeBinding {
        position: StoragePosition {
            base,
            path: path.into_iter().map(ContractProjection::Place).collect(),
        },
        binder,
    }
}
pub(super) fn module() -> TypedModule {
    let shared = address(Type::I64, false);
    let holder = storage();
    let receiver = address(holder.clone(), false);
    let mut make = function(
        0,
        vec![shared.clone()],
        holder.clone(),
        vec![shared.clone(), shared.clone(), holder.clone()],
        vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(
                Some(1),
                InstructionKind::Call {
                    function: FunctionId(4),
                    arguments: vec![ValueId(0)],
                },
            ),
            instruction(
                Some(2),
                InstructionKind::MakeAggregate {
                    fields: vec![ValueId(1)],
                },
            ),
        ],
        2,
    );
    make.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 1,
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(
                StorageBase::Result,
                vec![PlaceProjection::Field(0)],
                StorageBinder::Invocation(0),
            ),
        ],
    });
    let mut get = function(
        1,
        vec![receiver.clone()],
        shared.clone(),
        vec![receiver.clone(), shared.clone()],
        vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(
                Some(1),
                InstructionKind::LoadStoredBorrow {
                    storage: ValueId(0),
                    path: vec![PlaceProjection::Field(0)],
                    mode: StoredBorrowLoad::CopyShared,
                },
            ),
        ],
        1,
    );
    get.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 1,
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(
                StorageBase::Parameter(0),
                vec![PlaceProjection::Field(0)],
                StorageBinder::Fixed(0),
            ),
            row(StorageBase::Result, vec![], StorageBinder::Fixed(0)),
        ],
    });
    let mut relay = function(
        2,
        vec![receiver.clone()],
        shared.clone(),
        vec![receiver.clone(), shared.clone()],
        vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(
                Some(1),
                InstructionKind::Call {
                    function: FunctionId(1),
                    arguments: vec![ValueId(0)],
                },
            ),
        ],
        1,
    );
    relay.storage_life = get.storage_life.clone();
    let mut main = function(
        3,
        vec![],
        Type::I64,
        vec![
            Type::I64,
            shared.clone(),
            holder.clone(),
            receiver,
            shared.clone(),
            Type::I64,
        ],
        vec![
            instruction(Some(0), InstructionKind::Constant(Constant::I64(42))),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
            ),
            instruction(Some(1), InstructionKind::BorrowLocal { local: LocalId(0) }),
            instruction(
                Some(2),
                InstructionKind::Call {
                    function: FunctionId(0),
                    arguments: vec![ValueId(1)],
                },
            ),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(2),
                },
            ),
            instruction(Some(3), InstructionKind::BorrowLocal { local: LocalId(1) }),
            instruction(
                Some(4),
                InstructionKind::Call {
                    function: FunctionId(2),
                    arguments: vec![ValueId(3)],
                },
            ),
            instruction(None, InstructionKind::StorageDead(LocalId(1))),
            instruction(
                Some(5),
                InstructionKind::ReadAddress {
                    address: ValueId(4),
                },
            ),
        ],
        5,
    );
    main.locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "target".into(),
            ty: Type::I64,
            mutable: true,
            span: s(),
        },
        LocalDecl {
            id: LocalId(1),
            name: "holder".into(),
            ty: holder.clone(),
            mutable: true,
            span: s(),
        },
    ];
    let identity = function(
        4,
        vec![shared.clone()],
        shared.clone(),
        vec![shared],
        vec![instruction(Some(0), InstructionKind::Load(LocalId(0)))],
        0,
    );
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::default(),
        functions: vec![make, get, relay, main, identity],
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![LifeNominalDeclaration {
            id: declaration(),
            identity: LifeDeclarationIdentity {
                package: "test".into(),
                module_path: vec![],
                declaration_path: vec!["View".into()],
                generation_path: vec![],
            },
            kind: LifeNominalKind::Record,
            parameters: vec![LifeParameterKind::Lifetime],
        }],
        life_instances: vec![LifeNominalInstance {
            declaration: declaration(),
            ty: holder,
            arguments: vec![LifeNominalActual::Lifetime(0)],
        }],
    }
}

pub(super) fn exclusive_returns(overlap: bool) -> TypedModule {
    let mut m = module();
    let mutable = address(Type::I64, true);
    m.functions.push(function(
        5,
        vec![mutable.clone()],
        mutable.clone(),
        vec![mutable.clone()],
        vec![instruction(Some(0), InstructionKind::Load(LocalId(0)))],
        0,
    ));
    let f = &mut m.functions[3];
    f.values.extend([
        mutable.clone(),
        mutable.clone(),
        mutable,
        Type::I64,
        Type::I64,
    ]);
    let code = &mut f.blocks[0].instructions;
    code.push(instruction(
        Some(6),
        InstructionKind::BorrowLocalMut { local: LocalId(0) },
    ));
    code.push(instruction(
        Some(7),
        InstructionKind::Call {
            function: FunctionId(5),
            arguments: vec![ValueId(6)],
        },
    ));
    if !overlap {
        code.push(instruction(
            Some(9),
            InstructionKind::ReadAddress {
                address: ValueId(7),
            },
        ));
    }
    code.push(instruction(
        Some(8),
        InstructionKind::Call {
            function: FunctionId(5),
            arguments: vec![ValueId(6)],
        },
    ));
    if overlap {
        code.push(instruction(
            Some(9),
            InstructionKind::ReadAddress {
                address: ValueId(7),
            },
        ));
    }
    code.push(instruction(
        Some(10),
        InstructionKind::ReadAddress {
            address: ValueId(8),
        },
    ));
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(10)));
    m
}

#[allow(dead_code)] // Shared raw builder is used by the driver subset only.
pub(super) fn phantom() -> TypedModule {
    let mut m = module();
    let Type::BorrowedStorage { storage, life } = &mut m.life_instances[0].ty else {
        unreachable!()
    };
    *storage = Box::new(Type::Record {
        name: "Phantom".into(),
        fields: vec![("number".into(), Type::I32)],
    });
    life.slots.clear();
    let phantom = m.life_instances[0].ty.clone();
    m.life_declarations[0].identity.declaration_path = vec!["Phantom".into()];
    let mut use_value = function(
        0,
        vec![address(Type::I32, false)],
        Type::I32,
        vec![
            Type::I32,
            phantom.clone(),
            address(phantom.clone(), false),
            address(Type::I32, false),
            Type::I32,
        ],
        vec![
            instruction(Some(0), InstructionKind::Constant(Constant::I32(42))),
            instruction(
                Some(1),
                InstructionKind::MakeAggregate {
                    fields: vec![ValueId(0)],
                },
            ),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(1),
                },
            ),
            instruction(Some(2), InstructionKind::BorrowLocal { local: LocalId(1) }),
            instruction(
                Some(3),
                InstructionKind::AddressField {
                    address: ValueId(2),
                    index: 0,
                },
            ),
            instruction(
                Some(4),
                InstructionKind::ReadAddress {
                    address: ValueId(3),
                },
            ),
        ],
        4,
    );
    use_value.locals.push(LocalDecl {
        id: LocalId(1),
        name: "marker".into(),
        ty: phantom,
        mutable: false,
        span: s(),
    });
    let mut main = function(
        1,
        vec![],
        Type::I32,
        vec![Type::I32, address(Type::I32, false), Type::I32],
        vec![
            instruction(Some(0), InstructionKind::Constant(Constant::I32(0))),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
            ),
            instruction(Some(1), InstructionKind::BorrowLocal { local: LocalId(0) }),
            instruction(
                Some(2),
                InstructionKind::Call {
                    function: FunctionId(0),
                    arguments: vec![ValueId(1)],
                },
            ),
        ],
        2,
    );
    main.locals.push(LocalDecl {
        id: LocalId(0),
        name: "actual".into(),
        ty: Type::I32,
        mutable: false,
        span: s(),
    });
    m.functions = vec![use_value, main];
    m
}

#[allow(dead_code)]
pub(super) fn unimplemented_unreachable() -> TypedModule {
    let mut m = module();
    m.functions[0].blocks[0].instructions[1].kind = InstructionKind::ReborrowAddress {
        address: ValueId(0),
        mutable: false,
    };
    m.functions[4] = function(
        4,
        vec![Type::I64; 64],
        Type::I64,
        vec![Type::I64],
        vec![instruction(
            Some(0),
            InstructionKind::Constant(Constant::I64(42)),
        )],
        0,
    );
    let f = &mut m.functions[3];
    f.values.extend([
        Type::FunctionPointer {
            parameters: vec![Type::I64; 64],
            result: Box::new(Type::I64),
        },
        Type::I64,
    ]);
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![
            instruction(
                Some(6),
                InstructionKind::FunctionRef {
                    function: FunctionId(4),
                },
            ),
            instruction(
                Some(7),
                InstructionKind::CallIndirect {
                    callee: ValueId(6),
                    arguments: vec![ValueId(0); 64],
                },
            ),
        ],
        terminator: Terminator::Unreachable,
    });
    m
}

#[allow(dead_code)]
pub(super) fn wide_shared(width: usize) -> TypedModule {
    fn qualify(ty: &mut Type, width: usize) {
        match ty {
            Type::BorrowedStorage { storage, .. } => {
                let Type::Record { fields, .. } = storage.as_mut() else {
                    unreachable!()
                };
                for n in 0..width {
                    fields.push((format!("padding{n}"), Type::I64));
                }
            }
            Type::Address { inner, .. } => qualify(inner, width),
            _ => {}
        }
    }
    let mut m = module();
    qualify(&mut m.life_instances[0].ty, width);
    for f in &mut m.functions {
        qualify(&mut f.return_type, width);
        for l in &mut f.locals {
            qualify(&mut l.ty, width);
        }
        for t in &mut f.values {
            qualify(t, width);
        }
    }
    let make = &mut m.functions[0];
    make.values.push(Type::I64);
    make.blocks[0].instructions.insert(
        2,
        instruction(Some(3), InstructionKind::Constant(Constant::I64(0))),
    );
    let InstructionKind::MakeAggregate { fields } = &mut make.blocks[0].instructions[3].kind else {
        unreachable!()
    };
    fields.extend(vec![ValueId(3); width]);
    m
}
