# Research: preserve optimization closure across incremental O2 units

The frozen 888/four-TU runtime matrix contains a real quality failure: the
incremental 8192-arithmetic executable is roughly 326 times slower than the
single-unit candidate. All recorded native outputs are correct. Preserve that
NoGo result, compiler/library pair and all 54 prepared artifacts unchanged.

Current partitioning rotates fixed 256-function source chunks among requested
owners. It imports raw model functions only when individual cost is at most 64,
and spends a 65536 per-unit budget in definition-ID order. It neither guarantees
an optimization root's full call closure nor records an explicit quality fallback
when the budget leaves declarations. These are ZIL research limits, not Rust
heuristics. Budget exhaustion could explain one case, while available-externally
processing or inliner decisions could explain another; establish the actual units
before selecting a fix.

The first capture completed with two valid API builds, ten matching native
oracles and both executables byte-identical to the frozen bad-quality artifacts.
Each unit invokes LLVMRunPasses once with default<O2>; the native API enables
late MergeFunctions through LLVMPassBuilderOptionsSetMergeFunctions. Thus the
captured output includes the complete requested O2/late-Merge pipeline. The first runner incorrectly
expected sixteen calls rather than eight; its original receipt is retained with
that failed counting assertion, and the expectation is corrected for future runs.

First capture actual LLVMRunPasses input/output with a research-only C shim. It
must identify the exact compiler/library, print the existing modules, forward the
original arguments and return value, and add no transform or global LLVM option.
The existing LD_PRELOAD policy disables object-cache reuse. Use new output paths,
current Verify and LLVM verification, five native input oracles and the exact
existing source. This is functional evidence, not a performance measurement.

Then evaluate a separate graph-based candidate. Candidate mechanisms include
complete selective transitive imports for optimization roots and co-location of
an optimization region if its closure cannot be represented under the chosen
policy. Preserve one canonical emitted owner per symbol, exported address
identity, local/global reference closure, foreign ABI and current preflight. Any
imported body and its complete dependency changes must affect the consuming unit's
cache key. Do not use closed-executable assumptions to claim a default-linkage fix.

Do not silently replace the frozen incremental strategy, merely raise limits
without verifying effect, or call partial imports runtime-equivalent because
outputs match. The isolated candidate must pass same-input runtime-quality and
compile/cache controls; formal timing and any production/default decision follow
independent review. A complete optimization region may legitimately invalidate
when any function it optimizes changes; represent this dependency honestly.

## Actual-unit evidence and selected next candidate

The captured 8192-arithmetic root owns 1024 helpers, imports 2340 and retains
4828 declaration-only helper calls after O2. The 2048-branch root owns 256,
imports 1129 and retains 663 declaration-only calls. Printed LLVM instruction
counts differ from machine-model costs because the API builder folds constant
operations; neither the old estimate of 615 missing functions nor a printed-IR
cost estimate is the importer budget calculation.

Four actual-IR ablation arms separate closure from linkage. Bounded AE replay is
byte-identical to each frozen executable, using the same target, PIC mode, codegen
level, PTO settings, late MergeFunctions and object link order. Complete AE and
complete Internal implementation copies both recover arithmetic runtime quality,
but complete AE has substantially higher diagnostic optimizer cost. Complete AE
does not fully recover branch runtime quality; complete Internal copies do.
Bounded Internal alone leaves the large arithmetic regression. Additional
512-body default and closed captures/replays are byte-identical at baseline;
complete Internal copies recover runtime quality in both. All sixty ablation
native outputs and twenty capture outputs match the existing five-seed oracle.
These are functional diagnostics, not formal performance observations.

The next isolated structured-machine candidate therefore collects the complete
transitive **direct-call** graph of source functions for each consumer unit. It
does not spend an optimization-body budget on a mere address reference. Each
cross-owner implementation copy receives a fresh Internal symbol, retains the
complete function body/signature/attributes, and is really emitted if LLVM does
not inline it. Only direct Call.callee operands change. Canonical definitions,
function-valued operands, callback arguments, globals and indirect calls retain
their original symbols. This gives a real exact implementation for non-inlining
IPO; it does not relabel an AE body as External and later discard it.

The first candidate replaces both silent truncation limits with a whole-plan
resource guard. Sum every consumer's additional clone cost using the existing
machine instruction/block cost. If it exceeds the original source-function graph
cost, fall back to the original whole module before copying any body. Thus new
implementation bodies consume at most one additional source graph by this metric.
Distribute bodies with a remaining-use count and move the final use; avoid a third
full set of imported templates. This is a body-cost bound, not a promise that
total RSS is twice the baseline: signatures, declarations, types, LLVM analysis
and native emission have separate allocations and require measurement.
Unsupported local-definition dependencies also trigger a reported whole-module
fallback, never a partial closure disguised as complete.
All copied bodies and canonical declaration dependencies enter ordinary module
preflight and object-cache identity. Graph co-location may later reduce duplicate
work, but must recheck quality: merely converting a copied Internal body back to
an owned External body can change IPO and single-use inlining decisions.

Required structured-candidate controls include same-unit and cross-unit self
address comparisons, call @f(ptr @f), two distinct equal-body canonical function
addresses, callbacks, recursion, shared global state, resource/closure Drop and
error cleanup, complete edit invalidation and warm reuse. The raw-IR ablation is
not evidence that these controls or the production implementation already pass.
