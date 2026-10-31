# Generated-source CLI analysis

```sh
python3 tools/research/incremental-compilation/stage6_generated_cli_analysis.py \
  --window /absolute/path/to/confirmation-default \
  --out /absolute/path/to/new-analysis-directory
```

This adapter audits the completed 23-shape, six-block, five-request O2 protocol
for default, closed, isolation or incremental windows. It requires the adjacent
execution receipt and released exclusive lease. It does not run a compiler or
produced program, read Rust incremental-cache internals, or infer intermediate
cache state from the directory left after a sequence.

It checks all 1380 rows, their order, actual options, raw compiler reports,
current Verify and object pipeline identities, independent complete four-byte
native results, and cumulative failure history. ResourceLimit rows are retained
without assuming they reached Verify. All 115 comparison cells, all adverse
cells and all invalid cells are exported. A cell containing any failed request
does not produce a speed ratio.

Each recorded input SHA sidecar is compared with reproduction from the exact
frozen `runner.py`; a separate arithmetic calculation checks every expected
result. The original edited source bytes were not snapshotted, so this is
explicitly source reproduction, not discovery of old edited snapshots. Retained
restore bytes and baseline size metrics are directly checked. The runner is
loaded using `compile + exec` in a separate namespace, with `__name__` distinct
from `__main__`; source reproduction is written only beneath the new output
directory and removed after its SHA inventory is saved.

The first local analysis epoch used SourceFileLoader, which generated a single
Python bytecode file beside the frozen runner after measurement. That side
effect, original producer, exact bytes and unchanged measurement records are
preserved in `generated-analysis-import-correction-v1`. The revised analyzer
does not use a bytecode loader; v2 results were checked against v1 and were
identical. The old result and bytecode were left unchanged for the archive.

Unlike the real-source runner, this producer recorded no per-request executable
SHA. The analyzer therefore makes no claim that historical ELF artifacts were
rehashed. Native results are the retained row records. DSO binding uses the
frozen pair's loader probe, source/ELF/library/wrapper identities and each object
report, not new per-request loader tracing.

Statistics use ratios of six-block medians and an exploratory 2000-draw paired
bootstrap, seed 6302026. Forward/reverse strata each contribute three resampled
blocks, shared across both arms and all five requests within a case. Cases
remain fixed; valid cells have equal geometric-mean weight. Invalid cells are
always counted and listed, so the valid subset is not a claim of complete
workload coverage. The shared identity and statistics primitives are bound to
the copied `stage6_cli_analysis.py`.
