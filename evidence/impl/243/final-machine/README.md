# 243 author machine delivery

Author result is limited to the selected machine, layout and budget checks. The
implementation and tests were run at clean `9411e92db84fa9a71c40dfbe5cc0e67f1cfaa1f4`.
`RESULT.json` binds the normative hashes and compiler bytes. The later receipt
commit changes evidence only. Parent integration and independent acceptance are
separate obligations.

The machine representation retains Enum's i32 tag/aligned maximum payload and
ErrorUnion's i1 tag/two fixed slots. Dynamic ByteBuffer/Store heads occupy their
actual three words. Cleanup helpers form a DAG keyed by complete Type, with a
full equality check before memo reuse. Every owning Enum helper branches on the
actual tag and visits only that variant's fields, in declaration order; invalid
tags trap. ErrorUnion helpers branch on its actual tag. Product wrappers call the
appropriate typed child helper instead of recursively duplicating all plans.
Native entry error cleanup uses the same typed cleanup paths.

The four opcodes share the LLVM machine builder used by API/reference backends.
Enum-only observation paths validate each actual selected tag. Unpack loads the
complete selected payload before clearing the source flag, then initializes all
destinations/flags without an intervening failing operation. Existing capture,
StoreElement, arrays and external callback permissions are not widened.

New Lower and emission phases each use one shared counter against the actual
sealed Verify request. The paid presence bit enables payment only after exact
raw-LIR binding; it does not supply type permission. Complete type metadata,
per-operation cleanup helper hashing/layout copies, selected entry error cleanup,
and expanded constructor stores are prepaid. Legacy false-presence emission
returns without the new charges. Default limits were not increased.

## Executed selection

```sh
. /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target-243
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
cargo build --locked -p zil-cli --features llvm-api
cargo test --locked -p zil-backend-llvm -p zil-lower \
  --features zil-backend-llvm/llvm-api impl243 -- --nocapture
ZIL_TEST_REPORT_DIR=/workspace/.zil-env/243-machine-final01 \
  cargo test --locked -p zil-driver --features llvm-api \
  --test impl243_machine --test impl243_default_consumer -- --nocapture
```

The private unit selection passed 5 backend + 1 Lower tests. These are private
representation/budget models, not source acceptance. They cover numeric layouts,
structured branch plans, shared current-request N/N−1/fresh limits, and rejection
of a mutated raw LIR opcode against a false-presence verified capability.

The real-source selection passed 6 tests: 28 builds and 74 machine executions.
Sixty ordinary executions cover unchanged Q03/Q05, recursive multi-field match,
Copy-terminal patterns, owned Enum entry errors containing Product/Store,
Copy scratch lifetime, and empty/nested variant order. Fourteen probes relink
only the compiler's captured API object with the existing 173 allocator wrapper;
six allocations actually fail, all final live counts are zero, and allocations
match frees. The two fixed-ProjectSources five-module default-budget builds do
not execute the consumer. Each API report has builder input, parse=0, no fallback
and verification before/after. Reference backend builds are real Clang paths.

`EXECUTIONS.json` records each stdin, actual stdout/stderr and exit status.
`BUILD-REPORTS.json` records reports, executable hashes and API object facts.
The raw archive contains all of those files, captured API objects, the exact
wrapper/link commands and phase artifacts. `ARTIFACT-MANIFEST.json` hashes every
archived regular file. The wrapper contains no replacement ZIL implementation.

`run-final-cli.py` made six fresh original-Q checks and one standard CLI build of
the frozen five-module source. Five sources reached Verify exactly once; the
original double-consume source still rejects in Analyze. The standard CLI and
fixed-ProjectSources IDs both compile at default limits. The independent 244
runs are referenced separately in RESULT and never added to this author's 74.

Earlier logs/artifacts retain their original intermediate identities, parse,
unit-expectation, budget and lint failures. In particular the first new
five-module regression incorrectly required the reference Clang path to expose
API object reports; that test assertion was corrected. The source and oracle
were unchanged. The old Analyze limit failure preceded the required 241 COW
checkpoint and is not attributed to module-ID order.

## Rust goals and remaining boundary

| Goal | Evidence and limit |
| --- | --- |
| Comparable safety | Finite actual-tag cleanup, atomic extraction and raw binding are exercised; independent Analyze/Verify proofs and whole-language safety remain separate. |
| Runtime/memory | Inline ABI, O0/O2, real failure settlement and live=0 are observed; no Rust runtime/RSS comparison was measured. |
| Expression/maintenance | Original Session/Store option and error API sources now reach native code without changing their expected behavior; framework maintenance cost is unmeasured. |
| Compile cost | Actual-request shared budgets and default five-module compilation pass; this is not a comparative compilation performance result. |
| Concrete advance | The finite owning Sum/Product/Store machine composition is executable; a product-level net breakthrough and four-goal conjunction are not established. |

No full suite, formal product gate, native Windows run or main merge was performed
by this author. `validation/product-gates.json` and normative limits remain intact.
