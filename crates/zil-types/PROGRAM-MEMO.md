# CP29 / CP32: process-local scalar builder memo

`check_program_with_memo` accepts an opaque `ProgramMemo` and returns a raw
`TypedProgram` plus a candidate next memo. A caller commits that candidate only
after the requested compilation stages have succeeded. The original
`check_program` entry remains available and does not collect a memo.

## Retained unit and identity

The retained unit is one unchanged module's ordinary function builders. A body
edit rechecks every function in that module. The memo is one recent successful
project epoch; it is not a multiversion store. In an A → B → A sequence, the
changed module is checked again even when other modules remain reusable.

All shape, name, declaration, signature, visibility, resource-effect and
interface work still runs. The environment compares the exact complete signature
tables, ordered module and declaration identities, imports and aliases, root,
target, all eleven `Limits` fields, parameter/result modes, and full foreign
declarations. Driver callers additionally bind the existing Types rule/build
input fingerprint. Each reusable module compares its complete resolved
`CoreModule`, including bodies, NodeIds, FileIds, spans and schema version. A
forged or stale `source_fingerprint` cannot replace that equality check.

CP32 reuses a retained module's source eligibility and weight receipt only after
that full environment and Core equality check. Changed or omitted modules still
run the complete source whitelist. This removes the previous second traversal
of unchanged bodies without trusting caller-provided fingerprints.

Memo entries contain raw `TypedFunction`s with their original Builder names.
CP32 records only FunctionIds, weights and budget receipts during construction.
After all ordinary functions exist, but before lending can mutate them, it
copies snapshots only for modules selected within the capacity. The temporary
project diagnostic names are restored to the original Core function names in
those snapshots. No speculative TypedFunction cache copy is made by `record`.
There is no insertion, decoding, deserialization or verification capability API.
Later lending/interface construction runs on the assembled current program.
Analyze and Verify retain their original driver identities and rules; a changed
project runs fresh Analyze, and every requested Verify remains independent.

## Conservative eligibility

The project must use primitive scalar function signatures and a bounded
whitelist of scalar source statements/expressions. Closures, templates, opaque
callables, resource/aggregate/enum declarations, addresses, arrays, receivers,
and unsupported source forms take the original path. Calls must name real
scalar declarations; the original names preflight prohibits rebinding resource
and callback constructors. Typed outputs receive a separate whitelist check.

Ordinary scalar `extern` declarations and lexical `unsafe` blocks may coexist
with eligible modules. A module containing either is always checked by its
original builders and retains no module entry. Its complete foreign ABI,
symbol, noescape indices and static-callback contract still participate in the
environment identity. Nonprimitive foreign signatures disable the project
memo. This boundary follows source semantics; there are no fixture-name tests.

Within this boundary, no builder creates closure/helper/resource identities or
mutates their shared tables. The global source write-effect table is empty for
primitive signatures. Ordinary calls depend on complete signatures and fixed
FunctionIds; their callees' bodies are checked separately and later analyzed
and verified. The remaining shared builder state is the six budget counters.

## Budget and failure rules

A successful builder records the exact changes to shared blocks, locals,
values, instructions, closure work and closure nodes. A hit first checks every
remaining allowance without mutating any counter. Only a receipt that entirely
fits is applied. Otherwise the original builder executes, preserving the
original interleaving of charges, first diagnostic and source span. Changing
any configured limit invalidates the environment as an additional safeguard.

The driver publishes a candidate memo after all requested later stages and
artifacts succeed. Types-only requests may retain their successful raw Types
epoch; that does not establish any later verification result. Errors and
cooperative cancellation leave the previous successful epoch intact.

## Capacity and observation

Each epoch is capped at 16,384 module positions and 64 MiB of conservative
structural storage accounting. The accounting includes target strings, retained
source nodes and names, complete signatures/imports/contracts, typed arrays and
per-allocation slack. The complete global environment and bounded working
receipt/vector reservations are accounted even for modules not retained.
CP32 selects whole modules in stable source order, skips a module that does not
fit, and continues to consider later modules. Only an oversized mandatory global
environment disables the whole request for capacity reasons. An oversized module
still receives complete source/output eligibility checks and ordinary Builder
checking; a forbidden language form still disables the whole optional epoch.

Accounting is not allocator RSS. The driver has at most its old
epoch and a candidate epoch in flight; adjacent unchanged modules share immutable
`Arc` storage. The original TypedProgram, current builders and raw query cache
remain separate costs and must be included in whole-request measurements.

The driver's explicit `CompilationSession` enables memo collection by default.
`set_type_memo_enabled(false)` drops its retained epoch and permits a same-build
A/B control. `without_type_memo()` and free one-shot `compile_path` /
`compile_project` wrappers avoid collecting an epoch that has no later request.
An explicit `clear()` clears both query and module caches. Automatic raw-query
capacity eviction only clears the original query cache, leaving this separately
bounded epoch available.

`type_memo_stats()` reports the last project request's actual builder reuse and
checking; a whole Types-query hit reports no builder work. The separate
`retained_type_memo_stats()` describes the last committed epoch. Neither count
is a verifier count. `cached_query_count()` retains its original meaning and
does not include this memo.

CP32 additionally reports `source_modules_reused`, `source_modules_checked`,
`capacity_skipped_modules` and `snapshot_cloned_functions`. The last counter
counts new retained snapshots; output clones on memo hits are separate costs.
An eligible request can retain a partial set or no modules. Conversely, default
current stats on a whole Types-query hit do not mean eligibility rejection.

## Author controls

Run `cargo test -p zil-driver --test cp29_types_memo`. Its twelve tests cover
source body edits and restoration, complete environment changes, all limits,
late cross-module budget exhaustion, raw same-span stale-fingerprint edits,
scalar foreign contracts and unsafe errors, conservative fallback, fresh
Analyze/Verify, cancellation/failure rollback, explicit clear/disable and raw
query eviction. The budget test requires the first failure to occur in the
unchanged later module for each of the four growth counters.

`cargo test -p zil-types --lib cp32` exercises private small capacity boundaries:
omitted middle modules do not discard earlier/later entries, unmodified entries
share their old receipts, exact quota errors remain in a later retained module,
and an oversized module's forbidden tail is still checked. This unit-level
capacity injection is private; production always uses 64 MiB and no CLI option
or language limit was added. `cargo test -p zil-driver --test
cp32_partial_types_memo` supplies 1,024 real scalar modules plus a root using the
production cap, compares cold/edited/restored Typed programs to fresh checking,
and separately checks source-receipt reuse with fresh Analyze/Verify.

These tests establish bounded semantic controls, not performance results or
whole-language acceptance. Native execution, RSS, complete workspace validation
and independent performance/review are integration work. No runtime ownership,
ABI, diagnostic policy, default limit or product gate is changed. Expression
and maintenance cost relative to Rust, whole-program speed, and a product-level
net breakthrough are not established by this implementation alone.
