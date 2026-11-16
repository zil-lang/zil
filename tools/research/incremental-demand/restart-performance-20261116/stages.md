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

## /workspace/zil-push-main/out/runs/restart-performance-retain-01

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": false, "candidate_rust_accepted": true, "main_regression_accepted": null, "runtime_profile_present": true, "runtime_only_rust_efficiency_geomean": 6.4924649994707915, "cases": [{"case": "2500/cold", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/unchanged", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/warm-restart", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-001", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/recovery", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "5105/cold", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/unchanged", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/warm-restart", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-001", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/recovery", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}]}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 2500 | cold | 2 | 1248.188 | 1439.798 | -191.610 |
| 2500 | unchanged | 2 | 14.258 | 17.212 | -2.954 |
| 2500 | warm-restart | 2 | 650.100 | 49.372 | 600.729 |
| 2500 | private-001 | 2 | 406.797 | 397.793 | 9.004 |
| 2500 | error | 2 | 296.611 | 351.382 | -54.771 |
| 2500 | recovery | 2 | 647.301 | 1278.987 | -631.686 |
| 5105 | cold | 2 | 4028.748 | 5686.927 | -1658.179 |
| 5105 | unchanged | 2 | 18.781 | 14.362 | 4.419 |
| 5105 | warm-restart | 2 | 3196.505 | 13.264 | 3183.240 |
| 5105 | private-001 | 2 | 1244.866 | 995.086 | 249.780 |
| 5105 | error | 2 | 1079.357 | 1437.581 | -358.224 |
| 5105 | recovery | 2 | 2084.177 | 4996.141 | -2911.964 |

Eight-private totals use complete paired blocks only:

- 2500: C − Rust unknown ms; 0 complete blocks, 2 incomplete.
- 5105: C − Rust unknown ms; 0 complete blocks, 2 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2500/cold | 103.916 | 54.393 | 34.784 | 590.518 | 297.624 | 83.377 | 128.651 | 307.019 | 2.717 |
| 2500/unchanged | unknown | 0.030 | 0.008 | 1.138 | 0.195 | 0.081 | 0.422 | 11.579 | 1.499 |
| 2500/warm-restart | 66.896 | 23.858 | 17.365 | 303.396 | 169.467 | 57.983 | 6.222 | 143.285 | 1.915 |
| 2500/private-001 | 0.133 | 0.101 | 13.304 | 307.251 | 85.723 | 70.079 | 118.538 | 63.015 | 1.964 |
| 2500/error | 21.417 | unknown | unknown | unknown | unknown | unknown | unknown | 43.855 | 1.739 |
| 2500/recovery | 20.481 | 19.412 | 23.774 | 386.922 | 138.090 | 88.387 | 113.813 | 179.591 | 2.833 |
| 5105/cold | 411.745 | 174.147 | 118.140 | 1894.365 | 861.968 | 377.433 | 492.848 | 961.283 | 4.597 |
| 5105/unchanged | unknown | 0.038 | 0.004 | 1.413 | 0.474 | 0.012 | 0.570 | 15.485 | 1.836 |
| 5105/warm-restart | 270.103 | 152.205 | 105.006 | 1546.990 | 961.582 | 388.194 | 18.680 | 747.326 | 4.387 |
| 5105/private-001 | 0.172 | 0.115 | 28.853 | 1057.858 | 267.728 | 197.326 | 546.177 | 125.765 | 4.357 |
| 5105/error | 103.075 | unknown | unknown | unknown | unknown | unknown | unknown | 204.448 | 2.017 |
| 5105/recovery | 77.566 | 73.089 | 93.992 | 1426.949 | 704.983 | 291.914 | 323.603 | 373.738 | 5.625 |

ResourceLimit observations: 12; other invalid rows: 0.

