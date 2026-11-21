//! Current explicit untrusted raw fixtures for the archived operand/control
//! pairs. No historical record is parsed, patched, relabelled or accepted.
//! The archived hash selects a named reconstruction and remains a version
//! rejection control. These hand-built records claim no original Source Γ;
//! all actual CFG, capture, role, cleanup and candidate claims are rechecked.
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;
fn empty_head() -> StorageLifeContract {
    StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 0,
        bindings: vec![],
        static_bindings: vec![],
        classes: vec![],
    }
}
fn instruction(result: Option<u32>, kind: InstructionKind, span: Span) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span,
    }
}
#[allow(clippy::too_many_arguments)]
fn function(
    id: u32,
    name: String,
    environment: Option<ClosureId>,
    parameters: Vec<LocalId>,
    return_type: Type,
    locals: Vec<LocalDecl>,
    values: Vec<Type>,
    instructions: Vec<Instruction>,
    returned: Option<ValueId>,
    span: Span,
    callable_contract: Option<CallableParameterContract>,
    callable_return_contract: Option<CallableReturnContract>,
) -> TypedFunction {
    TypedFunction {
        dyn_instance: None,
        retained_return_sources: None,
        storage_life: Some(empty_head()),
        call_kind: CallKind::Direct,
        resume_bound: TaskResumeBound::Local,
        reference_return_bound: None,
        callable_return_contract,
        callable_contract,
        protocol_contracts: vec![],
        id: FunctionId(id),
        name,
        foreign: None,
        environment,
        parameters,
        return_type,
        result_mode: PassingMode::Infer,
        locals,
        values,
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions,
            terminator: Terminator::Return(returned),
        }],
        entry: BlockId(0),
        span,
    }
}
fn nominal(module: &mut TypedModule, index: u32, name: &str, kind: LifeNominalKind, ty: Type) {
    let id = LifeDeclarationId {
        module: module.module_id,
        declaration: NodeId(index),
        compiler: false,
    };
    module.life_declarations.push(LifeNominalDeclaration {
        id,
        identity: LifeDeclarationIdentity {
            package: "untrusted-raw-fixture".into(),
            module_path: vec![],
            declaration_path: vec![name.into()],
            generation_path: vec![],
        },
        kind,
        parameters: vec![],
    });
    module.life_instances.push(LifeNominalInstance {
        declaration: id,
        ty,
        arguments: vec![],
    });
}

// control_shared_callback_i32_argument: exact original actual CFG and operand order.
fn control_shared_callback_i32_argument() -> TypedModule {
    let module = TypedModule {
        semantic: None,
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![
            function(
                0,
                "callback".into(),
                Some(ClosureId(0)),
                vec![LocalId(0)],
                Type::I32,
                vec![LocalDecl {
                    id: LocalId(0),
                    name: "l0".into(),
                    ty: Type::I32,
                    mutable: true,
                    span: Span::new(FileId(0), 0, 1),
                }],
                vec![Type::I32],
                vec![instruction(
                    Some(0),
                    InstructionKind::Constant(Constant::I32(7)),
                    Span::new(FileId(0), 0, 1),
                )],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                None,
                None,
            ),
            function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(0)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![
                    Type::Closure(ClosureId(0)),
                    Type::I32,
                    Type::I32,
                    Type::Closure(ClosureId(0)),
                ],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::Load(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::Constant(Constant::I32(7)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::CallClosureShared {
                            callee: LocalId(1),
                            arguments: vec![ValueId(1)],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(3),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(3)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(0),
                        kind: CallableKind::Fn,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![CallEvent {
                            parameter: 1,
                            kind: CallableKind::Fn,
                        }],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(0),
                    kind: CallableKind::Fn,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![],
                    lending_captures: vec![],
                }),
            ),
        ],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            storage_life: Some(empty_head()),
            captures: vec![],
            parameters: vec![Type::I32],
            result: Type::I32,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        }],
        resources: vec![],
        unsupported: vec![],
    };
    module
}

// parent_valid_no_call_edge: exact original actual CFG and operand order.
fn parent_valid_no_call_edge() -> TypedModule {
    let mut module = TypedModule {
        semantic: None,
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![
            function(
                0,
                "callback".into(),
                Some(ClosureId(0)),
                vec![],
                Type::I32,
                vec![],
                vec![Type::I32],
                vec![instruction(
                    Some(0),
                    InstructionKind::Constant(Constant::I32(7)),
                    Span::new(FileId(0), 0, 1),
                )],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                None,
                None,
            ),
            function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(1)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(1)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![Type::Closure(ClosureId(1))],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(1),
                        kind: CallableKind::FnOnce,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(1),
                    kind: CallableKind::FnOnce,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![0],
                    lending_captures: vec![],
                }),
            ),
            function(
                2,
                "returned_parent".into(),
                Some(ClosureId(1)),
                vec![],
                Type::I32,
                vec![LocalDecl {
                    id: LocalId(0),
                    name: "l0".into(),
                    ty: Type::Closure(ClosureId(0)),
                    mutable: true,
                    span: Span::new(FileId(0), 0, 1),
                }],
                vec![
                    Type::Address {
                        mutable: true,
                        inner: Box::new(Type::Closure(ClosureId(0))),
                    },
                    Type::Closure(ClosureId(0)),
                    Type::I32,
                ],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::BorrowCapture {
                            index: 0,
                            mutable: true,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::CallClosureBorrowed {
                            callee: ValueId(0),
                            kind: CallableKind::FnMut,
                            arguments: vec![],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::TakeCapture { index: 0 },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(0),
                            value: ValueId(1),
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropClosure { local: LocalId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(2)),
                Span::new(FileId(0), 0, 1),
                None,
                None,
            ),
        ],
        closures: vec![
            ClosureDefinition {
                id: ClosureId(0),
                function: FunctionId(0),
                storage_life: Some(empty_head()),
                captures: vec![Type::Managed(Box::new(Type::Record {
                    name: "Token".into(),
                    fields: vec![("n".into(), Type::I32)],
                }))],
                parameters: vec![],
                result: Type::I32,
                kind: CallableKind::FnMut,
                lending_captures: vec![],
            },
            ClosureDefinition {
                id: ClosureId(1),
                function: FunctionId(2),
                storage_life: Some(empty_head()),
                captures: vec![Type::Closure(ClosureId(0))],
                parameters: vec![],
                result: Type::I32,
                kind: CallableKind::FnOnce,
                lending_captures: vec![],
            },
        ],
        resources: vec![ResourceDefinition {
            name: "Token".into(),
            payload: Type::Record {
                name: "Token".into(),
                fields: vec![("n".into(), Type::I32)],
            },
            teardown: None,
        }],
        unsupported: vec![],
    };
    nominal(
        &mut module,
        0,
        "Token",
        LifeNominalKind::Resource,
        Type::Record {
            name: "Token".into(),
            fields: vec![("n".into(), Type::I32)],
        },
    );
    module
}

