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
            index: index as u32,
            generic_index: index as u32,
            generic_name: format!("Role{index}"),
            kind,
            parameters: vec![],
            result: Type::I32,
        })
        .collect();
    let function = TypedFunction {
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
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions,
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        callable_contract: None,
        callable_return_contract: None,
        span: span(),
    };
    (function, bounds)
}

fn check(scoped: bool, captured: u32, forwarded: bool, work: usize) -> R<(Vec<u8>, Vec<u8>)> {
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
    // With-local previously cost 41/47: one prepaid queue entry and its
    // two-unit capture audit add three. Ordinary-local stays at 37/43.
    for (scoped, forwarded, n) in [
        (true, false, 44),
        (true, true, 50),
        (false, false, 37),
        (false, true, 43),
    ] {
        let expected = check(scoped, 2, forwarded, 10_000).unwrap();
        assert_eq!(expected.1[2], if scoped { 16 } else { 32 });
        assert_eq!(expected.1[0], 0);
        assert_eq!(check(scoped, 2, forwarded, n).unwrap(), expected);
        assert_eq!(check(scoped, 2, forwarded, n).unwrap(), expected);
        assert_eq!(
            check(scoped, 2, forwarded, n - 1).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
}
