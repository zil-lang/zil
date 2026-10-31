# llvm-combined frozen evidence

Frozen four-TU LLVM combination: original and candidate same-source API pair, source generators and pinned C++/licenses, fresh rebuild, five functional profiles, failures and independent review. Diagnostic build timings are not formal compile results. Original producer copies and measured source888 identities remain authoritative; current quality fixes and formal CLI matrices excluded.

Original raw bytes and source identities are retained, with each original path mapped to one content blob. JSONL manifests are gzip-compressed externally and internally; every stream and each <=40MiB segment is SHA-bound. Omitted binaries/cache bytes are individually inventoried. Source and request identities in original receipts remain authoritative; collection HEAD is not a measured compiler identity. Only retained files and ordinary permissions are reconstructible; not omitted files, symlinks, hardlink topology or timestamps.

Verify with:

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/llvm-combined
```

`ROOTS.json` and the adjacent `llvm-combined-plan.json` specify selection; `FILES.jsonl.gz` maps every retained path to its content blob; `OMITTED.jsonl.gz` records each omitted binary/cache or symlink identity. Restore uses `--extract <package> --dest <empty-directory>`. Source checkout layouts and original absolute paths must be restored or intentionally remapped before replay; transport validation is not a rerun of compiler controls.

Immutable744 tool sources include all six generator/driver folders, pinned LLVM C++/licenses, installer/release recipes and Cargo identity. The exact measured producer copies remain separate. The fresh rebuild receipt b733 revision refers to ZIL input source, not the generator commit. Original pair control IR/C/native oracle inputs are included. Original/candidate DSO, archives, objects and generated ELF are SHA-inventoried, not duplicated.
