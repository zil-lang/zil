//! Independent raw/wire trust-boundary cases. These do not claim source lowering
//! or native execution coverage; those belong to the frontend/integration gates.
use super::*;
use zil_base::{FileId, ModuleId};

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn union() -> Type {
    Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(Type::I64),
    }
}
fn payload() -> Type {
    Type::Record {
        name: "ScopeToken".into(),
        fields: vec![("code".into(), Type::I32)],
    }
}
fn owned() -> Type {
    Type::Managed(Box::new(payload()))
}
fn ins(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: span(),
    }
}
fn enter(scope: u32) -> Instruction {
    ins(
        None,
        InstructionKind::ScopeEnter {
            scope: ScopeId(scope),
        },
    )
}
fn exit(scope: u32, kind: CleanupExitKind) -> Instruction {
    ins(
        None,
        InstructionKind::ScopeExit {
            scope: ScopeId(scope),
            kind,
        },
    )
}
fn local(id: u32, ty: Type) -> LocalDecl {
    LocalDecl {
        id: LocalId(id),
        name: format!("local{id}"),
        ty,
        mutable: true,
        span: span(),
    }
}
struct Body(TypedFunction);
impl Body {
    fn new(result: Type) -> Self {
        Self(TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(0),
            name: "main".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: result,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![],
            entry: BlockId(0),
            span: span(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![enter(0)],
                terminator: Terminator::Trap,
            }],
        })
    }
    fn op(&mut self, kind: InstructionKind) {
        self.0.blocks[0].instructions.push(ins(None, kind));
    }
    fn value(&mut self, ty: Type, kind: InstructionKind) -> ValueId {
        let value = ValueId(self.0.values.len() as u32);
        self.0.values.push(ty);
        self.0.blocks[0].instructions.push(ins(Some(value), kind));
        value
    }
    fn storage(&mut self, ty: Type) -> LocalId {
        let id = LocalId(self.0.locals.len() as u32);
        self.0.locals.push(local(id.0, ty));
        self.op(InstructionKind::StorageLive(id));
        id
    }
    fn tag(&mut self, is_error: bool) -> ValueId {
        let (ty, constant) = if is_error {
            (Type::I32, Constant::I32(7))
        } else {
            (Type::I64, Constant::I64(42))
        };
        let value = self.value(ty, InstructionKind::Constant(constant));
        self.value(union(), InstructionKind::MakeErrorUnion { is_error, value })
    }
    fn resource(&mut self) -> ValueId {
        let field = self.value(Type::I32, InstructionKind::Constant(Constant::I32(65)));
        self.value(
            owned(),
            InstructionKind::MakeManaged {
                fields: vec![field],
            },
        )
    }
    fn close(mut self, value: Option<ValueId>, kind: CleanupExitKind) -> TypedFunction {
        self.0.blocks[0].instructions.push(exit(0, kind));
        self.0.blocks[0].terminator = Terminator::Return(value);
        self.0
    }
}
fn module(function: TypedFunction) -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"independent synchronous error/scope fixtures"),
        target: TargetProfile::host_llvm(),
        functions: vec![function],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "ScopeToken".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn verify(function: TypedFunction) -> CompileResult<VerifiedModule> {
    let module = module(function);
    let plan = resource_tests::plan(&module);
    verify_module(
        module,
        plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
}
fn rejected(function: TypedFunction, text: &str) {
    let diagnostics = verify(function).expect_err("hostile candidate must fail");
    assert!(
        diagnostics.iter().any(|d| d.message.contains(text)),
        "{diagnostics:?}"
    );
}
fn tagged(is_error: bool) -> TypedFunction {
    let mut body = Body::new(union());
    let value = body.tag(is_error);
    body.close(
        Some(value),
        if is_error {
            CleanupExitKind::Error
        } else {
            CleanupExitKind::Return
        },
    )
}

#[test]
fn error_and_success_values_return_as_ordinary_checked_data() {
    verify(tagged(true)).unwrap();
    verify(tagged(false)).unwrap();
}

#[test]
fn error_injection_requires_the_selected_payload_and_union_result_type() {
    let mut function = tagged(true);
    function.values[0] = Type::I64;
    function.blocks[0].instructions[1].kind = InstructionKind::Constant(Constant::I64(7));
    rejected(function, "payload does not match");
    let mut function = tagged(true);
    function.values[1] = Type::I32;
    function.return_type = Type::I32;
    rejected(function, "must produce an error-union");
}

#[test]
fn error_tag_is_boolean_and_projection_checks_payload_type() {
    let mut body = Body::new(Type::Bool);
    let value = body.tag(true);
    let tag = body.value(Type::Bool, InstructionKind::ErrorTag { value });
    let correct = body.close(Some(tag), CleanupExitKind::Return);
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.values[tag.0 as usize] = Type::I32;
    wrong.return_type = Type::I32;
    rejected(wrong, "instruction result presence or type");

    let mut body = Body::new(Type::I64);
    let value = body.tag(true);
    // Wrong active tag is intentionally a defined runtime trap. Static admission
    // proves the projection's payload type, not an impossible success value.
    let result = body.value(
        Type::I64,
        InstructionKind::ReadErrorUnion {
            value,
            is_error: false,
        },
    );
    let correct = body.close(Some(result), CleanupExitKind::Return);
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.values[result.0 as usize] = Type::I32;
    wrong.return_type = Type::I32;
    rejected(wrong, "instruction result presence or type");
}

#[test]
fn error_operations_cannot_treat_an_integer_as_a_union() {
    for projection in [false, true] {
        let mut body = Body::new(if projection { Type::I32 } else { Type::Bool });
        let value = body.value(Type::I32, InstructionKind::Constant(Constant::I32(7)));
        let kind = if projection {
            InstructionKind::ReadErrorUnion {
                value,
                is_error: true,
            }
        } else {
            InstructionKind::ErrorTag { value }
        };
        let result = body.value(body.0.return_type.clone(), kind);
        rejected(
            body.close(Some(result), CleanupExitKind::Return),
            "requires an error-union",
        );
    }
}

#[test]
fn error_operands_still_require_valid_ids_and_definition_dominance() {
    let mut wrong = tagged(true);
    wrong.blocks[0].instructions[2].kind = InstructionKind::MakeErrorUnion {
        is_error: true,
        value: ValueId(u32::MAX),
    };
    rejected(wrong, "absent SSA value");
    let mut wrong = tagged(true);
    wrong.blocks[0].instructions.swap(1, 2);
    rejected(wrong, "does not precede and dominate");
}

#[test]
fn owned_success_payload_requires_an_explicit_result_mode() {
    let mut body = Body::new(Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(owned()),
    });
    let value = body.resource();
    let result = body.value(
        body.0.return_type.clone(),
        InstructionKind::MakeErrorUnion {
            is_error: false,
            value,
        },
    );
    rejected(
        body.close(Some(result), CleanupExitKind::Return),
        "owned success result requires an explicit create or give result mode",
    );
}