## /workspace/zil-push-main/out/runs/restart-performance-native-01

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": false, "candidate_rust_accepted": true, "main_regression_accepted": null, "runtime_profile_present": true, "runtime_only_rust_efficiency_geomean": 4.1258358058669415, "cases": [{"case": "1000/cold", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/unchanged", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-001", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-002", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-003", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-004", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-005", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-006", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-007", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/private-008", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/unchanged-after-edits", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/positions", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/interface", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/broad", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/recovery", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/restore", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "1000/warm-restart", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/cold", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/unchanged", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-001", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-002", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-003", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-004", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-005", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-006", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-007", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-008", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/unchanged-after-edits", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/positions", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/interface", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/broad", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/recovery", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/restore", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/warm-restart", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "5105/cold", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/unchanged", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-001", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-002", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-003", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-004", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-005", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-006", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-007", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-008", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/unchanged-after-edits", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/positions", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/interface", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/broad", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/recovery", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/restore", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/warm-restart", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}]}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 1000 | cold | 2 | 961.939 | 1217.016 | -255.077 |
| 1000 | unchanged | 2 | 41.993 | 37.556 | 4.438 |
| 1000 | private-001 | 2 | 512.799 | 346.639 | 166.159 |
| 1000 | private-002 | 2 | 454.992 | 465.282 | -10.289 |
| 1000 | private-003 | 2 | 328.093 | 364.432 | -36.339 |
| 1000 | private-004 | 2 | 432.365 | 389.700 | 42.664 |
| 1000 | private-005 | 2 | 382.739 | 395.675 | -12.936 |
| 1000 | private-006 | 2 | 334.157 | 470.917 | -136.761 |
| 1000 | private-007 | 2 | 337.141 | 379.049 | -41.908 |
| 1000 | private-008 | 2 | 379.675 | 450.703 | -71.029 |
| 1000 | unchanged-after-edits | 2 | 12.745 | 16.258 | -3.512 |
| 1000 | positions | 2 | 270.386 | 380.995 | -110.609 |
| 1000 | interface | 2 | 441.932 | 592.226 | -150.294 |
| 1000 | broad | 2 | 772.017 | 1063.768 | -291.752 |
| 1000 | error | 2 | 24.380 | 156.691 | -132.312 |
| 1000 | recovery | 2 | 301.607 | 333.782 | -32.175 |
| 1000 | restore | 2 | 444.532 | 965.874 | -521.342 |
| 1000 | warm-restart | 2 | 732.253 | 20.114 | 712.138 |
| 2500 | cold | 2 | 1274.823 | 1566.181 | -291.358 |
| 2500 | unchanged | 2 | 49.624 | 24.773 | 24.852 |
| 2500 | private-001 | 2 | 420.977 | 473.003 | -52.026 |
| 2500 | private-002 | 2 | 365.726 | 426.413 | -60.688 |
| 2500 | private-003 | 2 | 353.440 | 413.857 | -60.417 |
| 2500 | private-004 | 2 | 399.726 | 423.094 | -23.368 |
| 2500 | private-005 | 2 | 402.036 | 616.092 | -214.056 |
| 2500 | private-006 | 2 | 349.115 | 442.316 | -93.202 |
| 2500 | private-007 | 2 | 604.429 | 545.949 | 58.480 |
| 2500 | private-008 | 2 | 462.855 | 672.955 | -210.100 |
| 2500 | unchanged-after-edits | 2 | 37.416 | 31.499 | 5.917 |
| 2500 | positions | 2 | 727.074 | 530.948 | 196.126 |
| 2500 | interface | 2 | 681.298 | 752.528 | -71.230 |
| 2500 | broad | 2 | 1599.150 | 1365.541 | 233.609 |
| 2500 | error | 2 | 54.200 | 217.449 | -163.249 |
| 2500 | recovery | 2 | 480.906 | 331.445 | 149.460 |
| 2500 | restore | 2 | 926.448 | 1125.420 | -198.972 |
| 2500 | warm-restart | 2 | 892.821 | 57.529 | 835.292 |
| 5105 | cold | 2 | 4594.763 | 6202.751 | -1607.988 |
| 5105 | unchanged | 2 | 78.208 | 18.606 | 59.602 |
| 5105 | private-001 | 2 | 1283.129 | 1206.714 | 76.415 |
| 5105 | private-002 | 2 | 1232.791 | 1118.642 | 114.148 |
| 5105 | private-003 | 2 | 1287.618 | 1159.447 | 128.171 |
| 5105 | private-004 | 2 | 1317.304 | 1090.439 | 226.865 |
| 5105 | private-005 | 2 | 1204.470 | 1076.887 | 127.583 |
| 5105 | private-006 | 2 | 1255.106 | 921.335 | 333.771 |
| 5105 | private-007 | 2 | 1128.825 | 1111.362 | 17.463 |
| 5105 | private-008 | 2 | 1122.711 | 1003.586 | 119.125 |
| 5105 | unchanged-after-edits | 2 | 52.680 | 19.722 | 32.958 |
| 5105 | positions | 2 | 1650.002 | 1694.990 | -44.988 |
| 5105 | interface | 2 | 1960.946 | 2107.569 | -146.622 |
| 5105 | broad | 2 | 3465.448 | 5803.278 | -2337.830 |
| 5105 | error | 2 | 118.529 | 395.083 | -276.554 |
| 5105 | recovery | 2 | 1336.126 | 891.117 | 445.009 |
| 5105 | restore | 2 | 2825.758 | 5717.806 | -2892.048 |
| 5105 | warm-restart | 2 | 3562.134 | 42.525 | 3519.609 |

