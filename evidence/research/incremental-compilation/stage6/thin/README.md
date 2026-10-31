# ThinLTO raw evidence

This archive preserves the actual ThinLTO experiment, its failed initial epochs,
section-GC repair, scalar/resource/opaque-input controls, required-current-post-key
corruption repair, and all four two-block screening directories. It includes the
actual unit LLVM text, normalization/preflight/export records, request commands,
stdout/stderr, tool/source/IR keys, and both request-local and frozen 644489605 tools.
The complete research source snapshot is retained; compiled tools and binary caches
are inventoried with observed byte hashes in `OMITTED.json`.

The screen contains 400 requests: 360 valid and 40 existing expansion-budget
refusals for body64. These remain two-block screening observations, not six-block
acceptance. The original `thin-author.md` and raw summaries retain the negative
compilation results and the narrower code-size observations.

`FILES.json` maps every retained original path to an exact SHA-addressed blob.
Identical raw bytes share storage; no LLVM text, receipt or observation was rewritten.
The gzip stream is divided into ordered parts below 45 MB. `IDENTITY.json` records
both part hashes and the hash of the original concatenated stream. Reconstruct with:

```
python3 tools/research/incremental-compilation/stage6_archive.py \
  --extract evidence/research/incremental-compilation/stage6/thin \
  --dest /tmp/stage6-thin-extracted
```

The two parts can alternatively be concatenated in their listed order for standard
tar inspection. Only retained evidence is reconstructible; omitted ELF, bitcode and
object/cache bytes require rebuilding from the preserved sources and commands.
`VERIFY.json` records complete blob and sidecar verification. Collection-time HEAD
and source files do not replace each original measurement's compiler/tool identity.