#[test]
fn error_types_and_error_instructions_require_a_complete_root_scope() {
    let mut function = tagged(true);
    function.blocks[0].instructions.retain(|i| {
        !matches!(
            i.kind,
            InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
        )
    });
    rejected(function, "must begin with a root");
    let mut body = Body::new(Type::Unit);
    body.0.locals.push(local(0, Type::Tuple(vec![union()])));
    let mut function = body.close(None, CleanupExitKind::Return);
    function.blocks[0].instructions.clear();
    rejected(function, "must begin with a root");
}

#[test]
fn nested_scope_storage_must_die_before_its_exit() {
    let mut body = Body::new(Type::Unit);
    body.op(InstructionKind::ScopeEnter { scope: ScopeId(9) });
    let storage = body.storage(Type::I32);
    body.op(InstructionKind::StorageDead(storage));
    body.op(InstructionKind::ScopeExit {
        scope: ScopeId(9),
        kind: CleanupExitKind::Fallthrough,
    });
    let correct = body.close(None, CleanupExitKind::Return);
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::StorageDead(_)));
    rejected(wrong, "scope exit leaves live storage");
}

#[test]
fn wrong_scope_exit_duplicate_scope_and_scope_reentry_are_rejected() {
    let mut wrong = tagged(true);
    wrong.blocks[0].instructions.last_mut().unwrap().kind = InstructionKind::ScopeExit {
        scope: ScopeId(7),
        kind: CleanupExitKind::Error,
    };
    rejected(wrong, "innermost active scope");
    let mut wrong = tagged(true);
    wrong.blocks[0].instructions.insert(1, enter(0));
    rejected(wrong, "exactly one syntactic enter");
    let mut wrong = tagged(true);
    wrong.blocks[0].instructions.push(enter(8));
    rejected(wrong, "root cannot be reopened");
}

