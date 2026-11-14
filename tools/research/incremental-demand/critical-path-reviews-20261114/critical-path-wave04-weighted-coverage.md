# Critical-path wave04 weighted coverage

State: **analyzed**. Candidate `5a1434573ecd2ce8aa233e6841e9181ff3d9fd9d`; main `9170bb4210554392e36eb7528596612596ebef80`; frozen tool `ad81486b976634e71bf1c61544d0fd28a783fbae`.

Sum the median wall time of each actual request; positive savings = reference − candidate. `broad` means the named broad request; error/recovery remain separate requests in other. No cross-window aggregate or industrial frequency assumption.

| Window | State | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-04-o2 | valid | 15.100 | 9.287 | 10.500 | 5.812 | 1.213 |
| large-project-large1000-edits-04-check | valid | 6.986 | 2.569 | 7.340 | 4.416 | 4.770 |
| large-project-large2500-04-check | valid | 2.890 | 1.939 | 3.832 | 0.951 | 1.893 |
| large-project-large2500-chain-04-o2 | valid | 12.759 | 9.450 | 10.497 | 3.309 | 1.047 |
| large-project-large2500-diamond-04-o2 | valid | 12.095 | 8.141 | 8.880 | 3.954 | 0.739 |
| large-project-large1000-edits-04-o2 | failed | — | — | — | — | — |

**Final wave04 evidence: 5/6 valid windows, 1 failed; 657 recorded matrix rows.** Results belong only to `5a1434573ecd2ce8aa233e6841e9181ff3d9fd9d`. Later alpha-normalized/closed-body code is outside this source and receives none of these measurements.

All five valid workflow totals favor the candidate over main and Rust. This is not a uniform request win: native cold and broad remain slower than Rust; private-edit frequency supplies a substantial part of the workflow gain. Signed cold/broad losses remain in the tables below.

## Observed native emission reuse totals

These are dynamic events, not unique source functions. Counts sum only actual non-fallback emission receipts; missing/cached receipts are not filled with zero. Failed-window counts are diagnostic observations, not accepted performance results.

| Window | Block | Receipt requests | Receipt units | Reuse events | Fallback units |
| --- | ---: | ---: | ---: | ---: | ---: |
| large-project-large2500-04-o2 | 0 | 11 | 41 | 5516 | 0 |
| large-project-large2500-04-o2 | 1 | 11 | 41 | 5516 | 0 |
| large-project-large2500-chain-04-o2 | 0 | 11 | 41 | 5516 | 0 |
| large-project-large2500-chain-04-o2 | 1 | 11 | 41 | 5516 | 0 |
| large-project-large2500-diamond-04-o2 | 0 | 11 | 41 | 5516 | 0 |
| large-project-large2500-diamond-04-o2 | 1 | 11 | 41 | 5516 | 0 |
| large-project-large1000-edits-04-o2 | 0 | 1 | 16 | 0 | 0 |

Each valid 2500 O2 graph records 2,092 reuses on cold, 148 on each of eight private edits, 2,092 on broad and 148 on recovery: **5,516 events/block; 11,032 across two blocks per window**, with zero reported emission fallback units. These reuse counters do not establish causal time savings. The failed 1000 O2 cold request records zero reuses despite passing behavioral oracles.


