# 344: bounded source inference and machine reliability

## Stack failure and proposed change

The preserved legal `large-copy-enum-error-borrow-capture-var.zil` completes
Parse/Core but aborts on a 2 MiB API thread during the CT closure-constraint
prepass, before Types/Analyze/Verify. An alternate-signal-stack abort trace of
the retained 338 rlib resolves `infer_source_closures -> prepare ->
Solver::expr_raw`, repeated through `expr_expected/expr/projected_base` for the
13 array projections. The unoptimized x64 `expr_raw` frame reserves
`0x1e568` bytes (124264): every projection retains storage for unrelated large
call/closure/member branches.

Split recursive expression variants into separate private helpers. Keep the
node/work charges, expression-index increment, write-place restriction and
evaluation order in the original dispatcher, at the same position and exactly
once. Each helper retains only its own variant's temporary values. Do not raise
thread stack sizes or language limits, introduce worker threads, skip inference,
or change public AST/IR/Wire and verified authority boundaries.

The acceptance change is reliability: the original invalid `let f` source must
return its ordinary Types `E-CLOSURE-MUTABILITY` diagnostic, and the legal `var f`
sibling must reach independent Verify on the default 2 MiB API stack. Their
source bytes remain unchanged. Neither input currently supplies evidence that
Verify itself was entered before the abort.

## Required observations

Run both inputs in subprocesses using a default Rust worker thread so a renewed
stack abort cannot terminate the Rust test harness. Preserve Parse/Core stop
controls, complete legal Verify and raw Types/Analyze/Verify artifacts, and
Clang/API O0/O2 native execution. Add a deeper nearest legal projection case and
an actual low-limit failure; budgets retain their existing meaning. Broader
large callable/helper/foreign and floating/anonymous-union obligations remain
open until real source and machine evidence closes them.

## Deeper legal control

After the first split, both preserved sources return normally at 2 MiB. A new
20-layer length-one array closure then reaches Types and exposes the equivalent
large-frame recursion in `closures::InferBuilder::expr_raw`. Apply the same
per-variant split there; keep its `visit` charge and all graph work unchanged.
This is a second reproduced failure, not an assumption about the first abort.

A 32-layer ladder-initialized projection then exposes cumulative Types
`Builder::expr_raw/aggregate` frames (expr_raw alone: 0xb3e8 = 46056 bytes).
Keep scalar/operator arms available to 340; split only Call/If/Sequence there,
and the six aggregate branches into private helpers. Retain the original
post-dispatch expected-type check and early-return behavior exactly.
