# Research: replace one LLVM JumpThreading object in an isolated native toolchain

This proposal keeps ZIL's existing LLVM C API lowering, verification, native code emission, linking, and public contracts. It changes one upstream LLVM implementation object in a separate research DSO. It does not replace the current LLVM prefix or introduce a production plugin/C++ FFI boundary.

## Problem and existing evidence

One authorized user-mode call-stack profile of the frozen compiler's actual LLVM API pipeline found 162 of 438 samples simultaneously in JumpThreading → `MergeBasicBlockIntoOnlyPred` → `DomTreeUpdater::recalculate` → `CalculateFromScratch`. Those samples were about 98% of the samples containing JumpThreading. The merge return address identifies its entry-replacement recalculation site. This establishes the hot path for that default-linkage scalar8192 case; it does not measure a scaling exponent or explain the separate closed + early-MergeFunctions InstCombine/IPSCCP regression.

An out-of-process opt plugin already checked the entry-preserving approach. Nine small IR cases passed LLVM and dominator verification, with the original clone byte-identical to built-in output. The first actual-source ownership/cache controls passed seven builds and one current-source rejection. The plugin has a different invocation path and research bookkeeping, so its measurements cannot establish native C API benefits. The wrapper's transactional ZIP cache revision still requires a fresh functional window; earlier evidence remains tied to its recorded epoch.

## Minimal implementation

Pin LLVM `llvmorg-23.1.4`, commit `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`. Generate a patch to `llvm/lib/Transforms/Scalar/JumpThreading.cpp` only:

* Keep the original `JumpThreadingPass` class name, namespace, constructor, header, object layout, options, registration, and statistics. Add no member, research counter, strategy flag, plugin entry point, or renamed command-line option.
* Use the checked `WeakTrackingVH` work snapshot in the fixed-point traversal. Re-read the tracking handle after transformations; it follows RAUW to the surviving entry and becomes null on deletion. New blocks are revisited in a later changed round.
* Only when the unique predecessor is the function entry, retain entry identity with the existing `MergeBlockIntoPredecessor` utility. Preserve original eligibility checks, clear both LVI block entries, remove the loop-header key before RAUW, return immediately after success, and restore bookkeeping before the original fallback if the stricter utility declines.
* Keep all other transformation paths, duplication thresholds, loop guards, and the current Verify contract unchanged. This is a merge-orientation/traversal change, not a pass disable or increased search budget.

`prepare_native_patch.py` will retain the pristine source, patched source, unchanged header identity, full unified diff, generator SHA, and upstream provenance. `build_native_toolchain.py` will compile exactly one source file per arm and copy the necessary existing component archives into a fresh prefix. It will replace exactly `JumpThreading.cpp.o` in that prefix's `libLLVMScalarOpts.a`, rebuild its index, and prove that all other object-member bytes are unchanged before relinking.

## Feasibility established by static reads

The installer already retains all required PIC component archives. Its successful DSO link reads 160,526,176 bytes of archives in total; `libLLVMScalarOpts.a` is 7,941,334 bytes. A read-only ar-header inspection found the exact `JumpThreading.cpp.o` member (197,248 bytes, native ELF64), distinct from `DFAJumpThreading.cpp.o`. No LLVM LTO rebuild is required for that member. Exact SDK headers, including generated headers, were previously extracted and verified; another SDK decompression is unnecessary.

The existing 72,146,000-byte DSO is itself a local relink of those official PIC archives. Reuse the recorded `tools/llvm/install.py` link command: whole-archive inputs, `-z defs`, `-z text`, `-Bsymbolic-functions`, GNU build ID, `libLLVM.so.23.1` SONAME, `LLVM_23.1` version map, and the recorded system libraries. Differences are isolated paths, the replaced ScalarOpts member, and an explicit LLD thread limit of at most two. Do not link the plugin into this DSO and do not modify the statically linked official opt; opt would still run its own original LLVM code.

Compile with the exact SDK headers and llvm-config ABI flags (C++17, no RTTI/exceptions, `_GLIBCXX_USE_CXX11_ABI=1`, assertions/ABI-breaking checks disabled), PIC and recorded release optimization flags. The upstream SDK does not provide a complete translation-unit compile command, so produce **both an unchanged-source rebuilt control DSO and the patched DSO with identical commands**. Compare that control against the current DSO before attributing a change to the patch. Matching version strings alone is insufficient.

Estimated budget, not a measurement: one small translation-unit compilation plus a 160 MB-input link per arm should be seconds to roughly a minute per arm on this host; reserve a bounded two-arm functional window before promising a number. Additional disk is roughly 0.25 GB per complete isolated prefix (archives, 72 MB DSO, object and logs), below 0.6 GB for two arms. Link/compiler peak memory is not established by this audit; record actual time/RSS later. No full LLVM source build or new SDK download is proposed.

## Tool and cache identities

Never write through a hard link into the current prefix. Record upstream commit, exact SDK archive SHA, every copied component SHA, original/replacement object SHA, unchanged-member inventory, exact source/header/patch SHA, clang/llvm-ar/lld identities, compiler/link commands, output DSO SHA/build ID, SONAME, and loaded system dependencies. Keep the baseline prefix intact.

Use a fresh process and `LD_LIBRARY_PATH=<isolated-prefix>/lib` for the existing ZIL binary; `ZIL_LLVM_LIB_DIR` is a build-time search setting and alone does not select a runtime library. Confirm the actual loaded DSO. No `LD_PRELOAD`, symbol interposition, or new unsafe call is needed. ZIL's existing native cache obtains the library through `dladdr(LLVMGetVersion)`, canonicalizes it, and incorporates installed-library identity/build ID into its namespace. The new library must create distinct actual keys; additionally keep any research pipeline tag explicit and fixed for each tested arm. Same version `23.1.4` must not imply cache compatibility.

## Validation and decision

Execution is deferred until the root releases its exclusive Session window. First build/load/version/dependency controls; then real C API before/after LLVM verification, the existing independent current Verify, actual native exports/addresses and runtime controls, resource/closure/Drop/FFI/pointer cases, warm/private-edit/corruption/rejection cache controls, and the previously diagnosed large inputs. Preserve failures and compare rebuilt-original versus original before patch attribution. Broad formal timing remains owned by the integrator.

Unchanged public schema, ABI and language acceptance are requirements. Exact native correctness and runtime quality must survive the optimization-order change. The plugin's small controls do not cover every EH, convergence-token, profile-metadata or sanitizer case; retain those limits and add targeted controls before default promotion. If the rebuilt control itself changes behavior or exposes an ABI issue, stop that path without changing the baseline toolchain.

This experiment targets compiler efficiency while keeping safety checks and optimized runtime quality as explicit gates. It is not yet a claimed Rust-level compile-speed breakthrough or a decision to ship a patched LLVM default.
