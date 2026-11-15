# Independent algorithm review: call classes and closed pruning

This records my read-only review of Astra's algorithms and the subsequent lint-only receipt-reuse reasoning. I authored the backend integration; this is **not independent review of my own integration**. Astra authored `machine_call_classes.rs` and `machine_closed_prune.rs`. For this archive note I reread the relevant algorithm paths and checked the current file hashes; I ran no tests, builds, native programs or Git commands and changed no repository files.

Current reviewed bytes in `/workspace/zil-push-main/crates/zil-backend-llvm/src/`:

| File | SHA256 |
| --- | --- |
| `machine_call_classes.rs` | `e0fd31dcb9dbed6fa0fe9e3e67fa2b9d4c05c16e07157639c2f5e63fda3cf615` |
| `machine_closed_prune.rs` | `27ce34401f6a4b0dbf5f8118c2a99101f0e349be45b196e203583464a41cd499` |

These are current physical-file identities, not an assertion that every earlier review used these exact post-format bytes. The requested frozen executable is source `027695318436fcb1f8994d0dc9802a0b2349c362`, binary SHA256 `5b3a309a7860dca3a1d3f15e7c930ec7d3f3a3fa59286e6d91c67e0067c0743f`, as declared in `out/runs/call-classes-pins-05.json`. I did not independently establish repository commit ancestry in this read-only task.

## Algorithm findings

Call classes use a restricted integer/direct-call domain with checked call signatures and a narrowly recognized intrinsic set. Generated names restrict eligibility; they do not establish equivalence. Protected entries and function-address uses, including global initializers and arguments to direct calls, exclude affected targets. Available-externally bodies are excluded. Unsupported operations and unresolved dependencies are declined. Bottom-up processing classifies only eligible DAGs; cycles and their dependent callers remain unclassified.

A normalized key clears only the function's own name and rewrites direct callees to already established representatives. Class authority is full `Function` equality, including signature, attributes, linkage and body; hashing only selects an equality lookup. Source-ordered processing within each DAG height makes representative choice deterministic. The published transform changes direct call targets while retaining all original definitions, names and address operands. COW staging and final cancellation checks prevent partial publication; unchanged bodies avoid needless mutable block access. I found no concrete algorithm correctness defect within this declared closed, prevalidated scalar scope.

Closed pruning runs on already internalized complete units. Every nonlocal function definition is a root, and **every retained global initializer**, including private globals, contributes roots. Reachability follows the model's structured operand walk through calls, addresses and aggregate constants. Only unreachable local function definitions can be deleted; globals, declarations, types and nonlocal definitions survive. A nonempty removal plan validates the complete original unpruned unit before mutation, so malformed dead bodies cannot be hidden. Validation errors retain their structured LLVM API type. Cancellation leaves the original module untouched, including during final staging. I found no concrete reachability or transactional-publication defect in these paths.

The integration invariants reviewed alongside these algorithms remain necessary: explicit closed + incremental-O2 + LLVM API O2 + merging-off scope; original-module preflight before a nonempty call rewrite; original-unit validation before pruning; later actual-unit preflight, Builder, full O2, LLVM verification and current language Verify. These are integration obligations, not properties proven by function equality alone. The partition-cache guard explicitly binds `closed_prune_removed.is_some() == call_classes_enabled`; it prevents pruned products and prior-work counters crossing into a disabled policy when other identities coincide. Unchanged warm reuse relies on the existing complete-input/options binding, not a weaker source subset.

## 027 lint-only change and receipt reuse

The reviewed `dependencies` lookup now reads `let &callee = names.get(name.as_str())?;`. Its predecessor's `let Some(&callee) = ... else { return None; };` has the same behavior: one lookup, extraction of the same index on `Some`, and immediate `None` return on absence. It changes neither eligibility nor rewriting. This supports reusing the earlier semantic-test evidence for that exact edit; it does not certify unrelated changes or a new executable's bytes.

Root reported the combined-chain gates as backend 291, driver 219, Source 9 and publication 1 passing before the strict `question_mark` lint repair. Those are root-owned receipts, not tests I executed or independently replayed. Root retained responsibility for strict lint, formatting, release and the actual pinned-artifact probes after the repair. No new tests or successful reruns are claimed here.

## Limits

This is bounded source review, not a formal semantic proof or universal ABI/effect analysis. It relies on the existing complete-model validation, explicit closed-world contract and current downstream checks. No open-world interposition, arbitrary callbacks/globals, pointer-product optimization or native-capture experiment is admitted by this review. Root's measured results and archived quality/runtime gates determine acceptance. I did not read a completed window05 runtime summary for this note, so it makes no new 51-case runtime or whole-workflow performance claim.
