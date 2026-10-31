# Stage 6 CI disk recovery

This package preserves the real runner disk failure and the limited recovery process.
It does not declare whole CI or compiler tests passed.

- Run `41207249551` on 888 failed with a runner ENOSPC annotation. Its annotations,
  checks and jobs JSON, zero-byte failed.log and 22-byte empty logs.zip are retained
  exactly. The empty downloaded logs do not establish which source tests passed
  or which directory caused the earlier disk exhaustion.
- Author controls and independent scoped review are preserved with all initial
  scripts, logs and revisions. Root found that creating the receipts directory
  under sudo would leave it root-owned. The ordinary runner now creates the parent
  before sudo. A second correction explicitly makes the single JSON receipt 0644
  regardless of umask. Initial reviewers' same-user controls did not test sudo or
  cross-UID ownership; later follow-ups retain that limitation and their fixture
  assertion/temporary-directory errors rather than relabeling the first review.
- Run `41209327141` produced and uploaded the actual cleanup receipt. Free space
  increased from 11624448000 bytes (10.82611084 GiB) to 33555800064 bytes
  (31.25127411 GiB). Android, dotnet and ghcup fixed paths were removed; /opt/ghc
  was missing. The protected repository and temporary evidence root were not cleanup
  targets. The ZIP member is byte-identical to the separately retained JSON.
  Receipt status Pass means this cleanup succeeded. At evidence handoff the whole
  run remained in progress; no full CI acceptance is inferred.

`research/stage6/ci-disk-recovery/` and `ci-disk-recovery-review/` preserve all author
and reviewer epochs. Adjacent `github-41207249551-*` and
`ci-disk-recovery-41209327141.{json,zip}` are original downloads. Collection-source
contains the exact final workflow/tool/proposal bound by author-final.json plus the
collector. Earlier source identities remain distinct.

Schema 2 retains per-path bytes/SHA and deduplicates exact byte blobs. No input in
this small package was omitted. Existing stage6 packages were not changed.

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/ci-disk
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/ci-disk --dest /tmp/stage6-ci-disk-restored
```
