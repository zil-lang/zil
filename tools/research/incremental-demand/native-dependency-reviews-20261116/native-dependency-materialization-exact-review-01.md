# Native dependency/materialization exact review 01

Read-only independent source review, 2026-11-15 UTC. No source edits, Git commands, builds, compiler invocations, or test execution. This report is the only written file. Root was editing concurrently; findings below identify the exact expressions observed, rather than claiming an immutable checkout review.

## Disposition

The item materialization and per-unit prune reuse paths have no identified stale-value correctness defect in this inspection. Two findings remain: a newly added whole-partition shortcut omits a real source dependency, and a materialization counter measures output equality rather than work actually reused. The shortcut finding was reported immediately in chat. No miscompile is claimed from this source review.

## Finding 1 — P1: completed-output shortcut omits source-call dependencies

Observed at `crates/zil-backend-llvm/src/incremental.rs:349`:

```rust
self.same_recipe_environment(other) && self.output.same_values(&other.output)
```

`same_recipe_environment` checks options, stable names, collected status, closure function IDs, and the scalar context owner. It deliberately ignores BOTH function-product vectors. In `build.rs:532`, this new predicate is accepted as an alternative to complete `PartitionInput` equality when call classes are enabled, returning the entire old partition before running the current planner.

The missing dependency is observable in `machine_cgu.rs:80`: `small_caller_components` scans current LIR blocks/instructions to identify source callers even when their physical machine calls disappeared. That decision can change grouping and owner selection. The source half of `FunctionProductIdentity` exists precisely to distinguish the ordered source-call projection; `emit/incremental.rs:601` gives it a different owner when the slots change. Equal completed machine values do not establish equality of that source input.

Concrete source-level trigger, not an executed reproducer: change a scalar function from calling an effect-free constant-returning callee to directly returning the same constant. Scalar summaries can produce identical physical machine definitions before and after. Keep all headers/files, names, entry, options and globals identical. The scalar context owner remains identical, but the source-call slots and their product identity change. The new shortcut accepts the old partition anyway. With enough unrelated leaf cost to satisfy the existing small-caller thresholds, and the caller's ordinary file bucket different from the entry bucket, the fresh planner can remove that caller from the selected caller group and choose a different owner. The cached shortcut skips that current decision.

Impact: the predicate is not a complete current-input proof for partition reuse. It violates the requested source-projection/current-planner contract and can hide a change in grouping/import work and its counters. This inspection has not established a semantic miscompile: an older grouping can still be a valid partition of identical physical machine values.

Required closure: additionally bind the existing source-product owners (or an equally complete current source-planning projection) before the completed-output shortcut; alternatively establish current planner equivalence before accepting the old plan. Keeping only machine-value equality and scalar context identity is insufficient. This is specific to the whole-partition shortcut, not the per-item materialization predicate.

## Finding 2 — P2: equal-unit counts are labeled as reused materialization

`machine_cgu/materialization.rs:33` counts positional full item/triple equality in `same_units`. `build.rs:683` assigns that number to `reused_partition_units`, and `build.rs:709` derives `rebuilt_partition_units` by subtraction. This does not count the reuse branches actually taken in `Layout::replay_reusing`.

For example, retain two physically identical unit item sequences but change both current layout owners, preserving their order and unit count, with no imported private symbols in those units. Moving source file buckets can do this without changing machine items. `Reuse::item` correctly rejects every old item because `input.owner != owner` (`materialization.rs:78`), so the replay reconstructs the units. `same_units` can nevertheless return two, reporting both as reused and zero as rebuilt. Prototypes, preludes, item distribution, allocations and the current planner also run even on ordinary successful body reuse.

The equality count is useful as "unchanged raw unit outputs". It is not evidence that two whole units skipped materialization or planning. Preserve that distinction in the counter meaning, or derive reuse counters from the actual reuse branches. This finding concerns reporting, not stale generated code.

## Checks that support the materialization/prune paths

