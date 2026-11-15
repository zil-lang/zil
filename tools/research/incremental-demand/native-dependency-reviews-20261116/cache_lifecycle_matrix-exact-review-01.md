# Bounded cache lifecycle tool review — READY

External deliverable: `/workspace/zil-recovery/cache_lifecycle_matrix-01.py`, 245 lines, SHA-256 `e35c02e298ae94338e5206a805c40eddac9108105c6983fbb7d757bdddc2eccd`.

Static author review found no blocking issue in the composed paths. No script, plan, compiler, test, or native execution was performed. No repository files or Git state were modified. Installation, tool-only freeze/pins, plan review, and measurement remain root-owned after compiler gates/release freeze.

Reviewed helper hashes:

- `closed_project_matrix.py`: `e00d3d3830994b5273bd7f1ac51f3f35a8b0468c7d955c7b77e7d2c4f2e9b7de`
- `closed_project_quality.py`: `de0daf5aeb2790503fcc10ce84ceaf0a7718069446e7f1aa04d713c5cfbb048d`

The wrapper composes existing main, initialization, generation, request-window, report, schema and quality functions through FunctionType/global namespace copies. Imported module globals are not patched. Quality cells, manifests, summary and runtime decisions use distinct retain/clear-on-restart schemas; archived producer policy remains the original `closed+off+complete-imports` identity. Original fixtures, EOF plus 256-byte functional checks, current independent Verify, physical CGU16, O2/compiler jobs1, and sustained runtime gates remain the original consumers. Five successful sustained cases per size follow the six-request history selected directly from `matrix.history(8)`.

Cleanup is limited to restart worker directory `1`, after the original predecessor closes. It removes only that arm's semantic/object caches; the candidate boundary also removes the disposable Rust target while no Rust request is active. Owned new-window inode/path checks, fixed directory layouts, ancestor/tree symlink rejection, device checks, and fd-safe rmtree constrain deletion. ZIL publication/IR, source, SDK, pins, evidence and the other ZIL arm are outside the deletion list. Rust target clearing includes its executable. Completed or interrupted deletion attempts retain operation timing and before/after disk snapshots; startup has separate receipts. Compile totals are explicitly named `compile_request_totals`, not user end-to-end totals. No partial-history GC or OS-cold-cache claim is made.

The installed tool must execute from its expected repository path. Original initialization still verifies all root helpers against `pins.tool_source`; the installed tool joins implementation archives, manifest-relative hashes, frozen Git-byte checks and post-run SHA checks. Plan-generated `run_argv` retains the selected cache state and chooses a new measurement directory. Root must pin the installed tool commit; external READY is not frozen-tool or measurement acceptance.

Root's current gate report is reused without rerunning: Expand 124 passing, CT 76 passing. Backend remains 293 passing/6 failing: four new emit fixtures with unbound LIR under Lovelace's repair, one exact work-counter expectation and one native-profile environment failure under root. These are pending integration results, not harness acceptance or new runs by this reviewer.

The earlier source-consumer review remains `/workspace/zil-recovery/native-dependency-emit-exact-review-01.md`. Subsequent read-only inspection of root's Expand OR gates confirms that full function-input compatibility can restore moved-import rebind/item reuse while explicit placement-to-current import span comparison and existing negative alias/module/file/context guards remain intact. No further source edits or gate reruns are requested by this review.

Status: READY for root installation after freeze; implementation and review work paused.