## large-project-large2500-04-o2

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 2.424 | 2.643 | 1.754 | -0.219 | -0.889 |
| broad | 1 | 3.086 | 2.319 | 1.346 | 0.767 | -0.973 |
| private | 8 | 6.321 | 2.352 | 4.542 | 3.969 | 2.190 |
| other | 7 | 3.268 | 1.973 | 2.859 | 1.295 | 0.885 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 26.906 | 26.728 | 8 | 14.875 |
| candidate | 12.531 | 11.164 | 8 | 1.082 |
| rust | 42.508 | 42.327 | 8 | 11.527 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.303, "expand": 226.896, "types": 132.864, "analyze": 89.717, "verify": 46.895, "lower": 1.625, "codegen": 1876.49, "ct": 51.923}, {"parse": 0.191, "expand": 125.694, "types": 80.678, "analyze": 53.492, "verify": 34.818, "lower": 4.722, "codegen": 1980.984, "ct": 37.357}]` |
| private-001 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.092, "expand": 17.856, "types": 0.541, "analyze": 0.381, "verify": 21.796, "lower": 0.061, "codegen": 204.423, "ct": 8.922}, {"parse": 0.093, "expand": 23.981, "types": 0.536, "analyze": 0.172, "verify": 20.133, "lower": 0.013, "codegen": 151.446, "ct": 9.508}]` |
| private-002 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.164, "expand": 11.631, "types": 0.16, "analyze": 0.269, "verify": 25.682, "lower": 0.028, "codegen": 190.635, "ct": 8.906}, {"parse": 0.087, "expand": 10.323, "types": 0.118, "analyze": 0.113, "verify": 12.204, "lower": 0.011, "codegen": 275.353, "ct": 4.964}]` |
| private-003 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.072, "expand": 9.218, "types": 0.138, "analyze": 0.241, "verify": 12.886, "lower": 0.012, "codegen": 147.19, "ct": 6.509}, {"parse": 0.181, "expand": 17.444, "types": 0.117, "analyze": 0.172, "verify": 14.378, "lower": 0.01, "codegen": 124.859, "ct": 5.217}]` |
| private-004 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.106, "expand": 7.479, "types": 0.294, "analyze": 0.305, "verify": 18.871, "lower": 0.035, "codegen": 134.699, "ct": 14.184}, {"parse": 0.134, "expand": 36.771, "types": 0.292, "analyze": 0.348, "verify": 29.903, "lower": 0.018, "codegen": 215.004, "ct": 14.874}]` |
| private-005 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.098, "expand": 16.858, "types": 0.164, "analyze": 0.406, "verify": 30.083, "lower": 0.017, "codegen": 180.859, "ct": 6.926}, {"parse": 0.3, "expand": 12.931, "types": 0.161, "analyze": 0.128, "verify": 17.468, "lower": 0.011, "codegen": 185.172, "ct": 5.825}]` |
| private-006 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.087, "expand": 9.919, "types": 0.161, "analyze": 0.254, "verify": 15.284, "lower": 0.017, "codegen": 199.456, "ct": 8.778}, {"parse": 0.215, "expand": 23.235, "types": 0.278, "analyze": 0.413, "verify": 19.423, "lower": 0.013, "codegen": 180.197, "ct": 8.155}]` |
| private-007 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.109, "expand": 13.092, "types": 0.239, "analyze": 0.182, "verify": 15.118, "lower": 0.016, "codegen": 158.072, "ct": 11.329}, {"parse": 0.11, "expand": 14.472, "types": 0.226, "analyze": 0.131, "verify": 15.679, "lower": 0.012, "codegen": 149.484, "ct": 7.292}]` |
| private-008 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.166, "expand": 13.252, "types": 0.173, "analyze": 0.397, "verify": 14.987, "lower": 0.011, "codegen": 123.839, "ct": 4.857}, {"parse": 0.167, "expand": 12.683, "types": 0.287, "analyze": 0.171, "verify": 22.757, "lower": 0.177, "codegen": 200.156, "ct": 14.282}]` |
| broad | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.437, "expand": 188.151, "types": 63.365, "analyze": 50.025, "verify": 50.113, "lower": 4.393, "codegen": 1584.905, "ct": 48.514}, {"parse": 0.616, "expand": 172.21, "types": 53.396, "analyze": 42.658, "verify": 32.166, "lower": 2.902, "codegen": 1736.74, "ct": 30.615}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.047, "analyze": 0.005, "verify": 32.226, "lower": 2.524, "codegen": 352.419}, {"types": 0.017, "analyze": 0.005, "verify": 33.85, "lower": 2.329, "codegen": 284.71}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large1000-edits-04-check

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 0.246 | 0.256 | 0.339 | -0.010 | 0.084 |
| broad | 1 | 0.195 | 0.212 | 0.402 | -0.018 | 0.190 |
| private | 32 | 5.858 | 1.653 | 5.693 | 4.204 | 4.040 |
| other | 7 | 0.688 | 0.448 | 0.905 | 0.239 | 0.457 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 53.738 | 53.689 | 32 | 46.625 |
| candidate | 8.145 | 6.780 | 32 | 2.752 |
| rust | 12.160 | 12.061 | 32 | 3.090 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[None, None]` | `[None, None]` | `[{"parse": 0.227, "expand": 106.775, "types": 30.926, "analyze": 18.948, "verify": 19.012, "ct": 15.669}, {"parse": 0.124, "expand": 59.945, "types": 23.755, "analyze": 18.083, "verify": 13.356, "ct": 7.782}]` |
| private-001 | `[None, None]` | `[None, None]` | `[{"parse": 0.056, "expand": 4.686, "types": 0.199, "analyze": 0.103, "verify": 7.005, "ct": 1.811}, {"parse": 0.096, "expand": 6.535, "types": 0.399, "analyze": 0.131, "verify": 11.439, "ct": 5.003}]` |
| private-002 | `[None, None]` | `[None, None]` | `[{"parse": 0.06, "expand": 8.773, "types": 0.195, "analyze": 2.014, "verify": 9.754, "ct": 2.03}, {"parse": 0.122, "expand": 11.294, "types": 0.589, "analyze": 0.622, "verify": 15.76, "ct": 2.41}]` |
| private-003 | `[None, None]` | `[None, None]` | `[{"parse": 0.065, "expand": 4.292, "types": 0.114, "analyze": 0.103, "verify": 6.902, "ct": 1.891}, {"parse": 0.057, "expand": 3.469, "types": 0.108, "analyze": 0.099, "verify": 6.472, "ct": 1.459}]` |
| private-004 | `[None, None]` | `[None, None]` | `[{"parse": 0.157, "expand": 4.201, "types": 0.108, "analyze": 0.099, "verify": 7.568, "ct": 1.477}, {"parse": 0.058, "expand": 3.785, "types": 0.109, "analyze": 0.132, "verify": 8.38, "ct": 1.594}]` |
| private-005 | `[None, None]` | `[None, None]` | `[{"parse": 0.089, "expand": 5.64, "types": 0.172, "analyze": 0.122, "verify": 11.721, "ct": 3.417}, {"parse": 0.065, "expand": 3.756, "types": 0.105, "analyze": 0.097, "verify": 5.782, "ct": 1.508}]` |
| private-006 | `[None, None]` | `[None, None]` | `[{"parse": 0.29, "expand": 13.921, "types": 0.354, "analyze": 0.534, "verify": 10.312, "ct": 4.264}, {"parse": 0.066, "expand": 3.413, "types": 0.155, "analyze": 0.096, "verify": 5.768, "ct": 1.499}]` |
| private-007 | `[None, None]` | `[None, None]` | `[{"parse": 0.148, "expand": 5.722, "types": 0.399, "analyze": 0.167, "verify": 8.623, "ct": 2.966}, {"parse": 0.113, "expand": 4.471, "types": 0.104, "analyze": 0.097, "verify": 6.74, "ct": 1.508}]` |
| private-008 | `[None, None]` | `[None, None]` | `[{"parse": 0.083, "expand": 4.935, "types": 0.119, "analyze": 0.106, "verify": 6.186, "ct": 2.41}, {"parse": 0.165, "expand": 7.304, "types": 0.222, "analyze": 0.434, "verify": 13.583, "ct": 1.999}]` |
| private-009 | `[None, None]` | `[None, None]` | `[{"parse": 0.294, "expand": 11.73, "types": 0.514, "analyze": 0.69, "verify": 8.989, "ct": 3.278}, {"parse": 0.092, "expand": 5.524, "types": 0.194, "analyze": 0.136, "verify": 5.572, "ct": 1.997}]` |
| private-010 | `[None, None]` | `[None, None]` | `[{"parse": 0.156, "expand": 4.551, "types": 0.267, "analyze": 0.115, "verify": 6.976, "ct": 1.89}, {"parse": 0.088, "expand": 3.751, "types": 0.101, "analyze": 0.121, "verify": 5.895, "ct": 1.484}]` |
| private-011 | `[None, None]` | `[None, None]` | `[{"parse": 0.082, "expand": 4.444, "types": 0.112, "analyze": 0.101, "verify": 7.446, "ct": 1.771}, {"parse": 0.069, "expand": 3.4, "types": 0.105, "analyze": 0.097, "verify": 5.486, "ct": 1.395}]` |
| private-012 | `[None, None]` | `[None, None]` | `[{"parse": 0.094, "expand": 4.965, "types": 0.163, "analyze": 0.108, "verify": 8.365, "ct": 1.949}, {"parse": 0.079, "expand": 3.604, "types": 0.107, "analyze": 0.097, "verify": 6.358, "ct": 1.478}]` |
| private-013 | `[None, None]` | `[None, None]` | `[{"parse": 0.162, "expand": 5.245, "types": 0.278, "analyze": 0.116, "verify": 7.152, "ct": 2.279}, {"parse": 0.083, "expand": 3.697, "types": 0.107, "analyze": 0.097, "verify": 5.691, "ct": 1.604}]` |
| private-014 | `[None, None]` | `[None, None]` | `[{"parse": 0.087, "expand": 6.503, "types": 0.206, "analyze": 0.401, "verify": 8.7, "ct": 1.888}, {"parse": 0.075, "expand": 3.717, "types": 0.11, "analyze": 0.098, "verify": 5.731, "ct": 1.535}]` |
| private-015 | `[None, None]` | `[None, None]` | `[{"parse": 0.086, "expand": 4.831, "types": 0.17, "analyze": 0.116, "verify": 9.009, "ct": 2.549}, {"parse": 0.077, "expand": 3.462, "types": 0.102, "analyze": 0.096, "verify": 5.875, "ct": 1.477}]` |
| private-016 | `[None, None]` | `[None, None]` | `[{"parse": 0.095, "expand": 4.173, "types": 0.147, "analyze": 0.119, "verify": 9.013, "ct": 2.121}, {"parse": 0.086, "expand": 3.749, "types": 0.111, "analyze": 0.098, "verify": 5.943, "ct": 1.621}]` |
| private-017 | `[None, None]` | `[None, None]` | `[{"parse": 0.089, "expand": 4.903, "types": 0.164, "analyze": 0.099, "verify": 6.972, "ct": 2.61}, {"parse": 0.117, "expand": 3.974, "types": 0.11, "analyze": 0.099, "verify": 5.678, "ct": 1.927}]` |
| private-018 | `[None, None]` | `[None, None]` | `[{"parse": 1.549, "expand": 18.459, "types": 0.228, "analyze": 0.525, "verify": 12.202, "ct": 4.373}, {"parse": 0.114, "expand": 3.506, "types": 0.106, "analyze": 0.098, "verify": 5.575, "ct": 1.516}]` |
| private-019 | `[None, None]` | `[None, None]` | `[{"parse": 0.122, "expand": 4.601, "types": 0.123, "analyze": 0.142, "verify": 7.401, "ct": 1.801}, {"parse": 0.472, "expand": 10.598, "types": 0.458, "analyze": 0.527, "verify": 16.162, "ct": 2.065}]` |
| private-020 | `[None, None]` | `[None, None]` | `[{"parse": 1.201, "expand": 27.46, "types": 0.307, "analyze": 0.851, "verify": 15.457, "ct": 3.841}, {"parse": 0.094, "expand": 3.776, "types": 0.111, "analyze": 0.1, "verify": 6.345, "ct": 1.546}]` |
| private-021 | `[None, None]` | `[None, None]` | `[{"parse": 0.152, "expand": 5.539, "types": 0.134, "analyze": 0.109, "verify": 7.19, "ct": 1.909}, {"parse": 0.092, "expand": 3.968, "types": 0.131, "analyze": 0.097, "verify": 5.739, "ct": 1.651}]` |
| private-022 | `[None, None]` | `[None, None]` | `[{"parse": 0.114, "expand": 4.027, "types": 0.12, "analyze": 0.108, "verify": 6.562, "ct": 1.788}, {"parse": 0.093, "expand": 3.376, "types": 0.108, "analyze": 0.097, "verify": 5.419, "ct": 1.511}]` |
| private-023 | `[None, None]` | `[None, None]` | `[{"parse": 0.186, "expand": 8.685, "types": 0.212, "analyze": 0.145, "verify": 8.618, "ct": 3.39}, {"parse": 0.095, "expand": 3.408, "types": 0.107, "analyze": 0.098, "verify": 5.721, "ct": 1.383}]` |
| private-024 | `[None, None]` | `[None, None]` | `[{"parse": 1.31, "expand": 25.814, "types": 0.398, "analyze": 1.021, "verify": 16.911, "ct": 7.619}, {"parse": 0.1, "expand": 4.974, "types": 0.153, "analyze": 0.101, "verify": 5.753, "ct": 1.454}]` |
| private-025 | `[None, None]` | `[None, None]` | `[{"parse": 0.136, "expand": 6.661, "types": 0.419, "analyze": 0.138, "verify": 8.401, "ct": 3.987}, {"parse": 0.289, "expand": 7.043, "types": 0.207, "analyze": 0.303, "verify": 8.171, "ct": 2.926}]` |
| private-026 | `[None, None]` | `[None, None]` | `[{"parse": 0.125, "expand": 5.994, "types": 0.172, "analyze": 0.263, "verify": 8.754, "ct": 3.257}, {"parse": 0.105, "expand": 3.674, "types": 0.109, "analyze": 0.125, "verify": 6.292, "ct": 1.558}]` |
| private-027 | `[None, None]` | `[None, None]` | `[{"parse": 1.419, "expand": 29.251, "types": 0.54, "analyze": 1.346, "verify": 17.493, "ct": 5.252}, {"parse": 0.108, "expand": 3.437, "types": 0.129, "analyze": 0.099, "verify": 5.685, "ct": 1.403}]` |
| private-028 | `[None, None]` | `[None, None]` | `[{"parse": 0.38, "expand": 8.117, "types": 0.263, "analyze": 0.163, "verify": 9.996, "ct": 3.885}, {"parse": 0.11, "expand": 3.43, "types": 0.109, "analyze": 0.101, "verify": 5.664, "ct": 1.522}]` |
| private-029 | `[None, None]` | `[None, None]` | `[{"parse": 0.165, "expand": 5.703, "types": 0.126, "analyze": 0.111, "verify": 12.881, "ct": 1.663}, {"parse": 0.114, "expand": 3.437, "types": 0.108, "analyze": 0.1, "verify": 5.59, "ct": 1.585}]` |
| private-030 | `[None, None]` | `[None, None]` | `[{"parse": 0.237, "expand": 5.618, "types": 0.13, "analyze": 0.136, "verify": 6.566, "ct": 3.479}, {"parse": 0.208, "expand": 4.04, "types": 0.215, "analyze": 0.102, "verify": 6.608, "ct": 1.696}]` |
| private-031 | `[None, None]` | `[None, None]` | `[{"parse": 0.14, "expand": 4.015, "types": 0.118, "analyze": 0.107, "verify": 6.973, "ct": 1.569}, {"parse": 0.117, "expand": 3.685, "types": 0.14, "analyze": 0.123, "verify": 5.686, "ct": 1.547}]` |
| private-032 | `[None, None]` | `[None, None]` | `[{"parse": 0.129, "expand": 4.323, "types": 0.14, "analyze": 0.106, "verify": 6.173, "ct": 1.998}, {"parse": 0.118, "expand": 3.574, "types": 0.113, "analyze": 0.1, "verify": 5.541, "ct": 1.603}]` |
| broad | `[None, None]` | `[None, None]` | `[{"parse": 0.426, "expand": 60.622, "types": 38.48, "analyze": 16.276, "verify": 23.168, "ct": 11.185}, {"parse": 0.616, "expand": 46.414, "types": 13.131, "analyze": 9.646, "verify": 11.469, "ct": 5.649}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.01, "analyze": 0.008, "verify": 18.199}, {"types": 0.007, "analyze": 0.005, "verify": 14.462}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large2500-04-check

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 0.239 | 0.356 | 0.609 | -0.117 | 0.253 |
| broad | 1 | 0.401 | 0.381 | 0.445 | 0.021 | 0.064 |
| private | 8 | 1.648 | 0.595 | 1.593 | 1.053 | 0.999 |
| other | 7 | 0.602 | 0.608 | 1.185 | -0.005 | 0.577 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 22.488 | 22.443 | 8 | 14.562 |
| candidate | 9.062 | 7.756 | 8 | 0.943 |
| rust | 29.301 | 29.217 | 8 | 7.547 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[None, None]` | `[None, None]` | `[{"parse": 0.129, "expand": 82.925, "types": 43.6, "analyze": 31.288, "verify": 22.028, "ct": 27.395}, {"parse": 0.136, "expand": 75.955, "types": 31.898, "analyze": 36.701, "verify": 21.39, "ct": 14.847}]` |
| private-001 | `[None, None]` | `[None, None]` | `[{"parse": 0.082, "expand": 14.93, "types": 0.344, "analyze": 0.556, "verify": 19.139, "ct": 5.433}, {"parse": 0.065, "expand": 8.872, "types": 0.266, "analyze": 0.422, "verify": 13.21, "ct": 4.257}]` |
| private-002 | `[None, None]` | `[None, None]` | `[{"parse": 0.056, "expand": 11.944, "types": 0.097, "analyze": 0.093, "verify": 10.178, "ct": 4.587}, {"parse": 0.059, "expand": 6.741, "types": 0.35, "analyze": 0.157, "verify": 10.311, "ct": 3.88}]` |
| private-003 | `[None, None]` | `[None, None]` | `[{"parse": 0.057, "expand": 6.399, "types": 0.146, "analyze": 0.094, "verify": 10.433, "ct": 5.648}, {"parse": 0.054, "expand": 5.879, "types": 0.094, "analyze": 0.094, "verify": 9.99, "ct": 3.894}]` |
| private-004 | `[None, None]` | `[None, None]` | `[{"parse": 0.52, "expand": 42.401, "types": 0.72, "analyze": 0.683, "verify": 26.004, "ct": 9.458}, {"parse": 0.105, "expand": 23.799, "types": 0.537, "analyze": 0.621, "verify": 24.178, "ct": 5.295}]` |
| private-005 | `[None, None]` | `[None, None]` | `[{"parse": 0.089, "expand": 6.735, "types": 0.167, "analyze": 0.101, "verify": 13.269, "ct": 4.503}, {"parse": 0.061, "expand": 6.042, "types": 0.093, "analyze": 0.094, "verify": 9.372, "ct": 3.696}]` |
| private-006 | `[None, None]` | `[None, None]` | `[{"parse": 0.07, "expand": 7.042, "types": 0.109, "analyze": 0.099, "verify": 13.113, "ct": 5.055}, {"parse": 0.062, "expand": 5.849, "types": 0.092, "analyze": 0.095, "verify": 9.656, "ct": 3.456}]` |
| private-007 | `[None, None]` | `[None, None]` | `[{"parse": 0.078, "expand": 6.887, "types": 0.109, "analyze": 0.104, "verify": 12.042, "ct": 5.469}, {"parse": 0.063, "expand": 6.728, "types": 0.09, "analyze": 0.093, "verify": 9.484, "ct": 3.675}]` |
| private-008 | `[None, None]` | `[None, None]` | `[{"parse": 0.071, "expand": 8.235, "types": 0.208, "analyze": 0.286, "verify": 10.548, "ct": 4.404}, {"parse": 0.182, "expand": 13.316, "types": 0.101, "analyze": 0.098, "verify": 12.188, "ct": 6.186}]` |
| broad | `[None, None]` | `[None, None]` | `[{"parse": 0.386, "expand": 65.1, "types": 19.612, "analyze": 19.61, "verify": 21.788, "ct": 11.072}, {"parse": 0.391, "expand": 64.847, "types": 19.55, "analyze": 20.945, "verify": 22.238, "ct": 10.287}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.007, "analyze": 0.004, "verify": 24.112}, {"types": 0.013, "analyze": 0.004, "verify": 18.178}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large2500-chain-04-o2

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 2.531 | 2.462 | 1.796 | 0.069 | -0.666 |
| broad | 1 | 2.278 | 2.595 | 1.706 | -0.317 | -0.889 |
| private | 8 | 5.412 | 2.585 | 4.190 | 2.828 | 1.606 |
| other | 7 | 2.537 | 1.808 | 2.805 | 0.729 | 0.997 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 26.902 | 26.796 | 8 | 14.875 |
| candidate | 12.375 | 11.150 | 8 | 0.916 |
| rust | 43.180 | 42.972 | 8 | 11.707 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.136, "expand": 78.431, "types": 31.28, "analyze": 27.636, "verify": 17.379, "lower": 1.057, "codegen": 1735.337, "ct": 13.58}, {"parse": 0.242, "expand": 77.383, "types": 38.165, "analyze": 43.486, "verify": 42.727, "lower": 1.379, "codegen": 2315.334, "ct": 13.346}]` |
| private-001 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.059, "expand": 9.903, "types": 0.273, "analyze": 0.244, "verify": 12.441, "lower": 0.012, "codegen": 133.556, "ct": 5.08}, {"parse": 0.082, "expand": 16.301, "types": 1.496, "analyze": 0.626, "verify": 603.575, "lower": 0.724, "codegen": 357.104, "ct": 18.785}]` |
| private-002 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.067, "expand": 8.622, "types": 0.199, "analyze": 0.113, "verify": 11.168, "lower": 0.01, "codegen": 137.388, "ct": 4.833}, {"parse": 0.084, "expand": 19.483, "types": 0.154, "analyze": 0.287, "verify": 15.603, "lower": 0.011, "codegen": 153.306, "ct": 6.808}]` |
| private-003 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.063, "expand": 13.185, "types": 0.145, "analyze": 0.362, "verify": 12.803, "lower": 0.01, "codegen": 142.212, "ct": 5.838}, {"parse": 0.076, "expand": 10.601, "types": 0.161, "analyze": 0.11, "verify": 11.02, "lower": 0.01, "codegen": 153.056, "ct": 8.34}]` |
| private-004 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.132, "expand": 15.28, "types": 0.102, "analyze": 0.107, "verify": 18.015, "lower": 0.011, "codegen": 182.017, "ct": 4.812}, {"parse": 0.081, "expand": 9.293, "types": 0.16, "analyze": 0.56, "verify": 13.822, "lower": 0.057, "codegen": 135.677, "ct": 6.807}]` |
| private-005 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.113, "expand": 6.688, "types": 0.105, "analyze": 0.098, "verify": 10.635, "lower": 0.011, "codegen": 282.588, "ct": 4.729}, {"parse": 0.137, "expand": 18.551, "types": 0.181, "analyze": 0.24, "verify": 17.217, "lower": 0.011, "codegen": 141.086, "ct": 9.565}]` |
| private-006 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.078, "expand": 18.216, "types": 0.175, "analyze": 0.205, "verify": 11.805, "lower": 0.01, "codegen": 141.23, "ct": 10.122}, {"parse": 0.074, "expand": 14.139, "types": 0.421, "analyze": 0.562, "verify": 14.392, "lower": 0.061, "codegen": 135.307, "ct": 5.203}]` |
| private-007 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.081, "expand": 12.622, "types": 0.169, "analyze": 0.118, "verify": 15.407, "lower": 0.018, "codegen": 138.114, "ct": 6.984}, {"parse": 0.078, "expand": 9.431, "types": 0.229, "analyze": 0.169, "verify": 12.895, "lower": 0.012, "codegen": 145.59, "ct": 4.19}]` |
| private-008 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.087, "expand": 12.057, "types": 0.262, "analyze": 0.113, "verify": 14.472, "lower": 0.011, "codegen": 157.18, "ct": 7.964}, {"parse": 0.144, "expand": 9.252, "types": 0.224, "analyze": 0.107, "verify": 12.429, "lower": 0.01, "codegen": 152.893, "ct": 4.548}]` |
| broad | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.419, "expand": 165.496, "types": 50.601, "analyze": 42.406, "verify": 40.117, "lower": 5.039, "codegen": 1925.549, "ct": 32.319}, {"parse": 0.784, "expand": 119.233, "types": 60.129, "analyze": 44.638, "verify": 29.055, "lower": 2.456, "codegen": 2098.035, "ct": 17.876}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.007, "analyze": 0.005, "verify": 38.995, "lower": 5.064, "codegen": 850.085}, {"types": 0.014, "analyze": 0.005, "verify": 16.954, "lower": 0.773, "codegen": 217.036}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large2500-diamond-04-o2

