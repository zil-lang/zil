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
One central graph is not itself a requirement: directly connected phase consumers can
implement the same propagation rule. The acceptance issue is unnecessary rediscovery,
reaggregation and recomputation in the actual source-to-executable path, not graph API
uniformity or the count of graph nodes.

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

## Subsequent verification in the same window

SDKv12 builds successfully. Native full06 has51 passed/2 failed: complex scalar and
alignment consumers fail their required machine reuse counts. Actual scalar now has4
optimization-stage reuses,26 analysis reconstructions,0 replay failure/0 fallback.
Exact machine-key diagnostics identify SSA naming suffixes as the difference (for
example old `%v.2049` versus current `%v.201`). The original LLVM ValueSymbolTable
uniquing counter is not serialized by bitcode; its restoration is under construction.
No key field or required reuse assertion was removed.

The worker stack failure is repaired by routing ordinary binary expressions directly
to the same scalar visitor, preserving the original common expected-type check and
short-circuit early returns. Original generic_regressions214/0/0 and Types library95/0/0
pass at integrated source d298737e9 (runs default-worker-stack-fix-02-full and
 types-stack-dispatch-tests-01). The recursive five-function chain was byte-identical
to main42 before this fix: do not attribute the complete stack failure to one new
receipt field. Box-stats alone failed; that negative is retained. This targeted repair
does not turn the earlier exact-source whole-workspace failure into a new frozen full pass.

Raw v7 was remotely pushed and freshly HTTPS recovered: archive/source-history at
c04b7a9adc7d2c53d1c64f18145c6eb371dc8638,935 files/495641306 raw bytes,6 parts,
134445243 compressed bytes. Read full-push-recovery-v7.json for scope and SHA256.
Later scalar04/05, stacktrace and successful stack repair are outside this snapshot.

## Joint-chain evidence and measured negative cost

SDKv13 and sourceafc1319 pass all53 native tests, with original complete object equality,
numbering/alignment/reentrant/failure/abort tests retained. A subsequent defensive name-map
admission check at bf45ea905 also passes53 native tests and strict native-products workspace
Clippy; the strengthened bundle-owner cold/warm scratch passes separately. These overlapping
53-test runs are not added together. Original default-feature strict Clippy07 passed.

The real425 diagnostic04 at afc1319 completed34 valid requests,180 native executions and15
same-request whole-ELF-equal pairs with original-SDK main42. Private edit:78 optimization
stage reuses/300 executions,507 current analysis reconstructions,39 machine function reuses/
3 executions,4875 machine pass skips,0 fallback. Pre-CT each visitor checks1/reuses424.

There is no demonstrated new wall-time gain: private633.931ms; cold8565.004ms,
broad8892.762ms are retained negatives. This candidate-only-profiled, single-block window
is not a formal comparison to oldv8 or Rust. Object441.666ms includes optimization204.91ms
and emission186.74ms; actual MIR parsing133.685ms, restoration/check9.799ms, key14.428ms,
capture0.458ms. Link discovery77.431ms and actual linking28.989ms remain. More pass reuse
without deleting serialization/reconstruction is insufficient.

Consequently the next construction is driven by cost: retain the original small MIR header
parser for complex current state but reconstruct the MI/MBB body directly; share or directly
render complete current world/header/TLI projections instead of repeatedly cloning/serializing
unchanged dependencies. Neither slice may weaken complete input/analysis binding or output
oracles. A separate raw-machine owner-history slice must restore identities atomically with
values, since restoring only the old module with the new epoch's scalar identity is unsound.
See full-push-diagnostic-v13.json and full-push-panorama-review.json.
