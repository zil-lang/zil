# Private-body edit to executable: source audit

Source-only audit on 2026-11-12, baseline checkout `46870205` in
`research/incremental-demand-continuation`. No compiler build, benchmark, native
execution or production edit was performed by this audit. Line references below
name this pre-change source; the continuation implementation can move them.
Read `AGENTS.md`, `.task/point-to-point-handoff.md`,
`.task/incremental-demand.md`, and fixed goal/workflow source
`ee7fcd1a229799f1146c4dbdee822d6c9f8ff269:{goal.md,workflow.md}`.
The named local `goal` ref is absent; the fetched fixed object supplies the
authoritative workflow without blocking the audit.

## Zil path and consumer work

The graph has useful local reuse, but does not currently propagate values from
source through Types, Analyze, Verify, Lower, CGU and Link as one demand graph.
A graph equality statistic alone does not demonstrate avoided consumer work.

| Stage | Identity/dependencies | Private body edit and invalidation/recompute | Equal-result stop, aggregation and remaining consumer work |
| --- | --- | --- | --- |
| Discovery / Parse | Current paths, module/file IDs, source bytes, target-independent syntax/rule/limit identities. `session.rs:215-333`, `1106-1162`; project inventory `project.rs:450-495`. | Unchanged individual source Parse values/receipts can be retained. The changed file gets a new source identity. Project fingerprint includes ordered full source fingerprints and path/module inventory (`project.rs:1622-1644`). | The project fingerprint changes even when the public interface does not. Discovery, current path/output alias checks and inventory admission remain. |
| Expand / source materialization | Whole project phase key; producer entries additionally bind parsed revision and complete environment of source layout, headers, imports, limits, target and CT context (`expand/incremental.rs:250-278`, `404-439`). | In eligible sealed scalar domain, unchanged module normalized/restored/lowered results and source maps can share ownership. A changed module gets a new `ModuleRevision`; incompatible environment clears receipts and all producer phase values. | `sources = parsed.iter().map(|p| p.source().clone())` still clones every source text; `cloned_source_bytes` records it (`443-461`). Complete environment and module inventory are reassembled before downstream reuse. Sharing Core/SourceMap is real, but no claim of zero source copies is justified. |
| CT / inference preparation | Project CT query depends on project Expand snapshot (`project.rs:1060-1155`). | Project fingerprint change reenters query. Existing produced/shared inference and unchanged-source CT validation avoid some evaluation; otherwise a complete program context and per-module evaluation are constructed with current budget charging. | This is a whole-program preparation result, not a function CT demand graph. Current entry contract and CT environment/limits remain. |
| Types body-key projection | `FunctionOwner {project,module,function}` is a semantic owner; it is separate from compact IDs and revision evidence. Inputs are complete Context fingerprint, Signature fingerprints and RawBody version (`types/incremental.rs:100-147`, `757-1017`). | Unchanged producer `ModuleRevision` lets a body keep RawBody version. A new producer revision marks every worthwhile body in that module for a new body projection, even if sibling functions are unchanged (`832-863`). Without producer revisions it compares prior/current Core functions. | `ReactiveGraph` compares result equality and notifies only for changes (`db/reactive.rs:426-434`), but **only BodyKey is evaluated here**, not CFG/Analyze/Verify/Lower/Object/Link. The evaluator reads Context, RawBody, own Signature and observed signatures. A same BodyKey does not avoid whole-program Types traversal. |
| Types CFG and program assembly | Full body keys bind current declarations, IDs, signatures, target and limits. Cache contains raw scalar TypedFunction plus six budget counters (`types/incremental.rs:168-183`, `1113-1315`). | Keys table is fully rebuilt/collected (`995-1016`), and `program::check_resolved_program_with_reuse` walks current program. A hit rechecks current lifetime header, replays current accounting, deep-clones cached TypedFunction and rebases spans (`514-576`). A miss compiles and clones/rebases a raw storage entry (`579-630`). | Local key equality can preserve a cached CFG entry, but there is no value-based stop before whole TypedProgram assembly. Global shape/context/signature work, inventory, interfaces and current admission remain. Trivial leaves intentionally do not persist; richer unsupported cases fall back. |
| Analyze | Driver analysis key has same full request identity as Types and observes PrivateImplementation (`session.rs:835-890`). Initialization subcache projects function semantics and stores cost + result (`analyze/incremental.rs:50-125`). | Reenters whole Analyze on project change. Matching initialization input reuses result, charges the prior semantic work cost and clones initialization result; other analyses and task interfaces run afresh. | Local initialization reuse is narrower than whole Analyze reuse. No downstream value graph edge suppresses Verify or Lower. |
| Verify | Fresh current Typed/Candidate values, target and limits; no retained Verified variant (`session.rs:20-76`, `897-910`). | Every successful request reaching Verify calls current `typed.verify`; even exact semantic/function cache restoration grants no authority. | Full current independent Verify is an explicit authority boundary. Eliminating it is outside this slice and must not be used to claim parity or speedup. |
| Lower / machine construction | Current Verified and borrowed LIR view (`session.rs:929-938`; `lower/lib.rs`); backend binds current verification. | `lower_view` borrows functions, and backend binding has pointer equality fast paths. Backend still builds current whole machine representation and derives ABI (`backend-llvm/build.rs:251-272`). | Do not invent a deep Typed-to-LIR copy. Whole machine construction, selected transforms, optional reachability/stable naming and complete diagnostic LLVM `machine.to_text()` still precede partition/object lookup (`build.rs:273-340`). |
| Partition / object | Current machine module bodies + linkage + globals + target; cache namespace binds installed compiler/backend/pipeline identity (`object_cache.rs:139-203`, `300-313`). | Partition/layout can be reused conservatively, but current partition machine inputs are assembled. Changed units miss; unaffected units can hit. Actual partition membership, imports and transformations determine the affected set, not requested CGU count alone. | Object hit avoids native LLVM Builder/optimization/emission, but still hashes module, reads/validates object bytes, runs current `zil_llvm_api::validate_module`, and writes the object to the new build directory (`build.rs:780-811`). |
| Link / publication | Selected entry, current native objects/target/linker; guarded artifact destination. | Native final link still executes (`build.rs:562-584`). Driver reads full diagnostic LLVM text back even before optional dump, validates output plan, publishes executable and commits pending object cache (`session.rs:1047-1083`). | Link is not a cached demand result here. Successful publication includes current cancellation and path/alias guards; optional cache failures cannot revoke successful compile. |

