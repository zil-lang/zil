# Private-edit backend cost review 01

Scope: frozen f19fc847c5774a2472bd3af85019b5c937546010 lowering/transform/partition design and already saved 5105 workflow/block-00 responses. Read-only source/evidence analysis; this report is the only written file. No builds, tests, profiling runs, source changes, tool changes, or native-capture experiment.

## Conclusion

The next useful backend target is repeated function-body work inside the existing whole-transform miss, followed by repeated per-function graph-fact extraction in the current planner. I found no demonstrated quadratic loop on this input and no proven small change that removes unit 0's dominant LLVM O2/emit cost while preserving its complete current input. Do not equate the 15 small-object misses with the main remaining wait.

The highest-potential bounded proposal is to retain exact function-local transform/class facts within the existing TransformProduct, rather than rebuilding them for all 5100 candidates after one scalar product changes. The current class topology, class representative ordering, full import policy, original-model checks and native O2 pipeline must remain unchanged. This is a source-supported optimization candidate, not measured savings or an implementation-ready acceptance claim.

## Absolute evidence: prioritize eight private requests

The saved `out/runs/native-dependency-full-window-01/workflow/5105/block-00/requests/003-private-002/candidate/response.json` reproduces the supplied figures:

| Component | private-002 |
| --- | ---: |
| Total native codegen | 980.548 ms |
| Machine, including global transforms | 274.090 ms |
| Partition, excluding diagnostic accounting | 186.254 ms |
| Diagnostic rendering | 14.151 ms |
| Object phase | 483.985 ms |
| Link execution | 21.033 ms |
| Unit 0 native production | 466.311 ms |
| Unit 0 O2 / emit | 283.645 / 171.500 ms |
| Other 15 units, reported native totals combined | 10.744 ms |

Summing the eight saved private responses in that same block, without running new requests:

| Component | Eight-private total |
| --- | ---: |
| Native codegen | 8742.156 ms |
| Machine | 2278.599 ms |
| Partition | 1942.771 ms |
| Diagnostic rendering | 152.707 ms |
| Object phase | 4114.409 ms |
| Link execution | 239.169 ms |
| Unit 0 native production | 3916.417 ms |
| Other 15 units, native totals combined | 109.922 ms |

The reported machine and partition buckets total **4221.370 ms** across the eight edits. That is a cost bucket, not a guaranteed saving. These phase timings contain no split between the individual transform/planner/checker routines, so source inspection cannot assign all 4221 ms to class construction or predict the benefit of caching it.

Private-002 has 5104 lowering hits, 7146 reused partition items, 2332 constructed partition items, 16 fresh original prune checks and 7104 freshly removed definitions. Class output is 5100 eligible functions, 157 classes and 4943 rewritten calls. The 15 small objects are 480 bytes each. Units 9–15 even share the same action key within this request; avoiding their duplicate native production is real but small relative to unit 0. The evidence does not justify prioritizing that over machine/partition work.

Native selection is deliberately excluded from this proposal: root reports 201772 LIR instructions across 5103 bodies, average about 39.5, below the existing 64-work admission threshold. The prior selective-capture experiment was negative and reverted. No recommendation to lower that threshold, revive selective capture, reduce O2, shrink imports, alter grouping, or weaken Verify follows here.

## Repeated work actually present

1. **One product change invalidates the entire transform product.** `build.rs:422` obtains the scalar identity, and `:432` uses complete `same_identity` for the retained whole TransformProduct. A successful hit is sound, but the observed one-function miss necessarily re-enters checked-add folding, scalar-call folding, complete class-input preflight and `canonicalize` over the complete model. The 5104 lowering hits do not bypass those body consumers.
2. **Checked-add analysis restarts per function.** `machine_checked_add_chain.rs:562` builds declaration facts and calls `function_chains` for every function. Eligible functions build checked-block/name maps, predecessor/successor arrays, SSA-use counts and cycle information before applying their chains. This computation depends on the exact function plus actual intrinsic/trap declarations, not unrelated source bodies. Existing gates already avoid many noncandidates, and the source explicitly handles the historical quadratic PHI-suffix case; that old issue is not a new finding.
3. **Class analysis and keys are rebuilt even for unchanged function values.** `machine_call_classes.rs` reconstructs name/address inventories, per-function dependency/ABI eligibility, parent lists and the topological frontier. `normalized_key` scans calls, clears only the function name, and rewrites representative callees when necessary. `HashMap<Function, usize>` then hashes every complete key. The model's SharedVec hash walks its contents; retaining a body allocation alone does not memoize that hash. Equal key comparisons remain full Function equality. The safe current implementation is expected approximately linear in scanned body/edge volume plus ordered-set/map costs, not an established N-squared algorithm.
4. **Planner/materialization rediscover body facts.** `machine_cgu.rs` reconstructs `Function::references`, direct-call sets and costs for the current graph. `Materialization::new` separately computes direct-call sets again for every source item, despite retaining an old source module and old `calls: Vec<Arc<BTreeSet<String>>>`. `layout::action_key` also hashes the complete physical model and source grouping/call projection. Current materialization reuse skips body rewrites; it does not skip those preceding body scans, the planner, or the fresh original-unit checks.
5. **Pruning reuse intentionally requires exact original units.** All 16 originals changed in this request, so the complete original-unit cache cannot hit even if many surviving outputs are empty or almost empty. `machine_closed_prune::prune` runs original validation before a nonempty deletion. Discarding that check because the final object is empty would violate the original-input contract. Existing checker receipts only replay complete matching modules; they do not supply a current per-function proof for changed original units.

