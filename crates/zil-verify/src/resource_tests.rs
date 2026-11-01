//! Hostile public-IR resource candidates. These are verifier boundary tests,
//! not claims of source, native execution, or full resource-language coverage.
use super::*;
use zil_base::{FileId, ModuleId};

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn payload() -> Type {
    Type::Record {
        name: "Token".into(),
        fields: vec![("code".into(), Type::I32)],
    }
}
fn owned() -> Type {
    Type::Managed(Box::new(payload()))
}
fn desc() -> Type {
    Type::Desc(Box::new(payload()))
}
fn i(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
fn local(id: u32, ty: Type) -> LocalDecl {
    LocalDecl {
        id: LocalId(id),
        name: format!("l{id}"),
        ty,
        mutable: true,
        span: span(),
    }
}
fn function() -> TypedFunction {
    TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(0),
        name: "main".into(),
        foreign: None,
        environment: None,
        parameters: vec![],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: vec![local(0, owned())],
        values: vec![Type::I32, owned(), desc(), Type::I32],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                i(None, InstructionKind::StorageLive(LocalId(0))),
                i(Some(0), InstructionKind::Constant(Constant::I32(65))),
                i(
                    Some(1),
                    InstructionKind::MakeManaged {
                        fields: vec![ValueId(0)],
                    },
                ),
                i(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(1),
                    },
                ),
                i(
                    Some(2),
                    InstructionKind::DescribeLocal { local: LocalId(0) },
                ),
                i(
                    Some(3),
                    InstructionKind::ReadDesc {
                        desc: ValueId(2),
                        index: 0,
                    },
                ),
                i(None, InstructionKind::DropManaged { local: LocalId(0) }),
                i(None, InstructionKind::StorageDead(LocalId(0))),
            ],
            terminator: Terminator::Return(Some(ValueId(3))),
        }],
        entry: BlockId(0),
        span: span(),
    }
}
fn module() -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(
            b"synthetic independent resource verification fixtures",
        ),
        target: TargetProfile::host_llvm(),
        functions: vec![function()],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "Token".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}

// Initialization and empty Address Hold claims are fixture plumbing. Tests below
// mutate resource operations; the resource state checker is never called here.
pub(super) fn plan(module: &TypedModule) -> CandidatePlan {
    let mut plan = CandidatePlan::empty_for(module);
    let mut budget = Budget::new(&Limits::default());
    for f in &module.functions {
        if f.foreign.is_some() {
            plan.functions.push(FunctionPlan {
                function: f.id,
                blocks: vec![],
                holds: vec![],
            });
            continue;
        }
        let n = f.blocks.len();
        let mut successors = vec![vec![]; n];
        let mut predecessors = vec![vec![]; n];
        for (b, block) in f.blocks.iter().enumerate() {
            match block.terminator {
                Terminator::Jump(to) => successors[b].push(to.0 as usize),
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    successors[b].push(then_block.0 as usize);
                    if then_block != else_block {
                        successors[b].push(else_block.0 as usize);
                    }
                }
                _ => {}
            }
            for &s in &successors[b] {
                predecessors[s].push(b);
            }
        }
        let reachable = reachability(f.entry.0 as usize, &successors, &mut budget).unwrap();
        let mut params = vec![false; f.locals.len()];
        for p in &f.parameters {
            params[p.0 as usize] = true;
        }
        let (ins, outs) =
            initialization(f, &predecessors, &reachable, &params, &mut budget).unwrap();
        let ids = |row: &[bool]| {
            row.iter()
                .enumerate()
                .filter_map(|(i, b)| b.then_some(LocalId(i as u32)))
                .collect()
        };
        plan.functions.push(FunctionPlan {
            function: f.id,
            blocks: (0..n)
                .map(|b| BlockPlan {
                    block: BlockId(b as u32),
                    initialized_in: ids(&ins[b]),
                    initialized_out: ids(&outs[b]),
                })
                .collect(),
            holds: (0..f.values.len())
                .map(|v| ValueHold {
                    value: ValueId(v as u32),
                    roots: vec![],
                })
                .collect(),
        });
    }
    plan
}
fn verify(module: TypedModule) -> CompileResult<VerifiedModule> {
    let p = plan(&module);
    verify_module(module, p, &TargetProfile::host_llvm(), &Limits::default())
}
fn rejected(module: TypedModule, needle: &str) {
    let error = verify(module).unwrap_err();
    assert!(
        format!("{error:?}").contains(needle),
        "expected {needle}: {error:?}"
    );
}

