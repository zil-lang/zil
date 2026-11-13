# Fixed-source architecture decision: native emission products

Reviewer: independent /root/astra_architecture. Read-only design review; no build, generated-code execution or new performance measurement. Reviewed ZIL 1e260a4ed8898e1ce92ae6f0b7931b47d4dceb7d and LLVM 7a941dfb2877a0b8cf30323fefa85ad019e0b0f5. Working-tree process notes were present and not edited. This is a design decision, not implementation acceptance.

## Decision and actual benefit boundary

The current external MIR snapshot is a correct, conservative compatibility layer. It cannot eliminate MachineFunction reconstruction while continuing to call the unmodified AsmPrinter on an ordinary current MachineFunction: the consumer requires the MI/MBB/MRI owner graph. Retaining an old MF with a keepalive, rebinding its Function pointer, or treating old register-use links as current does not close this boundary.

The implementable more fundamental boundary is an LLVM-owned immutable function emission product: the original backend and AsmPrinter produce a sealed sequence of MC-level operations, current-owner symbol recipes and explicit function-emission state effects. A hit binds that product to the current optimized input/config and current emission environment, then replays it into the current MCContext/MCStreamer. The original MC encoding, layout, relaxation, fixups, CFI frame production and object writer still execute. This requires a pinned LLVM internal interface; it is not achievable by adding another external snapshot codec or a partial MCStreamer proxy. It does not require an upstream merge before research implementation.

This is NOT the next automatically authorized implementation. The current AU sharing, CT gap and link-component slices should finish their unified gate first. An emission-product prototype is a separate complete feature because the AsmPrinter/MCContext/handler state boundary below is not yet implemented or independently proved.

Parent-reported formal private scopes: optimize159ms, emission82ms, MIR reconstruction17.6ms and validation22.9ms; native total449ms includes object306ms, link key85ms, link execution30ms, machine17ms, diagnostic8ms (these scopes overlap/nest as documented). Removing all current MIR reconstruction plus validation has a gross ceiling around40.5ms for this request, before product binding/replay costs. It is not449ms or the full82ms emission cost. MC encoding/layout/object output remain. Potential additional AsmPrinter lowering savings are unmeasured. Cold MIR capture452ms and key136ms are observed costs; an MC product would replace some capture work, not make all452ms disappear for free. It must still record and seal its own product, and current input binding remains. No substantial/global Rust win follows from this design.

## Evidence for the boundary

ZIL machine_products.cpp:1490-1578 binds complete input/world/config/pipeline, constructs the current MF through tiny MIR header plus restoreBody, restores target/number/emission state, recaptures the typed body, rechecks domain, runs MachineVerifier, prints complete MIR and compares it. At1596-1664 cold recording captures frame/body/target/numbers/full MIR and then always calls the real AsmPrinter. Those checks are transport checks required by this representation, not an intrinsic requirement to rebuild every mutable backend data structure whenever an unchanged emission result is consumed.

LLVM MachineModuleInfo.cpp:78-107 owns the Function*→MachineFunction map and constructs MFs with current TargetMachine, subtarget, MCContext and NextFnNum. MachineFunctionPass.cpp:47-100 obtains that current MF before running a pass. X86AsmPrinter.cpp:79-117 reads current profile/subtarget/module state, sets up function state and emits the function. AsmPrinter.cpp:3181-3240 mutates HasSplitStack/HasNoSplitStack, current symbol pointers, section ranges and ORE state;1016-1155 emits headers and begins EH handlers;2052-2535 lowers instructions and emits tail/size/EH/metadata effects. These are actual consumers, not hypothetical fields.

MCObjectStreamer.cpp:410-495 decides instruction relaxation and creates current fragments/fixups. Code-alignment685-704 remains a symbolic alignment operation with max-padding/subtarget information, not fixed padding bytes. MCStreamer.cpp:428-459 owns CFI frame-stack and frame-info mutations; emitCFIStartProc/EndProc are not virtual in MCStreamer.h:1022-1023. A proxy overriding only emitInstruction/emitBytes cannot observe or reproduce this complete effect.

