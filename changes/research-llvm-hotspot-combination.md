# Research: compose the measured LLVM hotspot fixes in one isolated toolchain

Compose four independently identified mechanisms using four translation units:
entry-preserving JumpThreading, its threadEdge SSAUpdaterBulk path with exact
placement-query grouping, LVI value-to-block invalidation indexing, and O2
post-IPSCCP CFG cleanup before InstCombine. The first two share JumpThreading.cpp;
the grouped placement cache additionally changes SSAUpdaterBulk.cpp. The other
units are LazyValueInfo.cpp and PassBuilderPipelines.cpp.

Keep every previous source, plugin, library, failure and receipt unchanged. The
new candidate is a separate epoch. An unchanged-source control must recompile
all four upstream translation units with the same flags before relinking the
same 61 original SDK archives; comparing only with the installed SDK would mix
build differences into algorithm attribution.

Composition must verify pinned input manifests, the exact entry patch applied
to the SSA source, every output source hash, and every unchanged archive member.
No public header, class layout or ZIL C++ boundary changes. The local LVI cache
layout is internal to its implementation. Add no new CLI optimization flag or
global LLVM option. Retain the native verification, export/address, overflow,
effect and cleanup contracts. Use common ZIL production source and explicit
private pipeline/library tags for the two compiler arms, with actual loader and
cache-namespace controls.

Individual diagnostic gains do not establish a combined gain. The combined
candidate needs real API semantic/differential controls, current-source
resource/closure/FFI/invalid-output/cache controls, independent review and
representative exclusive compile/runtime measurement. Preserve unaffected and
negative distributions, particularly entry-worklist cost and LVI historical
membership memory. The scalar, branch and opaque arithmetic cases exercise
different hotspots; do not present one as proving the others.

After the combined after-MergeFunctions control, perform only the coordinator's
bounded interaction probes: opaque8192 arithmetic default early versus after,
and the previously catastrophic scalar8192 closed early case, each with a
180-second cap and complete native/verification records. Earlier failures remain
valid for their old library. Any new early-policy decision requires its own
evidence and epoch; the composition itself does not promote it.
