This checkpoint preserves 51,872 regular files / 3,108,286,255 raw bytes: complete full-pipeline evidence through profile-r4, both frozen140 three-arm matched windows, prior failures, immutable measured binaries and build receipts, and the exact large-history regression. New Verify implementation/tests and future windows are outside this checkpoint. The original pre-continuation environment's lost raw is still unavailable.

Compressed archive: 261,930,012 bytes in eleven manifest-ordered 24 MiB parts (last shorter), SHA256 `86ca0b81d324f745078eb7b6da176ee814d443188138b7edf88914b69ec9120e`. `evidence-manifest.json` carries every part/file SHA and byte length. The helper rejects links/special files, path escapes and altered/extra payloads; it restores regular bytes and permission bits to a new directory. Sources are preserved as ancestors of this archive commit. Archiving is not performance or general correctness acceptance.

Recovery from this repository after a fresh HTTPS clone:

```sh
git show 0c2ddfcb8fa1cebbe033a1dfa47c338af69f1df6:tools/research/incremental-demand/archive_evidence.py > /tmp/zil-evidence-recover.py
python3 /tmp/zil-evidence-recover.py verify --archive-dir research/incremental-demand-full-pipeline-20261112-v2 --restore-dir /tmp/zil-evidence-restored-v2
```

Then run each restored matched window's archived `implementation/matched_validate.py` against its restored directory. This rechecks recorded native outcomes and archived artifacts, not a new compile or a rerun of transient executables. Exact source140997 and e1ad747 remain separate observations. Cold/broad regressions and the r4 large-history failure remain preserved, with the subsequently tested575548 correction separately identified. Root's fresh remote recovery receipt is published on the research source branch after verification; local archives alone are not durable backup claims.
