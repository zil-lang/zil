# 109: bounded NS selection after the fixed five-arm experiment

Root selected the exact ordinary-library plus Linux external-binding implementation for **limited production admission**, conditional on independent full archive verification, main-tree integration and the unchanged 35 required gates. The preregistered NS arm passes the original numerical thresholds in the six runtime cells, six startup cells and all twelve cells. This report records a finite experiment and an exact source decision; whole-product performance/conformance is not established by these measurements.

The three 4 MiB NS programs remain slower than the original Rust programs: stream efficiency is 0.988765524, buffer 0.949063705 and retained 0.907300115. Efficiency means `T_Rust / T_NS`; 0.9 permits an NS time up to 1/0.9 times Rust. Retained is close to that boundary. No interval, pooled result or later arm selection replaces the fixed per-case point rule.

A material negative control limits causal and general claims: retained N and NS are byte-identical executables with identical normalized invocations apart from their preassigned executable paths, yet their 4 MiB arm medians are 39.772567 ms and 46.885576 ms. NS/N is 1.178842 by the ratio of medians. This approximately 17.9% difference cannot be a source-factor effect. System variability, order and other non-algorithmic conditions remain; their specific contribution was not identified. The paired median is 1.049133 with descriptive interval [0.989751, 1.157340], which is a different estimator from the ratio of medians. This discrepancy is not removed, pooled away or treated as stable performance evidence.

The independent task 108 candidate remains **NoGo**, with its complete archive and production rollback. Task 109 neither changes that decision nor combines old 104/108 timing samples with the new campaign.

## Exact source and experiment boundary

| Arm | Compiler/source factor |
| --- | --- |
| BASE | Baseline ab285414c9bf97cdd2a37680c087f31db1623e5c; unchanged original consumer sources |
| N | Exact historical 108 foreign binding file; unchanged original consumers |
| S | Baseline compiler; ordinary explicit byte-map source only in buffer |
| NS | N compiler and S buffer source; sole preregistered admission candidate |
| Rust | Original 104 idiomatic source and the same six sealed plain executable bytes |

The source candidate is `f93443db131f55e6974ffaed3824f6addc36e84c`. The core controller is `f3548a7cff651ecfb9724a7367f20917cfe00799`; the final tools branch `646f236d37f704cc1f1b653b7336340112e9c8f6` adds receipts only. Preparation identity SHA256 is `75f7c296a2aa6f99d8030457a6b84f0d5c109fdc7002a723424f97849e49a09a`. The final source merge preserves all production and consumer bytes from the candidate.

All 22 original ZIL/Rust files come byte-for-byte from 104 commit `38ec6bc2d267b44bb7edab8a0d894af9cdb23954`. The mapped buffer sequence module is byte-identical to the actual public module, including its existing generic put/tail operations. Input/output, allocation growth, failure handling and cleanup contracts are unchanged. Stream and retained use identical sources in the corresponding factor arms. Fresh source, input and executable checks precede each registered arm execution. Startup means a new process, not a claim of OS cold-cache state.

The ordinary value function subtracts 32 only for u8 values 97 through 122. The mutable-window function explicitly assigns every visited mapped value, through existing source-level write permission and independent Verify. No backend speculates false-path writes; no new pure/readonly/noalias/nofree assertion is added. The general load-implies-write inference was rejected separately. See [SOURCE-CONTRACT.md](SOURCE-CONTRACT.md) and the archived independent boundary review.

N restores only the exact 108 foreign declaration file and its four tests. It is restricted to x86_64-unknown-linux-gnu. Eager symbol resolution can alter IFUNC/startup observations: the focused actual control produces 152 versus 52 at O0/O2. This is a documented binding-time difference under the current Native contract, with no universal loader-equivalence or zero-startup-cost claim.

## Complete fixed acquisition and numerical gates

Window `formal109-native-001` was granted and ended by root. Time collection ran 2026-10-25 13:28:33–13:29:02 UTC; the separate target-memory collection ran 13:30:09–13:30:16 UTC. There are exactly 1,920 time observations: 120 warmups and 1,800 retained, thirty retained five-arm rounds per cell. The ten Williams orders occur three times each; every arm occupies each position six times. The separate memory experiment has 300 observations, five per arm/cell. There is one campaign, with no dropped case, resampling, seed change, expanded sample or selected alternative admission arm.