#[test]
fn finite_resource_description_reads_and_cleans_once() {
    verify(module()).unwrap();
}

#[test]
fn missing_cleanup_before_storage_dead_is_rejected() {
    let mut m = module();
    m.functions[0].blocks[0].instructions.remove(6);
    rejected(m, "outstanding resource responsibility");
}

#[test]
fn normal_return_checks_local_and_unstored_ssa_responsibilities() {
    let mut m = module();
    m.functions[0].blocks[0].instructions.truncate(6);
    rejected(m, "normal return leaves");
    let mut m = module();
    let f = &mut m.functions[0];
    f.locals.clear();
    f.values.truncate(2);
    f.return_type = Type::Unit;
    f.blocks[0].instructions = vec![
        i(Some(0), InstructionKind::Constant(Constant::I32(65))),
        i(
            Some(1),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
    ];
    f.blocks[0].terminator = Terminator::Return(None);
    rejected(m, "normal return leaves");
}

#[test]
fn duplicate_cleanup_and_duplicate_ssa_transfer_are_rejected() {
    let mut m = module();
    m.functions[0].blocks[0].instructions.insert(
        7,
        i(None, InstructionKind::DropManaged { local: LocalId(0) }),
    );
    rejected(m, "duplicate automatic cleanup");
    let mut m = module();
    m.functions[0].locals.push(local(1, owned()));
    m.functions[0].blocks[0].instructions.insert(
        4,
        i(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
        ),
    );
    rejected(m, "managed SSA value was not available exactly once");
}

#[test]
fn managed_load_and_incomplete_constructor_are_rejected() {
    let mut m = module();
    m.functions[0].blocks[0].instructions[4].kind = InstructionKind::Load(LocalId(0));
    rejected(m, "never copying Load");
    let mut m = module();
    m.functions[0].blocks[0].instructions[2].kind = InstructionKind::MakeManaged { fields: vec![] };
    rejected(m, "every payload field exactly once");
}

#[test]
fn descriptions_cannot_survive_cleanup_or_give() {
    let mut m = module();
    m.functions[0].blocks[0].instructions.swap(5, 6);
    rejected(m, "invalidate a live description");
    let mut m = module();
    m.functions[0].values.push(owned());
    m.functions[0].blocks[0].instructions.insert(
        5,
        i(Some(4), InstructionKind::TakeLocal { local: LocalId(0) }),
    );
    rejected(m, "invalidate a live description");
}

#[test]
fn create_cannot_launder_incoming_responsibility_but_give_can_transfer_it() {
    let mut m = module();
    let f = &mut m.functions[0];
    f.parameters = vec![LocalId(0)];
    f.return_type = owned();
    f.result_mode = PassingMode::Create;
    f.values = vec![owned()];
    f.blocks[0].instructions = vec![i(Some(0), InstructionKind::TakeLocal { local: LocalId(0) })];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    rejected(m.clone(), "create result is not independently proven fresh");
    m.functions[0].result_mode = PassingMode::Give;
    verify(m).unwrap();
}

#[test]
fn fresh_constructor_can_be_returned_and_wire_stays_untrusted() {
    let mut m = module();
    let f = &mut m.functions[0];
    f.locals.clear();
    f.values.truncate(2);
    f.return_type = owned();
    f.result_mode = PassingMode::Create;
    f.blocks[0].instructions = vec![
        i(Some(0), InstructionKind::Constant(Constant::I32(65))),
        i(
            Some(1),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    verify(m.clone()).unwrap();
    let p = plan(&m);
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
    m.functions[0].result_mode = PassingMode::Infer;
    let bytes = zil_ir::wire::encode_module_candidate(&m, &plan(&m), &Limits::default()).unwrap();
    assert!(verify_module_bytes(&bytes, &m.target, &Limits::default()).is_err());
}

fn with_teardown() -> TypedModule {
    let mut m = module();
    m.resources[0].teardown = Some(FunctionId(1));
    m.functions.push(TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(1),
        name: "$dropToken".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0)],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![local(0, desc())],
        values: vec![desc(), Type::I32],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                i(Some(0), InstructionKind::Load(LocalId(0))),
                i(
                    Some(1),
                    InstructionKind::ReadDesc {
                        desc: ValueId(0),
                        index: 0,
                    },
                ),
            ],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: span(),
    });
    m
}

