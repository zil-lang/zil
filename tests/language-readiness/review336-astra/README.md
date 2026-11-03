# Independent 336–339 review inputs

These inputs were prepared independently of the implementations. The source
expectations were fixed against the frozen `dde8be301` profiles before candidate
execution. They are review cases, not new language specifications.

`expectations.json` contains the original source matrix. `baseline.json` records
the actual older B CLI results. `baseline-controls/` deliberately removes life
annotations or replaces only the child thread lane with the old scalar lane;
those altered controls check fixture setup and existing behavior, not the new
capabilities. Their results are in `baseline-controls.json`. The initial nested
writer used an immutable callback binding; both original invalid inputs remain
as `thread-nested-writer-setup-invalid.zil`. The corrected case uses `var inner`.

337 is independently reviewed at final commit
`97fb6e7686a01a1715a639d494c45048773fc6e1`. Its production and tests are identical
to the executed `51788f2b95d22394967ef102ad68b8976135596c`; only the author's two
result documents differ. `candidate337.json` records four positive, five safety
negative and one Unsupported source result. The unchanged no-life unused generic
alias gap is a separate KnownIncomplete observation and is excluded from passing
coverage. `native337.json` records two original positive inputs across both
backends and O0/O2, eight actual builds/executions, each printing `0` and exiting
zero. API runs used the builder with verification and no fallback.

The exact source commands are retained in `candidate337.json`. Native commands
used the same candidate CLI and this form with a fresh task temporary directory:

```sh
source /workspace/.zil-env/env.sh
/workspace/.zil-env/target337/debug/zil build SOURCE --entry main \
  --backend BACKEND --opt OPT --emit-dir TEMP/phases -o TEMP/program --json
TEMP/program
```

`raw.rs` is a standalone independent Rust test harness, not a new Cargo target.
It starts from the retained real ZIL sources, then mutates actual TypedIR operands
or contracts while retaining a correctly fingerprinted untrusted candidate.
Exact `rustc` arguments, linked rlib hashes and the three successful 337 test
results are in `raw337.json`. Both raw and Wire forms are tested. The two 339
tests are prepared but were filtered out on the 337-only candidate. A raw return
type mismatch deliberately records Analyze candidate success followed by Verify
rejection; Analyze is not claimed to duplicate complete raw instruction typing.

The 337 review conclusion is in `docs/validation/review337-astra.json`. No 339
candidate result, full-workspace result, independent budget-boundary execution or
Rust product comparison is claimed here. Original sources and compact receipts
are retained; temporary machine artifacts were hashed and removed.
