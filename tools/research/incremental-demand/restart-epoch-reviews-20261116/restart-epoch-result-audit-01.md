# Restart epoch result audit — completion record

**The compact receipts demonstrate that the diagnosed whole-chain first-private-edit miss is fixed for both measured sizes and both blocks. Restart is explicitly slower; the smaller observed restart-plus-edit totals are cross-window observations, not guaranteed algorithmic savings.** This closes the bounded result audit, not the parent optimization goal.

Read only `out/runs/restart-epoch-results-01.json`, the tracked `tools/research/incremental-demand/native-dependency-reviews-20261116/cache-time-space.json`, and the small producing script `/workspace/zil-recovery/restart-epoch-analyze-01.py`. The script was read, not executed. No raw response/window scan, build, test, benchmark, source/repository change or cache operation was performed. Only this external report was written; the ongoing full-native window was not inspected.

Current source: `bcf409b37c5e5733c57376c0b6123b76321574a9`; binary SHA-256 `57de85daefac2ec5f562d15f01bd4f54aafeb4f53838da975671cc49dd809bf2`. Prior retained-cache source: `f19fc847c5774a2472bd3af85019b5c937546010`. The compact prior observations agree with the tracked f19 **retain** data, not its separate clear-cache arm.

## Actual first-edit work

For preset 2500 (2,505 declared functions), blocks 0 and 1 each report one Types body check, zero context rebuilds, one sparse batch, and 2,504 retained functions. PlainCT checks one body and reuses 2,504, with zero fallbacks. Analyze executes one product function query and reuses 2,504. Backend lowering, representation checking and source projection likewise each execute for one function and reuse 2,504.

For 5,105 functions, both blocks have the same pattern with **5,104** reused functions. These are actual consumer counters, not merely a `cache_hit` label or low-level Analyze initialization reuse. All four restart records say `hydrated`; nonzero current restart phase timings remain visible. The previously completed source review established that this status requires real current producer/RawInference/PlainCT/Types/Analyze checkpoint associations and full Typed anchor equality.

The producing script additionally asserts restart and first-edit `verification_runs == 1`, and full restart Types body production at each size. Those values are not copied into the compact per-block records, so this bounded audit verifies the script's assertions as written, without independently reopening their raw inputs. Private-response paths and SHA-256 bindings are present in all four compact records; their raw files were not rehashed here.

The counters do **not** mean all native work disappears: all 16 partition units are rebuilt, whole-machine/transform/partition/link reuse remains false, and native-history restored units remain zero. This is genuine source production followed by current in-memory reuse, not persisted Source→Machine authorization.

## Costs remain honestly labeled

Times below are seconds, rounded; each pair is restart plus first edit within the original compile-request timing scope.

| Functions | ZIL restart, f19 → bcf | ZIL first edit, f19 → bcf | ZIL pair, f19 → bcf | Current Rust restart / edit / pair |
| --- | --- | --- | --- | --- |
| 2,505 | 0.4231 → 0.7073 | 0.7470 → 0.3789 | 1.1701 → 1.0863 | 0.0251 / 0.4379 / 0.4630 |
| 5,105 | 1.5971 → 2.7251 | 2.7727 → 1.2230 | 4.3697 → 3.9481 | 0.0189 / 1.2854 / 1.3042 |

**Restart regresses by about 284 ms at 2,505 functions and 1,128 ms at 5,105.** Including that cost, the observed pair reductions versus f19 are only about 84 ms (7.16%) and 422 ms (9.65%), respectively. The much larger first-edit reduction must not be reported as the joint saving.

The script computes current pair medians from each block's restart-plus-edit sum. It obtains the old pair by adding the two f19 request medians; with exactly two blocks and the ordinary median this has the same arithmetic value as the median of the two block sums. The compact current pairs agree with the displayed request medians. No restart work has been dropped or renamed as an excluded preparation phase.

These are **different windows**, not simultaneous paired candidates. Rust's first-edit medians also decrease from 501.587 to 437.854 ms and from 1,508.134 to 1,285.350 ms. Its corresponding pairs decrease from 533.465 to 462.998 ms and from 1,531.997 to 1,304.249 ms. That control drift prevents attributing the entire ZIL cross-window reduction to this change. Current ZIL first edits are below current Rust in these cells, but current ZIL restarts and restart-plus-edit pairs remain substantially slower. No all-Rust win or guaranteed net saving follows.

The protocol retains original O2/CGU16/jobs1/default-budget/current-Verify compile timers. Startup/cleanup, 257 functional inputs and runtime samples remain outside those timers; the pairs are not full user end-to-end totals. No new idle/background preparation is identified by the audited production path. Disk figures are request-boundary physical peaks, not transient peaks or peak memory.

## Completion scope

The compact quality record says `execution_complete=true`, 72 rows, 24 quality cells, 10 source-bound runtime cases and `candidate_rust_accepted=true`, while **overall `accepted=false` remains unchanged**. It must not be relabeled as universal acceptance. Root separately reports all frozen-source unified gates passed; this audit did not rerun or independently inspect those gate receipts. The full-native run and parent goal remain in progress. No new design or follow-on task is introduced by this audit.

Read-input SHA-256 bindings: compact results `8b833d7e9f2a2d70a2ce4591d040ac3198d6659b81248bcb67737027745bae13`; tracked f19 cache JSON `d761876054b19d733fa86ba19603999bc4b900277b0d40246e0e2245cea02e7f`; producing script `5386a92873362fbd47b3d38e1e796084b7c310a9db1ab92e34bced2a8f57d6b2`.
