//! Raw/Wire producer probes. These are independent of Verify and source Types.
use super::*;
use zil_base::{
    FileId, Fingerprint, ModuleId, ResultCategory, SCHEMA_VERSION, Span, TargetProfile,
};

fn s() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn op(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: s(),
    }
}
fn body(id: u32) -> TypedFunction {
    TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(id),
        name: format!("f{id}"),
        foreign: None,
        environment: None,
        parameters: vec![],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: s(),
    }
}
fn value(f: &mut TypedFunction, ty: Type, kind: InstructionKind) -> ValueId {
    let id = ValueId(f.values.len() as u32);
    f.values.push(ty);
    f.blocks[0].instructions.push(op(Some(id), kind));
    id
}
fn local(f: &mut TypedFunction, ty: Type) -> LocalId {
    let id = LocalId(f.locals.len() as u32);
    f.locals.push(LocalDecl {
        id,
        name: format!("l{}", id.0),
        ty,
        mutable: true,
        span: s(),
    });
    f.blocks[0]
        .instructions
        .push(op(None, InstructionKind::StorageLive(id)));
    id
}
fn emit(f: &mut TypedFunction, kind: InstructionKind) {
    f.blocks[0].instructions.push(op(None, kind));
}
fn payload() -> Type {
    Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn owner(f: &mut TypedFunction) -> LocalId {
    let ty = Type::Managed(Box::new(payload()));
    let local = local(f, ty.clone());
    let n = value(f, Type::I32, InstructionKind::Constant(Constant::I32(1)));
    let v = value(f, ty, InstructionKind::MakeManaged { fields: vec![n] });
    emit(f, InstructionKind::Store { local, value: v });
    local
}
fn fixture(actions: usize, copy: bool, mixed: bool) -> TypedModule {
    let mut f = body(0);
    emit(&mut f, InstructionKind::ScopeEnter { scope: ScopeId(0) });
    let older = mixed.then(|| owner(&mut f));
    let mut locals = vec![];
    for _ in 0..actions {
        let captures = if copy {
            vec![value(
                &mut f,
                Type::I32,
                InstructionKind::Constant(Constant::I32(3)),
            )]
        } else {
            vec![]
        };
        let local = local(&mut f, Type::Closure(ClosureId(0)));
        let v = value(
            &mut f,
            Type::Closure(ClosureId(0)),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures,
            },
        );
        emit(&mut f, InstructionKind::Store { local, value: v });
        emit(
            &mut f,
            InstructionKind::RegisterDefer {
                scope: ScopeId(0),
                local,
            },
        );
        locals.push(local);
    }
    let newer = mixed.then(|| owner(&mut f));
    if let Some(local) = newer {
        emit(&mut f, InstructionKind::DropManaged { local });
        emit(&mut f, InstructionKind::StorageDead(local));
    }
    for local in locals.into_iter().rev() {
        let callee = value(
            &mut f,
            Type::Closure(ClosureId(0)),
            InstructionKind::TakeLocal { local },
        );
        emit(
            &mut f,
            InstructionKind::CallClosureOnce {
                callee,
                arguments: vec![],
            },
        );
        emit(&mut f, InstructionKind::StorageDead(local));
    }
    if let Some(local) = older {
        emit(&mut f, InstructionKind::DropManaged { local });
        emit(&mut f, InstructionKind::StorageDead(local));
    }
    emit(
        &mut f,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        },
    );
    let mut action = body(1);
    action.environment = Some(ClosureId(0));
    emit(
        &mut action,
        InstructionKind::ScopeEnter { scope: ScopeId(0) },
    );
    emit(
        &mut action,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        },
    );
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![f, action],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(1),
            captures: if copy { vec![Type::I32] } else { vec![] },
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::FnOnce,
            lending_captures: vec![],
        }],
        resources: if mixed {
            vec![ResourceDefinition {
                name: "Token".into(),
                payload: payload(),
                teardown: None,
            }]
        } else {
            vec![]
        },
        unsupported: vec![],
    }
}
fn wire(module: &TypedModule) -> TypedModule {
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits)
            .unwrap();
    zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0
}
fn rejected(module: TypedModule, label: &str) {
    for input in [module.clone(), wire(&module)] {
        let errors = analyze(&input, &Limits::default()).expect_err(label);
        assert_ne!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{label}: {errors:?}"
        );
    }
}
fn take_index(module: &TypedModule) -> usize {
    module.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakeLocal { .. }))
        .unwrap()
}

