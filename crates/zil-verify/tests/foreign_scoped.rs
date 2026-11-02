//! Independent current raw/Wire trust-boundary tests, not source/native evidence.
#[path = "support/foreign_scoped.rs"]
mod fixture;
use zil_base::{Limits, ResultCategory};
use zil_ir::*;
fn check(module: TypedModule, mut plan: CandidatePlan, expected: Option<&str>) {
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let target = module.target.clone();
    let raw = zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits);
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits);
    for result in [raw, decoded] {
        if let Some(code) = expected {
            let errors = result.unwrap_err();
            assert_eq!(errors[0].code, code, "{errors:?}");
            assert_eq!(errors[0].category, ResultCategory::SafeReject);
        } else {
            result.unwrap();
        }
    }
}
#[test]
fn actual_mutable_environment_and_consumed_body_pass_raw_and_wire() {
    let (m, p) = fixture::fixture();
    check(m, p, None)
}
#[test]
fn callback_role_and_signature_are_rebuilt_from_declarations() {
    for change in 0..4 {
        let (mut m, p) = fixture::fixture();
        match change {
            0 => {
                m.functions[0].foreign.as_mut().unwrap().scoped_callback =
                    Some(ScopedCallbackRole::Close)
            }
            1 => m.functions[1].foreign.as_mut().unwrap().scoped_callback = None,
            2 => m.functions[0].return_type = Type::I32,
            3 => m.functions[1].foreign.as_mut().unwrap().static_callbacks = false,
            _ => unreachable!(),
        }
        check(m, p, Some("E-VERIFY-FOREIGN"));
    }
}
#[test]
fn missing_actual_unsafe_region_is_rejected() {
    let (mut m, p) = fixture::fixture();
    if let InstructionKind::WithForeignCallback { region, .. } =
        &mut m.functions[4].blocks[0].instructions[6].kind
    {
        *region = 8
    }
    check(m, p, Some("E-VERIFY-UNSAFE"));
}
#[test]
fn once_callback_and_shared_callback_address_are_rejected() {
    let (mut m, p) = fixture::fixture();
    m.closures[0].kind = CallableKind::FnOnce;
    check(m, p, Some("E-VERIFY-SCOPED-CALLBACK"));
    let (mut m, p) = fixture::fixture();
    if let Type::Address { mutable, .. } = &mut m.functions[4].values[1] {
        *mutable = false
    };
    m.functions[4].blocks[0].instructions[3].kind =
        InstructionKind::BorrowLocal { local: LocalId(0) };
    check(m, p, Some("E-VERIFY-SCOPED-CALLBACK"));
}
#[test]
fn body_responsibility_cannot_be_consumed_twice() {
    let (mut m, mut p) = fixture::fixture();
    m.functions[4].blocks[0].instructions.insert(
        7,
        fixture::instruction(
            None,
            InstructionKind::WithForeignCallback {
                register: FunctionId(0),
                close: FunctionId(1),
                callback: ValueId(1),
                body: ValueId(2),
                region: 7,
            },
        ),
    );
    m.functions[4]
        .values
        .push(Type::Tuple(vec![Type::Bool, Type::I32]));
    m.functions[4].blocks[0].instructions[7].result = Some(ValueId(4));
    p.functions[4].holds.push(ValueHold {
        value: ValueId(4),
        roots: vec![],
    });
    check(m, p, Some("E-VERIFY-RESOURCE"));
}