#[test]
fn sparse_scope_ids_are_opaque_but_unreachable_scope_claims_are_rejected() {
    let mut correct = tagged(true);
    correct.blocks[0].instructions[0].kind = InstructionKind::ScopeEnter {
        scope: ScopeId(u32::MAX),
    };
    correct.blocks[0].instructions.last_mut().unwrap().kind = InstructionKind::ScopeExit {
        scope: ScopeId(u32::MAX),
        kind: CleanupExitKind::Error,
    };
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![enter(1)],
        terminator: Terminator::Trap,
    });
    rejected(wrong, "unreachable scope operations");
}

#[test]
fn storage_reactivation_or_death_without_active_storage_is_rejected() {
    for reactivation in [false, true] {
        let mut body = Body::new(Type::Unit);
        let storage = body.storage(Type::I32);
        if reactivation {
            body.op(InstructionKind::StorageLive(storage));
        }
        body.op(InstructionKind::StorageDead(storage));
        if !reactivation {
            body.op(InstructionKind::StorageDead(storage));
        }
        rejected(
            body.close(None, CleanupExitKind::Return),
            if reactivation {
                "already live local"
            } else {
                "no active local storage"
            },
        );
    }
}

#[test]
fn local_writes_after_storage_death_cannot_reopen_a_closed_scope() {
    let mut body = Body::new(Type::Unit);
    let storage = body.storage(Type::I32);
    let value = body.value(Type::I32, InstructionKind::Constant(Constant::I32(1)));
    body.op(InstructionKind::StorageDead(storage));
    body.op(InstructionKind::Store {
        local: storage,
        value,
    });
    rejected(
        body.close(None, CleanupExitKind::Return),
        "outside its active storage scope",
    );
}

#[test]
fn parameter_storage_belongs_to_the_root_without_reinitializing_it() {
    let mut body = Body::new(Type::I32);
    body.0.locals.push(local(0, Type::I32));
    body.0.parameters.push(LocalId(0));
    let value = body.value(Type::I32, InstructionKind::Load(LocalId(0)));
    body.op(InstructionKind::StorageDead(LocalId(0)));
    let correct = body.close(Some(value), CleanupExitKind::Return);
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::StorageDead(_)));
    rejected(wrong, "scope exit leaves live storage");
}

#[test]
fn return_and_executable_instructions_cannot_skip_root_closure() {
    let mut wrong = tagged(true);
    wrong.blocks[0].instructions.pop();
    rejected(wrong, "return must explicitly close");
    let mut wrong = tagged(true);
    wrong.values.push(Type::I32);
    wrong.blocks[0].instructions.push(ins(
        Some(ValueId(2)),
        InstructionKind::Constant(Constant::I32(0)),
    ));
    rejected(wrong, "outside the active function root");
}

fn branch_join(storage: bool) -> TypedFunction {
    let mut body = Body::new(Type::Unit);
    let condition = body.value(Type::Bool, InstructionKind::Constant(Constant::Bool(true)));
    body.0.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    let instructions = if storage {
        body.0.locals.push(local(0, Type::I32));
        vec![ins(None, InstructionKind::StorageLive(LocalId(0)))]
    } else {
        vec![enter(1)]
    };
    body.0.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions,
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![exit(0, CleanupExitKind::Return)],
            terminator: Terminator::Return(None),
        },
    ]);
    body.0
}

#[test]
fn cfg_join_must_agree_on_scope_stack_and_live_storage_owners() {
    rejected(branch_join(false), "CFG join has incompatible cleanup");
    rejected(branch_join(true), "CFG join has incompatible cleanup");
}

#[test]
fn a_loop_may_reactivate_one_syntactic_scope_after_closing_it() {
    let mut body = Body::new(Type::Unit);
    let condition = body.value(Type::Bool, InstructionKind::Constant(Constant::Bool(true)));
    body.0.locals.push(local(0, Type::I32));
    body.0.blocks[0].terminator = Terminator::Jump(BlockId(1));
    body.0.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: vec![
                enter(1),
                ins(None, InstructionKind::StorageLive(LocalId(0))),
                ins(None, InstructionKind::StorageDead(LocalId(0))),
                exit(1, CleanupExitKind::Fallthrough),
            ],
            terminator: Terminator::Branch {
                condition,
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![exit(0, CleanupExitKind::Return)],
            terminator: Terminator::Return(None),
        },
    ]);
    let correct = body.0;
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.blocks[1].instructions.pop();
    rejected(wrong, "CFG join has incompatible cleanup");
}