Driver raw-query fingerprinting is deliberately conservative:
`RawSnapshot::digest` (`session.rs:96-119`) generally hashes tag, full QueryKey,
and ordered dependency key/private fingerprints; TypedModule alone uses its
content fingerprint. Query publication assigns the same digest to semantic and
private implementation and observes every dependency as PrivateImplementation
(`462-488`). Thus changed source construction can propagate through phase keys
even if a phase's public output is equal. The database's support for different
fingerprint kinds is not proof that this driver exploits semantic/ABI separation.

## Persistent function and semantic publication

The expensive function publication is separate from CFG reuse:

1. `project.rs:1271-1285` invokes exact semantic candidate and function epoch
   publication after successful `finish_typed` when trusted semantic caching is
   enabled. Even an optional write entails work before the filesystem.
2. `FunctionStore` (`semantic_cache.rs:668-727`) has one stable previous Types
   epoch under `function-types-v1`; root identity is stabilized but the enclosing
   Store key still binds installed compiler, rules, target and limits. Restore
   only runs when session is empty and directory admission succeeds.
3. `FunctionSession::encode_work_products` (`types/incremental.rs:338-385`)
   writes `ZILFUN01`, entry count and 80-byte metadata rows (32-byte key + six
   u64 costs). It then **deep-clones every cached TypedFunction** into a temporary
   TypedModule. It constructs an empty CandidatePlan and serializes all functions
   once with `wire::encode_module_candidate`. The resulting wire Vec is copied
   into the outer bytes Vec. This is a full successful epoch serialization, not
   a delta. No claim of double-pass serialization is intended.
4. `wire.rs:204-217`, `949-1024` uses one Writer with bounded bytes/nodes and
   existing per-field limits. The writer only borrows function input; the deep
   clone is required by the caller's temporary owning module, not the format.
5. FunctionStore rechecks cancellation/artifact plan and private directory,
   then ArtifactCache hashes the complete payload, writes a temp file header and
   entire payload, `sync_all`, checks cancellation and renames over final path
   (`db/cache.rs:128-174`). A successful identical write is not proactively
   skipped; equality fallback only occurs if rename fails.
