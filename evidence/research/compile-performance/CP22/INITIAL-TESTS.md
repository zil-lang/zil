# CP22 initial implementation checks

Initial source/receipt/codec check: `CARGO_TARGET_DIR=/workspace/scratch/4ab361722127/target-cp22 cargo check -p zil-syntax -j 1` — exit 0.

Targeted syntax command: `CARGO_TARGET_DIR=/workspace/scratch/4ab361722127/target-cp22 cargo test -p zil-syntax --test parse_cache_cp22 -j 2` — exit 0, 4 passed, 0 failed. Observed tests:

- deep_valid_syntax_bypasses_memo_without_changing_parse_acceptance
- receipts_cover_parser_and_attribute_budgets_and_preserve_low_budget_errors
- malformed_or_incomplete_bytes_never_construct_a_successful_tree
- source_roundtrips_preserve_syntax_positions_attributes_and_tokens

Targeted driver command: `CARGO_TARGET_DIR=/workspace/scratch/4ab361722127/target-cp22 cargo test -p zil-driver --test parse_cache_cp22 -j 2` — exit 0, 6 passed, 0 failed. Observed tests:

- session_parse_keys_do_not_depend_on_preceding_module_consumption
- separate_sessions_reuse_unchanged_modules_after_early_token_count_edit
- failed_cancelled_or_invalid_artifact_requests_do_not_create_cache_directories
- module_reordering_missing_files_and_ambiguous_paths_use_current_discovery
- damaged_entries_fall_back_and_repair_without_skipping_verify
- quota_depth_and_source_byte_failures_match_uncached_first_diagnostics

These initial commands were observed in executor tool output; this file is an observation summary, not a saved raw cargo log. A first driver compilation found Option-returning callbacks used with TimingTrace::measure (which expects Result); switching these instrumentation points to TimingTrace::value fixed compilation before the 6-test pass.

Attempted affected syntax suite: `cargo test --offline --locked -p zil-syntax -j 2`; full raw output retained as `syntax-tests.log`. Exit 101 was ENOSPC during rustc/link output, not a test assertion failure. Own debug/incremental output was removed to release space; no unique evidence or other task's outputs were deleted.

Freeze note: after the initial 10-test pass, query-key construction was factored to clone only scalar Limits (preserving ordinary key bytes), resource receipt comments were clarified, and the failure-publication test gained a post-parse type-error assertion. These final edits still require the resumed exact-candidate check. Release build, cross-process CLI controls and performance measurement are pending. This checkpoint preserves recoverable source, not final acceptance.
