# Restore established ordinary address signatures in closure inference

The unchanged recovery317 component calls `component.apply(&mut panel, event)` from a local closure. Its ordinary function signature includes Address<Package>. c262 replaced the contextual ordinary-call `is_scalar_or_address` admission with `closures::signature_parameter`, which describes the narrower callable ABI and omits established Package/shared-resource-head addresses. The original source therefore stops at Types before the independent ordinary call/loan checks.

Retain both the established ordinary scalar/address predicate and the new finite Sum signature predicate in `context_signature`. This restores prior ordinary-call acceptance without changing closure capture qualification, first-class function-pointer signatures, worker/FFI admission or any source/oracle. Complete types still traverse the existing bounded signature snapshot and ordinary concrete checkers.

Replay the original recovery317 success/error/null cleanup pair and nearest negative/positive controls. Add a focused ordinary mutable Package call inside a closure, and preserve a neighboring ordinary shared-to-exclusive permission rejection. No quota or ABI changes.
