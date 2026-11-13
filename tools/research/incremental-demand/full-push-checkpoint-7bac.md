# MC body and persistent output consumers: actual acceptance

Compiled source `7bac1cc5b626594d8db36e9d765051e35b0661a2`, main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`. [Exact receipts and boundaries](full-push-checkpoint-7bac.json). This research remains default-off; the complete branch is not ready to merge into main.

## Actual logic

Changed source owners propagate changed body identities into the existing phase consumers. Independently checked current results feed Analyze, Verify and Lower. Persistent machine-output leaves carry equal/changed values into diagnostic output and CGU recipes; unchanged leaves need no complete body aggregation. An affected CGU still enters LLVM, where current function/stage dependencies authorize bounded IR/MIR products. The new MC body product bypasses the ordinary MI lowering loop for admitted unchanged functions, then current MC encoding, layout and final ELF production continue. Object and final-link products retain their separate current input checks. This is real work removal in several consumers, but the central reactive graph still primarily computes BodyKey, and whole-project phase reentry/global inventories remain. It is not one complete globally minimal push scheduler.

Rust queries are demand-driven: a top-level demand recursively checks dependencies and reuses results that remain green; fingerprint equality can stop propagation. ZIL's intended direction is changed-owner push through consumers. Both need complete stable dependencies and result equality. Rust also does whole-crate work and recompiles affected CGUs. The latest formal matched Rust window remains the six-block source328 window; the measurements below contain no Rust arm.

## Unified gates

The full MC/output-pieces implementation passed native79, backend252 and driver210 at `d846f8bf5da99587654ad1c23e0b03b11c861af8`. The last production change at7bac only changes a Copy-type JSON renderer receiver from borrowed to by-value; its JSON body is identical. Strict workspace Clippy, format and release build passed at7bac. Do not describe the earlier runtime groups as reexecuted at7bac.

First failures are retained: native30/31 failed during C++ compilation/Rust helper imports; native32/33 actually ran77 pass/2 fail because the CFI fixtures placed nonterminators after terminators. The corrected fixtures use legal MBB positions and retain current MachineVerifier, original MI-loop/cold/warm full-ELF equality and oracle masks63/127. [Astra exact consumer review](full-push-astra-mc-runtime-fixes-review.json). No verification was bypassed.

## Original425 complete diagnostic window

O2, requested and observed CGU16, jobs1, cacheall, capacity256; original20 modules ×20 functions ×16 steps, all9 scenarios and named parents. One alternating diagnostic block, candidate profiled and main unprofiled. Same Rust toolchain and pinned LLVM23.1.4 source; main loads stock DSO2bf890, candidate the SDK18 hook DSO785a24. This is not a formal unprofiled speed acceptance or causal MC attribution.

| Request | Main ms | Candidate ms |
| --- | ---: | ---: |
| Cold | 6053.607 | 7577.458 |
| Unchanged | 501.949 | 91.030 |
| Private | 1586.522 | 500.971 |
| Multi | 1591.967 | 642.883 |
| Broad | 4948.373 | 6974.165 |
| Positions | 1404.094 | 986.855 |
| Error | 393.682 | 48.618 |
| Recovery | 1705.547 | 519.341 |
| Restore | 499.039 | 165.700 |

[Qualification](full-push-profile16-qualification.json):34 rows,180 recomputed native outcomes; all30 successful rows' complete ELF blobs physically rehashed and paired bytes compared,5 unique ELF values. LLVM blobs and current runtime libraries checked. Runner62623 and qualifiercb01 are separately pinned from compiled source7bac. Qualification01's historical environment-pin error remains saved;02 uses an exact source-specific null ZIL_LLVM_CONFIG pin rather than accepting a wildcard.

Private edit actually replays39 MC bodies/234 blocks and12998 MC input events, skips12998 logical machine-instruction lowering visits, captures1 changed function and has0 failed replays. Current MF restore, current MachineVerifier, AsmPrinter envelopes, encoder, layout and relaxation still execute. `captured_blocks` can include partial declined captures; sealed counts are not published counts. Timer scopes are nested and cannot be added as a partition.

Largest measured costs: cold objects4645ms (optimization1525ms/emission2621ms), machine639ms, link identity802ms; private objects255ms, link identity86ms, diagnostic42ms, actual link37ms. The cold tool identity includes the first complete hash of a282MB clang lacking a GNU build ID, which cannot honestly be called duplicate work. Broad edits remain a serious negative result. Full LLVM terminal incrementality, cold/broad nonregression and a broad lead over Rust remain open.

## Main integration decision

The isolated FunctionStore serialization slice Aff passed default6073/reference5958 and all required correctness gates. Four full alternating blocks nevertheless leave private-edit nonregression unresolved; the independent [Astra decision is HOLD](full-push-main-cache-retest-astra-review.json). [Full qualification and evidence limits](full-push-main-cache-four-block-qualification.json), [all scenario results](full-push-main-cache-four-block-summary.md). A bounded same-core ordinary Wire codec probe found no encoder slowdown; it does not resolve whole-request risk. No main merge, no default research enablement.

The old four-block runner recorded all paired executable hashes but overwrote some historical full ELF files:40 earlier successful rows lack physically retained complete bytes. This gap is explicit and cannot be backfilled. The new runner retains every complete success ELF outside the existing response timing boundary, symmetrically for both arms.

## Recovery and branches

Ten delivered branches/worktrees were actually retired: the initial eight through archive anchor a332, then machine-output-pieces and validate-push-profile through actual archive parents at cc8c17c6. The [second retirement receipt](full-push-two-components-retirement.json) records exact current tips, fresh remote commit/source proofs, unchanged ignored-file hashes and atomic expected-tip deletions. Its own remote-head count changed59 to57; unrelated concurrent branch changes are not credited. All original tips and decisions remain recoverable. V24 fixed indexfb91/closure77f remains complete. [V25 recovery receipt](full-push-recovery-v25.json) now closes the newer results: fixed index `4b7cb33ada7e7491609f8354add0599036a53a95`, path `research/incremental-demand-late-stage-20261113-v25-index`; closure `c5c87682a49eba006b166ba85d24caa373798f37`. Four raw shards contain2,910 regular files/1,899,309,248B; exact union including MASTER is2,911 files/1,900,242,737B. Every shard and metadata was actually restored from remote HTTPS and checked for complete file hashes/sizes/modes. Full compiled7bac and delivery52e source snapshots are distinct, real Git ancestors are retained, and required full SDK18 remains available through the fixed V23 index. The new combined restore wrapper is provided; only the per-shard/metadata recovery executions actually performed are claimed. Local tar copies are not the persistence boundary.

## Rejected follow-up, preserved decision

[Astra source-bound review](full-push-next-slice-astra-review.json) rejected direct execution of the sole parsed clang -### ld command: a custom driver can delegate discovery yet intentionally change its real-link behavior. ELF format does not prove genuine Clang semantics. No source change/build/benchmark occurred; the empty local branch/worktree was normally retired, with no unique commit or remote ref. The current first clang content hash remains necessary, and warm installed-tool identities already reuse complete metadata. This is a negative conclusion, not an implemented speedup. The required before/after worldText input checks were also retained.

Fixed combined recovery entry: `archive/source-history@33f035ec53dd17eca8f94f9231a8d548385b40d1:research/incremental-demand-late-stage-20261113-v25-recovery/README.md`. Its3 files were actually freshly read from remote and hashed; [entry receipt](full-push-recovery-v25-entry.json). It explicitly binds V23 SDK18 and V24 dependencies; the raw V25 index4b7/closurec5 stay unchanged.