Eight-private totals use complete paired blocks only:

- 1000: C − Rust -100.437 ms; 2 complete blocks, 0 incomplete.
- 2500: C − Rust -655.378 ms; 2 complete blocks, 0 incomplete.
- 5105: C − Rust 1143.542 ms; 2 complete blocks, 0 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1000/cold | 45.612 | 17.110 | 10.964 | 567.278 | 117.492 | 54.953 | 305.601 | 231.887 | 4.631 |
| 1000/unchanged | unknown | 0.037 | 0.075 | 5.123 | 0.320 | 0.091 | 3.735 | 34.195 | 2.560 |
| 1000/private-001 | 2.267 | 0.619 | 16.082 | 345.795 | 54.931 | 42.714 | 214.307 | 95.783 | 4.857 |
| 1000/private-002 | 0.146 | 0.155 | 7.005 | 354.219 | 42.258 | 35.714 | 232.907 | 80.835 | 3.199 |
| 1000/private-003 | 0.208 | 0.126 | 6.663 | 265.776 | 33.672 | 32.150 | 178.517 | 44.597 | 2.069 |
| 1000/private-004 | 0.615 | 0.270 | 14.349 | 309.091 | 49.220 | 45.709 | 186.502 | 73.136 | 4.184 |
| 1000/private-005 | 0.427 | 0.238 | 11.495 | 281.651 | 45.673 | 35.817 | 172.369 | 61.967 | 2.433 |
| 1000/private-006 | 0.519 | 0.208 | 6.895 | 260.322 | 35.928 | 31.054 | 173.105 | 53.710 | 1.904 |
| 1000/private-007 | 0.194 | 0.119 | 6.645 | 272.526 | 33.580 | 27.664 | 184.069 | 46.935 | 2.333 |
| 1000/private-008 | 0.331 | 0.444 | 12.091 | 282.108 | 45.797 | 37.061 | 177.091 | 62.790 | 2.284 |
| 1000/unchanged-after-edits | unknown | 0.036 | 0.004 | 0.800 | 0.098 | 0.013 | 0.331 | 10.372 | 1.530 |
| 1000/positions | 21.782 | 8.092 | 13.466 | 15.686 | 12.061 | 0.012 | 0.336 | 105.578 | 1.704 |
| 1000/interface | 6.670 | 6.582 | 9.268 | 255.681 | 38.827 | 21.127 | 191.566 | 99.629 | 1.957 |
| 1000/broad | 45.327 | 16.442 | 16.584 | 417.544 | 167.970 | 106.335 | 108.932 | 161.832 | 2.363 |
| 1000/error | 7.490 | unknown | unknown | unknown | unknown | unknown | unknown | 8.686 | 1.095 |
| 1000/recovery | 1.342 | 0.179 | 6.536 | 223.756 | 40.892 | 30.414 | 128.477 | 59.579 | 2.174 |
| 1000/restore | 32.754 | 10.563 | 13.571 | 255.048 | 117.239 | 98.340 | 5.339 | 82.214 | 2.094 |
| 1000/warm-restart | 21.124 | 15.345 | 9.509 | 488.477 | 321.577 | 74.377 | 11.972 | 125.613 | 2.305 |
| 2500/cold | 64.869 | 30.833 | 20.149 | 583.808 | 311.880 | 65.199 | 126.761 | 427.974 | 7.815 |
| 2500/unchanged | unknown | 0.049 | 0.196 | 5.312 | 0.539 | 0.081 | 3.416 | 40.251 | 3.772 |
| 2500/private-001 | 0.861 | 0.456 | 15.414 | 296.100 | 75.665 | 63.398 | 126.305 | 77.905 | 2.258 |
| 2500/private-002 | 0.157 | 0.096 | 11.987 | 271.778 | 76.289 | 61.558 | 106.065 | 63.155 | 2.260 |
| 2500/private-003 | 0.125 | 0.098 | 10.578 | 263.190 | 72.703 | 64.883 | 99.615 | 63.297 | 2.216 |
| 2500/private-004 | 0.470 | 0.222 | 11.598 | 298.082 | 78.273 | 77.532 | 113.450 | 69.116 | 2.218 |
| 2500/private-005 | 0.258 | 0.293 | 18.804 | 274.394 | 79.608 | 49.295 | 118.626 | 76.094 | 2.301 |
| 2500/private-006 | 0.432 | 0.225 | 12.775 | 241.512 | 66.202 | 45.692 | 103.945 | 73.864 | 2.212 |
| 2500/private-007 | 0.803 | 1.037 | 20.896 | 364.913 | 85.389 | 53.744 | 174.155 | 149.069 | 3.738 |
| 2500/private-008 | 0.532 | 0.556 | 19.776 | 300.487 | 83.928 | 52.283 | 131.282 | 98.054 | 4.427 |
| 2500/unchanged-after-edits | unknown | 0.099 | 0.181 | 4.902 | 0.443 | 0.050 | 3.760 | 29.465 | 2.753 |
| 2500/positions | 39.398 | 22.263 | 32.549 | 56.261 | 44.833 | 0.013 | 3.144 | 322.389 | 3.888 |
| 2500/interface | 20.860 | 15.761 | 24.894 | 297.539 | 119.116 | 50.078 | 118.552 | 211.315 | 2.687 |
| 2500/broad | 48.909 | 34.133 | 34.438 | 821.483 | 375.950 | 143.592 | 183.660 | 509.587 | 5.024 |
| 2500/error | 12.641 | unknown | unknown | unknown | unknown | unknown | unknown | 15.695 | 1.469 |
| 2500/recovery | 3.780 | 0.498 | 14.619 | 293.430 | 91.441 | 52.550 | 123.086 | 132.893 | 2.565 |
| 2500/restore | 81.624 | 43.978 | 39.169 | 411.971 | 244.632 | 92.610 | 11.560 | 219.792 | 4.879 |
| 2500/warm-restart | 82.112 | 26.555 | 20.404 | 331.567 | 189.932 | 53.850 | 9.134 | 217.057 | 2.094 |
| 5105/cold | 361.091 | 144.502 | 93.825 | 2170.451 | 951.342 | 562.750 | 462.667 | 1182.145 | 6.589 |
| 5105/unchanged | unknown | 0.047 | 0.125 | 8.485 | 0.818 | 0.014 | 6.226 | 64.606 | 4.914 |
| 5105/private-001 | 0.378 | 0.125 | 41.272 | 1104.708 | 295.015 | 227.316 | 533.429 | 102.732 | 2.575 |
| 5105/private-002 | 0.260 | 0.242 | 42.834 | 1049.693 | 294.766 | 225.302 | 486.096 | 94.623 | 2.469 |
| 5105/private-003 | 0.288 | 0.173 | 43.112 | 1107.499 | 327.140 | 234.339 | 511.924 | 92.590 | 2.148 |
| 5105/private-004 | 0.339 | 0.161 | 43.453 | 1124.568 | 329.703 | 238.434 | 519.875 | 113.013 | 2.114 |
| 5105/private-005 | 0.337 | 0.246 | 46.032 | 985.609 | 305.946 | 188.804 | 454.490 | 111.557 | 3.485 |
| 5105/private-006 | 0.620 | 0.413 | 49.118 | 1008.459 | 278.909 | 184.078 | 505.751 | 129.320 | 4.194 |
| 5105/private-007 | 0.250 | 0.251 | 68.368 | 922.870 | 278.295 | 184.655 | 424.125 | 94.849 | 1.965 |
| 5105/private-008 | 0.142 | 0.108 | 37.630 | 969.233 | 275.906 | 201.058 | 456.957 | 86.878 | 2.288 |
| 5105/unchanged-after-edits | unknown | 0.053 | 0.047 | 6.381 | 0.832 | 0.014 | 4.433 | 42.119 | 4.051 |
| 5105/positions | 132.822 | 52.172 | 91.285 | 119.792 | 104.896 | 0.068 | 3.648 | 538.380 | 3.847 |
| 5105/interface | 41.217 | 41.684 | 79.178 | 972.650 | 348.882 | 140.309 | 470.236 | 374.401 | 2.345 |
| 5105/broad | 211.948 | 95.596 | 113.960 | 1928.528 | 986.972 | 475.213 | 316.105 | 662.745 | 3.174 |
| 5105/error | 56.516 | unknown | unknown | unknown | unknown | unknown | unknown | 18.332 | 1.518 |
| 5105/recovery | 7.866 | 0.690 | 58.149 | 1042.741 | 501.862 | 210.531 | 288.783 | 179.307 | 2.522 |
| 5105/restore | 237.367 | 83.040 | 115.213 | 1352.257 | 795.523 | 425.103 | 19.402 | 412.125 | 3.125 |
| 5105/warm-restart | 208.609 | 110.505 | 74.886 | 1295.051 | 708.367 | 299.068 | 24.651 | 890.977 | 4.305 |