#[test]
fn mandatory_empty_copy_and_same_type_actions_and_mixed_resources_are_accepted() {
    for (count, copy, mixed) in [
        (1, false, false),
        (1, true, false),
        (2, false, false),
        (2, true, true),
    ] {
        let module = fixture(count, copy, mixed);
        analyze(&module, &Limits::default()).unwrap();
        analyze(&wire(&module), &Limits::default()).unwrap();
    }
}

#[test]
fn dropping_or_escaping_environment_does_not_fulfill_the_action() {
    for copy in [false, true] {
        let mut module = fixture(1, copy, false);
        let position = take_index(&module);
        let f = &mut module.functions[0];
        let InstructionKind::TakeLocal { local } = f.blocks[0].instructions[position].kind else {
            unreachable!()
        };
        f.values.pop();
        f.blocks[0].instructions.splice(
            position..position + 2,
            [op(None, InstructionKind::DropClosure { local })],
        );
        rejected(module, "drop is not a call");
    }
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let callee = f.blocks[0].instructions[position].result.unwrap();
    let spare = LocalId(f.locals.len() as u32);
    f.locals.push(LocalDecl {
        id: spare,
        name: "escape".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: true,
        span: s(),
    });
    f.blocks[0]
        .instructions
        .insert(1, op(None, InstructionKind::StorageLive(spare)));
    f.blocks[0].instructions.splice(
        position + 2..position + 3,
        [
            op(
                None,
                InstructionKind::Store {
                    local: spare,
                    value: callee,
                },
            ),
            op(None, InstructionKind::DropClosure { local: spare }),
            op(None, InstructionKind::StorageDead(spare)),
        ],
    );
    rejected(module, "take then escape/drop is not a call");
}

#[test]
fn invocation_is_adjacent_and_can_only_start_cleanup() {
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let id = ValueId(f.values.len() as u32);
    f.values.push(Type::Unit);
    f.blocks[0].instructions.insert(
        position + 1,
        op(Some(id), InstructionKind::Constant(Constant::Unit)),
    );
    rejected(module, "intervening Copy operation");
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let id = ValueId(f.values.len() as u32);
    f.values.push(Type::Unit);
    f.blocks[0].instructions.insert(
        position + 2,
        op(Some(id), InstructionKind::Constant(Constant::Unit)),
    );
    rejected(module, "ordinary work after early call");
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let tail = f.blocks[0].instructions.split_off(position + 2);
    f.blocks[0].terminator = Terminator::Jump(BlockId(1));
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: tail,
        terminator: Terminator::Return(None),
    });
    rejected(module, "call suffix crosses CFG edge");
}

#[test]
fn pending_actions_cannot_be_loaded_borrowed_rewritten_or_discharged_twice() {
    for kind in [
        InstructionKind::Load(LocalId(0)),
        InstructionKind::BorrowLocal { local: LocalId(0) },
        InstructionKind::BorrowLocalMut { local: LocalId(0) },
    ] {
        let mut module = fixture(1, false, false);
        let position = take_index(&module);
        let f = &mut module.functions[0];
        let id = ValueId(f.values.len() as u32);
        f.values.push(if matches!(kind, InstructionKind::Load(_)) {
            Type::Closure(ClosureId(0))
        } else {
            Type::Address {
                inner: Box::new(Type::Closure(ClosureId(0))),
                mutable: matches!(kind, InstructionKind::BorrowLocalMut { .. }),
            }
        });
        f.blocks[0]
            .instructions
            .insert(position, op(Some(id), kind));
        rejected(module, "ordinary access to registered slot");
    }
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    module.functions[0].blocks[0].instructions.insert(
        position,
        op(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(0),
            },
        ),
    );
    rejected(module, "rewrite action");
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    let call = module.functions[0].blocks[0].instructions[position + 1].clone();
    module.functions[0].blocks[0]
        .instructions
        .insert(position + 2, call);
    rejected(module, "repeat actual call");
}

#[test]
fn registration_uses_current_scope_unique_initial_store_and_actual_age() {
    let mut module = fixture(1, false, false);
    let register = module.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::RegisterDefer { .. }))
        .unwrap();
    register.kind = InstructionKind::RegisterDefer {
        scope: ScopeId(8),
        local: LocalId(0),
    };
    rejected(module, "wrong registration scope");
    let mut module = fixture(1, false, false);
    let register = module.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::RegisterDefer { .. }))
        .unwrap();
    let duplicate = module.functions[0].blocks[0].instructions[register].clone();
    module.functions[0].blocks[0]
        .instructions
        .insert(register + 1, duplicate);
    rejected(module, "duplicate registration");
    let mut module = fixture(1, false, true);
    let f = &mut module.functions[0];
    let register = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::RegisterDefer { .. }))
        .unwrap();
    let instruction = f.blocks[0].instructions.remove(register);
    let after_newer_store = f.blocks[0]
        .instructions
        .iter()
        .rposition(|i| matches!(i.kind, InstructionKind::Store { .. }))
        .unwrap()
        + 1;
    f.blocks[0]
        .instructions
        .insert(after_newer_store, instruction);
    rejected(
        module,
        "registration separated from initial Store by newer owner",
    );
}

