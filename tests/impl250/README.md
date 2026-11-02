# 250 synchronous owning callback consumer

The three source modules separate real event/storage construction (`model`),
public single-F by-value FnOnce APIs (`dispatch`), and callers (`main`). Input
0/1 exercises success/cancellation and consumed H versus leftover h captures;
2/9 covers failed/successful second argument preparation; 3/4 calls one concrete
Fn/FnMut twice with distinct owning arguments; 5/8 passes active/empty Enum data;
6 passes an ErrorUnion as callback data; 7 passes an ordinary Product containing
Store data. Renderers inspect real byte length/content after the call boundary.

`frozen-R` preserves the original three planning probes and old failed receipts
byte for byte. Old Unsupported results are historical failures, never current
passes. `ORACLE.json` is generated only from `tools/research/250/oracle.py`, an
independent model of source ownership/preparation/destruction. It never reads a
compiler report or stdout. Native observations are recorded separately.

Source-operation ordering predicts `SCHhRAQ` for mode 0, `SAHhQ` for mode 1,
`SBAHhQ` for mode 2, and `SCHBhRAQ` for mode 9 (body-local H, unused second
parameter B, remaining environment h, then rendered result A). Allocation
failures enumerate each actual source reserve plus disabled/one-past controls;
the native linker shim wraps real realloc/free and compares all six counters,
including live=0. No allocation is injected by replacing a ZIL builtin result.

Boundary source cases, budget probes, raw/Wire mutation checks and non-author
reviews are separate from this model and are bound to their tested commit.