| Group | Actual frequency | Main s | Candidate s | Rust s | Saved/main s | Saved/Rust s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cold | 1 | 2.977 | 2.335 | 1.795 | 0.642 | -0.539 |
| broad | 1 | 2.343 | 2.245 | 1.308 | 0.098 | -0.937 |
| private | 8 | 4.399 | 2.259 | 3.190 | 2.140 | 0.932 |
| other | 7 | 2.376 | 1.302 | 2.586 | 1.074 | 1.284 |

Disk: sum categories for one arm at each snapshot, THEN take MAX across snapshots/blocks. Growth is the median of block-specific cold→unchanged-after-edits endpoints.

| Arm | Physical peak MiB | Logical peak MiB | Private count | Private physical growth MiB |
| --- | ---: | ---: | ---: | ---: |
| baseline | 26.902 | 26.801 | 8 | 14.875 |
| candidate | 12.402 | 11.162 | 8 | 0.973 |
| rust | 43.129 | 42.936 | 8 | 11.656 |

| Request | Candidate reused functions by block | Fallback units by block | Candidate phase ms by block |
| --- | --- | --- | --- |
| cold | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.18, "expand": 127.743, "types": 87.954, "analyze": 31.164, "verify": 25.725, "lower": 1.574, "codegen": 1819.43, "ct": 26.333}, {"parse": 0.253, "expand": 71.222, "types": 43.276, "analyze": 31.313, "verify": 19.113, "lower": 0.892, "codegen": 1933.26, "ct": 12.787}]` |
| private-001 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.066, "expand": 10.662, "types": 0.212, "analyze": 0.128, "verify": 11.662, "lower": 0.012, "codegen": 147.266, "ct": 5.302}, {"parse": 0.077, "expand": 46.495, "types": 1.451, "analyze": 0.453, "verify": 21.144, "lower": 0.081, "codegen": 160.076, "ct": 7.077}]` |
| private-002 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.104, "expand": 27.511, "types": 0.146, "analyze": 0.241, "verify": 23.84, "lower": 0.013, "codegen": 169.051, "ct": 4.562}, {"parse": 0.068, "expand": 8.474, "types": 0.104, "analyze": 0.096, "verify": 11.462, "lower": 0.01, "codegen": 126.103, "ct": 4.958}]` |
| private-003 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.061, "expand": 7.275, "types": 0.102, "analyze": 0.099, "verify": 11.38, "lower": 0.01, "codegen": 391.91, "ct": 5.899}, {"parse": 0.062, "expand": 6.424, "types": 0.095, "analyze": 0.095, "verify": 9.937, "lower": 0.009, "codegen": 207.459, "ct": 3.892}]` |
| private-004 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.107, "expand": 21.478, "types": 0.123, "analyze": 0.299, "verify": 15.825, "lower": 0.01, "codegen": 142.103, "ct": 4.942}, {"parse": 0.066, "expand": 7.169, "types": 0.111, "analyze": 0.099, "verify": 9.756, "lower": 0.009, "codegen": 300.869, "ct": 6.626}]` |
| private-005 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.089, "expand": 14.594, "types": 0.142, "analyze": 0.222, "verify": 14.924, "lower": 0.01, "codegen": 149.982, "ct": 4.563}, {"parse": 0.082, "expand": 10.054, "types": 0.124, "analyze": 0.285, "verify": 15.968, "lower": 0.012, "codegen": 124.547, "ct": 4.352}]` |
| private-006 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.748, "expand": 61.372, "types": 0.677, "analyze": 0.413, "verify": 25.17, "lower": 0.01, "codegen": 177.776, "ct": 7.396}, {"parse": 0.082, "expand": 10.446, "types": 0.146, "analyze": 0.101, "verify": 11.454, "lower": 0.01, "codegen": 144.386, "ct": 4.345}]` |
| private-007 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.083, "expand": 8.463, "types": 0.123, "analyze": 0.231, "verify": 10.278, "lower": 0.011, "codegen": 141.427, "ct": 4.468}, {"parse": 0.068, "expand": 8.512, "types": 0.189, "analyze": 0.106, "verify": 11.144, "lower": 0.011, "codegen": 126.969, "ct": 4.237}]` |
| private-008 | `[148, 148]` | `[0, 0]` | `[{"parse": 0.336, "expand": 21.14, "types": 0.32, "analyze": 0.225, "verify": 19.528, "lower": 0.011, "codegen": 148.21, "ct": 6.444}, {"parse": 0.187, "expand": 12.234, "types": 0.135, "analyze": 0.224, "verify": 14.638, "lower": 0.011, "codegen": 148.273, "ct": 5.437}]` |
| broad | `[2092, 2092]` | `[0, 0]` | `[{"parse": 0.422, "expand": 170.858, "types": 54.402, "analyze": 43.639, "verify": 33.595, "lower": 2.064, "codegen": 1643.713, "ct": 38.645}, {"parse": 0.398, "expand": 151.972, "types": 52.927, "analyze": 37.412, "verify": 24.321, "lower": 2.002, "codegen": 1741.143, "ct": 31.41}]` |
| warm-restart | `[None, None]` | `[None, None]` | `[{"types": 0.016, "analyze": 0.008, "verify": 21.389, "lower": 0.813, "codegen": 230.408}, {"types": 0.009, "analyze": 0.005, "verify": 24.521, "lower": 1.597, "codegen": 238.143}]` |

