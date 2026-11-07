//! Independent non-Unit helper Source, actual cleanup and raw/Wire controls.
use super::*;
use zil_ir::{CandidatePlan, PassingMode, TypedModule};

const SCALAR: &str = include_str!("whole-helper/fallible-replacement-cleanup/scalar-success.zil");
const ORIGINAL_FAILURE: &str =
    include_str!("whole-helper/fallible-replacement-cleanup/original-failure.zil");
const SUCCESS: &str = include_str!("whole-helper/fallible-replacement-cleanup/branch-success.zil");
const FAILURE: &str = include_str!("whole-helper/fallible-replacement-cleanup/branch-failure.zil");
const COMMIT_ERROR: &str =
    include_str!("whole-helper/fallible-replacement-cleanup/commit-then-error.zil");
const OLD: &str = include_str!("whole-helper/fallible-replacement-cleanup/old-desc-error.zil");
const NO_COMMIT: &str =
    include_str!("whole-helper/fallible-replacement-cleanup/no-commit-old-desc.zil");
const FRESH_GIVE: &str =
    include_str!("whole-helper/fallible-replacement-cleanup/scalar-fresh-give.zil");
const INCOMING: &str = include_str!("whole-helper/fallible-replacement-cleanup/incoming-give.zil");

fn typed(text: &str) -> TypedModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

fn verify(module: &TypedModule, plan: &CandidatePlan, rejection: Option<&str>) {
    let limits = Limits::default();
    let mut claim = plan.clone();
    claim.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(module, &claim, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), claim, &module.target, &limits),
        zil_verify::verify_module_bytes(&bytes, &module.target, &limits),
    ] {
        if let Some(code) = rejection {
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|d| d.code == code), "{errors:?}");
        } else {
            result.unwrap();
        }
    }
}

fn candidate(text: &str) -> (TypedModule, CandidatePlan) {
    let module = typed(text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    verify(&module, &plan, None);
    (module, plan)
}

#[test]
fn types_actual_nonunit_ticket_rhs_and_abort() {
    for text in [
        SCALAR,
        ORIGINAL_FAILURE,
        SUCCESS,
        FAILURE,
        COMMIT_ERROR,
        OLD,
        FRESH_GIVE,
        INCOMING,
    ] {
        let module = typed(text);
        let f = module.functions.iter().find(|f| f.name == "reset").unwrap();
        assert_ne!(f.return_type, Type::Unit);
        let instructions: Vec<_> = f.blocks.iter().flat_map(|b| &b.instructions).collect();
        let (ticket, address) = instructions
            .iter()
            .find_map(|i| match &i.kind {
                InstructionKind::ReservePlaceAddress {
                    reservation,
                    address,
                    path,
                } if path.is_empty() => Some((*reservation, *address)),
                _ => None,
            })
            .expect("actual complete address reserve");
        assert!(instructions.iter().any(
            |i| matches!(i.kind, InstructionKind::Load(local) if local == f.parameters[0])
                && i.result == Some(address)
        ));
        assert!(instructions.iter().any(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { reservation, value } if reservation == ticket && matches!(f.values[value.0 as usize], Type::Package { .. }))));
        assert!(
            !instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::WriteAddress { .. }))
        );
        if [ORIGINAL_FAILURE, SUCCESS, FAILURE].contains(&text) {
            assert!(instructions.iter().any(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { reservation } if reservation == ticket)));
        }
    }
}

#[test]
fn source_returns_abort_continuation_and_conservative_fresh() {
    for text in [
        SCALAR,
        ORIGINAL_FAILURE,
        SUCCESS,
        FAILURE,
        COMMIT_ERROR,
        NO_COMMIT,
        FRESH_GIVE,
        INCOMING,
    ] {
        candidate(text);
    }
    for text in [
        OLD.to_owned(),
        FRESH_GIVE.replace("result(give)", "result(create)"),
        INCOMING.replace("result(give)", "result(create)"),
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
fn verify_actual_failure_settlement_and_prepared_value_cleanup() {
    let (module, plan) = candidate(FAILURE);
    for mutation in 0..5 {
        let mut raw = module.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "reset")
            .unwrap();
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }))
            })
            .unwrap();
        let abort = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }))
            .unwrap();
        let code = match mutation {
            0 => {
                block.instructions.remove(abort);
                "E-VERIFY-PLACE-REPLACE"
            }
            1 => {
                let InstructionKind::AbortPlaceReservation { reservation } =
                    &mut block.instructions[abort].kind
                else {
                    unreachable!()
                };
                reservation.0 += 1;
                "E-VERIFY-PLACE-REPLACE"
            }
            2 | 3 => {
                let drop = block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                    .unwrap();
                if mutation == 2 {
                    block.instructions.remove(drop);
                } else {
                    block
                        .instructions
                        .insert(drop, block.instructions[drop].clone());
                }
                "E-VERIFY-RESOURCE"
            }
            4 => {
                let f = raw.functions.iter_mut().find(|f| f.name == "work").unwrap();
                let block = f
                    .blocks
                    .iter_mut()
                    .find(|b| {
                        b.instructions
                            .iter()
                            .any(|i| matches!(i.kind, InstructionKind::DropPackage { .. }))
                    })
                    .unwrap();
                let drop = block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::DropPackage { .. }))
                    .unwrap();
                block
                    .instructions
                    .insert(drop, block.instructions[drop].clone());
                "E-VERIFY-RESOURCE"
            }
            _ => unreachable!(),
        };
        // Direct Verify invocations establish these rejections; Analyze is not
        // consulted for any attacked candidate.
        verify(&raw, &plan, Some(code));
    }
}

#[test]
fn verify_nonunit_may_and_incoming_are_not_fresh() {
    for (text, function) in [(FRESH_GIVE, "fresh"), (INCOMING, "incoming")] {
        let (mut raw, plan) = candidate(text);
        raw.functions
            .iter_mut()
            .find(|f| f.name == function)
            .unwrap()
            .result_mode = PassingMode::Create;
        verify(&raw, &plan, Some("E-VERIFY-RESOURCE"));
    }
}

#[test]
fn verify_error_result_does_not_restore_old_description() {
    let module = typed(OLD);
    let mut before = module.clone();
    let f = before
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let (which, read) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                .map(|i| (b, i))
        })
        .unwrap();
    let InstructionKind::ReadDesc { desc, .. } = f.blocks[which].instructions[read].kind else {
        unreachable!()
    };
    let start = f.blocks[which]
        .instructions
        .iter()
        .position(|i| i.result == Some(desc))
        .unwrap();
    let moved: Vec<_> = f.blocks[which].instructions.drain(start..=read).collect();
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { .. }))
        })
        .unwrap();
    let borrow = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { .. }))
        .unwrap();
    block.instructions.splice(borrow..borrow, moved);
    let plan = zil_analyze::analyze(&before, &Limits::default()).unwrap();
    verify(&before, &plan, None);
    verify(&module, &plan, Some("E-VERIFY-RESOURCE"));
}

#[test]
fn native_full_results_error_side_effects_and_cleanup() {
    use zil_driver::{NativeBackend, Optimization};
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, expected) in [
                ("scalar", SCALAR, b"AB42\n".as_slice()),
                ("original-failure", ORIGINAL_FAILURE, b"BA42\n".as_slice()),
                ("success", SUCCESS, b"BAC42\n".as_slice()),
                ("failure", FAILURE, b"CBA42\n".as_slice()),
                ("commit-error", COMMIT_ERROR, b"AB42\n".as_slice()),
            ] {
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