6. Decode (`types/incremental.rs:409-467`) checks count and wire limits, target,
   raw scalar eligibility, relative zero file/start spans, costs, duplicate keys
   and empty module/candidate auxiliary inventories. Restored data is still raw.
7. Exact semantic Store (`semantic_cache.rs:73-99`, `198` onward) serializes
   complete TypedProgram module/Candidate plus interface metadata under the exact
   project key. It already borrows the TypedProgram, so the same deep-clone
   removal does not directly remove its full encoding/hash/write costs.

The smallest useful low-risk slice is a borrowed raw-function wire entry point:
reuse the existing Writer/header/module field order/function encoder/empty plan,
accept an exact-size iterator of `&TypedFunction`, and have `encode_work_products`
pass entry functions by reference in existing BTreeMap order. Preserve `ZILFUN01`
and exact legacy wire bytes, shared cumulative node/byte limits, max_functions,
per-field limits, target and decoder. Keep existing successful completion,
cancellation, alias guards and fresh Verify. This removes a full cache-sized
TypedFunction allocation/copy without introducing a new trust or identity policy.

It does **not** remove full encoding, the wire-to-outer-buffer copy, full payload
hash, write/fsync, CFG hit clone/rebase, or global compiler work. A follow-up that
avoids encoding unchanged entries requires per-entry serialized memo ownership
and aggregate-budget semantics; streaming directly into a file changes failure
and cancellation cleanup. Those are broader slices. Caching whole bytes by
`Arc::ptr_eq(entries)` helps exact/no-op epochs only, because each successful
Types reconstruction installs `Arc::new(next)` even if most entries are reused.

Useful validation is wire byte equivalence against the old owning-module path,
roundtrip through the existing decoder, exact-limit/one-below limits, unsupported
nested claims and malformed input rejection, plus existing driver semantic cache
and independent Verify regressions. Wall-speed attribution must measure actual
`function_cache_publication` in a separate profiling run and then use frozen
unprofiled binaries; copy removal alone does not establish a wall gain.

## Official Rust/Cargo fixed-source comparison

All links below were fetched over inherited proxy/TLS with certificate checking.
Pin Rust `58cf8f9c35b355199727627d07ae34a4e2846d67` (rustc 1.99.1), Cargo
`5f94df4789f005f9a352888e8355ffc645b7ed0e`. These pins exist on official GitHub.
Earlier guessed `rustc_query_system/...` and `src/cargo/core/compiler/...` paths
returned 404 because these versions have moved code. The following are the
actual pinned paths, not references to a moving branch or an older guide.

