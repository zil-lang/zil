# Stage 6 syntax-session

This frozen schema-2 package preserves original evidence, not rerun timings.
Measured candidate: `ffe4b814e22d8db79985fdd6fe397fbf77e6a243`; isolation baseline
`ccf1d3fdbb02bcecefecf0703278cde65e919a95`; combination baseline
`3d6babe17ee9d0a2f62d123063fb7af6a0886612`.
Collection HEAD is newer and is never a measured compiler identity.

Restore all retained bytes with:

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/syntax-session --dest /tmp/syntax-session-restored
```

Use `--verify evidence/research/incremental-compilation/stage6/syntax-session` to stream-check
all manifests, segments and blobs. Gzip JSONL manifests preserve each original path,
byte count, SHA and disposition; all compressed streams are split at 40 MiB.
The external manifest is compressed too. Exact ELF/object/cache bytes are omitted
and explicitly inventoried; source, raw stdout/process/requests, failed controls,
commands, source/ELF identities and expected outputs remain retained.

Retained paths: 527635; omitted paths: 1756;
unique raw blobs: 957; archive bytes: 43112842.

The two roots `research/stage6/session-syntax-{isolation,vs-3d6}/` each contain
840 timed requests / 70 cells / 420 native controls and 126 preparation records.
The frozen lease records exclusive six-block AB/BA execution. Preparation timing
is not formal timing. Both modes are check/O0, with 32 raw slots and no disk semantic
cache. Time is compile_project only; VmRSS is current retained memory after the
native control, not per-request peak. The later LLVM/main candidate is unmeasured here.
Independent full recomputation is separately archived in `syntax-session-review`.
