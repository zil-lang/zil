//! Differential checks against the exact former clone/rebase implementation.
use super::*;
use zil_base::FileId;
use zil_ir::*;

fn function() -> TypedFunction {
    let span = |start, end| Span::new(FileId(3), start, end);
    TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(2),
        name: "f".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        result_mode: PassingMode::Copy,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "x".into(),
            ty: Type::I32,
            mutable: false,
            span: span(110, 115),
        }]
        .into(),
        values: vec![Type::I32, Type::I32].into(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span: span(120, 124),
                },
                Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::Constant(Constant::I32(7)),
                    span: span(130, 134),
                },
            ],
            terminator: Terminator::Return(Some(ValueId(0))),
        }]
        .into(),
        entry: BlockId(0),
        span: span(100, 180),
    }
}

fn oracle(left: &TypedFunction, right: &TypedFunction) -> Option<bool> {
    let mut left = left.clone();
    let mut right = right.clone();
    let a = Span::new(FileId(0), 0, left.span.end.checked_sub(left.span.start)?);
    let b = Span::new(FileId(0), 0, right.span.end.checked_sub(right.span.start)?);
    let left_origin = left.span;
    let right_origin = right.span;
    if !crate::incremental::rebase(&mut left, left_origin, a)
        || !crate::incremental::rebase(&mut right, right_origin, b)
    {
        return None;
    }
    Some(left == right)
}

fn compare(left: &TypedFunction, right: &TypedFunction, expected: Option<bool>, label: &str) {
    assert_eq!(oracle(left, right), expected, "old oracle: {label}");
    assert_eq!(
        relative_equal(left, right),
        expected,
        "borrowed comparison: {label}"
    );
    assert_eq!(
        relative_equal(right, left),
        expected,
        "reverse comparison: {label}"
    );
}

#[test]
fn every_raw_function_and_cfg_field_remains_an_equality_dependency() {
    type Edit = (&'static str, fn(&mut TypedFunction));
    let edits: &[Edit] = &[
        ("storage_life", |f| {
            f.storage_life = Some(StorageLifeContract {
                fixed_binder_count: 1,
                invocation_binder_count: 0,
                bindings: vec![],
                classes: vec![],
            })
        }),
        ("call_kind", |f| f.call_kind = CallKind::Suspend),
        ("reference_return_bound", |f| {
            f.reference_return_bound = Some(vec![0])
        }),
        ("callable_return_contract", |f| {
            f.callable_return_contract = Some(CallableReturnContract {
                source: CallableReturnSource::Fresh,
                closure: ClosureId(0),
                kind: CallableKind::Fn,
                hold: vec![],
                call: vec![],
                writes: vec![],
                cleanup: vec![],
                lending_captures: vec![],
            })
        }),
        ("callable_contract", |f| {
            f.callable_contract = Some(CallableParameterContract {
                roles: vec![],
                traces: CallableTraces::Finite(vec![]),
            })
        }),
        ("protocol_contract", |f| {
            f.protocol_contract = Some(FunctionProtocolContract {
                protocol: ProtocolKind::AsyncRegistrationV1,
                parameter: 0,
                entry: vec![],
                exits: vec![],
            })
        }),
        ("id", |f| f.id = FunctionId(3)),
        ("name", |f| f.name.push('2')),
        ("foreign", |f| {
            f.foreign = Some(ForeignDeclaration {
                abi: "C".into(),
                symbol: "other".into(),
                noescape_parameters: vec![],
                static_callbacks: false,
                scoped_callback: None,
            })
        }),
        ("environment", |f| f.environment = Some(ClosureId(1))),
        ("parameters", |f| f.parameters.clear()),
        ("return_type", |f| f.return_type = Type::I64),
        ("result_mode", |f| f.result_mode = PassingMode::Move),
        ("locals length", |f| {
            f.locals.push(f.locals[0].clone());
        }),
        ("local id", |f| f.locals[0].id = LocalId(1)),
        ("local name", |f| f.locals[0].name.push('2')),
        ("local type", |f| f.locals[0].ty = Type::I64),
        ("local mutable", |f| f.locals[0].mutable = true),
        ("local relative span", |f| f.locals[0].span.end += 1),
        ("values", |f| f.values[0] = Type::I64),
        ("blocks length", |f| {
            f.blocks.push(f.blocks[0].clone());
        }),
        ("block id", |f| f.blocks[0].id = BlockId(1)),
        ("terminator", |f| f.blocks[0].terminator = Terminator::Trap),
        ("instructions length", |f| {
            f.blocks[0].instructions.pop();
        }),
        ("instruction result", |f| {
            f.blocks[0].instructions[0].result = None
        }),
        ("instruction kind", |f| {
            f.blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::I32(9))
        }),
        ("instruction relative span", |f| {
            f.blocks[0].instructions[0].span.start += 1
        }),
        ("entry", |f| f.entry = BlockId(1)),
        ("function length", |f| f.span.end += 1),
    ];
    let original = function();
    compare(&original, &original, Some(true), "identical owner");
    for &(label, edit) in edits {
        let mut changed = original.clone();
        edit(&mut changed);
        compare(&original, &changed, Some(false), label);
    }
}

