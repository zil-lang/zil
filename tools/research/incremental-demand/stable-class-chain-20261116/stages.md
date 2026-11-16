# Restart compile stage observations

- Original request wall only: ZIL resident send-to-response versus Cargo/rustc request; startup, cleanup and runtime gates are separate.
- Pair within each window/preset/block/request and exact source identity; never pool windows or subtract independently computed medians.
- Positive candidate_minus_rust_ms means candidate compilation waited longer; negative values are retained. No runtime/compile speed conversion.
- Phase parents, backend children and driver buckets are separate views. Backend total is inside codegen, not additional work.
- Residuals are computed per sample without clipping. Medians of components need not sum to the median total.
- Candidate stages describe observed work, not guaranteed removable time or measured attribution of Rust's internal stages.
- Missing counters/timers are unknown, never inferred as zero or reuse. Call-class counts describe completed output, not current work.
- Report phase totals can include retries; native_codegen timings describe the reported final backend build. The signed codegen residual does not invent earlier-attempt attribution.
- backend.native_history_* describes SDK native_products opaque-product history, not the four MachineProduct slots; disabled SDK history says nothing about lower/transform/partition reuse.
- Machine-chain reuse is observed only through reused_machine_module, reused_machine_transforms, reused_partition and reused_object_keys. A long workflow can evict cold before restore; no hit is presumed.
- Main ResourceLimit is a failed observation, excluded from successful compile comparisons; runtime qualification is copied separately, never inferred.

## /workspace/zil-push-main/out/runs/stable-class-performance-retain-04

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": false, "candidate_rust_accepted": true, "main_regression_accepted": null, "runtime_profile_present": true, "runtime_only_rust_efficiency_geomean": 6.6323442516469235, "cases": [{"case": "2500/cold", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/unchanged", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/warm-restart", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-001", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/recovery", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "5105/cold", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/unchanged", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/warm-restart", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-001", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/recovery", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}]}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 2500 | cold | 2 | 1638.636 | 1618.230 | 20.406 |
| 2500 | unchanged | 2 | 30.469 | 34.438 | -3.970 |
| 2500 | warm-restart | 2 | 1059.549 | 26.769 | 1032.780 |
| 2500 | private-001 | 2 | 412.056 | 487.157 | -75.101 |
| 2500 | error | 2 | 479.634 | 499.221 | -19.587 |
| 2500 | recovery | 2 | 987.536 | 1728.828 | -741.293 |
| 5105 | cold | 2 | 6044.903 | 7471.634 | -1426.731 |
| 5105 | unchanged | 2 | 38.632 | 38.626 | 0.006 |
| 5105 | warm-restart | 2 | 4412.770 | 25.700 | 4387.070 |
| 5105 | private-001 | 2 | 1393.867 | 1412.727 | -18.860 |
| 5105 | error | 2 | 1691.984 | 1665.176 | 26.808 |
| 5105 | recovery | 2 | 2720.737 | 5838.962 | -3118.225 |

Eight-private totals use complete paired blocks only:

