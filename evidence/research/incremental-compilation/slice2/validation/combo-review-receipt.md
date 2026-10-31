# Independent review, large-project owner release

Reviewer: `/root/review_incremental_slice1`, independent of the implementer.

**Verdict: Pass, limited functional/rejection-boundary scope.** Exact commit: `646099a70105b6da3821ce7d8ea20bd8e28fe3e1`; direct parent: `d66a3142099209b7c5a7884b46e46418fa60021e`. No unresolved correctness finding.

This extends the independent review of `b230cfddcc12c3daf6ec4dcbca3a2dc11d9ebe79`. Original receipts are preserved as `combo-review-receipt-b230.md` and `.json`; the combined JSON embeds that prior receipt. The prior CP33/CP34/CP35/LirView/LLVM-DAG/Expand coverage applies to unchanged code. The new history policy is reviewed separately here.

## Reviewed increment

Only driver `prepare_project`, its project-pipeline call, one Weak lifecycle test and boundary documentation changed. The operation removes raw entries and matching recency records; it never changes query identity, supplies data or grants authority. Parse roots survive, and DB reachability preserves remaining raw roots' transitive dependencies.

It runs after request cancellation/profile/source/artifact checks and before constructing new project stage results. Current input/rules and compatible target identity are retained. The target-independent Expand key may retain ZERO across target changes; CT/Types/Candidate retain their current target key. All normal exact-key lookup, dependency validation and per-request Verify remain active. No route to a stale or cross-target cache hit was found.

## Actual command

After `. /workspace/.zil-env/env.sh`, from `/workspace/zil`:

`cargo test --offline --locked -p zil-driver --features llvm-api --lib large_edits_release_obsolete_owners_before_constructing_the_new_epoch`

Result: **1 passed, 0 failed, 30 filtered out**. Log: `combo-memory-review-owner.log` (SHA256 `148037d6986e497e1e756972615678e65616b79486571f185270340933d439a5`). Executed on the reviewed precommit working tree; the exact commit contains the same code/test, plus the boundary documentation. No test/compile load continued into the next measurement window.

The Weak control confirms unchanged requests retain the current ProjectTyped owner, while edited requests release the old owner before the first Parse event. The edited report is compared with a fresh session.

## Boundaries

- Same root is numeric ModuleId. Different filesystem projects normally share root=0 and may conservatively evict one another, including single-source Candidate entries. This affects hit rate, not correctness.
- Old entries under different root IDs can remain. The trigger is modules+4 greater than raw capacity. It is not a whole-session byte ceiling.
- Earlier history tests were not independently rerun for this changed policy. The new lifecycle control and static review cover this increment; no formal full suite was run.
- The first 1560-request regression remains valid evidence. This receipt does not certify corrected RSS/latency or Rust/product acceptance; the second measurement window must establish those outcomes separately.

Structured increment: `combo-memory-review.json`. Combined receipt: `combo-review-receipt.json`.
