# Signature dependency facets

Task G10.5.1.3. Baseline: e69091059ed999a5279303cdd07e9f602703f079 (PR65 integrated). Owner: current Chat, no external AI review requests.

## Why
The current function Types cache includes every declaration signature in each function's context. A signature-only edit to one function invalidates all cached scalar CFGs, even those that do not use it. This is a bounded unfinished part of the earlier semantic-facet/dependency-graph proposal, not a complete general incremental compiler.

## Scope and invariant
Retain the full module/import/name/function-ID namespace, target and all limits. Compute declaration content fingerprints once. Bind each eligible function to its own declaration and each direct callee selected through current name/import tables. Other functions' signature-only changes no longer invalidate it. Renames, insertions, deletions and ID/import changes remain conservative whole-context misses. Foreign/indirect calls and existing non-scalar/template/resource/closure exclusions remain; stored/protocol contracts explicitly take the original path. Types declaration validation and current Analyze/Verify are unchanged. No cached verification authority. No generated-code optimization change.

Write set: crates/zil-types/src/incremental.rs and crates/zil-driver/src/function_incremental_tests.rs. Source guards, full affected crate library tests, cross-session cache and current-error/visibility controls, exact fresh TypedProgram equivalence, budget/cancellation/corruption controls. Any original failure is retained.

## Frozen measurement
Use the prior exact graph corpus/harness from e2ffe7033127e006342465611181367995e6115e; add shared4096 without removing a prior case. Eight cases; graph group 45%, trait 25%, resource 30%, cases equally weighted within their group. Modes: interactive Check/O0/O2=30/50/20%, release=10/20/70%. Edit histories cold/warm/body/edit-warm/signature/restore=10/15/45/10/10/10%; clean-heavy=70/5/10/5/10/0%. Five formal paired cache histories plus one warm-up, same two CPUs, same release compiler profiles, pinned Rust/LLVM, real CLI process through link. Rust comparison uses actual rustc for O2, not Cargo Fresh. Weights are predeclared assumptions, not telemetry. Report arithmetic total wait and absolute seconds saved, each cell and severe tails; missing cells invalidate the aggregate. Preserve machine-code .text equality for every native ZIL pair, output checks and compute-heavy runtime measurements. Larger stress sample is not an upstream application port.

## Exit
Claim improvement only from complete paired data. Preserve negative results. Do not mix this gain with prior PR65-vs-old-main numbers. Existing four main budget assertion failures are outside this change and may not be silently edited or marked passed. No automatic main merge; any production delivery extracts only source/tests and a concise effective decision. General resource/generic/closure Types reuse, stable IDs across insertion, full semantic dependency scheduling, phase attribution and build-level Fresh remain open.
