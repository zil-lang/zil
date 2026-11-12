//! Raw helper-origin checks, independent of Expand's source admission.
use super::*;
use zil_base::FileId;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}

fn fixture(
    scoped: bool,
    captured: u32,
    forwarded: bool,
) -> (TypedFunction, Vec<CoreCallableParameter>) {
    let address = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    let capture_type = if captured == 2 {
        address
    } else {
        Type::Closure(ClosureId(captured))
    };
    let instruction = |result, kind| Instruction {
        result: Some(ValueId(result)),
        kind,
        span: span(),
    };
    let mut instructions = vec![instruction(
        0,
        if captured == 2 {
            InstructionKind::BorrowLocal { local: LocalId(2) }
        } else {
            InstructionKind::Load(LocalId(captured))
        },
    )];
    if forwarded {
        instructions.push(instruction(
            1,
            InstructionKind::Call {
                function: FunctionId(9),
                arguments: vec![ValueId(0)],
            },
        ));
    }
    instructions.push(instruction(
        2,
        InstructionKind::MakeClosure {
            closure: ClosureId(2),
            captures: vec![ValueId(u32::from(forwarded))],
        },
    ));
    if scoped {
        instructions.push(instruction(
            3,
            InstructionKind::WithForeignCallback {
                register: FunctionId(7),
                close: FunctionId(8),
                callback: ValueId(0),
                body: ValueId(2),
                region: 1,
            },
        ));
    }
    let bounds = [CallableKind::FnMut, CallableKind::FnOnce]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| CoreCallableParameter {
            source_life: None,
            life: None,
            closed_environment: false,
            transfer: false,
            thread_callable: false,
            index: index as u32,
            generic_index: index as u32,
            generic_name: format!("Role{index}"),
            kind,
            parameters: vec![],
            result: Type::I32,
        })
        .collect();
    let function = TypedFunction {
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        reference_return_bound: None,
        id: FunctionId(0),
        name: "raw_capture_origins".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0), LocalId(1)],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: [
            Type::Closure(ClosureId(0)),
            Type::Closure(ClosureId(1)),
            Type::I32,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, ty)| LocalDecl {
            id: LocalId(index as u32),
            name: format!("local{index}"),
            ty,
            mutable: false,
            span: span(),
        })
        .collect(),
        values: vec![
            capture_type.clone(),
            capture_type,
            Type::Closure(ClosureId(2)),
            Type::Tuple(vec![Type::Bool, Type::I32]),
        ]
        .into(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions,
            terminator: Terminator::Return(None),
        }]
        .into(),
        entry: BlockId(0),
        callable_contract: None,
        protocol_contract: None,
        callable_return_contract: None,
        span: span(),
    };
    (function, bounds)
}

fn check(
    scoped: bool,
    captured: u32,
    forwarded: bool,
    work: usize,
) -> R<(Vec<Origin>, Vec<Origin>)> {
    let (function, bounds) = fixture(scoped, captured, forwarded);
    let limits = Limits {
        max_analysis_steps: work,
        ..Limits::default()
    };
    let mut state = closures::State::new(0, &limits);
    if forwarded {
        // A checked public Parameter return preserves the actual role origin.
        state.forward_results.insert(FunctionId(9), 0);
    }
    environment_origins(&function, &bounds, &mut state, span())
}

#[test]
fn with_and_ordinary_local_captures_cannot_hide_either_supplied_role() {
    for scoped in [true, false] {
        for captured in [0, 1] {
            for forwarded in [false, true] {
                let diagnostic = check(scoped, captured, forwarded, 10_000).unwrap_err();
                assert_eq!(diagnostic.category, zil_base::ResultCategory::Unsupported);
                assert_eq!(
                    diagnostic.message,
                    "local concrete callable cannot capture a supplied callable role"
                );
            }
        }
    }
}

