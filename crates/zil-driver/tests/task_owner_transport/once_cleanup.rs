use super::*;

#[test]
fn consuming_call_requires_closed_remaining_captures_before_delivering_its_result() {
    let blocking = include_str!("../../../../tests/language-readiness/task-owner-transport/fnmut-async-close.zil")
        .replace("impl FnMut()->i32", "impl FnOnce()->i32");
    let polling = include_str!("../../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil")
        .replace("impl FnMut()->std.task.Poll<()>", "impl FnOnce()->std.task.Poll<()>")
        .replace("loop{match callback(){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}", "callback();");
    let options = CompileOptions { stop_after: Stage::Verify, ..CompileOptions::default() };
    let accepted = compile_source(&source(&blocking), &options);
    assert_eq!(accepted.status, ResultCategory::Success, "{}", accepted.to_json());
    assert_eq!(accepted.verification_runs, 1);
    let typed = typed_original(&blocking);
    assert!(typed.functions.iter().flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .any(|instruction| matches!(instruction.kind, zil_ir::InstructionKind::CallClosureOnce{..})));
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let claim = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), claim.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &claim, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let rejected = compile_source(&source(&polling), &options);
    assert_ne!(rejected.status, ResultCategory::Success, "{}", rejected.to_json());
    assert!(rejected.diagnostics.iter().any(|d| d.code == "E-TASK-ABI" || d.code == "E-VERIFY-TASK"),
        "{}", rejected.to_json());
}
