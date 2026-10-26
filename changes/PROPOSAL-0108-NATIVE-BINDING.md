# 108 — Linux x64 external-call binding experiment

Production baseline: `fe2e650038062641dc2b467018243f1566fecff4`.
The main-tree `.task/impl/108.md` is authoritative. Task 104's original 24ca
results remain unchanged. This proposal precedes any production edit or
candidate performance sampling.

## Concrete hypothesis

The observed 104 ZIL machine programs call `getchar`/`putchar` through PLT stubs
for each byte; the corresponding Rust programs use GOT-held addresses. The
hypothesis is that selecting LLVM `nonlazybind` for existing Linux x64 foreign
function declarations will remove an avoidable repeated call-path cost. The
experiment applies to every supported foreign declaration on that target,
not a list of benchmark function names. Machine instructions alone do not
establish a complete-process improvement.

LLVM's [Language Reference, function attributes](https://llvm.org/docs/LangRef.html#function-attributes)
describes `nonlazybind` as suppressing deferred symbol binding and explicitly
notes a potential startup-cost tradeoff. It supplies no purity, aliasing,
allocation, synchronization, exception, or callback claim. No such attribute
will be inferred here. The reference was checked on 2026-10-25; executable
tests bind the installed LLVM 23.1.5 behavior separately.

## Existing contract and binding-time scope

The current design's §12.4 gives foreign imports layout and symbol identity,
the finite C-value/address ABI, and explicit unsafe/noescape/static-callback
obligations. `ffi` does not imply purity or absence of I/O. The Native main
contract in §15.7 does not specify ELF lazy-binding policy. The current
CompileOptions and CLI expose no lazy-binding mode, late import declaration,
weak-symbol fallback, or dynamic module loader contract. Current external calls
use LLVM's default C convention and target-specific narrow integer extension.

This supports a **Linux x86_64-unknown-linux-gnu backend policy** experiment;
it does not establish a universal platform or loader equivalence theorem.
Eager symbol resolution can advance missing-symbol failures and IFUNC resolver
execution. These are explicit platform-loader observations, not ZIL function
calls that may be moved or removed. A deliberately observed resolver-timing
control and a missing-symbol control will be retained rather than asserted
identical. If an existing supported source contract requires deferred binding,
that is a blocking counterexample: keep that obligation instead of redefining
the source contract to fit the optimization.

Ordinary `LD_PRELOAD` symbol replacement must still select the replacement.
External function effects, call order, returned values, narrow C ABI, checked
conversions, storage windows and cleanup obligations must remain identical.
No `dso_local`, hidden visibility or symbol renaming is added. Windows x64
declarations and call sites remain byte-identical to their baseline output.

## Minimal implementation and non-goals

The only production edit is in `zil-backend-llvm/src/emit/foreign.rs`: append
the LLVM function attribute to foreign declarations for the exact checked
Linux target. Keep all function signatures, parameter/result `zeroext`, call
instructions and all runtime checks unchanged. The byte-buffer allocator's
separate internal declarations are not rewritten as part of this slice.

The implementation does not add a runtime table, shim, environment variable,
library dependency, source attribute, new ABI, or new compiler option. It does
not assign `nounwind`, `readonly`, `readnone`, `nofree`, `nosync`, `nocallback`,
`noalias`, or `nocapture` on the strength of this proposal. No schema, safety
checker, semantic solver or product-gate file changes.

## Required correctness and machine evidence

Before timing, preserve source, actual commands, complete LLVM output, objects,
executables, relocations/disassembly, stdout/stderr and exit status for:

1. A direct runtime-dependent external scalar/narrow-value caller, linked both
   with a C object and through a shared library at O0/O2. C is an ABI oracle,
   not the compiler or a replacement safety checker.
2. Preloaded replacement functions which change the observed result, and
   stateful external functions proving ordered calls have not become pure.
3. `u8`/`u16` parameter and return values, including high unsigned values,
   with the original Linux `zeroext` attributes and Win64 negative control.
4. Shared/mutable pointer and byte borrowing through existing noescape FFI,
   and existing error/trap/cleanup controls. The source's checked range,
   arithmetic and bounds behavior must remain observable.
5. Source-supported static-object linkage and a fully static link when the
   installed target runtime supports it. If a platform facility is unavailable,
   report that fact without relabeling a dynamic executable as static.
6. Existing 104 allocation-observer programs freshly compiled under the
   candidate: all original three consumers, O0/O2, fixed inputs and original
   failure indices. Match independent model output/exit and exact allocation,
   retention and release events. An untriggered failure index stays untriggered.
7. Linux binding-timing observations and unchanged Windows target emission.

Only a correctly linked and behaviorally checked machine program can enter
the performance matrix. Raw IR and C controls are labeled separately from
full ZIL source-to-native runs.

## Preregistered complete-process comparison

The machine protocol in `tools/product/108/contract.json` will be frozen before
candidate timing. It uses three arms: the exact 108 production baseline,
candidate, and 104's unchanged idiomatic Rust sources/settings. The sources,
toolchain binaries, compiler build identity, inputs, outputs and final native
binaries receive SHA256 identities before the window begins.

All three existing consumers run at O2 with 64 KiB and 4 MiB input, plus empty
and one-byte startup controls for each consumer: twelve cells, no selected
winner-only subset. Every cell gets two warmup triplets and thirty retained
triplets. All six three-arm orderings occur five times in the retained set,
with a fixed seed and published order. There is one complete fixed campaign;
any command failure, missing/duplicate cell, changed identity, timeout or wrong
output invalidates it and keeps its raw files. No post-result expansion or
deletion of slow samples.

The clock includes process spawn, execution and final wait. File preparation
and expected-output checks are outside the clock for every arm. Report each
arm's median and nearest-rank p95, all raw observations, candidate/baseline and
Rust/candidate ratios, and descriptive exact paired median intervals. Use
the existing product runtime thresholds: per-case efficiency at least 0.9
and geometric mean at least 0.95; show the six original cases and six startup
controls separately as well as together. A failing case cannot be offset by
another case's gain. These finite runtime results cannot mark the complete
four-goal product acceptance as passed.

Memory is measured in a separate fixed campaign using the independently
reviewed 104 small native launcher, with its parent-size controls repeated.
Five target-memory observations per arm/cell are reported separately from
time. Direct Python-parent `wait4` high-water RSS is retained but excluded from
target-memory conclusions because 104 demonstrated its inherited-parent
contamination. Untimed allocation traces measure logical live/requested bytes;
they are not timing or whole-process RSS.

Only the root may grant the isolated timing window. The author may perform
bounded construction, correctness and machine-code checks beforehand, but
does not self-review or merge the production change.