#[test]
fn error_cleanup_cannot_claim_success_unknown_tag_or_abort_as_propagation() {
    let mut wrong = tagged(false);
    wrong.blocks[0].instructions.last_mut().unwrap().kind = InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Error,
    };
    rejected(wrong, "independently constructed error value");
    let mut wrong = tagged(true);
    wrong.blocks[0].terminator = Terminator::Trap;
    rejected(wrong, "independently constructed error value");

    let mut body = Body::new(union());
    body.0.locals.push(local(0, union()));
    body.0.parameters.push(LocalId(0));
    let value = body.value(union(), InstructionKind::Load(LocalId(0)));
    body.op(InstructionKind::StorageDead(LocalId(0)));
    // An ordinary return may return either tag. Its marker is not a success proof.
    let correct = body.close(Some(value), CleanupExitKind::Return);
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.blocks[0].instructions.last_mut().unwrap().kind = InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Error,
    };
    rejected(wrong, "independently constructed error value");
}

#[test]
fn loop_exit_kinds_require_a_jump_and_must_retain_the_function_root() {
    for kind in [CleanupExitKind::Break, CleanupExitKind::Continue] {
        let mut wrong = tagged(false);
        wrong.blocks[0].instructions.last_mut().unwrap().kind = InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind,
        };
        rejected(wrong, "loop cleanup requires a jump");
    }
}

#[test]
fn explicit_break_and_continue_close_the_body_and_retain_the_outer_scope() {
    let mut body = Body::new(Type::Unit);
    let condition = body.value(Type::Bool, InstructionKind::Constant(Constant::Bool(true)));
    body.0.blocks[0].terminator = Terminator::Jump(BlockId(1));
    body.0.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: vec![enter(1)],
            terminator: Terminator::Branch {
                condition,
                then_block: BlockId(2),
                else_block: BlockId(3),
            },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![exit(1, CleanupExitKind::Continue)],
            terminator: Terminator::Jump(BlockId(1)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![exit(1, CleanupExitKind::Break)],
            terminator: Terminator::Jump(BlockId(4)),
        },
        BasicBlock {
            id: BlockId(4),
            instructions: vec![exit(0, CleanupExitKind::Return)],
            terminator: Terminator::Return(None),
        },
    ]);
    let correct = body.0;
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.blocks[2].instructions[0].kind = InstructionKind::ScopeExit {
        scope: ScopeId(1),
        kind: CleanupExitKind::Return,
    };
    rejected(wrong, "Return cleanup must lead to an ordinary return");
}

fn owned_error() -> TypedFunction {
    let mut body = Body::new(union());
    let storage = body.storage(owned());
    let resource = body.resource();
    body.op(InstructionKind::Store {
        local: storage,
        value: resource,
    });
    let result = body.tag(true);
    body.op(InstructionKind::DropManaged { local: storage });
    body.op(InstructionKind::StorageDead(storage));
    body.close(Some(result), CleanupExitKind::Error)
}

#[test]
fn error_return_preserves_independent_resource_cleanup_and_duplicate_checks() {
    let correct = owned_error();
    verify(correct.clone()).unwrap();
    let mut wrong = correct.clone();
    wrong.blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropManaged { .. }));
    rejected(wrong, "outstanding resource responsibility");
    let mut wrong = correct;
    let index = wrong.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        .unwrap();
    wrong.blocks[0].instructions.insert(
        index,
        ins(None, InstructionKind::DropManaged { local: LocalId(0) }),
    );
    rejected(wrong, "duplicate automatic cleanup");
}

#[test]
fn error_return_does_not_waive_reverse_successful_construction_order() {
    let mut body = Body::new(union());
    let first = body.storage(owned());
    let second = body.storage(owned());
    for storage in [first, second] {
        let value = body.resource();
        body.op(InstructionKind::Store {
            local: storage,
            value,
        });
    }
    let result = body.tag(true);
    for storage in [first, second] {
        body.op(InstructionKind::DropManaged { local: storage });
        body.op(InstructionKind::StorageDead(storage));
    }
    rejected(
        body.close(Some(result), CleanupExitKind::Error),
        "reverse successful",
    );
}

#[test]
fn incomplete_resource_payload_cannot_be_turned_into_complete_teardown_debt() {
    let mut wrong = owned_error();
    for i in &mut wrong.blocks[0].instructions {
        if let InstructionKind::MakeManaged { fields } = &mut i.kind {
            fields.clear();
        }
    }
    rejected(wrong, "field");
}

#[test]
fn error_return_rejects_unsubmitted_owned_ssa_even_without_any_local_debt() {
    let mut body = Body::new(union());
    let _pending_argument = body.resource();
    let result = body.tag(true);
    rejected(
        body.close(Some(result), CleanupExitKind::Error),
        "normal return leaves an outstanding resource",
    );
}

