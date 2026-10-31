# Stage 6 syntax-session-review

This frozen schema-2 package preserves original evidence, not rerun timings.
Measured candidate: `ffe4b814e22d8db79985fdd6fe397fbf77e6a243`; isolation baseline
`ccf1d3fdbb02bcecefecf0703278cde65e919a95`; combination baseline
`3d6babe17ee9d0a2f62d123063fb7af6a0886612`.
Collection HEAD is newer and is never a measured compiler identity.

Restore all retained bytes with:

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/syntax-session-review --dest /tmp/syntax-session-review-restored
```

Use `--verify evidence/research/incremental-compilation/stage6/syntax-session-review` to stream-check
all manifests, segments and blobs. Gzip JSONL manifests preserve each original path,
byte count, SHA and disposition; all compressed streams are split at 40 MiB.
The external manifest is compressed too. Exact ELF/object/cache bytes are omitted
and explicitly inventoried; source, raw stdout/process/requests, failed controls,
commands, source/ELF identities and expected outputs remain retained.

Retained paths: 15; omitted paths: 0;
unique raw blobs: 13; archive bytes: 2079008.

`research/stage6-syntax-session-review/` contains the independent script, exact
measured tool sources recovered from ffe4, all 140 cells and fixed 2000 paired-block
bootstrap intervals in `recomputed.json`, complete tables in `review.md`, per-request
and per-control audits, and the complete 440389-path inventory (101088529 raw bytes).
Actual source contents were rehashed for all 438620 source snapshots. No compiler
or native benchmark was rerun. The first audit incorrectly expected null for the
disk_cache field; the schema's disabled value was corrected, and both the failed
script/log and complete successful second run are preserved. This audit error is
not a compiler failure. No multiple-comparison or universal performance claim is made.
The audit's exact build targets are needed to rerun all original binary checks;
omitted ELF/rlib bytes cannot be reconstructed solely from this archive.
