# Independent final evidence review

Reviewer: `/root/review_incremental_slice1`.

**Verdict: Pass for scoped evidence integrity and statistical reconstruction.** Final implementation: `5cf1c417f5cbdc0895e5edceb3053d9d4da490c6`. This does not approve a product gate, Rust parity, general compile-speed breakthrough or formal full-suite acceptance. No benchmark was rerun.

## Complete windows

| Window | Candidate identity | Requests | ZIL requests | Native controls | Complete cells | Time-unfavorable cells |
|---|---|---:|---:|---:|---:|---:|
| stage2-frozen-v2 | d66a3142 | 1,560 | 1,360 | 620 | 44 | 13 |
| stage2-final-frozen | 646099a7 | 1,560 | 1,360 | 620 | 44 | 10 |
| stage2-corrected-frozen | 5cf1c417 | 1,560 | 1,360 | 620 | 44 | 7 |
| stage2-main-final | 5cf1c417 | 1,560 | 1,360 | 620 | 44 | 12 |

All expected positions, arms and request labels occur exactly once. CLI has 400 ZIL and 200 direct-rustc requests; Session has 960 requests. Each ZIL CLI raw report records Success and Verify=1. Each Session row records one executed Verify event, while the hash-bound probe asserts Success/verification_runs=1 before emitting that row.

Every cell was independently recomputed using exact fractions: within-block arm means, median of five block means, then candidate/baseline (and CLI candidate/rustc) ratio. Session private cells pool five consecutive new edits per arm, giving ten samples per arm/block rather than two. All **1,599 numeric fields** match, with maximum floating-point representation difference 1.42e-14. p95 and median RSS fields were also checked; no unfavorable cell was removed.

## Final main-to-candidate point estimates

| Session, capacity32 | Baseline ms | Candidate ms | Change |
|---|---:|---:|---:|
| 512 modules, unchanged check | 105.515 | 38.183 | −63.8% |
| 512 modules, private check | 108.016 | 104.204 | −3.5% |
| 128 modules, unchanged build | 57.034 | 43.331 | −24.0% |

The 512-module unchanged-check point estimate is approximately 2.76×. Private-check improvement in the final main window is 3.5%, not the more favorable 7.5% from the other final window. Initial 512-module private-check latency regressed about 4.9%, and its post-request RSS median rose from 65,220 to 91,242 KiB (about 39.9%); that window remains intact. Final main-window private-check RSS is 65,246→64,828 KiB. These are scoped observations, not confidence intervals or universal speed claims.

## Native, identity and mixed-source checks

Per window, all 300 CLI native stdout files match expected generated-source results. The other 320 Session native controls are probe assertions on actual exit/stdout; they do not have separately retained per-request stdout or CompileReport files. CLI native exit status is likewise asserted by the runner. The 60 final restore CLI ELF hashes per window match retained files; intermediate executables were overwritten, with their contemporaneous hashes remaining in rows.

All retained baseline/final compiler and historical/current probe hashes match manifests. First-two-window candidate CLI paths now contain a later rebuild; their old hashes are retained but cannot be rehashed at that mutable path. This limitation is explicit in RESULTS. Git comparison confirms 665a0a06→5d7c1eeb changes only tests/docs/evidence/tools, with no production src/Cargo/lock change.

Mixed-source evidence contains **320 measured builds/native controls plus 16 clang-reference builds/native controls**. All 336 raw reports have Success and Verify=1. Exactly ten old byte_io O2 executions return the wrong exit2 while matching stdout. Its speed ratio is null. All remaining case/optimization cells match the reference. The six unfavorable equivalent mixed cells remain recorded, including where Copy O2 at +5.14%.

## Validation log and documents

The scoped log has **486 passed, zero failed** test functions across 22 result groups; one zero-test group contributes no coverage. The affected-library Clippy command completes successfully under the checked runner's `-D warnings`. This is limited validation, not a formal repository-wide acceptance run.

All **192 MEASUREMENTS table rows** and **seven RESULTS main-table rows** match independently reconstructed values and rounding. The documents now locate the earlier +17.9% CLI regression in the memory-fix window and distinguish final large-capacity percentages from earlier windows.

- `docs/research/incremental-compilation/RESULTS.md` SHA256: `53730fdd521e1fd8e2e9b0312e834926ed6e5b668c32856d6715ecd04c05e69e`.
- `docs/research/incremental-compilation/MEASUREMENTS.md` SHA256: `58f24cc5561bb76acc06ef0c04ca0b8a18adbfcf487827431826a23c4e866af7`.

Original-file inventory: `final-evidence-review-inventory.json` (9,511 hashed records). Recomputable audit: `recompute_final_evidence.py`. Full cell-level reconstruction and all adverse cells: `final-evidence-recomputed.json`; machine-readable review: `final-evidence-review.json`.

## Interpretation limits

CLI ru_maxrss is child high-water RSS; Session VmRSS is sampled after each request. They cannot be combined into a peak-memory claim. Phase durations are intervals between observer events and include intervening driver work, not exclusive phase CPU costs. The Rust comparator is direct rustc with the stated O0/metadata/incremental/CGU flags, not Cargo or complete language/runtime parity. Four windows remain separate candidates/baselines and are not pooled or multiplied into a product result.