// loan_revival: exact original actual CFG and operand order.
fn loan_revival() -> TypedModule {
    let mut module = TypedModule {
        semantic: None,
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![
            function(
                0,
                "main".into(),
                None,
                vec![],
                Type::I32,
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "b".into(),
                        ty: Type::Managed(Box::new(Type::ByteBuffer)),
                        mutable: false,
                        span: Span::new(FileId(0), 65, 83),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "$record_field_0".into(),
                        ty: Type::Managed(Box::new(Type::ByteBuffer)),
                        mutable: false,
                        span: Span::new(FileId(0), 103, 109),
                    },
                    LocalDecl {
                        id: LocalId(2),
                        name: "$record_field_1".into(),
                        ty: Type::I32,
                        mutable: false,
                        span: Span::new(FileId(0), 114, 115),
                    },
                    LocalDecl {
                        id: LocalId(3),
                        name: "$temporary3".into(),
                        ty: Type::Managed(Box::new(Type::ByteBuffer)),
                        mutable: true,
                        span: Span::new(FileId(0), 95, 109),
                    },
                    LocalDecl {
                        id: LocalId(4),
                        name: "e".into(),
                        ty: Type::Package {
                            name: "@module0:Event".into(),
                            fields: vec![
                                ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                                ("tag".into(), Type::I32),
                            ],
                        },
                        mutable: false,
                        span: Span::new(FileId(0), 83, 117),
                    },
                    LocalDecl {
                        id: LocalId(5),
                        name: "view".into(),
                        ty: Type::Address {
                            mutable: false,
                            inner: Box::new(Type::Slice {
                                element: Box::new(Type::U8),
                            }),
                        },
                        mutable: false,
                        span: Span::new(FileId(0), 117, 148),
                    },
                    LocalDecl {
                        id: LocalId(6),
                        name: "h".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: false,
                        span: Span::new(FileId(0), 148, 183),
                    },
                    LocalDecl {
                        id: LocalId(7),
                        name: "$record_field_2".into(),
                        ty: Type::Managed(Box::new(Type::ByteBuffer)),
                        mutable: false,
                        span: Span::new(FileId(0), 207, 218),
                    },
                    LocalDecl {
                        id: LocalId(8),
                        name: "$record_field_3".into(),
                        ty: Type::I32,
                        mutable: false,
                        span: Span::new(FileId(0), 223, 224),
                    },
                    LocalDecl {
                        id: LocalId(9),
                        name: "$temporary9".into(),
                        ty: Type::Managed(Box::new(Type::ByteBuffer)),
                        mutable: true,
                        span: Span::new(FileId(0), 199, 218),
                    },
                    LocalDecl {
                        id: LocalId(10),
                        name: "other".into(),
                        ty: Type::Package {
                            name: "@module0:Event".into(),
                            fields: vec![
                                ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                                ("tag".into(), Type::I32),
                            ],
                        },
                        mutable: false,
                        span: Span::new(FileId(0), 183, 226),
                    },
                    LocalDecl {
                        id: LocalId(11),
                        name: "$temporary11".into(),
                        ty: Type::Package {
                            name: "@module0:Event".into(),
                            fields: vec![
                                ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                                ("tag".into(), Type::I32),
                            ],
                        },
                        mutable: true,
                        span: Span::new(FileId(0), 234, 239),
                    },
                    LocalDecl {
                        id: LocalId(12),
                        name: "a".into(),
                        ty: Type::Package {
                            name: "@module0:Event".into(),
                            fields: vec![
                                ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                                ("tag".into(), Type::I32),
                            ],
                        },
                        mutable: false,
                        span: Span::new(FileId(0), 226, 241),
                    },
                    LocalDecl {
                        id: LocalId(13),
                        name: "n".into(),
                        ty: Type::I32,
                        mutable: false,
                        span: Span::new(FileId(0), 241, 260),
                    },
                ],
                vec![
                    Type::Managed(Box::new(Type::ByteBuffer)),
                    Type::Managed(Box::new(Type::ByteBuffer)),
                    Type::I32,
                    Type::Managed(Box::new(Type::ByteBuffer)),
                    Type::I32,
                    Type::Managed(Box::new(Type::ByteBuffer)),
                    Type::Package {
                        name: "@module0:Event".into(),
                        fields: vec![
                            ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                            ("tag".into(), Type::I32),
                        ],
                    },
                    Type::Desc(Box::new(Type::ByteBuffer)),
                    Type::Address {
                        mutable: false,
                        inner: Box::new(Type::Slice {
                            element: Box::new(Type::U8),
                        }),
                    },
                    Type::Closure(ClosureId(0)),
                    Type::Managed(Box::new(Type::ByteBuffer)),
                    Type::I32,
                    Type::Managed(Box::new(Type::ByteBuffer)),
                    Type::I32,
                    Type::Managed(Box::new(Type::ByteBuffer)),
                    Type::Package {
                        name: "@module0:Event".into(),
                        fields: vec![
                            ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                            ("tag".into(), Type::I32),
                        ],
                    },
                    Type::Closure(ClosureId(0)),
                    Type::Package {
                        name: "@module0:Event".into(),
                        fields: vec![
                            ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                            ("tag".into(), Type::I32),
                        ],
                    },
                    Type::Package {
                        name: "@module0:Event".into(),
                        fields: vec![
                            ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                            ("tag".into(), Type::I32),
                        ],
                    },
                    Type::Package {
                        name: "@module0:Event".into(),
                        fields: vec![
                            ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                            ("tag".into(), Type::I32),
                        ],
                    },
                    Type::Address {
                        mutable: false,
                        inner: Box::new(Type::Slice {
                            element: Box::new(Type::U8),
                        }),
                    },
                    Type::I32,
                    Type::Address {
                        mutable: false,
                        inner: Box::new(Type::U8),
                    },
                    Type::Address {
                        mutable: false,
                        inner: Box::new(Type::U8),
                    },
                    Type::U8,
                    Type::I32,
                    Type::I32,
                    Type::I32,
                    Type::I32,
                ],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 50, 275),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(1) },
                        Span::new(FileId(0), 64, 275),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(11)),
                        Span::new(FileId(0), 234, 239),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::ByteBufferNew,
                        Span::new(FileId(0), 71, 82),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(0)),
                        Span::new(FileId(0), 65, 83),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(0),
                            value: ValueId(0),
                        },
                        Span::new(FileId(0), 65, 83),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(2) },
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(3)),
                        Span::new(FileId(0), 95, 109),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::TakeLocal { local: LocalId(0) },
                        Span::new(FileId(0), 103, 109),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(1)),
                        Span::new(FileId(0), 103, 109),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(1),
                            value: ValueId(1),
                        },
                        Span::new(FileId(0), 103, 109),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::Constant(Constant::I32(1)),
                        Span::new(FileId(0), 114, 115),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(2)),
                        Span::new(FileId(0), 114, 115),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(2),
                            value: ValueId(2),
                        },
                        Span::new(FileId(0), 114, 115),
                    ),
                    instruction(
                        Some(3),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 95, 109),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(3),
                            value: ValueId(3),
                        },
                        Span::new(FileId(0), 95, 109),
                    ),
                    instruction(
                        Some(4),
                        InstructionKind::Load(LocalId(2)),
                        Span::new(FileId(0), 110, 115),
                    ),
                    instruction(
                        Some(5),
                        InstructionKind::TakeLocal { local: LocalId(3) },
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        Some(6),
                        InstructionKind::MakePackage {
                            fields: vec![ValueId(5), ValueId(4)],
                        },
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropManaged { local: LocalId(3) },
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(3)),
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(2)),
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropManaged { local: LocalId(1) },
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(2),
                            kind: CleanupExitKind::Fallthrough,
                        },
                        Span::new(FileId(0), 89, 116),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(4)),
                        Span::new(FileId(0), 83, 117),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(4),
                            value: ValueId(6),
                        },
                        Span::new(FileId(0), 83, 117),
                    ),
                    instruction(
                        Some(7),
                        InstructionKind::DescribePackageField {
                            local: LocalId(4),
                            index: 0,
                        },
                        Span::new(FileId(0), 137, 146),
                    ),
                    instruction(
                        Some(8),
                        InstructionKind::ByteBufferBorrow {
                            buffer: ValueId(7),
                            mutable: false,
                        },
                        Span::new(FileId(0), 126, 147),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(5)),
                        Span::new(FileId(0), 117, 148),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(5),
                            value: ValueId(8),
                        },
                        Span::new(FileId(0), 117, 148),
                    ),
                    instruction(
                        Some(9),
                        InstructionKind::MakeClosure {
                            closure: ClosureId(0),
                            captures: vec![],
                        },
                        Span::new(FileId(0), 154, 182),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(6)),
                        Span::new(FileId(0), 148, 183),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(6),
                            value: ValueId(9),
                        },
                        Span::new(FileId(0), 148, 183),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(3) },
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(9)),
                        Span::new(FileId(0), 199, 218),
                    ),
                    instruction(
                        Some(10),
                        InstructionKind::ByteBufferNew,
                        Span::new(FileId(0), 207, 218),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(7)),
                        Span::new(FileId(0), 207, 218),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(7),
                            value: ValueId(10),
                        },
                        Span::new(FileId(0), 207, 218),
                    ),
                    instruction(
                        Some(11),
                        InstructionKind::Constant(Constant::I32(2)),
                        Span::new(FileId(0), 223, 224),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(8)),
                        Span::new(FileId(0), 223, 224),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(8),
                            value: ValueId(11),
                        },
                        Span::new(FileId(0), 223, 224),
                    ),
                    instruction(
                        Some(12),
                        InstructionKind::TakeLocal { local: LocalId(7) },
                        Span::new(FileId(0), 199, 218),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(9),
                            value: ValueId(12),
                        },
                        Span::new(FileId(0), 199, 218),
                    ),
                    instruction(
                        Some(13),
                        InstructionKind::Load(LocalId(8)),
                        Span::new(FileId(0), 219, 224),
                    ),
                    instruction(
                        Some(14),
                        InstructionKind::TakeLocal { local: LocalId(9) },
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        Some(15),
                        InstructionKind::MakePackage {
                            fields: vec![ValueId(14), ValueId(13)],
                        },
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropManaged { local: LocalId(9) },
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(9)),
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(8)),
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropManaged { local: LocalId(7) },
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(7)),
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(3),
                            kind: CleanupExitKind::Fallthrough,
                        },
                        Span::new(FileId(0), 193, 225),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(10)),
                        Span::new(FileId(0), 183, 226),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(10),
                            value: ValueId(15),
                        },
                        Span::new(FileId(0), 183, 226),
                    ),
                    instruction(
                        Some(16),
                        InstructionKind::Load(LocalId(6)),
                        Span::new(FileId(0), 232, 233),
                    ),
                    instruction(
                        Some(17),
                        InstructionKind::TakeLocal { local: LocalId(4) },
                        Span::new(FileId(0), 234, 239),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(11),
                            value: ValueId(17),
                        },
                        Span::new(FileId(0), 234, 239),
                    ),
                    instruction(
                        Some(18),
                        InstructionKind::TakeLocal { local: LocalId(11) },
                        Span::new(FileId(0), 232, 240),
                    ),
                    instruction(
                        Some(19),
                        InstructionKind::CallClosure {
                            callee: ValueId(16),
                            arguments: vec![ValueId(18)],
                        },
                        Span::new(FileId(0), 232, 240),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropPackage { local: LocalId(11) },
                        Span::new(FileId(0), 226, 241),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(12)),
                        Span::new(FileId(0), 226, 241),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(12),
                            value: ValueId(19),
                        },
                        Span::new(FileId(0), 226, 241),
                    ),
                    instruction(
                        Some(20),
                        InstructionKind::Load(LocalId(5)),
                        Span::new(FileId(0), 251, 255),
                    ),
                    instruction(
                        Some(21),
                        InstructionKind::Constant(Constant::I32(0)),
                        Span::new(FileId(0), 256, 257),
                    ),
                    instruction(
                        Some(22),
                        InstructionKind::AddressIndex {
                            address: ValueId(20),
                            index: ValueId(21),
                        },
                        Span::new(FileId(0), 251, 258),
                    ),
                    instruction(
                        Some(23),
                        InstructionKind::ReborrowAddress {
                            address: ValueId(22),
                            mutable: false,
                        },
                        Span::new(FileId(0), 251, 258),
                    ),
                    instruction(
                        Some(24),
                        InstructionKind::ReadAddress {
                            address: ValueId(23),
                        },
                        Span::new(FileId(0), 251, 258),
                    ),
                    instruction(
                        Some(25),
                        InstructionKind::ConvertInteger {
                            value: ValueId(24),
                            target: Type::I32,
                        },
                        Span::new(FileId(0), 247, 259),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(13)),
                        Span::new(FileId(0), 241, 260),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(13),
                            value: ValueId(25),
                        },
                        Span::new(FileId(0), 241, 260),
                    ),
                    instruction(
                        Some(26),
                        InstructionKind::Load(LocalId(13)),
                        Span::new(FileId(0), 267, 268),
                    ),
                    instruction(
                        Some(27),
                        InstructionKind::ReadPackageField {
                            local: LocalId(12),
                            index: 1,
                        },
                        Span::new(FileId(0), 269, 274),
                    ),
                    instruction(
                        Some(28),
                        InstructionKind::Binary {
                            op: BinaryOp::Add,
                            left: ValueId(26),
                            right: ValueId(27),
                        },
                        Span::new(FileId(0), 267, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(13)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropPackage { local: LocalId(12) },
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(12)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(11)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropPackage { local: LocalId(10) },
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(10)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(6)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(5)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropPackage { local: LocalId(4) },
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(4)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropManaged { local: LocalId(0) },
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(1),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 260, 274),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 260, 274),
                    ),
                ],
                Some(ValueId(28)),
                Span::new(FileId(0), 50, 275),
                None,
                None,
            ),
            function(
                1,
                "$closure0".into(),
                Some(ClosureId(0)),
                vec![LocalId(0)],
                Type::Package {
                    name: "@module0:Event".into(),
                    fields: vec![
                        ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                        ("tag".into(), Type::I32),
                    ],
                },
                vec![LocalDecl {
                    id: LocalId(0),
                    name: "e".into(),
                    ty: Type::Package {
                        name: "@module0:Event".into(),
                        fields: vec![
                            ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                            ("tag".into(), Type::I32),
                        ],
                    },
                    mutable: false,
                    span: Span::new(FileId(0), 155, 162),
                }],
                vec![Type::Package {
                    name: "@module0:Event".into(),
                    fields: vec![
                        ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                        ("tag".into(), Type::I32),
                    ],
                }],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 154, 182),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(1) },
                        Span::new(FileId(0), 172, 182),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(0) },
                        Span::new(FileId(0), 180, 181),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(1),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 173, 181),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropPackage { local: LocalId(0) },
                        Span::new(FileId(0), 173, 181),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 173, 181),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 173, 181),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 154, 182),
                None,
                None,
            ),
        ],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(1),
            storage_life: Some(empty_head()),
            captures: vec![],
            parameters: vec![Type::Package {
                name: "@module0:Event".into(),
                fields: vec![
                    ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                    ("tag".into(), Type::I32),
                ],
            }],
            result: Type::Package {
                name: "@module0:Event".into(),
                fields: vec![
                    ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                    ("tag".into(), Type::I32),
                ],
            },
            kind: CallableKind::Fn,
            lending_captures: vec![],
        }],
        resources: vec![ResourceDefinition {
            name: "@builtin:ByteBuffer".into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }],
        unsupported: vec![],
    };
    nominal(
        &mut module,
        0,
        "@module0:Event",
        LifeNominalKind::Package,
        Type::Package {
            name: "@module0:Event".into(),
            fields: vec![
                ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("tag".into(), Type::I32),
            ],
        },
    );
    module.source_fingerprint = Fingerprint::of(include_bytes!(
        "../../../tests/borrowed-callable-arguments/loan_control.zil"
    ));
    module
}

