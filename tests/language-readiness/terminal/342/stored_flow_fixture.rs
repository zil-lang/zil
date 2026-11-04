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
        bindings: vec![row(
            StorageBase::Parameter(0),
            vec![],
            StorageBinder::Invocation(0),
        )],
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Result,
                path: vec![],
                class: 0,
            },
            binder: StorageBinder::Invocation(0),
        }],
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
            row(StorageBase::Result, vec![], StorageBinder::Fixed(0)),
        ],
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        }],
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
    **storage = Type::Record {
        name: "Phantom".into(),
        fields: vec![("number".into(), Type::I32)],
    };
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

#[allow(dead_code)]
pub(super) fn phantom_class_calls(indirect: bool, reuse: bool) -> TypedModule {
    let mut m = phantom();
    let marker = m.life_instances[0].ty.clone();
    let classes = |base: StorageBase, binder: StorageBinder| SignatureLifeClassBinding {
        position: SignatureLifeClass {
            base,
            path: vec![],
            class: 0,
        },
        binder,
    };
    let mut require = function(
        0,
        vec![marker.clone(), marker.clone()],
        Type::I32,
        vec![Type::I32],
        vec![instruction(
            Some(0),
            InstructionKind::Constant(Constant::I32(42)),
        )],
        0,
    );
    require.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 0,
        bindings: vec![],
        classes: vec![
            classes(StorageBase::Parameter(0), StorageBinder::Fixed(0)),
            classes(StorageBase::Parameter(1), StorageBinder::Fixed(0)),
        ],
    });
    let pointer_life = StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 1,
        bindings: vec![],
        classes: vec![
            classes(StorageBase::Parameter(0), StorageBinder::Invocation(0)),
            classes(StorageBase::Parameter(1), StorageBinder::Invocation(0)),
        ],
    };
    let pointer = Type::BorrowedFunctionPointer {
        parameters: vec![marker.clone(), marker.clone()],
        result: Box::new(Type::I32),
        life: pointer_life,
    };
    let mut body = vec![
        instruction(Some(0), InstructionKind::Load(LocalId(0))),
        instruction(Some(1), InstructionKind::Load(LocalId(1))),
        instruction(Some(2), InstructionKind::Constant(Constant::I32(42))),
        instruction(
            Some(3),
            InstructionKind::MakeAggregate {
                fields: vec![ValueId(2)],
            },
        ),
        instruction(
            Some(5),
            InstructionKind::MakeAggregate {
                fields: vec![ValueId(2)],
            },
        ),
    ];
    if indirect {
        body.push(instruction(
            Some(7),
            InstructionKind::FunctionRef {
                function: FunctionId(0),
            },
        ));
    }
    let call = |arguments: Vec<ValueId>| {
        if indirect {
            InstructionKind::CallIndirect {
                callee: ValueId(7),
                arguments,
            }
        } else {
            InstructionKind::Call {
                function: FunctionId(0),
                arguments,
            }
        }
    };
    body.push(instruction(Some(4), call(vec![ValueId(0), ValueId(3)])));
    body.push(instruction(
        Some(6),
        call(vec![ValueId(1), ValueId(if reuse { 3 } else { 5 })]),
    ));
    let mut values = vec![
        marker.clone(),
        marker.clone(),
        Type::I32,
        marker.clone(),
        Type::I32,
        marker.clone(),
        Type::I32,
    ];
    if indirect {
        values.push(pointer);
    }
    let mut caller = function(1, vec![marker.clone(), marker], Type::I32, values, body, 6);
    caller.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 2,
        invocation_binder_count: 0,
        bindings: vec![],
        classes: vec![
            classes(StorageBase::Parameter(0), StorageBinder::Fixed(0)),
            classes(StorageBase::Parameter(1), StorageBinder::Fixed(1)),
        ],
    });
    m.functions = vec![require, caller];
    m
}

