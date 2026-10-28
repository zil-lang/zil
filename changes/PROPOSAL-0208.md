# 208 architecture change

# 208 integrated LLVM API v1

## Frozen input lineage
Language source: 174–177 `08cb9e6ff57492e11eec6c8592d37d601ee71f10`, restored from its actual
history bundle; snapshot source ZIP separately inspected. Wire/schema remains 21.
Compiler research: 207 `de7ce29528bab6567b9c524788cb2e62e29f6dc6` production changes
204 (owned AST transfer), 206 (bounded CGUs), 207 (direct imported declarations) transplanted
onto 177. Neither historical language nor research timings are counted as this run.
User design: `_zil_design_1.4.md` (original verbatim archived as Ante-R1-input.md).
The filename is 1.4 but its normative identifier is ZIL-LANG-1.3: do not invent a new identifier.
Its revision note refers to 173/Wire20; that note is historical. Integrated root updates only
this status note; normative Ante-R1 text is otherwise preserved. No language capability is
marked complete by changing the document. Existing product thresholds remain unchanged.

## Working first release
`zil build/run --backend llvm-api`, built with Cargo feature `llvm-api`, calls libLLVM17
in the compiler process. API handles single or opt-in parallel CGUs, O0/O2, owned buffers,
callbacks, checked arithmetic and existing layouts through the SAME current emitter.
LLVM context and target machine are per-unit. Registry initialization is once per process.
Returned objects are owned bytes; filesystem publication and linking remain in existing
safe backend/driver code. No Python worker, no per-instruction RPC and no clang compilation
of .ll in the API path. Clang is still the Linux link driver; Win64 uses explicit lld-link.
The public front-end path requires its current independent Verify and bound LIR.

## Not yet implemented
LIR directly lowered by LLVMBuilder, typed entity/CGU graph, stable symbols, object cache,
ThinLTO, new projection-precision rules and broader language profiles. The inherited text
CGU splitter remains explicitly transitional, with conservative fallback on unknown forms.
The optional API route is real production plumbing, not a complete direct-Builder migration.
Default remains Clang, one module. Do not default O2 to parallel units: 206's known quality
regression has not been eliminated by switching the API.

## External code lessons
Ante at 9f8f3def5a23ca38a8de4c891b386da429a89521, src/codegen/llvm/mod.rs: module-local
Context/Builder/TargetMachine, object byte results, O0 parallel/O2 whole-program distinction.
Do NOT copy its native target assumption, greedy partition as a stable identity scheme, or
build_module's verify-error printing followed by continued emission. ZIL fails closed.
Rust compiler/rustc_codegen_ssa/src/traits/backend.rs: backend/compile/join/link boundaries.
Reuse that separation without introducing per-instruction dynamic dispatch or rustc_private.
LLVM llvmorg-17.0.6 C API source checked for memory-buffer transfer and ownership.

External reference paths and exact upstream blob IDs are recorded in REFERENCES.json.
