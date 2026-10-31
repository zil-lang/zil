# Reactive DB attribution after the stage 6 Session ablation

This is a static attribution of the completed experiments, not another compiler
change or benchmark. Recommendation: restore the complete 3d6 DB implementation
for the default compiler. The reverse-index candidate is correct on the scoped
controls, but the isolated real-source window does not establish a net speed
benefit. Keep the separately implemented function-initialization projection and
the evidence of this unsuccessful DB candidate. Stable semantic owners require
a different query boundary; deleting source hashes is not a safe small fix.

## Measured identities and reproducible event evidence

The isolated window is EE `ee786e7fd7162cfd76d5f532fe5c80d9e175ad57` versus
that same production snapshot with only `crates/zil-db/src/lib.rs` restored to
`3d6babe17ee9d0a2f62d123063fb7af6a0886612`. The initialization projection is
enabled in both arms. Its 840 requests are in
`/workspace/.zil-env/research/stage6/session-db-isolation/rows.jsonl`.
The separate `session-scalar-vs-3d6` window measures the ca79 scalar-promotion
combination and cannot isolate the projection's speed contribution.

Static reader and output:
`/workspace/.zil-env/research/stage6/reactive-key-attribution/{collect.py,counts.json}`.
The output records SHA-256 of both windows' rows, summaries, manifests and
execution receipts, checks every event pair and every source-identity sequence,
and contains every case/mode/request/arm event count. It runs no compiler.
`counts.json` SHA-256 is
`357e47d92532df06b5e584ff2dca6695f9fc8ad4e08c74b493cd72d98ca5c737`.

In **each** window, all 420 paired requests have identical complete event
sequences in the two arms. Counts below are **per arm**, for 420 requests:

| Query event | Executed | Reused |
| --- | ---: | ---: |
| Parse | 76,332 | 24,888 |
| Expand | 300 | 120 |
| Ct | 300 | 120 |
| Types | 300 | 120 |
| Analyze outer query | 300 | 120 |
| Verify | 420 | 0 |
| Lower | 210 | 0 |
| Link | 210 | 0 |

For each private edit, identical in check/O0 and all six blocks:

| Distribution | Parse executed | Parse reused | Expand/Ct/Types/Analyze |
| --- | ---: | ---: | --- |
| One module, 512 functions | 1 | 0 | Each executes once |
| One module, 2,048 functions | 1 | 0 | Each executes once |
| 64 child modules × 16 functions | 37 | 28 | Each executes once |
| 512 child modules × 8 functions | 485 | 28 | Each executes once |
| 1,024 child modules × 4 functions | 997 | 28 | Each executes once |
| 64 branch-heavy child modules | 37 | 28 | Each executes once |
| 16 body-heavy child modules | 1 | 16 | Each executes once |

The root module adds one file to each multi-module case. Public edits have the
same outer counts. Restoring the original source reuses all four semantic stages
in the two single-module cases and 16-child-module case; the larger cases execute
them again. All 168 per-window source sequences satisfy
`cold.input == unchanged.input == restore.input`, with cold/private/public
identities mutually distinct.

These are actual QueryEvents, not an inference from `cache_hit`. They do **not**
instrument `Database::invalidate`, count its removed consumers, or expose
initialization-subquery counts. A replayed Parse receipt is labeled Reused even
when its raw syntax has been evicted. An Executed outer Analyze may reuse local
initialization projections internally. Neither qualification changes the
observed fact that private edits rerun the whole-project semantic queries.

## Exact input-to-key chain

Line references in this section refer to the immutable EE snapshot. An exact
local copy of its driver files is under
`/workspace/.zil-env/research/stage6-old-db-ablation/source/`; only its DB lib
differs from EE. There is no `InputId` type in the active driver/DB: the relevant
field is `QueryKey.input: Fingerprint`, not a stable input owner ID.

1. `zil-base/src/lib.rs:74` hashes all `SourceUnit.text` bytes.
2. `zil-driver/src/session.rs:781` constructs `query_key_with_limits`: source
   input is `H(domain, FileId, ModuleId, path length, path bytes, H(source bytes))`.
   Rules bind RULE_VERSION, SCHEMA_VERSION, package version, raw-session domain,
   all 11 limits, and `build_inputs`. Target-dependent stages also bind the
   target fingerprint. These are fields of the actual BTreeMap key, not merely
   observations attached to a stable node.
