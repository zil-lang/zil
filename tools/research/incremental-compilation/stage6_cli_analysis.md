# Real-source CLI analysis

`stage6_cli_analysis.py` reads the frozen 14-program, six-block O2 real-source
window. It does not run compilers or produced programs and does not reinterpret
the other generated/opaque protocols as this experiment.

```sh
python3 tools/research/incremental-compilation/stage6_cli_analysis.py \
  --window /absolute/path/to/confirmation-real \
  --out /absolute/path/to/new-analysis-directory
```

The adjacent `confirmation-real-execution.json` and
`confirmation-real-exclusive-lease.json`, the Git source revision, compiler pair,
source snapshots and retained final executables must still be available. The
output directory must be new. A failed audit remains recorded there and is not
silently overwritten.

The reader checks the complete request grid and ordering; exact compile options;
source copies against the recorded Git tree; compiler/wrapper/LLVM/source and
prior loader-probe identities; raw ZIL JSON; native oracle observations; and
cache-history validity. All comparison cells are retained, including failures
and adverse outcomes. Only the final executable for each sequence is rehashed,
because the runner reused that path for cold and unchanged requests. Earlier
artifact hashes and native outcomes remain historical records, not newly
executed controls.

Point estimates are ratios of the two six-block medians. The default exploratory
bootstrap uses 2000 draws and seed 6302026. Each draw samples three forward and
three reverse blocks separately, sharing indices across both arms and both
requests within a case. Cases remain fixed and have independent draws. Every
valid comparison cell has equal weight in the geometric mean; the count of
invalid cells is always shown. These intervals describe this window's variation,
not an industrial workload population or a product acceptance gate.

The default output includes the complete cell table, separate adverse/invalid
tables, per-request resource records, a row audit, source/artifact inventories,
method and identity details in `summary.json`, and `REPORT.md`. This version's
explicit after-MergeFunctions/semantic-cache/default-export protocol must not be
relabeled as an unparameterized compiler default.
