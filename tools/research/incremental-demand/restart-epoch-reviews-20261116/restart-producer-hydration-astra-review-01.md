# Restart producer hydration — bounded independent source review

**Core source review ready. No missing public entry route or semantic-authority blocker found.** The current implementation repairs the diagnosed terminal-query bypass by running the real current frontend and publishing its actual owners. One reporting qualification remains below. This is not a passing test receipt or performance acceptance.

Reviewed the shared `/workspace/zil-push-main` working tree over HEAD `7b604bef1c4d1f665cffc1c42c2a43b60d92c928`, including staged Types changes and unstaged driver integration. The frozen measured compiler remains `f19fc847c5774a2472bd3af85019b5c937546010`; none of its timings are new-candidate results. Read AGENTS.md, the implementation plan and both prior retained-restart/next-edit diagnoses. No local `goal` branch was available. No repository edits, Git mutations, builds, tests, formatting, native execution or cache operations were performed by this reviewer. Only this external report was written. The review does not depend on old raw-window files or assess their retirement.

## Remaining reporting qualification

`project.rs:1616` assigns `Hydrated` only after successful `finish_typed`, exact returned TypedProgram Arc adoption, and `ProjectTypedOwner::has_analysis_epoch` for the current key. The Arc adoption is justified by full TypedProgram equality in the Types hook; this is not digest equality. The helper at `session.rs:196` checks the matching Types epoch and its Analyze checkpoint.

**That predicate does not itself check the Expand owner's RawInference and PlainCT checkpoints**, although `DiskCacheStatus::Hydrated` says the producer/consumer epochs are available for subsequent edits. Those are separate optional associations (`ProducerEpoch.raw_inference` / `.plain_source`) established after actual CT production. Do not use the status alone as evidence that every upstream consumer has reusable state. Either include the existing exact-owner checkpoint associations in the status predicate, or document its narrower Typed/Analyze meaning. This is a status-contract qualification, not a demonstrated wrong-result path or a reproduced false-positive fixture. It needs no persisted-proof framework. The new test helper `epochs` explicitly checks the additional associations for its fixtures.

## Entry routes and immutable inputs

- Both filesystem disk-hit routes are covered. The import-free single-file probe now keeps `semantic_anchor` and continues ordinary parsing. A complete discovery-receipt hit keeps the pair but moves already-read dependency SourceUnits into `prefetched`, then runs the original ordered discovery/parser. It does not substitute the receipt's project graph for the newly parsed graph on this push route. Current import resolution, file/module assignment, byte/token/node limits and cancellation boundaries remain in that loop.
- `LoadedProject::Hydrating` carries genuine `LoadedSyntax` plus the raw pair (`project.rs:153,445,2561`). The pipeline borrows those parser results and sealed producer inputs; it does not parse them a second time. Canonical module hints retain filesystem-discovery provenance. `prefetched.remove` at line 2457 consumes the immutable reads rather than rereading the same files.
- `discovery_pending` now accepts Hydrating as well as Parsed (line 566). Discovery metadata publication retains the existing successful-request/current-Verify conditions; no cached Typed names manufacture source provenance.
- Direct `compile_project` requests, and filesystem paths without a discovery candidate, reach the same Store branch in `project_pipeline`. After the existing current source fingerprint/root/target checks, push mode retains only the Typed anchor and falls through (line 726). Non-push mode alone imports the terminal Types query and cached CandidatePlan. Thus fixing discovery has not left the second Store early return active for push.
- The existing resident route still requires the closed real Types→CT→Expand→Parse trace. The patch introduces no fabricated dependencies. Push remains default-off, and its query/cache domain remains distinct from non-push.

## Real production and output-owner publication

The current Parse→Expand→RawInference→PlainCT→Types→Analyze path remains the ordinary source path with the original limits and ordered allowances. Producer/materializer checkpoints, raw inference receipts, CT body/hint costs, Types readers/fees and Analyze admission records are earned by their existing computations. The decoded pair supplies none of them. The cached CandidatePlan is not used for push analysis.

`TypesPushSession::apply_with_output_anchor` (`push.rs:89`) executes the ordinary sparse or rebuilding checker first. Rebuild adoption occurs after successful checking and before `Recording::finish` (line 130). `adopt_output_anchor` (line 474) returns the whole old Arc only for full TypedProgram Eq; otherwise it replaces only same-slot, fully equal TypedFunction leaves. Derived equality includes IDs, names, spans, locals, values, blocks and all function headers, while whole-program equality also includes module/target/interface/root/task metadata. No hash or relative-position comparator selects an anchor owner.

Sparse adoption updates `context.program` to the exact returned program. Rebuild recording uses the adopted program when constructing the current Context. The ordinary `checkpoint_for` still requires the exact returned Arc. Work counters and change records continue to describe actual checks, rather than pretending owner adoption avoided their cost. Unsealed or unsupported inputs may return equal values but cannot manufacture a push checkpoint.

