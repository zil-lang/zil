# Matched three-arm results

Candidate `900293cfd8afcd7118775e8165e1ec4f5266e3d7`, tree `e3f16832a9751f25399d4dd8e58488ed03513f54`, binary SHA256 `936d7c107713c11dca86bbf196fc9a0c6ceee336ef4f57e60751543ba1817d82`.
Main `99035931d358add0eaed6d4535035c38413701dd`; Rust 1.99.1 commit `58cf8f9c35b355199727627d07ae34a4e2846d67`.

[JSON summary](full-pipeline-matched-900.json) retains all six observations and original CPU/RSS field names, named preparations, exact raw-file/build/tool hashes, actual unit content examples and current reuse observations. Raw archive: [fixed v4](https://github.com/zil-lang/zil/tree/32e1b04fb859440bb36ec3223cb35b26edf63041/research/incremental-demand-full-pipeline-20261112-v4); fresh HTTPS recovery verified24,642 files/2,600,107,619 raw bytes and all validators. See [recovery receipt](full-pipeline-recovery-v4.json) and [independent evidence review](full-pipeline-final-evidence-review.json).

Each mode validates 270 rows: 144 measured and 126 named parent preparations. Native execution counts and verified blobs are the exact validator receipts in JSON.

Wall medians in ms; each cell is main / candidate / Rust. All individual six-block values remain in JSON.

| Request | check | O2 |
|---|---:|---:|
| cold | 775.875 / 830.200 / 1518.591 | 7743.671 / 7843.636 / 7789.748 |
| unchanged | 98.451 / 4.132 / 12.337 | 559.256 / 73.568 / 13.932 |
| private | 625.834 / 276.374 / 282.428 | 1611.691 / 902.955 / 581.726 |
| multi | 1042.250 / 289.290 / 239.214 | 1830.269 / 1130.963 / 1079.397 |
| broad | 1594.496 / 1470.706 / 1328.574 | 5846.336 / 6463.730 / 6150.068 |
| restore | 108.948 / 97.985 / 243.195 | 594.841 / 226.273 / 696.840 |
| error | 654.374 / 113.615 / 302.516 | 409.966 / 89.451 / 320.945 |
| recovery | 1089.824 / 299.443 / 207.615 | 2128.731 / 785.298 / 696.138 |

Candidate medians slower than their same-window comparator (descriptive observations, not significance claims):

- check/cold: candidate 830.200 ms > main 775.875 ms
- check/multi: candidate 289.290 ms > Rust 239.214 ms
- check/broad: candidate 1470.706 ms > Rust 1328.574 ms
- check/recovery: candidate 299.443 ms > Rust 207.615 ms
- o2/cold: candidate 7843.636 ms > main 7743.671 ms
- o2/cold: candidate 7843.636 ms > Rust 7789.748 ms
- o2/unchanged: candidate 73.568 ms > Rust 13.932 ms
- o2/private: candidate 902.955 ms > Rust 581.726 ms
- o2/multi: candidate 1130.963 ms > Rust 1079.397 ms
- o2/broad: candidate 6463.730 ms > main 5846.336 ms
- o2/broad: candidate 6463.730 ms > Rust 6150.068 ms
- o2/recovery: candidate 785.298 ms > Rust 696.138 ms

CGU16/O2/jobs1 is matched native configuration, not matched partitions or optimizer/safety work. Rust embedded LLVM is in rustc -vV; observed Zil native LLVM versions: 23.1.4. Cargo check uses its dev profile and generates no native objects; Zil check executes independent Verify. Rust uses a new Cargo invocation; Zil is resident. Check and native are separate windows. Source writes, unit inventory and native oracle execution are excluded from compile wall.

CPU and RSS scopes differ: Zil /proc root CPU deltas and root response/cumulative-peak RSS; Rust wait4 invocation-tree CPU, Cargo-root sampled RSS, and largest-process maxrss. Do not infer cross-language memory superiority. Physical Rust object carry-forward and Zil object-cache hit receipts are distinct observations.

These are whole-request results for the stated fixed workload and source. No prior-candidate timing comparison, isolated-slice attribution, universal minimal-work claim or cross-language parity is implied.

Successful native requests have16 actual current units for every arm; complete emitted symbol/content and reuse observations are in JSON and raw. This does not establish equal partitions. The disposable debug build cache was removed only between completed check and O2 windows; timing receipts preserve that boundary.