ResourceLimit observations: 36; other invalid rows: 0.

## /workspace/zil-push-main/out/runs/restart-performance-check-2500-chain-01

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": null, "candidate_rust_accepted": null, "main_regression_accepted": null, "runtime_profile_present": false, "runtime_only_rust_efficiency_geomean": null, "cases": []}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 2500 | cold | 2 | 352.936 | 438.310 | -85.375 |
| 2500 | unchanged | 2 | 7.415 | 16.331 | -8.917 |
| 2500 | private-001 | 2 | 302.936 | 150.845 | 152.092 |
| 2500 | private-002 | 2 | 65.032 | 161.646 | -96.615 |
| 2500 | private-003 | 2 | 61.356 | 181.967 | -120.611 |
| 2500 | private-004 | 2 | 60.051 | 428.467 | -368.416 |
| 2500 | private-005 | 2 | 238.767 | 156.484 | 82.283 |
| 2500 | private-006 | 2 | 92.815 | 172.311 | -79.497 |
| 2500 | private-007 | 2 | 80.197 | 162.089 | -81.892 |
| 2500 | private-008 | 2 | 95.832 | 270.430 | -174.598 |
| 2500 | unchanged-after-edits | 2 | 11.608 | 23.185 | -11.578 |
| 2500 | positions | 2 | 352.217 | 776.030 | -423.813 |
| 2500 | broad | 2 | 701.666 | 580.987 | 120.679 |
| 2500 | error | 2 | 44.739 | 176.854 | -132.115 |
| 2500 | recovery | 2 | 137.946 | 485.032 | -347.086 |
| 2500 | restore | 2 | 776.265 | 550.094 | 226.172 |
| 2500 | warm-restart | 2 | 792.827 | 24.026 | 768.801 |

