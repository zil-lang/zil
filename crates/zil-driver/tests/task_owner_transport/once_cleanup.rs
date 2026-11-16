use super::*;

#[test]
fn consuming_call_requires_closed_remaining_captures_before_delivering_its_result() {
    // Keep the actual FnMut opaque interface. A declared FnOnce input selects
    // consuming adaptation without relabelling the factory's returned kind.
    let blocking = format!("fn consume<F:FnOnce()->i32>(callback:F)->i32{{return callback();}}\n{}",
        include_str!("../../../../tests/language-readiness/task-owner-transport/fnmut-async-close.zil")
            .replace("\n callback();", "\n consume(callback);"));
    // As in the original consuming negative, invoke the polling body only once:
    // its Pending result cannot be delivered after dropping a still-live Task.
    let polling = format!("fn consume<F:FnOnce()->std.task.Poll<()>>(callback:F)->std.task.Poll<()>{{return callback();}}\n{}",
        include_str!("../../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil")
            .replace("loop{match callback(){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}", "consume(callback);"));
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
    assert!(rejected.diagnostics.iter().any(|d| d.code == "E-TASK-ABI" || d.code == "E-VERIFY-TASK"
        || (d.phase == zil_base::Phase::Types && d.code == "E-ORIGINAL-CLEANUP"
            && d.message.contains("live original Task default Drop cleanup")
            && d.message.contains("Closed fact")
            && d.notes.iter().any(|note| note.contains("original consuming entry remaining capture")))),
        "{}", rejected.to_json());
}

#[test]
fn opaque_return_declaration_cannot_relabel_a_reusable_environment() {
    // Preserve the former positive's complete source transformation as a
    // precise negative: the body still retains its Task in a FnMut environment.
    let original = include_str!("../../../../tests/language-readiness/task-owner-transport/fnmut-async-close.zil")
        .replace("impl FnMut()->i32", "impl FnOnce()->i32");
    let report = compile_source(&source(&original), &CompileOptions {
        stop_after: Stage::Types, ..CompileOptions::default()
    });
    assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
    assert!(report.diagnostics.iter().any(|diagnostic|
        diagnostic.phase == zil_base::Phase::Types
            && diagnostic.code == "E-TYPE-MISMATCH"
            && diagnostic.message == "opaque callable declaration differs from its actual environment invocation kind"
    ), "{}", report.to_json());
}
