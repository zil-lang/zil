//! Actual frame qualification only; initialized storage and descriptor loans
//! are still required by the ordinary resource/borrow kernels.
use super::*;
fn module() -> TypedModule {
    let (mut m, _) = crate::tests::baseline();
    let record = Type::Record {
        name: "FrameToken".into(),
        fields: vec![("code".into(), Type::I32)],
    };
    m.resources.push(ResourceDefinition {
        name: "FrameToken".into(),
        payload: record.clone(),
        teardown: None,
    });
    let f = &mut m.functions[0];
    f.locals[0].ty = Type::Package {
        name: "Packet".into(),
        fields: vec![("token".into(), Type::Managed(Box::new(record.clone())))],
    };
    f.values = vec![Type::Desc(Box::new(record)), Type::I32];
    f.blocks.truncate(1);
    f.blocks[0].instructions = vec![
        Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::DescribePackageField {
                local: LocalId(0),
                index: 0,
            },
            span: f.span,
        },
        Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::ReadDesc {
                desc: ValueId(0),
                index: 0,
            },
            span: f.span,
        },
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    m
}
#[test]
fn saved_package_description_checks_exact_owner_field_and_descriptor_raw_wire() {
    let m = module();
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(&m, &CandidatePlan::empty_for(&m), &limits).unwrap();
    let (wire, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    for m in [&m, &wire] {
        let run = |m: &TypedModule| {
            let mut b = Budget::new(&limits);
            b.function_count = m.functions.len();
            profile(m, &m.functions[0], &mut b, None)
        };
        assert!(run(m).unwrap().is_empty());
        for attack in 0..4 {
            let mut wrong = m.clone();
            let f = &mut wrong.functions[0];
            match attack {
                0 => {
                    f.blocks[0].instructions[0].kind = InstructionKind::DescribePackageField {
                        local: LocalId(0),
                        index: 1,
                    }
                }
                1 => f.blocks[0].instructions[0].result = Some(ValueId(1)),
                2 => {
                    f.blocks[0].instructions[1].kind = InstructionKind::ReadDesc {
                        desc: ValueId(0),
                        index: 1,
                    }
                }
                _ => f.values[0] = Type::I32,
            }
            assert!(run(&wrong).is_err(), "attack {attack}");
        }
    }
}
