# Shared Product parameter capture sources

These ordinary synchronous programs freeze the original missing source and the
12 parameter/direct probes from the investigation. `expectations.tsv` is the
independent oracle: arithmetic and actual Store identities determine the result;
compiler reports do not generate the expected values. SHA-256 binds source bytes.
Successful programs must exit 0 with empty stdout/stderr. Rejected programs never
run natively and must reach Verify with E-BORROW-CONFLICT. A Local-only Unsupported
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