Eight-private totals use complete paired blocks only:

- 2500: C − Rust -687.253 ms; 2 complete blocks, 0 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2500/cold | 53.381 | 26.581 | 17.300 | unknown | unknown | unknown | unknown | 173.601 | 1.529 |
| 2500/unchanged | unknown | 0.029 | 0.005 | unknown | unknown | unknown | unknown | 5.894 | 1.487 |
| 2500/private-001 | 2.201 | 1.199 | 23.165 | unknown | unknown | unknown | unknown | 83.637 | 3.776 |
| 2500/private-002 | 0.107 | 0.097 | 11.219 | unknown | unknown | unknown | unknown | 39.289 | 1.470 |
| 2500/private-003 | 0.115 | 0.099 | 10.872 | unknown | unknown | unknown | unknown | 36.526 | 1.369 |
| 2500/private-004 | 0.105 | 0.140 | 9.854 | unknown | unknown | unknown | unknown | 36.946 | 1.342 |
| 2500/private-005 | 0.359 | 0.166 | 36.299 | unknown | unknown | unknown | unknown | 180.058 | 4.233 |
| 2500/private-006 | 0.260 | 0.412 | 20.583 | unknown | unknown | unknown | unknown | 49.426 | 2.212 |
| 2500/private-007 | 0.207 | 0.230 | 14.109 | unknown | unknown | unknown | unknown | 45.089 | 2.246 |
| 2500/private-008 | 0.177 | 0.418 | 16.014 | unknown | unknown | unknown | unknown | 53.785 | 2.482 |
| 2500/unchanged-after-edits | unknown | 0.030 | 0.004 | unknown | unknown | unknown | unknown | 9.963 | 1.611 |
| 2500/positions | 14.985 | 15.375 | 23.122 | unknown | unknown | unknown | unknown | 195.241 | 2.869 |
| 2500/broad | 74.830 | 35.859 | 41.111 | unknown | unknown | unknown | unknown | 246.675 | 3.361 |
| 2500/error | 11.205 | unknown | unknown | unknown | unknown | unknown | unknown | 12.664 | 1.760 |
| 2500/recovery | 5.462 | 0.549 | 15.332 | unknown | unknown | unknown | unknown | 87.538 | 2.030 |
| 2500/restore | 335.927 | 38.242 | 34.454 | unknown | unknown | unknown | unknown | 204.632 | 3.241 |
| 2500/warm-restart | 69.661 | 27.881 | 21.874 | unknown | unknown | unknown | unknown | 158.562 | 2.114 |

