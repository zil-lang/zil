# Copy Fn observations and aliases

The three sources are byte-for-byte copies of root's
`/tmp/zil-127-review/new_local_fn_{observe,move_alias,borrow_alias}.zil`.
EXPECTED.json records their original hashes and source-only arithmetic:
F returns 10, the local Fn returns 5, and run returns 15 or 25. Main subtracts
that expected value, so native exit is 0 and stdout/stderr are empty.

These are legal positive controls, not rejection cases. The existing
`impl127_local_factory` target's
`concrete_copy_fn_observations_and_aliases_reach_native` checks acceptance,
then production lowering/linking and actual execution at O0/O2. It explicitly
selects clang normally and LLVM API with `--features llvm-api`, checks the
actual backend and no fallback, and requires API builder input, parse_ns=0
and verification before/after optimization. No C callback/allocator driver
is linked and no allocation claim is inferred. The foreign callback target
keeps its original nineteen tests and all existing oracles unchanged.

The first native-control attempt on the f08ce32e checkpoint rejects all three
legal sources at Analyze/E-ANALYZE-IR before Verify/native: observe and borrow
report "by-value symbolic Fn call would require a Copy permission"; move
reports "a symbolic F observation cannot be copied into a transferred value".
This is the original compatibility failure, not a passing rejection oracle.
