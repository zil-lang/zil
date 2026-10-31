# Production quality CLI windows

These new tools consume the final production BUILD receipt and the shared
`RUST-REFERENCE.json` used by the runtime matrix. They do not change the old
private-tag tools, baseline compiler identities or measured epochs.

```sh
python3 tools/research/incremental-compilation/stage6_production_cli.py \
  --production /absolute/path/quality-production-a1ab/BUILD.json \
  --rust-reference /absolute/path/RUST-REFERENCE.json \
  --window generated-default \
  --lease /absolute/path/new-window-exclusive-lease.json \
  --out /absolute/path/new-window

python3 tools/research/incremental-compilation/stage6_production_cli_analysis.py \
  --window /absolute/path/completed-new-window \
  --out /absolute/path/new-analysis
```

Windows are `generated-default`, `generated-closed`, `opaque-default` and
`opaque-closed`. All enable the new explicit incremental quality policy, O2,
after-MergeFunctions and the LLVM API; default/closed distinguish the export
contract. The two generated windows contain 1380 requests each; the two opaque
windows contain 360 each. Six blocks alternate full five-request sequences.

The active lease must bind `window`, absolute `out`, `production_manifest` and
`rust_reference` identities (path/SHA/bytes), `production_revision`, the exact
benchmark checkout `source_commit`, `planned_cli_requests`, and `started_utc`.
The coordinator preserves the active lease bytes. The lease owner releases it
after process cleanup and records `finished_utc`, `observed_requests` and
`invalid_requests`. An analyzer requires the completed execution and released
lease. No tool silently opens or releases a formal measurement lease.

Source/build/ELF/actual DSO/clang/namespace validation is shared with the runtime
tools. The profile uses the exact canonical production O2 pipeline; no private
tag is weakened or reused. Each window creates its own launcher and actual
compiler-PID loader probe. The pinned Rust ELF and original version output are
shared across runtime and CLI, rather than comparing version names alone.

The new opaque profile reuses the frozen source generator, oracle, complete-CLI
timer and native-output checker. `--control-only --blocks 1 --case ...` provides
explicit functional controls; these are not formal performance evidence. The
generated runner is reused without editing it. Both retain failures and all
native artifacts. Analysis independently propagates cache-history validity and
retains every comparison cell, adverse cell and invalid cell.

Preparation writes a minimal receipt before fallible input checks. Compiler
interruptions and post-compiler/native interruptions remain distinct from
unattempted requests. `AttemptedIncomplete` means compiler-request files exist
without a complete row; it does not infer a successful launch, exit, timing or
native result. A separate ledger preserves these cases without editing old
runner rows. Only requests with neither a row nor compiler-request evidence are
`NotRun`. Either category prevents a complete formal comparison.

On Linux, the coordinator acts as a subreaper and handles SIGINT/SIGTERM. It
first permits runner cleanup, then kills and reaps only descendants identified
through its own process tree and PID/start-time pairs, including descendants
that created another process group. Cleanup also runs after a normal or failed
runner exit. Leaked descendants, incomplete cleanup or an incomplete grid
prevent `CompletedRunner`; unrelated agent processes are outside its scope.

Every ZIL request exports actual/requested units, jobs and `fallback_present`
plus the full fallback value. A valid whole-module fallback remains in total
CLI timing and is not labeled partition success. Budget refusals remain in the
full denominator. Conditional valid-cell geometric means use paired six-block
medians and fixed-corpus bootstrap intervals; they do not establish a universal
Rust win. Native correctness checks remain distinct from runtime performance.
