# Accepted disk epoch → next-edit session discontinuity

**Confirmed.** Retain's accepted whole-frontend disk hit bypasses the producers that create fine-grained session state. The next private edit misses those sessions and rebuilds current Core/Typed owners. Backend value reuse still succeeds, but its immutable-owner caches see different inputs. This is a restart/next-edit discontinuity, not evidence that clearing caches is a better general policy.

Read-only diagnosis of completed retain/clear windows, candidate `f19fc847c5774a2472bd3af85019b5c937546010`, tools `0f9d4d78ea3e5dace184dc63a3d5cfecaafd86d6`. No repo/source/window edits, builds, tests, native execution or patch application. Only this external report was written.

## Actual observations

Paths below are relative to `/workspace/zil-push-main/out/runs/native-dependency-cache-{retain,clear}-01/workflow/5105/block-00/requests/`.

| Field | Retain restart | Retain private-001 | Clear restart | Clear private-001 |
| --- | ---: | ---: | ---: | ---: |
| Disk frontend | hit | missing | missing | missing |
| Expand Lower modules executed | — | 102 | 102 | 1 |
| CT body checks / reuses | — | 5105 / 0 | 5105 / 0 | 1 / 5104 |
| Types body checks | — | 5105 | 5105 | 1 |
| Analyze product queries executed / reused | — | 5105 / 0 | 5105 / 0 | 1 / 5104 |
| Backend functions lowered / reused | 5105 / 0 | 1 / 5104 | 5105 / 0 | 1 / 5104 |
| Representation checks / reuses | 5105 / 0 | 5105 / 0 | 5105 / 0 | 1 / 5104 |
| Source-projection traversals / reuses | 5105 / 0 | 5105 / 0 | 5105 / 0 | 1 / 5104 |
| Current Verify executions | 1 | 1 | 1 | 1 |

Retain private phase milliseconds: Expand 559.873, Types 210.508, Analyze 186.638, CT 114.528, Verify 104.263, codegen 1172.097. Clear private: 35.445, 0.287, 0.112, 14.538, 40.300, 1082.830 respectively. The first four differences sum to about 1021 ms in this block. They are overlapping-request observations across separate windows, not a guaranteed removable budget.

Original request-wall medians over two blocks agree with the reported private gap: retain 2772.671 ms, clear 1196.166 ms. Restart medians are 1597.056 ms and 3798.066 ms. Median per-block **restart + next-edit compile-timer sums** are 4369.727 ms retained versus 4994.232 ms cleared. Retain already wins that pair by about 625 ms despite its worse edit latency. These sums exclude separately recorded ZIL startup/cleanup; they are not invented user end-to-end totals. Shifting 1.6 s into restart is not automatically a joint improvement.

Response SHA-256, in the table's column order:

- `94bcbfedbb83cac0608abb80b4140830b5a650d017d1dee6e7f91f2e57f1eb8c`
- `1deffa0bec1067d2c46ba6021c04a17f871417219cdce382c1dcf49b724f29e0`
- `fb0fc2e45240b7d9ba84ee8e3e9870ce1181147c1768a3a794b3fe792e1a8294`
- `bc8c3240d8c74b899ba357059971bff74293261df6e33f6d96b80c6254adc187`

## Exact consumer path

