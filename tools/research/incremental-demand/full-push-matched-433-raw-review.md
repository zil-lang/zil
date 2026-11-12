# 433 formal raw-record review

Measured source `433227ab1056d778ec382aa28c7e0b6a8cc1cfe0`, tree `556cd496776d03e6947dac35521c0e9aeba3aafd`; candidate ELF SHA256 `76a8204834d96353e9c21cf626f412424d9ce735e8f22963af7434b560db128c`. New validator `5d2e3dd849b95d75c2c8a88aadf1a0379e982170` accepted both complete immutable archives after correcting the requested-vs-resolved Cargo argv[0] false rejection. Original failure and archived implementation were preserved.

Tool contributor performing a separate raw-record audit; this is not independent tool-design review. No new build or measurement. Each mode has 270 rows: 144 measured and 126 named-parent requests. O2: 2,808 native oracles, 477 verified blobs; check: 0 native, 114 verified blobs.

Displayed values below are milliseconds rounded to three decimals, six observations in block order; median in parentheses. The JSON preserves every full original numeric value. These are descriptive observations, not significance estimates.

## o2

| Scene | main42 six values (median) | candidate six values (median) | Rust six values (median) |
|---|---|---|---|
| cold | 5942.540, 6222.891, 6342.140, 6453.946, 6357.031, 6071.805 (6282.515) | 8772.054, 9758.406, 9847.011, 9108.225, 10279.773, 9913.097 (9802.708) | 6089.347, 7442.103, 6433.451, 6439.009, 7506.434, 6574.857 (6506.933) |
| unchanged | 537.738, 454.636, 505.283, 801.147, 543.673, 961.044 (540.705) | 54.206, 57.122, 143.574, 69.292, 74.182, 90.277 (71.737) | 22.632, 11.444, 13.769, 13.129, 17.206, 16.425 (15.097) |
| private | 2316.359, 1361.306, 1911.454, 1506.691, 1871.961, 1627.407 (1749.684) | 625.421, 698.350, 895.272, 505.528, 617.359, 656.193 (640.807) | 727.891, 886.039, 705.294, 569.000, 581.230, 703.579 (704.436) |
| multi | 1775.969, 1679.382, 1649.180, 1936.361, 1796.533, 1750.062 (1763.015) | 1057.324, 834.015, 853.731, 922.042, 873.752, 1070.277 (897.897) | 879.961, 1271.998, 813.100, 971.737, 1184.408, 968.800 (970.268) |
| broad | 5676.945, 5463.545, 5822.119, 5287.172, 5214.218, 5469.239 (5466.392) | 8646.325, 8001.592, 8701.888, 7967.270, 8858.607, 8753.903 (8674.107) | 5419.459, 5584.526, 6168.401, 5780.356, 6093.259, 5827.696 (5804.026) |
| restore | 652.688, 510.548, 811.079, 748.602, 561.111, 589.731 (621.210) | 151.131, 158.117, 173.931, 195.753, 173.809, 161.470 (167.639) | 584.856, 482.240, 499.510, 505.950, 541.926, 474.592 (502.730) |
| error | 342.641, 367.175, 363.628, 573.570, 361.818, 398.411 (365.401) | 60.326, 55.764, 58.781, 75.338, 88.238, 55.855 (59.553) | 248.019, 215.393, 240.041, 295.493, 197.418, 309.735 (244.030) |
| recovery | 1610.285, 2062.366, 1690.278, 1759.956, 1734.979, 1816.226 (1747.468) | 963.116, 1404.968, 1011.202, 1099.937, 995.114, 1042.522 (1026.862) | 476.937, 583.013, 462.710, 467.249, 538.040, 568.586 (507.488) |

## check

