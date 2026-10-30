# Independent O2 runtime quality review

Reviewer: `/root/llvm_strategy`. This reviews the frozen runtime data and its
aggregation independently of the runtime runner's author. It is not an
independent review of this agent's CP13 backend implementation; root supplied
that separate review in `ac10f2c54`.

## Decision

**Data integrity and independent recomputation: Pass. Primary closed8 O2
configuration adoption: NoGo.** The small `tree4x2` workload regresses in every
paired block on both wall and CPU time. Its interval excludes 1 above it. The
near-one weighted result cannot cancel this per-workload regression under the
predeclared round-two plan. Keep the explicit option default off; this is not a
recommendation to enable it by default or merge the research result to main.

| Metric | Primary closed8 B/A | Paired block bootstrap 95% | Secondary closed1 B/A | Paired block bootstrap 95% |
| --- | ---: | --- | ---: | --- |
| Weighted wall | 0.987449 | [0.916458, 1.036777] | 0.992552 | [0.992552, 1.061060] |
| Weighted CPU | 0.985998 | [0.915822, 1.037712] | 0.994047 | [0.994047, 1.061510] |
| tree4x2 wall | 1.272744 | [1.205266, 1.376242] | 1.040457 | [0.914403, 1.267734] |
| tree4x2 CPU | 1.282923 | [1.208389, 1.388826] | 1.042404 | [0.913361, 1.273844] |

All other primary per-case intervals cross 1. All secondary per-case intervals
also cross 1. These are finite, exploratory intervals on five blocks, not proof
that the language or every program avoids runtime regression. Secondary closed1
was fixed as an exploratory configuration before measurement and is not
promoted by this review.

## Preserve the paired-block observations

The planned estimator averages the two same-arm observations inside each
ABBA/BAAB block, takes each case/arm's median across five blocks, and then forms
the weighted ratio. The script independently reconstructs this estimator and
reproduces every original per-case and weighted wall/CPU point estimate and the
original weighted wall interval. It does not import or call author aggregation
code. It adds CPU and per-case intervals with the same 5,000 paired block
resamples and seed, and gives paired geometric-ratio sensitivities separately.

The secondary's weighted point estimate below 1 must not be described as
consistent speedup: **all five paired-block weighted ratios are above 1**.
Case-wise medians followed by weighting can give a different direction from
the within-block weighted comparisons. Neither the original summary nor any
sample is changed to resolve this difference.

| Block | closed8 weighted wall | closed8 weighted CPU | closed1 weighted wall | closed1 weighted CPU |
| --- | ---: | ---: | ---: | ---: |
| 0 | 0.916458 | 0.915822 | 1.061060 | 1.061510 |
| 1 | 0.980726 | 0.980227 | 1.016732 | 1.018963 |
| 2 | 1.015624 | 1.017824 | 1.002173 | 1.003366 |
| 3 | 1.036777 | 1.037712 | 1.010627 | 1.017603 |
| 4 | 0.972763 | 0.965902 | 1.014233 | 1.015139 |

The JSON retains every case's wall/CPU means and ratio in every block, every
planned point estimate and interval, and the separate paired sensitivities.
No startup subtraction, slow-sample removal, weight change, compiler rebuild,
or native executable rerun was performed.

## Binding checks

The review checks both quality windows' complete ordered 100-row sets, frozen
source/input/oracle bytes, runner/helper Git identities, compiler/LLVM/linker
files, actual native ELF identities, raw build and runtime logs, actual command
configuration, full stdout equality to the oracle, empty stderr, timeout/exit
status, before/after input and executable identities, and completion receipts.
Each real build reports one independent Verify, LLVM 23.1.4 `default<O2>`, no
fallback, disabled object cache, and the intended closed/CGU settings. The
build/startup/warmup samples remain unscored. There are 3,521 distinct checked
file paths across the two quality windows and the attached prior preflights.

Both earlier Rust/ZIL closed1/closed8 preflights are rechecked and attached:
each has 32 short oracle checks, 10 startup checks, 10 full warmup checks, and 10
preserved executables. Their ZIL linkage-change counts are:

| Case | closed1 | closed8 | closed8 actual units / jobs |
| --- | ---: | ---: | --- |
| tree4x2 | 16 | 9 | 7 / 4 |
| tree32x8 | 292 | 261 | 8 / 4 |
| tree256x8 | 2308 | 2081 | 8 / 4 |
| tree1024x8 | 9220 | 8321 | 8 / 4 |
| retained | 11 | 9 | 3 / 3 |

These are functions changed from External to Internal before LLVM O2, not DCE
counts. Compiler source commits remain the caller-declared frozen provenance;
actual compiler hashes and real build receipts are checked here. LLVM text
sidecars were removed after hashing by the original protocol and are not
recreated or rehashed. Product gates and language-wide equivalence are not
claimed.

## Reproduce

Run the committed `review.py` with the preserved `/tmp/zil-llvm23-round2` input
tree, a repository containing the frozen Git objects, and a **new** output path:

```sh
python3 review.py --root /tmp/zil-llvm23-round2 --repo /workspace/scratch/ac5966c4c370/zil-research --out /tmp/independent-quality-recheck.json
```

The script only writes its own requested JSON receipt. Any integrity failure
suppresses scores. The committed `review-by-llvm-strategy.json` binds the exact
script SHA and raw artifact hashes. Original summaries and runner files are
untouched.
