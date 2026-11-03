# Synchronous shared Product parameter captures

Ordinary `render(&Product)` can create and call a local synchronous closure while
retaining the actual Parameter root and the caller's Local/Store dependencies.
The frozen independent oracle covers 43 sources: 23 legal programs and 20
conflicts. Both existing-target backend runs pass **17/0**, with zero ignored or
filtered tests; **92 new native runs** cover all 23 legal sources at O0/O2 through
clang and LLVM API. All exit 0 with empty stdout/stderr. LLVM API uses builder
input without fallback. `FINAL.json` binds the exact tested code, producer commits,
input manifest, commands and actual counts. Prior checkpoints are not added.

`expectations.tsv` records the independent arithmetic/actual-identity oracle,
SHA-256 source binding and each exact source phase. Thirteen captured conflicts
stop at Analyze; seven direct controls stop at Verify. Every conflict requires
E-BORROW-CONFLICT, and illegal sources never run natively. Seventeen source-derived
captures retain Parameter(0), rather than a relabeled Local, through raw/Wire.
All thirteen captured negative cases also reach independent Verify raw/Wire;
Analyze rejection does not substitute for that evidence. The bounded test claims
in `raw_prefix.rs` are untrusted fixture data, never a reused safety solver.

The initial baseline passed Types but blocked four legal capture sources at the
Analyze Local-only gate with zero Verify runs. Three corresponding captured
conflicts also stopped at that wrong early gate. Direct controls already exposed
the proper success/conflict behavior. Later nearest cases exposed Store origins
lost through temporary DB construction, extraction and replacement. These source
bytes remain frozen, including the original kernel-gap input and earlier 29
sources; no rejected legal input is counted as a passing negative.

Assembly-before-borrow with different fields is legal. Borrow-before-give remains
invalid even if a later operation gives the other field. Swaps completed before
the view may use opposite fields; same-field transfer and a swap across an active
window reject. The original four swap probes remain unchanged. Two additional
7/11 identity controls must read 11 from swapped left and 7 from swapped right;
omitting the swap cannot satisfy their zero-exit oracle.

The consumer is nested in the existing `impl226_shared_product_capture` target;
no task-specific Cargo target was added. Temporary executables, phase dumps and
raw candidate files are removed; only useful sources, assertions and this single
functional result remain. Current-call may effects stay conservative, including
callee reads followed by same-Store invalidation. Direct dynamic Store capture,
outer captures, escaping/long-lived callbacks and new borrowing modes are not
opened by this profile.

Safety evidence is bounded to these actual roots, windows and independent kernel
routes. The implementation keeps the existing borrowed-address representation;
no implicit heap box/RC/lock was added. Ordinary API composition advances, while
Rust runtime/RSS, maintenance cost and compile-speed comparisons remain unmeasured.
The original Unsupported source reaching both native backends is a language
increment, not completion of the product four-goal conjunction.
