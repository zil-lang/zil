# Opaque-source CLI record audit

```sh
python3 tools/research/incremental-compilation/stage6_opaque_cli_analysis.py \
  --window /absolute/path/to/completed-opaque-window \
  --out /absolute/path/to/new-analysis-directory
```

This independent reader requires the full six-family, six-block, five-request
protocol and a released adjacent `-exclusive-lease.json` (or an explicit
`--lease`). It reads original compiler/native outputs, source variants, recorded
ELF identities and both old/fresh loader probes. It runs no compiler or native
program. No result is implied for a window that has not been measured.

The analyzer checks the 360-request order and symmetric ZIL isolation options,
all 18 original source variants, source byte counts and line distributions,
current Verify and LLVM object reports, all five native inputs after each
successful request, complete four-byte outputs, and cumulative cache-history
validity. A separate direct-helper-sum/cyclic-visit oracle checks the producer's
residue-based arithmetic. Source reproduction extracts only the two frozen
generator functions into a separate namespace; it does not load the benchmark
as a module, execute its imports/main, or create bytecode beside measured files.

Each measured compiler/DSO/source identity is checked against the frozen pair;
the tool sources are checked against their explicitly recorded Git revision.
Only the final restore ELF paths can be directly rehashed because earlier
requests reused those paths. Earlier artifact hashes remain historical row
records. Rust incremental directories are neither read nor treated as snapshots
of historical cache states. Native checks establish correctness, not runtime
performance. Historical timing, CPU/RSS and exit/timeout values remain producer
row observations; this reader has no independent process receipt that could
re-prove those past events.

Statistics reuse the frozen generated-source analyzer: ratios of six-block arm
medians, 2000 paired bootstrap draws, seed 6302026, and separate three-block
forward/reverse resampling shared by both arms and all requests in each fixed
case. Fixed cells have equal geometric-mean weight; no workload-population
sampling claim is made. Every adverse and invalid comparison cell is exported.

This file adds an adapter without changing the frozen real/generated analyzers.