3. `session.rs:827` makes Parse's token/node quotas neutral in its key.
   `parse_key` checks `ParseResult::fits_limits`; insufficient successful receipts
   use the quota-specific key and original parser. Every other key axis remains.
   This is a proved syntax receipt optimization, not general permission to omit
   limits or source positions.
4. `zil-driver/src/project.rs:664` hashes the root ModuleId, ordered module list
   (each ModuleId, FileId, path and full source fingerprint), and the complete
   logical-path-to-ModuleId map. `project_key` at line 653 obtains the normal
   rules/module/target key and **replaces** its `input` with this full-project
   fingerprint. It does not retain only the root's source hash.
5. `project.rs:156` computes this fingerprint and constructs the project identity.
   `session.rs:837` changes only phase/target in `phase_key`. Expand
   (`project.rs:253`), Ct (322), and Types (393) consequently inherit the same
   complete project input. Analyze clones the Types key and changes only phase
   (`session.rs:593`). A literal edit in one private body changes all four keys.
6. `zil-db/src/lib.rs:63` stores module/phase/input/rules/target in QueryKey, whose
   fingerprint includes all fields. `session.rs:84` constructs most raw result
   digests from that complete key and the ordered upstream construction
   identities. Project, ProjectCore, ProjectTyped and Candidate are construction
   identities, not interface-content equivalence checks. Single-source Typed
   uses `TypedModule::fingerprint()` instead.
7. `session.rs:304` sets source_positions to key.input; line 311 creates every
   current driver dependency as `PrivateImplementation`. Semantic and private
   result fields both receive the same digest. The enum supports finer kinds,
   but the driver does not currently use separate public-contract dependencies.

## Why maintaining reverse edges does not produce semantic incrementality here

`Database::publish` invokes invalidation at EE DB line 264. The new index starts
from consumers of the **same QueryKey**, comparing the recorded fingerprint kind
at lines 292–304. It cannot join old and new source-content keys as the same
owner. The driver does not separately call `invalidate` when source changes.

The main edited-source path therefore publishes new whole-project keys. For an
unchanged request, `query` returns the existing raw result before executing any
producer. Republishing an evicted raw value under an otherwise identical
deterministic construction does not provide a changed semantic fingerprint
either. Exact invalidation is useful for the DB's supported mutable-owner API,
but that API is not how the current driver's source identities evolve. We have
not measured an invalidated-consumer count and do not claim a measured zero.

The 32-slot raw history makes the distinction especially visible. EE
`session.rs:202` drops obsolete whole-project owners before an edited large
project is constructed (`modules + 4 > capacity`), then keeps only metadata
reachable from remaining raw roots. `trim_history` at line 235 keeps the newest
32 raw values: the four project stages plus 28 Parse values after these cases.
Hence the 1,025-file private edit reparses 997 files and reuses 28. Stable reverse
edges cannot recreate the discarded syntax. Unchanged requests use
`reuse_project_frontend` at line 334 to replay prior dependency receipts without
rebuilding evicted Parse values; its many Reused events must not be sold as newly
implemented fine-grained checking.

The index does avoid old DB scans and permits fast reads under its maintained
closed-graph invariant. Those are real metadata improvements, with correct
cycle/rollback controls. However, it also allocates a second BTreeMap/BTreeSet
edge representation, including full producer/consumer query identities and
observed fingerprints, and maintains it during publication/collection. The
forward closure retains metadata for all files even when raw capacity is 32.
The experiments measure the net of these costs; they do not demonstrate that
reverse indexing removed a material amount of compiler semantic work.

## Stable identities versus required validity evidence

| Candidate stable owner | Evidence that must remain separate and checked |
| --- | --- |
| Source slot: package/project namespace + canonical/logical file binding | Full current source bytes/hash; FileId/ModuleId mapping and path when cached spans or names contain them; discovery/import mapping changes |
| Parse: source owner + phase + parser/rule context | Current source observation, exact span identity, successful cost receipt/current remaining budgets; changed source must invalidate before `query` can return cached raw data |
| Whole-project phase slot: project owner + phase + rule/target context | Complete current graph/source validity still required until finer queries exist; this changes bookkeeping, not the amount of semantic recomputation |
| Declaration/type/contract query: stable package/module/declaration binding | Declaration shape, selected configuration, namespace/negative lookup facts, actual public contracts/layouts consumed, nominal identity and recursion/SCC context |
| Function/instance query: stable declaration plus actual type/const/callable/selection arguments | Body or relevant semantic projection, captured environment, nominal identities, actual CT/inline/instantiation implementation dependencies, target/layout and rules |
| Current initialization projection (`zil-analyze/src/incremental.rs:103`) | Exact local CFG, local/parameter IDs and init operations; original logical cost; no cached span/diagnostic/authority; all other analyses and Verify stay current |

