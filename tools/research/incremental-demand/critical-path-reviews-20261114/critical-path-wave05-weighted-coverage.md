# Critical-path wave05 weighted coverage

State: **analyzed**. Candidate `76993726b2e986ccd49b50c6424748d335056aeb`; main `9170bb4210554392e36eb7528596612596ebef80`; frozen tool `ad81486b976634e71bf1c61544d0fd28a783fbae`.

Sum the median wall time of each actual request; positive savings = reference − candidate. `broad` means the named broad request; error/recovery remain separate requests in other. No cross-window aggregate or industrial frequency assumption.

| Window | State | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-05-o2 | valid | 11.328 | 8.360 | 8.206 | 2.968 | -0.154 |
| large-project-large2500-chain-05-o2 | valid | 11.287 | 8.479 | 9.082 | 2.809 | 0.603 |
| large-project-large2500-diamond-05-o2 | valid | 10.639 | 8.508 | 8.350 | 2.131 | -0.158 |
| large-project-large1000-edits-05-o2 | failed | — | — | — | — | — |

**Final wave05 evidence: 3/4 valid windows, 1 failed; 309 recorded matrix rows.** Results belong only to `76993726b2e986ccd49b50c6424748d335056aeb`. Check04 retains its original 5a source; any later code receives none of these measurements.

**Negative result: all 67 matched request/block reuse-count deltas against 04 are zero.** Alpha normalization added no observed reuse events in these inputs. This is a counter result, not a causal performance comparison between the two windows.

- large-project-large2500-05-o2: candidate workflow is 0.154s slower than Rust.
- large-project-large2500-diamond-05-o2: candidate workflow is 0.158s slower than Rust.

## Observed native emission reuse totals

These are dynamic events, not unique source functions. Counts sum only actual non-fallback emission receipts; missing/cached receipts are not filled with zero. Failed-window counts are diagnostic observations, not accepted performance results.

| Window | Block | Receipt requests | Receipt units | Reuse events | Fallback units |
| --- | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-05-o2 | 0 | 11 | 41 | 5516 | 0 |
| large-project-large2500-05-o2 | 1 | 11 | 41 | 5516 | 0 |
| large-project-large2500-chain-05-o2 | 0 | 11 | 41 | 5516 | 0 |
| large-project-large2500-chain-05-o2 | 1 | 11 | 41 | 5516 | 0 |
| large-project-large2500-diamond-05-o2 | 0 | 11 | 41 | 5516 | 0 |
| large-project-large2500-diamond-05-o2 | 1 | 11 | 41 | 5516 | 0 |
| large-project-large1000-edits-05-o2 | 0 | 1 | 16 | 0 | 0 |

Totals and phase costs above/below come only from actual 05 receipts. No 04 native counts or first-cold observations are filled into missing 05 requests.

## Alpha normalization: observed added hits versus wave04

Count deltas retain both source identities and do not establish causal time savings. A zero delta is an explicit negative result for added reuse on that input.

| Window | Request | Block | 04 reuses (5a) | 05 reuses (769) | Added events | 05 fallback units |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-05-o2 | cold | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-05-o2 | cold | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-05-o2 | private-001 | 0 | 148 | 148 | 0 | 0 |
| large-project-large2500-05-o2 | private-001 | 1 | 148 | 148 | 0 | 0 |
| large-project-large2500-05-o2 | broad | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-05-o2 | broad | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-chain-05-o2 | cold | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-chain-05-o2 | cold | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-chain-05-o2 | private-001 | 0 | 148 | 148 | 0 | 0 |
| large-project-large2500-chain-05-o2 | private-001 | 1 | 148 | 148 | 0 | 0 |
| large-project-large2500-chain-05-o2 | broad | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-chain-05-o2 | broad | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-diamond-05-o2 | cold | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-diamond-05-o2 | cold | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-diamond-05-o2 | private-001 | 0 | 148 | 148 | 0 | 0 |
| large-project-large2500-diamond-05-o2 | private-001 | 1 | 148 | 148 | 0 | 0 |
| large-project-large2500-diamond-05-o2 | broad | 0 | 2092 | 2092 | 0 | 0 |
| large-project-large2500-diamond-05-o2 | broad | 1 | 2092 | 2092 | 0 | 0 |
| large-project-large1000-edits-05-o2 | cold | 0 | 0 | 0 | 0 | 0 |