## Bounded proposal: reuse local facts, preserve current graph decisions

Use the existing raw function products and TransformProduct owner; add no generic query engine, IR or checker certificate.

**First reusable result: function-local checked-add production.** Retain the exact original function owner/value, complete actual intrinsic/trap declaration binding and the resulting Function (including an unchanged/empty-plan result). For a current equal original function under equal declaration/target inputs, reuse that function-local result before allocating and walking its chain analysis again. Keep the existing complete CURRENT original-module validation before applying any nonempty transformation; a raw function result cannot authorize the global model. A changed/unknown declaration inventory conservatively takes the existing path. This provides stable post-chain owners to the next consumer rather than reproducing all bodies and only comparing afterward.

**Next reusable result: class dependency/key facts.** Retain, per exact post-local-transform function, its direct-call/ABI dependency facts and normalized class-key owner/hash. Bind every actually read callee machine header/declaration, including missing/new targets, and the actual current callee representative-name tuple before reusing a normalized key. Retain the current global symbol/protected/address inventory and current DAG/order computation: changed addresses or roots can change admissibility; cycles remain unclassified; class splits/merges must still select the earliest current source-order representative. A changed representative invalidates affected callers even when their own bodies are unchanged. Hashes remain selectors and fresh candidates still require full Function equality. Shared previously equal normalized-key owners can avoid rehashing/rescanning thousands of equal old keys only after those current-input bindings succeed.

This is one scoped backend transform improvement: unchanged local production feeds unchanged class facts, and resulting actual machine values continue through the existing `.1` source-projection guard, complete current planner/import bindings, original-unit checks, object keys and full O2. It does not promise that the root unit becomes reusable when its actual code changes. Scalar-call evaluation has argument/fact dependencies and a shared fixed evaluation fuel; do not casually treat its results as independently replayable per function. Leave that consumer on its existing path until its ordered inputs/fuel are explicitly accounted for.

**Smaller fallback patch, lower expected impact:** exploit the existing Materialization.source/calls owners directly. Reuse the old call set for a completely equal current source item, rather than recomputing it in every `Materialization::new`. Then use that same current fact in planner Definition.calls, extending the record to exact reference/cost facts only where needed. The global current planner still runs. This saves concrete repeated body walks without changing grouping, but its absolute benefit is unmeasured and cannot honestly be sold as removing the 186 ms partition phase.

I did not find a one-line planner policy change with demonstrated high end-to-end benefit. Merely caching output-equal empty objects, weakening original pruning checks, choosing fewer imports, or changing class representatives would either target a small bucket or change an existing contract.

## Actual Rust 58cf comparison

Read from `/workspace/zil-recovery/rust-pain-rust-source-20261116`, except the retained graph file noted below:

- `compiler/rustc_query_impl/src/execution.rs:418`: green dependency lookup precedes provider execution; misses execute the provider under `with_task`. This is the relevant model for retaining a function-local consumer result and executing it only when its actual inputs change.
- `rustc_middle/src/dep_graph/graph.rs:368,791`: actual reads are recorded and equal provider results stop further propagation. This file is absent from the partial checkout and was read from the same audit's retained 58cf implementation under `rust-pain-audit-fresh-20261116-01/out/runs/rust-pain-audit-20261116-01/implementation/rust/`. ZIL should retain its stronger complete-value/current-authority rules rather than substituting Rust hashes.
- `compiler/rustc_monomorphize/src/collector.rs:354`: visited MonoItem identities prevent revisiting the same collected item. `partitioning.rs:220–281,301` separately inserts CGU-private reachable inline items into sets/maps, avoiding duplicate instances. This supports reusing actual graph facts; it is not evidence Rust merges arbitrary equal function bodies in this phase.
- `compiler/rustc_codegen_ssa/src/base.rs:735–747` still collects/partitions and ensures current CGU queries. Rust does not skip all current planning. `:1238` then requires an available work product and green CGU dependencies before native reuse.
- `compiler/rustc_codegen_llvm/src/base.rs:64–124`: the CGU task encloses module production; `ModuleLlvm::new`, declaration and definition construction happen afterward on a red CGU. Rust-style early avoidance requires complete dependencies; it cannot justify accepting an old ZIL final object for the root unit whose actual current input changed.
- `partitioning.rs:318–375` deliberately uses inline-item overlap during merging to reduce duplication. Copying that heuristic into ZIL would change its imported visibility/grouping and needs its own complete runtime-quality experiment. It is not part of this proposal.

## Acceptance scope for any subsequent implementation

Root should compare the same eight-private absolute wait, not only more cache-hit counters or the once-per-sequence interface request. Required equivalence is fresh versus cached complete transformed machine values, current partitions and native quality, including class split/merge, changed representative, changed callee ABI/missing target and changed address/protection cases. Existing Verify, original-model admission, original-unit pruning checks, current native preflight and full O2 remain. Until stage-level receipts/timings exist, the proposal's speedup and overhead on unique-body inputs remain unknown. This report neither authorizes implementation nor claims new measured improvement.
