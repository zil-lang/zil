//! Independent Source and raw/Wire controls for the ordinary mutable helper.
use super::*;
use zil_ir::{CandidatePlan, PassingMode, TypedModule};

const REBORROW: &str = include_str!("whole-helper/g137_review_reborrow.zil");
const INCOMING_READ: &str = include_str!("whole-helper/g137_review_incoming_reborrow.zil");
const FRESH: &str = include_str!("whole-helper/g137_review_fresh_create.zil");
const INCOMING: &str = include_str!("whole-helper/g137_review_incoming_give.zil");
const CONDITIONAL: &str = include_str!("whole-helper/g137_review_conditional_give.zil");
const OLD: &str = include_str!("whole-helper/g137_review_old_desc.zil");
const APART: &str = include_str!("whole-helper/g137_review_apart_desc.zil");
const SUCCESS: &str = include_str!("whole-helper/g137_review_success.zil");
const ARGUMENT_FAILURE: &str = include_str!("whole-helper/g137_review_argument_failure.zil");
const WRONG_ACTUAL: &str = include_str!("whole-helper/g137_review_wrong_actual_give.zil");
const LOCAL_FAILURE: &str = include_str!("replace-package-terminal-failure.zil");
const FAILURE: &str = include_str!("whole-helper/g137_review_failure.zil");

fn typed(text: &str) -> TypedModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

fn candidate(text: &str) -> (TypedModule, CandidatePlan) {
    let module = typed(text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    verify(&module, &plan, None);
    (module, plan)
}

fn verify(module: &TypedModule, plan: &CandidatePlan, error: Option<&str>) {
    let limits = Limits::default();
    let mut claim = plan.clone();
    claim.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(module, &claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), claim, &module.target, &limits),
        zil_verify::verify_module_bytes(&bytes, &module.target, &limits),
    ] {
        if let Some(code) = error {
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|d| d.code == code), "{errors:?}");
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn g137_review_types_has_actual_parameter_ticket_and_complete_rhs() {
    for text in [
        REBORROW,
        INCOMING_READ,
        FRESH,
        INCOMING,
        CONDITIONAL,
        OLD,
        APART,
        SUCCESS,
        FAILURE,
        ARGUMENT_FAILURE,
        WRONG_ACTUAL,
    ] {
        let module = typed(text);
        let f = module.functions.iter().find(|f| f.name == "reset").unwrap();
        let instructions: Vec<_> = f.blocks.iter().flat_map(|b| &b.instructions).collect();
        let (reservation, address) = instructions
            .iter()
            .find_map(|i| match &i.kind {
                InstructionKind::ReservePlaceAddress {
                    reservation,
                    address,
                    path,
                } if path.is_empty() => Some((*reservation, *address)),
                _ => None,
            })
            .expect("actual complete-address reserve");
        assert!(
            matches!(f.values[address.0 as usize], Type::Address { mutable: true, ref inner } if matches!(inner.as_ref(), Type::Package { .. }))
        );
        assert!(
            instructions.iter().any(
                |i| matches!(i.kind, InstructionKind::Load(local) if local == f.parameters[0])
                    && i.result == Some(address)
            ),
            "reserve must use actual mutable parameter"
        );
        assert!(instructions.iter().any(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { reservation: r, value } if r == reservation && matches!(f.values[value.0 as usize], Type::Package { .. }))));
        assert!(
            !instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::WriteAddress { .. }))
        );
        if text == FAILURE {
            assert!(instructions.iter().any(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { reservation: r } if r == reservation)));
        }
    }
}

#[test]
fn g137_review_source_continuation_actual_origins_and_apart() {
    for text in [
        REBORROW,
        INCOMING_READ,
        FRESH,
        INCOMING,
        CONDITIONAL,
        APART,
        SUCCESS,
        ARGUMENT_FAILURE,
        WRONG_ACTUAL,
    ] {
        candidate(text);
    }
    let report = compile_source(
        &source(FAILURE),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze);
    for text in [
        OLD.to_owned(),
        INCOMING.replace("result(give)", "result(create)"),
        CONDITIONAL.replace("result(give)", "result(create)"),
        WRONG_ACTUAL.replace("result(give)", "result(create)"),
    ] {
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    }
}

#[test]
fn g137_review_verify_incoming_and_no_write_do_not_grant_create() {
    for (text, name) in [
        (INCOMING, "incoming"),
        (CONDITIONAL, "conditional"),
        (WRONG_ACTUAL, "wrong"),
    ] {
        let (mut module, plan) = candidate(text);
        module
            .functions
            .iter_mut()
            .find(|f| f.name == name)
            .unwrap()
            .result_mode = PassingMode::Create;
        // Invoke Verify directly on both interfaces, regardless of Analyze's
        // response to this forged return contract.
        verify(&module, &plan, Some("E-VERIFY-RESOURCE"));
    }
}

#[test]
fn g137_review_verify_old_description_after_actual_helper_retirement() {
    let module = typed(OLD);
    let mut before = module.clone();
    let f = before
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
        })
        .unwrap();
    let read = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
        .unwrap();
    let InstructionKind::ReadDesc { desc, .. } = block.instructions[read].kind else {
        unreachable!()
    };
    let start = block
        .instructions
        .iter()
        .position(|i| i.result == Some(desc))
        .unwrap();
    let moved: Vec<_> = block.instructions.drain(start..=read).collect();
    let borrow = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { .. }))
        .unwrap();
    block.instructions.splice(borrow..borrow, moved);
    // The honest candidate reads the same descriptor before retirement. The
    // attacked module retains the original Source's complete CFG and tickets.
    let plan = zil_analyze::analyze(&before, &Limits::default()).unwrap();
    verify(&before, &plan, None);
    verify(&module, &plan, Some("E-VERIFY-RESOURCE"));
}

#[test]
fn g137_review_verify_ticket_settlement_and_old_owner_obligations() {
    let (module, plan) = candidate(REBORROW);
    let mut wrong_ticket = module.clone();
    let f = wrong_ticket
        .functions
        .iter_mut()
        .find(|f| f.name == "reset")
        .unwrap();
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::CommitPlaceReplace { reservation, .. } = &mut i.kind {
            reservation.0 += 1;
        }
    }
    verify(&wrong_ticket, &plan, Some("E-VERIFY-PLACE-REPLACE"));
    let mut missing_owner = module;
    let f = missing_owner
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let before = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.kind, InstructionKind::DropPackage { .. }))
        .count();
    assert_eq!(before, 1);
    for block in &mut f.blocks {
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
    }
    verify(&missing_owner, &plan, Some("E-VERIFY-RESOURCE"));

    let (mut module, plan) = candidate(LOCAL_FAILURE);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "work")
        .unwrap();
    let before = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }))
        .count();
    assert_eq!(before, 1);
    for block in &mut f.blocks {
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }));
    }
    verify(&module, &plan, Some("E-VERIFY-PLACE-REPLACE"));
}

#[test]
fn g137_review_native_complete_result_and_failed_rhs_old_value() {
    use zil_driver::{NativeBackend, Optimization};
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, expected) in [
                ("success", SUCCESS, b"AB42\n".as_slice()),
                ("argument-failure", ARGUMENT_FAILURE, b"BA42\n".as_slice()),
            ] {
                // Both independent kernels run before any executable is made.
                candidate(text);
                let directory = Temp::new();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(directory.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{name}: {output:?}");
                assert_eq!(
                    output.stdout, expected,
                    "{name}/{backend:?}/{optimization:?}"
                );
                assert!(output.stderr.is_empty());
            }
        }
    }
}
