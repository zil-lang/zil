//! Raw TypedIR tests for Analyze's independent role reconstruction. These are
//! deliberately separate from source/Verify/native acceptance evidence.
use zil_analyze::analyze;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn instruction(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: span(),
    }
}
fn address() -> Type {
    Type::Address {
        mutable: false,
        inner: Box::new(Type::Closure(ClosureId(0))),
    }
}
fn event(parameter: u32) -> CallEvent {
    CallEvent {
        parameter,
        kind: CallableKind::Fn,
    }
}
fn role(parameter: u32) -> CallableRole {
    CallableRole {
        parameter,
        closure: ClosureId(0),
        kind: CallableKind::Fn,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: CallableCleanup::Borrowed,
    }
}
fn trace(parameters: &[u32]) -> CallableTrace {
    CallableTrace {
        calls: parameters.iter().map(|p| event(*p)).collect(),
        exit: CallableExit::Return,
    }
}
fn function(id: u32) -> TypedFunction {
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
        span: span(),
    }
}
fn helper(id: u32) -> TypedFunction {
    let mut f = function(id);
    f.parameters = vec![LocalId(0), LocalId(1), LocalId(2)];
    f.locals = [address(), Type::Bool, address()]
        .into_iter()
        .enumerate()
        .map(|(index, ty)| LocalDecl {
            id: LocalId(index as u32),
            name: format!("p{index}"),
            ty,
            mutable: false,
            span: span(),
        })
        .collect();
    f.callable_contract = Some(CallableParameterContract {
        roles: vec![role(0), role(2)],
        traces: CallableTraces::Finite(vec![trace(&[])]),
    });
    f
}
fn load(f: &mut TypedFunction, block: usize, local: u32) -> ValueId {
    let value = ValueId(f.values.len() as u32);
    f.values.push(f.locals[local as usize].ty.clone());
    f.blocks[block].instructions.push(instruction(
        Some(value),
        InstructionKind::Load(LocalId(local)),
    ));
    value
}
fn invoke(f: &mut TypedFunction, block: usize, local: u32) {
    let value = load(f, block, local);
    let result = ValueId(f.values.len() as u32);
    f.values.push(Type::Unit);
    f.blocks[block].instructions.push(instruction(
        Some(result),
        InstructionKind::CallClosureBorrowed {
            callee: value,
            kind: CallableKind::Fn,
            arguments: vec![],
        },
    ));
}
fn fixture(order: &[u32]) -> TypedModule {
    let mut body = function(0);
    body.environment = Some(ClosureId(0));
    let mut f = helper(1);
    for local in order {
        invoke(&mut f, 0, *local);
    }
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Finite(vec![trace(order)]);
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"Analyze role raw fixture"),
        target: TargetProfile::host_llvm(),
        functions: vec![body, f],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        }],
        resources: vec![],
        unsupported: vec![],
    }
}
fn accepted(module: &TypedModule) {
    analyze(module, &Limits::default()).unwrap();
}
fn rejected(module: &TypedModule, text: &str) {
    let errors = analyze(module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.code == "E-ANALYZE-IR"
            && d.message.contains(text)),
        "{errors:?}"
    );
}

#[test]
fn same_closure_roles_keep_actual_parameter_positions_and_order() {
    let good = fixture(&[0, 2, 0]);
    accepted(&good);
    let mut swapped = good.clone();
    swapped.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Finite(vec![trace(&[2, 0, 0])]);
    rejected(&swapped, "trace claim differs");
    let mut nonrole = good.clone();
    nonrole.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Finite(vec![trace(&[0, 1, 0])]);
    rejected(&nonrole, "trace claim differs");
    accepted(&good);
}

#[test]
fn missing_duplicate_reordered_and_false_cleanup_roles_are_rejected() {
    let good = fixture(&[0, 2]);
    accepted(&good);
    for roles in [
        vec![role(0)],
        vec![role(0), role(0)],
        vec![role(2), role(0)],
    ] {
        let mut bad = good.clone();
        bad.functions[1].callable_contract.as_mut().unwrap().roles = roles;
        rejected(&bad, "ordered parameters");
    }
    let mut cleanup = good.clone();
    cleanup.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[1]
        .cleanup = CallableCleanup::Owned;
    rejected(&cleanup, "cleanup mode differs");
}