#[allow(dead_code)]
pub(super) fn rich_marker_pointer(wrong_actual: bool, wrong_target: bool) -> TypedModule {
    let mut m = phantom();
    let marker = m.life_instances[0].ty.clone();
    let tag = address(marker, false);
    let number = address(Type::I32, false);
    let class = |binder| SignatureLifeClassBinding {
        position: SignatureLifeClass {
            base: StorageBase::Parameter(0),
            path: vec![LifeTypeProjection::Pointee],
            class: 0,
        },
        binder,
    };
    let mut target = function(
        0,
        vec![tag.clone(), number.clone()],
        Type::I32,
        vec![Type::I32],
        vec![instruction(
            Some(0),
            InstructionKind::Constant(Constant::I32(42)),
        )],
        0,
    );
    target.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 1,
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(StorageBase::Parameter(1), vec![], StorageBinder::Fixed(0)),
        ],
        classes: vec![class(StorageBinder::Fixed(0))],
    });
    let pointer = Type::BorrowedFunctionPointer {
        parameters: vec![tag.clone(), number.clone()],
        result: Box::new(Type::I32),
        life: StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 2,
            bindings: vec![
                row(
                    StorageBase::Parameter(0),
                    vec![],
                    StorageBinder::Invocation(0),
                ),
                row(
                    StorageBase::Parameter(1),
                    vec![],
                    StorageBinder::Invocation(1),
                ),
            ],
            classes: vec![class(StorageBinder::Invocation(if wrong_target {
                0
            } else {
                1
            }))],
        },
    };
    let mut caller = function(
        1,
        vec![tag.clone(), number.clone()],
        Type::I32,
        vec![tag, number, pointer, Type::I32],
        vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(Some(1), InstructionKind::Load(LocalId(1))),
            instruction(
                Some(2),
                InstructionKind::FunctionRef {
                    function: FunctionId(0),
                },
            ),
            instruction(
                Some(3),
                InstructionKind::CallIndirect {
                    callee: ValueId(2),
                    arguments: vec![ValueId(0), ValueId(1)],
                },
            ),
        ],
        3,
    );
    caller.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: if wrong_actual { 2 } else { 1 },
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(
                StorageBase::Parameter(1),
                vec![],
                if wrong_actual {
                    StorageBinder::Invocation(1)
                } else {
                    StorageBinder::Fixed(0)
                },
            ),
        ],
        classes: vec![class(StorageBinder::Fixed(0))],
    });
    m.functions = vec![target, caller];
    m
}

#[allow(dead_code)]
pub(super) fn class_candidate(m: &TypedModule) -> CandidatePlan {
    let mut c = CandidatePlan::empty_for(m);
    for f in &m.functions {
        let mut holds = (0..f.values.len())
            .map(|i| ValueHold {
                value: ValueId(i as u32),
                roots: vec![],
            })
            .collect::<Vec<_>>();
        for instruction in &f.blocks[0].instructions {
            if let (Some(v), InstructionKind::Load(local)) = (instruction.result, &instruction.kind)
                && matches!(f.values[v.0 as usize], Type::Address { .. })
                && let Some(p) = f.parameters.iter().position(|p| p == local)
            {
                holds[v.0 as usize].roots = vec![HoldRoot::Parameter(p as u32)];
            }
        }
        c.functions.push(FunctionPlan {
            function: f.id,
            holds,
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: f.parameters.clone(),
                initialized_out: f.parameters.clone(),
            }],
        });
    }
    c
}

#[allow(dead_code)]
pub(super) fn rich_marker_exclusive_call(alias: bool) -> TypedModule {
    let mut m = rich_marker_pointer(false, false);
    let mutable = address(Type::I32, true);
    let target = &mut m.functions[0];
    target.locals[1].ty = mutable.clone();
    target.locals.push(LocalDecl {
        id: LocalId(2),
        name: "other".into(),
        ty: mutable.clone(),
        mutable: false,
        span: s(),
    });
    target.parameters.push(LocalId(2));
    let life = target.storage_life.as_mut().unwrap();
    life.invocation_binder_count = 2;
    life.bindings.push(row(
        StorageBase::Parameter(2),
        vec![],
        StorageBinder::Invocation(1),
    ));
    let caller = &mut m.functions[1];
    caller.locals[1].ty = mutable.clone();
    caller.values[1] = mutable.clone();
    caller.locals.push(LocalDecl {
        id: LocalId(2),
        name: "other".into(),
        ty: mutable.clone(),
        mutable: false,
        span: s(),
    });
    caller.parameters.push(LocalId(2));
    caller.values.push(mutable.clone());
    let life = caller.storage_life.as_mut().unwrap();
    life.invocation_binder_count = 2;
    life.bindings.push(row(
        StorageBase::Parameter(2),
        vec![],
        StorageBinder::Invocation(1),
    ));
    let Type::BorrowedFunctionPointer {
        parameters, life, ..
    } = &mut caller.values[2]
    else {
        unreachable!()
    };
    parameters[1] = mutable.clone();
    parameters.push(mutable);
    life.invocation_binder_count = 3;
    life.bindings.push(row(
        StorageBase::Parameter(2),
        vec![],
        StorageBinder::Invocation(2),
    ));
    caller.blocks[0]
        .instructions
        .insert(2, instruction(Some(4), InstructionKind::Load(LocalId(2))));
    let InstructionKind::CallIndirect { arguments, .. } =
        &mut caller.blocks[0].instructions.last_mut().unwrap().kind
    else {
        unreachable!()
    };
    arguments.push(ValueId(if alias { 1 } else { 4 }));
    m
}

