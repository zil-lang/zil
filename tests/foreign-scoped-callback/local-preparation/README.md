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
