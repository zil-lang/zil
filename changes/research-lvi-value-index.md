# Research: index the LLVM LVI cache by value for exact invalidation

The actual LLVM C API profile of opaque branch2048 recorded 1,685 of 6,940 samples in `LazyValueInfoCache::eraseValue`, all under CorrelatedValuePropagation → `Value::replaceAllUsesWith`. The pinned implementation scans every allocated block-cache slot for each deleted/replaced value. This is independent of the JumpThreading SSA-repair hotspot on the same input and the earlier scalar entry-merge hotspot. It warrants a separate, measurable candidate, not a claim that one fix solves all O2 costs.

## Scope and representation

Pin LLVM `llvmorg-23.1.4`, commit `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`. Change only `llvm/lib/Analysis/LazyValueInfo.cpp` in an isolated research toolchain. Add an internal `DenseMap<Value *, SmallDenseSet<unsigned, 2>>` to the anonymous `LazyValueInfoCache` class. Its keys are existing LLVM values; its sets contain the block numbers of cache entries that have held those values since their last erasure. This private implementation layout changes; no public header, C API, ZIL schema, optimization policy, budget, language acceptance or cache fact changes.

`addValueHandle` receives the owning basic block and inserts its number into the reverse set. The existing three registration sites cover all writes of values into persistent block caches:

1. `insertResult`, covering both lattice elements and overdefined values;
2. `insertPredecessorResults`, covering per-predecessor value entries;
3. lazy non-null initialization, registering every pointer returned by its initializer.

Registration remains idempotent through a hash set. It does not linearly scan an ever-growing vector on every insertion. The expected insertion cost is constant; erasure visits the distinct registered block numbers for that value, rather than all block slots. A value recorded in every block still pays for every block, and the reverse index increases memory and insertion work. Those tradeoffs require measurement.

Memory is proportional to distinct historical value/block pairs since the last value erasure or full clear, including stale memberships. It is not bounded by the current number of live cache entries. Clearing individual blocks does not reclaim the reverse relation immediately.

## Invariants and lifetime

Every live cached value entry must have both its existing callback handle and a reverse membership for its owner block. The reverse set may be a conservative superset of current memberships. `eraseBlock` and `threadEdgeImpl` retain their exact existing invalidation behavior and may leave stale memberships. Stale slots are safe to visit: the original cache vector only grows until `clear`, deleted block entries become null, and removing a missing value from a surviving map/set is a no-op. The existing block-number epoch contract remains in force. There is no stale `BasicBlock *` in the reverse index.

`eraseValue` looks up its reverse set, erases the value from each indexed block's lattice, overdefined, non-null and optional per-predecessor stores, then removes the reverse entry, then removes the callback handle **last**. The callback's `deleted()` method already permits its own deallocation only after all other work. Removing asserting/poisoning value handles from those stores unregisters the handles; it does not delete the underlying values or recursively mutate the reverse map. Values absent from the reverse map are still handled safely by the final handle lookup, including explicit `forgetValue` on an uncached value.

`clear` first clears block caches and their handles, then the reverse map, then the callback handles. Full clear prevents an old block number or reused value address from becoming a false live membership. The reverse index stores no ownership and never changes the lattice value returned to a client. Per-predecessor mode keeps its existing global predecessor invalidation on block removal; that operation may conservatively leave reverse memberships.

## Reproducible implementation and controls

The research directory retains the untouched upstream source, original LLVM license, commit/hash provenance, a deterministic patch generator, original/patched translation units, full diff and source manifest. The generator checks exact source hashes and replacement anchors; it refuses an existing output directory. No plugin registration, research counter, compiler option, C++ FFI extension or new global cache is added. The integrator owns isolated LLVMAnalysis single-object rebuild/relink and actual API controls; the official prefix stays intact. Both original and patched arms must rebuild the same translation unit with identical compiler/link flags.

Before performance attribution, require real API verification and native behavior for the existing branch/arithmetic and resource/closure/Drop/FFI/pointer inputs, actual loaded-library/cache identity, warm/edit/corruption/rejection behavior, and baseline versus rebuilt-original equivalence. Targeted LLVM controls should exercise lattice and overdefined entries, duplicate registration, RAUW and deletion, non-null-only pointer entries, per-predecessor mode, block invalidation, thread-edge invalidation, full clear and reinsertion. These are correctness gates; they must not be replaced by a model-only benchmark. A debug/assertion-capable cache-level control can additionally compare the reverse relation against a full scan after each mutation.

No compile or timing is performed by this proposal's source-preparation step. The two-profile receipt is `/workspace/.zil-env/research/stage6/ca79-opaque-perf-receipt.md`; official performance windows remain controlled by the integrator. Do not infer a broad speedup from the 24.28% self-sample share or attribute the remaining SSAUpdater cost to this index.

## Rust goals and decision boundary

Safety and runtime semantics require exact invalidation and unchanged verified output. Compile efficiency is the candidate benefit; runtime performance and emitted code should remain equivalent because this is cache bookkeeping. Syntax is unaffected. No Rust parity, default promotion, final compile-speed breakthrough or universal optimizer coverage is established by this implementation alone.
