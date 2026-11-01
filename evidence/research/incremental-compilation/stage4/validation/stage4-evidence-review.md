# Stage-3/4 independent evidence review

**Pass for scoped data integrity and statistics.** No benchmark rerun or product-gate acceptance. Stage-4 implementation: `9fe5f55e64c7d537b9d585d3d40e42c9e248308e`; stage-3 source: `5e0613cc4d09bc99dcd1fba00a371eed691f8a37` (ELF built at production-identical `a4bec09`).

| Window | Blocks | Rows | Cells | Native controls | Wall adverse | RSS adverse |
|---|---:|---:|---:|---:|---:|---:|
| stage3-main-final | 5 | 1560 | 44 | 620 | 6 | 10 |
| stage3-isolation | 5 | 1560 | 44 | 620 | 19 | 23 |
| stage4-main-final | 10 | 3120 | 44 | 1240 | 5 | 8 |
| stage4-isolation | 10 | 3120 | 44 | 1240 | 4 | 24 |

All 176 cells and 1,599 published numeric fields match independent Fraction-based reconstruction (maximum final floating difference 1.42e-14). Full expected arm/block/request sets match without omissions or duplicates. Five-block windows and ten-block windows use their own manifest sizes; even medians average the middle two block means.

Final main comparison: 512-module warm check **106.917496 → 13.443409 ms** (7.953×); private check **107.112810 → 94.652094 ms** (−11.633%); 128-module warm build **55.530534 → 35.478584 ms** (−36.110%). These are scoped medians, not universal speed claims.

All unfavorable cells are retained in the JSON/recomputed tables. Stage-3 isolation 512 private is **109.425980 → 124.151036 ms (+13.457%)**, and 32-module warm build is +2.742%. Final main still has five adverse wall cells, including 32-module private CLI build +4.604%. Mixed controls have six valid adverse cells, led by ct_selection219 O0 +5.256%.

Four windows contain 9,360 measured rows, 8,160 current ZIL Verify requests and 3,720 native controls. Mixed evidence adds 320 builds plus 16 text-path oracles, all Verify=1. Exactly ten old baseline byte_io O2 native executions are wrong; that cell remains semantically unequal and its ratio is null. Candidate results and all other cells match native controls.

All 3,840 stage-4 Session CPU fields are positive and match raw transcripts. CPU is process-thread CPU excluding child linkers/native controls. Phase timings remain intervals between observer callbacks. Source-hash/binary identities and 360 retained CLI final-restore ELF hashes match. Full 14,546-file SHA inventory and reproducible read-only script are attached; full original raw reports/native stdout were examined where preserved.

Session Verify/native controls are assertions in the hash-bound probe with phase records, not separately retained per-request full reports/stdout. CLI RSS is child high-water RSS; Session RSS is post-request VmRSS. Direct rustc metadata/build reference is not Cargo or four-goal product parity. No statistical-confidence or cross-window compounded-speed claim is made.

Same-source probe provenance is now independently checked: both probe-source Git blobs match 9fe, and all three reproduced ELF hashes match preserved measurement binaries and manifests. CURRENT’s seven main rows, RSS/CPU/count statements and all 192 wall/mixed plus 48 CPU table rows match the raw reconstruction. Verify event timing now distinguishes the 20-request raw median (9.953 ms) from the median of block means (10.045 ms). Historical stage-2 document hashes remain unchanged. Constructor’s 495-test log is counted separately from the prior independent source-review tests.

This statistical/document review is complete. The outer archive MANIFEST and subsequent exact delivery commit will receive a separate identity binding after freezing; no such delivery check is claimed here.

Frozen document SHA256:

- CURRENT.md: `9a23e8c560757dcb3c19fc9d493d77de5222a79b5724a2d054a2fd82081df3c9`
- LATEST-MEASUREMENTS.md: `c469a31b559971cf2ea37a27d60dbd33474a631239ea7b8e5d3d2f7e2a630fd8`
