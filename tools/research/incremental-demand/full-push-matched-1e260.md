# Matched three-arm results

Candidate `1e260a4ed8898e1ce92ae6f0b7931b47d4dceb7d`, tree `d1461e76726b6641e766efd6d441a74661a93900`, binary SHA256 `e2411c9d010cc7c8a8d4a21d40563384118568faba094b795d2cdad997ddb393`.
Main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`; Rust 1.99.1 commit `58cf8f9c35b355199727627d07ae34a4e2846d67`.

[JSON summary](full-push-matched-1e260.json) retains all six observations and original CPU/RSS field names, named preparations, exact raw-file/build/tool hashes, actual unit content examples and current reuse observations. Raw archive status: check: full fixed v14/8d10022 fresh HTTPS recovery plus captured original validator passed; summary reads verified 20-file projection. O2: original full validator passed; fixed archive/source-history sharded archive/recovery in progress, not yet claimed durable..

Each mode validates 270 rows: 144 measured and 126 named parent preparations. Native execution counts and verified blobs are the exact validator receipts in JSON.

Wall medians in ms; each cell is main / candidate / Rust. All individual six-block values remain in JSON.

| Request | check | O2 |
|---|---:|---:|
| cold | 678.505 / 658.042 / 1502.017 | 6164.063 / 7616.131 / 6819.087 |
| unchanged | 90.312 / 3.316 / 13.800 | 659.313 / 63.766 / 13.467 |
| private | 659.219 / 79.985 / 251.351 | 1855.035 / 560.182 / 593.243 |
| multi | 573.268 / 89.529 / 228.810 | 1730.771 / 758.382 / 1038.154 |
| broad | 768.335 / 626.168 / 1078.267 | 5567.459 / 7070.189 / 5826.406 |
| restore | 104.440 / 28.202 / 251.746 | 524.525 / 166.038 / 495.323 |
| error | 483.186 / 45.981 / 206.855 | 337.071 / 50.349 / 215.440 |
| recovery | 769.036 / 61.279 / 227.281 | 1834.174 / 516.202 / 515.146 |

Candidate medians slower than their same-window comparator (descriptive observations, not significance claims):

- o2/cold: candidate 7616.131 ms > main 6164.063 ms
- o2/cold: candidate 7616.131 ms > Rust 6819.087 ms
- o2/unchanged: candidate 63.766 ms > Rust 13.467 ms
- o2/broad: candidate 7070.189 ms > main 5567.459 ms
- o2/broad: candidate 7070.189 ms > Rust 5826.406 ms
- o2/recovery: candidate 516.202 ms > Rust 515.146 ms

CGU16/O2/jobs1 is matched native configuration, not matched partitions or optimizer/safety work. Rust embedded LLVM is in rustc -vV; observed Zil native LLVM versions: 23.1.4. Cargo check uses its dev profile and generates no native objects; Zil check executes independent Verify. Rust uses a new Cargo invocation; Zil is resident. Check and native are separate windows. Source writes, unit inventory and native oracle execution are excluded from compile wall.

CPU and RSS scopes differ: Zil /proc root CPU deltas and root response/cumulative-peak RSS; Rust wait4 invocation-tree CPU, Cargo-root sampled RSS, and largest-process maxrss. Do not infer cross-language memory superiority. Physical Rust object carry-forward and Zil object-cache hit receipts are distinct observations.

These are whole-request results for the stated fixed workload and source. No prior-candidate timing comparison, isolated-slice attribution, universal minimal-work claim or cross-language parity is implied.
