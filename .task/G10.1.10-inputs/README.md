# Independent Source consumers (early accepted-baseline preflight)

All inputs are derived from real existing source paths, not handwritten IR.
They have **no new-implementation acceptance result yet**. They must be run against the
exact frozen G1.3.9/G3.3.9 delivery, and a failure before the intended consumer
must retain its actual phase/diagnostic rather than count as a passing negative.

Accepted-release baseline preflight is now recorded in
`early-preflight-release-original.json`, binding CLI SHA256 `91ba589a...` to
the existing PR45 source `0fee93aa7...`; its crates tree equals fd261b22.
All eleven original sources pass parsing, then stop at actual Expand/Types/
Analyze implementation gates. The two lending helper headers were subsequently
made explicit as `f:&'env F -> &'env i32` and the mutable counterpart, so the
returned environment field is tied to the receiver's real life rather than an
elided relationship. Original bytes remain in input commit afaa8b44d and the
original receipt preserves their first rejection. Revised results and ordinary
lending control are in `early-preflight-release-revised.json`. These are stage
observations, not source admission or native results for the new implementation.
An earlier debug-CLI run is separately retained with its unproved source
provenance; it does not inherit the release binary's identity.

| Input | New obligation | Intended native observation |
|---|---|---|
| captured-whole-package.zil | whole target is an actual captured `&mut P`, not Direct | exit0, `AB42\n` |
| store-whole-package.zil | whole target is an actual mutable Store element | exit0, `AB42\n` |
| store-whole-package-abort.zil | real `?` before Commit; prepared B drops before original A | exit0, `BA42\n` |
| two-leaf-package-helper.zil | both old leaves retire and both new leaves remain owned | exit0, `ABCD42\n` |
| nested-package-helper.zil | recursive child and sibling through a borrowed whole target | exit0, `ABCD42\n` |
| nonlocal-enum-helper.zil | borrowed complete Enum target crosses Left→Left and Left→Right | exit0, `ABAC42,43\n` |
| nonlocal-enum-abort.zil | borrowed multi-leaf Enum retains its old active branch after `?` | exit0, `CAB42\n` |
| two-role-user-drop.zil | qualified F/G, real Managed give, invoked and uninvoked paths | exit0, `ABBA91\n` (callee parameter cleanup reverse preparation order) |
| two-role-fnmut-loop.zil | actual three-iteration CFG and two independent mutable qualified environments | exit0, `45\n` |
| qualified-lending.zil | qualified single-role helper returns each actual environment's field | exit0, empty stdout |
| qualified-lending-mut.zil | actual first result last-use releases exclusive environment | exit0, empty stdout |

Native expectations are semantic oracles to test, not observations. The abort
case uses a returned error, not a trap. It proves cleanup of an already prepared
argument B and preservation of A until Store teardown; allocation-injection
coverage remains a separate root-coordinated obligation.

The adjacent `consumers.rs` is ready to be included as a child module of the
existing `aggregates` test target after production freeze. It is intentionally
not registered while these inputs are still pending implementation. Existing
source references are terminal340/whole-helper/g137_review_success.zil,
g138_review_original_failure.zil, replace-enum-terminal-fresh-runtime.zil,
terminal342/fnonce-helper-bridge/helper-owned-byvalue.zil, and
`impl184_lending::borrowed_generic_helper_forwards_environment_lending_origin`.

Run bounded filters `g10110_source_and_honest_raw_wire`,
`g10110_nearest_source_negatives` and `g10110_native_complete_consumers` via
the existing aggregates binary/target. Root must bind implementation SHA and
input SHA first. Use workspace TMPDIR/ZIL_TEST_REPORT_DIR, a single shared
build window, existing target storage and no formal full gate. Do not create
per-case targets or retain duplicate raw phase archives.

Negative controls are surgical source transformations performed in the module:
second-leaf stale Desc after whole replacement; old same-Store element address
after reserve; double consumption of the actual F role; two mutable roles aliasing
one actual environment; a qualified FnMut result
used after another exclusive call; a borrowed local lender result returned past
environment destruction. Separate positive sources must pass before these
negatives count as coverage of the corresponding obligation.

The first fixture's whole captured target and Store fixtures are deliberately
new parent-scope cases. Existing accepted bare loan/pin and Direct single-leaf
whole-helper source tests remain separate regression controls; replaying those
does not establish these new cases. Multiple-target Package helper effects and
broader loop/early-error combinations remain additional parent obligations
beyond these eleven initial inputs.

Root initial integration c77b7a1e7 actually ran these consumers. It exposed a test oracle omission: CompileOptions.entry uses the existing full-return-value harness in zil-backend-llvm/src/emit.rs, which prints the selected main's i32 after explicit Source output. All fixtures compute zero on success, so native expected stdout is the table's explicit output followed by `0\n` (including lending). The original observed captured output `AB42\n0\n` and failing test log are preserved outside source; the oracle is corrected from the fixed harness contract, not by weakening equality. Nested Package still genuinely fails at Analyze unpaid responsibility, and qualified lending still fails before its intended negative obligation; neither is counted as Pass.
