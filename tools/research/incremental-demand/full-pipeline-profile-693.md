# Frozen 693 single-block scalar diagnostic profile

Single-block scalar diagnosis against same-run main only, not a formal Rust comparison, statistical result, acceptance, or improvement versus any prior candidate.

Source `693f55cd0394a1a72a0bfe9d680740d9509885d1`, tree `88cec28d91f274e2d6d156bd93e44e11afcf7a86`. Frozen693 contains plan fragments and input revision; current a6 array work is absent.

Validated 34 rows, 17 pairs (15 successful, 2 paired SafeReject), 180 exact native outcomes independently recomputed by the archived generator after its bytes are matched to the supplied source Git blob; every successful pair has equal recorded ELF and LLVM SHA-256. The cold pipeline processes all 425 functions with asserted CGU=16/jobs=1. No sample or fixture was reduced.

| Case | Main ms | Candidate ms | Candidate − main ms | Ratio |
|---|---:|---:|---:|---:|
| cold | 6191.890 | 6520.835 | +328.945 | 1.053 |
| unchanged | 488.151 | 88.411 | -399.740 | 0.181 |
| private | 1940.965 | 790.575 | -1150.390 | 0.407 |
| multi | 1668.753 | 1096.400 | -572.353 | 0.657 |
| broad | 5332.494 | 6008.830 | +676.336 | 1.127 |
| positions | 1474.442 | 492.541 | -981.901 | 0.334 |
| error | 360.450 | 71.967 | -288.483 | 0.200 |
| recovery | 1578.588 | 641.541 | -937.047 | 0.406 |
| restore | 472.567 | 181.640 | -290.927 | 0.384 |

Actual publication: private serializes 1 function/reuses 424 and serializes 0 FunctionPlans/reuses all 425 (1,447,096 cached row bytes). It writes 4 new logical segments / 15,214 payload bytes in 1 physical pack of 15,314 bytes; the manifest is 113,401 bytes. Cold has 856 ordered logical entries/855 unique items: identical 12-byte module/plan suffixes are deduplicated within that pack (`existing_segments=1` is not prior-epoch reuse). It serializes all 425 plans and writes 6,738,230 unique payload bytes. Segment counts describe logical items; physical pack writes remain separately reported. Multi/broad/positions/recovery also reuse all 425 raw plan rows.

Input revision: private, error and recovery each execute 1 BodyKey/reuse 420 and visit 389 body nodes; producer input hashes/reuses are 1/424. Positions is a remaining whole scan: BodyKey 421/0 and 159,558 visited nodes, producer input hashes/reuses 425/0. Positions 0/421 refers to FunctionStore serialization reuse, while Types CFG execution/reuse is 4/421 with 421 relocated CFGs. These counters must not be conflated.

Remaining costs are retained in JSON: private still clones 21 normalized items/1 restored module, multi 63/3, broad 420/20, positions 425/22. Private Analyze/Verify take 36.922/56.583 ms; its Analyze candidate counts are 5 executed/420 reused with all 425 equal outputs; Verify function proofs 1 checked/422 reused, empty-borrow proofs 1/424, 112197 values avoided. Current module/interface checks remain.

Cold and broad regressions versus this run’s main are explicit in the table. Multi remains over one second despite three changed final functions; broad executes 404 Types functions and 400 function serializations. Position edits still hash/normalize every function. No improvement versus a prior candidate is inferred from this one block.

Memory caveat: `tree_peak_sampled_rss_sum_kib` is a legacy field. All 4038 samples in 34 files report exactly one process, with per-row sample lengths/maxima verified. Only root RSS was observed; do not call it process-tree RSS, total build memory, or a memory advantage.

History: unchanged reuses one full Verify receipt. Restore is a historical raw frontend hit (`resident_frontend_hits=1`, no producer/Types/Analyze counters), but still checks 1 Verify function and reuses 422 (empty-borrow 1/424); it is not a full Verify hit or disk semantic restore. Backend source-projection counters remain zero where reported. Error and recovery results remain part of the 34-row validation.

Controller: persisted helper exit 0, root-reported tool exit 0, complete 17-pair valid log. These observations are recorded separately.

Compiler SHA-256 candidate `612b83c0fe9d2a552573e5fbb53a11ae3b46fc70ad8feaa132eac18168104f37`, main `e5fb6fdfdd9bd1a2a7d064519defaf860e0207d939924ab0fab6fe59044454c7`. Build receipt source/tree and candidate binary bytes were verified. Script SHA-256 `9d28ac151eb1c3b1ab7b82711ba02c802c8fd7c9e9883b9fb1c7460b7ea40b84`.

Core raw receipts:

- `out/runs/full-pipeline-profile-o2-r6/rows.jsonl`: `5ec3699077889c50eba75556b483bcfae65b13e152c72fa1fedd93aa72f50dc2`.
- `out/runs/full-pipeline-profile-o2-r6/manifest.json`: `c9e8534219decfbe14f831c9ba6be0dca61823b048446d17f9a72918e4d17260`.
- `out/runs/full-pipeline-profile-o2-r6/summary.json`: `cf2b452468698629a444ea3d1f61ea0e3be6f197af64b523ad964161b75c186f`.
- `out/runs/full-pipeline-profile-o2-r6.log`: `b971a552522a689aed7085315353cd3246c8397e468511fbab92e6ffc72d9efa`.
- `out/runs/full-pipeline-profile-o2-r6.exit`: `9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa`.
- `out/runs/full-pipeline-input-plan-build/candidate-build.json`: `ae3b55e6d2f365a8ba894659d60476d2b58d65a3f26b68edbb89aa1f77cc907a`.
- `out/runs/full-pipeline-input-plan-release-01.log`: `a3ebcc9139047de7a522c4ff5919982077835e684ff7a4b89620b259701f183f`.
- `out/runs/full-pipeline-profile-o2-r6/native_measure.py`: `7294a9ade9c8f4a198bbeab9d4dd7c4791684e3b2fb45c21b995abb0ea54ba5d`.
- `out/runs/full-pipeline-profile-o2-r6/native_workloads.py`: `df6e95b48825063b718fd65a0c4ab8afaa940f6880a40215d60ac9105ced6964`.

JSON includes all 34 reduced rows, 17 pair identity receipts, all nine measured candidate counter sets, and individual SHA-256 receipts for all 34 sample files. Fixture-reference comparison checks source identities only, never its timing values.

Reproduce from the repository root:

```sh
python tools/research/incremental-demand/profile_validate.py --repo . --run-dir out/runs/full-pipeline-profile-o2-r6 --source 693f55cd0394a1a72a0bfe9d680740d9509885d1 --build-receipt out/runs/full-pipeline-input-plan-build/candidate-build.json --controller-exit 0 --output-stem /tmp/full-pipeline-profile-693 --reference-fixtures out/runs/full-pipeline-profile-o2-r5/rows.jsonl --expect-plan-fragments --expect-body-revision --scope-note 'Frozen693 contains plan fragments and input revision; current a6 array work is absent.'
```

Add `--main-binary MAIN_SOURCE_CHECKOUT/target/release/zil` to physically recheck main as well as the saved candidate binary; recorded compiler identities and receipt consistency are always checked. No build or measurement is run by this validator. No root source was edited to produce this report.
