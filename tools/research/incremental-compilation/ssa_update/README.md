# JumpThreading SSA repair research

The active candidate is **v4 grouped SSAUpdaterBulk placement**. It preserves
LLVM 23.1.4's exact pruned placement calculation and value rewriting, but reuses
placement results when the complete defining/using block sets are identical.
It changes no public header or class layout. Two fixed-source translation units
are rebuilt into an isolated LLVM DSO; the installed LLVM prefix is untouched.
The research does not change production ZIL or disable any O2 pass.

`prepare.py` creates v1: call `SSAUpdaterBulk` only from the stable `threadEdge`
CFG point. External debug records and lifetime uses keep the original updater.
Other JumpThreading callers retain their original path because one runs while
the CFG/DT are temporarily inconsistent. v4 reuses this exact helper, including
LLVM Bulk's own value-handle notifications and PHI cleanup.

`prepare_grouped_bulk.py` creates v4 from frozen v1. Its cache lives only within
`SSAUpdaterBulk::RewriteAllUses`. It stores pruned IDF blocks and the original
live-in/definition count used to reserve the per-variable value map. Complete
sorted block sets form the key; no concrete value or type is shared. CFG/DT do
not change during a batch. The original predecessor-cache contract already
requires this invariant.

From the repository root, with the cloud toolchain environment loaded:

```sh
python3 -B tools/research/incremental-compilation/ssa_update/prepare_v4.py --out /new/ssa
python3 tools/research/incremental-compilation/jump_threading_plugin/build_component_toolchain.py \
  --llvm-prefix /workspace/.zil-env/llvm-23.1.4 \
  --headers-prefix /workspace/.zil-env/research/stage6/jump-threading-plugin-sdk \
  --recipe /new/ssa/v4/grouped-recipe.json --out /new/llvm-grouped --jobs 1 --execute
```

Build `original-recipe.json` in a second new prefix using the same command.
Both rebuild the same two units with the same flags. Source manifests and build
receipts bind all source, archive member, header, tool, and library hashes.
`upstream/PROVENANCE.json` pins the additional official SSAUpdaterBulk source;
the JumpThreading source/license are provided by the sibling pinned-source tool.
`prepare_v4.py` needs no downloaded research output or untracked workspace
source: it generates v1 and v4 exclusively from the vendored files and records
its commands. The parent four-unit clean CI preparation can call it once, then
consume `v4/grouped/SSAUpdaterBulk.cpp` and the v1 JumpThreading patch.

`validate_controls.py` reruns the independent review's frozen C API child/plan
and native oracle fixtures. Supply the review fixture paths explicitly; it
checks actual DSO loading, LLVM verification, complete native output, and IR
comparison against the chosen original. It refuses an existing output path.
Author reruns are labeled separately from the independent review. Actual ZIL
requests use `pass_timing/run_probe.py`, current Verify, the original opaque
source, disabled caches, and the five retained expected outputs. Source the
cloud environment so the CLI can launch clang at the link step.

`instrument_grouped.py` derives a separate counter-only source/recipe to measure
actual cache hits, misses, live-in visits and inserted PHIs. Never use its
timings as the performance arm. Keep all source/build/loader/request/native
receipts and raw failures. Formal performance requires the parent's same-window
original/candidate measurement protocol and independent review.

Earlier v2/v3 are retained negative experiments. `prepare_shared_idf.py` and
`prepare_demand_idf.py` use unpruned shared IDF or demanded PHI closure with a
PHI/use growth cap. Both rejected every actual 512-branch helper call: real SSA
needed two PHIs for one use. `instrument_demand.py` and
`inspect_demand_plan.py` reproduce the counters/first-plan explanation. These
epochs are not promoted and their results must not be reported as v4 results.

Current local receipts are under
`/workspace/.zil-env/research/stage6/ssa-update/`:
`source-v4`, `toolchain-v4`, `toolchain-v4-original`, `controls-v4`,
`controls-v4-original`, `opaque2048-v4`, `native-five-seeds-v1-v4.json`,
`source-v4-counts`, and `counts512-v4*`. Detailed rationale and limitations are
in `changes/research-jump-threading-ssa.md`.
