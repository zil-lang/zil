# Research: clean the post-IPSCCP CFG before the first InstCombine

The opaque 8192-helper arithmetic source spends most optimizer time in one
pre-inlining InstCombine invocation. This is a different hotspot from the
entry-preserving JumpThreading and LVI/SSA candidates; retain separate arms.

The actual API ca79 profile located repeated `visitAdd` known-bit/sign-bit and
inversion queries. A subsequent single external `opt` run used the actual checked
captured CGU and the complete pinned LLVM 23.1.4 builtin O2 pipeline, with only
diagnostic instrumentation. It passed five opaque-input native oracles and
produced the same object SHA as the prior original-clone control. Its
`InstCombinePass #8193` consumed 34.0375 of 37.5338 pass CPU seconds. These are
functional diagnostic timers, not a formal speedup result.

Before that invocation, IPSCCP has removed all sum overflow branches, but 8193
blocks remain in a single-successor chain. After the invocation, all 8191 add
instructions have sunk into the final block. `InstructionCombining.cpp` pushes
operands back onto the worklist after sinking an instruction (lines 5904–5916 in
the pinned source). The before/after IR plus samples support repeated sinking
and reanalysis as a mechanism; they do not yet count exact visits or prove an
asymptotic bound. The later post-inlining InstCombine input already has only 97
remainders and is unchanged by that invocation, so adding post-inline CSE does
not address the observed expensive invocation.

Try one existing SimplifyCFG pass immediately after the post-IPSCCP `mem2reg` and
before this first InstCombine. Use exactly the options of the immediately
following existing SimplifyCFG pass, and retain that pass and every other O2
node. This only removes already-obsolete CFG structure sooner; it does not
disable InstCombine, cap analysis, weaken overflow checks, reorder language
cleanup, add speculative `nsw`, or change symbol/address contracts.

First use an independent explicit-pipeline opt control on actual captured IR,
with ordinary LLVM input/output verification, the original native export list,
five opaque-input oracles, object identity/size, and the prior baseline retained.
If it removes the diagnosed cost, map the same insertion to the exact pinned
`PassBuilderPipelines.cpp` and rebuild that one component object in a separate
native LLVM prefix. A real C API baseline and candidate must use identical ZIL
source, explicit distinct library/pipeline/cache identities and verified actual
loader selection. The external opt result cannot stand in for the API result.

Keep LLVM source/license provenance and same-flags unchanged-source rebuilds.
Before any promotion, exercise branch/loop/PHI/EH/convergence/indirect-address
controls, checked arithmetic traps and effect ordering, current Verify/preflight,
resource/closure/FFI cases, invalid-input output preservation, and both default
and closed executable runtime quality. Added CFG cleanup may cost small functions
or change optimization decisions; measure the negative distributions too.

Rust's matching no-LLVM-passes input is useful context: MIR has already inlined
the helpers and shared their repeated arithmetic, leaving 97 remainder values.
ZIL's default external exports and Rust's internal helpers differ, and both
preserve checked arithmetic. This candidate tackles the measured LLVM scheduling
gap while retaining ZIL's exports, rather than assuming Rust linkage contracts.

Evidence: `stage6/arithmetic-ir-comparison` and
`stage6/arithmetic-stage-profile` under the research workspace. The production
pipeline, public schemas, verification and default options are unchanged while
this proposal is evaluated.
