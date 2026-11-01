# 236 relay and cleanup-age investigation

No kernel code changed. The joint candidate `bf7b1de72` reaches Analyze and stops before Verify. Its new defer checker rejects cleanup of `late` while `current` is newer because `current = relay(current)` performs a new whole-local Store after `late` was initialized.

The full main CFG first conflicts at block32 instruction7, `DropManaged(late=l11)`, source span971..979. The recorded incoming path is 0→1→3→4→6→7→9→11→12→14→15→17→18→19→21→23→24→26→29→30→32. `current=l7` had been taken, passed through a helper and stored again; the Store rebuilt `late→current`. Types subsequently cleans `late→Q→current`. This is a newly established age edge, not a stale old-current edge.

The no-defer reduction also fails the existing resource checker with `resource closure: cleanup is not reverse successful initialization order`. Therefore deleting or weakening the new defer pass would expose the same mismatch in the older resource path. The two original minimum inputs and their original CLI commands/reports, the author's no-defer reduction/report, and actual TypedIR files are retained here. The debug extraction script models only the relevant transfer operations and is not a safety oracle.

The frozen language requires reverse successful local construction/initialization order. Existing mutable Managed and concrete Closure rebinding instead use a retained private Package shell and explicit field replacement. A shell initialized once retains its cleanup position while its child is taken, prepared and refilled. Bare whole Product rebinding currently bypasses that lowering and emits Store to the whole local again.

Root chose to investigate the existing stable-shell frontend route under238. Both independent kernel age rules and Take/Store checks remain intact. The236 source and cleanup oracle are unchanged; no new acceptance or implementation success is claimed here.
