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

#[test]
fn saved_callback_array_replays_complete_element_types_and_indices_raw_wire() {
    let (mut module, _) = crate::tests::baseline();
    let header = StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 0,
        bindings: vec![],
        classes: vec![],
        static_bindings: vec![],
    };
    let mut entry = module.functions[0].clone();
    entry.id = FunctionId(1);
    entry.name = "closed_callback".into();
    entry.environment = Some(ClosureId(0));
    entry.parameters.clear();
    entry.locals.clear();
    entry.values.clear();
    entry.return_type = Type::Unit;
    entry.storage_life = Some(header.clone());
    entry.blocks[0].instructions.clear();
    entry.blocks[0].terminator = Terminator::Return(None);
    module.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: entry.id,
        captures: vec![],
        parameters: vec![],
        result: Type::Unit,
        kind: CallableKind::FnMut,
        storage_life: Some(header),
        lending_captures: vec![],
    });
    module.functions.push(entry);
    let closure = Type::Closure(ClosureId(0));
    let array = Type::Array {
        element: Box::new(closure.clone()),
        length: 1,
    };
    let f = &mut module.functions[0];
    f.parameters.clear();
    f.locals.truncate(1);
    f.locals[0].ty = array.clone();
    f.locals[0].mutable = true;
    f.values = vec![
        closure.clone(),
        array,
        Type::Address {
            mutable: true,
            inner: Box::new(closure.clone()),
        },
        closure,
        Type::I32,
    ];
    let i = |result: Option<u32>, kind| Instruction {
        result: result.map(ValueId),
        kind,
        span: f.span,
    };
    f.blocks[0].instructions = vec![
        i(None, InstructionKind::StorageLive(LocalId(0))),
        i(
            Some(0),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![],
            },
        ),
        i(
            Some(1),
            InstructionKind::MakeOwnedArray {
                elements: vec![ValueId(0)],
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
            InstructionKind::BorrowOwnedArrayElement {
                local: LocalId(0),
                index: 0,
                mutable: true,
            },
        ),
        i(
            None,
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(2),
                kind: CallableKind::FnMut,
                arguments: vec![],
            },
        ),
        i(
            Some(3),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(0),
                index: 0,
            },
        ),
        i(
            None,
            InstructionKind::StoreOwnedArrayElement {
                local: LocalId(0),
                index: 0,
                value: ValueId(3),
            },
        ),
        i(
            None,
            InstructionKind::DropOwnedArrayElement {
                local: LocalId(0),
                index: 0,
            },
        ),
        i(None, InstructionKind::DropOwnedArray { local: LocalId(0) }),
        i(Some(4), InstructionKind::Constant(Constant::I32(0))),
        i(None, InstructionKind::StorageDead(LocalId(0))),
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &CandidatePlan::empty_for(&module), &limits)
            .unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0;
    // This is the actual frame-layout/instruction sub-proof. It cannot prove
    // original creation, live element debt, Closed or a current Task epoch.
    for module in [&module, &decoded] {
        let check = |module: &TypedModule| {
            let mut b = Budget::new(&limits);
            b.function_count = module.functions.len();
            profile(module, &module.functions[0], &mut b, None)
        };
        assert!(check(module).unwrap().is_empty());
        for at in [4, 6, 7, 8] {
            let mut wrong = module.clone();
            let operation = &mut wrong.functions[0].blocks[0].instructions[at].kind;
            match operation {
                InstructionKind::BorrowOwnedArrayElement { index, .. }
                | InstructionKind::TakeOwnedArrayElement { index, .. }
                | InstructionKind::StoreOwnedArrayElement { index, .. }
                | InstructionKind::DropOwnedArrayElement { index, .. } => *index = 1,
                _ => unreachable!(),
            }
            assert!(
                check(&wrong).is_err(),
                "operation {at} must select the complete actual element"
            );
        }
        let mut wrong = module.clone();
        wrong.functions[0].values[0] = Type::I32;
        assert!(
            check(&wrong).is_err(),
            "a constructor cannot use a same-layout substitute for its complete Closure"
        );
        let mut wrong = module.clone();
        wrong.functions[0].locals[0].mutable = false;
        assert!(
            check(&wrong).is_err(),
            "refill/borrow requires the actual exclusive frame slot"
        );
    }
}
