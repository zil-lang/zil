//! Internal abstract-domain/ownership guards, not Source acceptance authority.
//! Existing driver scalar_local_promotion tests provide the Source/native chain.
use super::*;
use zil_base::Span;
use zil_ir::{BasicBlock, BlockId, FunctionId, LocalDecl, LocalId};

struct Builder {
    values: Vec<Type>,
    locals: Vec<LocalDecl>,
    blocks: Vec<BasicBlock>,
}
impl Builder {
    fn new(types: &[Type]) -> Self {
        Self {
            values: vec![],
            locals: types
                .iter()
                .enumerate()
                .map(|(id, ty)| LocalDecl {
                    id: LocalId(id as u32),
                    name: format!("local{id}"),
                    ty: ty.clone(),
                    mutable: true,
                    span: Span::default(),
                })
                .collect(),
            blocks: vec![],
        }
    }
    fn value(&mut self, ty: Type, kind: I) -> Instruction {
        let id = ValueId(self.values.len() as u32);
        self.values.push(ty);
        Instruction {
            result: Some(id),
            kind,
            span: Span::default(),
        }
    }
    fn statement(kind: I) -> Instruction {
        Instruction {
            result: None,
            kind,
            span: Span::default(),
        }
    }
    fn block(&mut self, instructions: Vec<Instruction>, terminator: Terminator) {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(BasicBlock {
            id,
            instructions,
            terminator,
        });
    }
    fn finish(self, parameters: Vec<LocalId>, return_type: Type) -> TypedFunction {
        TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            id: FunctionId(0),
            name: "current".into(),
            foreign: None,
            environment: None,
            parameters,
            return_type,
            result_mode: PassingMode::Infer,
            locals: self.locals.into(),
            values: self.values.into(),
            blocks: self.blocks.into(),
            entry: BlockId(0),
            span: Span::default(),
        }
    }
}
fn id(inst: &Instruction) -> ValueId {
    inst.result.unwrap()
}
// Parameter-top first checked operation is kept; signed rem narrows its defined
// result before the next checked operation is even expanded by the emitter.
fn chain() -> (TypedFunction, ValueId, ValueId, ValueId, ValueId) {
    let mut b = Builder::new(&[Type::I32, Type::I32]);
    let load = b.value(Type::I32, I::Load(LocalId(0)));
    let x = id(&load);
    let three = b.value(Type::I32, I::Constant(Constant::I32(3)));
    let c3 = id(&three);
    let first = b.value(
        Type::I32,
        I::Binary {
            op: BinaryOp::Add,
            left: x,
            right: c3,
        },
    );
    let first_id = id(&first);
    let divisor = b.value(Type::I32, I::Constant(Constant::I32(251)));
    let d = id(&divisor);
    let rem = b.value(
        Type::I32,
        I::Binary {
            op: BinaryOp::Remainder,
            left: first_id,
            right: d,
        },
    );
    let r = id(&rem);
    let store = Builder::statement(I::Store {
        local: LocalId(1),
        value: r,
    });
    let load_y = b.value(Type::I32, I::Load(LocalId(1)));
    let y = id(&load_y);
    let two = b.value(Type::I32, I::Constant(Constant::I32(2)));
    let c2 = id(&two);
    let last = b.value(
        Type::I32,
        I::Binary {
            op: BinaryOp::Multiply,
            left: y,
            right: c2,
        },
    );
    let output = id(&last);
    b.block(
        vec![load, three, first, divisor, rem, store, load_y, two, last],
        Terminator::Return(Some(output)),
    );
    (
        b.finish(vec![LocalId(0)], Type::I32),
        first_id,
        output,
        y,
        c2,
    )
}
#[test]
fn current_source_binding_and_checked_extremes_are_complete() {
    let target = TargetProfile::host_llvm();
    let (source, first, last, y, two) = chain();
    let facts = prepare(&source, &target).unwrap();
    assert!(facts.safe[first.0 as usize].is_none());
    assert!(facts.binary(&source, &target, BinaryOp::Multiply, y, two, last));
    assert!(!facts.binary(&source, &target, BinaryOp::Add, y, two, last));
    assert!(!facts.binary(&source, &target, BinaryOp::Multiply, two, y, last));
    assert!(!facts.binary(&source, &target, BinaryOp::Multiply, y, two, first));
    let same_bytes = source.clone();
    let same_target = target.clone();
    assert!(!facts.binary(&same_bytes, &target, BinaryOp::Multiply, y, two, last));
    assert!(!facts.binary(&source, &same_target, BinaryOp::Multiply, y, two, last));
    for (ty, lo, hi) in [
        (Type::I8, -128, 127),
        (Type::I16, -32768, 32767),
        (Type::I32, i32::MIN as i128, i32::MAX as i128),
        (Type::I64, i64::MIN as i128, i64::MAX as i128),
        (Type::U64, 0, u64::MAX as i128),
    ] {
        let integer = Integer::of(&ty).unwrap();
        for (op, a, b, expected) in [
            (BinaryOp::Add, hi, 1, false),
            (BinaryOp::Subtract, lo, 1, false),
            (BinaryOp::Multiply, hi, hi, false),
            (BinaryOp::Multiply, hi, 1, true),
            (BinaryOp::Add, lo, 0, true),
        ] {
            let a = Range::singleton(integer, a).unwrap();
            let b = Range::singleton(integer, b).unwrap();
            assert_eq!(
                arithmetic(op, a, b).1,
                expected,
                "{ty:?} {op:?} {a:?} {b:?}"
            );
        }
    }
    let signed = Integer::of(&Type::I64).unwrap();
    assert!(
        !arithmetic(
            BinaryOp::Multiply,
            Range::singleton(signed, i64::MIN as i128).unwrap(),
            Range::singleton(signed, -1).unwrap()
        )
        .1
    );
}
fn diamond(initialize_else: bool) -> (TypedFunction, ValueId, ValueId, ValueId) {
    let mut b = Builder::new(&[Type::Bool, Type::I32]);
    let load = b.value(Type::Bool, I::Load(LocalId(0)));
    let condition = id(&load);
    b.block(
        vec![load],
        Terminator::Branch {
            condition,
            then_block: BlockId(1),
            else_block: BlockId(2),
        },
    );
    let a = b.value(Type::I32, I::Constant(Constant::I32(-250)));
    let aid = id(&a);
    b.block(
        vec![
            a,
            Builder::statement(I::Store {
                local: LocalId(1),
                value: aid,
            }),
        ],
        Terminator::Jump(BlockId(3)),
    );
    let c = b.value(Type::I32, I::Constant(Constant::I32(250)));
    let cid = id(&c);
    let mut other = vec![c];
    if initialize_else {
        other.push(Builder::statement(I::Store {
            local: LocalId(1),
            value: cid,
        }));
    }
    b.block(other, Terminator::Jump(BlockId(3)));
    let load = b.value(Type::I32, I::Load(LocalId(1)));
    let x = id(&load);
    let two = b.value(Type::I32, I::Constant(Constant::I32(2)));
    let y = id(&two);
    let sum = b.value(
        Type::I32,
        I::Binary {
            op: BinaryOp::Multiply,
            left: x,
            right: y,
        },
    );
    let result = id(&sum);
    b.block(vec![load, two, sum], Terminator::Return(Some(result)));
    (b.finish(vec![LocalId(0)], Type::I32), x, y, result)
}
#[test]
fn block_worklist_unions_locals_and_never_initializes_a_missing_path() {
    let target = TargetProfile::host_llvm();
    for initialized in [false, true] {
        let (source, x, y, result) = diamond(initialized);
        let facts = prepare(&source, &target).unwrap();
        assert_eq!(
            facts.binary(&source, &target, BinaryOp::Multiply, x, y, result),
            initialized
        );
    }
    let (mut source, x, y, result) = diamond(true);
    // Two identical source edges must count twice, without double-processing
    // the successor or replacing its local state with an invented bottom.
    source.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(0),
        then_block: BlockId(1),
        else_block: BlockId(1),
    };
    let facts = prepare(&source, &target).unwrap();
    assert!(facts.binary(&source, &target, BinaryOp::Multiply, x, y, result));
}
#[test]
fn unsupported_alias_call_cycle_and_fuel_domains_never_grant_partial_sites() {
    let target = TargetProfile::host_llvm();
    let (original, _, _, _, _) = chain();
    for change in 0..8 {
        let mut source = original.clone();
        match change {
            0 => {
                source.locals[0].ty = Type::Address {
                    mutable: true,
                    inner: Box::new(Type::I32),
                }
            }
            1 => source.blocks[0].instructions[0].kind = I::BorrowLocal { local: LocalId(0) },
            2 => {
                source.blocks[0].instructions[0].kind = I::Call {
                    function: FunctionId(1),
                    arguments: vec![],
                }
            }
            3 => source.blocks[0].terminator = Terminator::Jump(BlockId(0)),
            4 => source.result_mode = PassingMode::Borrow,
            5 => source.environment = Some(zil_ir::ClosureId(0)),
            6 => source.blocks[0].instructions[0].result = Some(ValueId(u32::MAX)),
            _ => {
                source.blocks[0].instructions[0].kind = I::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(0),
                    right: ValueId(1),
                }
            }
        }
        assert!(prepare(&source, &target).is_none(), "domain {change}");
    }
    let snapshot = original.clone();
    let mut completed = false;
    for fuel in 0..4096 {
        if let Some(facts) = analyze(&original, &target, fuel) {
            assert!(facts.safe.iter().any(Option::is_some));
            completed = true;
            break;
        }
        assert_eq!(original, snapshot);
    }
    assert!(completed);
    let mut wrong_target = target.clone();
    wrong_target.pointer_width = 32;
    assert!(prepare(&original, &wrong_target).is_none());
}