ResourceLimit observations: 0; other invalid rows: 0.

## /workspace/zil-push-main/out/runs/restart-performance-check-2500-diamond-01

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": null, "candidate_rust_accepted": null, "main_regression_accepted": null, "runtime_profile_present": false, "runtime_only_rust_efficiency_geomean": null, "cases": []}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 2500 | cold | 2 | 398.302 | 1081.430 | -683.128 |
| 2500 | unchanged | 2 | 10.827 | 19.560 | -8.733 |
| 2500 | private-001 | 2 | 78.434 | 167.398 | -88.964 |
| 2500 | private-002 | 2 | 143.265 | 497.543 | -354.279 |
| 2500 | private-003 | 2 | 83.325 | 169.741 | -86.416 |
| 2500 | private-004 | 2 | 71.237 | 252.379 | -181.142 |
| 2500 | private-005 | 2 | 149.571 | 138.203 | 11.367 |
| 2500 | private-006 | 2 | 187.257 | 184.935 | 2.322 |
| 2500 | private-007 | 2 | 65.523 | 141.160 | -75.637 |
| 2500 | private-008 | 2 | 109.925 | 147.099 | -37.174 |
| 2500 | unchanged-after-edits | 2 | 10.790 | 69.435 | -58.645 |
| 2500 | positions | 2 | 438.317 | 678.959 | -240.642 |
| 2500 | broad | 2 | 463.214 | 688.540 | -225.326 |
| 2500 | error | 2 | 42.337 | 198.879 | -156.542 |
| 2500 | recovery | 2 | 94.226 | 211.954 | -117.728 |
| 2500 | restore | 2 | 383.966 | 594.250 | -210.284 |
| 2500 | warm-restart | 2 | 303.208 | 19.852 | 283.357 |

Eight-private totals use complete paired blocks only:

- 2500: C − Rust -809.923 ms; 2 complete blocks, 0 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2500/cold | 59.388 | 30.560 | 20.881 | unknown | unknown | unknown | unknown | 190.882 | 2.005 |
| 2500/unchanged | unknown | 0.024 | 0.004 | unknown | unknown | unknown | unknown | 9.218 | 1.580 |
| 2500/private-001 | 0.348 | 0.177 | 13.943 | unknown | unknown | unknown | unknown | 46.323 | 1.520 |
| 2500/private-002 | 0.691 | 0.569 | 19.017 | unknown | unknown | unknown | unknown | 76.154 | 5.131 |
| 2500/private-003 | 0.311 | 0.262 | 13.702 | unknown | unknown | unknown | unknown | 45.797 | 2.580 |
| 2500/private-004 | 0.120 | 0.099 | 13.443 | unknown | unknown | unknown | unknown | 39.838 | 1.783 |
| 2500/private-005 | 0.541 | 0.906 | 20.911 | unknown | unknown | unknown | unknown | 77.114 | 7.007 |
| 2500/private-006 | 0.792 | 0.774 | 21.456 | unknown | unknown | unknown | unknown | 76.145 | 5.751 |
| 2500/private-007 | 0.134 | 0.117 | 11.163 | unknown | unknown | unknown | unknown | 39.048 | 2.118 |
| 2500/private-008 | 0.148 | 0.100 | 12.102 | unknown | unknown | unknown | unknown | 65.094 | 20.127 |
| 2500/unchanged-after-edits | unknown | 0.026 | 0.004 | unknown | unknown | unknown | unknown | 9.536 | 1.223 |
| 2500/positions | 25.434 | 15.704 | 28.046 | unknown | unknown | unknown | unknown | 199.204 | 2.683 |
| 2500/broad | 49.654 | 32.506 | 30.123 | unknown | unknown | unknown | unknown | 196.669 | 1.642 |
| 2500/error | 10.039 | unknown | unknown | unknown | unknown | unknown | unknown | 11.810 | 1.225 |
| 2500/recovery | 3.523 | 0.255 | 11.231 | unknown | unknown | unknown | unknown | 63.937 | 1.783 |
| 2500/restore | 52.207 | 27.485 | 26.323 | unknown | unknown | unknown | unknown | 173.313 | 2.767 |
| 2500/warm-restart | 56.311 | 25.085 | 16.032 | unknown | unknown | unknown | unknown | 115.544 | 1.761 |

