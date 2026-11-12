//! Complete frozen-plan and machine-Module oracles; synthetic cases below do
//! not stand in for the separate original Source/native acceptance test.
use super::*;
use zil_ir::{BlockId, BlockPlan, CandidatePlan, FunctionPlan, ValueHold};

fn ins(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: Span::default(),
    }
}
fn fixture(mode: u8) -> zil_ir::TypedModule {
    let array = Type::Array {
        element: Box::new(Type::I64),
        length: if mode == 2 { 8 } else { 512 },
    };
    let aggregate = if mode == 1 {
        Type::Record {
            name: "ActualWideRecord".into(),
            fields: (0..512).map(|i| (format!("f{i}"), Type::I64)).collect(),
        }
    } else {
        array.clone()
    };
    let stored = if mode == 2 {
        Type::Enum {
            name: "enum_with_retained_source_names_".repeat(8),
            variants: vec![
                ("Small".repeat(32), vec![array.clone()]),
                (
                    "Wide".repeat(32),
                    vec![Type::Array {
                        element: Box::new(Type::I64),
                        length: 512,
                    }],
                ),
            ],
        }
    } else {
        aggregate.clone()
    };
    let fields = vec![ValueId(0); if mode == 2 { 8 } else { 512 }];
    let mut values = vec![Type::I64, aggregate];
    let mut instructions = vec![
        ins(Some(0), InstructionKind::Constant(Constant::I64(7))),
        ins(Some(1), InstructionKind::MakeAggregate { fields }),
    ];
    let stored_value = if mode == 2 {
        values.push(stored.clone());
        instructions.push(ins(
            Some(2),
            InstructionKind::MakeVariant {
                variant: 0,
                fields: vec![ValueId(1)],
            },
        ));
        2
    } else {
        1
    };
    instructions.push(ins(None, InstructionKind::StorageLive(LocalId(0))));
    instructions.push(ins(
        None,
        InstructionKind::Store {
            local: LocalId(0),
            value: ValueId(stored_value),
        },
    ));
    let loaded = values.len() as u32;
    values.push(stored.clone());
    instructions.push(ins(Some(loaded), InstructionKind::Load(LocalId(0))));
    let selected = if mode == 2 {
        let id = values.len() as u32;
        values.push(array);
        instructions.push(ins(
            Some(id),
            InstructionKind::ReadVariant {
                value: ValueId(loaded),
                variant: 0,
                index: 0,
            },
        ));
        id
    } else {
        loaded
    };
    let answer = if mode == 1 {
        let id = values.len() as u32;
        values.push(Type::I64);
        instructions.push(ins(
            Some(id),
            InstructionKind::ExtractField {
                aggregate: ValueId(selected),
                index: 511,
            },
        ));
        id
    } else {
        let index = values.len() as u32;
        values.push(Type::I64);
        instructions.push(ins(
            Some(index),
            InstructionKind::Constant(Constant::I64(0)),
        ));
        let id = values.len() as u32;
        values.push(Type::I64);
        instructions.push(ins(
            Some(id),
            InstructionKind::ExtractIndex {
                aggregate: ValueId(selected),
                index: ValueId(index),
            },
        ));
        id
    };
    let function = TypedFunction {
        id: FunctionId(0),
        name: "snapshot_probe".into(),
        environment: None,
        parameters: vec![],
        return_type: Type::I64,
        locals: vec![zil_ir::LocalDecl {
            id: LocalId(0),
            name: "stored".into(),
            ty: stored,
            mutable: false,
            span: Span::default(),
        }],
        values,
        blocks: vec![zil_ir::BasicBlock {
            id: BlockId(0),
            instructions,
            terminator: Terminator::Return(Some(ValueId(answer))),
        }],
        entry: BlockId(0),
        span: Span::default(),
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        result_mode: zil_ir::PassingMode::Infer,
        foreign: None,
    };
    no_dyn_test_module(TargetProfile::host_llvm(), &[function], &[], &[])
}
fn raw_lir(module: &zil_ir::TypedModule) -> LirModule {
    LirModule {
        schema_version: module.schema_version,
        module_id: module.module_id,
        verified_fingerprint: module.fingerprint(),
        target: module.target.clone(),
        functions: module.functions.clone(),
        closures: module.closures.clone(),
        resources: module.resources.clone(),
        layouts: vec![
            (Type::Unit, 0, 1),
            (Type::Bool, 1, 1),
            (Type::I32, 4, 4),
            (Type::I64, 8, 8),
            (Type::F32, 4, 4),
            (Type::F64, 8, 8),
            (Type::U8, 1, 1),
            (Type::U16, 2, 2),
            (Type::U32, 4, 4),
            (Type::I8, 1, 1),
            (Type::I16, 2, 2),
            (Type::Isize, 8, 8),
            (Type::ForeignHandle, 8, 8),
            (Type::CWideString, 8, 8),
        ]
        .into_iter()
        .map(|(ty, size, alignment)| ScalarLayout {
            ty,
            size,
            alignment,
        })
        .collect(),
    }
}
fn checked(module: zil_ir::TypedModule) -> VerifiedModule {
    let mut candidate = CandidatePlan::empty_for(&module);
    let f = &module.functions[0];
    candidate.functions.push(FunctionPlan {
        function: f.id,
        holds: (0..f.values.len())
            .map(|i| ValueHold {
                value: ValueId(i as u32),
                roots: vec![],
            })
            .collect(),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![LocalId(0)],
        }],
    });
    let target = module.target.clone();
    zil_verify::verify_module(module, candidate, &target, &Limits::default()).unwrap()
}