#[test]
fn scope_local_contract_and_whole_function_ssa_contract_remain_distinct() {
    let mut body = Body::new(union());
    let outer = body.storage(owned());
    body.op(InstructionKind::ScopeEnter { scope: ScopeId(1) });
    let moved_result = body.resource();
    body.op(InstructionKind::ScopeExit {
        scope: ScopeId(1),
        kind: CleanupExitKind::Fallthrough,
    });
    // The scope marker owns storage, not SSA results. A block result may transfer
    // into outer storage; actual ownership must still be consumed exactly once.
    body.op(InstructionKind::Store {
        local: outer,
        value: moved_result,
    });
    body.op(InstructionKind::DropManaged { local: outer });
    body.op(InstructionKind::StorageDead(outer));
    let result = body.tag(true);
    let correct = body.close(Some(result), CleanupExitKind::Error);
    verify(correct.clone()).unwrap();
    let mut wrong = correct;
    wrong.blocks[0].instructions.retain(|i| {
        !matches!(
            i.kind,
            InstructionKind::Store { .. } | InstructionKind::DropManaged { .. }
        )
    });
    rejected(wrong, "normal return leaves an outstanding resource");
}

#[test]
fn transferred_return_result_cannot_be_aliased_and_cleaned_again() {
    let mut body = Body::new(owned());
    body.0.result_mode = PassingMode::Create;
    let storage = body.storage(owned());
    let result = body.resource();
    body.op(InstructionKind::Store {
        local: storage,
        value: result,
    });
    body.op(InstructionKind::DropManaged { local: storage });
    body.op(InstructionKind::StorageDead(storage));
    rejected(
        body.close(Some(result), CleanupExitKind::Return),
        "managed SSA value was not available exactly once",
    );
}

#[test]
fn error_scope_wire_input_is_rechecked_after_roundtrip_and_mutation() {
    let module = module(owned_error());
    let plan = resource_tests::plan(&module);
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    verify_module_bytes(&bytes, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let mut changed = module;
    changed.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropManaged { .. }));
    let plan = resource_tests::plan(&changed);
    let bytes = zil_ir::wire::encode_module_candidate(&changed, &plan, &Limits::default()).unwrap();
    let diagnostics =
        verify_module_bytes(&bytes, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("outstanding resource responsibility")),
        "{diagnostics:?}"
    );
}

#[test]
fn scoped_pure_ct_is_admitted_without_adding_error_unions_to_ct() {
    fn program(function: TypedFunction) -> CtProgram {
        let module = module(function);
        let plan = resource_tests::plan(&module);
        CtProgram {
            module,
            plan,
            entry: FunctionId(0),
            max_steps: 100,
            max_call_depth: 8,
            max_live_slots: 16,
        }
    }
    let mut body = Body::new(Type::I32);
    let value = body.value(Type::I32, InstructionKind::Constant(Constant::I32(42)));
    verify_ct_program(
        program(body.close(Some(value), CleanupExitKind::Return)),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap();
    let errors = verify_ct_program(
        program(tagged(true)),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-CT-FORBIDDEN"),
        "{errors:?}"
    );
}

#[test]
fn synchronous_error_return_cleans_owned_closure_environments_exactly_once() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let mut module = resource_closure_tests::fixture(kind, false, 1);
        let main = &mut module.functions[0];
        let Terminator::Return(Some(payload)) = main.blocks[0].terminator else {
            unreachable!()
        };
        main.return_type = union();
        let error = ValueId(main.values.len() as u32);
        main.values.push(union());
        main.blocks[0].instructions.push(ins(
            Some(error),
            InstructionKind::MakeErrorUnion {
                is_error: true,
                value: payload,
            },
        ));
        main.blocks[0].terminator = Terminator::Return(Some(error));
        for function in &mut module.functions {
            function.blocks[0].instructions.insert(0, enter(0));
            function.blocks[0].instructions.push(exit(
                0,
                if function.id == FunctionId(0) {
                    CleanupExitKind::Error
                } else {
                    CleanupExitKind::Return
                },
            ));
        }
        let plan = resource_tests::plan(&module);
        verify_module(
            module.clone(),
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default(),
        )
        .unwrap();
        if kind != CallableKind::FnOnce {
            let mut wrong = module;
            wrong.functions[0].blocks[0]
                .instructions
                .retain(|i| !matches!(i.kind, InstructionKind::DropClosure { .. }));
            let plan = resource_tests::plan(&wrong);
            let errors =
                verify_module(wrong, plan, &TargetProfile::host_llvm(), &Limits::default())
                    .unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.message.contains("outstanding resource responsibility")),
                "{errors:?}"
            );
        }
    }
}