#[test]
fn teardown_binding_is_checked_and_ordinary_entries_are_rejected() {
    verify(with_teardown()).unwrap();
    let mut m = with_teardown();
    m.functions[0].blocks[0].instructions.insert(
        5,
        i(
            None,
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(2)],
            },
        ),
    );
    rejected(m, "only by a checked cleanup activation");
    let mut m = with_teardown();
    m.resources[0].teardown = Some(FunctionId(0));
    rejected(m, "teardown requires");
}

#[test]
fn unregistered_resource_and_payload_mismatch_are_rejected() {
    let mut m = module();
    m.resources.clear();
    rejected(m, "absent from its nominal definition");
    let mut m = module();
    m.resources[0].payload = Type::Record {
        name: "Token".into(),
        fields: vec![("code".into(), Type::I64)],
    };
    rejected(m, "absent from its nominal definition");
}

#[test]
fn cleanup_order_comes_from_store_order_not_local_number() {
    let mut m = module();
    let f = &mut m.functions[0];
    f.locals.push(local(1, owned()));
    f.values.push(owned());
    f.blocks[0].instructions.insert(
        4,
        i(
            Some(4),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
    );
    f.blocks[0].instructions.insert(
        5,
        i(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(4),
            },
        ),
    );
    f.blocks[0].instructions.insert(
        8,
        i(None, InstructionKind::DropManaged { local: LocalId(1) }),
    );
    verify(m.clone()).unwrap();
    m.functions[0].blocks[0].instructions.swap(8, 9);
    rejected(m, "reverse successful initialization order");
}

#[test]
fn process_abort_is_not_a_normal_cleanup_exit() {
    let mut m = module();
    m.functions[0].blocks[0].instructions.truncate(6);
    m.functions[0].blocks[0].terminator = Terminator::Trap;
    verify(m).unwrap();
}

fn consume_function(id: u32) -> TypedFunction {
    TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(id),
        name: format!("consume{id}"),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0)],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![local(0, owned())],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![i(None, InstructionKind::DropManaged { local: LocalId(0) })],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: span(),
    }
}

fn conditional_give() -> TypedModule {
    let mut m = module();
    m.functions.push(consume_function(1));
    let f = &mut m.functions[0];
    f.locals.push(local(1, Type::Bool));
    f.parameters.push(LocalId(1));
    f.values.extend([Type::Bool, owned()]);
    f.blocks[0].instructions.truncate(6);
    f.blocks[0]
        .instructions
        .push(i(Some(4), InstructionKind::Load(LocalId(1))));
    f.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(4),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: vec![
                i(Some(5), InstructionKind::TakeLocal { local: LocalId(0) }),
                i(
                    None,
                    InstructionKind::Call {
                        function: FunctionId(1),
                        arguments: vec![ValueId(5)],
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![
                i(None, InstructionKind::DropManaged { local: LocalId(0) }),
                i(None, InstructionKind::StorageDead(LocalId(0))),
            ],
            terminator: Terminator::Return(Some(ValueId(3))),
        },
    ]);
    m
}