- 2500: C − Rust unknown ms; 0 complete blocks, 2 incomplete.
- 5105: C − Rust unknown ms; 0 complete blocks, 2 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2500/cold | 156.540 | 218.204 | 34.895 | 623.158 | 314.120 | 105.333 | 121.712 | 458.269 | 2.285 |
| 2500/unchanged | unknown | 0.045 | 0.004 | 1.550 | 0.342 | 0.080 | 0.679 | 26.836 | 2.029 |
| 2500/warm-restart | 45.593 | 30.539 | 17.002 | 487.865 | 228.044 | 72.873 | 6.018 | 377.267 | 3.849 |
| 2500/private-001 | 0.220 | 0.247 | 12.812 | 291.250 | 78.686 | 46.336 | 102.873 | 77.279 | 3.067 |
| 2500/error | 36.181 | unknown | unknown | unknown | unknown | unknown | unknown | 87.467 | 1.523 |
| 2500/recovery | 21.075 | 20.292 | 24.861 | 662.550 | 357.779 | 141.756 | 106.379 | 239.602 | 2.445 |
| 5105/cold | 362.239 | 204.124 | 120.646 | 2449.678 | 1306.640 | 546.249 | 448.441 | 2109.986 | 7.765 |
| 5105/unchanged | unknown | 0.044 | 0.050 | 2.284 | 0.781 | 0.082 | 0.793 | 33.940 | 2.295 |
| 5105/warm-restart | 169.005 | 115.234 | 77.992 | 1940.756 | 1009.059 | 764.954 | 14.947 | 1623.326 | 2.743 |
| 5105/private-001 | 0.174 | 0.162 | 38.884 | 1103.802 | 326.888 | 161.751 | 552.036 | 215.273 | 2.499 |
| 5105/error | 123.423 | unknown | unknown | unknown | unknown | unknown | unknown | 197.093 | 1.684 |
| 5105/recovery | 85.371 | 74.564 | 103.335 | 1864.123 | 923.046 | 570.989 | 265.792 | 554.490 | 2.177 |

ResourceLimit observations: 12; other invalid rows: 0.

## /workspace/zil-push-main/out/runs/stable-class-performance-native-04

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": false, "candidate_rust_accepted": true, "main_regression_accepted": null, "runtime_profile_present": true, "runtime_only_rust_efficiency_geomean": 4.008015302259675, "cases": [{"case": "1000/cold", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/unchanged", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-001", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-002", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-003", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-004", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-005", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-006", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-007", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-008", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/unchanged-after-edits", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/positions", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/interface", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/broad", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/recovery", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/restore", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/warm-restart", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/cold", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/unchanged", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-001", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-002", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-003", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-004", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-005", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-006", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-007", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-008", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/unchanged-after-edits", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/positions", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/interface", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/broad", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/recovery", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/restore", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/warm-restart", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "5105/cold", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/unchanged", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-001", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-002", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-003", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-004", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-005", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-006", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-007", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-008", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/unchanged-after-edits", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/positions", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/interface", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/broad", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/recovery", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/restore", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/warm-restart", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}]}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 1000 | cold | 2 | 892.607 | 1119.235 | -226.627 |
| 1000 | unchanged | 2 | 80.039 | 36.266 | 43.773 |
| 1000 | private-001 | 2 | 423.455 | 434.657 | -11.202 |
| 1000 | private-002 | 2 | 362.552 | 874.765 | -512.213 |
| 1000 | private-003 | 2 | 652.261 | 466.245 | 186.016 |
| 1000 | private-004 | 2 | 418.460 | 517.468 | -99.008 |
| 1000 | private-005 | 2 | 358.360 | 445.800 | -87.440 |
| 1000 | private-006 | 2 | 442.784 | 529.572 | -86.788 |
| 1000 | private-007 | 2 | 491.283 | 555.756 | -64.473 |
| 1000 | private-008 | 2 | 388.729 | 516.000 | -127.271 |
| 1000 | unchanged-after-edits | 2 | 16.382 | 28.841 | -12.459 |
| 1000 | positions | 2 | 184.006 | 451.563 | -267.557 |
| 1000 | interface | 2 | 553.528 | 666.634 | -113.106 |
| 1000 | broad | 2 | 738.065 | 971.484 | -233.418 |
| 1000 | error | 2 | 31.000 | 325.056 | -294.056 |
| 1000 | recovery | 2 | 361.354 | 475.129 | -113.775 |
| 1000 | restore | 2 | 675.262 | 1069.930 | -394.668 |
| 1000 | warm-restart | 2 | 900.153 | 36.570 | 863.583 |
| 2500 | cold | 2 | 1521.560 | 1586.719 | -65.159 |
| 2500 | unchanged | 2 | 50.599 | 33.550 | 17.049 |
| 2500 | private-001 | 2 | 593.865 | 535.604 | 58.260 |
| 2500 | private-002 | 2 | 394.497 | 594.088 | -199.591 |
| 2500 | private-003 | 2 | 539.640 | 478.943 | 60.697 |
| 2500 | private-004 | 2 | 393.085 | 492.052 | -98.967 |
| 2500 | private-005 | 2 | 392.264 | 555.331 | -163.067 |
| 2500 | private-006 | 2 | 399.268 | 528.113 | -128.845 |
| 2500 | private-007 | 2 | 444.629 | 625.399 | -180.770 |
| 2500 | private-008 | 2 | 395.510 | 499.649 | -104.139 |
| 2500 | unchanged-after-edits | 2 | 56.390 | 31.961 | 24.430 |
| 2500 | positions | 2 | 495.753 | 575.083 | -79.330 |
| 2500 | interface | 2 | 967.817 | 962.582 | 5.235 |
| 2500 | broad | 2 | 1357.897 | 1188.865 | 169.031 |
| 2500 | error | 2 | 52.801 | 240.988 | -188.188 |
| 2500 | recovery | 2 | 404.402 | 486.092 | -81.691 |
| 2500 | restore | 2 | 946.807 | 1223.060 | -276.254 |
| 2500 | warm-restart | 2 | 1089.283 | 16.978 | 1072.305 |
| 5105 | cold | 2 | 5451.515 | 6873.672 | -1422.157 |
| 5105 | unchanged | 2 | 42.584 | 22.813 | 19.771 |
| 5105 | private-001 | 2 | 1633.735 | 1656.195 | -22.460 |
| 5105 | private-002 | 2 | 1444.292 | 1350.607 | 93.685 |
| 5105 | private-003 | 2 | 1513.701 | 1308.369 | 205.333 |
| 5105 | private-004 | 2 | 1734.011 | 1533.807 | 200.205 |
| 5105 | private-005 | 2 | 1422.798 | 1240.250 | 182.548 |
| 5105 | private-006 | 2 | 1353.874 | 1536.222 | -182.348 |
| 5105 | private-007 | 2 | 1603.014 | 1786.293 | -183.279 |
| 5105 | private-008 | 2 | 1423.745 | 1396.325 | 27.419 |
| 5105 | unchanged-after-edits | 2 | 27.034 | 26.142 | 0.892 |
| 5105 | positions | 2 | 1342.778 | 1842.342 | -499.564 |
| 5105 | interface | 2 | 2867.571 | 2844.826 | 22.744 |
| 5105 | broad | 2 | 4153.483 | 6085.594 | -1932.111 |
| 5105 | error | 2 | 149.592 | 801.188 | -651.596 |
| 5105 | recovery | 2 | 1387.065 | 1364.235 | 22.830 |
| 5105 | restore | 2 | 3603.296 | 6083.266 | -2479.970 |
| 5105 | warm-restart | 2 | 4193.909 | 36.855 | 4157.053 |