The driver publishes `ProjectTypedOwner::product` from that result and passes `TypedInput::Program` to `finish_typed`. Consequently Analyze calls `analyze_product` and can retain actual rows against those same function owners. Independent current Verify and the existing lowering/backend path remain live. The native object-cache option is forwarded unchanged; there is no new physical-cache clearing path. Ordinary successful semantic publication still applies.

This closes the source-level owner chain needed by the next private edit: retained Types leaves feed Analyze's immutable-function rows, backend representation's owner comparison and source projection's block-owner comparison. It does not by itself establish measured first-edit hit counts or a speedup.

## Failure, budgets and timing

Query execution retains its cancellation checks before publication (`session.rs:664`), aborts failed/cancelled queries, and the existing stage callers restore their speculative checkpoints on query failure. The new anchor hook does not run before a successful Types result or bypass budget errors. Successful earlier raw phases can still remain when a later phase rejects, as in the original pipeline; that is not a Verified capability or a synthetic complete request epoch.

The non-push cached-candidate retry remains inside the non-push branch. Push does not first finish cached native output and then compile the source again, nor replay an old source baseline before compiling the edit. The function-product retry requires legacy `function_type_work`, which the push Types branch does not produce. Existing conservative stage fallbacks remain ordinary current computations with their original accounting.

Discovery, actual production, equality comparison, Verify/native work and publication are synchronous inside the request. Equality work is inside the Types query timing; the outer filesystem request timer includes discovery and publication. There is no new idle/background hydration or deferred unpriced work. Root acceptance must report restart and first-edit separately and together, with startup/cleanup and retained memory/disk kept explicit. A faster edit alone cannot establish a better total sequence.

## Controls read, not executed

The staged Types controls cover full equality adoption after nonzero real fees/body checks, next-edit sharing, unequal function fields and whole-program metadata, sparse checkpoint rebinding, six exhausted budget columns, source errors, and unsealed input refusing a context.

The newly authored `restart_tests.rs` was still being finalized during this review. As read, its 102-module/default-capacity-32 control drops the writer session, restarts from physical semantic cache, requires real Types/Analyze execution and the real dependency/checkpoint chain, then asserts one Types body check, one PlainCT body check, one Analyze product query and 305 retained/reused functions for a same-length private edit. It also compares current Typed output/diagnostics against fresh non-push compilation, checks actual equal leaf owners, and covers error/recovery. Other controls cover namespace/signature/position/push-flag/limit changes and injected bad Typed/Candidate values, including plan-only corruption being ignored in favor of real analysis.

These are assertions in source, **not successful executions**. This fixture stops at Verify; it does not demonstrate native representation/source-projection hits. First-edit Expand/RawInference work counts, native consumers, target/cancellation/unsupported-domain and boundary-budget controls must be assessed through root's centralized applicable gates and complete retained-cache window. Do not substitute `Hydrated`, pointer-sharing assertions, zero-limit cases, or prior f19 observations for those results. No independent test window was started here.

## Rust comparison and scope

Re-read official Rust HEAD `58cf8f9c35b355199727627d07ae34a4e2846d67`, `compiler/rustc_query_impl/src/execution.rs`: green dependencies precede loading or real provider invocation (418–519), and `try_execute_query` completes the actual obtained value through `ActiveJobGuard::complete` / `cache.complete` (254–270, 86–92). It publishes the query whose value was obtained; it does not infer all upstream producer state from a downstream result. The ZIL change follows the useful current-memory-publication principle by actually producing the missing stages. Cargo Fresh is not this rustc execution path, and no stronger persisted authorization scheme is required for this repair.

## Reviewed content bindings

Paths are relative to `/workspace/zil-push-main`. These hashes identify the read snapshots, not a claim that the concurrently edited tree is frozen. Driver tests were still under authoring.

| File | SHA-256 |
| --- | --- |
| `crates/zil-driver/src/project.rs` | `75ed72992ced65d976d6a2d09b16cac18d4eb11112fb018d7b36a86ee0b925f6` |
| `crates/zil-driver/src/session.rs` | `aef0fe9ca04afae429ea6c800d64ac7cf6419697836b19930996cefc770b6aa2` |
| `crates/zil-driver/src/lib.rs` | `54967266c47293a62615bbbb638b9fcfad7d6a844cbd76e9c500870fe1bacad3` |
| `crates/zil-types/src/push.rs` | `9cf2ce6674a839df84fc7405236a6a0479c44aaabbaf7703a43f676223eada09` |
| `crates/zil-driver/src/restart_tests.rs` | `b9f500eeeaa0079f6be8e2a926201b580d1f6303f7e29aa0a6f9f3e891be5df5` |

## Final bounded delta acknowledgment — reporting qualification resolved

