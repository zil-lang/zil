# Shared Product parameter capture sources

These ordinary synchronous programs freeze the original missing source and the
12 parameter/direct probes from the investigation. Two further frozen
different-Store seeds cover pop and the whole-call ordering control. `expectations.tsv` is the
independent oracle: arithmetic and actual Store identities determine the result;
compiler reports do not generate the expected values. SHA-256 binds source bytes.
Successful programs must exit 0 with empty stdout/stderr. Rejected programs never
run natively. The oracle names each exact source phase: captured conflicts are
rejected by Analyze, while their direct-reader controls reach Verify. Both require
E-BORROW-CONFLICT; raw/Wire always test Verify independently. A Local-only Unsupported
for a legal source is a missing capability, never a passing negative check.

`original-local` is the exact original kernel-gap source. `local-last-use` and
`store-last-use` resume mutation only after render returns and the view has no
future use. `different-store` operates on a different actual Store.
`same-store-reserve` and `same-store-pop` conflict with the active borrowed element.
`inside-call-last-use` retains the conservative whole-call may-effect boundary;
this slice does not refine the callee operation order. Each `-direct` source
removes the closure while keeping its matching operations and expected outcome.

The consumer lives inside the existing `impl226_shared_product_capture` Cargo
target under `parameter_capture`; it adds no Cargo target. Source verification,
source-derived Parameter holds/raw/Wire, and native O0/O2 are separate tests.
The native backend follows the existing feature switch: default clang, or
`--features llvm-api` for the LLVM API. Temporary executable/object files are
removed; only small result rows are retained under ZIL_TEST_REPORT_DIR when set.

Before the core changes, run the legal-source Verify test to preserve its real
failure. Do not weaken expected outcomes or mark it ignored to make this baseline
pass. Analyze/Verify implementation slices are separate dependencies.

The raw/Wire conflict test first verifies each actual different-Store source
(and its candidate), then changes only the main Call's second actual argument
to the Store descriptor already used by OwnedStoreBorrow. Types and candidate
holds remain identical; only the actual Store identity changes. The mutated
module goes directly to Verify raw and Wire, without calling Analyze, so an
earlier source rejection never substitutes for the independent check.

Additional frozen controls distinguish two Store fields inside one DB, give of
the same or a different Store, and transfer of an entire owning DB. The lawful
whole-owner neighbor borrows DB A and transfers independent DB B. Their expected
values follow the stored count 7 and concrete owner identities, independently of
the implementation probes. The original fifteen source files remain unchanged.
