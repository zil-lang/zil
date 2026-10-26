# 108 result: NoGo under the frozen runtime rule

The fixed campaign completed, and independent acquisition/integrity checks passed. The production candidate is **NoGo**: `runtime/buffer/4194304` has `T_Rust/T_candidate = 0.8966415293706075`, below the unchanged 0.9 per-case requirement. Its interval crossing 0.9 does not alter the predeclared point-estimate rule. There is no resampling, dropped case, new threshold or pooled replacement for this result.

The six original runtime cases have geometric mean efficiency 1.04314827139063; the six startup cases have 1.243001844749291; all twelve have 1.1386989179258875. The failing individual case prevents runtime and combined admission. These are equally weighted finite-cell geometric means, not a general application-speed claim.

## Fixed identities and complete acquisition

- Baseline: `fe2e650038062641dc2b467018243f1566fecff4`.
- Candidate retained in history: `7a219f2d782532324e1ea7401cab68c9eada7c82`.
- Reviewed controller: `bc6271ab1ce885806658d801f7f4d2c070bbb13c`.
- Preparation identity SHA256: `b32f3652ec6c7df96dfdf2c7084fa7486332a05007c0aa5a824070f98c4ff01d`.
- Window: `formal108-native-001`, CPU 2. Time collection: 2026-10-25 12:44:54–12:45:07 UTC. Memory collection: 12:45:18–12:45:20 UTC.
- Time: 1,152 observations, 72 warmups and 1,080 retained; thirty retained triplets per cell. Memory: 180 separate target peaks, five per arm/cell.
- Fresh validation: 180 plain executions, 90 allocation-observer executions, including 48 actually triggered allocation failures; twelve memory-launcher parent-padding controls.
- Author and nonauthor integrity controls: 30 and 18 respectively. The initial NeedsFix and all failed author fixture attempts remain retained.

## Every runtime and startup cell

Times below are milliseconds. Ratios in this table are ratios of arm medians. Values are rounded for display; the JSON summaries preserve full precision.

| Cell / input bytes | BASE median | Candidate median | Rust median | Candidate/BASE | Rust/candidate | p95 BASE / candidate / Rust |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| runtime/buffer/4194304 | 39.912135 | 38.412246 | 34.442015 | 0.962420 | 0.896642 | 42.492204 / 47.065810 / 41.694650 |
| runtime/buffer/65536 | 1.296582 | 1.280382 | 1.411515 | 0.987506 | 1.102417 | 1.551020 / 1.530300 / 1.669299 |
| runtime/retained/4194304 | 39.430346 | 36.853682 | 35.130035 | 0.934653 | 0.953230 | 63.873623 / 48.779875 / 43.169628 |
| runtime/retained/65536 | 1.304904 | 1.233621 | 1.400734 | 0.945373 | 1.135465 | 1.529718 / 1.504221 / 1.608989 |
| runtime/stream/4194304 | 33.684159 | 30.342023 | 30.804701 | 0.900780 | 1.015249 | 44.412882 / 33.907132 / 36.863459 |
| runtime/stream/65536 | 1.268770 | 1.263302 | 1.498562 | 0.995690 | 1.186226 | 1.655328 / 1.584581 / 2.026868 |
| startup/buffer/0 | 0.702144 | 0.705024 | 0.823658 | 1.004101 | 1.168269 | 0.875601 / 1.049564 / 1.098017 |
| startup/buffer/1 | 0.682666 | 0.707838 | 0.844294 | 1.036874 | 1.192779 | 0.897194 / 1.060780 / 1.242886 |
| startup/retained/0 | 0.637502 | 0.640552 | 0.790238 | 1.004784 | 1.233683 | 0.806577 / 0.907580 / 0.912547 |
| startup/retained/1 | 0.825350 | 0.815685 | 1.052859 | 0.988290 | 1.290766 | 0.937745 / 0.901781 / 1.297197 |
| startup/stream/0 | 0.846943 | 0.841615 | 1.104852 | 0.993709 | 1.312776 | 1.125960 / 0.908811 / 1.391080 |
| startup/stream/1 | 0.868511 | 0.862947 | 1.092624 | 0.993594 | 1.266154 | 1.087201 / 1.071397 / 1.499373 |

The paired intervals below estimate the median of within-triplet ratios. That is a different estimator from the ratio of medians above; an interval is not asserted to contain that different point estimate. These are descriptive exact-binomial per-cell 95% median intervals, with no simultaneous-family coverage claim.

