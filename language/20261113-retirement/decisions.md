# Language foundation retirement decisions — 2026-11-13

This archive preserves 31 inactive language source/review branches as commit parents. It does not accept those snapshots on main. The manifest identifies exact refs, source/index/worktree commits and ancestor coverage. All checked worktrees were clean, all exact heads were on GitHub before retirement. Current source branches, goal and compiler work are excluded.

## Accepted and superseded snapshots

PR85 numeric and PR86 ten-width Store integer layout are already accepted on main. Their source/review refs can retire. Anonymous/callable and closure receiver snapshots have been integrated into current public/ownership branches. Early standalone Analyze/Verify, nested Store, ownership compatibility, iteration, Dyn provenance, and CT schema snapshots remain recoverable here; current branches supersede them but do not imply all old unique work is accepted. The original Dyn provenance tip includes additional Origins tests retained in this archive. Old failed inputs and diagnostic decisions remain tied to their original commits.

## Decisions retained for current construction

- Analyze and Verify independently prove actual constructors, writes, reachable returns, loan ancestry and complete identity domains. Neither trusts the other's summaries.
- None means unknown, never empty. Some(0) requires a complete proof that the actual domain is closed. Static positions are distinct from ordinary lifetime positions. Mixed Static/parameter values retain actual parameter ancestry.
- Persistent Fixed classes remain stable; Invocation relations are reproved separately for actual call roles. A per-call relation cannot merge persistent classes.
- Opaque Dyn types retain the full original nominal, phantom and intrinsic domains. A two-class origin mapped to one lexical image remains two intrinsic classes; the image cannot recover its preimage. Associated RHS and full witness queries use original identity terms.
- Compile-time queries observe an immutable original Source stage. Generated declarations become visible only after atomic validation/commit of the entire pending generation batch. Errors and cycles do not publish a partial catalogue.
- Phantom lifetime captures expose a real missing semantic chain: Marker<'a> can retain a lifetime with no physical borrow slots. Proposed completion requires ordinary declaration semantic scope and actual construct/call/type-use sites, carrying full Type/Const/Life identity through Core, Typed, Wire, fingerprints, CT and independent verification. This is a pending design, not implemented acceptance. No fake physical borrow or Known(0) may replace semantic identity.
- An unused witness lifetime with no intrinsic or method occurrence still needs an actual original lexical anchor. A fallback based on the witness lexical binder is under Astra review and is not yet a settled implementation rule.
- Task nested owners need actual capture/physical-leaf responsibility and same-frame phase proofs for asynchronous cleanup; recursively permitting a type is insufficient.

## Publication boundary

The independent standard-library stage is PR87 (owning hash collections, integer/float text, buffered I/O, graph/resource corrections). At this archive point its final candidate is in unified validation and independent review. Task, Dyn, broad lifetime composition, phantom capture and full compile-time query work remain open. No full language 1.0 claim is made.
