# Combined position/interface miss — confirmed, frozen window unchanged

Read-only diagnosis of candidate `f19fc847c5774a2472bd3af85019b5c937546010`, binary SHA-256 `68867c4c0333cbceac34e6d688a9581702c7ef337b4ace34a88458df21e72e85`. Frozen source checkout `/workspace/zil-research-measure-sourcef19` was checked at that HEAD. No compiler, tests, fixture generation, or native execution; no repository/source/window writes. This external report is a proposal for a subsequent candidate, not a change to the active formal protocol or its results.

## Evidence and cause

Evidence root: `/workspace/zil-push-main/out/runs/native-dependency-full-window-01/workflow/1000/block-00/requests/`.

| Consumer | `011-positions` | `012-interface` |
| --- | ---: | ---: |
| Scalar relocation classifications | 102 | 100 |
| Completed frontend rebindings | 102 | 0 |
| Normalize / Restore / Lower modules | 0 / 0 / 0 | 102 / 102 / 102 |
| Executed Lower items | 0 | 1005 |
| CT shape / body checks | 1005 / 1005 | 1005 / 1005 |
| CT receipt-context matches | 0 | 0 |
| Backend reused / lowered functions | 1005 / 0 | 1003 / 2 |
| Interface partition reused / rebuilt items | — | 1769 / 823 |

Response SHA-256: positions `be9996b599e179d98dad080404d62ddb3772e09a06fdea0edf9b2bc376030083`; interface `836b4d2415dbdf11e610c688710fa9145b61401a0e813e72851a5cf8ed3f052a`. Current Verify executed once in both responses. Observed interface phase times: Expand 251.049 ms, CT 13.694 ms, codegen 376.178 ms. These are observations, not promised savings. The completed 2500/block-00 interface independently exhibits the same 100 classified / 0 rebound / 102 rebuilt pattern and CT 2505 full checks.

The declared history is decisive: `positions` has epoch 8 and `positions=True`; immediately following `interface` keeps epoch 8 but has `positions=False, interface=True`. It removes the comment prefix from every source while changing `m0.value` to accept `offset` and changing its matching main caller. This is not the isolated same-position signature edit covered by the new test.

Expand's two current outer attempt gates are `function_inputs_compatible OR scalar_products_compatible` (`incremental.rs:1065,1133`). The first requires equal global headers and fails on the signature. The second requires exact import spans and fails on the position reversal. Thus 100 valid Placement candidates never enter rebind. The broad receipt invalidation also fires, and item-plan admission fails, explaining the full rebuild. Profiling environment counters are disabled/zero in these responses; the predicate failure is established by source/history and code, not those zero counters.

CT has two independent misses. `same_plain_hint_environment` compares full imports including spans (`plain_session.rs:500`), so no prior epoch is selected. Even after narrowing that comparison, `validate_module` requires identical Core function Arc owners: `Placement::lower` correctly creates new current owners when spans move (`scalar_placement.rs:265`). The pure-position row already proves that Expand rebind success does not close CT reuse. Retaining a position-bearing old Core owner would be incorrect.

## Exact correction scope for the next candidate

**1. Expand: admit attempts under `namespace_compatible` at those two gates.** This subsumes both existing alternatives, but authorizes only an attempt. Keep these downstream obligations unchanged:

- Real Placement requires identical declaration order/bytes/attributes, source path/file/module and parser attribute context, with lexer-checked trivia gaps.
- Keep the exact `placement.imports(old_environment.imports[position]) == current_environment.imports[position]` check before rebind. Equal alias/module/file alone must not authorize an arbitrary current import diagnostic span.
- Rebind every syntax/Core/SourceMap position and validate relocated maps against current sources at the actual Lower allowance. Return current Core owners, imports and exports.
- Preserve exact-import `scalar_products_compatible` for blanket receipt retention, unchanged-item transfer, scalar inventory reuse and unrebased deferred recipes. Do not globally replace it with namespace equality: those paths have not performed the position mapping.
- Item plans still require current ordinal/span, full local source bytes, node/temporary/map cursors, current output owner and paid work. Modified main/m0 modules may rebuild; the 100 untouched declaration sets should rebind. Do not promise item reuse inside modified modules whose offsets/IDs changed.
- Preserve full-header public InputContext and independent current Types/Verify checks.

