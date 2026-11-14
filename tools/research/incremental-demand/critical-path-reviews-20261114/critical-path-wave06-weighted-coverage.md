# Critical-path wave06 weighted coverage

State: **analyzed**. Candidate `789f879f249c7f45a5c84ce062be30f9c0985740`; main `9170bb4210554392e36eb7528596612596ebef80`; frozen tool `ad81486b976634e71bf1c61544d0fd28a783fbae`.

Sum the median wall time of each actual request; positive savings = reference − candidate. `broad` means the named broad request; error/recovery remain separate requests in other. No cross-window aggregate or industrial frequency assumption.

Predeclared expectation (not an observation): large2500 cold normalized_functions=0 while cheap-shape/local-name-profile selection may retain 2,092 cold reuse events. This expectation never fills observations or authorizes extra MC hits; actual receipt counts decide.

| Window | State | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-06-o2 | valid | 10.622 | 7.231 | 7.906 | 3.391 | 0.675 |
| large-project-large1000-edits-06-o2 | failed | — | — | — | — | — |

**Final wave06 evidence: 1/2 valid windows, 1 failed; 105 recorded matrix rows.** Results belong only to `789f879f249c7f45a5c84ce062be30f9c0985740`. Check04 retains its original 5a source; any later code receives none of these measurements.

Matrix composition: large-project-large2500-06-o2: 102 recorded/102 expected (valid); large-project-large1000-edits-06-o2: 3 recorded/246 expected (failed). Capacity06 is counted separately.

Gate23 recorded all_gates_valid=True; release and both measured window pins bind binary `e9bcf50ceb40a3f5e86b9e5abd4793a4302e9e92b279917b1f4da7b49d693274`. This is a receipt consistency check, not an additional compiler execution.

All 1 valid workflow totals favor the candidate over main and Rust. Signed request losses remain below; workflow gains do not imply every cold/broad request wins.

**Negative result: all 23 matched request/block reuse-count deltas against 04 are zero.** The cost-filter candidate added no observed reuse events in these inputs. This is a counter result, not a causal performance comparison between the two windows.


## Observed native emission reuse totals

These are dynamic events, not unique source functions. Counts sum only actual non-fallback emission receipts; missing/cached receipts are not filled with zero. Failed-window counts are diagnostic observations, not accepted performance results.

| Window | Block | Receipt requests | Receipt units | Reuse events | Normalized functions | Fallback units |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-06-o2 | 0 | 11 | 41 | 5516 | 10 | 0 |
| large-project-large2500-06-o2 | 1 | 11 | 41 | 5516 | 10 | 0 |
| large-project-large1000-edits-06-o2 | 0 | 1 | 16 | 0 | 38 | 0 |

Cold/broad normalization must not be generalized to all edits. Every nonzero observed normalization count is retained here; failed-window rows are diagnostic only.

| Window | Request | Block | Normalized functions | Reused events | Selection ms (sum of unit receipts) |
| --- | --- | ---: | ---: | ---: | ---: |
| large-project-large2500-06-o2 | private-002 | 0 | 2 | 148 | 6.877 |
| large-project-large2500-06-o2 | private-002 | 1 | 2 | 148 | 5.566 |
| large-project-large2500-06-o2 | private-003 | 0 | 2 | 148 | 5.807 |
| large-project-large2500-06-o2 | private-003 | 1 | 2 | 148 | 14.694 |
| large-project-large2500-06-o2 | private-005 | 0 | 2 | 148 | 6.381 |
| large-project-large2500-06-o2 | private-005 | 1 | 2 | 148 | 5.545 |
| large-project-large2500-06-o2 | private-006 | 0 | 2 | 148 | 5.122 |
| large-project-large2500-06-o2 | private-006 | 1 | 2 | 148 | 5.635 |
| large-project-large2500-06-o2 | recovery | 0 | 2 | 148 | 5.056 |
| large-project-large2500-06-o2 | recovery | 1 | 2 | 148 | 5.262 |
| large-project-large1000-edits-06-o2 | cold | 0 | 38 | 0 | 50.296 |