#[test]
fn conditional_give_keeps_a_real_cleanup_obligation_on_the_other_edge() {
    verify(conditional_give()).unwrap();
    let mut m = conditional_give();
    m.functions[0].blocks[3].instructions.remove(0);
    rejected(m, "outstanding resource responsibility");
    let mut m = conditional_give();
    m.functions[0].blocks[1].instructions.remove(1);
    rejected(m, "normal return leaves");
}

#[test]
fn giving_an_owner_and_its_description_in_one_call_is_rejected() {
    let mut m = module();
    let mut callee = consume_function(1);
    callee.locals.push(local(1, desc()));
    callee.parameters.push(LocalId(1));
    callee.values = vec![desc(), Type::I32];
    callee.blocks[0]
        .instructions
        .insert(0, i(Some(0), InstructionKind::Load(LocalId(1))));
    callee.blocks[0].instructions.insert(
        1,
        i(
            Some(1),
            InstructionKind::ReadDesc {
                desc: ValueId(0),
                index: 0,
            },
        ),
    );
    m.functions.push(callee);
    let f = &mut m.functions[0];
    f.values.push(owned());
    f.blocks[0].instructions.insert(
        6,
        i(Some(4), InstructionKind::TakeLocal { local: LocalId(0) }),
    );
    f.blocks[0].instructions.insert(
        7,
        i(
            None,
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(4), ValueId(2)],
            },
        ),
    );
    rejected(m, "invalidate a live description");
}

#[test]
fn a_description_cannot_revive_when_its_old_head_slot_is_reinitialized() {
    let mut m = module();
    let f = &mut m.functions[0];
    f.values.push(owned());
    f.blocks[0].instructions = vec![
        i(None, InstructionKind::StorageLive(LocalId(0))),
        i(Some(0), InstructionKind::Constant(Constant::I32(65))),
        i(
            Some(1),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
        i(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(1),
            },
        ),
        i(
            Some(2),
            InstructionKind::DescribeLocal { local: LocalId(0) },
        ),
        i(None, InstructionKind::DropManaged { local: LocalId(0) }),
        i(None, InstructionKind::StorageDead(LocalId(0))),
        i(None, InstructionKind::StorageLive(LocalId(0))),
        i(
            Some(4),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(0)],
            },
        ),
        i(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(4),
            },
        ),
        i(
            Some(3),
            InstructionKind::ReadDesc {
                desc: ValueId(2),
                index: 0,
            },
        ),
        i(None, InstructionKind::DropManaged { local: LocalId(0) }),
        i(None, InstructionKind::StorageDead(LocalId(0))),
    ];
    rejected(m, "invalidate a live description");
}

fn resource_loop() -> TypedModule {
    let mut m = module();
    let f = &mut m.functions[0];
    f.locals.push(local(1, Type::Bool));
    f.parameters.push(LocalId(1));
    f.values.push(Type::Bool);
    let mut body = f.blocks.remove(0);
    body.id = BlockId(1);
    body.terminator = Terminator::Jump(BlockId(0));
    f.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![i(Some(4), InstructionKind::Load(LocalId(1)))],
            terminator: Terminator::Branch {
                condition: ValueId(4),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        body,
        BasicBlock {
            id: BlockId(2),
            instructions: vec![],
            terminator: Terminator::Return(None),
        },
    ];
    f.return_type = Type::Unit;
    m
}

#[test]
fn each_loop_activation_must_discharge_before_reusing_its_static_slots() {
    verify(resource_loop()).unwrap();
    let mut m = resource_loop();
    m.functions[0].blocks[1].instructions.remove(6);
    rejected(m, "outstanding resource responsibility");
}

#[test]
fn backedge_to_entry_does_not_recreate_owned_parameters() {
    let mut m = module();
    m.functions.push(consume_function(1));
    let f = &mut m.functions[0];
    f.parameters = vec![LocalId(0)];
    f.values = vec![owned()];
    f.return_type = Type::Unit;
    f.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            i(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
            i(
                None,
                InstructionKind::Call {
                    function: FunctionId(1),
                    arguments: vec![ValueId(0)],
                },
            ),
        ],
        terminator: Terminator::Jump(BlockId(0)),
    }];
    rejected(m, "give requires a live owned position");
}