1. `zil-driver/src/project.rs:711–758` loads the whole `(TypedProgram, CandidatePlan)`, imports a Types query with an empty dependency list, constructs `ProjectTypedOwner::restored`, calls `finish_typed(CachedProgram(...))`, then immediately returns on success. `session.rs:150` deliberately gives that restored owner `epoch: None`.
2. That return precedes the project's Parse/Expand/CT/Types recording path. Thus it cannot call `capture_producer_epoch` (`project.rs:110`) or capture current materializer, raw inference and plain-source checkpoints. `TypesPushSession::apply` is only called in the later normal Types query. Note that missing Parse phase timing does **not** prove discovery did no parsing: the closed-project loader supplies real `LoadedSyntax`/sealed ParsedSource inputs at `project.rs:2299` before the disk branch.
3. `session.rs:1134` imports the cached CandidatePlan directly; it does not call `AnalysisSession::analyze_product`. `activate_types_epoch:840` cannot recover push/analysis checkpoints from `epoch: None`. The normal exact resident frontend path (`reuse_project_frontend:746`) also requires the actual Types → CT → Expand → Parse dependency chain; an imported zero-dependency Types query does not provide it. Inventing those dependency edges would be incorrect.
4. `finish_typed` still executes current independent Verify, current Lower and backend production. That populates Verify/backend state over the decoded TypedFunction owners. On the next source miss, genuine full frontend construction emits newly allocated TypedFunction/block owners. `TypesPushSession::rebuild` has no prior output anchor and allocates its new result at `zil-types/src/push.rs:88`.
5. Backend `emit/incremental.rs:569` compares full semantic function inputs and can reuse 5104 machine results. But representation checks use actual changed immutable owners (`emit/incremental/representation.rs:10`); source projection's fast path requires identical block owners (`emit/incremental.rs:526`). Hence 5104 value hits coexist with 5105 representation/scoped-thread checks and 201772 source-projection instruction visits. Clear's fresh source restart creates the fine-grained Source/Types/Analyze epochs first; its edit preserves 5104 owners and those paths reuse normally.

The initialization analyzer's 5098 low-level reuses in retained private-001 do not contradict this: its distinct candidate/product counters still show all 5105 executed. Those counters describe different consumers.

## Available data and the limit on seeding

At successful disk return the driver can retain, under the existing bounded raw owner: the exact current source bytes/module/file/path/import-resolution context and already supplied sealed parser owners; complete key, compiler/rules, target and limits; decoded TypedProgram/CandidatePlan; and their association with the just-completed current Verify/backend request. The raw decoded function owners are especially useful because the backend has just checked them. Capture that association only for a successful, uncancelled request; it is raw input/result data, not a Source→Machine authorization or a reusable Verified capability.

The existing disk format does **not** contain an ExpandSession, scalar materialization receipts, raw-inference recording, CT BodyCost/hint rows, Types push Recording/reader graph, or Analyze product admission rows. `semantic_cache.rs:83,239,409` and `semantic_items.rs:545` restore typed/plan values and interface/binding metadata. `FunctionStore::new` explicitly returns None in push mode (`semantic_cache.rs:759`).

This is a concrete obstacle to free full-session adoption: `TypesPushSession::Recording::record/finish` (`push.rs:675,715`) needs genuinely observed dependencies, six fee columns, shape/name/interface costs and successful post checks; Analyze Epoch construction (`product.rs:310`) needs its own admission/cost/initialization records; CT BodyCost requires an actual successful Walker ledger transition. Current Verify accepting a plan does not construct those independent frontend records. Do not fill them with zero costs, infer them from instruction counts, or mint them from a hash/source name/accepted plan.

Rust 58cf's `rustc_query_impl/src/execution.rs:418,459` first establishes the query's green dependencies, then loads that query's value or actually invokes its provider when absent. The useful comparison is query-granular values with an established dependency graph, not pretending one decoded terminal result contains all intermediate provider state. This does not describe Cargo Fresh or permit weaker ZIL input/Verify checks.

## Bounded practical correction

**A. Fix the result-owner discontinuity without pretending to seed receipts.** Retain one accepted decoded TypedProgram as a raw comparison anchor. In the next *actual* source Types production, compare completed per-function results against that anchor by complete equality, including IDs, spans, locals, values, blocks and headers; preserve the accepted immutable owner only when completely equal. This is result interning after current checks, not permission to skip Types. Construct Types recording against those actual retained leaves; if the complete result equals the accepted TypedProgram, the complete accepted Arc can be adopted before attaching the new real TypesEpoch. No digest authorizes equality. Non-equal functions retain their fresh current owners.