Totals and phase costs above/below come only from actual 06 receipts. No 04 native counts or first-cold observations are filled into missing 06 requests.

## Cost-filter candidate: observed added hits versus wave04

Count deltas retain both source identities and do not establish causal time savings. A zero delta is an explicit negative result for added reuse on that input.

| Window | Request | Block | 04 reuses (5a) | 06 reuses (789) | Added events | 06 fallback units |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-06-o2 | cold | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-06-o2 | cold | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-06-o2 | private-001 | 0 | 148 | 148 | 0 | 0 |
| large-project-large2500-06-o2 | private-001 | 1 | 148 | 148 | 0 | 0 |
| large-project-large2500-06-o2 | broad | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-06-o2 | broad | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large1000-edits-06-o2 | cold | 0 | 0 | 0 | 0 | 0 |

## large-project-large2500-06-o2

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 2.301 | 2.354 | 1.426 | -0.054 | -0.928 |
| broad | 1 | 2.199 | 1.802 | 1.238 | 0.397 | -0.564 |
| private | 8 | 3.807 | 1.770 | 3.100 | 2.037 | 1.330 |
| other | 7 | 2.315 | 1.305 | 2.141 | 1.010 | 0.837 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 26.906 | 26.728 | 8 | 14.875 |
| candidate | 12.504 | 11.145 | 8 | 1.094 |
| rust | 42.508 | 42.327 | 8 | 11.527 |

