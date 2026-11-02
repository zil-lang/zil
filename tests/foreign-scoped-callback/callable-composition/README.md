# Concrete callback composition

`frozen/` preserves exact source bytes from the exploratory probe at compiler
`9c6a64ee84fe8c03af66634236edc4391d5b772a`; hashes are in SOURCE_HASHES.json.
No source or C symbol adaptation is needed. Sources use the original `reg`
and `close` names. The prior compiler was explicitly built from that clean
revision with the existing target-275 cache; binary SHA-256 was
`1f6b0a34b407607b4d35530420463ee2ad30dbbfa51c71bd878b0f614f04f60c`.

The two handler sources put an actual concrete Event->Reply closure inside
the generic With body, or in the enclosing generic wrapper and then capture
it. Both initially reject at Expand/E-UNSUPPORTED, "this generic expression
requires additional symbolic checking" (original spans 1144..1178 and
1111..1145). Their two `-concrete` controls differ only by moving F into the
non-generic run and both reach Verify.

The simple factory source moves an Event into a concrete factory-produced
FnOnce body; its generic wrapper reaches Verify but initially rejects at
E-VERIFY-CALLABLE-CONTRACT, "scoped callback operand is not one actual helper
role" (original span 799..1009). Its `-concrete` control reaches Verify.
The dispatch-factory source carries the complete Event->dispatch->Reply->render
pipeline inside that concrete body factory and hits the same Verify gate.
These are legal source obligations, not semantic rejection controls.

`nested-helper-propagate.zil` is an already-supported result-propagation
control: prepare()? -> run(f)? -> main? all reach Verify, while F(4)==5 is
checked after success. The direct generic `return Fault.NoMemory` spelling
still has a separate old Expand/E-GENERIC-ARGUMENT context gap; these sources
use no_memory() in the generic branch and do not claim to fix that issue.

The separately probed `handler-without-with.zil` calls F normally, creates the
same concrete Event->Reply handler, and dispatches/renders without any With.
It hits the same old Expand/E-UNSUPPORTED gate at 1137..1171 on the baseline.
Its source acceptance is a separate positive control: With's presence must
not grant closure permissions. It is included in source verification only.

Before implementation, that nested-helper control and the concrete local
handler control passed 32 actual native runs in a temporary probe: clang/API
O0/O2, success/null/reserve-failure/beyond-point, 16 per backend, four actual
allocator failures and four beyond controls per backend, all live=0. This
does not establish source or native success for the initially rejected files.

`observed/` contains explicit nearby variants of the two handler and two
factory sources. They add C observations, Mark Drop, and a remaining Mark(7)
inside the body environment. Event's Mark(3)/ByteBuffer transfer through Reply
and render; after body execution, With must drop the still-live Mark(7) before
close. Null cleans both without body entry. Reserve failure creates neither
Event nor Mark(7), and only the initialized Mark(3) is dropped. Main stores
run's outcome, invokes the real F(4)==5, checks cleared roots/live=0, and only
then propagates the outcome with ?. Thus F reuse is observable even on null
and preparation failure, rather than being skipped by an early main ?.

The independent C driver uses a real register callback and Closing reentry,
requires actual ByteBuffer live=0 before close, clears roots, and checks the
observed Drop set and restored caller access. The probe wraps actual runtime
realloc/free calls. The source-operation oracle is separate from compiler or
native output: one reserve, Event(3, one byte)->Reply->render(4), one remaining
Mark(7), and exact registration/cleanup order. Each oracle has six rows:
success/null crossed with no injection, actual reserve failure, and beyond
the only request. Success exits 0; null and reserve failure propagate Fault
through main and exit 1. All rows require live=0.

This first input checkpoint adds no new Cargo target and changes no prior
ORACLE or expectation. It establishes no acceptance for the rejected source
groups. Validation will use the existing foreign_scoped_callbacks functional
target with target-280/jobs2 and explicit backend/O0/O2 separation.

The first real source-target run preserves all four existing controls and
reproduces the two original handler Expand gates and two original factory
Verify gates, plus the matching four observed variants. Unsupported/Reject
is recorded as the original failure of a legal source, not a passing negative.
The new functional native-control group passes 48 clang executions (four
controls, six rows, O0/O2), including 16 actual reserve failures and 16 beyond
controls, all live=0. This supersets the earlier exploratory control cases;
replays are not additional coverage.

Commands source `/workspace/.zil-env/env.sh` and set
`CARGO_TARGET_DIR=/workspace/.zil-env/target-280 CARGO_BUILD_JOBS=2`.
Use `cargo test -p zil-driver --test foreign_scoped_callbacks SELECTOR --
--exact --nocapture` for one test; add `--features llvm-api` for API. Source
verification uses `scoped_concrete_composition_sources_reach_independent_verification`.
Native tests are grouped by existing controls, local handlers, factory bodies,
and observed handler/factory bodies under `scoped_concrete_composition_`.
The separate observed groups allow each producer/kernel checkpoint to be
tested before the other one is ready. Existing functional tests share the
same target and preserve their original oracles and assertions.