JSON retains every request/block, object receipt, actual reused-function counts, fallback flags, selection/assembly/reconstruction timings, backend/frontend counters and peak witnesses. `null` means not observed. The table credits only non-fallback emission receipts; JSON also retains raw attempted reuse counts. Phase and nested emission timings must not be added as disjoint wall time.

## large-project-large1000-edits-04-o2

Comparisons remain empty; no workflow or storage advantage is inferred.

`{"block": 0, "index": 0, "request": "cold", "arm": "rust", "error": "ValueError: three-arm request/native/CGU16/full-ELF quality failed"}`

Recorded rows: 3/246. Quality: `[{"schema": "native-link-exact-elf-reviewed-ir-v1", "valid": false, "semantics_equal": true, "executable_equal": false, "llvm_text_equal": false, "raw_llvm_equal": false, "llvm_equal_excluding_identity_comment": false, "per_unit_exports_equal_observed": true, "aggregate_zil_function_exports_equal": true, "native_oracles_valid": true, "physical_zil_units_valid": true, "llvm_equality_is_acceptance_prerequisite": false, "block": 0, "index": 0, "request": "cold"}]`

## Original5105 capacity

State: recorded; comparisons=[]; formal_ratio=null. Main ResourceLimit is a failed capacity request, never a fast timing reference. Historical Rust receipts remain explicitly historical.

