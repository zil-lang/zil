//! Frozen raw/Wire31 operands from the independent Analyze 253 review.
use zil_base::{Limits, TargetProfile};

#[test]
fn forwarded_f_cannot_pass_as_its_own_i32_callback_argument() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for (name, bytes, expected) in [
        (
            "f_as_i32",
            include_bytes!("../../../tests/impl254/escape_callback_argument.wire").as_slice(),
            "E-VERIFY-CLOSURE-CALL",
        ),
        (
            "symbolic_copy",
            include_bytes!("../../../tests/impl254/control_callback_i32_argument.wire").as_slice(),
            "E-VERIFY-CALLABLE-CONTRACT",
        ),
    ] {
        let (typed, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
        for result in [
            zil_verify::verify_module(typed, plan, &target, &limits),
            zil_verify::verify_module_bytes(bytes, &target, &limits),
        ] {
            let errors = result.unwrap_err();
            assert_eq!(errors[0].code, expected, "{name}: {errors:?}");
        }
    }
    // A genuine Fn call borrows its live local before the by-value transfer
    // to Return. The callback's i32 argument stays an actual i32.
    let (mut typed, mut plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("../../../tests/impl254/control_callback_i32_argument.wire"),
        &limits,
    )
    .unwrap();
    let instructions = &mut typed.functions[1].blocks[0].instructions;
    instructions[3].kind = zil_ir::InstructionKind::CallClosureShared {
        callee: zil_ir::LocalId(1),
        arguments: vec![zil_ir::ValueId(1)],
    };
    instructions.swap(1, 2);
    instructions.swap(2, 3);
    plan.module_fingerprint = typed.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for result in [
        zil_verify::verify_module(typed, plan, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        result.unwrap();
    }
}