DwarfCFIException.cpp:39-60 has module-final personality state;69-163 derives function CFI decisions and mutates hasEmittedCFISections before start/end-proc events. The current admitted no-personality/no-EH domain removes personality and LSDA work but does not remove ordinary uwtable/CFI state. MCContext.cpp:365-398 allocates temporary symbols through current naming state. Object equivalence depends on creating and binding symbols in the correct order even when retained code is reused.

## Concrete proposed interface

Use an opaque, immutable, process-local FunctionEmissionProduct owned by the fixed LLVM build. ZIL may retain/share/release it but cannot manufacture a valid one. A product contains values and typed indices only; no MF, MI, MBB, Function, BasicBlock, MCContext, MCSymbol, MCExpr, section, handler or subtarget pointer escapes the producing request.

The API boundary needs four operations, with exact names left to implementation:

1. beginFunctionProduct(current input binding, current pipeline and emission configuration): opens an optional recording while the ordinary original machine pipeline still executes. Current scope/domain and all existing input/world/pipeline checks remain. Original language Verify, native preflight and LLVM IR verification still run on current inputs.
2. sealFunctionProduct(current verified MF, ordinary AsmPrinter completion): produces a private immutable value only after the original producer and entire function emission successfully finish. A current MF verifier check at this producer boundary establishes the backend representation being lowered. Record/seal failure merely declines the optional product; ordinary cold emission already performed remains authoritative.
3. prepareFunctionProductReplay(current Function, MMI, AsmPrinter/MC state, product): checks complete current input/world/config/pipeline plus the explicit emission domain, resolves all external bindings, and obtains a current function-emission identity. No output bytes may be emitted during this preflight. Unknown operations/state or binding mismatch decline before replay.
4. replayFunctionProduct(current emission host, prepared binding, product): applies the original MC operations and declared state effects in their original order. If any failure occurs after output or MC state is mutated, abort the entire native attempt and use the existing fresh-context retry; never resume ordinary emission inside a partly changed streamer. Publish products only after the whole native/link result succeeds, following current session publication rules.

For a hit, the original machine-codegen region can be bypassed only through a compiler-owned region contract: its output is this emission transaction and declared state effects. Full current pass ordering/AU binding remains. The new interface must account for non-machine passes and providers in that region; it must not infer that every pass with an analysis ID is harmless. The present providers() closure is sufficient for restored-MF emission, not automatically sufficient to delete all MF-producing providers. Unsupported additional consumers retain the existing MIR-product/original pipeline. Current module initialization, finalization and global emission remain fresh.

## Product contents and closure

Initial domain must cover the existing real x86-64 Linux integer/overflow/trap, normal spills/frames and ordinary CFI workload; do not replace the425 benchmark or remove its pressure functions. Keep present exclusions for calls, inline assembly, EH/personality/LSDA, owner-bearing metadata, jump tables and constant pools unless a separately proved product representation covers them. Additional instrumentation/debug/split-stack/target-streamer modes may conservatively decline this new product. Decline must preserve ordinary compilation and existing MIR fallback.

Required value contents:

- Original ordered MC instructions: exact opcodes, flags and typed register/immediate/expression operands, bound to the actual current subtarget configuration. Do not store post-relaxation bytes as the sole instruction result.
- Ordered section switches, labels, symbol attributes/visibility/linkage, assignments, ELF size expressions, alignment/max-padding, bytes and CFI semantic operations actually produced by the admitted path. Store complete expression variants admitted by the domain, including symbol-ref variants and add/sub expressions. Unknown target expressions decline recording.
- Symbol roles and creation recipes: current canonical function/global binding; per-product local ordinal; exact temporary-symbol creation operation/prefix/suffix policy and occurrence order. Resolve into current MCContext objects. Do not copy old MCSymbol* or old generated names. Current function-number allocation must advance exactly as fresh MachineModuleInfo creation would, even if no MF is reconstructed; expose this as a compiler-owned function-emission identity operation.
- Explicit AsmPrinter and EH-handler effects, or refactored shared begin/end-function operations taking an owner-free FunctionEmissionInfo. In this domain these include current function-symbol/size/end bindings, section/alignment choices, HasNoSplitStack accounting, cleared function-local ranges, CFI-section policy/state and instruction accounting. The implementation should extract and share those operations between cold and replay paths rather than copy private state opportunistically.
- A closed admission/operation schema and exact bounded ownership ledger. Construction uses checked typed builders; the sealed immutable product cannot be modified by consumers. New operations or state fields require an explicit handler or recording refusal. All retained and temporary storage remains subject to existing optional caps and real-consumer LRU; no separate unbounded symbol/MCContext history.

Recording must occur at explicit LLVM effect boundaries, not be treated as proof merely because a trace saw some methods. Nonvirtual CFI and direct symbol/context/state writes must be covered. Record one semantic operation level: for example replaying a CFIDefCfa operation should let the original MCStreamer generate its internal current CFI label, rather than replaying that internal allocation a second time. Nested effect recording therefore needs an explicit depth/ownership contract. This prevents double symbol allocation and double frame-stack changes.

The product avoids rebuilding the large MI/MRI graph, reconstructing register-use order, reparsing MIR headers, recapturing the typed MIR, and printing/comparing current MIR solely to validate transport. It also avoids running the AsmPrinter instruction-lowering traversal again. It DOES still bind the current compiler input and emit/re-encode operations through the original MC layer. A sealed operation product can be checked once at construction, then streamed once on a hit with already validated indices and current symbol bindings; pretending that the entire current MachineVerifier still ran would be false.

## Verification distinction and exact blocking conditions

Independent language Verify is unchanged and current. The existing MachineVerifier-on-reconstructed-MF is a different transport check. At the new boundary there is deliberately no reconstructed complete MF for that checker to inspect; its role is replaced by producer MF verification plus the complete sealed product/current-input binding and emission-product invariants. This is analogous to reusing an exact object after current input validation, not accepting changed input under old authority. If the requirement literally insists on executing LLVM MachineVerifier over a current complete MF on every hit, eliminating that MF's reconstruction is impossible. Do not claim both simultaneously.

The current code has no proof that its per-function machine pass region has only the proposed emission effects, and no complete MC operation/state product. Those two closures are the blocking implementation obligations. A general-purpose replay proxy or bare MCInst list would not satisfy them. Full MIR/text equality remains the old representation's oracle; complete original object/ELF equality remains the new representation's mandatory external acceptance oracle.

## Concrete counterexamples the interface must handle

1. Edit the function emitted before an unchanged one so that temporary-label use, machine-function numbering or layout offsets change. Saved raw local symbol names/old MCSymbol pointers can collide or bind the wrong label. Replay creation recipes with current ordinal/state; retain complete same-policy object equality.
2. Keep branch operands and function logic the same but change preceding code size so a branch or alignment crosses a boundary. Reusing final encoded bytes or saved padding can be wrong. Replay MC instructions and alignment directives through current relaxation/layout.
3. Keep the same number of CFI instructions but change uwtable/module CFI policy or ordered CFI operands. A virtual emitInstruction-only recorder misses start/end-proc state and .eh_frame. Current header/config binding, semantic CFI operations and handler-state effects are required.
4. Mix fresh and reused functions. Skipping AsmPrinter Setup without reproducing module flags or EH hasEmittedCFISections may change module-final output even though each function text matches. Current begin/end emission state must be explicit.
5. Encounter a new target expression, target-streamer side effect, symbol/section attribute mutation, unsupported analysis provider or debug handler. Cold recording declines; preflight hit declines before output, or after-mutation failure aborts the whole native attempt. It must never silently omit the operation.

## Files that must change for an implementation

Pinned LLVM:

- llvm/include/llvm/CodeGen/AsmPrinter.h and llvm/lib/CodeGen/AsmPrinter/AsmPrinter.cpp: formal owner-free function-emission product API, common begin/end effects and recording/replay host.
- llvm/lib/Target/X86/X86AsmPrinter.cpp and llvm/lib/Target/X86/X86MCInstLower.cpp: produce exact admitted target MC operations through that API; unknown lowering/event domains decline.
- llvm/lib/CodeGen/AsmPrinter/DwarfCFIException.cpp (and its declaration header): expose/share ordinary no-personality CFI begin/end effects and hasEmittedCFISections state handling.
- llvm/include/llvm/MC/MCStreamer.h and llvm/lib/MC/MCStreamer.cpp: explicit operation recording hooks covering nonvirtual CFI entry/exit and preventing duplicate nested effects.
- llvm/include/llvm/MC/MCContext.h and llvm/lib/MC/MCContext.cpp: current symbol creation recipes/bindings; no old symbol owners. If a complete effect API can avoid changes here, that must be demonstrated, not assumed.
- llvm/include/llvm/CodeGen/MachineModuleInfo.h and llvm/lib/CodeGen/MachineModuleInfo.cpp: reserve current function emission identity/number without constructing full MF.
- llvm/lib/CodeGen/MachineFunctionPass.cpp and the existing pinned legacy scheduler hook: select the compiler-owned emission region without creating an empty MF or running unsupported consumers on one.

MCObjectStreamer.cpp/MCELFStreamer.cpp remain the ordinary encoding/layout/relocation consumers in the initial design. They may need a narrow replay entry hook, but no fragment cache, alternate object writer or changed relaxation algorithm is proposed.

ZIL: native/machine_products.cpp and the pinned hook ABI/patch integrate the new opaque product and fallbacks; native products ownership/budget ledger and observer schema distinguish actual emission-product reuse from MF reconstruction/AsmPrinter execution. Existing full typed MIR path remains for unsupported cases. Do not report asm_printer_runs for a skipped instruction traversal or machine_function_reuses as if an MF had been reconstructed.

## Complete feature exit conditions

One frozen feature, then unified validation; no per-field build sequence is requested.

- Capture and reuse real functions in the original425 workload, including the actual large spill/frame function and uwtable/nonempty .eh_frame consumers. Domain misses are visible, not scored as reuse.
- Full same-policy original O2 LLVM/object/ELF oracle across cold/private/multi/broad/recovery/restore and mixed fresh/reused order. Keep original Verify, budgets, pipeline and CGU partition/inlining policy. Original failures remain in evidence.
- Product lifetime outlives all producing LLVM owners; current replay contains no retained old owner pointers. Symbol/CFI/layout counterexamples and failure after partial output use whole-attempt fresh fallback.
- Count actual avoided MF reconstruction/typed recapture/MIR print and actual new MC replay/record/binding work. Report cold, private and broad net totals with current retained logical bytes; do not turn the40ms gross private ceiling into a predicted speedup.
- The machine-region effect closure and per-function/module-emission state closure receive independent source review. Until both close, this is not ready to replace the current adapter.

## Whole-O2 region conclusion

A larger natural analysis-lifetime boundary has NOT been proved in this review. LLVMRunPasses/whole existing CGU is a real boundary where the FAM is destroyed, but the unchanged object path already exploits an unchanged unit. A one-member change makes the42-definition unit dirty. Moving the boundary to an entire function across Module/CGSCC/inlining requires the actual optimization read/effect closure, including caller/sibling/header changes and region membership; current local-stage function/world keys are not that proof. Arbitrary FAM clearing, changing CGU partition, or restricting inlining is not accepted as preservation of the original quality/pipeline. The MC product does not solve this optimizer closure.

Current conclusion: complete the already implemented overhead reductions; retain the measured negative cold/broad results. The emission interface is a concrete separate design with an approximately40ms private reconstruction/validation opportunity, not a demonstrated route to the full optimization goal or a decisive Rust victory.