| Request | Candidate reused functions by block | Normalized functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- | --- |
| cold | `[2092, 2092]` | `[0, 0]` | `[0, 0]` | `[{"parse": 0.265, "expand": 155.909, "types": 80.872, "analyze": 55.27, "verify": 32.83, "lower": 1.185, "codegen": 1730.36, "ct": 20.649}, {"parse": 0.175, "expand": 76.43, "types": 36.812, "analyze": 27.692, "verify": 18.171, "lower": 0.91, "codegen": 1983.912, "ct": 14.293}]` |
| private-001 | `[148, 148]` | `[0, 0]` | `[0, 0]` | `[{"parse": 0.06, "expand": 14.976, "types": 0.68, "analyze": 0.312, "verify": 11.677, "lower": 0.013, "codegen": 163.818, "ct": 6.641}, {"parse": 0.058, "expand": 7.767, "types": 0.254, "analyze": 0.101, "verify": 10.68, "lower": 0.011, "codegen": 129.219, "ct": 4.333}]` |
| private-002 | `[148, 148]` | `[2, 2]` | `[0, 0]` | `[{"parse": 0.053, "expand": 7.426, "types": 0.093, "analyze": 0.209, "verify": 9.44, "lower": 0.009, "codegen": 127.918, "ct": 4.128}, {"parse": 0.063, "expand": 8.733, "types": 0.13, "analyze": 0.188, "verify": 11.314, "lower": 0.01, "codegen": 133.657, "ct": 3.935}]` |
| private-003 | `[148, 148]` | `[2, 2]` | `[0, 0]` | `[{"parse": 0.056, "expand": 6.622, "types": 0.096, "analyze": 0.095, "verify": 9.773, "lower": 0.009, "codegen": 128.66, "ct": 4.164}, {"parse": 0.06, "expand": 7.451, "types": 0.095, "analyze": 0.092, "verify": 9.739, "lower": 0.01, "codegen": 141.956, "ct": 3.927}]` |
| private-004 | `[148, 148]` | `[0, 0]` | `[0, 0]` | `[{"parse": 0.058, "expand": 6.386, "types": 0.25, "analyze": 0.121, "verify": 9.562, "lower": 0.01, "codegen": 118.699, "ct": 4.529}, {"parse": 0.06, "expand": 8.381, "types": 0.181, "analyze": 0.098, "verify": 11.71, "lower": 0.009, "codegen": 125.371, "ct": 5.894}]` |
| private-005 | `[148, 148]` | `[2, 2]` | `[0, 0]` | `[{"parse": 0.059, "expand": 6.598, "types": 0.133, "analyze": 0.123, "verify": 10.142, "lower": 0.009, "codegen": 126.075, "ct": 4.384}, {"parse": 0.085, "expand": 11.314, "types": 0.141, "analyze": 0.094, "verify": 13.927, "lower": 0.009, "codegen": 130.298, "ct": 4.84}]` |
| private-006 | `[148, 148]` | `[2, 2]` | `[0, 0]` | `[{"parse": 0.116, "expand": 8.021, "types": 0.134, "analyze": 0.098, "verify": 9.712, "lower": 0.009, "codegen": 145.641, "ct": 4.557}, {"parse": 0.074, "expand": 7.67, "types": 0.139, "analyze": 0.094, "verify": 9.879, "lower": 0.01, "codegen": 304.269, "ct": 4.271}]` |
| private-007 | `[148, 148]` | `[0, 0]` | `[0, 0]` | `[{"parse": 0.068, "expand": 6.893, "types": 0.126, "analyze": 0.2, "verify": 9.177, "lower": 0.011, "codegen": 119.259, "ct": 5.021}, {"parse": 0.124, "expand": 9.385, "types": 0.26, "analyze": 0.099, "verify": 16.159, "lower": 0.014, "codegen": 130.087, "ct": 6.357}]` |
| private-008 | `[148, 148]` | `[0, 0]` | `[0, 0]` | `[{"parse": 0.066, "expand": 6.257, "types": 0.123, "analyze": 0.134, "verify": 9.124, "lower": 0.011, "codegen": 116.672, "ct": 3.818}, {"parse": 0.075, "expand": 9.499, "types": 0.19, "analyze": 0.153, "verify": 9.825, "lower": 0.01, "codegen": 149.27, "ct": 4.859}]` |
| broad | `[2092, 2092]` | `[0, 0]` | `[0, 0]` | `[{"parse": 0.41, "expand": 116.375, "types": 47.388, "analyze": 34.164, "verify": 25.165, "lower": 2.115, "codegen": 1440.426, "ct": 25.294}, {"parse": 0.392, "expand": 67.708, "types": 20.385, "analyze": 20.881, "verify": 22.895, "lower": 2.084, "codegen": 1397.949, "ct": 10.578}]` |
| recovery | `[148, 148]` | `[2, 2]` | `[0, 0]` | `[{"parse": 0.233, "expand": 10.609, "types": 2.765, "analyze": 0.214, "verify": 10.691, "lower": 0.051, "codegen": 123.955, "ct": 4.906}, {"parse": 0.093, "expand": 8.815, "types": 3.058, "analyze": 0.336, "verify": 10.671, "lower": 0.083, "codegen": 124.696, "ct": 5.056}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[None, None]` | `[{"types": 0.097, "analyze": 0.005, "verify": 19.571, "lower": 0.902, "codegen": 236.82}, {"types": 0.009, "analyze": 0.004, "verify": 25.876, "lower": 1.371, "codegen": 231.113}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large1000-edits-06-o2

Comparisons remain empty; no workflow or storage advantage is inferred.

`{"block": 0, "index": 0, "request": "cold", "arm": "rust", "error": "ValueError: three-arm request/native/CGU16/full-ELF quality failed"}`

Recorded rows: 3/246. Quality: `[{"schema": "native-link-exact-elf-reviewed-ir-v1", "valid": false, "semantics_equal": true, "executable_equal": false, "llvm_text_equal": false, "raw_llvm_equal": false, "llvm_equal_excluding_identity_comment": false, "per_unit_exports_equal_observed": true, "aggregate_zil_function_exports_equal": true, "native_oracles_valid": true, "physical_zil_units_valid": true, "llvm_equality_is_acceptance_prerequisite": false, "block": 0, "index": 0, "request": "cold"}]`

## Original5105 capacity

State: recorded; comparisons=[]; formal_ratio=null. Main ResourceLimit is a failed capacity request, never a fast timing reference. Historical Rust receipts remain explicitly historical.

Recorded complete=True, protocol_valid=True, candidate_full_source_verify_native_success=True.

| Arm | Mode/request | Status | Current Verify runs | Wall ms (diagnostic only) | Emission reuse events | Normalized functions |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| candidate | check/cold | Success | 1 | 1985.092 | None | None |
| candidate | check/warm | Success | 1 | 7.924 | None | None |
| candidate | o2/cold | Success | 1 | 10175.033 | 0 | 0 |
| candidate | o2/warm | Success | 1 | 12.916 | None | None |
| baseline | check/cold | ResourceLimit | 0 | 397.644 | None | None |
| baseline | check/warm | ResourceLimit | 0 | 649.628 | None | None |
| baseline | o2/cold | ResourceLimit | 0 | 410.985 | None | None |
| baseline | o2/warm | ResourceLimit | 0 | 296.889 | None | None |
| rust | check/historical-cold [historical receipt] | Success | None | — | None | None |
| rust | o2/cold | Success | None | — | None | None |
| rust | o2/warm | Success | None | — | None | None |

Only the recorded statuses, Verify counts, source-inventory flags, oracle observations and emission counters support capacity claims. This is not a matched performance window. Missing receipt counters remain unknown; no 04 capacity result is substituted.

Independently counted from capacity rows: 4/4 candidate Source requests have Success, Verify=1, one current Verify event and the complete 102-file inventory. Valid native expected/actual matches: `{"baseline": 0, "candidate": 24, "rust": 24}`; these process counts are separate from Verify runs and workflow timings.

## Explicit historical check04 references

Both checks below were measured at `5a1434573ecd2ce8aa233e6841e9181ff3d9fd9d`, not `789f879f249c7f45a5c84ce062be30f9c0985740`. Production differences are native/emission.rs plus the normalized_functions observation field/JSON in LLVM API lib.rs (verified during preparation); driver/frontend/transport/Verify are unchanged. Per user authorization they remain check-path context; this is not a fresh 789 check measurement or a binary/timing equivalence claim.

| Original check04 window | Measured source | Main s | 5a candidate s | Rust s |
| --- | --- | ---: | ---: | ---: |
| large-project-large1000-edits-04-check | 5a143457 | 6.986 | 2.569 | 7.340 |
| large-project-large2500-04-check | 5a143457 | 2.890 | 1.939 | 3.832 |

## Scope and limits

- Synthetic scalar fixtures, not industrial projects or industrial edit frequencies.
- Per-window current main/candidate/Rust only; no cross-window grand speedup.
- Two blocks unless the actual manifest states otherwise; no confidence claim.
- Resident ZIL response wall versus fresh Cargo/rustc; startup scopes differ.
- OS caches are not flushed; outside-timer disk/native scans may warm caches.
- Disk is recorded boundary storage, not transient build peak, RSS or SDK size.
- Physical bytes include recorded directory/symlink blocks; logical bytes use recorded inode deduplication.
- Missing counters are unknown, not zero; nested/parallel timings are not additive wall partitions.
- Emission reused_functions are current-request post-O2 body reuses, not incremental function cache hits.
- normalized_functions is an observed normalization count, not an extra emission reuse or MC hit; missing is unknown.
- Fallback receipts can include discarded-attempt work; retain fallback flags and ordinary object costs.
- 5105 capacity observations never enter workflow sums, timing ratios or disk advantage comparisons.
- No compiler/native quality gates are rerun by this analyzer; it checks recorded evidence consistency.

Prepared invocation (run only after the batch-done notice):

`python3 -B /workspace/zil-recovery/critical-path-wave06-weighted-analysis.py --analyze`
