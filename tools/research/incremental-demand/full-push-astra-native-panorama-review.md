# Native panorama review — bounded architecture decision

Reviewer: non-author `/root/astra_architecture`. Read-only source/evidence review; no Cargo/Clang build, no generated-code execution, no new timing. Only this `/tmp` receipt is written. Read AGENTS.md, goal/workflow, panorama, formal433 review, diagnostic5bf, scope contract, native producers/consumers and fixed LLVM field/AsmPrinter consumers. Cloud runtime skill was read and environment identity confirmed. A transient exec transport disconnect recovered on the next read.

Source reference: `368ea509b6c8227f0c45fd68ffbde5b82e6b1fd5` in `/workspace/zil-push-main`, including Frame `2dd8a991ffd492cc63741ae241ff200cc3a55cae`. Initial root was9daad; root advanced during review. This receipt does not independently accept later c356/current changes or their uncompleted native gate. Fixed LLVM `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`. Full default9daad 33scope/6375 pass is root-reported recovery, not executed here; original ENOSPC failure remains.

## Decision

The direction is not structurally wrong: real fine producers already stop at equal Typed/Analyze/Verify/machine/object results, and direct consumer connections can implement push without one central graph. The unresolved problem is **the cost of proving and reconstructing reuse versus the work removed**, plus aggregate context work that is still actually consumed. Renaming PrivateImplementation or replacing demand with push would not remove those costs. `session.rs:577-650` still keys whole phases by construction identity, but the downstream 1/424 work observations prove that whole-phase entry is not equivalent to whole-function recomputation.

I most disagree with **making broader per-FPM-stage cache admission the default next investment before showing its net cost**. These are original FunctionPassManager groups, not necessarily single passes. More admitted groups can add more full input/world projection, output capture, owner reconstruction and analysis-state work. Those operations are implementation costs, not inherent requirements of incremental compilation. Keeping their complete semantics does not require retaining their current representation.

No reviewed design is yet proved substantially faster than Rust over the requested workload. Formal433 private640.807/Rust704.436 is a limited win; cold9802.708/Rust6506.933, broad8674.107/Rust5804.026 and recovery1026.862/Rust507.488 remain negatives. Cargo-process/resident, LLVM versions, checks and actual CGU membership differ. New Frame has no fresh formal ratio.

## What is proved, and what is not

At fixed368, `work_products.cpp:842-897` builds eligibility, exact analysis facts, pipeline, complete function input and world before selecting a prior product. It unpacks **the entire old StageStrings**, including output bitcode/text/declarations, before comparing entry values. `packed_stage.h:14-16`, `work_products.cpp:599-606` establish that representation. A broad-edit miss therefore still pays old output decoding when its slot exists.

`work_products.cpp:749-840` proves the changed-body replay path parses bitcode, validates the parsed module, rebinds declarations, clears the current FAM, deletes/clones the body, restores numbering/use/name state, compares complete output and reconstructs/checks exit analyses. `restoreStageAnalyses:715-747` eagerly requires every retained exit key. `afterStage:920-942` projects the world/output/facts again, then snapshots and packs a new retained record. BodyUnchanged already avoids body transport; do not propose it as new work.

Profile07 raw response JSON has **observer_schema2**, not3. Thus its236.406ms private optimizer total does **not** identify IR parse/key/unpack/replay/capture proportions. MIR timers are separate and nested. Reading the original raw responses gives:

| Scene | IR key bytes | IR snapshot bytes | IR replay bytes | Analysis reconstructions | MIR key / reconstruct / validate ms |
|---|---:|---:|---:|---:|---|
| cold | 52,974,230 | 22,936,810 | 0 | 0 | 154.021 / 0 / 0 |
| private | 5,382,232 | 57,079 | 460,040 | 507 | 15.191 / 19.956 / 23.846 |
| broad | 52,976,768 | 22,976,379 | 0 | 0 | 171.398 / 0 / 0 |
| recovery | 5,382,179 | 57,383 | 460,040 | 507 | 19.502 / 19.210 / 27.976 |

Private/recovery each78 reused FPM groups,300 executed groups,39 reused MF,3 executed MF. Cold/broad each3816 executed groups/424 executed MF. Those counts prove eliminated operations and retained overhead, not elapsed-time attribution. Do not add nested parse to replay, pack to capture, or MIR timers to native totals.

## Three implementation slices, in order

### 1. Finish the existing Frame slice at the actual consumer; do not widen it further first

The common closed-executable diagnostic provides the strongest concrete opportunity: unchanged `zil.fn.361` has identical25,480 machine bytes yet reruns because its ordinary spill/frame/CFI product was rejected. Current Frame removes that actual admission obstacle without changing O2. It is already implemented and is the smallest distance to a real consumer saving. Rank is by evidence/readiness, not an asserted speedup.

Keep `machine_products.cpp::snapshotDomain`, `frame::capture`, `restoreBody`, `beforeMachine` and the existing fresh-attempt fallback as one frozen slice. The outcome to inspect is: a real prior361 snapshot exists; the current changed request selects and restores it; original machine passes are actually skipped; current AsmPrinter emits it; complete objects/ELF match ordinary same-policy O2. No new toy replacement of425, no forced noinline, no cold snapshot zero described as invalidation. If its capture/key/reconstruction cost exceeds removed codegen, that is a negative, even if MF reuse rises.

