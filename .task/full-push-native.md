# Full push and native products implementation

Active user mandate (2026-11-12): implement the complete source-to-executable push path and LLVM optimization plus emission incrementality together. The earlier emission-only feasibility plan is not an accepted endpoint. Keep ordinary optimization quality, independent Verify, budgets, error ordering, and all measured scenarios. No default research enablement or main merge.

Isolated branch `research/full-push-native`; base b7703b95a59cc87dd1ac13ce6f652386ea4dfd0d merged current main 42f0396f36b2a36860ecb55371d436e4327877a4 as b3c918149f84ed1f65608781d104561a007ec4f9. Original research and other worktrees remain intact. Current main is newer than the old 99035931 measurements: those numbers are historical, not this candidate's performance.

Implemented checkpoints, not whole-task acceptance:
- Persistent typed/plan/LIR function tables with exact immutable owners, changed-subtree traversal and a separate versioned structural product identity. Legacy Wire bytes/digests preserved. Dense middle insertion/removal still rebuilds/reindexes.
- Transactional reactive scheduler, activity closure and rollback. Independently reviewed clone-history and first-error defects fixed. DB API alone is not a production consumer.
- Expand sealed change batches and Types actual sparse name/CFG consumers for proved contexts; unknown declarations/effects use current full checker. Position rebasing and global/complex work remain.
- Root wiring under explicit default-off `--push-reuse` in progress; Analyze, independent Verify, Lower and LLVM work are being integrated. No end-to-end speedup claimed yet.

Root owns shared IR, driver, Cargo/schema, integration and the only build/measurement window. Authors have distinct new worktrees. Raw logs are currently in this worktree's `out/runs`; they are local and not yet a cross-environment archive. Source checkpoints are pushed to the research branches. Previous frozen 900 evidence remains recoverable from archive/source-history@32e1b04fb859440bb36ec3223cb35b26edf63041; the earlier lost environment raw is still unavailable.

Checks so far: current-main integration check02 passed; persistent IR test04 passed 105 tests; integrated IR/Expand/Types run01 passed 424 tests, including actual 425-function delta/fresh controls; DB run01 passed 38 unit plus 1 compile-fail doc test. First failures and later runs have distinct files. These totals overlap and must not be summed as unique coverage. Full current workspace and native comparison are pending.

LLVM SDK headers extracted only after validating official archive 1190060068 bytes / SHA256 efdd1dbd089b99bfcfcde7b8eb1236163f2e88df5a981cca4f783ec9cb5e0378; source commit 7a941dfb2877a0b8cf30323fefa85ad019e0b0f5. Original installed DSO remains the quality control. ThinLTO/ORC/Zig references do not establish generic function-level O2 replay; every implemented replay needs actual optimizer and codegen dependency closure.
