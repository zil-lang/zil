# Stage6 correctness and author controls

This package preserves the original Analyze/DB and semantic-cache independent
reviews, the later MergeFunctions and algorithm-data reviews, their commands,
logs, probes, fixed source snapshots and failure controls. Merge reviewer `exact/`
and `target/` are deliberately outside this collection; its review, commit receipt
and source hashes remain included. The two formal Session windows are also separate.

Author evidence includes reactive/analysis checks, semantic-cache tests, the 230
shape requests and their later audit, Session protocol/identity controls, the old
DB ablation preparation and source, Merge tests and their initial failures, tool
failure controls, and the latest Rust-source/SLP/Merge references. Shape controls
have 220 valid requests and 10 existing budget refusals; they are functional
observations, not a new performance window. Surrounding original run logs are kept
verbatim; formal Merge observations have a dedicated `merge-screens` package.

`FILES.json` maps 48,338 retained paths to exact raw-byte blobs; `OMITTED.json`
individually identifies excluded ELF, compiled caches and large semantic candidates
by path, size and observed SHA. The six 29-byte `incremental` fault stubs and all
24 small semantic-review `.candidate` controls are retained. `COVERAGE.json`
checks those boundaries and the required review/preparation receipts.

`VERIFY.json` checks every blob and sidecar. Use `stage6_archive.py --extract
<this-directory> --dest <new-directory>` to reconstruct retained files and ordinary
rwx permissions. It does not reconstruct omitted binaries, symlink entities,
hardlink topology, empty directories or timestamps. Original compiler/tool/source
identities remain in their own receipts; collection HEAD is not a replacement.