The added uwtable case should consume an actual nonempty `.eh_frame` section and preserve complete object equality, because CFI instruction count alone is not emission. Root is already adding this to the unified full-slice gate. No per-getter reruns are recommended.

### 2. Separate Stage candidate selection from Stage payload transport

Change `StageState/PackedStage`, `unpackStage`, `retainStage`, `beforeStage` in `work_products.cpp`, and the packing schema in `packed_stage.{h,cpp}` as a single internal representation slice.

First, add a small selector derived from **all existing entry fields**: lengths plus a deterministic cheap content digest of Input, World, Pipeline and EntryFacts; retain typed EntryKeys for exact comparison. On selector inequality, execute the original group without decoding the old record. On equality, keep the existing full decode, full byte equality, typed-key equality and every replay check. A digest is only a negative filter; collisions trigger the old exact path, never authorize reuse. The stored selector is created with the sealed product, not inferred from a historical address.

This certainly deletes whole-output/bitcode/declaration decompression on rejected broad/private slots. It does not claim to delete current key construction or hit-side IR parsing. Its cost can be negative if hashing exceeds avoided decode; schema3 reports the actual boundary.

Then factor immutable key fragments so exact-equal World/Pipeline/facts can be stored/packed once: separate module-header bytes from subject caller/use bytes in `worldText:424-478`, and intern only after complete equality (hash just selects a candidate). Pipeline text can be memoized per live PM instance within the same invocation; retained identity remains complete text. Preserve exact header metadata, body-presence, incoming caller bodies/use order, named types, analysis fields and existing caps. Merely interning results does **not** make printing mutable LLVM state free. Do not memoize world by Module*/Function* or PA::all: function passes can mutate headers and users. This limited slice deletes duplicate representation/codec work, not the proof itself.

This has lower semantic risk than typed IR transport or a resident-context rewrite. Do not initiate either larger rewrite before the current schema3 whole-slice measurement shows transport is the problem.

### 3. Make optional FPM retention/replay pay for itself before expanding its domain

After the unified schema3 run, use complete original FPM groups as the decision unit. Add an internal product-policy record next to the slot/PM identity, covering original execution cost and capture/key/replay cost. The policy may decline optional recording before `captureStageBody/snapshot/retainStage`, or choose original group execution before replay. It never skips an O2 pass without the existing complete reuse proof and never changes the pipeline, budgets, checks or result oracle. All declines remain original fresh execution. Do not encode fixture names, function numbers or scene labels in this policy.

The first implementation can be an explicit research policy selecting demonstrated high-payoff groups and leaving other groups fresh; one frozen full-slice measurement decides it. A later general adaptive rule can use observed work sizes/costs. The purpose is to remove snapshot+codec work for groups that are cheaper to execute than to retain/replay. Existing callbacks must not do the expensive work merely to discover that a group is declined.

This is an economic stop-loss, **not a claim that complete push is done**. If nearly every admitted group loses, replace the boundary rather than perpetually micro-optimizing its serialization. Conversely, if key dominates, widening analysis support or typed-body replay will miss the bottleneck. If parse/replay dominates on large transformations, only then evaluate an owned typed IR snapshot or same-context clone design with a complete context-lifetime/metadata bound. These are alternatives to be selected by current costs, not an additional immediate plan.

## Whole optimized region, stable fine CGU, or function backend product?

| Choice | Current judgement |
|---|---|
| Whole optimized existing CGU | Sound with complete current input/config, and already effectively provided by the unchanged object/product path. The private changed CGU has42 definitions; caching its whole optimized output cannot hit when one member changed. Adding another cache at that boundary alone does not solve the observed problem. |
| Whole optimized function across all O2 | Attractive because it would avoid transient FAM replay, but current proof is insufficient. Module/CGSCC work, incoming callers, attributes, inlining and deletion can change it. Current functionText/world for a local stage is not a certificate for the entire O2 pipeline. A region must include its real optimization/read closure and membership changes, including caller/inlining dependencies. Do not join FPM stages across a module/CGSCC barrier by function-name equality. |
| Stable finer CGU | A stable ownership/key mechanism is useful, but changing compilation units changes the optimization problem. `machine_cgu/recipe.rs:35-109` already reuses topology and replaces only changed definitions, and explicitly rejects import-bearing layouts. `layout.rs:40-126` shows imported definitions/direct-call rewriting are real dependencies. More/finer units without preserving those imports/cross-unit quality are not a free win. Sixteen units on each compiler are not equal contents. This is a separate code-quality/partition experiment, not the next small fix. |
| Function backend products | Best supported near-term boundary: post-optimizer current function input is explicit, final MF is the actual AsmPrinter consumer, and Frame targets a real missed unchanged function. It leaves optimization and MC packaging work, so it alone cannot prove the final goal. Caching MC fragments would need actual section/symbol/fixup/CFI/layout closure; raw function byte concatenation is not sound. |

