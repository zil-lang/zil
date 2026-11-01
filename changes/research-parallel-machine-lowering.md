# Deterministic parallel machine function lowering

Stage14 baseline 5821120ac5952bcf799afe76de1030741d9ccb89. Machine lowering
currently emits independent functions in source order on one thread, even when
BuildOptions already explicitly permits several codegen jobs. The opaque branch
workload has 2048 nontrivial independent helpers; the prior private diagnostic
recorded about 44 ms for this phase. This proposal changes execution scheduling,
not generated machine definitions or language acceptance.

Keep current VerifiedModule/LIR binding, entry-wrapper checks, scalar-summary
planning and closed-function collection serial and before workers. Their existing
shared logical budgets are unchanged. Extract the current per-function loop into
a batch operation that owns its body items and intrinsic set and borrows only
immutable functions, target, closures, resources and summaries. Source functions
are assigned in contiguous batches of 64. At least 128 functions and more than
one requested job are required for parallel execution; a closed collection with
fewer than 128 retained functions remains serial despite a larger original input; defensively bound the
private job argument to 1..=64. Existing serial emitter entrypoints pass one job;
the native build entrypoint passes its existing validated codegen.jobs setting.

Each worker returns source-indexed batches. Join every worker, sort batches into
source order, and only then select the first ordinary diagnostic or concatenate
machine items. Merge intrinsic sets in their existing BTreeSet order. A worker
panic or thread-spawn failure returns an infrastructure diagnostic after all
started workers join; it never produces successful partial output. Other workers
may compute later functions after a source error, but this remains bounded by the
current input and requested jobs, with no publication or extra semantic budget.

Prelude/runtime/wide-string construction, final wrappers, LLVM text, transforms,
partitioning and complete current machine preflight remain on their original
paths. Workers end before LLVM codegen workers start; there is no nested LLVM
parallelism. Current cancellation checks remain before/after this phase. No
Verify bypass, cached proof, new dependency/unsafe code, public schema, logical
budget, output ordering or object-cache protocol change.

Use compact scheduling controls for out-of-order completion, source-first errors,
joining after panic and the small-input serial path. Root runs real-source
jobs=1/jobs=2 output and diagnostic/native controls and focused complete-request
measurements; no speedup is established merely by adding threads. Reduced wall
time can cost worker allocation/stack memory and extra work following failure;
report measured CPU/RSS as well as wall time.
