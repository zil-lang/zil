# Independent functional review receipt

Reviewer: `/root/review_incremental_slice1`, independent of implementation author.

**Verdict: Pass, limited functional/rejection-boundary scope.** Exact implementation: `b230cfddcc12c3daf6ec4dcbca3a2dc11d9ebe79`; baseline: `5d7c1eeb2b74052eb239e19529dc5a943c6622a3`. The worktree was clean when bound. No unresolved correctness finding was identified. This is not formal full-suite, Rust-comparison or product acceptance.

Later commit `d66a3142099209b7c5a7884b46e46418fa60021e` changes only the benchmark timing wrapper; production and test sources are exactly unchanged, so this functional evidence is reusable. Performance results are not reviewed here.

## Findings and resolutions

- An intermediate worktree failed to compile because `compile_units` received an unborrowed String. The author restored the &str borrow; subsequent tests passed. Original failure remains in `combo-review-driver.log`.
- Parse usage metadata initially changed syntax Debug output. Final custom Debug preserves the original fields/order.
- Final raw-view controls reject a same-start-pointer slice of different length and data from another checked module even when its fingerprint is copied. Independently rerun at the exact commit.

## Reviewed boundaries

- CP33 neutralizes only token/node quotas for successful Parse identity, checks current receipt fit, and falls back to the original quota-specific parser on insufficiency. Downstream stages retain full limits, project charging remains active, and current source/import identity is rechecked.
- CP34 streams the exact v27 byte encoding, with frozen original-oracle and buffer-boundary controls.
- CP35 shares immutable raw Arcs. Every request still calls complete independent Verify with current target/limits; no Verified capability is cached.
- LRU eviction happens after requests. Raw and recency entries are removed together. Only dependencies reachable from raw roots survive; database reads still validate the full closure. Raw item count is bounded; byte size and receipt count are not capped by that capacity.
- LLVM preflight retains successful named subgraph heights per request. Active-stack checks precede cache hits. Insufficient depth returns to original ordered DFS; failures are not cached.
- Owned and borrowed LIR APIs share complete field binding. Exact slice aliasing includes length, other slices compare every element, and schema/module/target/fingerprint/layout/functions/closures/resources stay bound. A fingerprint is insufficient by itself.
- Expand constructs the aggregate index once with original module/item order and first-wins insertion. Ordinary downstream validation still runs.

## Actual limited commands

All Cargo commands ran in `/workspace/zil` after `. /workspace/.zil-env/env.sh`.

- `cargo test --offline --locked -p zil-ir --test cp34_fingerprint -p zil-syntax --test cp33_parse_receipts`: 6 passed; `combo-review-encoding.log`.
- `cargo test --offline --locked -p zil-driver --features llvm-api --test cp33_parse_budget --test cp34_fingerprint_sources --test cp35_shared_verify`: 15 passed; `combo-review-driver-retry.log`.
- `cargo test --offline --locked -p zil-driver --features llvm-api --lib bounded_history_tests`: 2 passed; `combo-review-history.log`.
- `cargo test --offline --locked -p zil-llvm-api --lib named_layout_tests`: 3 passed; `combo-review-named.log`.
- `cargo test --offline --locked -p zil-db --lib collecting_raw_roots_keeps_transitive_receipts_and_current_invalidation`: 1 passed; `combo-review-db.log`.
- `cargo test --offline --locked -p zil-backend-llvm --features llvm-api --lib borrowed_lir_aliases_bind_length_and_the_current_capability`: 1 passed; `combo-review-final-lir.log`.
- `cargo test --offline --locked -p zil-syntax --test cp33_parse_receipts`: 2 passed; `combo-review-final-syntax.log`.

Total: 30 executions, 28 unique test functions (syntax controls rerun). Earlier working-tree evidence is reused only for unchanged semantics; final edits were formatting/borrow cleanup, Debug preservation, negative controls, docs and tools. Final alias and syntax controls ran at the exact commit. No formal full suite was run.

## Independent old/new validator oracle

`combo-preflight-oracle.tar.gz` contains portable Rust sources, frozen old/candidate validators, machine model, exact seed/loop, output, hashes and run instructions. Actual commands: `rustc --edition=2024 -O probe.rs -o probe`, then `./probe`.

200,000 exact Result comparisons passed: 1,768 successes and 198,232 rejections. Depths were 0/1/2/123/124/125/126/127/128/129, with forward/reverse item order. Captured candidate and committed candidate matched after formatting. Committed preflight SHA256: `a72f366ae9a1eae72f213aa332b942b41bfc02a6aa945f4daa1b0bf5532336ea`. This is synthetic machine-IR correctness evidence, not source-level timing evidence.

Archive SHA256: `71baee9eb80abbfae9f5ec4d93aa66d82d32c35ba9fbb0c66e9edf93194e62bb`. `combo-review-receipt.json` records per-log hashes and structured evidence.

## Limits

No speedup is certified. Whole-project semantic recomputation, default-cache Parse eviction, linear current Verify, and machine construction before object hits remain. Rust four-goal/product conclusions need separate evidence.
