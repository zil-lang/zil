//! Source-derived raw and Wire controls for independent trait lending Verify.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, InstructionKind, TypedModule, ValueId};

fn source() -> String {
    // Keep the declaration and provider identical to the project consumer.
    // Flatten only import spelling so this raw test can inspect one TypedModule.
    let api = include_str!("project/api.zil");
    let cursor = include_str!("project/cursor.zil")
        .replace("import api", "")
        .replace("api.LendingIterator", "LendingIterator");
    format!(
        "{api}\n{cursor}\nfn main()->i32{{var first=make(20,22);var second=make(3,4);let held=first.next();let other=second.next();match held{{Option.Some(a)=>{{match other{{Option.Some(b)=>{{return *a+*b-23}},Option.None()=>{{return 1}}}}}},Option.None()=>{{return 2}}}}}}"
    )
}

fn baseline() -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "trait-lending-verify.zil", source());
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    (module, plan)
}

fn verify_raw_and_wire_reject(module: TypedModule, mut plan: CandidatePlan, expected: &str) {
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    for errors in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default())
            .expect_err("raw Verify accepted forged actual call receiver"),
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default())
            .expect_err("Wire Verify accepted forged actual call receiver"),
    ] {
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-VERIFY-STORED"
                && d.message.contains(expected)),
            "{errors:?}"
        );
    }
}

#[test]
fn actual_receiver_is_checked_while_the_first_option_is_live() {
    let (mut module, plan) = baseline();
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let functions = &module.functions;
    let calls: Vec<(usize, usize, ValueId)> = module.functions[main]
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(block, body)| {
            body.instructions
                .iter()
                .enumerate()
                .filter_map(move |(index, i)| {
                    let InstructionKind::Call {
                        function,
                        arguments,
                    } = &i.kind
                    else {
                        return None;
                    };
                    let callee = &functions[function.0 as usize];
                    (arguments.len() == 1
                        && callee.storage_life.is_some()
                        && callee.name.contains("next"))
                    .then_some((block, index, arguments[0]))
                })
        })
        .collect();
    assert_eq!(
        calls.len(),
        2,
        "source must have two actual next calls; functions={:?}; main_calls={:?}",
        module
            .functions
            .iter()
            .map(|f| (&f.name, f.parameters.len(), f.storage_life.is_some()))
            .collect::<Vec<_>>(),
        module.functions[main]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter_map(|i| match &i.kind {
                InstructionKind::Call {
                    function,
                    arguments,
                } => Some((function.0, arguments.len())),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
    let (block, instruction, _) = calls[1];
    let InstructionKind::Call { arguments, .. } =
        &mut module.functions[main].blocks[block].instructions[instruction].kind
    else {
        unreachable!()
    };
    arguments[0] = calls[0].2;
    verify_raw_and_wire_reject(module, plan, "live incompatible loan");
}
