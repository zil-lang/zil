# 205: actual State alias sources at a loop backedge

The Expand author's unchanged 242-byte regression has SHA-256
`31e57e41505e9e501ec946c10354e2be3b520658315e4473a6a4f1b99cf6dc23`.
At clean `1821104458406a8d10db0902a4498d018e0eef14`, it reached Verify,
but its ordinary `T:Copy` definition was unused. Keeping the entire State and
`recycle` definitions unchanged and supplying actual factories produced a
698-byte source, SHA-256
`a4c9bda2e53625bd14f0d0ea45c40406ade1aa78369b47f022ad9c98d7f21833`.
That source actually failed in Types with `E-STATE-ALIAS`, Verify count zero,
and no native execution. Both original inputs and the failed command remain
in `run-logs/205-types-followup/crosspoint01*`.

The private definition ledger previously recorded only a borrow generation,
then treated every cached expression carrying that generation as a value the
next iteration would reuse. A refreshed local had a new valid source, while
an obsolete expression snapshot still carried the invalid old source. The
ledger now records the actual alias key together with its original generation.
For a key read from the loop's incoming state, the backedge checks every current
source of that same key. This includes a freshly bound source that is invalidated
again during the iteration. Independent copied locals, actual parent positions,
borrow kinds, ancestor chains and invalidation generations remain intact.

The nearest raw `continue` control then exposed a second precision issue:
the jump snapshot preceded lexical scope cleanup and retained a moved temporary
owned by the loop body. The original `crosspoint02` failure is preserved.
Break and continue snapshots now retire only locals created inside that body.
Any alias copied into an outer local that still points at such an owner becomes
invalid before the inner local entry is discarded. Outer ownership positions
and their move facts are retained.

At clean `99a553ca74ea42fd913b49ef704eb03a8a7eb330`, the bounded crosspoint
passed three tests and eight native executions. The unchanged 698-byte source
ran at O0/O2 and printed `AB`. A separate source keeps the same `recycle`
definition and supplies a `getchar`-based factory: inputs force zero, one and
three iterations, while an observable marker counts two, three and five actual
callback invocations. Both optimization levels print `CCAB`, `CCCAB` and
`CCCCCAB` respectively. The raw legacy single-State header is tested through
both public Types entry points: current refresh, dead copied alias, branch
refresh and continue succeed; future stale-copy use, stale backedge use,
missing refresh and fresh-then-invalidated sources reject with `E-STATE-ALIAS`.

The prior fixed Types controls remain 5293 (old State), 88 (ordinary eager) and
4498 (replacement), with actual N / N-1 ResourceLimit / fresh equality checks.
The newly measured unused refresh-plus-continue source requires 2935 steps;
its fixed assertion uses the same source as the successful measurement at
`2b0a4416c621a33b22fddfdd168a159c935e6f80`. This is private definition checking,
not a new IR, public permission, runtime loan or accepted final task status.