- **Current layout authority.** `partition_view_cached_impl` obtains the layout from the existing current-key/validated disk path or the ordinary current global planner. Materialization itself never supplies layout/ownership. A rejected complete-copy budget reruns the ordinary no-import planner before replay. Resource-helper and single-module/fallback paths do not manufacture materialization records.
- **Full current items.** `Materialization::bind` compares each current source item to the complete old item at that position, including function name, linkage, signature, attributes, blocks, operands, globals and types. A moved/changed item conservatively misses. Old positional indices or names alone do not authorize reuse.
- **Roles and owners.** Lookup requires the same current owner and an old slot for the same `(source index, role)`. Changing between canonical definition and private import cannot reuse the other role's item. Slot construction matches replay order: prelude, declarations, import declaration/body pairs, definitions. Changing the unit count disables reuse; changing an owner disables reuse for that unit.
- **Added/removed import targets.** The union of old and current canonical-name keys detects absent-to-present, present-to-absent, and private-name changes. An equal caller is rejected if any direct callee is in that changed set. An imported body's own private name is checked too. The old direct-call set is valid for this check because complete source-item equality was already established.
- **Canonical addresses.** `rewrite_direct_calls` changes only `Op::Call`'s direct `Val::Global` callee. Function addresses in arguments, returns, aggregate constants, globals and other operands retain the canonical name. Current import prototypes are emitted even when their body is reused. Current planner reference/local-component and collision rules remain in force.
- **Signatures/prototypes.** Prototypes and preludes are freshly taken from current items in every replay. Changed function signatures invalidate source-item reuse for that definition/import, and their current declarations reach all current consumers. An unchanged caller may retain identical raw instructions referencing that symbol; complete current unit validation, rather than a cached ABI assertion, remains responsible for call/declaration consistency.
- **Layout changes after global planning.** Changed membership/imports are handled by the new layout, current clone-name map, owner/role predicates and current prototype generation. No whole old materialized unit is substituted by the item path. Removed source items cannot be selected by a valid current layout.
- **Unpruned materialization owners.** `Materialization::new` receives partition results inside the CGU planner, before `build.rs` internalizes or prunes them. `SharedVec` mutation uses `Arc::make_mut`, so later linkage rewriting/pruning cannot mutate the retained source/unit owners.
- **Current linkage before prune reuse.** `build.rs` re-runs closed-executable linkage from all current units and current LIR, followed by closure linkage. Only then does it snapshot the complete original unit in `PruneProduct.inputs`. It compares the current triple and all original items before substituting `old.units[index]`; fingerprint replacement is diagnostic-only. Added addresses, global references, prototypes or changed linkage therefore invalidate prune reuse wherever they affect that complete unit.
- **First production check.** On a fresh nonempty deletion, `machine_closed_prune::prune` validates the COMPLETE original unit before replacing its item array. Failure/cancellation leaves that unit unpruned and aborts the current build path. Cached deletion results therefore originate after original-input validation, rather than from an unchecked pruned-only model. A zero-deletion result removes nothing and is still subject to ordinary current native preflight.
- **Retained prune counters.** Fresh nonempty original checks/removals and exact-input reused units/prior removal counts are separate. Cached removals accumulate into `retained_removed`, not fresh removals. Whole-partition hits likewise report zero fresh original checks/removals and retain the saved removal total. `checked` is not a count of every prune traversal or ordinary native preflight.
- **Native authorization remains current.** Object-cache hits use `validate_module_with_receipt` on the current unit before object publication; fresh production routes retain normal current preflight/Builder/O2/LLVM verification. Final-product reuse independently calls `final_product::authorize`. Materialization/prune records remain raw process-owned values, not checker capabilities.

## Coverage and remaining limits

No test execution or compile check was performed, as requested. Existing CGU tests cover canonical addresses, declarations, copy limits and layout replay. No direct regression calling `partition_view_cached_reusing` or `Materialization::bind` was present in the inspected test sources. The new owner/import-name/added-target cases and exact-original prune replay therefore still need root's concentrated validation; static reasoning is not an execution receipt. The source-call/machine-equal shortcut case and owner-change counter case above are especially discriminating regressions.

The existing whole-transform lookup still uses full scalar identity including context. Header-only edits can therefore miss it legitimately. This review does not recommend removing context or source dependencies from that lookup. The per-function header batch remains separate and untouched.

## Actual Rust reference

Read the supplied 58cf checkout at `/workspace/zil-recovery/rust-pain-rust-source-20261116`:

- `compiler/rustc_query_impl/src/execution.rs:418`: dependency-green lookup before provider execution, otherwise `with_task` records provider dependencies. This supports reusing exact consumed results, not dropping source inputs after machine equality.
- `compiler/rustc_codegen_ssa/src/base.rs:1238`: `determine_cgu_reuse` requires an available work product and green CGU dependencies; the decision distinguishes pre/post-LTO reuse.
- `compiler/rustc_codegen_llvm/src/base.rs:64`: the CGU task wraps `module_codegen`; `ModuleLlvm::new` occurs after the reuse decision. ZIL's reviewed materialization still runs the current planner and complete raw item comparisons, so this is a narrower construction reuse and not Rust-style avoidance of all green CGU production.

ZIL's full raw equality/current independent checks are intentionally stronger than treating Rust's stable hashes as checker authority. Import-affected final outputs must remain bound to the actual complete current unit, not merely its canonical owner name.

## Reviewed live-source fingerprints

SHA-256 recorded after the above expressions were read. Root may continue editing; these are not Git identities or a promise that later source is unchanged.

```text
b96871c2773d801c6ab4fcdc162a3147a38c1446e59afc8a1ac2d048e9dce117  build.rs
f51275d7fc714c27e68698decd167e704502b51d385ea04df5ea1b8cadbd1447  incremental.rs
74816462542f7591f5ca726f623485aa37fee2318d28aac9ccd29be9f03b5ff8  machine_cgu.rs
be41c663c4d050f6534e0171a9be265f1d3e83faac3972113264e9b969554648  machine_cgu/layout.rs
3396e4f77844874e26daa53980ea5e3a94c9e30903c712678354de928acd3256  machine_cgu/materialization.rs
3d609fc124d92e2216c625fb3e79e4241e444dde519c12da5a0e1ce5b2d22ee0  machine_output.rs
```
