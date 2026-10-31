# Standalone SSA repair v1–v4 evidence

The entire frozen `stage6/ssa-update`, ssa-handoff.log, independent v2/v4 reviews and
explicit original input/oracle/perf references are preserved. Shared pinned
JumpThreading and SSAUpdaterBulk source/license/provenance and both builders are kept.

v1 limits bulk repair to threadEdge where CFG/DT is consistent, preserving original
paths for the other callers and debug/lifetime fallback. The initial missing-clang
link failure and valid linked retry are distinct. v2 shared unpruned IDF and v3 demanded
closure preserve their PHI/use growth cap; both rejected all 505 actual 512-branch
helper calls and did not deliver the intended gain. v3's first rejected plan needs
two real PHIs for one use. These negative epochs are not relabeled as v4.

v4 retains original precise Bulk liveness/pruned placement and value rewriting,
caching only placement/reserve information for complete equal defining/using block
sets within one stable batch. No concrete values, types, notifications or ABI layout
are shared. Same-flags original and candidate rebuild exactly two translation units.
Counter-only diagnostics preserve 505 calls, 18781 variables, 18276 hits/505 misses,
511838 live-in visits and 37562 PHIs, distinct from timing arms. Actual 2048-branch
JT 5.2435 CPU seconds is a functional diagnostic; CVP remains 19.3791 seconds.

Independent v2 review is limited to v2, with its own linking/debug/lifetime fixture
failures. Independent v4 has 82 C API requests, 134 native invocations, 39 helpers,
and two direct utility functions/34 variables. The reviews are not merged into one
claim. All 14 raw input/oracle .bin files are explicitly retained, including debug,
lifetime, edge-self and fallback oracles. Original ca79 opaque branch perf.data is
kept as the original baseline profile, not a v4 measurement. Clean-source-v4 generation,
counts/probes, malformed CLI attempts and all early errors remain.

These are frozen isolated functional/diagnostic observations, not an exclusive
performance comparison, final combined toolchain, broad runtime-quality proof or
production-default promotion. Historical source/compiler/library identities remain
unchanged. New combined/runtime/residual roots are excluded.

Per-path bytes/SHA/dispositions are in compressed FILES/OMITTED manifests; generated
source, patches, source licenses/provenance, command argv, raw logs, failures and
controls remain byte-exact. Selected ELF/object/static archives (including small
harness executables) are omitted by byte identity. Their source/commands are kept.
Official SDK installation/header receipts identify reconstructible external inputs;
no SDK archive or compiler target is copied. An omission SHA is not restored bytes.
Collection-time tools have explicit exact-source inventories in the adjacent plan;
collection HEAD is not a measured production/compiler identity.

Retained paths: 1896; omitted paths: 868;
unique blobs: 539; archive bytes: 9836111.
Each compressed stream is split at 40 MiB.

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/llvm-ssa
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/llvm-ssa --dest /tmp/llvm-ssa-restored
```