Re-read the updated `project.rs:1616` predicate after root's correction. **The reporting qualification above is resolved in this source snapshot.** The original finding and its original content bindings remain preserved as the initial review record.

In addition to successful `finish_typed`, whole-Typed anchor adoption and the matching Types/Analyze epoch, `Hydrated` now requires `retained_producer_epoch` at the actual Expand key, including `selection_needed || artifact_needed`. That existing helper binds the retained Expand/ProducedProgram/Core owners, current target and full limits, and every current SourceUnit against the sealed parser inputs. The new predicate then requires both recorded RawInference and PlainSource checkpoints to match `epoch.core` through their existing `checkpoint_for` methods. Missing or incompatible consumer recordings therefore cannot earn this status; the predicate creates no epochs or receipts and changes no query disposition.

No remaining blocker found in this bounded delta. This acknowledgment concerns source-level status completeness only; it does not convert the authored tests into passing receipts or establish measured first-edit reuse/performance. Root's formatting, freeze and unified acceptance remain separate. No source, Git, build, test, format or cache mutations were performed by this reviewer; only this report was appended.

Delta binding: HEAD remained `7b604bef1c4d1f665cffc1c42c2a43b60d92c928`; `crates/zil-driver/src/project.rs` SHA-256 was `824fd39ddaf7225f7f7d5f5a563691c85131c32104d91a59cb6b724125ed1898`. The re-read `session.rs` remained `aef0fe9ca04afae429ea6c800d64ac7cf6419697836b19930996cefc770b6aa2`.

## Exact committed-source completion — bcf409b37

**Independent source review complete for `bcf409b37c5e5733c57376c0b6123b76321574a9`; no new blocker found.** HEAD matched that commit and `git status --short` was empty at inspection. Read the complete five-file commit diff and the finalized, formatted 402-line `restart_tests.rs`. This supersedes the earlier “tests still under authoring” qualification for the source snapshot, without changing the distinction between authored assertions and executed results.

The committed production changes preserve the reviewed routing, real stage production, full-equality owner adoption and actual checkpoint publication. The resolved `Hydrated` predicate includes the exact Expand key/current-source/target/limit association and both RawInference/PlainSource checkpoints, in addition to the current Types/Analyze key and whole-Typed anchor adoption. Both discovery routes and Hydrating discovery-metadata publication remain covered. No additional terminal-import bypass, synthetic recording, budget substitution or native-cache policy change appeared in the committed diff. Formatting introduced no semantic change found in this review.

The meaningful finalized control detail is the injected-anchor `limits` case: it now computes the sum of the decoded functions' block counts and sets the current whole-program allowance to **N−1**, rather than merely setting it to zero. It requires rejection with no Verify run or published current Typed snapshot, and compares the delivered result against fresh non-push compilation. Each individual small body can fit while the aggregate cannot; the raw anchor therefore cannot substitute for the current aggregate ledger. This is an aggregate rejection control, not a claim that all quota boundaries have been exhaustively tested.

The completed tests also distinguish plan-only corruption from Typed corruption: a bad cached plan is discarded and genuine current Analyze may still yield Hydrated when Typed and every checkpoint match; a changed Typed leaf cannot earn whole-anchor adoption. The Types control compares signature/interface costs, names, shape, visibility, headers and post-check totals alongside CFG/context costs, and verifies that equal owner adoption leaves current work/change statistics intact. The 102-module/capacity-32 restart→private-edit/error/recovery assertions and their fresh-output comparison remain as described above.

Acceptance scope remains unchanged: these three driver controls stop at Verify, so native representation/source-projection hits and total restart-plus-edit performance still require root's complete window. The existing centralized target/cancellation/unsupported-domain and other boundary controls remain relevant. Root reported unified gates running; this reviewer neither ran them nor inspected their results and makes no passing-gate claim. No source/Git/build/test/format/cache mutations were made; only this external report was appended.

Committed content bindings (SHA-256):

| File | SHA-256 |
| --- | --- |
| `crates/zil-driver/src/project.rs` | `e67a561dbac3187f07910496005468e25e8f89ea06bcacebcd8011a48de618a2` |
| `crates/zil-driver/src/session.rs` | `aef0fe9ca04afae429ea6c800d64ac7cf6419697836b19930996cefc770b6aa2` |
| `crates/zil-driver/src/lib.rs` | `54967266c47293a62615bbbb638b9fcfad7d6a844cbd76e9c500870fe1bacad3` |
| `crates/zil-types/src/push.rs` | `501b5f3ad2a67c24d428edbc088bc8273e7afc85c388f2d90a7511c8ddf969ca` |
| `crates/zil-driver/src/restart_tests.rs` | `7b5dfb199e4a1e0cd24012256fe4cc9545436bd2eb9f05d983f106444fcd8474` |