#[allow(dead_code)]
pub(super) fn unbound_fixed_pointer() -> TypedModule {
    let mut m = phantom_class_calls(true, false);
    let Type::BorrowedFunctionPointer { life, .. } = &mut m.functions[1].values[7] else {
        unreachable!()
    };
    life.invocation_binder_count = 0;
    life.fixed_binder_count = 1;
    for row in &mut life.classes {
        row.binder = StorageBinder::Fixed(0);
    }
    m
}

#[allow(dead_code)]
pub(super) fn qualified_fn_role(wrong_actual: bool, wrong_role: bool) -> TypedModule {
    let mut m = rich_marker_pointer(false, false);
    let target = &mut m.functions[0];
    target.storage_life = None;
    target.environment = Some(ClosureId(0));
    let parameters = target
        .parameters
        .iter()
        .map(|p| target.locals[p.0 as usize].ty.clone())
        .collect::<Vec<_>>();
    let definition = ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(0),
        captures: vec![],
        parameters: parameters.clone(),
        result: Type::I32,
        kind: CallableKind::Fn,
        lending_captures: vec![],
    };
    let env = address(Type::Closure(ClosureId(0)), false);
    let role_life = StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 2,
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(
                StorageBase::Parameter(1),
                vec![],
                StorageBinder::Invocation(1),
            ),
        ],
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Invocation(if wrong_role { 0 } else { 1 }),
        }],
    };
    let mut helper = function(
        1,
        vec![env.clone(), parameters[0].clone(), parameters[1].clone()],
        Type::I32,
        vec![env, parameters[0].clone(), parameters[1].clone(), Type::I32],
        vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(Some(1), InstructionKind::Load(LocalId(1))),
            instruction(Some(2), InstructionKind::Load(LocalId(2))),
            instruction(
                Some(3),
                InstructionKind::CallClosureBorrowed {
                    callee: ValueId(0),
                    kind: CallableKind::Fn,
                    arguments: vec![ValueId(1), ValueId(2)],
                },
            ),
        ],
        3,
    );
    helper.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: if wrong_actual { 3 } else { 2 },
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(
                StorageBase::Parameter(1),
                vec![],
                StorageBinder::Invocation(1),
            ),
            row(
                StorageBase::Parameter(2),
                vec![],
                if wrong_actual {
                    StorageBinder::Invocation(2)
                } else {
                    StorageBinder::Fixed(0)
                },
            ),
        ],
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(1),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        }],
    });
    helper.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            life: Some(role_life),
            parameter: 0,
            closure: ClosureId(0),
            kind: CallableKind::Fn,
            hold: vec![],
            call: vec![],
            writes: vec![],
            cleanup: CallableCleanup::Borrowed,
        }],
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent {
                parameter: 0,
                kind: CallableKind::Fn,
            }],
            exit: CallableExit::Return,
        }]),
    });
    m.functions[1] = helper;
    m.closures = vec![definition];
    m
}

