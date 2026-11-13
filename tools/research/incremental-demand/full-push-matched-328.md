# Matched three-arm results

Candidate `328eb392d8d88148a75a743d899b43aebae73587`, tree `f8cf388608f0afe700ab76a0abb156239d68ae8d`, binary SHA256 `e7d8b194edad4dca5db8d1f2c3d38fa2c0ba18420bc6d7eb9e9e4b2f00c8dc5a`.
Main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`; Rust 1.99.1 commit `58cf8f9c35b355199727627d07ae34a4e2846d67`.

[JSON summary](full-push-matched-328.json) retains all six observations and original CPU/RSS field names, named preparations, exact raw-file/build/tool hashes, actual unit content examples and current reuse observations. Raw archive status: check03: fixed archive/source-history@57bafe3fa331ff155bd94029d5dd0921f8b236e6, all 3058 files/728179825 bytes; O2-03: fixed archive/source-history@4985eabf85b543d083181f8aa0ca1a5e7b80f572, all 21672 files/2313541824 bytes. Each storage piece freshly recovered via HTTPS and all file hashes/modes plus exact disjoint original-master union verified. Original full validators passed before retirement; no assembled whole fresh validator rerun is claimed. Receipts: full-push-recovery-v17.json and full-push-recovery-v18.json.

Each mode validates 270 rows: 144 measured and 126 named parent preparations. Native execution counts and verified blobs are the exact validator receipts in JSON.

Wall medians in ms; each cell is main / candidate / Rust. All individual six-block values remain in JSON.

| Request | check | O2 |
|---|---:|---:|
| cold | 1218.514 / 851.864 / 1052.319 | 6251.237 / 7865.769 / 6881.102 |
| unchanged | 97.066 / 3.205 / 14.398 | 522.862 / 62.755 / 15.415 |
| private | 585.530 / 69.239 / 421.587 | 1561.521 / 555.363 / 705.634 |
| multi | 708.018 / 100.695 / 210.015 | 1820.972 / 728.836 / 1022.350 |
| broad | 816.192 / 685.830 / 1133.392 | 5512.620 / 7012.454 / 5821.477 |
| restore | 89.655 / 25.684 / 217.836 | 531.663 / 179.855 / 540.280 |
| error | 404.951 / 46.564 / 198.691 | 363.505 / 52.056 / 236.442 |
| recovery | 686.648 / 58.711 / 215.022 | 1849.760 / 527.408 / 564.604 |

Candidate medians slower than their same-window comparator (descriptive observations, not significance claims):

- o2/cold: candidate 7865.769 ms > main 6251.237 ms
- o2/cold: candidate 7865.769 ms > Rust 6881.102 ms
- o2/unchanged: candidate 62.755 ms > Rust 15.415 ms
- o2/broad: candidate 7012.454 ms > main 5512.620 ms
- o2/broad: candidate 7012.454 ms > Rust 5821.477 ms

CGU16/O2/jobs1 is matched native configuration, not matched partitions or optimizer/safety work. Rust embedded LLVM is in rustc -vV; observed Zil native LLVM versions: 23.1.4. Cargo check uses its dev profile and generates no native objects; Zil check executes independent Verify. Rust uses a new Cargo invocation; Zil is resident. Check and native are separate windows. Source writes, unit inventory and native oracle execution are excluded from compile wall.

CPU and RSS scopes differ: Zil /proc root CPU deltas and root response/cumulative-peak RSS; Rust wait4 invocation-tree CPU, Cargo-root sampled RSS, and largest-process maxrss. Do not infer cross-language memory superiority. Physical Rust object carry-forward and Zil object-cache hit receipts are distinct observations.

These are whole-request results for the stated fixed workload and source. No prior-candidate timing comparison, isolated-slice attribution, universal minimal-work claim or cross-language parity is implied.

Paired candidate/Rust geometric wall ratios: private 0.844, recovery 1.092. Recovery has a favorable median but an unfavorable paired geometric ratio; it is not an unqualified win. Six blocks do not establish significance or universal superiority.
