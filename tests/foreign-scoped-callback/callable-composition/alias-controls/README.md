# Copy Fn observations and aliases

The three preserved sources are bound by `EXPECTED.json`.
EXPECTED.json records their original hashes and source-only arithmetic:
F returns 10, the local Fn returns 5, and run returns 15 or 25. Main subtracts
that expected value, so native exit is 0 and stdout/stderr are empty.

These are legal positive controls, not rejection cases. The existing
`local_callable_factories_local_factory` target's
`concrete_copy_fn_observations_and_aliases_reach_native` checks acceptance,
then production lowering/linking and actual execution at O0/O2. It explicitly
selects clang normally and LLVM API with `--features llvm-api`, checks the
actual backend and no fallback, and requires API builder input, parse_ns=0
and verification before/after optimization. No C callback/allocator driver
is linked and no allocation claim is inferred. The foreign callback target and its oracle are separate from these plain Copy Fn controls.

Historical early Analyze failures remain bound to the original candidate in Git; they are gaps for these legal sources, never passing rejections.
