//! Raw transport and identity tests only. Round-trip decoding grants no safety
//! authority; independent semantic near-neighbor tests live in zil-verify.
use std::collections::BTreeSet;
use zil_base::{Fingerprint, Limits, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

fn raw(start: Option<ValueId>, end: Option<ValueId>) -> TypedModule {
    let slice = Type::Address {
        mutable: false,
        inner: Box::new(Type::Slice {
            element: Box::new(Type::U8),
        }),
    };
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"raw wire slice transport only"),
        target: TargetProfile::host_llvm(),
        resources: vec![],
        closures: vec![],
        unsupported: vec![],
        functions: vec![TypedFunction {
            id: FunctionId(0),
            name: "raw".into(),
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            callable_contract: None,
            callable_return_contract: None,
            foreign: None,
            locals: vec![],
            values: vec![slice.clone(), slice, Type::I64],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction {
                        result: Some(ValueId(1)),
                        kind: InstructionKind::AddressSlice {
                            address: ValueId(0),
                            start,
                            end,
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

#[test]
fn both_optional_endpoints_and_slice_element_participate_in_wire_and_identity() {
    let limits = Limits::default();
    let mut identities = BTreeSet::new();
    for start in [None, Some(ValueId(0)), Some(ValueId(2))] {
        for end in [None, Some(ValueId(0)), Some(ValueId(2))] {
            let module = raw(start, end);
            let plan = CandidatePlan::empty_for(&module);
            let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            let (decoded, decoded_plan) = wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(decoded, module);
            assert_eq!(decoded_plan, plan);
            assert!(identities.insert(module.fingerprint().to_hex()));
        }
    }
    let mut module = raw(None, None);
    let fingerprint = module.fingerprint();
    let Type::Address { inner, .. } = &mut module.functions[0].values[1] else {
        unreachable!()
    };
    let Type::Slice { element } = inner.as_mut() else {
        unreachable!()
    };
    **element = Type::U16;
    assert_ne!(module.fingerprint(), fingerprint);
}

#[test]
fn new_recursive_pointee_is_bounded_before_wire_allocation() {
    let mut module = raw(None, None);
    let mut ty = Type::U8;
    for _ in 0..32 {
        ty = Type::Slice {
            element: Box::new(ty),
        };
    }
    module.functions[0].values[0] = ty;
    let plan = CandidatePlan::empty_for(&module);
    let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let bounded = Limits {
        max_call_depth: 8,
        ..Limits::default()
    };
    let errors = wire::decode_module_candidate(&bytes, &bounded).unwrap_err();
    assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
    assert!(wire::encode_module_candidate(&module, &plan, &bounded).is_err());
}

#[test]
fn range_transport_cannot_be_relabelled_as_an_old_or_unknown_wire_version() {
    let module = raw(Some(ValueId(0)), Some(ValueId(2)));
    let limits = Limits::default();
    let bytes = wire::encode_module_candidate(&module, &CandidatePlan::empty_for(&module), &limits)
        .unwrap();
    for version in (0..SCHEMA_VERSION).chain([SCHEMA_VERSION + 1, u32::MAX]) {
        let mut changed = bytes.clone();
        changed[8..12].copy_from_slice(&version.to_le_bytes());
        assert!(
            wire::decode_module_candidate(&changed, &limits).is_err(),
            "wire {version}"
        );
    }
}
