# Opaque O2 complete-request comparison

`stage6_opaque_bench.py` adds compilation measurements for the six live-input
families used by `stage6_merge_runtime.py`: 512/2048/8192 arithmetic helpers,
512/2048 branch helpers, and 512 long-body helpers. The program reads a runtime
byte and consumes every helper over 20,000 rounds. This complements the 23-family
constant-input/module-distribution matrix; neither corpus replaces the other.

For each family, each arm and each block has its own compiler caches. The sequence
is `cold → unchanged → early_edit → late_edit → restore`. An early edit adds one
to the result of helper 0; a late edit adds two to the last helper. Both branch
exits are edited. Each edit starts from the original source, and restore has
exactly the cold source bytes. Both languages receive the same edit.

The independent oracle computes all helper contributions, then the fixed loop
for seeds 0, 1, 7, 42 and 255. Every successful compiler request is followed by all
five native checks, comparing the complete four-byte output, exit status and
stderr. Source preparation, identity checks, oracle computation and native
checks are outside the complete compiler CLI timer. Cold means an empty compiler
cache, with the OS page cache left uncontrolled.

Six alternating AB/BA blocks give **360 compiler requests per full window**.
Both ZIL arms use O2, eight requested CGUs, two requested codegen jobs,
`--function-merging after`, independent object caches and independent semantic
caches. An isolation window applies any `--closed-executable` and
`--incremental-o2` switches to both ZIL arms. Rust uses opt-level 2, eight CGUs,
overflow checks, aborting panic and one incremental directory per sequence.
These are explicit research strategies, not the no-option ZIL defaults.

```sh
python3 tools/research/incremental-compilation/stage6_opaque_bench.py \
  --pair-receipt /absolute/path/api-pair-receipt.json \
  --comparison isolation --blocks 6 --out /new/output/directory

python3 tools/research/incremental-compilation/stage6_opaque_bench.py \
  --pair-receipt /absolute/path/api-pair-receipt.json \
  --comparison rust --rustc /absolute/path/rustc \
  --blocks 6 --out /another/new/output/directory
```

The pair is the frozen common-source native LLVM pair, not an `opt` wrapper.
The runner rechecks the compiler, launcher, LLVM library, source and toolchain
receipts, embedded pipeline tag and common original-source identity. A fresh
actual compiler PID loader probe must resolve exactly the expected library;
interposition variables are removed from all timed launches. New objects require
LLVM verification before and after optimization. Cached objects instead retain
the verified-producer fact and truthfully report that LLVM verification did not
run again; the current ZIL Program Verify must run once in both cases.

Every original stdout/stderr and row survives failures. A timer interruption
kills and reaps the entire compiler process group, preserves an `Interrupted`
row and a separate interruption receipt, and restores the previous alarm handler.
An interrupted window also lists each unattempted request in `not-run.json`. Any failed or missing
observation invalidates its comparison cell; no favorable ratio is calculated
from the remaining successes. A failure also invalidates all later rows of that
arm's five-request sequence, since its cache history no longer matches the
protocol. Later requests still run with complete evidence; `execution_valid`
records their own outcome separately from `sequence_history_valid`.
`--control-only` is required for non-six-block
functional controls, and those timings are not performance evidence. Existing
output directories cannot be overwritten.

Large original LLVM branch/arithmetic compiles can take roughly a minute per
cold or edited request. Plan an exclusive CPU window before running the full
matrix. The separate prepared runtime experiment is still required to measure
generated-code performance; the five oracle executions here only establish
correct behavior.
