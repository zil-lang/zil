# Session windows and preparation

This schema-2 package preserves both completed formal roots: `session-db-isolation`
and `session-scalar-vs-3d6`. Each has 840 requests, 70 cells, 420 native controls
and 126 separately recorded preparation controls. The earlier 529
`session-final-vs-3d6` root is **SupersededPrepared**: 126 controls and zero formal
requests. Preparation, cancellation/recovery, runner/probe identities, source
versions, window receipts and static reactive-key attribution remain separate.

The DB comparison isolates the DB change; the scalar-versus-3d6 comparison measures
a combined production change. Its results cannot be attributed solely to the DB
or scalar optimization. Timed work is the in-memory `compile_project` call; source
writes, native and fresh controls are outside timing. VmRSS is current parent
resident memory after native checks, not a request or child-process peak.

All 547,991 retained paths map to exact SHA-addressed bytes. The 264 MB logical
FILES manifest is stored as 4.95 MB gzip JSONL; no oversized plain manifest is
checked into Git. Raw gzip bytes are split into two ordered segments, each at most
40 MiB. `IDENTITY.json` binds every part, concatenated stream and logical manifest.
The 1,760 omitted ELF/object/cache paths retain sizes and observed byte hashes.

Verify or restore with `tools/research/incremental-compilation/stage6_archive_large.py`
using `--verify <this-directory>` or `--extract <this-directory> --dest <new-dir>`.
Use this schema-2 reader, rather than the earlier schema-1 collector. `VERIFY.json`
records full segment/manifest/blob validation. Original receipt identities remain
authoritative; collection HEAD is not the compiler identity for both windows.