| Cell | Paired candidate/BASE median interval | Paired Rust/candidate median interval |
| --- | --- | --- |
| runtime/buffer/4194304 | [0.945381, 0.989244] | [0.885112, 0.916839] |
| runtime/buffer/65536 | [0.943983, 1.037554] | [1.045704, 1.147248] |
| runtime/retained/4194304 | [0.898000, 0.950116] | [0.941925, 0.971482] |
| runtime/retained/65536 | [0.919674, 0.967611] | [1.115828, 1.179115] |
| runtime/stream/4194304 | [0.893754, 0.918503] | [0.987439, 1.019390] |
| runtime/stream/65536 | [0.904705, 1.040798] | [1.095664, 1.279119] |
| startup/buffer/0 | [0.957911, 1.105761] | [1.202659, 1.283462] |
| startup/buffer/1 | [0.993059, 1.126480] | [1.117663, 1.309596] |
| startup/retained/0 | [0.992950, 1.061209] | [1.166959, 1.308484] |
| startup/retained/1 | [0.957233, 1.003713] | [1.251531, 1.340773] |
| startup/stream/0 | [0.965243, 1.028759] | [1.272273, 1.355203] |
| startup/stream/1 | [0.972454, 1.021683] | [1.252555, 1.320660] |

## Baseline costs and binding-time difference

All startup median regressions remain visible: buffer/0 is 1.0041010077 times BASE, buffer/1 is 1.0368738423, and retained/0 is 1.0047835144. Their paired intervals cross 1, so the campaign does not establish a positive median regression with those per-cell intervals; it also does not justify claiming zero startup cost. Candidate p95 exceeds BASE in buffer/4 MiB and those same three startup cells. The larger 4 MiB buffer p95 is 47.065810 ms versus BASE 42.492204 ms.

In the separate exact IFUNC fixture the eager candidate prints 152 and the deferred attribute-ablation prints 52, at O0 and O2. This is an actual resolver-timing difference. It is permitted by the audited current Linux Native contract but is never described as universal loader equivalence. No new function-effect attribute was assumed.

## Every target-memory cell

Values are KiB, reported as median / p95. All five original observations per cell and arm are in `memory-summary.json`. These are individual target peaks from the native exec/fork launcher, not inherited Python-parent high-water marks and not a sum of a process tree. BASE and candidate medians coincide in all twelve cells; this finite result does not prove identical memory under all loaders or applications.

| Cell | BASE median / p95 | Candidate median / p95 | Rust median / p95 |
| --- | ---: | ---: | ---: |
| runtime/buffer/4194304 | 4608 / 4608 | 4608 / 4608 | 5052 / 5080 |
| runtime/buffer/65536 | 640 / 640 | 640 / 640 | 1092 / 1092 |
| runtime/retained/4194304 | 512 / 512 | 512 / 512 | 1080 / 1080 |
| runtime/retained/65536 | 512 / 512 | 512 / 512 | 1080 / 1092 |
| runtime/stream/4194304 | 384 / 384 | 384 / 384 | 1044 / 1084 |
| runtime/stream/65536 | 384 / 384 | 384 / 384 | 1032 / 1092 |
| startup/buffer/0 | 384 / 384 | 384 / 384 | 872 / 984 |
| startup/buffer/1 | 512 / 512 | 512 / 512 | 1084 / 1092 |
| startup/retained/0 | 384 / 384 | 384 / 384 | 952 / 1080 |
| startup/retained/1 | 512 / 512 | 512 / 512 | 1080 / 1080 |
| startup/stream/0 | 384 / 384 | 384 / 384 | 952 / 1056 |
| startup/stream/1 | 384 / 384 | 384 / 384 | 1060 / 1092 |

## Machine isolation and the bounded next hypothesis

Across all three consumers at O0/O2, the six BASE/candidate pairs have byte-identical syntax, Core, typed, verified and LIR artifacts. Their emitted LLVM differs only by `nonlazybind`. All nine O2 programs show the intended relocation distinction: BASE getchar/putchar use JUMP_SLOT; candidate and the sealed original Rust binaries use GLOB_DAT. The candidate retains checked u8 conversion and its trap path.

The buffer residual has a specific structural difference. Its candidate output loop is already the same five-operation shape as Rust: load byte, indirect register call to putchar, increment, compare and branch. In its uppercase phase, however, the original ZIL conditional assignment becomes a 16-byte vector load followed by per-lane branches and scalar conditional stores. The original Rust make_ascii_uppercase uses a 32-byte vector map and two vector stores. The input loop also retains the mandatory u8 check, a capacity check and a GOT-memory indirect getchar call. These observations identify hypotheses; they do not measure which instruction accounts for any particular number of milliseconds.

An independently recorded, untimed ordinary-source probe makes the writing semantics explicit: a pure u8-returning ascii_upper helper and an unconditional per-element assignment. Only the copied sequence module changes; the original 104/108 programs and trials remain untouched. Exact BASE and candidate compilers both produce the 32-byte vector mask/map/store shape. Four O0/O2 programs pass sixty boundary/all-256-byte executions. The source—not a backend assumption—grants the writes and keeps checked arithmetic on its original true path. This is the basis of separate task 109, whose library and external-binding factors require their own frozen factorial experiment. It is not a reinterpretation of 108 as Pass.

## Delivery boundary

The final 108 research delivery restores all production and crate test files to the exact baseline. Candidate code and its four tests remain in Git history, alongside complete actual binaries and raw evidence. Accepted research delivery and acquisition integrity do not grant production admission. The original 104 deficiency, the 108 NoGo and all startup/p95 costs remain unchanged.
