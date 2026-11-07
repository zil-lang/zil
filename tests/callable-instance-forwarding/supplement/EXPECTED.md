This is a separate supplementary group for the structural private callback
carrier review, registered by root in bfbb677ec. It does not change the frozen
256 source matrix, its oracle, or its historical results.

`ordinary_main.zil` imports only the frozen model. There is no callable
forwarding helper or call anywhere in that project. A mutable callback owns a
Mark M and a nonempty ByteBuffer and accepts Event by value, returning Reply.
Its environment remains alive between two calls, and each Reply is checked and
destroyed by render before preparing the next Event. Input `1` returns before
any callback call; its environment must still be destroyed exactly once.

| Project | Input bytes | Source operations | Expected stdout | Exit | Allocation requests |
| --- | --- | --- | --- | --- | --- |
| ordinary | 30 | Call twice, count 1 then 2, consume both replies, drop environment | S1RA2RBMQ | 0 | 3 |
| ordinary | 31 | Never call; drop environment | SMQ | 0 | 1 |
| mixed | 30 | Select A before keep; call returned A; drop returned A and unselected B | SaABQ | 0 | 0 |
| mixed | 31 | Select B before keep; call returned B; drop returned B and unselected A | SbBAQ | 0 | 0 |

In ordinary mode 0, allocation failure at requests 1, 2, and 3 yields respectively
`SMQ`, `SAMQ`, and `S1RABMQ`, each exit 1. At request 1 the Mark is still an
ordinary local. At later requests the failed Event preparation cleans its own
Mark before the still-live mutable callback environment is destroyed. Its body
does not run on failed argument preparation. Mode 1 failure at request 1 yields
`SMQ`, exit 1. The allocator oracle includes disabled injection and one request
beyond each complete source operation. Every path has zero live allocations.

`mixed_main.zil` reuses the frozen marker factory, producing two distinct owning
instances with the same concrete ClosureId. The branch occurs in the caller
before the forwarding call: the selected actual argument position is the only
instance the helper must return. This complements the preserved illegal sources
that substitute another instance inside a helper. The two branch alternatives
are both valid and exercise branch-specific live flags and unique destruction.

Existing owning callback native sources all pass through keep; their invocation
and uncalled cleanup cases do not cover the no-keep ordinary private slot.
Existing branch forwarding sources always select the same supplied instance
inside the helper, so they do not cover this caller-side selection either.
The mixed source deliberately reuses the existing Copy call signature with an
owning Mark environment; it does not add an owning-signature opaque factory.

The independent generator is `tools/research/restored-product-transport/supplement_oracle.py`. It reads
source operations and the original source-oracle definitions, never reports or
native stdout. The supplementary ORACLE.json is frozen before native execution.
Planned O0/O2 totals are 32 native runs (8 normal and 24 allocator), including 8
real allocation failures. These supplement the existing 256/190/90 native,
allocator, and failure counts; they do not replace or augment historical counts.
The two groups may be combined only after an actual single-candidate replay.

At preparation time: source/native executions 0. Compile-only harness validation
does not establish source acceptance. Final native replay waits for root's exact
common candidate R. Artifacts will use the existing input-byte hex directory
labels and must pass the unchanged evidence_transport pack/recover/restore.