ResourceLimit observations: 0; other invalid rows: 0.

## /workspace/zil-push-main/out/runs/restart-performance-check-5000-wide-01

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": null, "candidate_rust_accepted": null, "main_regression_accepted": null, "runtime_profile_present": false, "runtime_only_rust_efficiency_geomean": null, "cases": []}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 5000 | cold | 2 | 1621.930 | 807.345 | 814.585 |
| 5000 | unchanged | 2 | 9.542 | 20.334 | -10.792 |
| 5000 | private-001 | 2 | 352.414 | 601.697 | -249.284 |
| 5000 | private-002 | 2 | 110.586 | 268.231 | -157.645 |
| 5000 | private-003 | 2 | 107.551 | 333.272 | -225.721 |
| 5000 | private-004 | 2 | 109.478 | 321.748 | -212.270 |
| 5000 | private-005 | 2 | 321.805 | 308.247 | 13.558 |
| 5000 | private-006 | 2 | 107.184 | 307.693 | -200.510 |
| 5000 | private-007 | 2 | 146.921 | 292.721 | -145.801 |
| 5000 | private-008 | 2 | 97.458 | 358.077 | -260.619 |
| 5000 | unchanged-after-edits | 2 | 7.948 | 15.498 | -7.550 |
| 5000 | positions | 2 | 785.063 | 886.212 | -101.149 |
| 5000 | broad | 2 | 1280.198 | 864.271 | 415.927 |
| 5000 | error | 2 | 68.867 | 367.619 | -298.752 |
| 5000 | recovery | 2 | 189.660 | 427.115 | -237.455 |
| 5000 | restore | 2 | 872.297 | 842.528 | 29.769 |
| 5000 | warm-restart | 2 | 1385.004 | 27.416 | 1357.588 |

Eight-private totals use complete paired blocks only:

- 5000: C − Rust -1438.291 ms; 2 complete blocks, 0 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 5000/cold | 527.740 | 93.511 | 56.169 | unknown | unknown | unknown | unknown | 425.009 | 3.011 |
| 5000/unchanged | unknown | 0.042 | 0.005 | unknown | unknown | unknown | unknown | 8.086 | 1.410 |
| 5000/private-001 | 0.450 | 0.275 | 239.085 | unknown | unknown | unknown | unknown | 80.597 | 2.344 |
| 5000/private-002 | 0.112 | 0.099 | 32.522 | unknown | unknown | unknown | unknown | 52.561 | 1.622 |
| 5000/private-003 | 0.346 | 0.106 | 25.947 | unknown | unknown | unknown | unknown | 53.503 | 1.474 |
| 5000/private-004 | 0.113 | 0.101 | 25.998 | unknown | unknown | unknown | unknown | 55.995 | 1.641 |
| 5000/private-005 | 0.304 | 0.121 | 222.098 | unknown | unknown | unknown | unknown | 70.180 | 2.571 |
| 5000/private-006 | 0.237 | 0.267 | 23.613 | unknown | unknown | unknown | unknown | 55.249 | 1.585 |
| 5000/private-007 | 0.271 | 0.508 | 34.083 | unknown | unknown | unknown | unknown | 69.332 | 2.633 |
| 5000/private-008 | 0.131 | 0.107 | 22.748 | unknown | unknown | unknown | unknown | 50.416 | 1.690 |
| 5000/unchanged-after-edits | unknown | 0.029 | 0.004 | unknown | unknown | unknown | unknown | 6.764 | 1.152 |
| 5000/positions | 51.981 | 34.869 | 63.943 | unknown | unknown | unknown | unknown | 346.228 | 1.814 |
| 5000/broad | 246.118 | 110.407 | 82.650 | unknown | unknown | unknown | unknown | 394.784 | 2.590 |
| 5000/error | 25.050 | unknown | unknown | unknown | unknown | unknown | unknown | 10.808 | 1.014 |
| 5000/recovery | 6.319 | 0.308 | 27.544 | unknown | unknown | unknown | unknown | 126.767 | 1.672 |
| 5000/restore | 103.226 | 152.170 | 80.723 | unknown | unknown | unknown | unknown | 303.478 | 2.442 |
| 5000/warm-restart | 388.851 | 163.457 | 139.140 | unknown | unknown | unknown | unknown | 481.563 | 3.907 |