| Group | Cells | Geometric mean T_Rust/T_NS | Failed per-case gates | Original rule |
| --- | --- | --- | --- | --- |
| runtime | 6 | 1.069597276948 | None | each >= 0.9; geometric mean >= 0.95 |
| startup | 6 | 1.446098850181 | None | each >= 0.9; geometric mean >= 0.95 |
| all | 12 | 1.243681386992 | None | each >= 0.9; geometric mean >= 0.95 |

These are equally weighted finite-cell geometric means, not a weighted application-throughput prediction. The six runtime cells pass independently of the startup cells. Memory observations are separate from timing, and Python-parent wait4 RSS is excluded from target-memory conclusions.

## All five-arm medians

Times are milliseconds; efficiency and NS/BASE below are ratios of arm medians. Full-precision observations and summaries remain unchanged in the archive.

| Cell / input bytes | BASE | N | S | NS | Rust | NS/BASE | Rust/NS |
| --- | --- | --- | --- | --- | --- | --- | --- |
| runtime/buffer/4194304 | 39.416159 | 38.337271 | 37.365750 | 36.296044 | 34.447258 | 0.920842 | 0.949064 |
| runtime/buffer/65536 | 1.237884 | 1.181614 | 1.166722 | 1.167723 | 1.353670 | 0.943322 | 1.159238 |
| runtime/retained/4194304 | 50.996017 | 39.772567 | 49.527865 | 46.885576 | 42.539288 | 0.919397 | 0.907300 |
| runtime/retained/65536 | 1.232792 | 1.203577 | 1.249656 | 1.238735 | 1.438662 | 1.004821 | 1.161397 |
| runtime/stream/4194304 | 33.517066 | 30.496634 | 33.604228 | 30.720214 | 30.375089 | 0.916554 | 0.988766 |
| runtime/stream/65536 | 1.213558 | 1.086130 | 1.149036 | 1.072580 | 1.401066 | 0.883831 | 1.306258 |
| startup/buffer/0 | 0.655110 | 0.638505 | 0.663697 | 0.640768 | 0.904456 | 0.978107 | 1.411521 |
| startup/buffer/1 | 0.648950 | 0.612536 | 0.617212 | 0.606215 | 0.931757 | 0.934147 | 1.537007 |
| startup/retained/0 | 0.591773 | 0.638564 | 0.651334 | 0.624899 | 0.852583 | 1.055977 | 1.364352 |
| startup/retained/1 | 0.608308 | 0.619390 | 0.629110 | 0.624904 | 0.874781 | 1.027282 | 1.399865 |
| startup/stream/0 | 0.736241 | 0.715025 | 0.717278 | 0.721635 | 1.073216 | 0.980161 | 1.487201 |
| startup/stream/1 | 0.653748 | 0.661584 | 0.642901 | 0.631193 | 0.936709 | 0.965500 | 1.484029 |

## All five-arm p95 values and adverse tails

| Cell / input bytes | BASE | N | S | NS | Rust | NS/BASE p95 |
| --- | --- | --- | --- | --- | --- | --- |
| runtime/buffer/4194304 | 46.620595 | 55.266268 | 47.816947 | 41.017274 | 46.466007 | 0.879810 |
| runtime/buffer/65536 | 1.739897 | 1.337299 | 1.519144 | 1.303038 | 1.731545 | 0.748917 |
| runtime/retained/4194304 | 64.036045 | 58.923626 | 65.117878 | 60.305433 | 55.135918 | 0.941742 |
| runtime/retained/65536 | 2.593016 | 1.973169 | 1.492563 | 1.545093 | 1.627337 | 0.595867 |
| runtime/stream/4194304 | 42.517263 | 37.417899 | 42.519207 | 38.429379 | 41.517509 | 0.903854 |
| runtime/stream/65536 | 1.362107 | 1.373273 | 1.324840 | 1.647518 | 1.727939 | 1.209536 |
| startup/buffer/0 | 0.901452 | 1.990015 | 0.788200 | 0.762502 | 1.203527 | 0.845860 |
| startup/buffer/1 | 1.367586 | 1.008683 | 0.812777 | 0.903584 | 1.347856 | 0.660715 |
| startup/retained/0 | 0.921882 | 0.907501 | 1.999920 | 0.966995 | 1.075905 | 1.048936 |
| startup/retained/1 | 0.831305 | 0.778806 | 0.773027 | 0.865557 | 1.041112 | 1.041203 |
| startup/stream/0 | 0.982153 | 0.965658 | 0.897155 | 1.252321 | 1.510961 | 1.275077 |
| startup/stream/1 | 1.622330 | 0.848841 | 0.951296 | 1.528879 | 1.824595 | 0.942397 |