fn branching() -> TypedModule {
    let mut module = fixture(&[]);
    let f = &mut module.functions[1];
    let condition = load(f, 0, 1);
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    for index in 1..=2 {
        f.blocks.push(BasicBlock {
            id: BlockId(index),
            instructions: vec![],
            terminator: Terminator::Return(None),
        });
    }
    invoke(f, 1, 0);
    invoke(f, 2, 2);
    f.callable_contract.as_mut().unwrap().traces =
        CallableTraces::Finite(vec![trace(&[0]), trace(&[2])]);
    module
}

#[test]
fn branches_preserve_joint_correlation_instead_of_role_cross_product() {
    let good = branching();
    accepted(&good);
    let mut crossed = good.clone();
    crossed.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Finite(vec![trace(&[]), trace(&[0, 2])]);
    rejected(&crossed, "trace claim differs");
}

fn nested() -> TypedModule {
    let mut module = fixture(&[0, 2]);
    let mut parent = helper(2);
    let left = load(&mut parent, 0, 0);
    let flag = load(&mut parent, 0, 1);
    let right = load(&mut parent, 0, 2);
    for arguments in [vec![left, flag, right], vec![right, flag, left]] {
        let result = ValueId(parent.values.len() as u32);
        parent.values.push(Type::Unit);
        parent.blocks[0].instructions.push(instruction(
            Some(result),
            InstructionKind::Call {
                function: FunctionId(1),
                arguments,
            },
        ));
    }
    parent.callable_contract.as_mut().unwrap().traces =
        CallableTraces::Finite(vec![trace(&[0, 2, 2, 0])]);
    module.functions.push(parent);
    module
}

#[test]
fn each_static_child_call_uses_its_own_actual_role_substitution() {
    let good = nested();
    accepted(&good);
    let mut shared_mapping = good.clone();
    shared_mapping.functions[2]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Finite(vec![trace(&[0, 2, 0, 2])]);
    rejected(&shared_mapping, "trace claim differs");
    let mut coordinated = good.clone();
    coordinated.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Finite(vec![trace(&[2, 0])]);
    coordinated.functions[2]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Finite(vec![trace(&[2, 0, 0, 2])]);
    rejected(&coordinated, "trace claim differs");
}

fn looping() -> TypedModule {
    let mut module = fixture(&[]);
    // Entry calls role 0; the backedge calls role 2; the other branch returns.
    let mut clean = helper(1);
    clean.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(0)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![],
            terminator: Terminator::Return(None),
        },
    ];
    let condition = load(&mut clean, 0, 1);
    invoke(&mut clean, 0, 0);
    invoke(&mut clean, 1, 2);
    clean.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    clean.callable_contract.as_mut().unwrap().traces = CallableTraces::Flow(CallableFlow {
        entry: 0,
        nodes: vec![
            CallableFlowNode {
                calls: vec![event(0)],
                next: CallableFlowNext::Edges(vec![1, 2]),
            },
            CallableFlowNode {
                calls: vec![event(2)],
                next: CallableFlowNext::Edges(vec![0]),
            },
            CallableFlowNode {
                calls: vec![],
                next: CallableFlowNext::Exit(CallableExit::Return),
            },
        ],
    });
    module.functions[1] = clean;
    module
}

