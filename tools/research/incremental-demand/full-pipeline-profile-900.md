# Frozen 900 single-block scalar diagnostic profile

Single-block scalar diagnosis against same-run main only, not a formal Rust comparison, statistical result, acceptance, or improvement versus any prior candidate.

Source `900293cfd8afcd7118775e8165e1ec4f5266e3d7`, tree `e3f16832a9751f25399d4dd8e58488ed03513f54`. Frozen 900 includes plan fragments, relative inputs, mixed function lowering, shared candidate arrays, current-input whole machine reuse, and import-position input bindings. External controller response unavailable; persisted helper exit is checked independently.

Validated 34 rows, 17 pairs (15 successful, 2 paired SafeReject), 180 exact native outcomes independently recomputed by the archived generator after its bytes are matched to the supplied source Git blob; every successful pair has equal recorded ELF and LLVM SHA-256. The cold pipeline processes all 425 functions with asserted CGU=16/jobs=1. No sample or fixture was reduced.

| Case | Main ms | Candidate ms | Candidate − main ms | Ratio |
|---|---:|---:|---:|---:|
| cold | 6907.034 | 7866.906 | +959.872 | 1.139 |
| unchanged | 526.711 | 94.880 | -431.831 | 0.180 |
| private | 1562.823 | 784.021 | -778.802 | 0.502 |
| multi | 1778.213 | 1626.467 | -151.746 | 0.915 |
| broad | 6163.278 | 8248.034 | +2084.756 | 1.338 |
| positions | 1878.668 | 538.796 | -1339.871 | 0.287 |
| error | 372.539 | 93.908 | -278.631 | 0.252 |
| recovery | 2397.771 | 1098.359 | -1299.412 | 0.458 |
| restore | 499.362 | 200.542 | -298.820 | 0.402 |

Actual publication: private serializes 1 function/reuses 424 and serializes 0 FunctionPlans/reuses all 425 (1,447,096 cached row bytes). It writes 4 new logical segments / 15,214 payload bytes in 1 physical pack of 15,314 bytes; the manifest is 113,401 bytes. Cold has 856 ordered logical entries/855 unique items: identical 12-byte module/plan suffixes are deduplicated within that pack (`existing_segments=1` is not prior-epoch reuse). It serializes all 425 plans and writes 6,738,230 unique payload bytes. Segment counts describe logical items; physical pack writes remain separately reported. Multi/broad/positions/recovery also reuse all 425 raw plan rows.

Input revision counters: private: BodyKey 1 executed/420 reused, 389 visited nodes; producer text hashes/reuses 1/424, current binding hashes 21; error: BodyKey 1 executed/420 reused, 389 visited nodes; producer text hashes/reuses 1/424, current binding hashes 21; recovery: BodyKey 1 executed/420 reused, 389 visited nodes; producer text hashes/reuses 1/424, current binding hashes 21; positions: BodyKey 0 executed/421 reused, 0 visited nodes; producer text hashes/reuses 0/425, current binding hashes 425. FunctionStore positions serialization/reuse is 0/421; these are separate counters.

Relocation preserves raw text revisions while renewing current bindings: positions Lower 22 executed/0 reused and materialization 22 executed/0 reused/22 cloned. The revision shortcut does not reuse old lowering or materialization outputs.

Unchanged whole-machine reuse: reused_machine_module=True, per-function lowered/reused=0/0, whole input functions compared=425. The 425 compatibility comparisons remain real work. This does not assert zero full equality or zero input checking for cold or edited requests; their reported counters remain in JSON.

Reported remaining cloning: private 21 normalized-item/1 restored-module clones; multi 63 normalized-item/3 restored-module clones; broad 420 normalized-item/20 restored-module clones; positions 425 normalized-item/22 restored-module clones.

Private Analyze/Verify take 25.707/45.568 ms. Analyze candidate 5 executed/420 reused, 425 equal outputs; Verify function proofs 1 checked/422 reused, empty-borrow 1/424, 112197 values avoided. Current module/interface checks remain.

Wall regressions versus this run’s main: cold +959.872 ms, broad +2084.756 ms. Multi remains 1626.467 ms; broad executes 404 Types functions. Residual position work is represented by its actual counters above and in JSON. No improvement versus a prior candidate is inferred from this one block.

