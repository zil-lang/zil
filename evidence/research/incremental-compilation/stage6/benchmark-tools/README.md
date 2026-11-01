# benchmark-tools frozen evidence

Frozen benchmark author and independent controls, initial failures and later timer/opaque/reference-cache revisions. Immutable df04 delivery sources are separate from a638 Git revision and its c682 bench file SHA. No final formal CLI matrices are included.

Original raw bytes and source identities are retained, with each original path mapped to one content blob. JSONL manifests are gzip-compressed externally and internally; every stream and each <=40MiB segment is SHA-bound. Omitted binaries/cache bytes are individually inventoried. Source and request identities in original receipts remain authoritative; collection HEAD is not a measured compiler identity. Only retained files and ordinary permissions are reconstructible; not omitted files, symlinks, hardlink topology or timestamps.

Verify with:

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/benchmark-tools
```

`ROOTS.json` and the adjacent `benchmark-tools-plan.json` specify selection; `FILES.jsonl.gz` maps every retained path to its content blob; `OMITTED.jsonl.gz` records each omitted binary/cache or symlink identity. Restore uses `--extract <package> --dest <empty-directory>`. Source checkout layouts and original absolute paths must be restored or intentionally remapped before replay; transport validation is not a rerun of compiler controls.

The first unfrozen capture lacked three imported df04 helpers; the correction receipt is retained, and the final capture includes merge-runtime, real-pair and distribution helpers. Initial/final failures, stdout and historical tool bytes are preserved. a638 is a Git revision whose bench file SHA is c682, not a different file SHA.

Running the full real-project generator also requires the exact df04 repository checkout, including `examples/mainline/`; this package does not claim to be a complete Git checkout. Actual controls retain their source copies. The recorded scripts contain their original repository/absolute path assumptions, which must be restored deliberately.
