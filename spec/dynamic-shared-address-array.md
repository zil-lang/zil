# Dynamic shared address Array selection

A dynamic read selects an element from an actual nonempty fixed Array of thin shared addresses to finite closed Copy storage. The initial contract covers ordinary Array values and the existing qualified Record and owning Package direct fields. It adds no dynamic slot borrowing, replacement, extraction/refill, exclusive Array copying, Slice profile, or general Address Copy qualification.

Literal indexing retains its existing exact physical ordinal path. A dynamic expression captures the actual Array value exactly once before evaluating its index exactly once, then uses the existing `ExtractIndex` value operation. Shared views and fields use the existing complete `ReadAddress` snapshot. This may copy the pointer Array; it is not an O(1) runtime claim.

Both kernels must independently check the real Array length, shared thin element profile, integer operand and exact element result type before the ordinary pure-Copy fallback. Every possible `Field(i)` must be present, initialized and sourced. Final snapshot checks use the current CFG state for each actual root and ordinal; one valid root or another nonempty leaf cannot hide a missing or vacant candidate. Extraction performs the existing shared access check on each real candidate and retains their actual target and loan union. Index evaluation and intervening raw instructions must not create a gap in those dependencies.

Upper checks use actual element upper facts, not the containing Array borrow or `Element` metadata as provenance. The existing equality-based class solver does not implement a general lifetime meet. Same-upper and distinct rigid-upper local-consumption controls must identify that qualification boundary honestly before extending the admitted profile.

The backend keeps its existing actual-length unsigned bounds comparison before GEP/load. Constant selection keeps its previous precision; a dynamic value conservatively retains every candidate even when a caller happens to pass zero or one. Each new scan, lookup, path copy and source/loan union is charged before work. Large owning-Package snapshots and their native ABI remain a separate boundary.