| Mechanism | Official pinned implementation and implication |
| --- | --- |
| Stable node identity | [DepNodeKey](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/dep_graph/dep_node_key.rs#L71-L104) uses DefPathHash for DefId/LocalDefId; recovery resolves it in current tcx. Semantic query ownership is distinct from transient IDs and from validity evidence. Stable names alone do not authorize reuse. |
| Query result hashing / red-green | [alloc_and_color_node](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/dep_graph/graph.rs#L791-L834) compares reexecuted value fingerprint with previous fingerprint and marks equal output green. `no_hash` queries are conservatively red. [try_mark_previous_green](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/dep_graph/graph.rs#L943-L1036) traverses old dependency targets, forces queries where needed and can continue when the forced result is green. Thus an input edit need not propagate beyond equal query output. |
| Query value persistence | [encode_query_values](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_query_impl/src/incremental.rs#L22-L43) traverses CACHE_ON_DISK query caches and passes borrowed selected values to encoding. [OnDiskCache::serialize](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/query/on_disk_cache.rs#L204-L324) emits values, side effects, allocation/hygiene data and footer indexes to FileEncoder. It is not zero persistence cost. |
| Persistence remaining whole-cache work | [save_dep_graph](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/save.rs#L62-L91) promotes every green node with an old disk-cached value into memory so it can be serialized for the new session. Its FIXME explicitly says this does substantial work in few/no-change builds and asks whether promotion without decoding is possible. Do not portray Rust as writing only changed result records. |
| Loaded-result integrity checking | [should_verify_loaded_value](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_query_impl/src/incremental.rs#L82-L104) states full rehash is expensive; default subset rotates over 32 sessions, with optional full incremental-verify-ich. This is a different assurance model from Zil's mandatory current Verify, not permission to remove Zil Verify. |
| CGU work product reuse | [determine_cgu_reuse](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_ssa/src/base.rs#L1238-L1284) requires enabled dep graph and an existing work product before marking current CGU dep node green; LTO decides pre/post-LTO reuse. [codegen_crate](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_ssa/src/base.rs#L733-L787) still requests complete mono collection/partitioning before calculating CGU reuse. |
| Affected-CGU granularity | [partitioning contract](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_monomorphize/src/partitioning.rs#L4-L64) describes complete mono set, opaque LLVM modules rebuilt from scratch when affected, body/signature/reference effects and inlined copies. Same requested CGU16 does not guarantee same actual partitions/imports as Zil. Rust is not generally changed-function-only native compilation. |
| Work product IO | [copy_cgu_workproduct_to_incr_comp_cache_dir](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/work_product.rs#L17-L55) links/copies CGU artifacts and records saved files; known links can avoid repeated copying. This is separate from frontend query persistence and final linking. |
| Rust session identity | [session publication and GC](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/fs.rs#L293-L350) renames `-working` to finalized. [GC](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/fs.rs#L701-L860) skips current session and preserves the newest older candidate, so multiple finalized directories are normal. Current-request CGU inventory must distinguish the newly finalized session, or prior current session on a Cargo-fresh request, from historical artifacts; count of every `.o` in the target tree is not current CGU count. |
| Cargo freshness | [fingerprint module](https://github.com/rust-lang/cargo/blob/5f94df4789f005f9a352888e8355ffc645b7ed0e/src/compiler/fingerprint/mod.rs#L1-L42) determines whole Unit fresh/dirty from stored unit fields, dependency fingerprints, source/dep-info mtimes and output/dependency mtimes. `checksum-freshness` is still unstable/nightly opt-in. Cargo can skip invoking rustc for a fresh Unit; after a source edit dirty Unit invocation and rustc's internal graph are distinct layers. [mtime handling](https://github.com/rust-lang/cargo/blob/5f94df4789f005f9a352888e8355ffc645b7ed0e/src/compiler/fingerprint/mod.rs#L220-L256) rewinds dep-info reference time to invocation start to catch concurrent edits; mtimes have documented edge cases. |

The evidence supports implementing borrowed persistence as a bounded cost
reduction and separately designing semantic projections for true downstream
demand propagation. It supports neither a Rust parity claim nor an isolated
speed ratio. The prescribed matched, sequential O2/jobs1/CGU16 window must retain
actual work-product inventory, source/binary hashes, exact native outcome and
negative diagnostics, with cache history and assurance differences explicit.

## Continuation slice under review

The selected continuation goes beyond removing the temporary TypedFunction
clones: each immutable `Entry` owns one `OnceLock<RawFunctionEncoding>`, retaining
one successful function fragment under one complete `Limits` value. Publication
borrows functions; matching fragments replay their bytes and exact node charge.
If limits differ or remaining aggregate bytes/nodes do not fit a fragment, the
ordinary function encoder runs, preserving the original first failure. Outer
metadata, module/candidate wire format, full-payload hashing/write/fsync and
current independent Verify remain.

A shared `Arc<Mutex<()>>` in FunctionSession serializes publication across clones
and checkpoints. Current-map admission subtracts already retained fragment bytes
before storing another fragment, closing concurrent same-map admission races.
This is **not a strict global or historical byte bound**: an old live checkpoint
can share an unfilled Entry slot with a later epoch; filling that slot later can
raise the old checkpoint's retained fragment sum beyond its earlier admission
cap. One Entry retains at most one fragment and one Limits version, no internal
unbounded history container is added, and the driver keeps a transaction
checkpoint for the current request. Holding arbitrary public snapshots retains
their data. Neither long-session leak freedom nor a whole-lineage
`max_source_bytes` bound is established. Strict Wire execution quotas remain
independent and unchanged. The root explicitly accepted this limit rather than
expanding this slice to a shared lifetime budget registry.

`function_cache_encoding` reports actual full function traversals and fragment
replays only when cost profiling is requested. It describes the final attempt,
not successful disk publication or verification authority. Unprofiled wall runs
must retain `null` and omit profiling switches. Cold publication still traverses
all functions and additionally copies fragment bytes for retention; warm/edited
epochs can avoid traversal of shared Entry bodies, but full-epoch output copying,
hashing and filesystem costs remain. No measured speed or memory gain follows
from this source review alone.
