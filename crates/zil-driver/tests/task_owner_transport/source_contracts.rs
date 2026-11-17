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
    // The complete source also has a Direct parent that drops an incoming
    // Task without Closed or a known synchronous graph. Either independent
    // obligation can be visited first; no other rejection establishes it.
    rejects_matching(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/direct-child-cleanup.zil"
        ),
        |diagnostic| {
            (diagnostic.code == "E-TYPE-MISMATCH"
                && diagnostic.message
                    == "opaque callable declaration differs from its actual environment invocation kind")
                || (diagnostic.code == "E-ORIGINAL-CLEANUP"
                    && diagnostic.message
                        == "live original Task default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root")
        },
    );
}

#[test]
fn unused_incoming_task_requires_closed_or_synchronous_graph() {
    rejects(
        include_str!(
            "../../../../tests/language-readiness/task-owner-transport/invalid-source-contracts/mutable-frame-never-called.zil"
        ),
        "default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root",
    );
}
