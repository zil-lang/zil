# 227 frontend author evidence

The initial author Pass below was superseded by an independent runtime failure in persistent Copy capture writeback. The failure, unchanged old executables (replayed exit 2), root cause, repair and new scoped 53/0 run are preserved in [writeback-fix/AUTHOR.json](writeback-fix/AUTHOR.json). That correction is the latest author receipt; independent review of the repaired candidate remains separate.

Author scoped Pass only. Independent review, full-language and product gates are NotRun in this receipt. The candidate is uncommitted; root will bind the final commit.

`AUTHOR.json` records exact commands, LLVM API feature/backend, per-run counts, and the prior budget reference. Initial and updated runs are separate; do not add historical replays to the current acceptance counts. `SOURCE-SHA256.json` fingerprints the current source and executable test files.

The three raw tar.gz archives retain actually executed ZIL inputs, phase reports, LLVM API objects/executables, output and allocator-probe records under their original /tmp directory basename. Corresponding `.files.json` inventories record every member digest. Compilation reports retain execution-time absolute paths. Test logs are separately gzip-compressed with both compressed and original hashes in `AUTHOR.json`.

Final new scope: 11 source tests plus 1 allocation test; 7 source cases at O0/O2 (14 executions), plus 2 machine objects with 10 probe executions including 6 real allocation failures and 4 controls. Existing regressions: 38 tests. These are overlapping mechanisms, not a sum of independent product proofs.