Eight-private totals use complete paired blocks only:

- 1000: C − Rust -802.380 ms; 2 complete blocks, 0 incomplete.
- 2500: C − Rust -756.422 ms; 2 complete blocks, 0 incomplete.
- 5105: C − Rust 321.103 ms; 2 complete blocks, 0 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1000/cold | 51.455 | 23.610 | 11.968 | 452.109 | 146.504 | 69.087 | 182.141 | 246.057 | 2.605 |
| 1000/unchanged | unknown | 0.121 | 0.098 | 4.798 | 1.044 | 0.426 | 2.233 | 66.591 | 8.406 |
| 1000/private-001 | 1.211 | 0.307 | 13.810 | 312.400 | 49.941 | 63.148 | 167.798 | 68.686 | 2.532 |
| 1000/private-002 | 0.203 | 0.168 | 9.364 | 285.971 | 49.740 | 39.259 | 174.722 | 51.275 | 2.248 |
| 1000/private-003 | 0.142 | 0.154 | 7.950 | 569.525 | 282.049 | 74.298 | 177.241 | 59.452 | 2.741 |
| 1000/private-004 | 0.321 | 0.244 | 10.655 | 326.529 | 51.164 | 75.121 | 170.549 | 60.234 | 2.683 |
| 1000/private-005 | 0.137 | 0.185 | 8.444 | 281.484 | 40.399 | 39.854 | 174.163 | 55.426 | 2.033 |
| 1000/private-006 | 0.624 | 0.492 | 15.798 | 279.403 | 47.340 | 39.245 | 157.090 | 105.659 | 5.054 |
| 1000/private-007 | 0.357 | 0.286 | 11.987 | 349.082 | 48.492 | 36.474 | 219.424 | 105.034 | 2.793 |
| 1000/private-008 | 0.489 | 0.258 | 13.973 | 259.306 | 50.304 | 31.560 | 154.781 | 84.543 | 2.661 |
| 1000/unchanged-after-edits | unknown | 0.035 | 0.004 | 1.213 | 0.179 | 0.050 | 0.427 | 13.482 | 1.644 |
| 1000/positions | 19.322 | 8.252 | 14.147 | 19.093 | 15.186 | 0.039 | 0.338 | 65.830 | 2.029 |
| 1000/interface | 15.516 | 8.451 | 12.697 | 323.270 | 79.168 | 64.368 | 171.180 | 93.971 | 2.265 |
| 1000/broad | 33.335 | 13.559 | 14.733 | 421.733 | 156.255 | 128.581 | 100.346 | 160.157 | 2.832 |
| 1000/error | 7.888 | unknown | unknown | unknown | unknown | unknown | unknown | 12.230 | 1.170 |
| 1000/recovery | 3.130 | 0.411 | 9.602 | 237.694 | 57.522 | 40.915 | 117.351 | 83.555 | 3.017 |
| 1000/restore | 46.867 | 13.316 | 16.725 | 338.638 | 164.062 | 124.845 | 5.415 | 156.119 | 3.520 |
| 1000/warm-restart | 24.922 | 16.738 | 10.168 | 276.032 | 137.371 | 64.499 | 8.575 | 473.454 | 3.237 |
| 2500/cold | 71.029 | 32.617 | 21.481 | 703.612 | 437.957 | 80.525 | 113.509 | 597.497 | 5.921 |
| 2500/unchanged | unknown | 0.082 | 0.036 | 3.403 | 0.489 | 0.136 | 0.999 | 44.553 | 2.521 |
| 2500/private-001 | 0.424 | 0.233 | 14.996 | 455.710 | 99.045 | 192.972 | 115.904 | 89.231 | 3.583 |
| 2500/private-002 | 0.155 | 0.208 | 16.183 | 260.556 | 94.351 | 47.957 | 93.322 | 84.308 | 2.478 |
| 2500/private-003 | 0.135 | 0.178 | 20.828 | 415.830 | 217.267 | 60.306 | 92.578 | 78.692 | 2.518 |
| 2500/private-004 | 0.257 | 0.168 | 14.169 | 279.912 | 95.564 | 57.354 | 92.544 | 69.073 | 3.071 |
| 2500/private-005 | 0.186 | 0.185 | 24.290 | 270.895 | 91.727 | 46.345 | 107.519 | 70.952 | 2.413 |
| 2500/private-006 | 0.482 | 0.217 | 15.321 | 268.877 | 84.522 | 48.492 | 106.525 | 82.120 | 2.493 |
| 2500/private-007 | 0.274 | 0.301 | 18.077 | 282.711 | 85.917 | 49.440 | 112.855 | 98.006 | 3.537 |
| 2500/private-008 | 0.416 | 0.329 | 19.713 | 267.211 | 81.101 | 48.899 | 105.774 | 65.679 | 2.131 |
| 2500/unchanged-after-edits | unknown | 0.050 | 0.043 | 2.718 | 0.480 | 0.044 | 1.117 | 49.780 | 3.795 |
| 2500/positions | 38.561 | 21.654 | 31.232 | 44.575 | 36.075 | 0.025 | 0.902 | 114.526 | 2.273 |
| 2500/interface | 38.586 | 18.974 | 129.395 | 397.751 | 202.825 | 68.481 | 110.392 | 184.271 | 2.880 |
| 2500/broad | 61.286 | 37.331 | 28.121 | 559.443 | 283.304 | 124.677 | 101.011 | 473.725 | 2.778 |
| 2500/error | 18.420 | unknown | unknown | unknown | unknown | unknown | unknown | 11.452 | 1.457 |
| 2500/recovery | 4.015 | 0.401 | 13.236 | 282.638 | 92.390 | 46.748 | 112.297 | 62.377 | 2.430 |
| 2500/restore | 40.064 | 28.562 | 29.562 | 426.745 | 241.688 | 134.784 | 4.796 | 299.605 | 3.579 |
| 2500/warm-restart | 65.673 | 33.127 | 19.510 | 321.330 | 193.436 | 62.124 | 5.517 | 227.224 | 2.500 |
| 5105/cold | 470.714 | 214.196 | 127.878 | 2681.628 | 1582.799 | 340.779 | 538.609 | 1391.174 | 5.152 |
| 5105/unchanged | unknown | 0.045 | 0.023 | 2.383 | 0.720 | 0.034 | 0.717 | 37.465 | 2.664 |
| 5105/private-001 | 0.506 | 0.164 | 44.234 | 1284.495 | 485.832 | 224.741 | 517.197 | 263.533 | 2.654 |
| 5105/private-002 | 0.152 | 0.109 | 41.485 | 1233.865 | 461.886 | 181.851 | 526.716 | 129.530 | 3.831 |
| 5105/private-003 | 0.162 | 0.110 | 45.971 | 1256.614 | 394.034 | 261.889 | 536.554 | 176.065 | 3.647 |
| 5105/private-004 | 0.210 | 0.112 | 41.189 | 1200.012 | 304.182 | 205.326 | 600.577 | 446.880 | 3.731 |
| 5105/private-005 | 0.318 | 0.137 | 49.449 | 1094.396 | 303.630 | 178.723 | 542.839 | 235.659 | 2.621 |
| 5105/private-006 | 0.252 | 0.142 | 46.692 | 1094.648 | 288.520 | 171.960 | 575.991 | 171.502 | 2.754 |
| 5105/private-007 | 0.212 | 0.197 | 52.535 | 1202.369 | 293.537 | 219.530 | 629.684 | 284.645 | 3.981 |
| 5105/private-008 | 0.172 | 0.145 | 41.930 | 1187.104 | 396.763 | 163.776 | 564.779 | 161.194 | 2.345 |
| 5105/unchanged-after-edits | unknown | 0.045 | 0.018 | 2.252 | 0.747 | 0.028 | 0.631 | 22.257 | 2.458 |
| 5105/positions | 129.915 | 54.318 | 99.927 | 152.183 | 119.711 | 0.069 | 0.647 | 234.454 | 2.357 |
| 5105/interface | 77.276 | 51.661 | 93.757 | 1680.090 | 647.042 | 442.608 | 527.960 | 384.289 | 4.782 |
| 5105/broad | 233.717 | 102.730 | 123.651 | 2160.671 | 1116.851 | 600.292 | 299.598 | 742.835 | 2.737 |
| 5105/error | 78.613 | unknown | unknown | unknown | unknown | unknown | unknown | 23.803 | 1.589 |
| 5105/recovery | 8.011 | 0.647 | 57.622 | 973.160 | 360.815 | 194.556 | 288.752 | 295.612 | 3.401 |
| 5105/restore | 279.915 | 90.730 | 139.053 | 1995.488 | 1225.369 | 617.217 | 11.976 | 487.791 | 3.919 |
| 5105/warm-restart | 175.258 | 116.140 | 556.632 | 1948.824 | 1417.750 | 357.080 | 16.399 | 775.988 | 2.935 |