Recorded complete=True, protocol_valid=True, candidate_full_source_verify_native_success=True.

| Arm | Mode/request | Status | Current Verify runs | Wall ms (diagnostic only) | Emission reuse events |
| --- | --- | --- | ---: | ---: | ---: |
| candidate | check/cold | Success | 1 | 1685.522 | None |
| candidate | check/warm | Success | 1 | 9.171 | None |
| candidate | o2/cold | Success | 1 | 11552.735 | 0 |
| candidate | o2/warm | Success | 1 | 18.375 | None |
| baseline | check/cold | ResourceLimit | 0 | 425.763 | None |
| baseline | check/warm | ResourceLimit | 0 | 323.397 | None |
| baseline | o2/cold | ResourceLimit | 0 | 394.547 | None |
| baseline | o2/warm | ResourceLimit | 0 | 904.281 | None |
| rust | check/historical-cold [historical receipt] | Success | None | — | None |
| rust | o2/cold | Success | None | — | None |
| rust | o2/warm | Success | None | — | None |

Candidate check/O2 cold and warm preserve all 102 source files and current Verify; the two successful candidate O2 requests each retain 12 native oracle observations. Candidate O2 cold records zero emission reuses and zero emission fallback units. No candidate/main timing ratio is valid because main fails Expand at the default budget.

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

`python3 -B /workspace/zil-recovery/critical-path-weighted-analysis.py --analyze`