This bounded change directly addresses collateral Lower/Verify/backend owner misses even when the first source edit still runs all frontend checks. It leaves representation's exact-owner rule intact. Apply the same output-equality rule to genuinely completed Analyze output where useful; importing the cached candidate alone must not mark Analyze product rows successful. This is a concrete smaller implementation than reconstructing a compiler from terminal Typed IR, although its net timing still requires measurement.

**B. Make accepted disk epochs eligible for lazy *genuine* producer hydration.** Attach the retained exact source/parser/result tuple to the existing bounded restored raw owner, initially pending and with no successful frontend checkpoints. Take its owned handle before `prepare_project:586` evicts obsolete project snapshots for 102 modules/default capacity 32. Do not create a second unbounded history.

On first actual need for those intermediate products, run the existing producer → materializer → identity inference → plain CT → Types recording → Analyze recording on that retained source baseline, without backend/native publication. Keep the original full limits, ordered per-stage charges and cancellation boundaries; do not replenish the edited request's allowance or select its diagnostic from this maintenance work. Check the complete resulting TypedProgram and CandidatePlan against the accepted baseline, canonicalize equal typed owners as in A **before Analyze records them**, and install only complete successful existing ProducerEpoch/TypesEpoch/AnalysisEpoch checkpoints. A mismatch/unknown domain/failure discards the tentative hydration and uses the ordinary current-source path. The edited request still constructs its own current context and runs independent Verify once.

This route is implementable using current computations and data, with narrow driver orchestration/result-owner hooks. It needs no CT→Expand dependency, generic span comparator or trusted SourceMachine sidecar. In particular, it resolves the exact-owner problem by real baseline production followed by full-value owner adoption, not by reusing an old owner with different current spans. Publishing synthetic DB dependency traces or merely setting `epoch: Some(...)` is not sufficient.

**Important performance limit:** B computes missing records; it is not a demonstrated removal of their work. Account for hydration time, comparison work and retained memory explicitly. Eagerly doing it at restart or hiding it between timed requests would merely move costs unless the measured joint sequence improves. A lazy baseline replay can also cost more than today's first-edit rebuild. Implement/evaluate A first within the same whole-chain candidate; treat B as demand materialization with an explicit net-benefit question, not a promised 1.6 s saving.

If the goal is both today's cheap disk-hit restart and clear-like first-edit frontend time, current terminal-value persistence is insufficient. A later narrow extension would have to preserve **genuinely produced** reusable per-function inputs/dependencies/fee results in the existing raw cache and rebind them through each consumer's complete validators. That is additional work, not an available restore API. Do not start a generic cache framework or serialize an entire live Session to avoid this proof boundary.

## Acceptance for the complete correction

Use the original retained physical cache, new process, successful disk-hit restart → same no-position private edit → interface edit/error/recovery. Keep actual 16 CGUs, full O2/imports, current Verify and all original runtime/functional gates. Report absolute restart and edit times separately plus per-block compile-timer sums, startup/cleanup separately, and retained memory/disk. Never clear history as an implementation shortcut or run unpriced background hydration.

Check that identical completed outputs retain the decoded owners, only changed outputs replace them, backend representation/source projections consume the expected 5104 unchanged owners, and errors cannot publish partial seed state. Include wrong root/source bytes/import binding/target/full limits, corrupted or rejected candidates, cancellation, unsupported producer domain and source-position changes. Without valid hydration data, preserve the existing conservative rebuild and its real diagnostics. Reuse existing applicable gates; do not rerun unrelated frontend suites for this static report.

The strongest current finding is the confirmed missing promotion path plus a safe output-owner repair opportunity. Full microsecond Types/Analyze session restoration cannot honestly be claimed from the present two-value disk format alone. All completed f19 evidence remains unchanged.
