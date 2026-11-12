# Full-pipeline raw checkpoint v3

This delta follows v2 at archive/source-history7fd718bc80535a5e0b19eff39418e49a1b4aa9a8. It preserves906 regular files/297,949,836 raw bytes, in2 parts/34,140,024 compressed bytes. Complete archive SHA256: `7b1990bb68a96712da48b6d4f5bf992d2898e6a430b2276e39d0f06532ad47ec`. The evidence manifest binds every part, file, byte length and permission mode. Sources are reachable ancestors, preserved by an archive-only ours merge; no source is merged into main.

Included: complete532 workspace6229 and693 workspace6242; original Verify/projection/plan/input/mixed failures and fixes; immutable main/532/693 binaries and build receipts; full425-function diagnostic profiles r5 and r6 with all9 scenarios; raw counters and recorded native execution; arrays targeted279Analyze/200driver/79IR/624Verify including initial constructor errors; wholemachine230backend/200driver; scoped reviews and network503 observations. Formal140 check/O2 matrices and older raw remain in v2. New import-position binding source and subsequent lint/full-workspace/release/formal windows are excluded. The old pre-continuation lost environment raw remains unavailable.

The latest source4bc is not a complete workspace or performance acceptance for arrays/wholemachine; those are targeted results only. Profile r5 binds532, profile r6 binds693. Cold/broad negative results and r6 positions421 BodyKeys are retained. r5's helper exit0 and tool-reported exit1 remain separate unresolved observations. RSS legacy fields represent only observed roots, not process-tree totals.

From a fresh HTTPS clone of this archive branch, check out the exact checkpoint SHA recorded by the research recovery receipt, then:

```sh
git show 4bc594151f5dc8f83ecb73f88c769196318a5a44:tools/research/incremental-demand/archive_evidence.py > /tmp/zil-evidence-recover-v3.py
python3 /tmp/zil-evidence-recover-v3.py verify --archive-dir research/incremental-demand-full-pipeline-20261112-v3 --restore-dir restored-v3
git show 4bc594151f5dc8f83ecb73f88c769196318a5a44:tools/research/incremental-demand/profile_validate.py > /tmp/zil-profile-validate-v3.py
python3 /tmp/zil-profile-validate-v3.py --repo restored-v3 --run-dir out/runs/full-pipeline-profile-o2-r5 --source 532e01141b644d13c6e350a7dd310c50226d19d3 --build-receipt out/runs/full-pipeline-proof-build/candidate-build.json --controller-exit 1 --main-binary restored-v3/out/runs/full-pipeline-baseline-snapshot-v3/main-zil --output-stem /tmp/zil-profile-r5-recovered
python3 /tmp/zil-profile-validate-v3.py --repo restored-v3 --run-dir out/runs/full-pipeline-profile-o2-r6 --source 693f55cd0394a1a72a0bfe9d680740d9509885d1 --build-receipt out/runs/full-pipeline-input-plan-build/candidate-build.json --controller-exit 0 --main-binary restored-v3/out/runs/full-pipeline-baseline-snapshot-v3/main-zil --reference-fixtures out/runs/full-pipeline-profile-o2-r5/rows.jsonl --expect-plan-fragments --expect-body-revision --output-stem /tmp/zil-profile-r6-recovered
```

`restored-v3` is inside the fresh archive checkout so pinned Git source objects are available to the validator. Each profile rechecks34rows/17pairs/180 recorded native outcomes against the pinned original source oracle and equal paired LLVM/ELF recorded hashes. This is byte recovery/revalidation, not new native compilation or reexecution of overwritten per-request executables. Candidate/main compiler bytes are rehashed. The research branch will record actual remote recovery separately after it completes; a local archive alone is not durable evidence.