ResourceLimit observations: 0; other invalid rows: 0.

## /workspace/zil-push-main/out/runs/restart-performance-clear-01

Qualification (separate): `{"source": "summary.json; copied qualification, no compiler/runtime ratio inference", "execution_complete": true, "functional_quality_valid": true, "accepted": false, "candidate_rust_accepted": true, "main_regression_accepted": null, "runtime_profile_present": true, "runtime_only_rust_efficiency_geomean": 6.6185504821617505, "cases": [{"case": "2500/cold", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/unchanged", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/warm-restart", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/private-001", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "2500/recovery", "main_resource_limit": false, "rust_gate_pass": true, "main_gate_pass": true}, {"case": "5105/cold", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/unchanged", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/warm-restart", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/private-001", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}, {"case": "5105/recovery", "main_resource_limit": true, "rust_gate_pass": true, "main_gate_pass": null}]}`

| Preset | Request | Paired blocks | Candidate ms | Rust ms | C − Rust ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 2500 | cold | 2 | 927.234 | 1450.366 | -523.132 |
| 2500 | unchanged | 2 | 49.233 | 55.352 | -6.119 |
| 2500 | warm-restart | 2 | 1049.250 | 1675.639 | -626.388 |
| 2500 | private-001 | 2 | 648.062 | 532.787 | 115.275 |
| 2500 | error | 2 | 237.979 | 591.303 | -353.323 |
| 2500 | recovery | 2 | 1129.207 | 1471.797 | -342.590 |
| 5105 | cold | 2 | 4982.309 | 6851.736 | -1869.427 |
| 5105 | unchanged | 2 | 46.418 | 36.771 | 9.647 |
| 5105 | warm-restart | 2 | 5028.088 | 6978.988 | -1950.899 |
| 5105 | private-001 | 2 | 1384.438 | 1254.029 | 130.409 |
| 5105 | error | 2 | 992.641 | 2047.810 | -1055.169 |
| 5105 | recovery | 2 | 3030.421 | 6015.405 | -2984.983 |

Eight-private totals use complete paired blocks only:

- 2500: C − Rust unknown ms; 0 complete blocks, 2 incomplete.
- 5105: C − Rust unknown ms; 0 complete blocks, 2 incomplete.

Candidate phase/codegen detail (separate parent/child views; medians in ms):

| Preset/request | Types | Analyze | Verify | Codegen parent | Machine child | Partition child | Object child | Unassigned report | Outside report |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2500/cold | 74.176 | 35.410 | 20.915 | 407.086 | 171.683 | 58.892 | 113.707 | 234.053 | 2.848 |
| 2500/unchanged | unknown | 0.040 | 0.139 | 6.457 | 0.908 | 0.167 | 4.164 | 39.409 | 3.159 |
| 2500/warm-restart | 54.107 | 27.057 | 19.214 | 421.871 | 159.239 | 88.310 | 106.586 | 434.786 | 8.670 |
| 2500/private-001 | 0.455 | 0.241 | 14.088 | 510.565 | 89.119 | 173.408 | 156.481 | 96.091 | 3.371 |
| 2500/error | 17.863 | unknown | unknown | unknown | unknown | unknown | unknown | 50.658 | 1.432 |
| 2500/recovery | 52.285 | 39.420 | 31.425 | 777.246 | 449.903 | 123.050 | 136.687 | 212.172 | 2.299 |
| 5105/cold | 359.455 | 250.188 | 126.505 | 2180.071 | 1134.425 | 433.712 | 450.770 | 1207.167 | 6.272 |
| 5105/unchanged | unknown | 0.052 | 0.064 | 3.914 | 0.580 | 0.046 | 2.584 | 40.186 | 2.192 |
| 5105/warm-restart | 268.436 | 163.694 | 321.297 | 2510.576 | 940.232 | 749.347 | 512.162 | 1036.879 | 3.123 |
| 5105/private-001 | 1.800 | 0.818 | 57.889 | 1103.764 | 312.033 | 219.118 | 501.708 | 99.137 | 2.146 |
| 5105/error | 67.633 | unknown | unknown | unknown | unknown | unknown | unknown | 238.329 | 2.153 |
| 5105/recovery | 216.739 | 167.068 | 121.476 | 1950.041 | 852.676 | 660.576 | 314.722 | 532.495 | 6.607 |

ResourceLimit observations: 12; other invalid rows: 0.