Memory caveat: `tree_peak_sampled_rss_sum_kib` is a legacy field. All 4840 samples in 34 files report exactly one process, with per-row sample lengths/maxima verified. Only root RSS was observed; do not call it process-tree RSS, total build memory, or a memory advantage.

History: unchanged reuses one full Verify receipt. Restore is a historical raw frontend hit (`resident_frontend_hits=1`, no producer/Types/Analyze counters), but still checks 1 Verify function and reuses 422 (empty-borrow 1/424); it is not a full Verify hit or disk semantic restore. Backend source-projection counters remain zero where reported. Error and recovery results remain part of the 34-row validation.

Controller: persisted helper exit 0, root-reported tool exit None, complete 17-pair valid log. These observations are recorded separately.

Compiler SHA-256 candidate `936d7c107713c11dca86bbf196fc9a0c6ceee336ef4f57e60751543ba1817d82`, main `e5fb6fdfdd9bd1a2a7d064519defaf860e0207d939924ab0fab6fe59044454c7`. Build receipt source/tree and candidate binary bytes were verified. Script SHA-256 `194a7d25394cca29b0e498356ac1d9e64c884610290cbd4e10e09e122232929a`.

Core raw receipts:

- `out/runs/full-pipeline-profile-o2-r7/rows.jsonl`: `e05eeefb5643f9a03145617b1b024ecc598277b5f96eab5e8a9e12a9cde3f266`.
- `out/runs/full-pipeline-profile-o2-r7/manifest.json`: `0ecb8a3b0628ca91084f7a7c8416a499d50b6b13fb1886d0656d79fc38774b5c`.
- `out/runs/full-pipeline-profile-o2-r7/summary.json`: `2a6a7886e0bca25821dcab85d748a24ee4ece3d58481e0c334ed5cc5499b7668`.
- `out/runs/full-pipeline-profile-o2-r7.log`: `e5448e1d52b99051e7f19a128f24032d5f84eeadba5ea26306ab98acde5d2d43`.
- `out/runs/full-pipeline-profile-o2-r7.exit`: `9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa`.
- `out/runs/full-pipeline-final-build/candidate-build.json`: `e6d7bb6cb3d9d18d27baaa450ccd120b16f6109d02a3a656598dd766747e053d`.
- `out/runs/full-pipeline-final-release-01.log`: `e7c181ada3aa8dcf7c4d1fddf53a83162dd9ff568988d024ea1452add6699157`.
- `out/runs/full-pipeline-profile-o2-r7/native_measure.py`: `7294a9ade9c8f4a198bbeab9d4dd7c4791684e3b2fb45c21b995abb0ea54ba5d`.
- `out/runs/full-pipeline-profile-o2-r7/native_workloads.py`: `df6e95b48825063b718fd65a0c4ab8afaa940f6880a40215d60ac9105ced6964`.

JSON includes all 34 reduced rows, 17 pair identity receipts, all nine measured candidate counter sets, and individual SHA-256 receipts for all 34 sample files. Fixture-reference comparison checks source identities only, never its timing values.

Reproduce from the repository root:

```sh
python tools/research/incremental-demand/profile_validate.py --repo . --run-dir out/runs/full-pipeline-profile-o2-r7 --source 900293cfd8afcd7118775e8165e1ec4f5266e3d7 --build-receipt out/runs/full-pipeline-final-build/candidate-build.json --controller-exit unknown --output-stem /tmp/full-pipeline-profile-900 --reference-fixtures out/runs/full-pipeline-profile-o2-r6/rows.jsonl --expect-plan-fragments --expect-body-revision --expect-relocated-input-revision --expect-whole-machine --scope-note 'Frozen 900 includes plan fragments, relative inputs, mixed function lowering, shared candidate arrays, current-input whole machine reuse, and import-position input bindings. External controller response unavailable; persisted helper exit is checked independently.'
```

Add `--main-binary MAIN_SOURCE_CHECKOUT/target/release/zil` to physically recheck main as well as the saved candidate binary; recorded compiler identities and receipt consistency are always checked. No build or measurement is run by this validator. No root source was edited to produce this report.
