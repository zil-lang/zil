# Research: batch SSA repair only at a consistent JumpThreading edge

Actual API profiling of the opaque 2048-function branch distribution identifies
`JumpThreading::threadEdge -> updateSSA -> SSAUpdater::RewriteUse ->
SSAUpdaterImpl::BuildBlockList` as a separate hotspot from entry-block dominator
recalculation. The existing entry-preserving candidate does not address this
path. The intended experiment keeps the full O2 pipeline, duplication policy,
language Verify, machine preflight, and native behavior.

Pin LLVM 23.1.4 commit `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`.
`SSAUpdaterBulk` already shares predecessor caches across variables and uses
the current dominator tree for iterated dominance-frontier placement. Use its
`RewriteAndOptimizeAllUses` entry, including inserted-PHI simplification and
deduplication; do not merely insert extra PHIs and leave optimization weakened.
Its liveness walk is still per variable, so speedup is an experimental question.

Do not replace all `updateSSA` calls. In
`duplicateCondBranchOnPHIIntoPred`, SSA repair occurs before deleting the old
terminator and before publishing the edge updates; asking for a current DT there
would consume a temporarily inconsistent CFG. The candidate is limited to
`threadEdge`, after its three edge updates have been queued and after source-atom
remapping. Flush through the existing `DomTreeUpdater::getDomTree` only when an
actual nonlocal use needs repair. Other callers retain the original algorithm.

Before editing uses, collect the complete rewrite set. If a defining instruction
has external debug records or lifetime-marker uses, keep the complete original
update path so its debug kill-location and whole-lifetime-marker cleanup rules
remain exact. Preserve the original rule excluding normal same-block uses and
PHI uses whose incoming edge is the defining block. For accepted candidates,
register both the original and cloned available values and the exact nonlocal
uses, then rewrite the batch. No class/header layout, new public ABI, ownership
permission, default budget, or source acceptance change is proposed.

Produce standalone original and candidate sources, unified patch, fixed-source
hashes, and an isolated one-object LLVM DSO; never overwrite the installed
prefix or the already-frozen entry candidate. First verify small CFG/PHI/loop,
debug/lifetime fallbacks, and current true-source/native cases. Compare actual
opaque sources and preserve optimized IR/object changes and negative runtime or
compile results. Present current functional timings only as diagnostic until
the parent grants an exclusive measurement window. The candidate is not a
claimed universal compiler or Rust-level win before those checks.

The first bulk candidate preserves the per-variable backwards live-region walk.
The next candidate therefore shares one unpruned iterated dominance frontier
for the two identical defining blocks across the whole batch, then obtains
reaching values through the existing dominator tree. It caches by block using
block keys (not pointers into a growing DenseMap), handles PHI operands at the
end of their incoming predecessor, and preserves poison on unreachable paths.
It uses ordinary `Use::set`, matching the original SSAUpdater rather than
announcing a whole-value RAUW for one rewritten use. Inserted PHIs are simplified
and deduplicated using the existing utilities. Before editing IR it falls back
if the unpruned frontier would insert more PHIs than registered nonlocal uses;
this bounds speculative PHI growth and keeps the original algorithm available.
This is a separate epoch; original bulk results and the first missing-clang
link failure remain unchanged.

The unpruned v2 guard rejected all 505 helper calls on the 512-branch control
(18,781 variables/uses versus 55,823 potential PHIs), and the full 2048 case did
not improve. A separate demand-selected epoch keeps the same growth cap: shared
DT stations distinguish live-out definitions from live-in frontier PHIs; real
uses select roots, and only those PHIs' predecessor stations enter a marked
closure. Complete the plan and cap check before changing IR, create all needed
placeholder PHIs before filling cyclic edges, preserve repeated predecessor
edges and PHI incoming-edge use positions, then simplify/deduplicate as before.
Unneeded trap/exit/loop-header frontiers are not materialized just because they
appear in the unpruned IDF. Earlier epochs and negative measurements stay intact.

The demand-selected v3 also rejected all 505 helper calls. Its first real
rejected plan explains why: one use at `zil.fn.1.exit` needs a PHI there and
another PHI at `bb.1.i6`, joining the old/new definitions before the final use.
The bound rejected necessary SSA, not only speculative PHIs. Keep both negative
epochs; do not relax that bound and claim an unmeasured win.

The promoted-for-review v4 instead preserves the first candidate's original
`SSAUpdaterBulk` behavior and caches only its existing structural work. Inside
one `RewriteAllUses` call, canonicalize the complete defining-block set and
complete using-block set (including PHI incoming blocks), separated by an
impossible null block. Identical keys reuse the original backward live-in walk's
pruned IDF result and the exact original `BBInfos.reserve` size. Values, types,
uses, available-value maps, notifications, PHI insertion, simplification, and
deduplication are not cached or changed. CFG and DT remain fixed in that scope;
the original utility already shares its predecessor cache across the batch.
The cache holds IDF lists and reserve counts, not all live-in sets. Public
headers, class layout, and the two other JumpThreading SSA call sites stay exact.

The isolated v4 library recompiles `JumpThreading.cpp` and `SSAUpdaterBulk.cpp`
only; its unchanged-source control recompiles the same two units with the same
flags. The standalone v4 actual 2048-branch request passed current Verify,
LLVM before/after verification, and all five retained native seed oracles. Its
383,440-byte executable is byte-identical to the original. The 14 C API controls
produce byte-identical optimized IR and pass 18 native invocations / 33,280 full
u32 results; these author reruns do not replace the separate independent review.
The diagnostic pass time is 5.2435 CPU seconds for JumpThreading, versus about
25 for v1 and about 42 for the original in earlier windows. These are mechanism
diagnostics, not matched formal speedup measurements; CVP remains 19.3791 seconds
in this run and is addressed by a separate candidate.

A separately built counter-only v4 confirms the mechanism on 512-branch:
505 bulk calls, 18,781 variables, 18,276 placement-cache hits, 505 misses,
511,838 visited live-in blocks, and 37,562 inserted PHIs. Each actual group
reuses the same precise two-PHI placement across its variables. It passes current
Verify and all five native seed oracles. No candidate or original library was
overwritten; initial missing-clang link failure and malformed diagnostic CLI
request are retained as invalid observations.