#[test]
fn registration_shape_initialization_and_call_source_are_not_frontend_claims() {
    let mut module = fixture(1, false, false);
    module.closures[0].kind = CallableKind::Fn;
    rejected(
        module,
        "repeatable Fn is not a must-invoke action environment",
    );
    let mut module = fixture(1, false, false);
    module.closures[0].parameters.push(Type::I32);
    rejected(module, "action has parameters");
    let mut module = fixture(1, false, false);
    module.closures[0].result = Type::I32;
    rejected(module, "action has non-Unit result");
    let mut module = fixture(1, false, false);
    let register = module.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::RegisterDefer { .. }))
        .unwrap();
    module.functions[0].blocks[0]
        .instructions
        .remove(register - 1);
    rejected(module, "uninitialized registered storage");
    let mut module = fixture(2, false, false);
    let position = take_index(&module);
    // The replacement has the same concrete environment type and dominates
    // this call, but is not the result of the immediately preceding Take.
    module.functions[0].blocks[0].instructions[position + 1].kind =
        InstructionKind::CallClosureOnce {
            callee: ValueId(0),
            arguments: vec![],
        };
    rejected(module, "same-type different SSA callee");
    let mut module = fixture(1, true, false);
    let position = take_index(&module);
    module.functions[0].blocks[0].instructions[position + 1].kind =
        InstructionKind::CallClosureOnce {
            callee: module.functions[0].blocks[0].instructions[position]
                .result
                .unwrap(),
            arguments: vec![ValueId(0)],
        };
    rejected(module, "nonzero-argument invocation");
}

#[test]
fn each_normal_recoverable_and_loop_exit_requires_the_actual_call() {
    for kind in [
        CleanupExitKind::Return,
        CleanupExitKind::Fallthrough,
        CleanupExitKind::Error,
        CleanupExitKind::Break,
        CleanupExitKind::Continue,
    ] {
        let mut module = fixture(1, false, false);
        if matches!(kind, CleanupExitKind::Break | CleanupExitKind::Continue) {
            let f = &mut module.functions[0];
            f.blocks[0].instructions.insert(
                1,
                op(None, InstructionKind::ScopeEnter { scope: ScopeId(1) }),
            );
            for i in &mut f.blocks[0].instructions {
                match &mut i.kind {
                    InstructionKind::RegisterDefer { scope, .. } => *scope = ScopeId(1),
                    InstructionKind::ScopeExit {
                        scope,
                        kind: actual,
                    } => {
                        *scope = ScopeId(1);
                        *actual = kind;
                    }
                    _ => {}
                }
            }
            f.blocks[0].terminator = Terminator::Jump(BlockId(1));
            f.blocks.push(BasicBlock {
                id: BlockId(1),
                instructions: vec![op(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                )],
                terminator: Terminator::Return(None),
            });
        } else if kind == CleanupExitKind::Error {
            let f = &mut module.functions[0];
            let union = Type::ErrorUnion {
                error: Box::new(Type::I32),
                result: Box::new(Type::Unit),
            };
            f.return_type = union.clone();
            f.values.insert(0, Type::I32);
            f.values.insert(1, union);
            for i in &mut f.blocks[0].instructions {
                if let Some(result) = &mut i.result {
                    result.0 += 2;
                }
                match &mut i.kind {
                    InstructionKind::Store { value, .. } => value.0 += 2,
                    InstructionKind::CallClosureOnce { callee, .. } => callee.0 += 2,
                    InstructionKind::ScopeExit { kind: actual, .. } => *actual = kind,
                    _ => {}
                }
            }
            f.blocks[0].instructions.splice(
                1..1,
                [
                    op(
                        Some(ValueId(0)),
                        InstructionKind::Constant(Constant::I32(1)),
                    ),
                    op(
                        Some(ValueId(1)),
                        InstructionKind::MakeErrorUnion {
                            is_error: true,
                            value: ValueId(0),
                        },
                    ),
                ],
            );
            f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
        } else {
            for i in &mut module.functions[0].blocks[0].instructions {
                if let InstructionKind::ScopeExit { kind: actual, .. } = &mut i.kind {
                    *actual = kind;
                }
            }
        }
        analyze(&module, &Limits::default()).unwrap();
        let position = take_index(&module);
        let f = &mut module.functions[0];
        let InstructionKind::TakeLocal { local } = f.blocks[0].instructions[position].kind else {
            unreachable!()
        };
        f.values.pop();
        f.blocks[0].instructions.splice(
            position..position + 2,
            [op(None, InstructionKind::DropClosure { local })],
        );
        rejected(
            module,
            &format!("{kind:?} cannot claim affine Drop as actual defer invocation"),
        );
    }
}