// mutable-product: exact original actual CFG and operand order.
fn mutable_product() -> TypedModule {
    let mut module = TypedModule {
        semantic: None,
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![
            function(
                0,
                "identity".into(),
                None,
                vec![LocalId(0)],
                Type::Package {
                    name: "@module0:Root".into(),
                    fields: vec![
                        (
                            "child".into(),
                            Type::Package {
                                name: "@module0:Leaf".into(),
                                fields: vec![
                                    (
                                        "owner".into(),
                                        Type::Managed(Box::new(Type::Record {
                                            name: "@module0:R".into(),
                                            fields: vec![("n".into(), Type::I32)],
                                        })),
                                    ),
                                    ("tag".into(), Type::I32),
                                ],
                            },
                        ),
                        ("tag".into(), Type::I32),
                    ],
                },
                vec![LocalDecl {
                    id: LocalId(0),
                    name: "p".into(),
                    ty: Type::Package {
                        name: "@module0:Root".into(),
                        fields: vec![
                            (
                                "child".into(),
                                Type::Package {
                                    name: "@module0:Leaf".into(),
                                    fields: vec![
                                        (
                                            "owner".into(),
                                            Type::Managed(Box::new(Type::Record {
                                                name: "@module0:R".into(),
                                                fields: vec![("n".into(), Type::I32)],
                                            })),
                                        ),
                                        ("tag".into(), Type::I32),
                                    ],
                                },
                            ),
                            ("tag".into(), Type::I32),
                        ],
                    },
                    mutable: false,
                    span: Span::new(FileId(0), 101, 107),
                }],
                vec![Type::Package {
                    name: "@module0:Root".into(),
                    fields: vec![
                        (
                            "child".into(),
                            Type::Package {
                                name: "@module0:Leaf".into(),
                                fields: vec![
                                    (
                                        "owner".into(),
                                        Type::Managed(Box::new(Type::Record {
                                            name: "@module0:R".into(),
                                            fields: vec![("n".into(), Type::I32)],
                                        })),
                                    ),
                                    ("tag".into(), Type::I32),
                                ],
                            },
                        ),
                        ("tag".into(), Type::I32),
                    ],
                }],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 89, 124),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(1) },
                        Span::new(FileId(0), 114, 124),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(0) },
                        Span::new(FileId(0), 122, 123),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(1),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 115, 123),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropPackage { local: LocalId(0) },
                        Span::new(FileId(0), 115, 123),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 115, 123),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 115, 123),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 89, 124),
                None,
                None,
            ),
            function(
                1,
                "write".into(),
                None,
                vec![LocalId(0)],
                Type::Unit,
                vec![LocalDecl {
                    id: LocalId(0),
                    name: "p".into(),
                    ty: Type::Address {
                        mutable: true,
                        inner: Box::new(Type::Package {
                            name: "@module0:Root".into(),
                            fields: vec![
                                (
                                    "child".into(),
                                    Type::Package {
                                        name: "@module0:Leaf".into(),
                                        fields: vec![
                                            (
                                                "owner".into(),
                                                Type::Managed(Box::new(Type::Record {
                                                    name: "@module0:R".into(),
                                                    fields: vec![("n".into(), Type::I32)],
                                                })),
                                            ),
                                            ("tag".into(), Type::I32),
                                        ],
                                    },
                                ),
                                ("tag".into(), Type::I32),
                            ],
                        }),
                    },
                    mutable: false,
                    span: Span::new(FileId(0), 134, 145),
                }],
                vec![
                    Type::Address {
                        mutable: true,
                        inner: Box::new(Type::Package {
                            name: "@module0:Root".into(),
                            fields: vec![
                                (
                                    "child".into(),
                                    Type::Package {
                                        name: "@module0:Leaf".into(),
                                        fields: vec![
                                            (
                                                "owner".into(),
                                                Type::Managed(Box::new(Type::Record {
                                                    name: "@module0:R".into(),
                                                    fields: vec![("n".into(), Type::I32)],
                                                })),
                                            ),
                                            ("tag".into(), Type::I32),
                                        ],
                                    },
                                ),
                                ("tag".into(), Type::I32),
                            ],
                        }),
                    },
                    Type::Address {
                        mutable: true,
                        inner: Box::new(Type::I32),
                    },
                    Type::I32,
                ],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 125, 155),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(1) },
                        Span::new(FileId(0), 146, 155),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::Load(LocalId(0)),
                        Span::new(FileId(0), 147, 148),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::AddressField {
                            address: ValueId(0),
                            index: 1,
                        },
                        Span::new(FileId(0), 147, 152),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::Constant(Constant::I32(7)),
                        Span::new(FileId(0), 153, 154),
                    ),
                    instruction(
                        None,
                        InstructionKind::WriteAddress {
                            address: ValueId(1),
                            value: ValueId(2),
                        },
                        Span::new(FileId(0), 147, 152),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(1),
                            kind: CleanupExitKind::Fallthrough,
                        },
                        Span::new(FileId(0), 146, 155),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 146, 155),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 146, 155),
                    ),
                ],
                None,
                Span::new(FileId(0), 125, 155),
                None,
                None,
            ),
            function(
                2,
                "main".into(),
                None,
                vec![],
                Type::Unit,
                vec![],
                vec![],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 156, 167),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(1) },
                        Span::new(FileId(0), 165, 167),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(1),
                            kind: CleanupExitKind::Fallthrough,
                        },
                        Span::new(FileId(0), 165, 167),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 165, 167),
                    ),
                ],
                None,
                Span::new(FileId(0), 156, 167),
                None,
                None,
            ),
        ],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "@module0:R".into(),
            payload: Type::Record {
                name: "@module0:R".into(),
                fields: vec![("n".into(), Type::I32)],
            },
            teardown: None,
        }],
        unsupported: vec![],
    };
    nominal(
        &mut module,
        0,
        "@module0:Root",
        LifeNominalKind::Package,
        Type::Package {
            name: "@module0:Root".into(),
            fields: vec![
                (
                    "child".into(),
                    Type::Package {
                        name: "@module0:Leaf".into(),
                        fields: vec![
                            (
                                "owner".into(),
                                Type::Managed(Box::new(Type::Record {
                                    name: "@module0:R".into(),
                                    fields: vec![("n".into(), Type::I32)],
                                })),
                            ),
                            ("tag".into(), Type::I32),
                        ],
                    },
                ),
                ("tag".into(), Type::I32),
            ],
        },
    );
    nominal(
        &mut module,
        1,
        "@module0:Leaf",
        LifeNominalKind::Package,
        Type::Package {
            name: "@module0:Leaf".into(),
            fields: vec![
                (
                    "owner".into(),
                    Type::Managed(Box::new(Type::Record {
                        name: "@module0:R".into(),
                        fields: vec![("n".into(), Type::I32)],
                    })),
                ),
                ("tag".into(), Type::I32),
            ],
        },
    );
    nominal(
        &mut module,
        2,
        "@module0:R",
        LifeNominalKind::Resource,
        Type::Record {
            name: "@module0:R".into(),
            fields: vec![("n".into(), Type::I32)],
        },
    );
    let head = module.functions[1].storage_life.as_mut().unwrap();
    head.invocation_binder_count = 1;
    head.bindings.push(StorageLifeBinding {
        position: StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        },
        binder: StorageBinder::Invocation(0),
    });
    module
}