#[test]
fn mixed_same_type_origins_do_not_guess_one_role() {
    let mut module = fixture(&[]);
    let f = &mut module.functions[1];
    f.locals.push(LocalDecl {
        id: LocalId(3),
        name: "joined".into(),
        ty: address(),
        mutable: true,
        span: span(),
    });
    let condition = load(f, 0, 1);
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    for index in 1..=3 {
        f.blocks.push(BasicBlock {
            id: BlockId(index),
            instructions: vec![],
            terminator: if index == 3 {
                Terminator::Return(None)
            } else {
                Terminator::Jump(BlockId(3))
            },
        });
    }
    for (block, local) in [(1, 0), (2, 2)] {
        let value = load(f, block, local);
        f.blocks[block].instructions.push(instruction(
            None,
            InstructionKind::Store {
                local: LocalId(3),
                value,
            },
        ));
    }
    invoke(f, 3, 3);
    f.callable_contract.as_mut().unwrap().traces =
        CallableTraces::Finite(vec![trace(&[0]), trace(&[2])]);
    let errors = analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::Unsupported
            && d.message.contains("unknown, mixed")),
        "{errors:?}"
    );
}

#[test]
fn an_unused_once_role_still_keeps_a_cyclic_helper_closed() {
    let mut module = fixture(&[]);
    let f = &mut module.functions[1];
    f.locals[0].ty = Type::Closure(ClosureId(0));
    f.callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::FnOnce;
    f.callable_contract.as_mut().unwrap().roles[0].cleanup = CallableCleanup::Owned;
    let condition = load(f, 0, 1);
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![],
        terminator: Terminator::Jump(BlockId(0)),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![],
        terminator: Terminator::Return(None),
    });
    invoke(f, 1, 2);
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Flow(CallableFlow {
        entry: 0,
        nodes: vec![
            CallableFlowNode {
                calls: vec![],
                next: CallableFlowNext::Edges(vec![1, 2]),
            },
            CallableFlowNode {
                calls: vec![event(2)],
                next: CallableFlowNext::Edges(vec![0]),
            },
            CallableFlowNode {
                calls: vec![],
                next: CallableFlowNext::Exit(CallableExit::Return),
            },
        ],
    });
    let errors = analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::Unsupported
            && d.message.contains("cyclic FnOnce")),
        "{errors:?}"
    );
}

#[test]
fn flow_checks_role_events_and_backedges_not_only_call_counts() {
    let good = looping();
    accepted(&good);
    let mut swapped = good.clone();
    if let CallableTraces::Flow(flow) = &mut swapped.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
    {
        flow.nodes[0].calls[0].parameter = 2;
    }
    rejected(&swapped, "omits or changes calls");
    let mut edges = good.clone();
    if let CallableTraces::Flow(flow) = &mut edges.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
    {
        flow.nodes[1].next = CallableFlowNext::Edges(vec![2]);
    }
    rejected(&edges, "ordered edges");
}

#[test]
fn second_role_envelope_is_checked_even_when_only_that_role_is_invoked() {
    let mut good = fixture(&[2]);
    good.closures[0].captures = vec![Type::Desc(Box::new(Type::Record {
        name: "R".into(),
        fields: vec![("n".into(), Type::I32)],
    }))];
    for role in &mut good.functions[1].callable_contract.as_mut().unwrap().roles {
        role.call = vec![0];
        role.writes = vec![0];
    }
    accepted(&good);
    let mut erased = good.clone();
    erased.functions[1]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[1]
        .writes
        .clear();
    rejected(&erased, "write envelope differs");
}

#[test]
fn ordered_child_and_branch_sequences_have_exact_budget_failure_and_fresh_retry() {
    for module in [nested(), branching(), looping()] {
        accepted(&module);
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            };
            match analyze(&module, &limits) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert!(
                        errors.iter().any(|d| d.phase == Phase::Analyze
                            && d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle + 1;
                }
            }
        }
        assert!(low > 0);
        let at = Limits {
            max_analysis_steps: low,
            ..Limits::default()
        };
        let below = Limits {
            max_analysis_steps: low - 1,
            ..Limits::default()
        };
        assert!(analyze(&module, &at).is_ok());
        assert!(
            analyze(&module, &below)
                .unwrap_err()
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );
        assert!(analyze(&module, &at).is_ok());
        println!("Analyze196 exact frontier: {low}, N-1 rejected, fresh N accepted");
    }
}
