//! Repeated field work must pay for its payload on every dataflow visit, even
//! when the type and SSA input were already admitted once by raw preflight.
use zil_analyze::analyze;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

#[derive(Clone, Copy, Debug)]
enum Operation {
    StoreCopy,
    ReadCopy,
    TakeRefill,
    Describe,
}

fn candidate(copy: Type, operation: Operation, repetitions: usize) -> TypedModule {
    let span = Span::new(FileId(0), 0, 1);
    let payload = Type::Record {
        name: "R".into(),
        fields: vec![("n".into(), copy.clone())],
    };
    let managed = Type::Managed(Box::new(payload.clone()));
    let package = Type::Package {
        name: "P".into(),
        fields: vec![
            ("r".into(), managed.clone()),
            ("value".into(), copy.clone()),
        ],
    };
    let ins = |result, kind| Instruction { result, kind, span };
    let mut instructions = vec![ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) })];
    let values = if matches!(operation, Operation::StoreCopy) {
        // Exactly one Copy SSA carrier and one admitted type for every store.
        instructions.push(ins(Some(ValueId(0)), InstructionKind::Load(LocalId(1))));
        for _ in 0..repetitions {
            instructions.push(ins(
                None,
                InstructionKind::StorePackageField {
                    local: LocalId(0),
                    index: 1,
                    value: ValueId(0),
                },
            ));
        }
        vec![copy.clone()]
    } else {
        // Keep the type/value inventory identical for 1 and 32 field uses.
        // Ordinary local operations fill the remaining positions. This makes
        // the width × repetitions interaction independent of preflight cost.
        for index in 0..32 {
            let field = index < repetitions;
            let value = ValueId(index as u32);
            let kind = match operation {
                Operation::ReadCopy if field => InstructionKind::ReadPackageField {
                    local: LocalId(0),
                    index: 1,
                },
                Operation::ReadCopy => InstructionKind::Load(LocalId(1)),
                Operation::Describe if field => InstructionKind::DescribePackageField {
                    local: LocalId(0),
                    index: 0,
                },
                Operation::Describe => InstructionKind::DescribeLocal { local: LocalId(2) },
                Operation::TakeRefill if field => InstructionKind::TakePackageField {
                    local: LocalId(0),
                    index: 0,
                },
                Operation::TakeRefill => InstructionKind::TakeLocal { local: LocalId(2) },
                Operation::StoreCopy => unreachable!(),
            };
            instructions.push(ins(Some(value), kind));
            if matches!(operation, Operation::TakeRefill) {
                instructions.push(ins(
                    None,
                    if field {
                        InstructionKind::StorePackageField {
                            local: LocalId(0),
                            index: 0,
                            value,
                        }
                    } else {
                        InstructionKind::Store {
                            local: LocalId(2),
                            value,
                        }
                    },
                ));
            }
        }
        vec![
            match operation {
                Operation::ReadCopy => copy.clone(),
                Operation::Describe => Type::Desc(Box::new(payload.clone())),
                Operation::TakeRefill => managed.clone(),
                Operation::StoreCopy => unreachable!(),
            };
            32
        ]
    };
    instructions.extend([
        ins(None, InstructionKind::DropManaged { local: LocalId(2) }),
        ins(None, InstructionKind::DropPackage { local: LocalId(0) }),
        ins(None, InstructionKind::StorageDead(LocalId(2))),
        ins(None, InstructionKind::StorageDead(LocalId(1))),
        ins(None, InstructionKind::StorageDead(LocalId(0))),
        ins(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"package field budget controls"),
        target: TargetProfile::host_llvm(),
        functions: vec![TypedFunction {
            id: FunctionId(0),
            name: "probe".into(),
            foreign: None,
            environment: None,
            callable_return_contract: None,
            callable_contract: None,
            parameters: vec![LocalId(0), LocalId(1), LocalId(2)],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: [package, copy, managed]
                .into_iter()
                .enumerate()
                .map(|(index, ty)| LocalDecl {
                    id: LocalId(index as u32),
                    name: format!("p{index}"),
                    ty,
                    mutable: true,
                    span,
                })
                .collect(),
            values,
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span,
        }],
        resources: vec![ResourceDefinition {
            name: "R".into(),
            payload,
            teardown: None,
        }],
        closures: vec![],
        unsupported: vec![],
    }
}

fn minimum(raw: &TypedModule) -> usize {
    analyze(raw, &Limits::default()).expect("raw budget control must be admissible");
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        match analyze(
            raw,
            &Limits {
                max_analysis_steps: mid,
                ..Limits::default()
            },
        ) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert_eq!(errors.len(), 1);
                assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                assert_eq!(errors[0].phase, Phase::Analyze);
                assert_eq!(errors[0].code, "E-RESOURCE-LIMIT");
                low = mid + 1;
            }
        }
    }
    analyze(
        raw,
        &Limits {
            max_analysis_steps: low,
            ..Limits::default()
        },
    )
    .unwrap();
    let errors = analyze(
        raw,
        &Limits {
            max_analysis_steps: low - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "E-RESOURCE-LIMIT");
    assert_eq!(errors[0].phase, Phase::Analyze);
    low
}

fn width_interaction(operation: Operation, shape: impl Fn(usize) -> Type) {
    let budgets: Vec<_> = [(2, 1), (2, 32), (64, 1), (64, 32)]
        .into_iter()
        .map(|(width, repetitions)| minimum(&candidate(shape(width), operation, repetitions)))
        .collect();
    let narrow_delta = budgets[1] as isize - budgets[0] as isize;
    let wide_delta = budgets[3] as isize - budgets[2] as isize;
    println!(
        "{operation:?}: {budgets:?}; width interaction = {}",
        wide_delta - narrow_delta
    );
    assert!(
        wide_delta - narrow_delta >= (32 - 1) * (64 - 2),
        "repeated field work must pay for payload width, beyond one-time preflight: {budgets:?}"
    );
}

#[test]
fn repeated_copy_field_stores_charge_payload_width() {
    width_interaction(Operation::StoreCopy, |width| {
        Type::Tuple(vec![Type::I32; width])
    });
}

#[test]
fn repeated_copy_field_reads_charge_payload_width() {
    width_interaction(Operation::ReadCopy, |width| {
        Type::Tuple(vec![Type::I32; width])
    });
}

#[test]
fn repeated_managed_field_take_refill_charges_payload_width() {
    width_interaction(Operation::TakeRefill, |width| {
        Type::Tuple(vec![Type::I32; width])
    });
}

#[test]
fn repeated_field_descriptions_charge_payload_width() {
    width_interaction(Operation::Describe, |width| {
        Type::Tuple(vec![Type::I32; width])
    });
}

#[test]
fn repeated_field_comparisons_charge_nominal_name_bytes() {
    width_interaction(Operation::StoreCopy, |width| Type::Record {
        name: "N".repeat(width),
        fields: vec![],
    });
}