ResourceLimit observations: 36; other invalid rows: 0.

## /workspace/zil-push-main/out/runs/stable-class-performance-check-5000-wide-04

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": null, "candidate_rust_accepted": null, "main_regression_accepted": null, "runtime_profile_present": false, "runtime_only_rust_efficiency_geomean": null, "cases": []}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 5000 | cold | 2 | 1191.292 | 1558.005 | -366.712 |
| 5000 | unchanged | 2 | 17.252 | 19.551 | -2.298 |
| 5000 | private-001 | 2 | 110.441 | 247.609 | -137.168 |
| 5000 | private-002 | 2 | 116.500 | 607.626 | -491.125 |
| 5000 | private-003 | 2 | 104.148 | 246.421 | -142.273 |
| 5000 | private-004 | 2 | 114.454 | 397.547 | -283.093 |
| 5000 | private-005 | 2 | 100.901 | 234.544 | -133.644 |
| 5000 | private-006 | 2 | 170.125 | 683.614 | -513.489 |
| 5000 | private-007 | 2 | 303.480 | 250.013 | 53.467 |
| 5000 | private-008 | 2 | 100.352 | 364.308 | -263.957 |
| 5000 | unchanged-after-edits | 2 | 9.364 | 15.268 | -5.904 |
| 5000 | positions | 2 | 946.803 | 881.476 | 65.328 |
| 5000 | broad | 2 | 1127.391 | 961.900 | 165.491 |
| 5000 | error | 2 | 61.748 | 306.964 | -245.216 |
| 5000 | recovery | 2 | 115.035 | 311.540 | -196.504 |
| 5000 | restore | 2 | 987.302 | 778.061 | 209.242 |
| 5000 | warm-restart | 2 | 1185.083 | 51.510 | 1133.574 |

