# 250 synchronous owning callback consumer

The three source modules separate real event/storage construction (`model`),
public single-F by-value FnOnce APIs (`dispatch`), and callers (`main`). Input
0/1 exercises success/cancellation and consumed H versus leftover h captures;
2/9 covers failed/successful second argument preparation; 3/4 calls one concrete
Fn/FnMut twice with distinct owning arguments; 5/8 passes active/empty Enum data;
6 passes an ErrorUnion as callback data; 7 passes an ordinary Product containing
Store data; `:` enters a FnOnce body that returns a Copy error. The Event/Reply
renderer inspects real byte length/content after the call boundary; the Store
renderer checks real length and per-element Copy data through a shared window.
`owned_error_main` uses the same model/dispatch modules with an owning Product
error payload containing Enum data. It returns that payload through the public
callback boundary and caller `?`, then the native entry destroys the payload.
`;` returns a separate captured ByteBuffer through Reply while the consumed
Event's original ByteBuffer is independently freed. Its renderer requires V
(86), while Event preparation always writes U (85); leftover H/h still clean.

`frozen-R` preserves the original three planning probes and old failed receipts
byte for byte. Old Unsupported results are historical failures, never current
passes. `ORACLE.json` is generated only from `tools/research/250/oracle.py`, an
independent model of source ownership/preparation/destruction. It never reads a
compiler report or stdout. Native observations are recorded separately.

Source-operation ordering predicts `SCHhRAQ` for mode 0, `SAHhQ` for mode 1,
`SBAHhQ` for mode 2, and `SCHBhRAQ` for mode 9 (body-local H, unused second
parameter B, remaining environment h, then rendered result A).
The post-entry Copy-error path predicts `SCHAhQ`; the owning-error root predicts
`SCHhQA`, so its payload A must remain alive across caller defer Q. Failure while
preparing that root's event predicts `SAHhQ`, including pending callee cleanup.
The original ten-mode snapshot is immutable in commit c4b82a6ec; subsequent
source fixes and the two new error paths are independently reviewable in Git.

Allocation
failures enumerate each actual source reserve plus disabled/one-past controls;
the native linker shim wraps real realloc/free and compares all six counters,
including live=0. No allocation is injected by replacing a ZIL builtin result.

Boundary source cases, budget probes, raw/Wire mutation checks and non-author
reviews are separate from this model and are bound to their tested commit.
