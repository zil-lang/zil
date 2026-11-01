# Buffer small writes while computing object action keys

Baseline: 365c8a558, after the Stage 8 implementation and results. Warm default requests restore the semantic candidate and hit all object units, but still hash each current machine model. `machine_key` sends derived `Hash` fields directly to SHA-256; the TypedModule fingerprint already coalesces small writes with a 4 KiB buffer.

Add a private 4 KiB buffered hasher only in `crates/zil-backend-llvm/src/object_cache.rs`. Keep the namespace, field order, derived Hash encoding, SHA-256 byte stream and resulting complete object keys identical to the unbuffered implementation. Preserve the diagnostic-only fingerprint exclusion. Do not change the global FingerprintBuilder, action protocol, installed tool identity, cache format, current Verify/preflight, publication, language acceptance, errors or logical budgets.

The bounded candidate uses fixed stack storage and forwards large slices directly after flushing earlier bytes. Tests compare byte streams with the existing SHA-256 implementation at hash-block and buffer boundaries, and compare current model keys with the previous unbuffered key calculation, including field mutations and large byte initializers. These are identity controls, not evidence of compilation speedup.

Writing scope: this proposal and object_cache.rs, including its inline tests. The author does not run builds, tests or timings while root performs the isolated Stage 9 diagnostic requests. Root will arrange focused checks, independent review and any small paired performance screen. Keep the Stage 8 warm regression unresolved until measured evidence explains or changes it; do not infer a gain from fewer SHA update calls.