## large-project-large2500-05-o2

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 2.713 | 2.564 | 1.439 | 0.149 | -1.126 |
| broad | 1 | 2.433 | 2.130 | 1.461 | 0.303 | -0.669 |
| private | 8 | 4.166 | 1.882 | 3.174 | 2.284 | 1.292 |
| other | 7 | 2.015 | 1.784 | 2.132 | 0.231 | 0.348 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 26.906 | 26.728 | 8 | 14.875 |
| candidate | 12.543 | 11.162 | 8 | 1.123 |
| rust | 42.508 | 42.327 | 8 | 11.527 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.262, "expand": 205.542, "types": 105.01, "analyze": 86.224, "verify": 105.435, "lower": 3.484, "codegen": 1821.721, "ct": 45.202}, {"parse": 0.131, "expand": 78.65, "types": 42.57, "analyze": 35.415, "verify": 19.231, "lower": 1.425, "codegen": 1876.171, "ct": 14.94}]` |
| private-001 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.098, "expand": 63.357, "types": 1.72, "analyze": 0.619, "verify": 20.467, "lower": 0.013, "codegen": 168.324, "ct": 10.847}, {"parse": 0.06, "expand": 11.539, "types": 0.752, "analyze": 0.23, "verify": 16.202, "lower": 0.013, "codegen": 127.263, "ct": 6.274}]` |
| private-002 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.064, "expand": 9.788, "types": 0.103, "analyze": 0.128, "verify": 11.303, "lower": 0.009, "codegen": 125.932, "ct": 5.755}, {"parse": 0.055, "expand": 6.509, "types": 0.089, "analyze": 0.09, "verify": 10.185, "lower": 0.009, "codegen": 126.516, "ct": 3.628}]` |
| private-003 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.062, "expand": 10.15, "types": 0.102, "analyze": 0.143, "verify": 12.251, "lower": 0.009, "codegen": 132.821, "ct": 4.649}, {"parse": 0.118, "expand": 18.397, "types": 0.243, "analyze": 0.342, "verify": 22.17, "lower": 0.01, "codegen": 142.49, "ct": 5.771}]` |
| private-004 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.089, "expand": 7.213, "types": 0.096, "analyze": 0.096, "verify": 9.676, "lower": 0.009, "codegen": 125.29, "ct": 4.259}, {"parse": 0.073, "expand": 8.774, "types": 0.099, "analyze": 0.289, "verify": 13.722, "lower": 0.009, "codegen": 130.774, "ct": 4.421}]` |
| private-005 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.066, "expand": 9.012, "types": 0.119, "analyze": 0.138, "verify": 15.979, "lower": 0.011, "codegen": 128.177, "ct": 7.083}, {"parse": 0.068, "expand": 6.918, "types": 0.128, "analyze": 0.097, "verify": 10.675, "lower": 0.009, "codegen": 315.957, "ct": 4.561}]` |
| private-006 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.062, "expand": 6.47, "types": 0.172, "analyze": 0.099, "verify": 9.56, "lower": 0.01, "codegen": 120.623, "ct": 3.764}, {"parse": 0.06, "expand": 7.053, "types": 0.154, "analyze": 0.129, "verify": 11.6, "lower": 0.01, "codegen": 122.61, "ct": 3.942}]` |
| private-007 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.076, "expand": 7.802, "types": 0.146, "analyze": 0.096, "verify": 9.803, "lower": 0.012, "codegen": 129.289, "ct": 4.152}, {"parse": 0.071, "expand": 7.868, "types": 0.235, "analyze": 0.114, "verify": 10.967, "lower": 0.011, "codegen": 131.096, "ct": 5.216}]` |
| private-008 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.082, "expand": 9.191, "types": 0.206, "analyze": 0.105, "verify": 15.287, "lower": 0.012, "codegen": 147.99, "ct": 10.908}, {"parse": 0.072, "expand": 9.388, "types": 0.302, "analyze": 0.111, "verify": 10.852, "lower": 0.01, "codegen": 125.892, "ct": 5.209}]` |
| broad | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.408, "expand": 142.808, "types": 53.284, "analyze": 39.708, "verify": 32.042, "lower": 3.041, "codegen": 1758.357, "ct": 26.902}, {"parse": 0.375, "expand": 113.435, "types": 49.596, "analyze": 34.638, "verify": 24.429, "lower": 1.937, "codegen": 1452.39, "ct": 23.261}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.01, "analyze": 0.005, "verify": 244.206, "lower": 194.628, "codegen": 345.426}, {"types": 0.006, "analyze": 0.004, "verify": 18.67, "lower": 0.79, "codegen": 244.037}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large2500-chain-05-o2

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 2.686 | 2.336 | 1.563 | 0.350 | -0.773 |
| broad | 1 | 2.387 | 2.168 | 1.142 | 0.218 | -1.026 |
| private | 8 | 4.160 | 2.396 | 3.785 | 1.764 | 1.389 |
| other | 7 | 2.054 | 1.578 | 2.592 | 0.476 | 1.013 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 26.902 | 26.796 | 8 | 14.875 |
| candidate | 12.324 | 11.118 | 8 | 0.943 |
| rust | 43.180 | 42.972 | 8 | 11.707 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.133, "expand": 81.866, "types": 31.333, "analyze": 26.428, "verify": 16.209, "lower": 0.938, "codegen": 1885.676, "ct": 15.507}, {"parse": 0.13, "expand": 75.839, "types": 33.231, "analyze": 28.922, "verify": 18.17, "lower": 0.931, "codegen": 1915.758, "ct": 13.688}]` |
| private-001 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.06, "expand": 7.975, "types": 0.182, "analyze": 0.095, "verify": 9.357, "lower": 0.011, "codegen": 133.831, "ct": 4.42}, {"parse": 0.058, "expand": 7.922, "types": 0.25, "analyze": 0.1, "verify": 10.742, "lower": 0.012, "codegen": 514.634, "ct": 6.161}]` |
| private-002 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.06, "expand": 8.598, "types": 0.102, "analyze": 0.091, "verify": 9.769, "lower": 0.009, "codegen": 129.717, "ct": 5.163}, {"parse": 0.062, "expand": 8.318, "types": 0.1, "analyze": 0.121, "verify": 10.51, "lower": 0.009, "codegen": 496.645, "ct": 5.187}]` |
| private-003 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.06, "expand": 8.143, "types": 0.091, "analyze": 0.09, "verify": 10.414, "lower": 0.01, "codegen": 127.16, "ct": 3.881}, {"parse": 0.174, "expand": 17.746, "types": 0.124, "analyze": 0.294, "verify": 13.921, "lower": 0.01, "codegen": 156.173, "ct": 5.458}]` |
| private-004 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.074, "expand": 8.296, "types": 0.21, "analyze": 0.094, "verify": 11.517, "lower": 0.01, "codegen": 340.139, "ct": 4.963}, {"parse": 0.111, "expand": 9.482, "types": 0.158, "analyze": 0.174, "verify": 11.503, "lower": 0.01, "codegen": 148.158, "ct": 5.177}]` |
| private-005 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.57, "expand": 42.486, "types": 0.21, "analyze": 0.283, "verify": 18.708, "lower": 0.01, "codegen": 152.317, "ct": 5.988}, {"parse": 0.242, "expand": 13.511, "types": 0.222, "analyze": 0.154, "verify": 12.85, "lower": 0.01, "codegen": 142.667, "ct": 5.382}]` |
| private-006 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.063, "expand": 7.302, "types": 0.138, "analyze": 0.097, "verify": 9.848, "lower": 0.009, "codegen": 124.474, "ct": 4.111}, {"parse": 0.072, "expand": 7.629, "types": 0.136, "analyze": 0.097, "verify": 9.882, "lower": 0.01, "codegen": 133.944, "ct": 4.478}]` |
| private-007 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.163, "expand": 10.077, "types": 0.116, "analyze": 0.119, "verify": 11.848, "lower": 0.011, "codegen": 130.262, "ct": 4.421}, {"parse": 0.1, "expand": 11.353, "types": 0.205, "analyze": 0.092, "verify": 12.096, "lower": 0.021, "codegen": 136.89, "ct": 4.193}]` |
| private-008 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.091, "expand": 6.167, "types": 0.094, "analyze": 0.092, "verify": 9.171, "lower": 0.01, "codegen": 120.296, "ct": 3.742}, {"parse": 0.092, "expand": 12.741, "types": 0.154, "analyze": 0.4, "verify": 18.03, "lower": 0.01, "codegen": 137.352, "ct": 4.826}]` |
| broad | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.386, "expand": 122.36, "types": 45.608, "analyze": 31.96, "verify": 21.562, "lower": 1.895, "codegen": 1695.559, "ct": 22.977}, {"parse": 0.406, "expand": 125.862, "types": 47.266, "analyze": 33.256, "verify": 22.3, "lower": 2.004, "codegen": 1531.13, "ct": 11.292}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.007, "analyze": 0.004, "verify": 18.37, "lower": 0.731, "codegen": 224.625}, {"types": 0.01, "analyze": 0.005, "verify": 18.285, "lower": 0.839, "codegen": 231.955}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large2500-diamond-05-o2

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 2.624 | 2.677 | 1.628 | -0.053 | -1.049 |
| broad | 1 | 2.266 | 1.917 | 1.203 | 0.349 | -0.714 |
| private | 8 | 3.890 | 2.292 | 3.121 | 1.598 | 0.829 |
| other | 7 | 1.859 | 1.622 | 2.398 | 0.237 | 0.776 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 26.902 | 26.801 | 8 | 14.875 |
| candidate | 14.340 | 11.756 | 8 | 1.619 |
| rust | 43.129 | 42.936 | 8 | 11.656 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.137, "expand": 99.21, "types": 88.277, "analyze": 59.283, "verify": 39.196, "lower": 1.323, "codegen": 2411.553, "ct": 31.084}, {"parse": 0.131, "expand": 87.301, "types": 36.247, "analyze": 29.61, "verify": 19.265, "lower": 1.01, "codegen": 1844.523, "ct": 16.168}]` |
| private-001 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.059, "expand": 9.638, "types": 0.249, "analyze": 0.144, "verify": 11.349, "lower": 0.013, "codegen": 147.548, "ct": 4.887}, {"parse": 0.232, "expand": 11.898, "types": 0.293, "analyze": 0.134, "verify": 12.288, "lower": 0.015, "codegen": 139.839, "ct": 4.591}]` |
| private-002 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.056, "expand": 12.373, "types": 0.143, "analyze": 0.163, "verify": 13.824, "lower": 0.01, "codegen": 127.489, "ct": 4.242}, {"parse": 0.364, "expand": 58.539, "types": 0.172, "analyze": 0.44, "verify": 23.419, "lower": 0.01, "codegen": 150.986, "ct": 6.345}]` |
| private-003 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.642, "expand": 62.726, "types": 0.362, "analyze": 0.306, "verify": 28.435, "lower": 0.082, "codegen": 164.172, "ct": 6.345}, {"parse": 0.063, "expand": 7.019, "types": 0.186, "analyze": 0.1, "verify": 9.98, "lower": 0.01, "codegen": 125.22, "ct": 5.777}]` |
| private-004 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.177, "expand": 7.874, "types": 0.136, "analyze": 0.14, "verify": 11.158, "lower": 0.009, "codegen": 129.271, "ct": 4.738}, {"parse": 0.197, "expand": 12.859, "types": 0.097, "analyze": 0.096, "verify": 13.865, "lower": 0.01, "codegen": 134.462, "ct": 4.722}]` |
| private-005 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.167, "expand": 12.066, "types": 0.199, "analyze": 0.222, "verify": 11.872, "lower": 0.008, "codegen": 142.313, "ct": 5.465}, {"parse": 0.069, "expand": 8.137, "types": 0.11, "analyze": 0.1, "verify": 10.404, "lower": 0.009, "codegen": 129.345, "ct": 5.863}]` |
| private-006 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.424, "expand": 17.243, "types": 0.321, "analyze": 0.196, "verify": 15.759, "lower": 0.01, "codegen": 134.35, "ct": 4.6}, {"parse": 0.065, "expand": 6.685, "types": 0.168, "analyze": 0.098, "verify": 9.855, "lower": 0.009, "codegen": 130.02, "ct": 3.815}]` |
| private-007 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.072, "expand": 7.682, "types": 0.165, "analyze": 0.221, "verify": 10.304, "lower": 0.011, "codegen": 129.425, "ct": 4.167}, {"parse": 0.157, "expand": 13.141, "types": 0.125, "analyze": 0.125, "verify": 21.153, "lower": 0.013, "codegen": 149.098, "ct": 5.362}]` |
| private-008 | `[148, 148]` | `[0, 0]` | `[{"parse": 1.409, "expand": 78.445, "types": 0.439, "analyze": 0.493, "verify": 30.582, "lower": 0.013, "codegen": 170.084, "ct": 7.205}, {"parse": 0.19, "expand": 9.736, "types": 0.098, "analyze": 0.097, "verify": 12.865, "lower": 0.008, "codegen": 131.88, "ct": 4.366}]` |
| broad | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.395, "expand": 105.752, "types": 48.484, "analyze": 36.453, "verify": 27.632, "lower": 2.304, "codegen": 1501.821, "ct": 25.2}, {"parse": 0.428, "expand": 65.56, "types": 19.466, "analyze": 20.45, "verify": 22.867, "lower": 2.117, "codegen": 1532.387, "ct": 10.069}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.007, "analyze": 0.005, "verify": 25.18, "lower": 2.721, "codegen": 613.168}, {"types": 0.015, "analyze": 0.005, "verify": 52.834, "lower": 2.226, "codegen": 454.483}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large1000-edits-05-o2

