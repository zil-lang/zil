use super::*;
use zil_ir::*;

const SPLIT: &str = include_str!("branch-before-two-calls.zil");
const DIAMOND: &str = include_str!("diamond-transfer-then-call.zil");
const OPTIONAL: &str = include_str!("drop-without-call-one-path.zil");

fn rejected(errors: &[zil_base::Diagnostic], label: &str) {
    assert!(!errors.is_empty(), "{label}");
    assert!(
        errors.iter().all(|d| matches!(
            d.category,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        )),
        "{label}: {errors:?}"
    );
}

#[test]
fn p0_g338_independent_source_and_raw_wire_path_mutations() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for text in [SPLIT, DIAMOND, OPTIONAL] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        if text != DIAMOND {
            continue;
        }
        let helper = raw
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        let (call_block, call_index, callee) = helper
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block.instructions.iter().enumerate().find_map(|(i, op)| {
                    if let InstructionKind::CallClosureOnce { callee, .. } = op.kind {
                        Some((b, i, callee))
                    } else {
                        None
                    }
                })
            })
            .unwrap();
        let (take_block, take_index, stage) = helper
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block.instructions.iter().enumerate().find_map(|(i, op)| {
                    if op.result == Some(callee)
                        && let InstructionKind::TakeLocal { local } = op.kind
                    {
                        return Some((b, i, local));
                    }
                    None
                })
            })
            .unwrap();
        assert_eq!(take_block, call_block);
        assert!(take_index < call_index);
        assert!(!helper.parameters.contains(&stage));
        let predecessor = helper.blocks.iter().position(|block| {
            matches!(block.terminator, Terminator::Jump(next) if next.0 as usize == call_block)
        }).expect("diamond has an actual predecessor of the consuming merge");
        let crossing_owner = helper.blocks[helper.entry.0 as usize]
            .instructions
            .iter()
            .rev()
            .find_map(|i| {
                if let InstructionKind::Store { local, .. } = i.kind
                    && matches!(helper.locals[local.0 as usize].ty, Type::Closure(_))
                {
                    return Some(local);
                }
                None
            })
            .expect("a real owning local crosses the diamond");
        for name in [
            "missing-live",
            "late-live",
            "one-predecessor-drop",
            "copied-load",
            "unreachable-island",
            "backedge",
        ] {
            let mut candidate = raw.clone();
            let f = &mut candidate.functions[helper.id.0 as usize];
            match name {
                "missing-live" | "late-live" => {
                    let mut removed = None;
                    for block in &mut f.blocks {
                        if let Some(index) = block.instructions.iter().position(|i| matches!(i.kind, InstructionKind::StorageLive(local) if local == stage)) {
                            assert!(removed.is_none());
                            removed = Some(block.instructions.remove(index));
                        }
                    }
                    let live = removed.expect("actual stage has StorageLive");
                    if name == "late-live" {
                        let block = f.blocks.iter_mut().find(|block| block.instructions.iter().any(|i| matches!(i.kind, InstructionKind::Store { local, .. } if local == stage))).unwrap();
                        let index = block.instructions.iter().position(|i| matches!(i.kind, InstructionKind::Store { local, .. } if local == stage)).unwrap();
                        block.instructions.insert(index + 1, live);
                    }
                }
                "one-predecessor-drop" => {
                    f.blocks[predecessor].instructions.push(Instruction {
                        result: None,
                        kind: InstructionKind::DropClosure {
                            local: crossing_owner,
                        },
                        span: zil_base::Span::default(),
                    });
                }
                "copied-load" => {
                    f.blocks[take_block].instructions[take_index].kind =
                        InstructionKind::Load(stage)
                }
                "unreachable-island" => f.blocks.push(BasicBlock {
                    id: BlockId(f.blocks.len() as u32),
                    instructions: vec![],
                    terminator: Terminator::Trap,
                }),
                "backedge" => f.blocks[predecessor].terminator = Terminator::Jump(f.entry),
                _ => unreachable!(),
            }
            rejected(
                &zil_analyze::analyze(&candidate, &limits).unwrap_err(),
                name,
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = candidate.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&candidate, &claim, &limits).unwrap();
            for wire in [false, true] {
                let errors = if wire {
                    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
                } else {
                    zil_verify::verify_module(candidate.clone(), claim.clone(), &target, &limits)
                        .unwrap_err()
                };
                rejected(&errors, name);
                eprintln!(
                    "P0 G338 independent raw {name} wire={wire}: {}",
                    errors[0].message
                );
            }
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn p0_g338_independent_dynamic_source_complete_values() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for (name, text, expected) in [
        ("split", SPLIT, "101\n"),
        ("diamond", DIAMOND, "103\n"),
        ("optional", OPTIONAL, "51\n"),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-p0-g338-{}-{name}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    entry: Some("main".into()),
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("ir")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let run = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(run.status.success(), "{name}/{optimization:?}: {run:?}");
            assert_eq!(run.stdout, expected.as_bytes());
            assert!(run.stderr.is_empty());
            eprintln!(
                "P0 G338 {name}/{backend:?}/{optimization:?}: stdout {expected:?}; Source {}",
                report.input_fingerprint
            );
        }
    }
}