#[test]
fn same_identity_fresh_and_supplied_values_cannot_mix_inside_a_capture() {
    for scoped in [true, false] {
        let (mut function, bounds) = fixture(scoped, 1, false);
        function.locals[2].ty = Type::Closure(ClosureId(1));
        function.values.push(Type::Closure(ClosureId(1)));
        let instruction = |result: Option<u32>, kind| Instruction {
            result: result.map(ValueId),
            kind,
            span: span(),
        };
        function.blocks[0].instructions.splice(
            1..1,
            [
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(0),
                    },
                ),
                instruction(
                    Some(1),
                    InstructionKind::MakeClosure {
                        closure: ClosureId(1),
                        captures: vec![],
                    },
                ),
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(1),
                    },
                ),
                instruction(Some(4), InstructionKind::Load(LocalId(2))),
            ],
        );
        let InstructionKind::MakeClosure { captures, .. } =
            &mut function.blocks[0].instructions[5].kind
        else {
            unreachable!()
        };
        *captures = vec![ValueId(4)];
        let mut state = closures::State::new(0, &Limits::default());
        let diagnostic = environment_origins(&function, &bounds, &mut state, span()).unwrap_err();
        assert_eq!(
            diagnostic.message,
            "local concrete callable cannot capture a supplied callable role"
        );
    }
}

