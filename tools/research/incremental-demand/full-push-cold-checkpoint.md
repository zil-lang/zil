# Cold compilation group: implementation and actual results

Compiled source `022081e4361b8b00ffed0fe01fdcfb74b006bac7`, compiler SHA256 `f9b448780db18dce7781cb50dee284cc3165962b68fe102959bea7198132251c`, main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`. [Exact source, gates, full nine-scenario results and limits](full-push-cold-checkpoint.json).

## Implemented work removal

* The real FunctionEmitter now consumes events directly into final machine blocks. The old Code API uses the same state machine; complete per-function event buffering and the second event walk are removed. Temporary helper Code fragments remain. First structural errors are deferred to the original finish point, preserving budget/helper/unsupported error precedence.
* Function snapshot capture first proves complete current mapped Use sequence equality. Equal sequences need no rank-map allocations or sorting; mismatches retain the original repair and all current owner/operand/extra-use checks. The final verifier is unchanged. Each callee declaration is rendered once within the same synchronous capture. Packed frames reserve their exact checked complete length once.
* Optional final-link products are refused before discovery/full-file hashing when the configured driver lacks the bounded GNU build-ID identity fast path. This executes the original configured clang, preserving custom wrapper behavior. It does not defer hashing into startup or a later request. Admitted products retain every original identity/environment/library/script/directory/object and pre/post check. The 1MiB cumulative note-read cap applies only to the economic hint; ordinary complete installed-tool identity and its full SHA fallback are unchanged.

This last change intentionally accepts a lower link-cache hit rate when executing the real link costs less than proving optional reuse. Native IR/MIR/MC products are retained. No new switch, reduced optimization, verification bypass or budget reduction was introduced. Research remains default-off.

## One unified acceptance batch

Native/API82, backend253 and driver210 tests passed with zero failures/ignored tests at022081. Strict all-target workspace Clippy, format and release build also passed there. [Independent Rust review](full-push-cold-streaming-source-review.json), [Astra native/policy review](full-push-cold-batch-astra-source-review.json), [runtime acceptance](full-push-cold-batch-runtime-acceptance.json).

The first complete batch atc8fdce actually failed with SIGSEGV in the new use-order fixture: LLVM CloneFunction inserts its result into the source Module, so an additional owning unique_ptr left a dangling Module list entry.022081 uses the Module-owned pointer. All assertions remain; the entire batch was rerun after the fix. The original failure/partial execution and unexecuted later gates remain recorded. [Ownership addendum](full-push-cold-batch-source-addendum.json).

## Four alternating paired blocks

Both arms unprofiled, same original425 workload (20 modules ×20 functions ×16 steps), all9 requests and named parents, requestedCGU16/O2/jobs1/cacheall/capacity256. Check and native windows are separate. Source history, toolchain, cache policy and response timing boundary are fixed. Both use LLVM23.1.4 at pinned source7a941d; main loads stock DSO2bf890, candidate SDK18 DSO785a24. The SDK replacement TUs were confirmed built withO3/NDEBUG, not an accidental debug build. No Rust arm or earlier candidate arm is present in this window.

O2 medians in milliseconds; GM is the geometric mean of the four paired candidate/main ratios, not a ratio of medians.

| Request | Main | Candidate | Paired GM |
| --- | ---: | ---: | ---: |
| Cold | 5353.9 | 6238.6 | 1.119 |
| Unchanged | 458.5 | 95.6 | 0.198 |
| Private | 1649.3 | 393.7 | 0.251 |
| Multi | 1785.2 | 639.7 | 0.362 |
| Broad | 5094.7 | 6601.7 | 1.281 |
| Positions | 1656.1 | 859.4 | 0.552 |
| Error | 364.7 | 50.4 | 0.139 |
| Recovery | 1649.2 | 419.4 | 0.246 |
| Restore | 503.1 | 115.4 | 0.209 |

Cold and broad native requests are slower in all4 pairs. Check broad is slower in3/4 pairs (GM1.060), despite large check improvements elsewhere. These negative cells remain open; four samples do not establish significance or global nonregression. Older profiled windows cannot establish a controlled before/after speedup for this group.

[Compact check qualification](cold-batch-check-qualification.json) and [compact O2 qualification](cold-batch-o2-qualification.json) each cover136 rows (72 measured/64 parents). O2 independently recomputes720 native outcomes and physically checks all120 successful full ELF rows,48 LLVM blobs,288 object files,24 layouts and1920 unit references. The actual same-slot definitions and object payloads are compared, beyond merely matching requested CGU flags. Check has no native executions or artifacts. Full LLVM comparison normalizes only the unique verified-fingerprint value in its second line.

## Remaining actual cost

In the first measured native block, candidate cold link identity/discovery costs1.24ms and actual original-driver link42ms; driver full-hash counts are zero. Cold object generation remains4298ms versus main3729ms (candidate optimization1399ms, emission2435ms). Broad objects remain4599ms versus main3606ms; candidate machine preparation is570ms. These are observed scopes, not an additive complete partition or a causal attribution of all unexplained time.

Current MachineFunction restoration, MachineVerifier, AsmPrinter envelopes, MC encoding/layout/relaxation and whole-CGU work remain. The central graph still does not schedule every downstream stage. Full global push/LLVM terminal incrementality, cold/broad nonregression and a broad lead over Rust are not accepted. Main stays3ec; the unrelated Aff main serialization slice remains HOLD.

## Branches and recovery

This round actually retired31 old remote refs,42 local branches and45 old worktrees after preserving50 original tips/source snapshots and small ignored files, followed by fresh remote verification. [Fixed retirement publication](full-push-branches-retirement-04.json). Original preservation is `c52f5fc07c3346609eed2c4e9b4831afc1d93296`; final retirement result is `dd566da437329b105914a13ee24c3ad7b02e226d:research/incremental-demand-delivered-retirement-20261113-04-result`. The old b770 tree's large out/target, local branch and data remain; main/goal/current/original468/heldAff/NativeRegion remain protected.

The new author branch961f was also retired after its exact production files were proven consumed, the full unified gates passed, and its original tip/source were actually freshly recovered as real archive parents. [Its retirement receipt](full-push-streaming-branch-retirement.json). This is1 additional delivered temporary remote/local branch and worktree, distinct from the31/42/45 old entries.

[V26 fixed recovery receipt](full-push-recovery-v26.json) closes all4 raw shards and metadata after actual fresh HTTPS restore and hash/size/mode checks. Fixed INDEX `f450bfcf51f57045a8230f55e79e285ec39b4961`, closure `bc8edb16ef6086609c129befaeea1200f38daf00`. The [combined recovery entry](full-push-recovery-v26-entry.json) is pinned at `0f02d7820952cb008b34b936764148ddf8887acd:research/incremental-demand-late-stage-20261113-v26-recovery/README.md`; its files/dependency indexes were freshly read back too. The combined wrapper itself is provided, not separately claimed as an executed end-to-end restore. Raw MASTER is2,429 files/1,719,399,362B; exact union including MASTER is2,430 files/1,720,143,031B. Local tar files are working copies, not the cross-environment persistence boundary. The final fixed index and recovery entry will be recorded separately when actually complete.

[Independent matched-window review](full-push-cold-matched-independent-review.json) recomputes all18 positive and negative statistical cells from the bound rows. The full original qualification inventories remain byte-for-byte in V26 raw; the Git summaries identify their full SHA/size and omit bulky per-object lists.

Metadata01's exact-set guard rejected accidentally collected old main-cache top-level logs before index publication; failure evidence is retained,02 excludes those14 unrelated logs and passed the exact-set guard. A source-label clarification also corrects a reused oldprofile16 Git title/oldAff ELF-gap suffix: current authoritative SOURCE/manifests bind022/136+136 rows and all120 successful complete ELF files. No raw/MASTER bytes were changed.
