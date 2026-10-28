# 169: exact Analyze work fixtures after owning-array presence scanning

The first full-workspace run of `33ac699268f0b2466a5fa79e9d53007ac54984f3`
failed two existing exact-budget tests. The task 167 resource-presence path now
visits each local and SSA value even when the module has no resource definitions:
an array of a Copy-capturing closure still needs affine element checking.

The one-function, 100-value fixture therefore needs exactly 100 additional work
units (1130 becomes 1230, plus the unchanged target-text charge). The two-function,
one-local-per-function fixture needs exactly two additional units (104 becomes
106, plus target text). The empty eight-block chain adds no locals or values and
keeps its existing bound.

Only the two fixture constants and explanations change. Their one-unit-below
ResourceLimit checks remain, as do all production accounting and default limits.
The failed original run is retained separately. This is a fixture migration for
newly performed work, not a budget increase or a compilation-speed result.

The second complete run passed 1458 tests and failed three assertions. Two
impl125 tests still required Unsupported for now-grounded factory dependencies
and concrete forwarding. They become positive Verify assertions while the exact
recursive, borrowed escape and callable-mode neighbors remain rejected. The
impl166 partial-Package case still rejects at Analyze with E-ANALYZE-IR; its text
expectation changes to the shared Package/array diagnostic. No rejection category
or production rule changes in these migrations.
