use std::{fs, path::Path};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{InstructionKind, Type};

fn main() {
    let root = Path::new("/workspace/scratch/39be3faa4442/run-logs/review163/raw");
    let text = "resource R{n:i32}impl R{fn read(self:Self){};fn finish(give self:Self){}}fn main(){let r=R{n:42};r.read();(give r).finish()}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "review163-raw.zil", text);
    fs::write(root.join("source.zil"), text).unwrap();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
    fs::write(root.join("original-typed.txt"), format!("{original:#?}\n")).unwrap();
    let owned = original.functions.iter().find(|function| {
        function.parameters.first().is_some_and(|parameter| matches!(function.locals[parameter.0 as usize].ty, Type::Managed(_)))
    }).unwrap().id;
    let borrowed = original.functions.iter().find(|function| function.name.ends_with(".read")).unwrap().id;
    for mutation in ["duplicate-owned-method-call", "descriptor-fed-to-owned-method"] {
        let mut candidate = original.clone();
        let mut changed = 0;
        for function in &mut candidate.functions {
            for block in &mut function.blocks {
                if mutation == "duplicate-owned-method-call" {
                    if let Some(index) = block.instructions.iter().position(|instruction| matches!(&instruction.kind, InstructionKind::Call {function, ..} if *function == owned)) {
                        assert!(block.instructions[index].result.is_none());
                        block.instructions.insert(index + 1, block.instructions[index].clone());
                        changed += 1;
                    }
                } else {
                    for instruction in &mut block.instructions {
                        if let InstructionKind::Call { function, .. } = &mut instruction.kind
                            && *function == borrowed
                        {
                            *function = owned;
                            changed += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(changed, 1, "each mutation must change exactly its intended source call");
        let mut proof = plan.clone();
        proof.module_fingerprint = candidate.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&candidate, &proof, &limits).unwrap();
        fs::write(root.join(format!("{mutation}.bin")), &wire).unwrap();
        for (route, result) in [
            ("raw", zil_verify::verify_module(candidate, proof, &target, &limits)),
            ("wire", zil_verify::verify_module_bytes(&wire, &target, &limits)),
        ] {
            let errors = result.unwrap_err();
            let (code, message) = if mutation == "duplicate-owned-method-call" {
                ("E-VERIFY-RESOURCE", "managed SSA value was not available exactly once")
            } else {
                ("E-VERIFY-CALL", "call argument does not match the target parameter type")
            };
            assert!(errors.iter().any(|error| error.phase == Phase::Verify && error.category == ResultCategory::SafeReject && error.code == code && error.message == message), "{mutation}/{route}: {errors:?}");
            fs::write(root.join(format!("{mutation}-{route}.txt")), format!("{errors:#?}\n")).unwrap();
            println!("{mutation}/{route}: {errors:?}");
        }
    }
}
