# Local-only baseline

Production source: d3b7b29b58250c2dec55aec31afeb90b9751d250. Only consumer
inputs/test code were modified for these observations. Rust 1.99/LLVM 23,
CARGO_TARGET_DIR=/workspace/.zil-env/target-287, CARGO_BUILD_JOBS=1.

The existing impl226_shared_product_capture target builds. The frozen-oracle test
passes 1/1. The legal-source Verify test fails: three direct controls pass Verify;
four legal parameter-capture sources stop at Analyze Unsupported, E-UNSUPPORTED,
`shared Product capture requires proven ordinary local Product origins`, with
zero verification runs. This is the original missing capability, not a negative
success. The negative-boundary test also fails: three direct controls reach
Verify SafeReject E-BORROW-CONFLICT; three captured cases stop at the earlier
Local-only gate. Actual rows are in baseline.tsv.

Commands (after sourcing /workspace/.zil-env/env.sh):

```sh
CARGO_TARGET_DIR=/workspace/.zil-env/target-287 CARGO_BUILD_JOBS=1 cargo test -p zil-driver --test impl226_shared_product_capture --no-run
CARGO_TARGET_DIR=/workspace/.zil-env/target-287 CARGO_BUILD_JOBS=1 cargo test -p zil-driver --test impl226_shared_product_capture parameter_capture::source_oracles_are_frozen
CARGO_TARGET_DIR=/workspace/.zil-env/target-287 CARGO_BUILD_JOBS=1 cargo test -p zil-driver --test impl226_shared_product_capture parameter_capture::frozen_shared_parameter_sources_reach_verify -- --nocapture
CARGO_TARGET_DIR=/workspace/.zil-env/target-287 CARGO_BUILD_JOBS=1 cargo test -p zil-driver --test impl226_shared_product_capture parameter_capture::same_store_invalidation_reaches_the_borrow_conflict_boundary -- --nocapture
```

Source-derived Parameter/raw/Wire and native O0/O2 remain NotRun here pending the
independent core slices. A later actual Analyze rejection for a captured conflict
must be recorded separately from each direct control's Verify rejection; no
arbitrary-phase allowance is authorized by this baseline record.