NS has three adverse median points against BASE and four adverse p95 points. None is suppressed by the Rust numerical gate:

| Cell | NS/BASE median change | NS/BASE p95 change | Paired median-ratio interval |
| --- | --- | --- | --- |
| runtime/retained/65536 | +0.482117% | -40.413287% | [0.911944, 1.105878] |
| runtime/stream/65536 | -11.616920% | +20.953640% | [0.860948, 0.934547] |
| startup/retained/0 | +5.597665% | +4.893576% | [0.971015, 1.148027] |
| startup/retained/1 | +2.728223% | +4.120269% | [0.968695, 1.113473] |
| startup/stream/0 | -1.983928% | +27.507730% | [0.914165, 1.071966] |

The three adverse median-point intervals include 1. That is not proof of zero regression or a guarantee for another run. In particular, startup/stream/0 has a 27.507730% adverse p95 point and runtime/stream/65536 has 20.953640%. The complete arm distributions, including maxima and all individual values, remain in the raw summary.

## Factor relations and interaction

The following table retains all six paired relations in every cell. The point column is a ratio of arm medians; the paired median and its interval use within-round ratios. These estimators are distinct. Intervals are descriptive per-cell exact-binomial median intervals, not simultaneous-family coverage and not a replacement admission rule.

| Cell | Relation | Ratio of medians | Paired median | Paired median interval |
| --- | --- | --- | --- | --- |
| runtime/buffer/4194304 | N/BASE | 0.972628 | 0.957455 | [0.946617, 1.007031] |
| runtime/buffer/4194304 | S/BASE | 0.947981 | 0.949295 | [0.913813, 0.968798] |
| runtime/buffer/4194304 | NS/N | 0.946756 | 0.944427 | [0.920779, 0.966333] |
| runtime/buffer/4194304 | NS/S | 0.971372 | 0.976713 | [0.958476, 1.011493] |
| runtime/buffer/4194304 | NS/BASE | 0.920842 | 0.915702 | [0.900828, 0.941697] |
| runtime/buffer/4194304 | Rust/NS | 0.949064 | 0.943673 | [0.923251, 0.968650] |
| runtime/buffer/65536 | N/BASE | 0.954543 | 0.961308 | [0.919103, 0.998448] |
| runtime/buffer/65536 | S/BASE | 0.942513 | 0.945056 | [0.918449, 0.985080] |
| runtime/buffer/65536 | NS/N | 0.988244 | 0.978465 | [0.934267, 0.993006] |
| runtime/buffer/65536 | NS/S | 1.000858 | 0.981343 | [0.953862, 1.019871] |
| runtime/buffer/65536 | NS/BASE | 0.943322 | 0.932976 | [0.901487, 0.965544] |
| runtime/buffer/65536 | Rust/NS | 1.159238 | 1.179751 | [1.142906, 1.222567] |
| runtime/retained/4194304 | N/BASE | 0.779915 | 0.879937 | [0.856718, 0.929171] |
| runtime/retained/4194304 | S/BASE | 0.971210 | 0.971455 | [0.911511, 1.018099] |
| runtime/retained/4194304 | NS/N | 1.178842 | 1.049133 | [0.989751, 1.157340] |
| runtime/retained/4194304 | NS/S | 0.946650 | 0.907287 | [0.889142, 0.974743] |
| runtime/retained/4194304 | NS/BASE | 0.919397 | 0.941117 | [0.912935, 0.970855] |
| runtime/retained/4194304 | Rust/NS | 0.907300 | 0.944613 | [0.820969, 1.017327] |
| runtime/retained/65536 | N/BASE | 0.976302 | 0.967759 | [0.939398, 1.024770] |
| runtime/retained/65536 | S/BASE | 1.013680 | 0.991623 | [0.920177, 1.045706] |
| runtime/retained/65536 | NS/N | 1.029211 | 1.050154 | [0.938333, 1.100376] |
| runtime/retained/65536 | NS/S | 0.991260 | 0.971527 | [0.929501, 1.051659] |
| runtime/retained/65536 | NS/BASE | 1.004821 | 0.952501 | [0.911944, 1.105878] |
| runtime/retained/65536 | Rust/NS | 1.161397 | 1.171907 | [1.067773, 1.255788] |
| runtime/stream/4194304 | N/BASE | 0.909884 | 0.908300 | [0.885435, 0.928832] |
| runtime/stream/4194304 | S/BASE | 1.002601 | 1.008165 | [0.985509, 1.023961] |
| runtime/stream/4194304 | NS/N | 1.007331 | 1.003198 | [0.970567, 1.033026] |
| runtime/stream/4194304 | NS/S | 0.914177 | 0.921659 | [0.890603, 0.950922] |
| runtime/stream/4194304 | NS/BASE | 0.916554 | 0.911327 | [0.886562, 0.939564] |
| runtime/stream/4194304 | Rust/NS | 0.988766 | 1.005111 | [0.995325, 1.032910] |
| runtime/stream/65536 | N/BASE | 0.894997 | 0.912098 | [0.854069, 1.006463] |
| runtime/stream/65536 | S/BASE | 0.946832 | 0.936437 | [0.903666, 0.995230] |
| runtime/stream/65536 | NS/N | 0.987524 | 1.012350 | [0.931329, 1.081782] |
| runtime/stream/65536 | NS/S | 0.933461 | 0.952976 | [0.898564, 1.033276] |
| runtime/stream/65536 | NS/BASE | 0.883831 | 0.905953 | [0.860948, 0.934547] |
| runtime/stream/65536 | Rust/NS | 1.306258 | 1.229354 | [1.189489, 1.302309] |
| startup/buffer/0 | N/BASE | 0.974653 | 1.046237 | [0.911565, 1.116324] |
| startup/buffer/0 | S/BASE | 1.013108 | 0.997397 | [0.928102, 1.049088] |
| startup/buffer/0 | NS/N | 1.003544 | 0.942014 | [0.864389, 1.053553] |
| startup/buffer/0 | NS/S | 0.965452 | 0.979745 | [0.893878, 1.044960] |
| startup/buffer/0 | NS/BASE | 0.978107 | 0.956319 | [0.867411, 1.045388] |
| startup/buffer/0 | Rust/NS | 1.411521 | 1.405855 | [1.345594, 1.571611] |
| startup/buffer/1 | N/BASE | 0.943887 | 0.965941 | [0.904181, 1.046034] |
| startup/buffer/1 | S/BASE | 0.951093 | 0.959783 | [0.905826, 1.050612] |
| startup/buffer/1 | NS/N | 0.989681 | 0.976640 | [0.940165, 1.042530] |
| startup/buffer/1 | NS/S | 0.982183 | 0.988228 | [0.955769, 1.060046] |
| startup/buffer/1 | NS/BASE | 0.934147 | 0.913422 | [0.901585, 0.971393] |
| startup/buffer/1 | Rust/NS | 1.537007 | 1.464591 | [1.406960, 1.540364] |
| startup/retained/0 | N/BASE | 1.079068 | 1.037364 | [0.942910, 1.105723] |
| startup/retained/0 | S/BASE | 1.100647 | 1.054166 | [0.960902, 1.210064] |
| startup/retained/0 | NS/N | 0.978600 | 1.027116 | [0.910502, 1.084360] |
| startup/retained/0 | NS/S | 0.959415 | 0.994864 | [0.875340, 1.042408] |
| startup/retained/0 | NS/BASE | 1.055977 | 1.049316 | [0.971015, 1.148027] |
| startup/retained/0 | Rust/NS | 1.364352 | 1.422273 | [1.270634, 1.546278] |
| startup/retained/1 | N/BASE | 1.018218 | 1.029551 | [0.983625, 1.110784] |
| startup/retained/1 | S/BASE | 1.034196 | 0.993962 | [0.934814, 1.086702] |
| startup/retained/1 | NS/N | 1.008902 | 0.993972 | [0.935948, 1.035991] |
| startup/retained/1 | NS/S | 0.993314 | 1.022757 | [0.946911, 1.087830] |
| startup/retained/1 | NS/BASE | 1.027282 | 1.043972 | [0.968695, 1.113473] |
| startup/retained/1 | Rust/NS | 1.399865 | 1.431401 | [1.338616, 1.469984] |
| startup/stream/0 | N/BASE | 0.971183 | 0.981778 | [0.905166, 1.040374] |
| startup/stream/0 | S/BASE | 0.974243 | 0.996386 | [0.923462, 1.089261] |
| startup/stream/0 | NS/N | 1.009244 | 0.996093 | [0.939819, 1.083810] |
| startup/stream/0 | NS/S | 1.006074 | 1.025948 | [0.921975, 1.101624] |
| startup/stream/0 | NS/BASE | 0.980161 | 0.966827 | [0.914165, 1.071966] |
| startup/stream/0 | Rust/NS | 1.487201 | 1.470467 | [1.379962, 1.536488] |
| startup/stream/1 | N/BASE | 1.011987 | 0.966395 | [0.884302, 1.063683] |
| startup/stream/1 | S/BASE | 0.983409 | 1.005086 | [0.923922, 1.076469] |
| startup/stream/1 | NS/N | 0.954063 | 1.044621 | [0.962509, 1.079620] |
| startup/stream/1 | NS/S | 0.981789 | 0.989400 | [0.931774, 1.065850] |
| startup/stream/1 | NS/BASE | 0.965500 | 0.982845 | [0.926198, 1.059996] |
| startup/stream/1 | Rust/NS | 1.484029 | 1.444710 | [1.330783, 1.480545] |