#[test]
fn absolute_file_and_origin_changes_preserve_only_equal_relative_positions() {
    let original = function();
    for (file, start) in [(FileId(0), 0), (FileId(9), 7), (FileId(3), usize::MAX - 80)] {
        let mut shifted = original.clone();
        assert!(crate::incremental::rebase(
            &mut shifted,
            original.span,
            Span::new(file, start, start + 80)
        ));
        compare(
            &original,
            &shifted,
            Some(true),
            "whole function translation",
        );
        shifted.blocks[0].instructions[1].span.end += 1;
        compare(
            &original,
            &shifted,
            Some(false),
            "relative instruction difference",
        );
    }
}

#[test]
fn unequal_headers_and_lengths_cannot_hide_invalid_late_spans() {
    type Edit = (&'static str, fn(&mut TypedFunction));
    let edits: &[Edit] = &[
        ("function reversed", |f| f.span.end = f.span.start - 1),
        ("local wrong file", |f| f.locals[0].span.file = FileId(9)),
        ("local before origin", |f| {
            f.locals[0].span.start = f.span.start - 1
        }),
        ("local beyond end", |f| {
            f.locals[0].span.end = f.span.end + 1
        }),
        ("local reversed", |f| {
            f.locals[0].span.end = f.locals[0].span.start - 1
        }),
        ("instruction wrong file", |f| {
            f.blocks[0].instructions[1].span.file = FileId(9)
        }),
        ("instruction before origin", |f| {
            f.blocks[0].instructions[1].span.start = f.span.start - 1
        }),
        ("instruction beyond end", |f| {
            f.blocks[0].instructions[1].span.end = usize::MAX
        }),
        ("instruction reversed", |f| {
            let i = &mut f.blocks[0].instructions[1];
            i.span.end = i.span.start - 1;
        }),
        ("unpaired malformed local", |f| {
            f.locals.push(LocalDecl {
                id: LocalId(2),
                name: "late".into(),
                ty: Type::I32,
                mutable: false,
                span: Span::new(FileId(9), 0, 1),
            })
        }),
        ("unpaired malformed block", |f| {
            f.blocks.push(BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction {
                    result: None,
                    kind: InstructionKind::Constant(Constant::I32(0)),
                    span: Span::new(FileId(9), 0, 1),
                }],
                terminator: Terminator::Trap,
            })
        }),
    ];
    let original = function();
    for &(label, edit) in edits {
        let mut changed = original.clone();
        changed.id = FunctionId(99); // First ordinary field already differs.
        changed.name = "different".into();
        edit(&mut changed);
        compare(&original, &changed, None, label);
        compare(
            &changed,
            &changed,
            None,
            "identical malformed owner still rejects",
        );
    }
}
