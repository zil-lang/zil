# CI evidence recovery

The official and research LLVM workflows run on pull requests to `main` and on
`main` pushes. Their concurrency groups and toolchain identities remain distinct.
Deleting a merged research branch does not disable research LLVM verification.

The Stage 14 runs `41321753073` (official) and `41321745753` (research) completed
their compiler tests, native controls, Clippy and formatting, but failed evidence
delivery because GitHub reported **Artifact storage quota has been hit**. These
historical runs remain failures. A successful test step is not proof that its
native evidence was uploaded; upload steps using `continue-on-error` must be read
through their `outcome`, not their displayed `conclusion`.

Normal artifact uploads remain the primary transport. If an upload fails, the
workflows preserve the remaining original files and use
`tools/ci/evidence_transport.py` to archive all files under `receipts`,
`native-evidence`, `consumer209`, and the LLVM installation receipt/log paths.
No Cargo or LLVM build/download cache is included. A phase's native directory can
have been removed only after its original artifact and inventory were uploaded
successfully; its successful artifact remains part of that run's evidence.

The recovery container maps every original path and mode to its byte count and
SHA-256. Identical contents share a blob; solid zstd compression retains native
executables and all source/IR/log bytes. It does not substitute a hash for a
missing payload. Limits are 256 MiB compressed, 16 GiB logical restored content,
250,000 files and a 64 MiB index. Exceeding a bound fails explicitly. Symlinks,
nonregular files, path traversal, missing/duplicate blobs and digest mismatches
are rejected. Publication first verifies the whole archive and rechecks the
current file inventory.

The fallback writes numbered 4 KiB base64 records between `ZIL_CI_EVIDENCE_V1`
BEGIN and END markers, with the run/checkout identity, exact compressed byte
count and archive SHA-256. The CI gate requires either successful primary
uploads or successful local verification and emission of the complete fallback.
Upload failures remain explicit in the run summary. Existing test failures are
not suppressed by evidence recovery.

**Local emission does not prove remote log preservation.** Before merging a run
that used fallback, the integrator must retrieve that exact run's complete logs,
reconstruct the archive and independently verify every blob. Truncated, reordered,
duplicated or incomplete records fail recovery. The expected run ID and checkout
must match the candidate being accepted. Keep the recovered archive and its SHA
with the delivery evidence; Actions logs remain subject to retention and are not
a permanent archive. For example:

```sh
gh run view RUN_ID --repo zil-lang/zil --log > exact-run.log
python3 tools/ci/evidence_transport.py recover exact-run.log recovered.tar.zst
python3 tools/ci/evidence_transport.py verify recovered.tar.zst --restore restored-new
```

Restoration requires a new directory and preserves original file bytes and modes.
The archive contains the source input snapshot and actual compiler/native test
receipts; it does not claim that a CI success satisfies ZIL's full language or
product performance gates.
