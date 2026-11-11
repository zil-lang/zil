# Incremental producer continuation

User authorized a new research branch continuing incremental compilation. Later
steering explicitly requires actual main and Rust comparisons, not comparison
against another active research worker. Implementation and measurement follow
that scope; no main merge or default enablement.

- Branch: `research/incremental-demand-propagation`.
- Historical starting point: `ea0ce45741fde0082b6c92173b7480b167c888d4`.
- Formal baseline: actual main `99035931d358add0eaed6d4535035c38413701dd`.
- Frozen measured compiler: `0601b9786d0fe683b4321f0d086230e5be027602`.
- Delivery report/protocol/reproducer: `tools/research/incremental-demand/`.
- Primary raw evidence: `out/runs/main-matrix-r1`; Rust check and normal
  incremental CGU256 O2: `out/runs/rust-main-check-r2` and
  `out/runs/rust-main-cgu256-r1`. Ordinary CGU16 supplement is separate.

Implemented immutable shared CoreModule/SourceMap assembly and sealed scalar
materialization receipt replay. Current environment admission, budget accounting,
first diagnostics, independent Verify and richer-domain fallbacks remain. A
query-history identity bug was reproduced and fixed. Default-worker stack tests
also exposed a regression inherited from the research base relative to main;
private state/report heap storage and separated frames fixed it without changing
stack size, budgets, depths or assertions.

Independent source review accepts exact 0601. Exact-source workspace lib1654,
LLVM API generic214, strict all-targets Clippy, fmt and release build pass. Earlier
full-workspace run passed6119/failed1; the original sole stack failure is fixed
and revalidated in the full generic suite. Do not claim the whole workspace was
rerun after the storage fix. Raw failures and successful followups remain in
unique validation `out/runs/` directories.

Six paired blocks show reliable main-to-candidate check gains for private and
three-file edits. Actual materialization/Lower work is1/3modules with21/19shared
modules, zero map-entry copies and unchanged CFG counts. This is a whole-branch
comparison, not isolated attribution to the continuation patch. Main already has
substantial caches. Graph B-to-C gain remains unproven. CGU1 O2 multi-edit
regression and negative/incomplete configurations remain visible. Rust is a
separate workflow/timing window with different assurance and partitioning;
cross-language median ratios are descriptive only.

Keep one heavy build or measurement at a time, independent target directories,
frozen binaries/helpers during each run, and named-parent/Source/native outcomes.
Legacy tree-named RSS fields only observe the root in this environment; retain
raw names but do not claim process-tree or Rust memory advantage. Long-session
controls are finite entry/source bounds, not byte bounds or leak proofs.

The former base-cost profiles remain provenance/diagnosis only. The accepted
delivery is a bounded research result with reproducible evidence, not closure
of the general incremental compilation or Rust parity goals. Final numbers,
hashes, review receipts, caveats and archive index are in the delivery report.
