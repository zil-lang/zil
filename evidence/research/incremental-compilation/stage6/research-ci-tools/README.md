# research-ci-tools frozen evidence

Frozen six-ed research LLVM CI workflow identity controls, original and independent fixtures with their exact fake-library bytes and immutable workflow/tool source. Local protocol fixture success is not hosted CI success or real compiler acceptance.

Original raw bytes and source identities are retained, with each original path mapped to one content blob. JSONL manifests are gzip-compressed externally and internally; every stream and each <=40MiB segment is SHA-bound. Omitted binaries/cache bytes are individually inventoried. Source and request identities in original receipts remain authoritative; collection HEAD is not a measured compiler identity. Only retained files and ordinary permissions are reconstructible; not omitted files, symlinks, hardlink topology or timestamps.

Verify with:

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/research-ci-tools
```

`ROOTS.json` and the adjacent `research-ci-tools-plan.json` specify selection; `FILES.jsonl.gz` maps every retained path to its content blob; `OMITTED.jsonl.gz` records each omitted binary/cache or symlink identity. Restore uses `--extract <package> --dest <empty-directory>`. Source checkout layouts and original absolute paths must be restored or intentionally remapped before replay; transport validation is not a rerun of compiler controls.

All small fake DSO/archive/clang fixtures are retained as bytes; these protocol tests do not prove that LLVM was loaded or that hosted CI passed.