#[allow(dead_code)]
pub(super) fn qualified_two_fn_roles(wrong_second: bool) -> TypedModule {
    let mut m = qualified_fn_role(false, false);
    let old = &m.functions[1];
    let env = old.locals[0].ty.clone();
    let tag = old.locals[1].ty.clone();
    let number = old.locals[2].ty.clone();
    let mut first = old.callable_contract.as_ref().unwrap().roles[0].clone();
    first.parameter = 0;
    let mut second = first.clone();
    second.parameter = 1;
    let life = second.life.as_mut().unwrap();
    if wrong_second {
        life.classes[0].binder = StorageBinder::Invocation(0);
    } else {
        life.invocation_binder_count = 3;
        life.bindings[1].binder = StorageBinder::Invocation(2);
    }
    let mut helper = function(
        1,
        vec![env.clone(), env.clone(), tag.clone(), number.clone()],
        Type::I32,
        vec![env.clone(), env, tag, number, Type::I32, Type::I32],
        vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(Some(1), InstructionKind::Load(LocalId(1))),
            instruction(Some(2), InstructionKind::Load(LocalId(2))),
            instruction(Some(3), InstructionKind::Load(LocalId(3))),
            instruction(
                Some(4),
                InstructionKind::CallClosureBorrowed {
                    callee: ValueId(0),
                    kind: CallableKind::Fn,
                    arguments: vec![ValueId(2), ValueId(3)],
                },
            ),
            instruction(
                Some(5),
                InstructionKind::CallClosureBorrowed {
                    callee: ValueId(1),
                    kind: CallableKind::Fn,
                    arguments: vec![ValueId(2), ValueId(3)],
                },
            ),
        ],
        5,
    );
    helper.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 3,
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(
                StorageBase::Parameter(1),
                vec![],
                StorageBinder::Invocation(1),
            ),
            row(
                StorageBase::Parameter(2),
                vec![],
                StorageBinder::Invocation(2),
            ),
            row(StorageBase::Parameter(3), vec![], StorageBinder::Fixed(0)),
        ],
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(2),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        }],
    });
    helper.callable_contract = Some(CallableParameterContract {
        roles: vec![first, second],
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![
                CallEvent {
                    parameter: 0,
                    kind: CallableKind::Fn,
                },
                CallEvent {
                    parameter: 1,
                    kind: CallableKind::Fn,
                },
            ],
            exit: CallableExit::Return,
        }]),
    });
    m.functions[1] = helper;
    m
}

#[allow(dead_code)]
pub(super) fn qualified_fn_exclusive_call(alias: bool) -> TypedModule {
    let mut m = qualified_fn_role(false, false);
    let mutable = address(Type::I32, true);
    let d = &mut m.closures[0];
    d.parameters[1] = mutable.clone();
    d.parameters.push(mutable.clone());
    let entry = &mut m.functions[0];
    entry.locals[1].ty = mutable.clone();
    entry.locals.push(LocalDecl {
        id: LocalId(2),
        name: "other".into(),
        ty: mutable.clone(),
        mutable: false,
        span: s(),
    });
    entry.parameters.push(LocalId(2));
    let helper = &mut m.functions[1];
    helper.locals[2].ty = mutable.clone();
    helper.values[2] = mutable.clone();
    helper.locals.push(LocalDecl {
        id: LocalId(3),
        name: "other".into(),
        ty: mutable.clone(),
        mutable: false,
        span: s(),
    });
    helper.parameters.push(LocalId(3));
    helper.values.push(mutable);
    let c = helper.storage_life.as_mut().unwrap();
    c.invocation_binder_count = 3;
    c.bindings.push(row(
        StorageBase::Parameter(3),
        vec![],
        StorageBinder::Invocation(2),
    ));
    let life = helper.callable_contract.as_mut().unwrap().roles[0]
        .life
        .as_mut()
        .unwrap();
    life.invocation_binder_count = 3;
    life.bindings.push(row(
        StorageBase::Parameter(2),
        vec![],
        StorageBinder::Invocation(2),
    ));
    helper.blocks[0]
        .instructions
        .insert(3, instruction(Some(4), InstructionKind::Load(LocalId(3))));
    let InstructionKind::CallClosureBorrowed { arguments, .. } =
        &mut helper.blocks[0].instructions.last_mut().unwrap().kind
    else {
        unreachable!()
    };
    arguments.push(ValueId(if alias { 2 } else { 4 }));
    m
}

