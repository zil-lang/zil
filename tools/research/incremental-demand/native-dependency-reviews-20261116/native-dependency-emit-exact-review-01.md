# Exact dependency review

Result: **no blocking dependency omission found in the reviewed scalar paths**. This is a static review, not acceptance of the integrated native chain. Expand is an author re-review; backend emit is a non-author review. No source edits, builds, tests, native execution, formatting, or Git mutations were performed during this review. Only this report was written.

Reviewed shared `/workspace/zil-push-main`, `research/full-push-native`, HEAD `7be53850d4f95059a9fe3e73bba17db711bce81d`; backend `emit/incremental.rs` diff against that HEAD. Exact working-file SHA-256:

- `crates/zil-backend-llvm/src/emit/incremental.rs`: `723ca2cf18d532ec4531a13d3b18f92022166ddff383d88dd69dd1ef4f23ab4d`
- `crates/zil-expand/src/incremental.rs`: `ce1b39631ac9c88fb9134ced95f3fd7be6d731d506d9ad5e590c822bf9b2e88d`

## Expand: current Prepare/checker binding

The projection at `zil-expand/src/incremental.rs:409` is sufficient for these raw scalar products. It retains ordered module/file/path identity, exact imports including diagnostic spans, target, limit domain, source identity context, fused-stage mode, and the ordered declaration namespace. `namespace_headers:1350` retains names, visibility, function/extern kind, attributes, and declaration order. Add/delete/rename/reorder/visibility changes therefore invalidate the broad product path, including negative and shadowable namespace lookups. Full headers still bind the public `InputContext` at line 1161.

Actual consumers support this distinction:

- `modules.rs:1051` builds Index/Functions from declaration names, visibility, kind, generic slots, owner and imports; scalar admission excludes generic slots. `symbol:1518` reads that namespace and lexical binders, including misses/private symbols. Restore's `expr_name:2217` reads canonical owner/name and ordered import edges, not callee signatures.
- `normalize_scalar:1134` still calls `generics::validate_plain_products` at line 1199 before Restore/Lower. That function (`generics.rs:329`) runs current Prepare, then Lifetimes, then all headers, in separate original ledger order. Prepare (`generic_options.rs:265`) scans only each local item for Option/task triggers; Lifetimes (`lifetimes.rs:44`) restores item-local binders/header state. Primitive-only scalar admission excludes their cross-item feature effects.
- Replayed checks remain bound to the unchanged local parser revision, or exact local bytes/attributes/slot/spans on transfer. `scalar_normalized:1776` rebinds deferred recipes to the current parser/environment. Header replay at line 1872 requires the saved complete closed outcome plus a payable current receipt; changed local declarations execute their checks. This preserves current-input checks by exact-input replay, **not by re-executing every unchanged visitor**.
- Actual scalar Lower is `item_lowering.rs:319,390`: it uses empty nominal declarations and no project callee-signature index. Item replay at line 122 binds exact local bytes and current node/temporary/map/function positions. Current ordered Lower/map debits remain at lines 235/376; relocated maps take current-source validation. Rich/unknown syntax declines scalar admission and uses the conservative path.

Thus retaining unrelated normalized/restored/Core products across a remote scalar signature edit does not leave Prepare or header checks bound to the old signature. A newly non-scalar header cannot enter this projection.

## Backend emit: independent review

The new per-entry header binding is complete for the admitted consumer set:

- `emit/incremental.rs:217` compares the complete semantic current function input, with exhaustive field destructuring; immutable pointer equality only shortcuts complete input equality. Entry headers at line 622 include **self and every direct Call/ForeignCall target** collected by `eligible:165`. Full header equality at line 570 covers IDs, name, foreign declaration/symbol/ABI, parameter IDs/types, return/mode, file, contracts and environment. A callee body is intentionally not a header dependency.
- Current admission is rebuilt at line 410. Missing/ineligible/non-scalar/versioned/large-ABI targets decline cached lowering. Direct-call emission (`emit.rs:2872`) reads callee environment, return, parameter emptiness, foreign status and ABI; foreign emission (`emit/foreign.rs:55`) reads current symbol/result/target extensions. These inputs are covered by the saved headers, current scalar ABI admission, target and own CFG. Observed summary facts include absent values; summary/collection changes are separately compared before reuse.
- `same_environment:91` still binds full limits, target, entry and scalar policy plus `Globals:110`: schema/module, resources, closures, dyn/life inventories, task interfaces/workers/roles and project flags. Crucially, ordered `roles` at line 368 contains **every function ID** even when all flags are false. Add/delete/reorder therefore rebuild the EntryTable and clear products; excluding headers does not silently preserve a stale ID inventory. Backend calls resolve checked FunctionIds; source-name misses/shadowing are upstream obligations, not omitted backend lookups. Entry-wrapper name lookup is rebuilt currently before cached batches.
- `emit.rs:194` runs current metadata reservation and exact `bind_lir:989`, rebuilds copy/ABI plans and wrapper, then prepares the session. Cache hits call the same current reservation consumer (`incremental.rs:582`, `emit.rs:942`) with the original early exits. New output identities survive a miss only after complete FunctionBatch equality at line 596. Whole-machine history remains behind complete current module/function/layout/options/limit binding in `incremental/whole.rs`; this diff does not authorize a new whole-result shortcut.

Existing unrun backend tests cover name edits, scalar ABI edits, own-file changes, ID reassignment and inventory changes. They do not themselves establish foreign-symbol edits or failure/budget behavior after a header edit; those remain items for root's centralized integrated validation. No conclusion here substitutes for current independent Verify, full machine checking, O2, or native runtime acceptance.
