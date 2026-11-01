# Complete-call-closure runtime confirmation

This is a new epoch. The previous 54-build / 1620-observation experiment and its
bounded-import quality failure remain immutable. The new epoch does not reuse
any previous elapsed time.

The fixed suite has the same six source families, five one-byte input seeds,
20,000 consumer iterations and six alternating-order blocks as
`stage6_toolchain_runtime.py`. Every new source must match the previous frozen
source identity. There are five arms, in this order:

| Arm | Compiler | Explicit options |
|---|---|---|
| `monolithic_after` | Final production CLI, composed four-TU LLVM | `--function-merging after` |
| `monolithic_after_closed` | Same CLI and LLVM | `--function-merging after --closed-executable` |
| `quality_after` | Same CLI and LLVM | `--function-merging after --incremental-o2` |
| `quality_after_closed` | Same CLI and LLVM | `--function-merging after --incremental-o2 --closed-executable` |
| `rust` | Frozen Rust 1.99.1 | O2, checked overflow, abort panic, eight codegen units |

All ZIL builds explicitly request the actual LLVM API backend, eight units and
two compilation jobs. They must run current independent Verify exactly once and
report every emitted object's pre/post LLVM verification. The production pass
identity is compared in full; a private research-tagged prototype cannot be
substituted for the final production compiler. The exact source revision, source
inventory, build receipt, ELF, LLVM DSO and tool identities are frozen together.
Actual compiler-PID loader records are checked for each build.

Preparation performs 30 new builds and 150 complete native output controls.
These builds occur in the functional window and their elapsed times and peak
RSS remain diagnostic. Formal timing runs only the 30 frozen native programs:
900 observations, with forward/reverse arm order in alternating blocks. It
requires a separate exclusive CPU lease; no compiler invocation, source edit,
archive operation or artifact replacement occurs inside that window.

Codegen results retain requested and actual unit counts, jobs, and the complete
fallback reason. A legitimate complete-closure resource fallback is visible as a
whole-module result, never counted as successful multi-unit reuse. For the six
fixed families the expected partition counts can be frozen before preparation
from their function counts and the reviewed strategy: three units at 512 helpers
and eight at 2048 or 8192 helpers. A fallback or another count is an explicit
strategy deviation requiring review; it must not be silently admitted by changing
expectations after a build. Functional rejection, launch failure, timeout and
wrong native output are retained, with no overwrite or silent resumption.

The new runner reuses the frozen process-group cleanup, per-PID loader audit,
symbol/size audit and source generator helpers. It does not modify the old
nine-arm runner or old prepared directories. Statistical analysis follows the
same six-block median ratios and exploratory paired bootstrap, with the same
resampling indices for both arms and all five seeds within each case. New
quality/monolithic, quality/Rust and monolithic/Rust comparisons keep ordinary
visibility and explicitly closed results separate; all adverse cells are kept.

The production identity reader consumes the author's immutable `BUILD.json`
directly. Its source inventory is checked against the exact Git tree, every
selected source file SHA and Git blob identity, both recorded build-boundary
checks, the installed compiler/LLVM/clang bytes and the actual namespace source
file. The namespace is
`ZIL-OBJECT-ACTION-212-v2-complete-direct-call-closures`; it is not invented as a
new field in the public compiler report. The private `quality-closure-build-v1`
snapshot remains functional evidence only and cannot substitute for the final
production build.

All new CLI and runtime windows consume one immutable `RUST-REFERENCE.json`.
`stage6_production_identity.create_rust_reference` records the actual frozen
compiler's `-vV` output; `load_rust_reference` checks its ELF SHA, Rust 1.99.1,
exact commit and LLVM 23.1.1. Neither runtime arm can independently select another
Rust compiler.

The operational entry points are:

```sh
python3 stage6_quality_runtime.py create-config \
  --production-build /absolute/frozen/BUILD.json \
  --rust-reference /absolute/shared/RUST-REFERENCE.json \
  --reference-prepared /absolute/combined-runtime-prepared-v1 \
  --out /new/quality-runtime-config.json
python3 stage6_quality_runtime.py prepare \
  --config /new/quality-runtime-config.json --out /new/prepared
python3 stage6_quality_runtime.py measure \
  --config /new/quality-runtime-config.json --out /new/prepared \
  --lease /absolute/exclusive-lease.json
```

The lease must have `status: Active`, `window: quality-runtime-900`,
`profile: production-quality-five-arm-v1`, the exact absolute `out`, and the
configuration, production-build and Rust-reference identity triples. It is a
record of the root coordinator's exclusive-window grant, not a replacement for
stopping other local CPU work.
It also binds `prepared_manifest` and `prepared_marker` identity triples. The
execution keeps the exact Active lease bytes as `lease_raw_utf8` plus their SHA,
so a later explicit release of the coordinator's lease file does not rewrite
the measured grant. A separate exclusive-created attempt receipt records setup
failure or interruption without appending to a previously protected epoch.

An additional boundary epoch can be configured with `--boundary-8192-branch`.
It generates one new 8192-helper branch source pair with the same frozen
generator and 20,000 rounds, retains all five builds and 25 native controls, and
uses the ordinary compiler budgets. Its profile has zero formal observations;
the `measure` command refuses it. Resource rejection, timeout or unexpected
whole-module fallback remains explicit. This control does not extend the six
formal families or retroactively fill the old four SLP `NotRun` arms, whose
8192-branch source was never generated.
