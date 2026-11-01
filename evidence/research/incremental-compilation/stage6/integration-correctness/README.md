# Stage 6 integration correctness

Frozen limited correctness evidence for upstream integration `ac4ebb9e5fe668b160cbc4b1fe30db735858fdce`
and matching DB test repair `888af53a0a7389d10a508250059db6b631c61e4a`.
This package is not full-workspace acceptance or a performance result.

Root ran 75 scoped tests, Clippy for seven named crates with llvm-api/all-targets,
and workspace fmt. Independent ac4 review ran 3 tests, 29 complete warm/fresh report
pairs and 4 LLVM API O2 native executions. Initial fixture/expected-first-error/restore
assumptions are preserved along with corrected tests and complete reports.

GitHub run 41206406105 failed because the withdrawn DB implementation still had
private tests for removed reverse-index APIs. Its original failure log and pre-repair
tests remain. Commit 888 restores the exact 3d6 matching test module; production DB is
unchanged. Default/reference each passed 13 tests (13 unique, 26 executions), with DB
Clippy. The supplemental reviewer checked source and root logs; independent reruns=0.
Root reported the later 412072… CI as in progress when handing off these frozen inputs.
Its active records are not archived here and are not a Pass.

The actual ac4/888 CLI has identical SHA256:
`0d0e05f4b3ba14c298fef7907c8a6ec23f40f01351509c90afea63c1289246e1`.
Old measurements retain their original identities; future benchmark and loaded LLVM
library identities must be bound separately.

Selected original roots:

- `research/stage6/integration-checks/`: commands/logs, source/build identities, real
  CI failure, old DB tests, repair controls and release identity.
- `research/stage6-integration-review/`: root receipts/source plus all four `controls*`
  epochs, including initial failures. The exact checkout is excluded and identified
  by its commit/source inventory; build targets are excluded.
- `research/upstream-integration/`: only `STATIC.md`, `STATIC.json`,
  `resolve-expand.patch`, and `resolved-expand-lib.rs`. Static pre-merge scope is
  preserved as originally written; the worktree/full-diff/target are excluded.
- `research/stage6/old-rust-cache-cleanup-2-*` and
  `remaining-old-rust-cache-scan.json`: complete cache identity/deletion receipts,
  initial safe-stop and resumed cleanup; source/test/target caches were excluded by
  the cleanup. Omitted/deleted cache bytes cannot be restored from their SHA alone.

Active API, plugin, runtime, SSA candidates and subsequent CI are outside scope.

Schema 2 uses exact content-addressed blobs with per-path bytes/SHA/disposition in
compressed JSONL manifests. Each compressed stream is bounded to 40 MiB per part.
Only the two native ELF products are omitted here; their identities are recorded.
The collection-time collector source is separate from measured production inputs.

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/integration-correctness
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/integration-correctness --dest /tmp/stage6-integration-restored
```
