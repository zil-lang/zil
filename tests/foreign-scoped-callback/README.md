# Scoped foreign callback consumer

The three ordinary ZIL modules and `driver.c` are inputs to the actual compiler
and native linker. `bridge.run` is a public generic wrapper; it borrows the real
callback location and consumes a distinct body. The C driver uses a bounded
same-thread stack to simulate registrations and clears both code and userdata
roots before returning from close. It never calls through a cleared root.

`ORACLE.json` comes only from reading source operations and the frozen protocol
in design §12.4.1. `tools/language/foreign-scoped-callback/oracle.py` does not read
compiler reports or runtime observations. It predicts source cleanup, callback
counts, packed status/result bits, stdout, exit and six real allocator counters.
Preparing this oracle does not establish compiler or native success.

Inputs `0` through `7` cover Fn, owning FnMut, register-null before and after a
synchronous callback, two actual instances of the same concrete closure type,
nested distinct concrete closures, signed i32 boundaries, and repeated scopes
in a loop. FnMut bodies call the callback repeatedly; an active callback invokes
the same entry recursively, which must return status 1 without entering f.
Every close also calls its entry and requires the same rejection. The source
calls f after close to check restored access and persistent mutable state.

Owning cases prepare a real Mark and a nonempty ByteBuffer for f and the body.
The body is passed as the last argument, so its allocation can fail after f is
prepared/borrowed. The nested body handles its own allocation failures normally,
then closes the outer registration. Null registration never enters the body and
drops its remaining environment exactly once. Body cleanup precedes close;
callback environment cleanup follows close. Every returning allocator path has
zero live allocations. The disabled-injection normal run serves as its control;
fault runs include each request and one beyond the complete operation.

Inputs `8` and `9` trap inside f and body respectively. These are subprocess
abort controls, not recoverable error paths. The oracle expects Linux LLVM trap
signal 4 and the flushed prefix only; it claims no destructor/atexit or live-zero
guarantee after abort. The C driver obeys the external protocol in these controls.
It does not invoke undefined C behavior as a language-safety test.

Temporary output directories use input-byte hex labels. No historical evidence,
full-output archive, transport round trip or permanent audit store is required.

`negative/` contains 13 current standalone rejection controls and the preserved
explicit Address<Closure> capture gap. Their risk descriptions are
frozen in RISKS.json; the one functional Rust target also matches source phase,
code and relevant reason, with zero machine execution for illegal cases. These
are obligations, not a claim of rejection before actual execution. The raw
verifier's own codes are not substituted for the first source diagnostic.

`project/disjoint_body_main.zil` is a separate positive nearby control: body
updates an unrelated borrowed i32 twice in its actual event loop. The observable
C events, result and six zero-allocation counters equal the original mode-0 row;
the original ten-mode source/oracle stays unchanged. Its native group is two
additional runs per selected backend (O0/O2), separate from the original 66.

The explicit body-captured Address<Closure> alias source currently fails an old
capture-profile gate before this feature's alias kernel. It is retained as an
uncovered gap, never counted as a semantic rejection Pass. The current local
alias control instead keeps an ordinary shared f address live across exclusive
publication without capturing that address in body; it reaches Analyze. Raw
same-root rejection and distinct-root acceptance remain independent obligations.

`project/local_body_main.zil` and `local_body_bridge.zil` are a second separate
nearby control. The public wrapper has only an F parameter: it creates its own
concrete, noncapturing body, then publishes the caller's actual FnMut location.
The local body pumps two events; the caller checks the body result, cleared C
roots and persistent callback count after close. `LOCAL_BODY_ORACLE.json` is
generated independently by `tools/language/foreign-scoped-callback/local_body_oracle.py`
from those source operations and the C protocol. It adds two native executions
per backend, with no change to the original source/oracle matrix. The initial
run rejected at Expand's existing generic-expression profile; this is a
required legal combination, not a negative control. The first expanded input
also imported two modules that redeclared the same C symbols; Verify correctly
rejected that fixture with E-VERIFY-FOREIGN. The final nearby input has one
self-contained bridge module and one declaration per foreign symbol.
After exact frontend `9f4e5b9c` and independent Verify `abf642d5`, the local
body source reaches independent verification and passes O0/O2 clang and LLVM
API execution, including builder input, parse_ns=0 and LLVM verification.
The complete functional target has five passing tests with zero ignored or
filtered tests on each backend, including all 13 strict source rejection controls.
The original group remains 66 executions per backend; disjoint and local-body
groups each add two separately counted executions per backend.

The body factory takes its captured Event into a real body local, so its fresh
implementation is genuinely FnOnce. The nested-body factory reads its owning
Event and declares the actual Fn mode; the structured call still consumes that
body once and cleans its remaining capture before close. These exercise both
responsibility paths without changing the existing exact-kind factory rule or
the original observable oracle.

Local checkpoint after exact producer `4db36c6e`, Verify `de5849cc` and Analyze
`f7dd7a81`: the three-module source reaches independent verification, and all
13 current source rejection controls match their required phase and code.
The original frozen matrix passes clang and LLVM API at O0/O2: 66 executions
per backend, including 34 actual allocator failures, 12 beyond-allocation
controls and four abort controls. All 62 returning executions per backend
match all six counters, including live=0. The disjoint borrowed-scalar nearby
control passes two additional executions on each backend. These are bounded
functional checks, not a full gate.

Commands use `source /workspace/.zil-env/env.sh`,
`CARGO_TARGET_DIR=/workspace/.zil-env/target-270 CARGO_BUILD_JOBS=2` and
`cargo test -p zil-driver --test foreign_scoped_callbacks NAME -- --exact --nocapture`.
The exact names are `scoped_event_loop_source_reaches_independent_verification`,
`scoped_callback_source_rejections_bind_the_actual_risk`,
`scoped_event_loop_matches_source_oracle_at_o0_and_o2` and
`scoped_body_can_borrow_a_disjoint_scalar_through_an_event_loop`.
The new control is `scoped_f_only_wrapper_creates_its_own_concrete_body`.
Final runs omit `NAME -- --exact` to run all five tests in this functional
target; this does not run the workspace suite.
For the API backend add `--features llvm-api` before `--test`; its native
reports must identify builder input with parse_ns=0, verification before and
after LLVM optimization and no fallback. The three existing positive tests
were rerun together after the Analyze alias fix and these report assertions.

The first combined API positive replay passed the original matrix and source
verification but failed the disjoint harness with BrokenPipe: that nearby
program does not read stdin and can exit before the parent writes its byte.
The harness now accepts only that write error and still checks the child's
exact exit, output and six counters. The subsequent combined replay passes
all three existing positive tests on both backends. The initial harness
failure remains distinct from compiler or program failure.

The first targeted API Clippy run failed in Types' helper-flow classifier
with too_many_arguments. Frontend `9f4e5b9c` reorganizes those arguments; the
final `cargo clippy -p zil-driver --features llvm-api --test foreign_scoped_callbacks
-- -D warnings` passes. The consumer Rust file passes rustfmt, both oracle
files exactly match independent source-operation regeneration, and git diff
passes whitespace checks.

This slice adds real source-level composition and independently checked
ownership/borrow controls toward Rust-like safety. Runtime evidence is limited
to these native executions and allocator counters; no Rust runtime/RSS pairing,
complete framework maintenance comparison or Rust compile-cost advantage was
measured. The original default budgets and product gates remain unchanged;
the four-goal product conjunction and full language remain incomplete.