Eight-private totals use complete paired blocks only:

- 5000: C − Rust -1911.281 ms; 2 complete blocks, 0 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 5000/cold | 142.077 | 76.717 | 47.806 | unknown | unknown | unknown | unknown | 709.248 | 2.570 |
| 5000/unchanged | unknown | 0.033 | 0.009 | unknown | unknown | unknown | unknown | 15.507 | 1.704 |
| 5000/private-001 | 0.213 | 0.152 | 23.216 | unknown | unknown | unknown | unknown | 57.816 | 1.543 |
| 5000/private-002 | 0.123 | 0.164 | 27.591 | unknown | unknown | unknown | unknown | 56.009 | 1.527 |
| 5000/private-003 | 0.144 | 0.099 | 24.255 | unknown | unknown | unknown | unknown | 52.833 | 1.579 |
| 5000/private-004 | 0.220 | 0.192 | 25.340 | unknown | unknown | unknown | unknown | 56.563 | 3.086 |
| 5000/private-005 | 0.125 | 0.214 | 24.090 | unknown | unknown | unknown | unknown | 50.326 | 1.570 |
| 5000/private-006 | 0.160 | 0.097 | 25.396 | unknown | unknown | unknown | unknown | 83.509 | 32.213 |
| 5000/private-007 | 0.347 | 0.264 | 29.881 | unknown | unknown | unknown | unknown | 192.609 | 9.683 |
| 5000/private-008 | 0.268 | 0.101 | 24.353 | unknown | unknown | unknown | unknown | 50.257 | 1.859 |
| 5000/unchanged-after-edits | unknown | 0.035 | 0.004 | unknown | unknown | unknown | unknown | 8.023 | 1.301 |
| 5000/positions | 60.002 | 36.071 | 62.218 | unknown | unknown | unknown | unknown | 502.803 | 2.689 |
| 5000/broad | 107.289 | 79.476 | 69.634 | unknown | unknown | unknown | unknown | 531.780 | 2.329 |
| 5000/error | 25.259 | unknown | unknown | unknown | unknown | unknown | unknown | 10.158 | 1.084 |
| 5000/recovery | 5.620 | 0.275 | 23.640 | unknown | unknown | unknown | unknown | 59.757 | 3.195 |
| 5000/restore | 126.721 | 66.051 | 64.313 | unknown | unknown | unknown | unknown | 438.374 | 7.391 |
| 5000/warm-restart | 103.061 | 76.535 | 44.621 | unknown | unknown | unknown | unknown | 291.602 | 2.068 |

ResourceLimit observations: 0; other invalid rows: 0.