#[test]
fn complete_frozen_max_plan_and_machine_module_agree() {
    let limits = Limits::default();
    for mode in 0..=2 {
        let module = fixture(mode);
        let lir = raw_lir(&module);
        // This is private accounting inspection, not a raised Source/native
        // limit: record both full ledgers without accepting an oversized input.
        let mut actual = plan(&lir.as_view(), &limits, usize::MAX).unwrap();
        let mut old = max_plan_reference::plan(&lir.as_view(), &limits, usize::MAX).unwrap();
        let current_cost = usize::MAX - actual.remaining_work;
        let old_cost = usize::MAX - old.remaining_work;
        actual.remaining_work = 0;
        old.remaining_work = 0;
        assert_eq!(actual, old, "complete logical plan mode={mode}");
        let exact = plan(&lir.as_view(), &limits, current_cost).unwrap();
        assert_eq!(exact.remaining_work, 0);
        let errors = plan(&lir.as_view(), &limits, current_cost - 1)
            .err()
            .unwrap();
        assert_eq!(errors[0].phase, Phase::Codegen);
        assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(plan(&lir.as_view(), &limits, current_cost).unwrap(), exact);
        let old_exact = max_plan_reference::plan(&lir.as_view(), &limits, old_cost).unwrap();
        assert_eq!(old_exact.remaining_work, 0);
        assert_eq!(
            max_plan_reference::plan(&lir.as_view(), &limits, old_cost - 1)
                .err()
                .unwrap()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        // Actual immutable independent Verify precedes both complete machine
        // lowerings. Neither comparison is a text substring or a hash.
        let verified = checked(module);
        let actual_machine = lower_with_options(&verified, &lir, &EmitOptions::default()).unwrap();
        let old_machine = max_plan_reference::with_reference(|| {
            lower_with_options(&verified, &lir, &EmitOptions::default())
        })
        .unwrap();
        assert_eq!(
            actual_machine, old_machine,
            "complete machine module mode={mode}"
        );
        eprintln!("snapshot emission oracle mode={mode} new={current_cost} old={old_cost}");
    }
}

#[test]
fn wide_struct_clones_are_not_flattened_to_array_cardinality() {
    let limits = Limits::default();
    let array = fixture(0);
    let record = fixture(1);
    let array = plan(&raw_lir(&array).as_view(), &limits, usize::MAX).unwrap();
    let record = plan(&raw_lir(&record).as_view(), &limits, usize::MAX).unwrap();
    let a = usize::MAX - array.remaining_work;
    let r = usize::MAX - record.remaining_work;
    assert!(
        r > a * 16,
        "real Struct copies must retain their field multiplicity: {a}/{r}"
    );
}

#[test]
fn non_snapshot_load_prepays_its_actual_complex_pointee() {
    let limits = Limits::default();
    let mut charges = vec![];
    for width in [1, 32, 256] {
        let mut module = fixture(0);
        let f = &mut module.functions[0];
        let ty = Type::Address {
            mutable: false,
            inner: Box::new(Type::Record {
                name: "Pointee".repeat(4),
                fields: (0..width)
                    .map(|i| (format!("field{i}"), Type::I64))
                    .collect(),
            }),
        };
        f.values.push(ty);
        let id = ValueId((f.values.len() - 1) as u32);
        let lir = raw_lir(&module);
        let plans = plan(&lir.as_view(), &limits, usize::MAX).unwrap();
        let values = &plans.values[&FunctionId(0)];
        let mut left = usize::MAX;
        let machines = emission_work::allocations(values, &mut left, &limits).unwrap();
        let before = left;
        assert!(
            emission_work::instruction(
                &module.functions[0],
                &ins(Some(id.0), InstructionKind::Load(LocalId(0))),
                values,
                &machines,
                &mut left,
                &limits
            )
            .unwrap()
        );
        charges.push(before - left);
    }
    assert!(charges[1] > charges[0] * 4);
    assert!(charges[2] > charges[1] * 4, "{charges:?}");
}

#[test]
fn enum_source_and_canonical_payload_keep_the_complete_legacy_fee() {
    let module = fixture(2);
    let lir = raw_lir(&module);
    let limits = Limits::default();
    let planned = plan(&lir.as_view(), &limits, usize::MAX).unwrap();
    let values = &planned.values[&FunctionId(0)];
    let mut left = usize::MAX;
    let machines = emission_work::allocations(values, &mut left, &limits).unwrap();
    for i in &module.functions[0].blocks[0].instructions {
        if matches!(
            i.kind,
            InstructionKind::MakeVariant { .. }
                | InstructionKind::ReadVariant { .. }
                | InstructionKind::Store { .. }
                | InstructionKind::TakeLocal { .. }
        ) {
            let before = left;
            assert!(
                !emission_work::instruction(
                    &module.functions[0],
                    i,
                    values,
                    &machines,
                    &mut left,
                    &limits
                )
                .unwrap()
            );
            assert_eq!(left, before);
        }
    }
    assert!(canonical_payload_work(&module.functions[0].values[1]) >= 24);
}

#[test]
fn complete_reference_covers_protocols_indirect_signatures_and_rejections() {
    let limits = Limits::default();
    let mut module = fixture(0);
    module.resources.push(ResourceDefinition {
        name: "@builtin:OwnedStore<i64>".into(),
        payload: Type::OwnedStore {
            element: Box::new(Type::I64),
        },
        teardown: None,
    });
    let big = module.functions[0].values[1].clone();
    module.functions[0].values.push(Type::FunctionPointer {
        parameters: vec![big.clone()],
        result: Box::new(big.clone()),
    });
    let lir = raw_lir(&module);
    let mut actual = plan(&lir.as_view(), &limits, usize::MAX).unwrap();
    let mut old = max_plan_reference::plan(&lir.as_view(), &limits, usize::MAX).unwrap();
    assert!(!actual.store_protocols.is_empty());
    assert!(!actual.indirect.is_empty());
    actual.remaining_work = 0;
    old.remaining_work = 0;
    assert_eq!(actual, old);
    module.functions[0].values.push(Type::FunctionPointer {
        parameters: vec![Type::ForeignCallback {
            parameters: vec![big],
            result: Box::new(Type::I64),
        }],
        result: Box::new(Type::I64),
    });
    let lir = raw_lir(&module);
    let actual = plan(&lir.as_view(), &limits, usize::MAX).err().unwrap();
    let old = max_plan_reference::plan(&lir.as_view(), &limits, usize::MAX)
        .err()
        .unwrap();
    assert_eq!(format!("{actual:#?}"), format!("{old:#?}"));
    assert_eq!(actual[0].category, zil_base::ResultCategory::Unsupported);
}