/// Untrusted existing-public-Typed model, not a Source producer witness.
/// Parameter and actual capture share Tag's retained class; the captured
/// outer Address is a separate anonymous Fixed class.
#[allow(dead_code)]
pub(super) fn captured_fixed(kind: CallableKind, wrong_actual: bool) -> TypedModule {
    let mut m = module();
    let tag = address(storage(), false);
    let i64ref = address(Type::I64, false);
    let class = |base, binder| SignatureLifeClassBinding {
        position: SignatureLifeClass {
            base,
            path: vec![LifeTypeProjection::Pointee],
            class: 0,
        },
        binder,
    };
    let life = StorageLifeContract {
        fixed_binder_count: 2,
        invocation_binder_count: 1,
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(StorageBase::Capture(0), vec![], StorageBinder::Fixed(1)),
        ],
        classes: vec![
            class(StorageBase::Parameter(0), StorageBinder::Fixed(0)),
            class(StorageBase::Capture(0), StorageBinder::Fixed(0)),
        ],
    };
    let mut entry = function(
        0,
        vec![tag.clone()],
        Type::I64,
        vec![
            tag.clone(),
            i64ref.clone(),
            Type::I64,
            tag.clone(),
            i64ref,
            Type::I64,
            Type::I64,
        ],
        vec![
            instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
            instruction(
                Some(1),
                InstructionKind::LoadStoredBorrow {
                    storage: ValueId(0),
                    path: vec![PlaceProjection::Field(0)],
                    mode: StoredBorrowLoad::CopyShared,
                },
            ),
            instruction(
                Some(2),
                InstructionKind::ReadAddress {
                    address: ValueId(1),
                },
            ),
            instruction(Some(3), InstructionKind::Load(LocalId(0))),
            instruction(
                Some(4),
                InstructionKind::LoadStoredBorrow {
                    storage: ValueId(3),
                    path: vec![PlaceProjection::Field(0)],
                    mode: StoredBorrowLoad::CopyShared,
                },
            ),
            instruction(
                Some(5),
                InstructionKind::ReadAddress {
                    address: ValueId(4),
                },
            ),
            instruction(
                Some(6),
                InstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(2),
                    right: ValueId(5),
                },
            ),
        ],
        6,
    );
    entry.storage_life = Some(life.clone());
    entry.environment = Some(ClosureId(0));
    let mut caller = function(
        1,
        vec![tag.clone(), tag.clone()],
        Type::I64,
        vec![tag.clone(), tag, Type::Closure(ClosureId(0)), Type::I64],
        vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(Some(1), InstructionKind::Load(LocalId(1))),
            instruction(
                Some(2),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![ValueId(0)],
                },
            ),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(2),
                    value: ValueId(2),
                },
            ),
            instruction(
                Some(3),
                match kind {
                    CallableKind::Fn => InstructionKind::CallClosureShared {
                        callee: LocalId(2),
                        arguments: vec![ValueId(if wrong_actual { 1 } else { 0 })],
                    },
                    CallableKind::FnMut => InstructionKind::CallClosureMut {
                        callee: LocalId(2),
                        arguments: vec![ValueId(if wrong_actual { 1 } else { 0 })],
                    },
                    CallableKind::FnOnce => InstructionKind::CallClosureOnce {
                        callee: ValueId(2),
                        arguments: vec![ValueId(if wrong_actual { 1 } else { 0 })],
                    },
                },
            ),
        ],
        3,
    );
    if kind == CallableKind::FnOnce {
        // Keep one real owner of the non-Copy environment; no duplicate Store.
        caller.blocks[0].instructions.remove(3);
    }
    caller.locals.push(LocalDecl {
        id: LocalId(2),
        name: "environment".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: kind == CallableKind::FnMut,
        span: s(),
    });
    caller.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 2,
        invocation_binder_count: 2,
        bindings: vec![
            row(
                StorageBase::Parameter(0),
                vec![],
                StorageBinder::Invocation(0),
            ),
            row(
                StorageBase::Parameter(1),
                vec![],
                StorageBinder::Invocation(1),
            ),
        ],
        classes: vec![
            class(StorageBase::Parameter(0), StorageBinder::Fixed(0)),
            class(StorageBase::Parameter(1), StorageBinder::Fixed(1)),
        ],
    });
    m.functions = vec![entry, caller];
    m.closures = vec![ClosureDefinition {
        storage_life: Some(life),
        id: ClosureId(0),
        function: FunctionId(0),
        captures: vec![address(storage(), false)],
        parameters: vec![address(storage(), false)],
        result: Type::I64,
        kind,
        lending_captures: vec![],
    }];
    m
}