Longer-term the useful larger region boundary is one whose **analysis lifetime naturally ends**, so the consumer needs optimized IR/output but not an emulated transient FAM cache. Existing whole LLVMRunPasses/CGU is such a boundary; a smaller one requires a real region contract. Adding an arbitrary FAM.clear to manufacture the boundary may alter getCachedResult-dependent passes and is not accepted as preserving original O2. Likewise, recording only analyses requested by the skipped stage is insufficient unless negative cached queries, invalidation and later consumers are represented. No simple safe whole-O2-function shortcut was found.

From the panorama, independent Verify receipts and backend1/424 are already valuable. Remaining whole signature/global obligations and current source/context binding are real targets, but not a reason to abandon them or replace Verify with Analyze. Link discovery also has a measured roughly80ms private opportunity, but the current research trace is not a complete authority for caching clang/GCC/loader/config/negative-search behavior. Do not rank a blind stdout cache ahead of a slice with a closed dependency proof.

## Frame owner and ordering review

No new blocking owner omission was found in the inspected fixed scope. This is bounded static review, not native acceptance.

* Fixed `MachineFrameInfo.h:120-356` contains value fields/containers plus StackObject.Alloca and SaveRestorePoints MBB-pointer keys. Full public assignment preserves hidden flags, offsets, fixed-negative IDs/dead holes, CSI and local layout. `machine_frame_products.h:110-120` checks allocation owners for every slot, including dead slots. Capture clears both owner maps in the retained copy; production `machine_products.cpp:386-393` additionally rejects nonempty maps, consistent with post-PEI. Retain that production guard: sorting maps by ordinal would not preserve DenseMap iteration order.
* `restoreBody:570-612` creates all current MBBs, restores full frame/ordered CFI table, and only then creates MI FI/CFI/MMO consumers. `restoreMemory:232-251` obtains current PSV-manager Stack/FixedStack owners and retains full LLT/offset/address-space/StackID/flags/alignment/ordering. Current FI table is authoritative, including negative IDs and holes. Non-stack/foreign PSV, IR/metadata/AA owners remain excluded.
* CFI is consumed by actual MI position and `AsmPrinter::emitCFIInstruction` (`llvm/.../AsmPrinter.cpp:1421-1445`), which skips final transient-only CFI and checks the current function's CFI section policy. `getFunctionCFISectionType:1389-1410` reads current declaration/linkage/uwtable/needsUnwindTableEntry and target policy. Current function/world config plus restored BB/MI order are therefore essential; restoring only the CFI vector is insufficient. Existing inputs and typed reconstruction cover these dependencies in the admitted domain.
* `beforeMachine:1260-1323` restores X86 target state, sparse BB numbering/emission ordering/properties, recaptures typed body, rechecks domain, runs current MachineVerifier and compares complete MIR before reuse success. The public MFI value copy, rather than MIR equality alone, carries private frame fields. A failed restoration must still abort the attempt and rebuild in a fresh context; never let partial frame/MI mutation reach AsmPrinter.
* AsmPrinter stack-size/usage consumers (`AsmPrinter.cpp:1698-1750`) use full current frame stack size/unsafe-stack state. The complete assignment preserves these, while unsafe-stack is excluded. If target options/CFI policy become configurable beyond the fixed adapter config, bind the actual new option before widening admission.

Nearest concrete counterexamples to retain at the full-slice boundary: same valid FI but a foreign PSV owner; an old Alloca hidden in a dead slot; same CFI count with changed ordered operands/CFI policy; nonempty post-PEI save/restore maps; an owner-bearing symbol/location in CFI. Existing helper/consumer tests cover most of these. This is not a request for a new test per field; finish and validate the single frozen feature.

## Source hashes and evidence boundary

At368ea509b:

* work_products.cpp SHA256 `8d6eaeb5ac06d95d82ebebfe2046c1e5176c6454159eff76dc851820290d5bfb`
* machine_products.cpp `f4b218de9bfe69beff2c85ac66cc0ac4460b643e6e9dd8b71a5c40ef8b2c338c`
* machine_frame_products.h `764c36eb8060ee34d8a795af8769d4b42610e6e55cab7b4636f741092a37126b`
* analysis_facts.cpp `e19b624b3147b1ad67d504eb91cc42ccc58eda4984d1a4aa8e8aa08f98fdebe2`

Fixed LLVM local MachineFrameInfo.h SHA256 `823e195126a95076d2f666142f18721e28eff422256e1ba714d87c534340c886`; AsmPrinter.cpp `708823dcb12c6e2b9a6c3fc8fa1a18c96278bcc435167800d91435d4e48d16e0`. Prior independent Frame receipt `/tmp/full-push-spill-frame-pre-review.json` was read; it is corroboration, not a substitute for actual consumers above. Raw timing counts were read from `out/runs/full-push-native-profile-07/block-0-candidate/{0-cold,2-private,4-broad,7-recovery}.response.json` and are schema2 only.

Research switches remain default-off; no merge is recommended or performed. All suggestions preserve independent Verify, default budgets, original O2 execution for misses, current complete equality and fresh fallback. Unified measurement after a complete slice is the decision point; no microscopic test/build sequence is requested.
