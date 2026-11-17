use super::*;

// These are fixed original bytes, not new weakened approximations of the
// positive controls. Unsupported or unrelated failures do not establish the
// independently reviewed language error. Each runs separately so one failing
// check cannot hide the remaining source contracts.
fn rejects(text: &str, expected: &str) {
    rejects_matching(text, |diagnostic| diagnostic.message.contains(expected));
}

fn rejects_matching(text: &str, expected: impl Fn(&zil_base::Diagnostic) -> bool) {
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Types,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.phase == zil_base::Phase::Types
                && diagnostic.category == ResultCategory::SafeReject
                && expected(diagnostic)),
        "{}",
        report.to_json()
    );
}

#[test]
fn direct_generic_drop_requires_cleanup() {
    rejects_consuming_contract_and_unused_cleanup(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/direct-worker-owner-cleanup.zil"
        ),
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/unused-direct-generic-drop.zil"
        ),
    );
}

#[test]
fn nonconsuming_recovery_requires_cleanup() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/nested-callable-child-cleanup.zil"
        ),
        "SyncDrop",
    );
}

#[test]
fn nonconsuming_wrapper_requires_cleanup() {
    rejects_consuming_contract_and_unused_cleanup(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/nested-once-callable-child-cleanup.zil"
        ),
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/unused-nonconsuming-wrapper.zil"
        ),
    );
}

fn rejects_consuming_contract_and_unused_cleanup(original: &str, unused: &str) {
    // The original caller also passes a Suspend consuming operation to a
    // Direct bound. Either that real contract violation or the definition's
    // unconsumed cleanup obligation may be reported first.
    rejects_matching(original, |diagnostic| {
        (diagnostic.code == "E-ORIGINAL-CLEANUP"
            && diagnostic.message == "live original value has no independent SyncDrop proof")
            || (diagnostic.code == "E-SEMANTIC-ORIGINAL"
                && diagnostic.message
                    == "original callable changes its complete scoped call scheme"
                && diagnostic.notes.iter().any(|note| note ==
                    "original callable scheme: actual P=0 Bound=0 Invocation=0 ABI=Suspend resultMode=Infer; wanted P=0 Bound=0 Invocation=0 ABI=Direct resultMode=Infer"))
    });
    // Preserve every original definition byte-for-byte, but remove the
    // instantiating caller. Its mismatch must not mask missing universal
    // cleanup checking; Unsupported or another signature error cannot pass.
    let main = original.rfind("fn main()").expect("original final main");
    assert_eq!(
        unused,
        format!("{}fn main()->i32 {{return 0;}}\n", &original[..main])
    );
    rejects(unused, "SyncDrop");
}

fn verifies_original(text: &str) {
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Verify,
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
}

pub(super) fn consuming_operation_requires_suspend(diagnostic: &zil_base::Diagnostic) -> bool {
    diagnostic.phase == zil_base::Phase::Types
        && diagnostic.category == ResultCategory::SafeReject
        && diagnostic.code == "E-SEMANTIC-ORIGINAL"
        && diagnostic.message == "original callable changes its complete scoped call scheme"
        && diagnostic.notes.iter().any(|note| note ==
            "original callable scheme: actual P=0 Bound=0 Invocation=0 ABI=Suspend resultMode=Infer; wanted P=0 Bound=0 Invocation=0 ABI=Direct resultMode=Infer")
}

fn inferred_once_requires_a_real_suspend_consumer(original: &str) {
    // These original bytes were previously rejected by a natural-FnMut-only
    // whitelist. The independently Suspend parent can infer its omitted
    // FnOnce operation upper before publishing the original generic contract.
    verifies_original(original);
    let header = "(callback:F)->i32 {\n task_yield();let answer=callback();";
    assert_eq!(original.matches(header).count(), 1);
    // Remove the independent yield and explicitly require Direct, leaving
    // the complete callback operation and its pending Task cleanup intact.
    let direct = original.replacen(
        header,
        "(callback:F)->i32 contract{effects(io);}{\n let answer=callback();",
        1,
    );
    rejects_matching(&direct, consuming_operation_requires_suspend);
}

#[test]
fn scalar_once_infers_complete_operation_but_preserves_direct_consumers() {
    inferred_once_requires_a_real_suspend_consumer(include_str!(
        "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/once-pending-callable-cleanup.zil"
    ));
}

#[test]
fn packet_once_infers_complete_operation_but_preserves_direct_consumers() {
    inferred_once_requires_a_real_suspend_consumer(include_str!(
        "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/once-pending-owned-result-cleanup.zil"
    ));
}

#[test]
fn distinct_task_classes_cannot_be_equal() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/equal-distinct-task-classes.zil"
        ),
        "incompatible rigid actuals",
    );
}

#[test]
fn distinct_returned_task_classes_cannot_be_equal() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/returned-distinct-task-classes.zil"
        ),
        "incompatible rigid actuals",
    );
}

#[test]
fn unused_once_factory_and_known_task_cleanup_infer_without_kind_restrictions() {
    let original = include_str!(
        "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/direct-child-cleanup.zil"
    );
    verifies_original(original);
    let header = "fn parent(give callback:Task<i32>)->i32 {";
    assert_eq!(original.matches(header).count(), 1);
    let direct = original.replacen(
        header,
        "fn parent(give callback:Task<i32>)->i32 contract{effects(io);}{",
        1,
    );
    rejects(
        &direct,
        "default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root",
    );
}

#[test]
fn explicitly_direct_unused_incoming_task_requires_closed_or_synchronous_graph() {
    let original = include_str!(
        "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/mutable-frame-never-called.zil"
    );
    let direct = include_str!(
        "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/explicit-direct-mutable-frame-cleanup.zil"
    );
    verifies_original(original);
    // The original bytes now also run in the full native helper matrix.
    // Only the declared execution upper differs in this negative identity.
    assert_eq!(direct, original.replacen(
        "fn drive_callback<life a>(give original:Task<i32,&'a mut i32>)->i32 {",
        "fn drive_callback<life a>(give original:Task<i32,&'a mut i32>)->i32 contract{effects(io);}{",
        1,
    ));
    rejects(
        direct,
        "default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root",
    );
}
