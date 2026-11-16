//! Analyze-only raw controls for declaration and whole-program cleanup roots.
//! They do not stand for a Source/Types or native acceptance receipt.
use super::*;

fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction { result: result.map(ValueId), kind, span: Default::default() }
}

fn roots() -> TypedModule {
    let mut module = crate::static_utf8_tests::module();
    let template = module.functions[0].clone();
    let task = Type::Managed(Box::new(Type::Task {
        result: Box::new(Type::I32), retained_life: super::super::empty_context(),
    }));
    let mut worker = template.clone();
    worker.name = "local_worker".into();
    worker.return_type = Type::I32;
    worker.values = vec![Type::I32];
    worker.blocks[0].instructions = vec![instruction(Some(0), InstructionKind::Constant(Constant::I32(1)))];
    worker.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    let mut incoming = template.clone();
    incoming.id = FunctionId(1);
    incoming.name = "incoming".into();
    incoming.return_type = Type::Unit;
    incoming.values = vec![task.clone()];
    incoming.parameters = vec![LocalId(0)];
    incoming.locals = vec![
        LocalDecl { id: LocalId(0), name: "incoming".into(), ty: task.clone(), mutable: false, span: Default::default() },
        LocalDecl { id: LocalId(1), name: "alias".into(), ty: task.clone(), mutable: true, span: Default::default() },
    ];
    incoming.blocks[0].instructions = vec![
        instruction(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
        instruction(None, InstructionKind::StorageLive(LocalId(1))),
        instruction(None, InstructionKind::Store { local: LocalId(1), value: ValueId(0) }),
        instruction(None, InstructionKind::DropManaged { local: LocalId(1) }),
    ];
    incoming.blocks[0].terminator = Terminator::Return(None);
    let mut caller = template;
    caller.id = FunctionId(2);
    caller.name = "safe_actual_caller".into();
    caller.return_type = Type::Unit;
    caller.values = vec![
        super::super::task_create_type(&Type::I32, super::super::empty_context()), Type::Bool, task,
    ];
    caller.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![
                instruction(Some(0), InstructionKind::TaskCreate {
                    worker: FunctionId(0), arguments: vec![], port_bindings: vec![],
                }),
                instruction(Some(1), InstructionKind::ErrorTag { value: ValueId(0) }),
            ],
            terminator: Terminator::Branch { condition: ValueId(1), then_block: BlockId(1), else_block: BlockId(2) },
        },
        BasicBlock { id: BlockId(1), instructions: vec![], terminator: Terminator::Return(None) },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![
                instruction(Some(2), InstructionKind::TakeErrorUnion { value: ValueId(0), is_error: false }),
                instruction(None, InstructionKind::Call { function: FunctionId(1), arguments: vec![ValueId(2)] }),
            ],
            terminator: Terminator::Return(None),
        },
    ];
    module.functions = vec![worker, incoming, caller];
    module
}

#[test]
fn original_cleanup_cannot_borrow_an_incoming_roots_safe_actual_caller() {
    let module = roots();
    let limits = zil_base::Limits::default();
    let mut raw = Budget::new(&limits);
    raw.task_origin_roots = true;
    require(&module, &module.functions[1], LocalId(1), (0, 3), &mut raw, None).unwrap();
    let mut original = Budget::new(&limits);
    original.task_origin_roots = true;
    let errors = require_original(&module, &module.functions[1], LocalId(1), (0, 3), &mut original, None).unwrap_err();
    assert!(errors.iter().any(|error| error.category == zil_base::ResultCategory::SafeReject
        && error.message.contains("declaration-local root")), "{errors:?}");

    let mut local = roots();
    let mut local_slot = module.functions[1].locals[1].clone();
    local_slot.id = LocalId(0);
    local.functions[2].locals = vec![local_slot];
    local.functions[2].blocks[2].instructions.truncate(1);
    local.functions[2].blocks[2].instructions.extend([
        instruction(None, InstructionKind::StorageLive(LocalId(0))),
        instruction(None, InstructionKind::Store { local: LocalId(0), value: ValueId(2) }),
        instruction(None, InstructionKind::DropManaged { local: LocalId(0) }),
    ]);
    let mut original = Budget::new(&limits);
    original.task_origin_roots = true;
    require_original(&local, &local.functions[2], LocalId(0), (2, 3), &mut original, None).unwrap();
}
