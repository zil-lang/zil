//! Raw component fixture only. It supplies no permission, initialization or
//! native acceptance proof; both kernels must check those independently.
use zil_base::{Fingerprint, ModuleId, Span, TargetProfile};
use zil_ir::*;
pub fn instruction(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: Span::default(),
    }
}
pub fn module() -> TypedModule {
    let payload = Type::Record {
        name: "TicketToken".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let resource = Type::Managed(Box::new(payload.clone()));
    let package = Type::Package {
        name: "TicketPacket".into(),
        fields: vec![("owner".into(), resource.clone()), ("n".into(), Type::I32)],
    };
    let address = Type::Address {
        mutable: true,
        inner: Box::new(package),
    };
    let function = TypedFunction {
        retained_return_sources: None,
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(0),
        name: "replace".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0), LocalId(1)],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![
            LocalDecl {
                id: LocalId(0),
                name: "target".into(),
                ty: address.clone(),
                mutable: false,
                span: Span::default(),
            },
            LocalDecl {
                id: LocalId(1),
                name: "input".into(),
                ty: resource.clone(),
                mutable: true,
                span: Span::default(),
            },
        ],
        values: vec![address, resource],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
                instruction(
                    None,
                    InstructionKind::ReservePlaceAddress {
                        reservation: ReservationId(71),
                        address: ValueId(0),
                        path: vec![PlaceProjection::Field(0)],
                    },
                ),
                instruction(
                    Some(ValueId(1)),
                    InstructionKind::TakeLocal { local: LocalId(1) },
                ),
                instruction(
                    None,
                    InstructionKind::CommitPlaceReplace {
                        reservation: ReservationId(71),
                        value: ValueId(1),
                    },
                ),
            ],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: Span::default(),
    };
    TypedModule {
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::default(),
        functions: vec![function],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "TicketToken".into(),
            payload,
            teardown: None,
        }],
        unsupported: vec![],
    }
}

pub fn module_with_relay_and_caller(fresh: bool) -> TypedModule {
    let mut m = module();
    let address = m.functions[0].locals[0].ty.clone();
    let resource = m.functions[0].locals[1].ty.clone();
    let Type::Address { inner: package, .. } = &address else {
        unreachable!()
    };
    let package = package.as_ref().clone();
    let mut relay = m.functions[0].clone();
    relay.id = FunctionId(1);
    relay.name = "relay".into();
    relay.blocks[0].instructions = vec![
        instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
        instruction(
            Some(ValueId(1)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        ),
        instruction(
            None,
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![ValueId(0), ValueId(1)],
            },
        ),
    ];
    m.functions.push(relay);
    let mut caller = m.functions[0].clone();
    caller.id = FunctionId(2);
    caller.name = "create_result".into();
    caller.return_type = resource.clone();
    caller.result_mode = PassingMode::Create;
    caller.parameters = if fresh { vec![] } else { vec![LocalId(0)] };
    caller.locals[0].ty = resource.clone();
    caller.locals[1].ty = package.clone();
    caller.values = vec![
        Type::I32,
        resource.clone(),
        package,
        address,
        resource.clone(),
        resource,
    ];
    caller.blocks[0].instructions = vec![
        instruction(
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I32(65)),
        ),
        instruction(
            Some(ValueId(1)),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
        instruction(
            Some(ValueId(2)),
            InstructionKind::MakePackage {
                fields: vec![ValueId(1), ValueId(0)],
            },
        ),
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(2),
            },
        ),
        instruction(
            Some(ValueId(3)),
            InstructionKind::BorrowLocalMut { local: LocalId(1) },
        ),
        instruction(
            Some(ValueId(4)),
            if fresh {
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(0)],
                }
            } else {
                InstructionKind::TakeLocal { local: LocalId(0) }
            },
        ),
        instruction(
            None,
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(3), ValueId(4)],
            },
        ),
        instruction(
            Some(ValueId(5)),
            InstructionKind::TakePackageField {
                local: LocalId(1),
                index: 0,
            },
        ),
        instruction(None, InstructionKind::DropPackage { local: LocalId(1) }),
    ];
    caller.blocks[0].terminator = Terminator::Return(Some(ValueId(5)));
    m.functions.push(caller);
    m
}