The registered interaction ratio is `(NS * BASE) / (N * S)` within each round. It describes this measured combination; it is not an independent proof of multiplicative algorithmic effects:

| Cell | Paired interaction median | Interval |
| --- | --- | --- |
| runtime/buffer/4194304 | 1.003242 | [0.965983, 1.018880] |
| runtime/buffer/65536 | 1.030249 | [0.988164, 1.073335] |
| runtime/retained/4194304 | 1.087358 | [0.987414, 1.144150] |
| runtime/retained/65536 | 1.079193 | [0.929537, 1.136296] |
| runtime/stream/4194304 | 1.002265 | [0.949968, 1.051604] |
| runtime/stream/65536 | 1.009100 | [0.943720, 1.167330] |
| startup/buffer/0 | 0.928857 | [0.836825, 1.167212] |
| startup/buffer/1 | 1.001820 | [0.908598, 1.144135] |
| startup/retained/0 | 0.908677 | [0.802110, 1.111473] |
| startup/retained/1 | 0.975624 | [0.933532, 1.120714] |
| startup/stream/0 | 1.073463 | [0.884244, 1.133280] |
| startup/stream/1 | 1.003289 | [0.965741, 1.062778] |

For 4 MiB buffer, S/BASE is 0.947981 and NS/N is 0.946756 by ratios of medians; their paired intervals are [0.913813, 0.968798] and [0.920779, 0.966333]. The actual S/NS loop has two 16-byte loads, a byte mask/map and two vector stores, whereas the original conditional-write loop expands to per-lane branches/stores. This supports the bounded source-shape finding. It does not assign a universal number of milliseconds to individual instructions. In retained, S changes no executable byte; the large N/NS discrepancy must not be described as a source optimization benefit or regression.