#[test]
fn mixed_cleanup_and_same_type_action_order_is_reconstructed() {
    let mut module = fixture(2, false, false);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let first: Vec<_> = f.blocks[0]
        .instructions
        .drain(position..position + 3)
        .collect();
    f.blocks[0]
        .instructions
        .splice(position + 3..position + 3, first);
    rejected(
        module,
        "same concrete closure type cleaned in declaration order",
    );
    let mut module = fixture(1, false, true);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let older_drop = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { local: LocalId(0) }))
        .unwrap();
    let earlier: Vec<_> = f.blocks[0]
        .instructions
        .drain(older_drop..older_drop + 2)
        .collect();
    f.blocks[0].instructions.splice(position..position, earlier);
    rejected(module, "older owner before pending action");
    let mut module = fixture(1, false, true);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let newer_drop = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { local: LocalId(2) }))
        .unwrap();
    let later: Vec<_> = f.blocks[0]
        .instructions
        .drain(newer_drop..newer_drop + 2)
        .collect();
    f.blocks[0]
        .instructions
        .splice(position + 1..position + 1, later);
    rejected(module, "action before newer owner");
}

#[test]
fn outer_action_may_cross_internal_loop_but_not_reexecute_its_registration() {
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    let f = &mut module.functions[0];
    let cleanup = f.blocks[0].instructions.split_off(position);
    f.blocks[0].terminator = Terminator::Jump(BlockId(1));
    let condition = ValueId(f.values.len() as u32);
    f.values.push(Type::Bool);
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![op(
            Some(condition),
            InstructionKind::Constant(Constant::Bool(false)),
        )],
        terminator: Terminator::Branch {
            condition,
            then_block: BlockId(1),
            else_block: BlockId(2),
        },
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: cleanup,
        terminator: Terminator::Return(None),
    });
    analyze(&module, &Limits::default()).unwrap();
    let mut reenter = module.clone();
    reenter.functions[0].blocks[1].terminator = Terminator::Jump(BlockId(0));
    rejected(reenter, "backedge reenters outstanding activation");
    let mut reopened = fixture(1, false, false);
    reopened.functions[0].blocks[0].terminator = Terminator::Jump(BlockId(0));
    for instruction in &mut reopened.functions[0].blocks[0].instructions {
        if let InstructionKind::ScopeExit { kind, .. } = &mut instruction.kind {
            *kind = CleanupExitKind::Fallthrough;
        }
    }
    rejected(
        reopened,
        "backedge cannot reopen the already closed function root",
    );
    let mut abandoned = module;
    abandoned.functions[0].blocks[1].terminator = Terminator::Unreachable;
    rejected(abandoned, "Unreachable is not Trap abort exemption");
}

#[test]
fn trap_has_no_unwind_obligation_and_scope_storage_exits_do() {
    let mut module = fixture(1, false, false);
    let position = take_index(&module);
    module.functions[0].values.pop();
    module.functions[0].blocks[0]
        .instructions
        .truncate(position);
    module.functions[0].blocks[0].terminator = Terminator::Trap;
    analyze(&module, &Limits::default()).unwrap();
    let mut dead = module.clone();
    dead.functions[0].blocks[0]
        .instructions
        .push(op(None, InstructionKind::StorageDead(LocalId(0))));
    rejected(dead, "StorageDead with pending action");
    let mut scope = module;
    scope.functions[0].blocks[0].instructions.push(op(
        None,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Fallthrough,
        },
    ));
    rejected(scope, "ScopeExit with pending action");
}

#[test]
fn defer_budget_boundary_is_exact_and_fresh() {
    let module = fixture(2, true, true);
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        match analyze(
            &module,
            &Limits {
                max_analysis_steps: mid,
                ..Limits::default()
            },
        ) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                low = mid + 1;
            }
        }
    }
    assert!(high > 0);
    for steps in [high, high - 1, high] {
        let result = analyze(
            &module,
            &Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            },
        );
        if steps == high {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            );
        }
    }
    eprintln!("Analyze defer raw budget N={high}");
}