#[test]
fn two_description_parameters_may_alias_for_sequential_short_operations() {
    let mut m = module();
    let f = &mut m.functions[0];
    f.parameters = vec![LocalId(0), LocalId(1)];
    f.locals = vec![local(0, desc()), local(1, desc())];
    f.values = vec![desc(), desc(), Type::I32, Type::I32];
    f.blocks[0].instructions = vec![
        i(Some(0), InstructionKind::Load(LocalId(0))),
        i(Some(1), InstructionKind::Load(LocalId(1))),
        i(
            Some(2),
            InstructionKind::ReadDesc {
                desc: ValueId(0),
                index: 0,
            },
        ),
        i(
            None,
            InstructionKind::WriteDesc {
                desc: ValueId(1),
                index: 0,
                value: ValueId(2),
            },
        ),
        i(
            Some(3),
            InstructionKind::ReadDesc {
                desc: ValueId(0),
                index: 0,
            },
        ),
    ];
    verify(m).unwrap();
}

#[test]
fn resource_types_and_work_are_bounded_before_untrusted_hashing() {
    let mut m = module();
    let mut deep = payload();
    for _ in 0..16 {
        deep = Type::Managed(Box::new(deep));
    }
    m.functions[0].locals[0].ty = deep;
    let p = plan(&m);
    let limits = Limits {
        max_call_depth: 4,
        ..Limits::default()
    };
    let errors = verify_module(m.clone(), p, &m.target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    let m = conditional_give();
    let p = plan(&m);
    let limits = Limits {
        max_analysis_steps: 64,
        ..Limits::default()
    };
    let errors = verify_module(m.clone(), p, &m.target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
}

fn many_definitions() -> TypedModule {
    let mut m = module();
    for index in 0..128 {
        let name = format!("Unused{index}");
        m.resources.push(ResourceDefinition {
            payload: Type::Record {
                name: name.clone(),
                fields: vec![],
            },
            name,
            teardown: None,
        });
    }
    m
}

#[test]
fn resource_registry_growth_charges_each_type_and_constructor_use() {
    let small = module();
    let large = many_definitions();
    for check in 0..3 {
        for (m, expected_limit) in [(&small, false), (&large, true)] {
            let mut budget = Budget {
                remaining: 256,
                ..Budget::new(&Limits::default())
            };
            let result = match check {
                0 => check_function_signature_type(m, &owned(), Some(span()), &mut budget),
                1 => check_local_type(&owned(), Some(span()), m, &mut budget),
                _ => check_instruction(
                    m,
                    &m.functions[0],
                    &m.functions[0].blocks[0].instructions[2],
                    &mut budget,
                ),
            };
            if expected_limit {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit,
                );
            } else {
                result.unwrap();
            }
        }
    }
}

#[test]
fn ordinary_call_and_function_pointer_pay_teardown_table_search() {
    let small = module();
    let large = many_definitions();
    for pointer in [false, true] {
        for (m, expected_limit) in [(&small, false), (&large, true)] {
            let mut function = m.functions[0].clone();
            let kind = if pointer {
                function.values[0] = Type::FunctionPointer {
                    parameters: vec![],
                    result: Box::new(Type::I32),
                };
                InstructionKind::FunctionRef {
                    function: FunctionId(0),
                }
            } else {
                InstructionKind::Call {
                    function: FunctionId(0),
                    arguments: vec![],
                }
            };
            let mut budget = Budget {
                remaining: 64,
                ..Budget::new(&Limits::default())
            };
            let result = check_instruction(m, &function, &i(Some(0), kind), &mut budget);
            if expected_limit {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit,
                );
            } else {
                result.unwrap();
            }
        }
    }
}
