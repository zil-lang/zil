# Full-pipeline review, 2026-11-12

This is an implementation checkpoint, not acceptance of complete push or a Rust speed claim.
Compiler integration at this review: `d6b84929b`; default-feature full workspace was frozen at
`16a8c2c02cd36b786632822a6d12ad687f6dabe3`. Research switches remain off by default.

## Propagation and actual consumers

The intended rule is: deliver a changed input to its recorded consumers, recompute only
those consumers, and stop delivery when the consumer's complete observable result is equal.
Batch publication at compiler phase boundaries can avoid UI-style read-time glitch repair;
it still requires that every dependent phase observe a consistent completed input epoch.
No old Verify capability can authorize a newly assembled input.

Current implementation uses stable function products, persistent structural trees and
sparse actual consumer updates in Types, Analyze, independent Verify obligations, Lower,
machine functions, partition recipes, object keys and link results. Pre-CT raw inference
also reuses successful body visits with original budget charges and a fresh traversal on
unrepresented replay. These are real consumers, but they do not constitute a single
complete scheduler: central reactive queries still primarily compute BodyKey, outer
whole-project query identities and several global checks/traversals remain. Names,
global declarations, positions and dense changes have broader invalidation costs.

LLVM uses the original fixed O2 pass pipeline, with separately bound function-stage and
machine-function products. Module and CGSCC work still runs; unknown analysis state is
fresh. Whole-module/caller dependencies can broaden function-stage invalidation. Object
reuse is coarser than function reuse, and an affected CGU still has current module setup,
checks, assembly emission and object packaging. All object hits do not justify reusing a
link result without its current external tool/runtime dependency closure.

## What the real workload proves

See `full-push-diagnostic-v8.json`: source `7c86653443be9d57e2d6632776a304dd57c8ad08`,
425 functions, requested CGU16/O2/jobs1. All 34 requests were valid; 15 successful pairs
had byte-identical complete executables to original-SDK main42; 180 native executions
were retained. A private edit rebuilt one machine-model function and one CGU, with 39
machine functions reused and three executed inside that CGU, 4875 machine pass skips,
zero fallback. Optimizer stages had **zero reuse** in this source. Therefore this run
does not meet the user's combined optimization/emission endpoint.

Private-edit candidate wall was 625.635ms, including object work 416.676ms (optimization
about150.84ms, emission211.99ms), link discovery76.542ms and link25.993ms. It is one
candidate-profiled diagnostic, not a controlled ratio or Rust comparison. Cold and broad
changes regressed; all results stay in the receipt. Position-only changes were still
866.294ms, notably Expand446.652ms; a broad-to-old-parent restore still rebuilt substantial
backend model work despite native history reuse. Pass skip counts alone do not quantify
net savings after key construction, parsing, restoration and checks.

## Correctness failures and immediate construction

The v9 native full gate failed: scratch target-info initialization caused a test crash
(fixed and independently rerun), and real optimizer replay rejected bitcode-upgraded
intrinsic declaration attributes. Preserve original declaration projections and bind them
to current declarations before reconstruction; retain exact output comparison. A failed
reconstruction must abort every enclosing original pass/adaptor before invalidation or
further execution, destroy the invocation, and use a fresh context for fallback. ABI2
adds that explicit failure protocol; its SDK and full native tests are pending here.

The default workspace run completed all33 groups:32 succeeded; generic_regressions had
one real default-worker-stack overflow. Successful summaries total6142 tests; the failing
target separately reports213 passed/1 failed. This gate is **failed**, not a pass. The
same single test reproduced the overflow. Fix stack use without increasing the default
worker stack or weakening the test. Raw logs: `out/runs/full-push-workspace-batched-01`
and `out/runs/default-worker-stack-repro-01.*`.

Next acceptance requires real425 optimizer reuse with full current analysis binding,
the four measured MIR cost scopes, strict native/default gates, then unprofiled balanced
main/candidate/Rust runs with the same requested CGU/O2/jobs and inspected actual units.
Fixed industry source references are in `industry/references.json`; Rust demand queries
and red-green checks, ThinLTO and Zig work products inform distinct parts of the design.
None proves that arbitrary LLVM pipelines can be cached using function bodies alone.

The next raw archive must include post-v6 failures, SDK source/build receipts and actual
runtime libraries. Existing remotely recovered v6 is documented in
`full-push-recovery-v6.json`; newer local evidence is not yet a durable backup.
