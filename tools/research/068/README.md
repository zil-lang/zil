# Complete file-entry comparison

`main.rs` links the actual public `CompilationSession` and calls `compile_path`
twice, first with an empty session and then unchanged. Its interval includes
source reads and import discovery. It does not instrument or modify production
passes. `CompileReport.seconds`, the outer API interval, and controller process
wall are all retained separately.

`measure.py` compares two already-built runner binaries. It runs all three cases
in `tests/perf/impl068/cases.json`, alternating pair order, with one retained
warm-up and seven statistical rounds by default. Every request must execute
independent Verify exactly once. Full interfaces, source hashes, query events,
all non-executable artifact hashes, and specified native behavior must match.
Native behavior checks happen after timed runner execution.

The frozen production comparison is:

- baseline: `400a082444275e1d0e2ad78c4910b9b6fc526be5`;
- candidate: `0f59d0ad176aac7b2ff0b9b7a4b1f951dacb7879`;
- schema/wire: 10;
- Rust 1.99.1 and clang 23.1.5;
- release, codegen-units=1, debug=false, incremental=0, jobs=1.

To reproduce, materialize `Cargo.toml`, `Cargo.lock`, and `crates` from each
complete commit into two isolated directories. Copy the identical committed
`tools/research/068` runner into each directory so its relative crate dependencies
resolve there. Build both using `cargo build --offline --locked --release
--manifest-path tools/research/068/Cargo.toml --target-dir NEW_TARGET`.
Use the frozen candidate's input files for both binaries, at the **same absolute
paths**. Do not substitute later language examples or schema11 production code
and relabel that run as this result.

Once compilation and correctness preparation have finished, arrange a sampling
window with no other project build/native/graph benchmark activity, then run:

```sh
python tools/research/068/measure.py \
  --source /absolute/frozen-candidate \
  --baseline-binary /absolute/baseline-target/release/pipeline068 \
  --candidate-binary /absolute/candidate-target/release/pipeline068 \
  --baseline-commit 400a082444275e1d0e2ad78c4910b9b6fc526be5 \
  --candidate-commit 0f59d0ad176aac7b2ff0b9b7a4b1f951dacb7879 \
  --output /absolute/new-output-directory \
  --warmup 1 --rounds 7
```

`--source` must be a Git checkout containing the complete frozen commit objects.
An output directory must not already exist; failures are retained, not overwritten.
The full-history companion contains those objects. This experiment defines cold
as an empty compiler session; it does not flush OS file pages. Do not infer
allocation or peak-memory improvements from its wall-clock output.

The first preflight used the correct baseline binary but a mistyped metadata
label. It is preserved as an invalid identity record and excluded from formal
timing claims. The final controller validates complete commit objects before
starting a measurement. See `docs/research/068/results.md` for all regressions and
`evidence/impl/068` for the exact hashes and portable raw archive identity.
