//! Exact activation components; original-node identity and actual resource/
//! loan dataflow remain independently checked by their normal consumers.
use super::*;
fn module() -> TypedModule {
    let (mut m, _) = crate::tests::baseline();
    let f = &mut m.functions[0];
    let owner = Type::Managed(Box::new(Type::ByteBuffer));
    let tuple = Type::Tuple(vec![owner.clone(), Type::I32]);
    f.parameters.clear();
    f.locals.truncate(1);
    f.locals[0].ty = tuple.clone();
    for (id, ty) in [(1, owner), (2, Type::I32)] {
        let mut local = f.locals[0].clone();
        local.id = LocalId(id);
        local.ty = ty;
        f.locals.push(local);
    }
    f.values = vec![tuple, Type::I32];
    f.blocks.truncate(1);
    let i = |result: Option<u32>, kind| Instruction {
        result: result.map(ValueId),
        kind,
        span: f.span,
    };
    f.blocks[0].instructions = vec![
        i(None, InstructionKind::StorageLive(LocalId(0))),
        i(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(0),
            },
        ),
        i(None, InstructionKind::StorageLive(LocalId(1))),
        i(None, InstructionKind::StorageLive(LocalId(2))),
        i(
            None,
            InstructionKind::UnpackTuple {
                local: LocalId(0),
                fields: vec![LocalId(1), LocalId(2)],
            },
        ),
        i(Some(1), InstructionKind::Load(LocalId(2))),
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    m
}
fn budget(m: &TypedModule) -> Budget {
    let mut b = Budget::new(&Limits::default());
    b.function_count = m.functions.len();
    b.sums = true;
    b
}
#[test]
fn atomic_pattern_initialization_keeps_the_exact_bundle_and_readonly_scalar_raw_wire() {
    let m = module();
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(&m, &CandidatePlan::empty_for(&m), &limits).unwrap();
    let (wire, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    for m in [&m, &wire] {
        let f = &m.functions[0];
        let mut b = budget(m);
        for local in [LocalId(1), LocalId(2)] {
            assert_eq!(
                pattern_initial_transfer(m, f, local, f.span, &mut b).unwrap(),
                PatternStart::Unpack {
                    local,
                    at: (BlockId(0), 4)
                }
            );
        }
        let slot = scalar_preparations::unpacked_slot(f, LocalId(2), (BlockId(0), 4), &mut b)
            .unwrap()
            .unwrap();
        assert!(slot.stores.is_empty());
        assert_eq!(slot.loads.len(), 1);
        assert!(
            scalar_preparations::slot(f, LocalId(2), true, &mut b)
                .unwrap()
                .is_none(),
            "ordinary EvalOnce does not silently inherit an unpack exception"
        );
        assert!(
            scalar_preparations::unpacked_slot(f, LocalId(2), (BlockId(0), 3), &mut b)
                .unwrap()
                .is_none()
        );
        for attack in 0..5 {
            let mut wrong = m.clone();
            let f = &mut wrong.functions[0];
            match attack {
                0 => {
                    let InstructionKind::UnpackTuple { fields, .. } =
                        &mut f.blocks[0].instructions[4].kind
                    else {
                        unreachable!()
                    };
                    fields.reverse();
                }
                1 => {
                    let InstructionKind::UnpackTuple { fields, .. } =
                        &mut f.blocks[0].instructions[4].kind
                    else {
                        unreachable!()
                    };
                    fields[0] = LocalId(2);
                }
                2 => f.blocks[0].instructions[3].kind = InstructionKind::Constant(Constant::I32(1)),
                3 => {
                    let repeat = f.blocks[0].instructions[3].clone();
                    f.blocks[0].instructions.push(repeat);
                }
                _ => f.blocks[0].instructions[4].kind = InstructionKind::Load(LocalId(0)),
            }
            assert!(
                pattern_initial_transfer(
                    &wrong,
                    &wrong.functions[0],
                    LocalId(2),
                    f_span(m),
                    &mut budget(&wrong)
                )
                .is_err(),
                "attack {attack}"
            );
        }
        let mut aliased = m.clone();
        aliased.functions[0].blocks[0]
            .instructions
            .push(Instruction {
                result: None,
                kind: InstructionKind::BorrowLocal { local: LocalId(2) },
                span: f.span,
            });
        assert!(
            scalar_preparations::unpacked_slot(
                &aliased.functions[0],
                LocalId(2),
                (BlockId(0), 4),
                &mut budget(&aliased)
            )
            .unwrap()
            .is_none()
        );
    }
}
fn f_span(m: &TypedModule) -> Span {
    m.functions[0].span
}