## Complete separate target-memory results

Each cell below is median / p95 KiB from five individual target processes, measured by the independently controlled native exec/fork launcher. This is neither the Python parent high-water mark nor aggregate process-tree RSS. At n=5, the reported p95 order statistic is the sample maximum.

| Cell | BASE | N | S | NS | Rust |
| --- | --- | --- | --- | --- | --- |
| runtime/buffer/4194304 | 4608 / 4608 | 4608 / 4608 | 4608 / 4608 | 4608 / 4608 | 5088 / 5180 |
| runtime/buffer/65536 | 640 / 640 | 640 / 640 | 640 / 640 | 640 / 640 | 1076 / 1084 |
| runtime/retained/4194304 | 512 / 512 | 512 / 512 | 512 / 512 | 512 / 512 | 1080 / 1080 |
| runtime/retained/65536 | 512 / 512 | 512 / 512 | 512 / 512 | 512 / 512 | 1080 / 1092 |
| runtime/stream/4194304 | 384 / 384 | 384 / 384 | 384 / 384 | 384 / 384 | 916 / 1084 |
| runtime/stream/65536 | 384 / 384 | 384 / 384 | 384 / 384 | 384 / 384 | 952 / 1056 |
| startup/buffer/0 | 384 / 384 | 384 / 384 | 384 / 384 | 384 / 384 | 948 / 956 |
| startup/buffer/1 | 512 / 512 | 512 / 512 | 512 / 512 | 512 / 512 | 948 / 1084 |
| startup/retained/0 | 384 / 384 | 384 / 384 | 384 / 384 | 384 / 384 | 952 / 960 |
| startup/retained/1 | 512 / 512 | 512 / 512 | 512 / 512 | 512 / 512 | 964 / 1092 |
| startup/stream/0 | 384 / 384 | 384 / 384 | 384 / 384 | 384 / 384 | 928 / 952 |
| startup/stream/1 | 384 / 384 | 384 / 384 | 384 / 384 | 384 / 384 | 952 / 1032 |

