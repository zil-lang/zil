# Borrow current machine types during LLVM API preflight

Stage13 baseline 4cb368712 / production 743c9699e. Machine preflight currently
copies each SSA result type into its definition table. Structural result types,
including checked arithmetic's `{ i32, i1 }`, allocate another field vector for
each definition. Aggregate indexing and GEP traversal also clone types despite
the complete input model remaining immutable throughout validation.

Borrow types from the same current Module, operation, argument or signature.
Return borrowed types from result/indexed lookup and walk GEP component types by
reference. The pointer and comparison result types may use promoted immutable
constants. References remain confined to the current preflight call; no cache,
cross-request identity, Analyze/Verify fact or validation capability is introduced.

Preserve result lookup, result-type validation and identifier/duplicate checking
in that exact order. Keep both complete validation passes, all operand/type/ABI
checks, original GEP diagnostics, type-depth handling, CFG/dominance analysis and
the existing collecting/validation-only scheduling distinction. Do not change
HashMap capacity, other algorithms, supported inputs, budgets or public formats.

Reuse existing preflight controls and add only compact aggregate/GEP and
first-error controls for the affected references. Root coordinates builds,
tests, measurement and independent review. Removed clones are a structural
observation; current-request timing determines any compilation-speed gain.
