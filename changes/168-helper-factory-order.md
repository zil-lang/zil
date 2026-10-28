# 168 follow-up: helper bodies contribute factory dependencies

Baseline: `7179b49ca556479acbd53c1ae4566f63970f31cf`. The full-workspace
regression migration exposed a real declaration-order dependency: `make()` uses
`apply(&init)`, and `apply` constructs an independent opaque factory `other()`.
When `other` follows `make`, the existing direct-call factory graph omits that
edge and helper instantiation encounters an unresolved concrete result.

The Types scheduler will include factory dependencies from the definition body
of every reached callable helper. Direct calls remain resolved with the same
lexical shadowing rules; each helper body uses its own module signatures and
import aliases. Helper keys use existing `(ModuleId, NodeId)` identities, not a
possibly overlapping ordinary FunctionId. A bounded visited worklist avoids
speculative Builder execution and repeated graph cycles. Actual AST visits,
lookup snapshots, and dependency expansion are charged to the existing budget
before allocation or cloning.

This changes only scheduling of already-supported concrete factory/helper
compositions. Existing helper nesting/recursion, borrow, escape, callable mode,
opaque identity, and independent Verify rules continue to apply. A factory cycle
through a helper remains Unsupported; no public schema or budget default changes.

Validation will exercise both declaration orders through the native result 42,
single-module and project entry paths, definition-site helper names/imports,
lexical shadowing, and the nearest recursive rejection. Limited 125 and 168
regressions precede a separate exact-commit review. Old failed workspace and
source diagnostics remain recorded rather than relabeled as fixture errors.