#[test]
fn disjoint_borrowed_local_capture_keeps_exact_fresh_work_boundaries() {
    // Keep the old measured traversal controls. The seven actual origin slots
    // now pay their full physical record size, including padding, before alloc.
    // This is an explicit size migration, not a widened work allowance.
    for (scoped, forwarded, n) in [
        (true, false, 44),
        (true, true, 50),
        (false, false, 37),
        (false, true, 43),
    ] {
        let expected = check(scoped, 2, forwarded, 10_000).unwrap();
        let (function, _) = fixture(scoped, 2, forwarded);
        let physical_delta = (function.locals.len() + function.values.len())
            * (std::mem::size_of::<Origin>() - std::mem::size_of::<u8>());
        assert_eq!(
            check(scoped, 2, forwarded, n).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        let n = n + physical_delta;
        assert_eq!(
            expected.1[2],
            if scoped {
                Origin::BODY
            } else {
                Origin::CONCRETE
            }
        );
        assert_eq!(expected.1[0], Origin::EMPTY);
        assert_eq!(check(scoped, 2, forwarded, n).unwrap(), expected);
        assert_eq!(check(scoped, 2, forwarded, n).unwrap(), expected);
        assert_eq!(
            check(scoped, 2, forwarded, n - 1).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
}

#[test]
fn ordinary_aggregates_cannot_launder_a_supplied_role_into_a_local_body() {
    for shape in ["package", "union", "array", "tuple", "variant"] {
        for source in ["supplied", "fresh", "unknown"] {
            let (mut function, bounds) = fixture(true, 1, false);
            let callback = Type::Closure(ClosureId(1));
            let payload = Type::Record {
                name: "Token".into(),
                fields: vec![("n".into(), Type::I32)],
            };
            let managed = Type::Managed(Box::new(payload));
            let (ty, kind) = match shape {
                "package" => (
                    Type::Package {
                        name: "Hidden".into(),
                        fields: vec![
                            ("callback".into(), callback.clone()),
                            ("owner".into(), managed.clone()),
                        ],
                    },
                    InstructionKind::MakePackage {
                        fields: vec![ValueId(0), ValueId(4)],
                    },
                ),
                "union" => (
                    Type::ErrorUnion {
                        error: Box::new(Type::I32),
                        result: Box::new(callback.clone()),
                    },
                    InstructionKind::MakeErrorUnion {
                        is_error: false,
                        value: ValueId(0),
                    },
                ),
                "array" => (
                    Type::Array {
                        element: Box::new(callback.clone()),
                        length: 2,
                    },
                    InstructionKind::MakeOwnedArray {
                        elements: vec![ValueId(0), ValueId(0)],
                    },
                ),
                "tuple" => (
                    Type::Tuple(vec![callback.clone()]),
                    InstructionKind::MakeAggregate {
                        fields: vec![ValueId(0)],
                    },
                ),
                _ => (
                    Type::Enum {
                        name: "Hidden".into(),
                        variants: vec![("Callback".into(), vec![callback])],
                    },
                    InstructionKind::MakeVariant {
                        variant: 0,
                        fields: vec![ValueId(0)],
                    },
                ),
            };
            function.values[1] = ty;
            function.values.extend([managed, Type::I32]);
            let instruction = |result, kind| Instruction {
                result: Some(ValueId(result)),
                kind,
                span: span(),
            };
            function.blocks[0].instructions.splice(
                1..1,
                [
                    instruction(5, InstructionKind::Constant(Constant::I32(0))),
                    instruction(
                        4,
                        InstructionKind::MakeManaged {
                            fields: vec![ValueId(5)],
                        },
                    ),
                    instruction(1, kind),
                ],
            );
            if source == "fresh" {
                function.blocks[0].instructions[0].kind = InstructionKind::MakeClosure {
                    closure: ClosureId(1),
                    captures: vec![],
                };
            } else if source == "unknown" {
                function.blocks[0].instructions[0].kind = InstructionKind::Call {
                    function: FunctionId(77),
                    arguments: vec![],
                };
            }
            let InstructionKind::MakeClosure { captures, .. } =
                &mut function.blocks[0].instructions[4].kind
            else {
                unreachable!()
            };
            *captures = vec![ValueId(1)];
            let run = |work| {
                let limits = Limits {
                    max_analysis_steps: work,
                    ..Limits::default()
                };
                let mut state = closures::State::new(0, &limits);
                let result = environment_origins(&function, &bounds, &mut state, span());
                (result, state.accounting()[4])
            };
            let (result, work) = run(10_000);
            if source != "fresh" {
                let diagnostic = result.unwrap_err();
                assert_eq!(
                    diagnostic.message, "supplied callable role cannot enter an ordinary aggregate",
                    "{shape}"
                );
            } else {
                let expected = result.unwrap_or_else(|error| panic!("{shape}: {error:?}"));
                assert_eq!(run(work).0.unwrap(), expected);
                assert_eq!(run(work).0.unwrap(), expected);
                assert_eq!(
                    run(work - 1).0.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
}

#[test]
fn a_single_element_parameter_bridge_does_not_authorize_an_aggregate_call() {
    let (mut function, bounds) = fixture(true, 1, false);
    function.values[1] = Type::Array {
        element: Box::new(Type::Closure(ClosureId(1))),
        length: 1,
    };
    function.blocks[0].instructions.insert(
        1,
        Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::MakeOwnedArray {
                elements: vec![ValueId(0)],
            },
            span: span(),
        },
    );
    let InstructionKind::MakeClosure { captures, .. } =
        &mut function.blocks[0].instructions[2].kind
    else {
        unreachable!()
    };
    captures.clear();
    let mut state = closures::State::new(0, &Limits::default());
    let (_, origins) = environment_origins(&function, &bounds, &mut state, span()).unwrap();
    assert_eq!(origins[1], Origin::supplied(1, false));
    let call = Instruction {
        result: None,
        kind: InstructionKind::Call {
            function: FunctionId(9),
            arguments: vec![ValueId(1)],
        },
        span: span(),
    };
    let diagnostic = forwarded_contract(&function, &call, &origins, &bounds, &mut state)
        .err()
        .expect("aggregate Call must reject");
    assert_eq!(
        diagnostic.message,
        "nested helper cannot receive an aggregate containing a supplied callable role"
    );
}

#[test]
fn reading_a_role_address_preserves_its_actual_capture_origin() {
    for captured in [0, 1] {
        let (mut function, bounds) = fixture(true, captured, false);
        function.values[0] = Type::Address {
            mutable: false,
            inner: Box::new(Type::Closure(ClosureId(captured))),
        };
        function.blocks[0].instructions[0].kind = InstructionKind::BorrowLocal {
            local: LocalId(captured),
        };
        function.blocks[0].instructions.insert(
            1,
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::ReadAddress {
                    address: ValueId(0),
                },
                span: span(),
            },
        );
        let InstructionKind::MakeClosure { captures, .. } =
            &mut function.blocks[0].instructions[2].kind
        else {
            unreachable!()
        };
        *captures = vec![ValueId(1)];
        let mut state = closures::State::new(0, &Limits::default());
        let diagnostic = environment_origins(&function, &bounds, &mut state, span()).unwrap_err();
        assert_eq!(
            diagnostic.message,
            "local concrete callable cannot capture a supplied callable role"
        );
    }
}
