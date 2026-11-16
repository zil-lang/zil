use super::*;

// These are fixed original bytes, not new weakened approximations of the
// positive controls. Unsupported or unrelated failures do not establish the
// independently reviewed language error. Each runs separately so one failing
// check cannot hide the remaining source contracts.
fn rejects(text: &str, expected: &str) {
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
                && diagnostic.message.contains(expected)),
        "{}",
        report.to_json()
    );
}

#[test]
fn direct_generic_drop_requires_cleanup() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/direct-worker-owner-cleanup.zil"
        ),
        "SyncDrop",
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
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/nested-once-callable-child-cleanup.zil"
        ),
        "SyncDrop",
    );
}

#[test]
fn scalar_body_cannot_claim_once_kind() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/once-pending-callable-cleanup.zil"
        ),
        "opaque callable declaration differs",
    );
}

#[test]
fn packet_body_cannot_claim_once_kind() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/once-pending-owned-result-cleanup.zil"
        ),
        "opaque callable declaration differs",
    );
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
fn unused_child_helper_cannot_claim_once_kind() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/direct-child-cleanup.zil"
        ),
        "opaque callable declaration differs",
    );
}