fn ordinary(variant: u8) -> TypedModule {
    let mut m = control_shared_callback_i32_argument();
    match variant {
        0 => {}
        1 => {
            m.functions[1] = function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(0)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![Type::Closure(ClosureId(0)), Type::I32, Type::I32],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::Constant(Constant::I32(7)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::CallClosure {
                            callee: ValueId(0),
                            arguments: vec![ValueId(1)],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(0),
                        kind: CallableKind::Fn,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![CallEvent {
                            parameter: 1,
                            kind: CallableKind::Fn,
                        }],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(0),
                    kind: CallableKind::Fn,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![],
                    lending_captures: vec![],
                }),
            );
        }
        2 => {
            m.functions[1] = function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(0)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![Type::Closure(ClosureId(0)), Type::I32, Type::I32],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::Constant(Constant::I32(7)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::CallClosure {
                            callee: ValueId(0),
                            arguments: vec![ValueId(0)],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(0),
                        kind: CallableKind::Fn,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![CallEvent {
                            parameter: 1,
                            kind: CallableKind::Fn,
                        }],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(0),
                    kind: CallableKind::Fn,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![],
                    lending_captures: vec![],
                }),
            );
        }
        3 => {
            m.functions[1] = function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(0)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![
                    Type::Closure(ClosureId(0)),
                    Type::I32,
                    Type::I32,
                    Type::Closure(ClosureId(0)),
                ],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::Load(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::Constant(Constant::I32(7)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::CallClosureShared {
                            callee: LocalId(1),
                            arguments: vec![ValueId(0)],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(3),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(3)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(0),
                        kind: CallableKind::Fn,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![CallEvent {
                            parameter: 1,
                            kind: CallableKind::Fn,
                        }],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(0),
                    kind: CallableKind::Fn,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![],
                    lending_captures: vec![],
                }),
            );
        }
        4 => {
            m.functions[1] = function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(0)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![
                    Type::Closure(ClosureId(0)),
                    Type::I32,
                    Type::Closure(ClosureId(0)),
                    Type::I32,
                ],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::Constant(Constant::I32(7)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(2),
                        InstructionKind::MakeClosure {
                            closure: ClosureId(0),
                            captures: vec![],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(3),
                        InstructionKind::CallClosure {
                            callee: ValueId(2),
                            arguments: vec![ValueId(1)],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(0),
                        kind: CallableKind::Fn,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(0),
                    kind: CallableKind::Fn,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![],
                    lending_captures: vec![],
                }),
            );
        }
        5 => {
            m.functions[0] = function(
                0,
                "callback".into(),
                Some(ClosureId(0)),
                vec![],
                Type::I32,
                vec![],
                vec![Type::I32],
                vec![instruction(
                    Some(0),
                    InstructionKind::Constant(Constant::I32(7)),
                    Span::new(FileId(0), 0, 1),
                )],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                None,
                None,
            );
            m.functions[1] = function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(0)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(2),
                        name: "l2".into(),
                        ty: Type::Array {
                            element: Box::new(Type::Closure(ClosureId(0))),
                            length: 2,
                        },
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![
                    Type::Closure(ClosureId(0)),
                    Type::Array {
                        element: Box::new(Type::Closure(ClosureId(0))),
                        length: 2,
                    },
                ],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(2)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(1),
                        InstructionKind::MakeOwnedArray {
                            elements: vec![ValueId(0), ValueId(0)],
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(2),
                            value: ValueId(1),
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::DropOwnedArray { local: LocalId(2) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(2)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(0),
                        kind: CallableKind::FnOnce,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(0),
                    kind: CallableKind::FnOnce,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![],
                    lending_captures: vec![],
                }),
            );
            m.closures = vec![ClosureDefinition {
                id: ClosureId(0),
                function: FunctionId(0),
                storage_life: Some(empty_head()),
                captures: vec![],
                parameters: vec![],
                result: Type::I32,
                kind: CallableKind::Fn,
                lending_captures: vec![],
            }];
        }
        6 => {
            m.functions[0] = function(
                0,
                "callback".into(),
                Some(ClosureId(0)),
                vec![],
                Type::I32,
                vec![],
                vec![Type::I32],
                vec![instruction(
                    Some(0),
                    InstructionKind::Constant(Constant::I32(7)),
                    Span::new(FileId(0), 0, 1),
                )],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                None,
                None,
            );
            m.functions[1] = function(
                1,
                "keep".into(),
                None,
                vec![LocalId(0), LocalId(1)],
                Type::Closure(ClosureId(0)),
                vec![
                    LocalDecl {
                        id: LocalId(0),
                        name: "l0".into(),
                        ty: Type::Bool,
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(1),
                        name: "l1".into(),
                        ty: Type::Closure(ClosureId(0)),
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                    LocalDecl {
                        id: LocalId(2),
                        name: "l2".into(),
                        ty: Type::FunctionPointer {
                            parameters: vec![],
                            result: Box::new(Type::I32),
                        },
                        mutable: true,
                        span: Span::new(FileId(0), 0, 1),
                    },
                ],
                vec![Type::Closure(ClosureId(0))],
                vec![
                    instruction(
                        None,
                        InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageLive(LocalId(2)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        Some(0),
                        InstructionKind::TakeLocal { local: LocalId(1) },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(2),
                            value: ValueId(0),
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(2)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(1)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                        Span::new(FileId(0), 0, 1),
                    ),
                    instruction(
                        None,
                        InstructionKind::ScopeExit {
                            scope: ScopeId(0),
                            kind: CleanupExitKind::Return,
                        },
                        Span::new(FileId(0), 0, 1),
                    ),
                ],
                Some(ValueId(0)),
                Span::new(FileId(0), 0, 1),
                Some(CallableParameterContract {
                    roles: vec![CallableRole {
                        parameter: 1,
                        closure: ClosureId(0),
                        kind: CallableKind::FnOnce,
                        life: Some(empty_head()),
                        hold: vec![],
                        call: vec![],
                        writes: vec![],
                        cleanup: CallableCleanup::Returned,
                    }],
                    traces: CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![],
                        exit: CallableExit::Return,
                    }]),
                }),
                Some(CallableReturnContract {
                    life: Some(empty_head()),
                    capture_sources: Some(vec![]),
                    source: CallableReturnSource::Parameter(1),
                    closure: ClosureId(0),
                    kind: CallableKind::FnOnce,
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: vec![],
                    lending_captures: vec![],
                }),
            );
            m.closures = vec![ClosureDefinition {
                id: ClosureId(0),
                function: FunctionId(0),
                storage_life: Some(empty_head()),
                captures: vec![],
                parameters: vec![],
                result: Type::I32,
                kind: CallableKind::Fn,
                lending_captures: vec![],
            }];
        }
        _ => unreachable!(),
    }
    m
}
fn parent(stale: bool, consumer: bool) -> TypedModule {
    let mut m = parent_valid_no_call_edge();
    if stale {
        m.functions[2].blocks[0].instructions.swap(3, 4);
    }
    if consumer {
        m.functions.push(function(
            3,
            "consumer".into(),
            None,
            vec![LocalId(0), LocalId(1)],
            Type::I32,
            vec![
                LocalDecl {
                    id: LocalId(0),
                    name: "l0".into(),
                    ty: Type::Bool,
                    mutable: true,
                    span: Span::new(FileId(0), 0, 1),
                },
                LocalDecl {
                    id: LocalId(1),
                    name: "l1".into(),
                    ty: Type::Closure(ClosureId(1)),
                    mutable: true,
                    span: Span::new(FileId(0), 0, 1),
                },
            ],
            vec![
                Type::Bool,
                Type::Closure(ClosureId(1)),
                Type::Closure(ClosureId(1)),
                Type::I32,
            ],
            vec![
                instruction(
                    None,
                    InstructionKind::ScopeEnter { scope: ScopeId(0) },
                    Span::new(FileId(0), 0, 1),
                ),
                instruction(
                    Some(0),
                    InstructionKind::Load(LocalId(0)),
                    Span::new(FileId(0), 0, 1),
                ),
                instruction(
                    Some(1),
                    InstructionKind::TakeLocal { local: LocalId(1) },
                    Span::new(FileId(0), 0, 1),
                ),
                instruction(
                    Some(2),
                    InstructionKind::Call {
                        function: FunctionId(1),
                        arguments: vec![ValueId(0), ValueId(1)],
                    },
                    Span::new(FileId(0), 0, 1),
                ),
                instruction(
                    Some(3),
                    InstructionKind::CallClosureOnce {
                        callee: ValueId(2),
                        arguments: vec![],
                    },
                    Span::new(FileId(0), 0, 1),
                ),
                instruction(
                    None,
                    InstructionKind::StorageDead(LocalId(1)),
                    Span::new(FileId(0), 0, 1),
                ),
                instruction(
                    None,
                    InstructionKind::StorageDead(LocalId(0)),
                    Span::new(FileId(0), 0, 1),
                ),
                instruction(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                    Span::new(FileId(0), 0, 1),
                ),
            ],
            Some(ValueId(3)),
            Span::new(FileId(0), 0, 1),
            Some(CallableParameterContract {
                roles: vec![CallableRole {
                    parameter: 1,
                    closure: ClosureId(1),
                    kind: CallableKind::FnOnce,
                    life: Some(empty_head()),
                    hold: vec![],
                    call: vec![],
                    writes: vec![],
                    cleanup: CallableCleanup::Owned,
                }],
                traces: CallableTraces::Finite(vec![CallableTrace {
                    calls: vec![CallEvent {
                        parameter: 1,
                        kind: CallableKind::FnOnce,
                    }],
                    exit: CallableExit::Return,
                }]),
            }),
            None,
        ));
    }
    m
}
pub fn rebuild(archived: &[u8], limits: &Limits) -> Vec<u8> {
    let errors = zil_ir::wire::decode_module_candidate(archived, limits).unwrap_err();
    assert_eq!(errors[0].code, "E-WIRE-VERSION");
    let hash = Fingerprint::of(archived).to_hex();
    let (module, functions) = match hash.as_str() {
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/control_concrete_copy_byvalue_call.wire
        "4c4b440bfb693fbe1a9b544302c7fea6f2bc12ae5043a470de4312379d6f366e" => (ordinary(4), vec![]),
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/control_shared_callback_i32_argument.wire
        "e544bdc6b175229704572fe18b0b318b57cb9359e854cf67f12c1413fdb5b516" => (ordinary(0), vec![]),
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_byvalue_symbolic_fn_call.wire
        "31c462fbfdc3ec3f9efbb2da9c4b31fdcbcbd92675915404548432a5f121754d" => (ordinary(1), vec![]),
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_callback_argument.wire
        "e3d61e6f1e8e2a2324383753077d9e0449600dfed08f417753122fe72f981d3b" => (ordinary(2), vec![]),
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_shared_callback_f_argument.wire
        "612afe15ff2d13b52f76d4a0afbe6ec2c45cd16c43222c4f74f7d80e1d389e97" => (ordinary(3), vec![]),
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_thin_store.wire
        "81a97f6a646aa09f8c1590794a7697a731f48018c1cc98af7fd376872022543c" => (ordinary(6), vec![]),
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_two_element_array.wire
        "1df37d660f6175d8da818a303b0828cb5d1ce29bdce2f71d717576d00db844ab" => (ordinary(5), vec![]),
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_stale_no_call_edge.wire
        "f56c04244a28ac12d9469b9b0fc2103cb6aac50effd88f512e4459dc3481c094" => {
            (parent(true, false), vec![])
        }
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_stale_with_call_edge.wire
        "e41340a11fcf1b119a5af0d587b5b16b26895705e530a9e5f985947822e576aa" => {
            (parent(true, true), vec![])
        }
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_valid_no_call_edge.wire
        "f193b557e3d4c2660f9a79e0a97671319ca30d0b694a28f0c3cab8dd8d6f745e" => {
            (parent(false, false), vec![])
        }
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_valid_with_call_edge.wire
        "a20e2cb8c8750ee488b6d30a8350a4ce13ee2d629236abf6813283073bc46045" => {
            (parent(false, true), vec![])
        }
        // tests/borrowed-callable-arguments/v40/control_callback_i32_argument.wire
        "c4f59db2ce30691888fa548b6100b608d82904c3809bc7469c5d7bb0ade9443d" => (
            ordinary(1),
            vec![
                FunctionPlan {
                    function: FunctionId(0),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0)],
                        initialized_out: vec![LocalId(0)],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
                FunctionPlan {
                    function: FunctionId(1),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0), LocalId(1)],
                        initialized_out: vec![],
                    }],
                    holds: vec![
                        ValueHold {
                            value: ValueId(0),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(1),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(2),
                            roots: vec![],
                        },
                    ],
                },
            ],
        ),
        // tests/borrowed-callable-arguments/v40/escape_callback_argument.wire
        "9504658bd740ec175e6e53320298d114c4cc5bde4ae493a1efb8afeefc148c19" => (
            ordinary(2),
            vec![
                FunctionPlan {
                    function: FunctionId(0),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0)],
                        initialized_out: vec![LocalId(0)],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
                FunctionPlan {
                    function: FunctionId(1),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0), LocalId(1)],
                        initialized_out: vec![],
                    }],
                    holds: vec![
                        ValueHold {
                            value: ValueId(0),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(1),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(2),
                            roots: vec![],
                        },
                    ],
                },
            ],
        ),
        // tests/borrowed-callable-arguments/v40/loan_revival.wire
        "00c022e566a4b855f5a3c97ae4eea0a216ee0f8dd05c01730a52f8b2d5561c7f" => (
            loan_revival(),
            vec![
                FunctionPlan {
                    function: FunctionId(0),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![],
                        initialized_out: vec![],
                    }],
                    holds: vec![
                        ValueHold {
                            value: ValueId(0),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(1),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(2),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(3),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(4),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(5),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(6),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(7),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(8),
                            roots: vec![HoldRoot::Local(LocalId(4))],
                        },
                        ValueHold {
                            value: ValueId(9),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(10),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(11),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(12),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(13),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(14),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(15),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(16),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(17),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(18),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(19),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(20),
                            roots: vec![HoldRoot::Local(LocalId(4))],
                        },
                        ValueHold {
                            value: ValueId(21),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(22),
                            roots: vec![HoldRoot::Local(LocalId(4))],
                        },
                        ValueHold {
                            value: ValueId(23),
                            roots: vec![HoldRoot::Local(LocalId(4))],
                        },
                        ValueHold {
                            value: ValueId(24),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(25),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(26),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(27),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(28),
                            roots: vec![],
                        },
                    ],
                },
                FunctionPlan {
                    function: FunctionId(1),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0)],
                        initialized_out: vec![],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
            ],
        ),
        // tests/borrowed-callable-arguments/v40/parent_valid_no_call_edge.wire
        "ceaee43c3f99be1a17967fc54093d1a10bf48590adc7af18b09f52a1b5c217ff" => (
            parent(false, false),
            vec![
                FunctionPlan {
                    function: FunctionId(0),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![],
                        initialized_out: vec![],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
                FunctionPlan {
                    function: FunctionId(1),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0), LocalId(1)],
                        initialized_out: vec![],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
                FunctionPlan {
                    function: FunctionId(2),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![],
                        initialized_out: vec![],
                    }],
                    holds: vec![
                        ValueHold {
                            value: ValueId(0),
                            roots: vec![HoldRoot::Capture(0)],
                        },
                        ValueHold {
                            value: ValueId(1),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(2),
                            roots: vec![],
                        },
                    ],
                },
            ],
        ),
        // tests/borrowed-callable-arguments/v40/parent_valid_with_call_edge.wire
        "c35f9e2a36451b7ad32eb4873236fc0ad7f05a7501daf00566049e14c90354c5" => (
            parent(false, true),
            vec![
                FunctionPlan {
                    function: FunctionId(0),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![],
                        initialized_out: vec![],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
                FunctionPlan {
                    function: FunctionId(1),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0), LocalId(1)],
                        initialized_out: vec![],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
                FunctionPlan {
                    function: FunctionId(2),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![],
                        initialized_out: vec![],
                    }],
                    holds: vec![
                        ValueHold {
                            value: ValueId(0),
                            roots: vec![HoldRoot::Capture(0)],
                        },
                        ValueHold {
                            value: ValueId(1),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(2),
                            roots: vec![],
                        },
                    ],
                },
                FunctionPlan {
                    function: FunctionId(3),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0), LocalId(1)],
                        initialized_out: vec![],
                    }],
                    holds: vec![
                        ValueHold {
                            value: ValueId(0),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(1),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(2),
                            roots: vec![],
                        },
                        ValueHold {
                            value: ValueId(3),
                            roots: vec![],
                        },
                    ],
                },
            ],
        ),
        // crates/zil-driver/tests/product_store_wire_replay_fixtures/v40/mutable-product.wire
        "d3c496c8fc68d4135731791e7fbb052eb977475433c4646e3692f2deb1c0e14f" => (
            mutable_product(),
            vec![
                FunctionPlan {
                    function: FunctionId(0),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0)],
                        initialized_out: vec![],
                    }],
                    holds: vec![ValueHold {
                        value: ValueId(0),
                        roots: vec![],
                    }],
                },
                FunctionPlan {
                    function: FunctionId(1),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![LocalId(0)],
                        initialized_out: vec![],
                    }],
                    holds: vec![
                        ValueHold {
                            value: ValueId(0),
                            roots: vec![HoldRoot::Parameter(0)],
                        },
                        ValueHold {
                            value: ValueId(1),
                            roots: vec![HoldRoot::Parameter(0)],
                        },
                        ValueHold {
                            value: ValueId(2),
                            roots: vec![],
                        },
                    ],
                },
                FunctionPlan {
                    function: FunctionId(2),
                    blocks: vec![BlockPlan {
                        block: BlockId(0),
                        initialized_in: vec![],
                        initialized_out: vec![],
                    }],
                    holds: vec![],
                },
            ],
        ),
        other => panic!("unknown archived raw fixture {other}"),
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = functions;
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap();
    let (decoded, claims) = zil_ir::wire::decode_module_candidate(&bytes, limits).unwrap();
    assert_eq!(decoded, module);
    assert_eq!(claims, plan);
    bytes
}