Path+phase alone is unsafe today. FileId and ModuleId are assigned from discovery
order (`project.rs` current lines 946–947), so an inserted import can change IDs
without changing a remaining file's text. Either retain that mapping in validity
or explicitly rebind/revalidate all cached spans and references. Do not conflate
compact function IDs with stable declarations or instantiated function owners.
Current whole-source keys conservatively cover generic and CT inputs; any finer
replacement must capture their actual dependencies, including lookup absence.

Likewise, rules, build inputs, target and exact budget behavior must remain bound
as context or observations. Source/body hashes may move **out of owner identity**
but cannot disappear from validity. The outer raw caches contain full semantic
objects and source positions; only the proved initialization projection can
ignore unrelated constants and positions. Persistent candidates additionally
bind the installed compiler and must survive current Analyze and independent
Verify; an in-memory stable-owner experiment must not weaken that boundary.

## Why the minimal key edit is not an appropriate rescue

Current `CompilationSession::query` checks `database.read(key)` and `raw[key]`
and immediately returns them; it does not compare the caller's new dependencies
or source observations on a hit. Merely removing full-source input from the key
would serve stale raw state. A mechanically safe migration needs explicit stable
source slots, updates before any dependent read, transactional batch publication,
and exact context validation. Invalidating the whole downstream closure would
still run Expand/Ct/Types/Analyze on every body edit. It would also replace prior
epochs and could lose today's cheap small-project restoration unless a separate
content-value cache is retained.

Even after that migration, current construction fingerprints incorporate changed
inputs rather than true interface projections, and dependencies all observe full
private implementations. A useful equality barrier requires splitting declaration
shape/contracts from bodies and recording what each consumer actually read.
That includes the source/ID/limits/CT/lookup counterexamples above, not just a
renamed map key. The finite, conservatively testable first slice already landed
is initialization projection; it crosses body/FunctionId changes only because
its smaller pure input and exact cost behavior are explicitly defined.

## Disposition and limits of the performance conclusion

For the isolated EE/oldDB window, 37/70 timing cells are adverse. Equal-weight
cell geometric means are check **1.009864** and O0 **0.998980** (candidate/oldDB).
This supports **no demonstrated net acceleration**, not a statistically proved
slowdown in every workload. Do not reinterpret the ca79 combination's O0 gains
as gains from this DB: scalar promotion also changed that window.

RSS is adverse in 52/70 cells, with median cell difference +130 KiB. Be precise:
check RSS GM is 1.003991, O0 0.991206, and the full 70-cell GM is 0.997578 because
five large-2048/O0 cells decrease about 16–20 MiB. Current VmRSS is not peak
memory, heap allocation attribution or a guarantee that every case regressed.
The distribution gives no basis to overlook the added machinery or to claim a
universal memory regression.

Restore the complete tested old DB implementation rather than selectively
retaining unmeasured fragments of the new implementation. Keep the failed
candidate, exact-cycle tests and formal ablation as research evidence. A future
owner/query-boundary change should first prove reduced actual semantic work on
the existing distributions and controls, then remeasure total latency and memory.
If the default is restored, the final binary is a new identity: existing ca79
combination timings must remain labeled as that candidate, not silently relabeled
as final-default measurements. A rollback needs scoped correctness checks, but
this report requests no additional timing matrix.

## CI closure of the rollback

Run 41206406105 on ac4 exposed that ccf1 restored lib.rs but left the experimental private test module referencing removed reverse-index types and fields. Restore tests.rs exactly from the same 3d6 baseline; the experimental implementation and its tests remain in ee786e7fd and archived research evidence. This repairs the test/implementation pairing, without disabling surviving tests or changing runtime DB behavior. Preserve the failed CI log and recheck the DB test target, both feature modes, and all-target Clippy before rerunning CI.
