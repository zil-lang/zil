# Concrete local preparation in scoped helpers

`frozen/` contains the three exact source files from the read-only baseline
probe `/tmp/zil-next-local-preparation-fa34160`, at compiler revision
`fa34160bacb8ca5bd4c998467be6e6177e3a5aec`. No source byte or foreign symbol
has been adapted. SHA-256:

- generic-local.zil: `4174035e1b867455d2037c7dac3376d10ca4bf5677d728b6595c6de821f497e6`
- concrete-local.zil: `689b846c9746e557a9542479ea13bee8ba6fc1cdbd178aba5f36b393c7772948`
- generic-owned-parameter.zil: `244533ae9e658b65ba16e2febe66ae9697c480047519e8d813f4bea3441f03eb`

The baseline generic-local source rejects at Expand/E-UNSUPPORTED on `give
mark`: "generic managed give requires a direct Package manages field". The
concrete-local and generic-owned-parameter controls reach Verify successfully.
An earlier direct `return Fault.NoMemory` spelling failed independently at
Expand/E-GENERIC-ARGUMENT; these frozen sources use the already-working
`no_memory()` spelling and do not claim to fix that separate issue.

The frozen mains intentionally ignore the result. Native tests still require
the exact actual allocator counters, including one real reserve failure and
live=0. `observed/generic-local.zil` is an explicit nearby variant: it adds
three C observation declarations, a Mark Drop implementation, an observation
inside consume, and an explicit f(4)==5 check plus observation in main after
run returns. The Mark/ByteBuffer/Event/body preparation and failure branch
remain the frozen generic-local operations. This nearby variant makes body
entry, exactly-once Mark cleanup and restored caller F access observable on
success, registration-null and preparation failure.

`driver.c` implements the unchanged `reg` and `close` C symbols. Registration
reads one byte: `0` succeeds and `1` returns null after a synchronous callback.
Both require f(1)==2. Success close requires the actual ByteBuffer allocation
already freed, rejects Closing reentry with the full packed i64 status, and
clears both roots. The observed caller checks cleared roots and live=0.
The allocator object reuses the real realloc/free wrapper and exposes only its
live counter to this independent C lifecycle observer.

`local_preparation_oracle.py` models source operations without compiler reports
or native observations. The separate FROZEN_ORACLE and OBSERVED_ORACLE each
have six rows: success/null crossed with disabled injection, actual first
reserve failure and a beyond-request control. Every row returns exit 0 because
the frozen main ignores its error result; the failure is required in the real
allocator counters. All returning rows require live=0. Planned O0/O2 groups
are 36 frozen executions (three sources) and 12 observed executions per
backend, kept separate from each other and the original callback matrix.

This input/oracle checkpoint establishes no new source, Verify or native Pass.
The existing foreign_scoped_callbacks functional target will execute these
groups after the frontend's precise admission fix is available. No new Cargo
target, original ORACLE change, default budget or product gate is introduced.

The first real functional source replay on the baseline preserves both old
controls' Verify success. Frozen generic-local still rejects at the exact
original Expand/E-UNSUPPORTED span 706..710; the observed variant reaches the
same give gate at span 967..971. These are positive obligations and the test
fails until the admission fix is integrated, without substituting Unsupported
for semantic rejection. The old-control clang O0/O2 native group passes its
24 executions, including eight actual reserve failures and eight beyond-point
controls, with live=0 throughout. API replay is tracked separately.

After exact frontend checkpoint `cc2bcc8a`, all three frozen sources and the
observed nearby source reach independent Verify (one verification run each).
Both explicit backends pass the new groups at O0/O2 independently:

| Group per backend | Executions | Actual reserve failures | Beyond controls | Live-zero returns |
| --- | ---: | ---: | ---: | ---: |
| frozen generic-local | 12 | 4 | 4 | 12 |
| frozen concrete-local and generic-owned-parameter controls | 24 | 8 | 8 | 24 |
| observed generic-local | 12 | 4 | 4 | 12 |

The observed group additionally matches exact body/Mark-drop/after-call events;
body is absent on null and reserve failure, Mark drops once on each path, and
the caller invokes f(4)==5 after every return. Successful close independently
observes zero actual live ByteBuffer allocations before reentry rejection.
The old controls' early replay and later combined replay are repetitions,
not additional coverage. The complete existing functional target also passes
all nine tests on clang and LLVM API, zero ignored/filtered, preserving the
original callback matrix, disjoint/local-body groups and 13 strict negative
source obligations. This is a bounded target run, not a workspace full gate.

Use the existing `foreign_scoped_callbacks` target with
`CARGO_TARGET_DIR=/workspace/.zil-env/target-275 CARGO_BUILD_JOBS=2` after
sourcing `/workspace/.zil-env/env.sh`. The exact selectors are
`scoped_local_preparation_frozen_sources_reach_independent_verification`,
`scoped_local_preparation_frozen_controls_match_native_oracle`,
`scoped_local_preparation_generic_source_matches_native_oracle`, and
`scoped_local_preparation_preserves_cleanup_and_caller_callback`.
For source or native checks use `cargo test -p zil-driver --test
foreign_scoped_callbacks NAME -- --exact --nocapture`; add `--features llvm-api`
for the API backend. Native checks assert the actual backend, no fallback,
and API builder input, parse_ns=0 and verification before/after optimization.

The targeted API Clippy command `cargo clippy -p zil-driver --features llvm-api
--test foreign_scoped_callbacks -- -D warnings` passes, as do rustfmt for the
consumer Rust file and git whitespace checks. Both new oracle files exactly
match independent model regeneration; all three frozen sources still match
the original probe bytes and the recorded hashes. The original callback
ORACLE and project source files have no changes from the baseline.

The concrete generic-local preparation previously rejected at Expand now
composes ordinary owned resources, an Event and a consumed body through real
native execution. This is an expression/composition improvement with limited
ownership/cleanup evidence toward Rust-like safety. There is no measured
same-semantics Rust runtime/RSS comparison, full framework-maintenance claim
or Rust compile-speed comparison; unchanged budgets do not establish lower
compile cost. The four-goal product conjunction and full language remain
incomplete, with existing product-gate requirements unchanged.
