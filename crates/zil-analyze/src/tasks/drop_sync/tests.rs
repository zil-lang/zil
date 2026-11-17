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

// A raw cleanup-predicate control, not a Source/Types or native receipt. The
// actual Ready observation belongs to the callback's current owning occupant.
fn closed_array(replace: bool) -> (TypedModule, usize) {
    let mut module = crate::static_utf8_tests::module();
    let template = module.functions[0].clone();
    let task = Type::Managed(Box::new(Type::Task {
        result: Box::new(Type::I32), retained_life: super::super::empty_context(),
    }));
    let closure = Type::Closure(ClosureId(0));
    let address = Type::Address { mutable: true, inner: Box::new(closure.clone()) };
    let array = Type::Array { element: Box::new(closure.clone()), length: 1 };
    let poll = super::super::task_poll_type(&Type::Unit);
    let life = StorageLifeContract { fixed_binder_count: 0, invocation_binder_count: 0,
        bindings: vec![], static_bindings: vec![], classes: vec![] };
    let mut entry = template.clone();
    entry.name = "close_actual_capture".into();
    entry.environment = Some(ClosureId(0));
    entry.storage_life = Some(life.clone());
    entry.parameters = vec![];
    entry.locals = vec![];
    entry.return_type = poll.clone();
    entry.values = vec![Type::Address { mutable: true, inner: Box::new(task.clone()) }, poll.clone()];
    entry.blocks[0].instructions = vec![
        instruction(Some(0), InstructionKind::BorrowCapture { index: 0, mutable: true }),
        instruction(Some(1), InstructionKind::TaskPollClose { task: ValueId(0) }),
    ];
    entry.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    let mut caller = template;
    caller.id = FunctionId(1);
    caller.name = "same_typed_callback_replacement".into();
    caller.parameters = vec![LocalId(0), LocalId(1)];
    caller.return_type = Type::Unit;
    caller.locals = vec![
        LocalDecl { id: LocalId(0), name: "first".into(), ty: task.clone(), mutable: false, span: Default::default() },
        LocalDecl { id: LocalId(1), name: "second".into(), ty: task.clone(), mutable: false, span: Default::default() },
        LocalDecl { id: LocalId(2), name: "slot".into(), ty: array.clone(), mutable: true, span: Default::default() },
        LocalDecl { id: LocalId(3), name: "pointer".into(), ty: address.clone(), mutable: true, span: Default::default() },
    ];
    caller.values = vec![task.clone(), closure.clone(), array, address.clone(), address.clone(), address,
        poll.clone(), Type::I32, Type::I32, Type::Bool, task.clone(), closure.clone()];
    let mut cleanup = vec![];
    if replace {
        cleanup.extend([
            instruction(None, InstructionKind::DropOwnedArrayElement { local: LocalId(2), index: 0 }),
            instruction(Some(10), InstructionKind::TakeLocal { local: LocalId(1) }),
            instruction(Some(11), InstructionKind::MakeClosure { closure: ClosureId(0), captures: vec![ValueId(10)] }),
            instruction(None, InstructionKind::StoreOwnedArrayElement { local: LocalId(2), index: 0, value: ValueId(11) }),
        ]);
    }
    let point = cleanup.len();
    cleanup.push(instruction(None, InstructionKind::DropOwnedArray { local: LocalId(2) }));
    caller.blocks = vec![
        BasicBlock { id: BlockId(0), instructions: vec![
            instruction(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
            instruction(Some(1), InstructionKind::MakeClosure { closure: ClosureId(0), captures: vec![ValueId(0)] }),
            instruction(Some(2), InstructionKind::MakeOwnedArray { elements: vec![ValueId(1)] }),
            instruction(None, InstructionKind::StorageLive(LocalId(2))),
            instruction(None, InstructionKind::Store { local: LocalId(2), value: ValueId(2) }),
            instruction(None, InstructionKind::StorageLive(LocalId(3))),
        ], terminator: Terminator::Jump(BlockId(1)) },
        BasicBlock { id: BlockId(1), instructions: vec![
            instruction(Some(3), InstructionKind::BorrowOwnedArrayElement { local: LocalId(2), index: 0, mutable: true }),
            instruction(None, InstructionKind::Store { local: LocalId(3), value: ValueId(3) }),
            instruction(Some(4), InstructionKind::Load(LocalId(3))),
            instruction(Some(5), InstructionKind::ReborrowAddress { address: ValueId(4), mutable: true }),
            instruction(Some(6), InstructionKind::CallClosureBorrowed { callee: ValueId(5), kind: CallableKind::FnMut, arguments: vec![] }),
            instruction(Some(7), InstructionKind::VariantTag { value: ValueId(6) }),
            instruction(Some(8), InstructionKind::Constant(Constant::I32(1))),
            instruction(Some(9), InstructionKind::Binary { op: BinaryOp::Equal, left: ValueId(7), right: ValueId(8) }),
        ], terminator: Terminator::Branch { condition: ValueId(9), then_block: BlockId(2), else_block: BlockId(3) } },
        BasicBlock { id: BlockId(2), instructions: cleanup, terminator: Terminator::Return(None) },
        BasicBlock { id: BlockId(3), instructions: vec![], terminator: Terminator::Jump(BlockId(1)) },
    ];
    module.closures = vec![ClosureDefinition { id: ClosureId(0), function: FunctionId(0),
        storage_life: Some(life), captures: vec![task], parameters: vec![], result: poll,
        kind: CallableKind::FnMut, lending_captures: vec![] }];
    module.functions = vec![entry, caller];
    (module, point)
}

#[test]
fn closed_callback_array_retains_actual_ready_and_rejects_same_type_replacement() {
    let limits = zil_base::Limits::default();
    let (module, point) = closed_array(false);
    let mut budget = Budget::new(&limits);
    budget.task_origin_roots = true;
    require_original(&module, &module.functions[1], LocalId(2), (2, point), &mut budget, None).unwrap();
    // One actual predecessor pays the child debt, the other still owns the
    // same already-Closed occupant. The join proves only conditional cleanup.
    let (mut conditional, _) = closed_array(false);
    let function = &mut conditional.functions[1];
    function.parameters.push(LocalId(4));
    function.locals.push(LocalDecl { id: LocalId(4), name: "remove".into(), ty: Type::Bool,
        mutable: false, span: Default::default() });
    function.values.push(Type::Bool);
    function.blocks[2].instructions = vec![instruction(Some(12), InstructionKind::Load(LocalId(4)))];
    function.blocks[2].terminator = Terminator::Branch { condition: ValueId(12),
        then_block: BlockId(4), else_block: BlockId(5) };
    function.blocks.extend([
        BasicBlock { id: BlockId(4), instructions: vec![instruction(None,
            InstructionKind::DropOwnedArrayElement { local: LocalId(2), index: 0 })],
            terminator: Terminator::Jump(BlockId(6)) },
        BasicBlock { id: BlockId(5), instructions: vec![], terminator: Terminator::Jump(BlockId(6)) },
        BasicBlock { id: BlockId(6), instructions: vec![instruction(None,
            InstructionKind::DropOwnedArray { local: LocalId(2) })], terminator: Terminator::Return(None) },
    ]);
    let mut budget = Budget::new(&limits);
    budget.task_origin_roots = true;
    require_original(&conditional, &conditional.functions[1], LocalId(2), (6, 0), &mut budget, None).unwrap();
    let (module, point) = closed_array(true);
    let mut budget = Budget::new(&limits);
    budget.task_origin_roots = true;
    let errors = require_original(&module, &module.functions[1], LocalId(2), (2, point), &mut budget, None).unwrap_err();
    assert!(errors.iter().any(|error| error.category == zil_base::ResultCategory::SafeReject), "{errors:?}");
}