| Scene | main42 six values (median) | candidate six values (median) | Rust six values (median) |
|---|---|---|---|
| cold | 1509.100, 772.680, 695.913, 1441.177, 753.762, 1149.542 (961.111) | 1508.164, 1132.807, 647.835, 568.153, 1012.895, 780.297 (896.596) | 1707.485, 949.705, 1454.625, 980.147, 1233.487, 921.707 (1106.817) |
| unchanged | 93.086, 98.880, 89.158, 92.148, 94.954, 87.040 (92.617) | 2.989, 2.963, 3.240, 3.131, 2.936, 3.376 (3.060) | 51.589, 13.664, 12.873, 21.689, 17.290, 13.153 (15.477) |
| private | 1060.709, 631.486, 501.398, 650.160, 538.362, 572.640 (602.063) | 84.703, 127.402, 527.700, 115.965, 65.531, 127.951 (121.683) | 375.858, 660.101, 202.930, 264.398, 271.073, 698.018 (323.465) |
| multi | 1017.176, 597.190, 602.748, 773.424, 492.483, 999.095 (688.086) | 111.440, 356.497, 102.833, 125.619, 116.747, 117.037 (116.892) | 313.078, 230.658, 245.326, 250.999, 223.686, 241.154 (243.240) |
| broad | 780.843, 735.093, 743.948, 1212.636, 974.134, 844.620 (812.732) | 926.190, 519.363, 740.360, 618.826, 827.448, 596.804 (679.593) | 1420.075, 1296.640, 1061.936, 1261.333, 862.627, 948.076 (1161.635) |
| restore | 94.900, 113.925, 96.313, 149.084, 95.910, 90.933 (96.112) | 23.917, 23.748, 26.276, 178.949, 23.646, 29.111 (25.096) | 310.829, 238.608, 242.903, 311.441, 185.573, 242.319 (242.611) |
| error | 369.324, 432.806, 550.036, 526.842, 739.114, 381.541 (479.824) | 60.924, 53.435, 88.416, 76.085, 101.708, 63.200 (69.643) | 379.734, 203.622, 255.310, 305.320, 187.056, 206.100 (230.705) |
| recovery | 803.469, 802.198, 854.010, 835.310, 775.304, 727.776 (802.834) | 187.005, 163.665, 267.181, 186.032, 198.438, 159.700 (186.519) | 237.809, 191.513, 353.344, 519.463, 591.444, 255.644 (304.494) |

## Material limits

- Six balanced sequential three-arm blocks; 8 measured scenes and all named parents. Formal windows have no profile flag; profile06 single-observation numbers are excluded.
- Native O2, requested CGU16, codegen jobs1; same requested CGU does not mean equal partition membership or optimizer work. Check and O2 are separate windows.
- Rust Cargo --offline --locked --jobs 1, CARGO_BUILD_JOBS=1, incremental=true, release opt-level=2/CGU16/lto=false. Exact actual Cargo argv and verbose rustc command are retained; no helper shell or wrapper replaces the compiler.
- Cargo uses its normal jobserver control under --jobs 1. This archive does not sample every rustc thread or jobserver token, so it does not establish one OS thread or identical scheduling to Zil codegen jobs1. No jobserver warning appears in all 90 O2 Cargo stderr records.
- Zil uses a resident process; every Rust request launches a fresh Cargo process. Zil startup is excluded while Cargo/rustc startup is included. OS caches were not flushed; DSO archiving outside timing can warm file cache.
- Zil root CPU is /proc utime+stime; resident_and_waited CPU adds waited children. Cargo wait4 CPU includes waited descendants. These are different scopes.
- Zil RSS is response-time root RSS and cumulative resident HWM; Cargo sampled RSS is the Cargo root, and wait4 maxrss is largest-process high water, not a tree sum. No cross-language memory advantage is inferred.
- Rust reports embedded LLVM 23.1.1; actual mapped baseline/candidate Zil DSOs are separately archived SDK 23.1.4 and patched v15 23.1.4 with different hashes. Compiler safety/optimizer obligations differ.
- Zil successful rows require one current independent Verify; the error rejects in Types with no Verify or new publication. Cargo check is its dev checking workflow and does not generate native objects; its guarantees are not equivalent to Zil Verify.
- Main/candidate complete ELF bytes are equal. LLVM comparison permits only the strict second-line verified-fingerprint hex64 value; all remaining bytes must match. This does not assert Rust binary/LLVM equality.
- Object cache hit state and raw physical inode/mtime/content carry-forward are observations. Equal bytes alone are not proof of skipped work; Rust observations are not compiler red/green counters.
- Native hook counters and MIR scope timers describe the first hooked attempt. Subsequent ordinary fallback per-pass work is unknown; full native phase/total/request wall include all retries. MIR timers are not a total-time partition.
- This bounded 425-declaration/22-source scalar fixture does not establish whole-language incremental coverage or that candidate is globally faster than Rust. O2 cold, unchanged, broad and recovery remain negative versus Rust.

Actual unit example: main/candidate private changed unit 2 contains 42 defined `zil.fn` members, while the Rust changed object is unit 5 and defines `m0::value`. Both inventories have 16 current successful objects; that is not matching partitions. All Rust fixtures (176 files) have no inline(never)/black_box, and the representative complete Zil LLVM has no noinline/alwaysinline attributes.

Full six-value CPU/RSS data, exact raw paths/hashes, DSO/source/build bindings, unit members/reuse observations and corrected validator receipts are in `full-push-matched-433-raw-review.json`.
