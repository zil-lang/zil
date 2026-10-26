# IMPLEMENTATION-0026 — Copy aggregate native slice

## Candidate scope and integration boundary

Parent task B0, implementation slice 026. The root delegated Copy aggregate semantics and permitted this worktree to propose public IR changes for serial integration. This proposal admits finite nominal records, tuples and fixed arrays whose fields are recursively Copy; addresses, closure environments, resource values and managed fields are excluded. Named identity includes the defining module. No host-language transpilation, RC, heap boxing or unchecked verification bypass is introduced.

Source construction, projection, record update, nested projected assignment, Copy parameters/returns, static methods and Copy value receivers use real aggregate types and values. LLVM emits structural structs/arrays with insert/extract operations. Dynamic array reads/writes check signed-negative and upper bounds before GEP; fixed scratch storage is allocated in the entry block. Field/index evaluation follows source order and evaluates each index once. Array length is currently a literal; qualified imported type names and address receivers are outside this slice.

## Proposed public schema and budgeting

`Type::Record { name, fields }`, `Type::Array { element, length }`; Core array/index nodes; TypedCFG aggregate construction, field extract/update, dynamic index extract/update. Record/array wire type tags 13/14; instruction tags 13 through 17 respectively. The root owns the combined schema/version change and lockfile; this branch leaves constants unchanged. Interface identities encode type structure, nominal identity and address qualifiers.

Before raw type trees are hashed, compared or expanded into layouts, analyzer and verifier traverse their structure under caller-selected work/depth limits, including record names, field names, fields and array lengths. Cached source type expansion is charged before cloning to prevent exponential declaration expansion. Independent verification checks aggregate arity, field and element types, index types and projection bounds; a matching fingerprint is only identity, never evidence of validity.

Small traversal additions in `zil-ct/src/source.rs` preserve the established bounded recursive source traversal for the new Core array/index nodes. Root and 027 were notified; these do not add CT authority or evaluation operations.

## Evidence and limits

Acceptance evidence will bind the exact production commit and contain actual limited commands. The new driver suite executes complete source-to-verify-to-LLVM-to-native programs at O0/O2, signed/upper-bound traps, source rejections and forged raw candidates. This slice is not the complete B0 language or a substitute for independent review/full product gates. Shared-reference signature support from 025 must be unioned during integration; aggregates containing references remain excluded.

## Author limited verification and Rust comparison

Author checks on 2026-10-23: workspace clippy (`-D warnings`) succeeded; selected 8 library suites passed 83 tests; aggregate driver suite passed 10 tests. Its five normal programs and four trap programs each ran at O0/O2, for 18 native executions. It also checked 10 source rejections, three forged aggregate candidates, bounded raw shapes and one serialized candidate roundtrip. Result programs returned 6483 (records/tuples) and 7498 (arrays). Detailed commands and hashes are in the compact slice receipt; this is author evidence, not independent acceptance.

| Rust goal | Judgment / evidence / remaining gap |
|---|---|
| Safety | Progress: independent aggregate arity/type/projection checks, actual bounds traps, raw shape budgets. Full Rust-level resource, alias and concurrency scope remains unfinished. |
| Runtime and memory | Actual structural stack/SSA layout, no imposed Box or RC; no controlled paired Rust timing or memory measurement, so parity is unproven. |
| Expression and evolution | A multi-function data-processing program now uses records, nested updates, arrays and methods end to end. Complete-task usability superiority is unproven. |
| Compile cost | Source expansion, raw checking and serialization remain budgeted. This slice does not establish paired Rust compile-time superiority. |
| Substantive breakthrough | Not yet established. The concrete advance is widening the independently checked native subset; the next useful comparison is a matched aggregate/array workload, after integrated correctness. |
