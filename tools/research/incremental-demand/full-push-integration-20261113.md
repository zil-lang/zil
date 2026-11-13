> Superseded current status: [7bac component acceptance, profile16 and main HOLD](full-push-checkpoint-7bac.md). The initial compile failures and pending states below are historical, not the latest verdict.

# Main integration and retained research, 2026-11-13

The user now explicitly authorizes merging independently accepted stage results into main and retiring delivered branches. This does not authorize merging the complete research branch or enabling its experimental switches by default.

## Main stage

`perf/main-cache-serialization@aff75567688d493cbd3d3f006f6952d6923bb6c0`, based on main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`, changes only IR Wire and the real FunctionSession cache publisher. Immutable bounded per-function raw encodings remove complete cached CFG cloning and unchanged function serialization from repeated publication. Wire bytes, complete Limits, exact first budget error, decode checks and independent Verify remain current. This does not remove whole output assembly or disk writes.

The original static review rejected early capacity release; the final implementation frees encoded bytes before returning shared capacity. Independent exact-source addendum accepts that fix. The frozen complete gate began with format pass, but workspace compilation ran out of disk, returned101 and executed zero tests. Its linker bus errors are infrastructure failure, not a pass or a source correctness conclusion. Original main-cache-workspace-01 logs and receipts remain under the stage worktree out/runs. Release/native/reference/clippy gates have not executed. No main merge yet.

## Research

Frozen production source `5341ac90b` integrates actual machine output pieces, the bounded per-basic-block MC emission product, optional production observer and the two real CFI/forward-symbol oracles. MC still restores and verifies current MachineFunctions and runs current function envelope, MC encoding/layout and final output. A full no-MF function product remains a design with unclosed effect obligations; it is not implemented or accepted. SDK17 compilation failure and SDK18 successful nine-TU rebuild are both retained. SDK18 and failedSDK17 full inventories are durably archived/freshly recovered in V23. No native/Cargo unified tests or timing yet for this combined source. Prior verified production remains96f; latest original425 measured source remains4b809 (private482/main1505ms; cold7366/main6070 and broad6211/main5210ms remain negative). Do not attach those times or old Rust comparisons to5341.

## Delivered branch retirement

Eight delivered root-owned author branches and clean worktrees were actually retired after exact status, remote-tip, ignored-file hash, patch-equivalence and remote archive ancestry checks. The original eight commits are real parents of fixed archive anchor `a33273f9fdca04c2a3c86c84da64d9f556d0fbb7`. Independent HTTPS clone checked all commit/tree objects and20 changed source blobs. Atomic remote deletion used expected-tip leases; no branch source history was overwritten. Own operation reduced remote heads66 to58; the earlier95 inventory is historical and concurrent unrelated cleanup is not credited here. New archive head12c is a legitimate successor by another worker and was preserved. Main, goal, original468 continuation, current research, active MC/output pieces, validator and unique negative prototypes remain protected. Two ignored CPython bytecode caches were the only files removed beyond tracked trees. Conclusions/source are retained in current research and the archive manifest.