NS and BASE target-RSS medians coincide in all twelve cells. NS medians are below the original Rust medians in these observations. These finite measurements do not prove universal memory equivalence or an all-application memory advantage; peak RSS is also distinct from the separately validated exact allocation/live-owner trace.

## Functional, protocol and independent evidence

The five new source groups pass eight O0/O2 native builds and 36 executions, including two expected initialized-window traps; two nearest source permission controls reject. The full u8 domain, empty/interior windows, outside sentinels and vector tails are checked. Each accepted build has one independent Verify run. Root additionally rebuilt and ran the exact five source groups and four restored binding groups in a new target. The first author attempt had a wrong diagnostic expectation: shared access was rejected earlier in Expand. That failed fixture, its source and test binary are retained.

The fresh five-arm preparation validates 300 ordinary and 150 observer executions, including 80 actually triggered allocator failures at registered indices. These are controlled failures, not naturally occurring compiler defects. All source/error/output/allocation/release observations pass their original independent model. Twelve native memory controls distinguish target RSS from padded-parent high-water marks. The six original Rust plain executable identities are unchanged.

Actual machine isolation covers fifteen O2 programs. Twelve N-factor comparisons have identical syntax/Core/CT-Core/candidate/typed/verified/LIR artifacts, and their LLVM differs only by nonlazybind. Eight corresponding stream/retained executable pairs are byte-identical. Controller integrity includes 39 author synthetic controls and independent protocol/refusal checks. Graph independently verified all 2,220 formal slots, input/environment/cwd bindings, captures, time journal and inner RSS receipts. Root independently recomputed all twelve cells, six paired factors, interactions, original gates and target-RSS results. Both checks pass; neither review erases the same-binary noise or adverse tails.

## Preservation and final integration boundary

All original data and failed fixtures are retained through the standard tar hardlink archive described in [ARCHIVE.md](ARCHIVE.md). Every logical path retains its original bytes, mode, length and SHA256. The archive stores repeated payloads once and checks complete restoration, including every hardlink referent. `evidence/impl/109/raw001/raw-archive.json` provides the archive and manifest identities; the archive itself is not used as a substitute for semantic or numerical review.

The source candidate is retained as the exact NS implementation; no production rollback is requested for 109 under root's bounded selection. Final admission still requires the independent archive check, main integration and the original 35 necessary gates. The measurement summary's `product: NotRun` remains immutable, and the broader four-product-objective result is determined separately. Task 108 remains NoGo. This is an ordinary-library/source improvement combined with a bounded foreign-binding policy, without a claim of a new general graph theorem or automatic conditional-store compiler pass.

