This receipt covers the one actual root R1 workspace default run at
`b05a70b2ce834ff2ea93618035eec35ff136b622`. The root check records 3733/0.
The selected 256 suites record 19/0. No compiler or native program was run by
this collection, restoration, validation, or transport work.

| Actual group | Normal native | Allocator native | Total native |
| --- | ---: | ---: | ---: |
| Original identity and six signature probes | 14 | 0 | 14 |
| Frozen legacy 13 modes | 26 | 82 | 108 |
| Frozen extended 12 modes | 24 | 108 | 132 |
| FnMut direct-F positive pair | 2 | 0 | 2 |
| Separate ordinary no-keep supplement | 4 | 16 | 20 |
| Separate caller-branch selection supplement | 4 | 8 | 12 |
| Total from this one R1 run | 74 | 214 | 288 |

The original group is 256 native runs with 190 allocator runs and 90 actual
allocation failures. The separate supplement is 32/24/8. All observed allocator
paths have live=0. There are 2148 native harness assertions. Historical runs are
not added. Nine original source negatives and one FnMut pair negative reach
their pinned semantic rejection reasons. Two self-containment Unsupported
boundaries are distinguished from semantic rejections. The historical
FnMut-to-i32 B0 gap remains unfinished and is not counted as Pass.

`RESULT.json` records every observation and the external mapping from original
absolute artifact paths to the archive's logical index. Original report JSON
and source bytes remain unchanged. `ACCEPTANCE.json` summarizes current scope
and the four Rust goals. Whole-workspace integration, reference gates, and
remote CI acceptance remain root-owned.

The full finalized workspace archive has SHA-256
`fc6708ccd6d53f5ca72a05590b3168e555be563ccf06fe939c6268b1922f3b54`.
The independent streaming selector verified every archive blob before restoring
2047 selected originals (114872122 logical bytes), including phase IR, programs,
objects, probes, scripts, source copies, and all modes. `RESTORE.json` records
this selection. The auditor's exact source snapshots are included in
`independent-audit-source/`, with author provenance in the restoration receipt.

Actual commands, without additional compilation/native execution:

```sh
source /workspace/.zil-env/env.sh
python3 /workspace/.zil-env/forward/ci-audit/select_restore.py \
  --archive /workspace/.zil-env/forward/R1/workspace.tar.zst \
  --repo /workspace/zil-251 \
  --expected-head b05a70b2ce834ff2ea93618035eec35ff136b622 \
  --output /workspace/.zil-env/forward/R1/consumer256-workspace-selected
python3 tools/research/256/final_receipt.py \
  --runner /workspace/.zil-env/forward/R1/consumer256-workspace-selected/restored \
  --index /workspace/.zil-env/forward/R1/consumer256-workspace-selected/INDEX.json \
  --archive /workspace/.zil-env/forward/R1/workspace.tar.zst \
  --source-commit b05a70b2ce834ff2ea93618035eec35ff136b622 \
  --validators /workspace/.zil-env/forward/ci-audit \
  --out /workspace/.zil-env/forward/R1/consumer256-FINAL.json
GITHUB_SHA=b05a70b2ce834ff2ea93618035eec35ff136b622 \
  python3 tools/research/256/transport.py \
  --runner /workspace/.zil-env/forward/R1/consumer256-transport/runner \
  --out /workspace/.zil-env/forward/R1/consumer256-transport/result
```

The transport runner is a mode-preserving copy of the restored originals plus
ten receipt/auditor-source files; it does not edit an original file. Its actual
pack/emit/recover/verify/restore round trip preserves 2057 paths and 48 executable
files, 116001929 logical bytes. The 658974-byte archive has SHA-256
`5825d979d263adb81663f94e03968d59fec5d9fa7bc5b34d3e3ae26c5bb12086`.
`TRANSPORT.json` binds the unchanged producer, all four real command results,
and exact paths/bytes/SHA/modes. Portable input directory labels use byte hex.
Its `tested_commit` identifies collector/transport tools at 672d8ecc9, while
the actual native compiler identity remains the R1 commit above.

To restore the committed selected archive into a new path without executing
any program:

```sh
python3 tools/ci/evidence_transport.py verify \
  evidence/impl/256/final-R1/native-evidence.tar.zst \
  --restore /tmp/impl256-R1-replay
```

Missing optional consumer209/LLVM roots in this selected local archive are
explicitly recorded. This archive is a complete selection for 256, rather than
a replacement for the full workspace archive or remote evidence.
