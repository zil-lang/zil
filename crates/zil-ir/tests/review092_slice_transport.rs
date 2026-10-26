//! Nonauthor transport/identity probes. These deliberately raw modules have
//! no executable authority and are never represented as semantic acceptance.
use std::collections::BTreeSet;
use zil_base::{Fingerprint, Limits, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

const ADDRESS: u32 = 0x1234_5678;
const START: u32 = 0x3456_789a;
const END: u32 = 0x4567_89ab;

fn raw() -> TypedModule {
    let window = Type::Address {
        mutable: false,
        inner: Box::new(Type::Slice {
            element: Box::new(Type::U16),
        }),
    };
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(17),
        source_fingerprint: Fingerprint::of(b"independent 092 raw transport"),
        target: TargetProfile::host_llvm(),
        resources: vec![],
        closures: vec![],
        unsupported: vec![],
        functions: vec![TypedFunction {
            id: FunctionId(0),
            name: "transport_only".into(),
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            callable_contract: None,
            callable_return_contract: None,
            foreign: None,
            locals: vec![],
            values: vec![window.clone(), window, Type::I64],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction {
                        result: Some(ValueId(1)),
                        kind: InstructionKind::AddressSlice {
                            address: ValueId(ADDRESS),
                            start: Some(ValueId(START)),
                            end: Some(ValueId(END)),
                        },
                        span: Span::default(),
                    },
                    Instruction {
                        result: Some(ValueId(2)),
                        kind: InstructionKind::SliceLength {
                            address: ValueId(1),
                        },
                        span: Span::default(),
                    },
                ],
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span: Span::default(),
        }],
    }
}

fn encoded(module: &TypedModule) -> Vec<u8> {
    wire::encode_module_candidate(
        module,
        &CandidatePlan::empty_for(module),
        &Limits::default(),
    )
    .unwrap()
}

#[test]
fn optional_range_presence_flags_require_canonical_zero_or_one() {
    let original = encoded(&raw());
    // Locate the uniquely identified instruction payload, not a guessed offset
    // into unrelated headers. The sentinels deliberately do not denote values.
    let mut instruction = vec![61];
    instruction.extend(ADDRESS.to_le_bytes());
    instruction.push(1);
    instruction.extend(START.to_le_bytes());
    instruction.push(1);
    instruction.extend(END.to_le_bytes());
    let occurrences = original
        .windows(instruction.len())
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == instruction).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(occurrences.len(), 1);
    for field_offset in [5, 10] {
        for invalid_flag in [2, 127, 255] {
            let mut changed = original.clone();
            changed[occurrences[0] + field_offset] = invalid_flag;
            let error = wire::decode_module_candidate(&changed, &Limits::default()).unwrap_err();
            assert_eq!(error[0].code, "E-WIRE-BOOL");
        }
    }
}

#[test]
fn all_new_fields_change_identity_and_roundtrip_without_loss() {
    let original = raw();
    let mut variants = vec![original.clone()];
    for which in 0..6 {
        let mut changed = original.clone();
        let InstructionKind::AddressSlice {
            address,
            start,
            end,
        } = &mut changed.functions[0].blocks[0].instructions[0].kind
        else {
            unreachable!()
        };
        match which {
            0 => *address = ValueId(ADDRESS + 1),
            1 => *start = None,
            2 => *start = Some(ValueId(START + 1)),
            3 => *end = None,
            4 => *end = Some(ValueId(END + 1)),
            5 => std::mem::swap(start, end),
            _ => unreachable!(),
        }
        variants.push(changed);
    }
    let mut changed = original.clone();
    changed.functions[0].blocks[0].instructions[1].kind = InstructionKind::SliceLength {
        address: ValueId(0),
    };
    variants.push(changed);
    for which in 0..3 {
        let mut changed = original.clone();
        let Type::Address { mutable, inner } = &mut changed.functions[0].values[0] else {
            unreachable!()
        };
        match which {
            0 => *mutable = true,
            1 => {
                **inner = Type::Slice {
                    element: Box::new(Type::U8),
                }
            }
            2 => {
                **inner = Type::Array {
                    element: Box::new(Type::U16),
                    length: 0,
                }
            }
            _ => unreachable!(),
        }
        variants.push(changed);
    }
    let mut identities = BTreeSet::new();
    let mut encodings = BTreeSet::new();
    assert_eq!(variants.len(), 11);
    for module in variants {
        assert!(identities.insert(module.fingerprint().to_hex()));
        let bytes = encoded(&module);
        assert!(encodings.insert(bytes.clone()));
        let (decoded, plan) = wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
        assert_eq!(decoded, module);
        assert_eq!(plan, CandidatePlan::empty_for(&module));
        assert_eq!(decoded.fingerprint(), module.fingerprint());
    }
}