Comparisons remain empty; no workflow or storage advantage is inferred.

`{"block": 0, "index": 0, "request": "cold", "arm": "rust", "error": "ValueError: three-arm request/native/CGU16/full-ELF quality failed"}`

Recorded rows: 3/246. Quality: `[{"schema": "native-link-exact-elf-reviewed-ir-v1", "valid": false, "semantics_equal": true, "executable_equal": false, "llvm_text_equal": false, "raw_llvm_equal": false, "llvm_equal_excluding_identity_comment": false, "per_unit_exports_equal_observed": true, "aggregate_zil_function_exports_equal": true, "native_oracles_valid": true, "physical_zil_units_valid": true, "llvm_equality_is_acceptance_prerequisite": false, "block": 0, "index": 0, "request": "cold"}]`

## Original5105 capacity

State: recorded; comparisons=[]; formal_ratio=null. Main ResourceLimit is a failed capacity request, never a fast timing reference. Historical Rust receipts remain explicitly historical.

Recorded complete=True, protocol_valid=True, candidate_full_source_verify_native_success=True.

| Arm | Mode/request | Status | Current Verify runs | Wall ms (diagnostic only) | Emission reuse events |
| --- | --- | --- | ---: | ---: | ---: |
| candidate | check/cold | Success | 1 | 1664.572 | None |
| candidate | check/warm | Success | 1 | 10.083 | None |
| candidate | o2/cold | Success | 1 | 10011.509 | 0 |
| candidate | o2/warm | Success | 1 | 16.183 | None |
| baseline | check/cold | ResourceLimit | 0 | 437.613 | None |
| baseline | check/warm | ResourceLimit | 0 | 320.354 | None |
| baseline | o2/cold | ResourceLimit | 0 | 427.668 | None |
| baseline | o2/warm | ResourceLimit | 0 | 492.917 | None |
| rust | check/historical-cold [historical receipt] | Success | None | — | None |
| rust | o2/cold | Success | None | — | None |
| rust | o2/warm | Success | None | — | None |

Only the recorded statuses, Verify counts, source-inventory flags, oracle observations and emission counters support capacity claims. This is not a matched performance window. Missing receipt counters remain unknown; no 04 capacity result is substituted.

## Explicit historical check04 references

Both checks below were measured at `5a1434573ecd2ce8aa233e6841e9181ff3d9fd9d`, not `76993726b2e986ccd49b50c6424748d335056aeb`. The only source difference is native/emission.rs (verified during preparation). Per user authorization they remain check-path context; this is not a fresh 769 check measurement or a binary/timing equivalence claim.

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
- Fallback receipts can include discarded-attempt work; retain fallback flags and ordinary object costs.
- 5105 capacity observations never enter workflow sums, timing ratios or disk advantage comparisons.
- No compiler/native quality gates are rerun by this analyzer; it checks recorded evidence consistency.

Prepared invocation (run only after the batch-done notice):

`python3 -B /workspace/zil-recovery/critical-path-wave05-weighted-analysis.py --analyze`
