# Full-pipeline raw checkpoint v4

Final compiler `900293cfd8afcd7118775e8165e1ec4f5266e3d7`; evidence/tools `3b955a5197c9001211e4777615862a9bbda80eb9`. This explicit delta preserves24,642 regular files/2,600,107,619 raw bytes in9 parts/215,359,477 compressed bytes. Complete archive SHA256 `e16c1ed5ccb48cf02f0f15e44457fc446627d243c34be9bb7d22a49ad8178863`. The manifest binds each file, part, size and permission mode. Sources are reachable ancestors via an archive-only ours merge; main is unchanged.

Included: exact900 fullworkspace6253, final release binary and receipt, import-position tests, original Clippy01 failure and Clippy02/fmt02 success, r7 all34rows/180 recorded native outcomes, both matched six-block windows (each270rows:144measured+126parents), all2808 recorded native executions and actual unit/symbol contents, scoped nonauthor source and independent evidence reviews, compiler baseline snapshot, r6 fixture identities and timing/disk-boundary receipts. Final matched negatives remain. The debug build cache was removed between check completion and O2 start, with no timed request active. r7 controller observation remains unknown; helper exit0 is a distinct observation. No original lost pre-continuation raw is claimed recovered.

Prior stages and failures remain in v3 at353a6754bb4225fbd3281a94206e738488f81aec and v2 at7fd718bc80535a5e0b19eff39418e49a1b4aa9a8. This checkpoint contains enough bytes to revalidate final900 independently of those older raw trees.

Clone https://github.com/zil-lang/zil.git using `--single-branch --branch archive/source-history`, check out the exact v4 archive commit in the research recovery receipt, and run these commands from that fresh clone:

```sh
mkdir recovered-tools
for name in archive_evidence profile_validate matched_validate rust_compare matched_summary; do
  git show 3b955a5197c9001211e4777615862a9bbda80eb9:tools/research/incremental-demand/$name.py > recovered-tools/$name.py
done
python3 recovered-tools/archive_evidence.py verify --archive-dir research/incremental-demand-full-pipeline-20261112-v4 --restore-dir restored-v4
python3 recovered-tools/profile_validate.py --repo restored-v4 --run-dir out/runs/full-pipeline-profile-o2-r7 --source 900293cfd8afcd7118775e8165e1ec4f5266e3d7 --build-receipt out/runs/full-pipeline-final-build/candidate-build.json --controller-exit unknown --main-binary restored-v4/out/runs/full-pipeline-baseline-snapshot-v3/main-zil --reference-fixtures out/runs/full-pipeline-profile-o2-r6/rows.jsonl --expect-plan-fragments --expect-body-revision --expect-relocated-input-revision --expect-whole-machine --output-stem /tmp/zil-profile900-recovered
python3 recovered-tools/matched_validate.py restored-v4/out/runs/full-pipeline-matched-check-900-r1
python3 recovered-tools/matched_validate.py restored-v4/out/runs/full-pipeline-matched-o2-900-r1
```

Keep restored-v4 inside the clone so the profile oracle is bound to the pinned source Git blob. Validators recheck recorded native outcomes against the pinned oracle and current archived artifacts/identities; they do not execute a compiler or recreate overwritten historical executables. Both saved compiler binaries are physically rehashed. Unit carry-forward/inode observations refer to the original measurement, not normalized recovery filesystem metadata. RSS scopes and new Cargo/resident ZIL/LLVM/checking differences are preserved. The research branch records actual remote readback after it happens; the local pack alone is not a durable backup.
