//! Non-author raw/Wire attacks. Neither kernel receives source admission as a
//! proof; each mutation starts from a real typed and independently valid body.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;

fn module() -> TypedModule {
    let source = "resource R{n:i32}package Event{manages r:R;n:i32}fn main(){let f=(first:Event,second:Event)->Event=>{return first};let result=f(Event{r:R{n:1},n:2},Event{r:R{n:3},n:4})}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review248.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn rejected(result: Result<impl Sized, Vec<zil_base::Diagnostic>>, label: &str) {
    let diagnostics = match result {
        Ok(_) => panic!("{label}: forged ownership accepted"),
        Err(d) => d,
    };
    assert!(
        diagnostics
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
        "{label}: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| !d.code.starts_with("E-VERIFY-PLAN")),
        "{label}: only claimed-plan mismatch: {diagnostics:#?}"
    );
}

#[test]
fn both_kernels_reject_duplicate_owning_ssa_and_raw_call_signature_forgery() {
    let original = module();
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &original.target, &limits).unwrap();
    let control = wire::encode_module_candidate(&original, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&control, &original.target, &limits).unwrap();
    for attack in [
        "duplicate-arg",
        "missing-arg",
        "unused-signature",
        "forged-result",
    ] {
        let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("review248-{attack}-{}", std::process::id()));
        fs::create_dir_all(&out).unwrap();
        let mut bad = original.clone();
        if attack == "unused-signature" {
            bad.closures[0].parameters[0] = Type::I32;
        } else {
            let function = bad.functions.iter_mut().find(|f| f.name == "main").unwrap();
            let call = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::CallClosure { .. }
                            | InstructionKind::CallClosureShared { .. }
                            | InstructionKind::CallClosureMut { .. }
                            | InstructionKind::CallClosureOnce { .. }
                            | InstructionKind::CallClosureBorrowed { .. }
                    )
                })
                .unwrap();
            if attack == "forged-result" {
                call.kind = InstructionKind::Constant(Constant::I32(0));
            } else {
                let arguments = match &mut call.kind {
                    InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. } => arguments,
                    _ => unreachable!(),
                };
                assert_eq!(arguments.len(), 2);
                if attack == "duplicate-arg" {
                    arguments[1] = arguments[0]
                } else {
                    arguments.pop();
                }
            }
        }
        fs::write(out.join("typed.txt"), format!("{bad:#?}")).unwrap();
        let analyzed = zil_analyze::analyze(&bad, &limits);
        fs::write(out.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
        rejected(analyzed, attack);
        let mut candidate = plan.clone();
        candidate.module_fingerprint = bad.fingerprint();
        let encoded = wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
        fs::write(out.join("candidate.wire"), &encoded).unwrap();
        let raw = zil_verify::verify_module(bad.clone(), candidate, &bad.target, &limits);
        let bytes = zil_verify::verify_module_bytes(&encoded, &bad.target, &limits);
        fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
        fs::write(out.join("wire.txt"), format!("{bytes:#?}")).unwrap();
        rejected(raw, &format!("{attack}/raw"));
        rejected(bytes, &format!("{attack}/wire"));
    }
}