**2. CT: narrow only import lookup inputs, and supply an actual owner bridge.** Compare ordered import alias/module/file fields exhaustively; ignore only diagnostic start/end for hint-environment equality. Keep root, target, full user limits, module identity/order, all non-function inventories and exact live context checks. Imports are consumed by `SourceProgramContext::function` as alias-to-module bindings, not offsets. Current import/declaration diagnostics remain current. `checkpoint_for` must continue requiring complete current owners, fingerprints, imports and exports.

The owner bridge is the missing interface, not a two-line predicate fix. The existing raw-Core CT entry point must retain its exact-owner rule, including the test requiring a separately materialized equal body to execute. Do not replace that rule with name/hash/header equality or a caller-provided reusable bit.

A bounded complete solution uses existing `ProducedProgram`, `FunctionOwner` and `FunctionBodyRevision` products:

- Add an optional producer-aware CT entry point; keep the raw-Core entry point unchanged. CT currently has no zil-expand dependency, so the narrow direct dependency/API hook requires root-owned Cargo wiring, not merely edits to plain_session.rs. Expand has no CT dependency, so this need not create a cycle. No new IR or generic certificate system is needed.
- Expose the existing producer from `InferredProgram` only when inference borrowed that exact produced Core under the exact original inference limits. Its current `producer` field alone is insufficient: it can also be present with `Cow::Owned`. The driver must not pass the original producer for materialized/inferred output. Missing proof uses the old path.
- CT independently checks its live context's program is that immutable produced program. For an old receipt, bind its old function owner to the old produced Core; bind the current function owner to the current produced Core. Match stable FunctionOwner, module/ordinal/name/file, conservative current declaration ID, complete own header and FunctionBodyRevision. The revision is minted from complete declaration text equality plus real FunctionLowerBinding/generated-name inputs; it is not a digest or placement authority.
- Equal revisions can carry successful shape/body costs across a proven relocation while the returned raw Core and SourceMap remain current. Save the current owner/product with the new epoch only after every module and ordered project charge succeeds. Raw independent clones still miss. Changed ID/allocation/generated-name inputs conservatively recheck.
- Still compare every saved positive/negative/ambiguous lexical hint key against the independently built current header index. Still execute all current declarations, preserve full limits and ordered shape/Walker/project costs, and retry the complete original path for insufficiency/error/unknown shape. New producer input equality does not authorize changed callee hint fees.

Without this producer-to-inferred-Core-to-CT binding, report the remaining CT owner miss explicitly; import projection alone is incomplete. A generic full-body comparison ignoring spans would change the current exact-owner receipt contract and is not proposed as a silent replacement.

**3. Preserve downstream current checks.** Backend already lowers only the two changed functions in this interface response. Its source-projection counters still scan all 1005 functions because span edits replace block owners; that is a separate observed cost, not proof that backend function reuse failed. No whole-machine or partition identity should be weakened to fix a frontend miss. Retain the existing exact source-slot, callee-header, machine-value, import-closure, current preflight and independent Verify obligations.

## Combined regression and subsequent validation

Add a sequential real Source test, not two independent cold comparisons: baseline → private epoch 8 → prepend the exact matrix position comment to every source → remove those comments while adding a real parameter to one exported callee and updating its caller. Include imported unrelated modules and unchanged siblings. Keep one Expand/CT/inference session across all steps, and compare each full current Core, imports/exports, SourceMap and ordered CT usage against the original fresh pipeline. Exercise both relocation directions.

Assert the combined transition actually changes both global headers and import offsets, preserves namespace compatibility, yields rebinding for untouched modules (the full fixture has 100), and avoids all-module Normalize/Restore/Lower. For CT assert producer-bound reuse in untouched modules, fresh checks for edited/current-ID-changed functions and affected hint readers, current declaration walks for every module and one current independent Verify. Do not pin an unjustified global 'exactly two CT checks' count: raw ID shifts inside modified modules may require more.

Keep the existing wrong-current-import-span test in this combined transition: move an import diagnostic span to the function instead of its mapped import; only that module must lose rebind, and fresh output must agree. Also retain alias/module/file/context changes, independently cloned equal Core, missing→present/ambiguous bindings, generated discard/pipeline names and actual budget N/N−1 with a changed earlier module. These negatives must not be replaced by new success assertions.

After implementation and centralized gates, run a new immutable full-history window including the original positions→interface adjacency at 1000/2500/5105. The new six-request lifecycle harness omits this adjacency and cannot establish its fix. Preserve f19's current complete-window negatives and absolute timings; do not change its process, tools, source, fixtures, pins or artifacts. No speedup or Rust lead is established by this static proposal.
