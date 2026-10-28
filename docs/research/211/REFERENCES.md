# 211: inspected primary implementations and the applicable idea
Read 2026-10-28. These are design references, not imported/copied source implementations.

- Rust `compiler/rustc_codegen_llvm/src/context.rs`, blob
  `8ef5ce26e1c2de911d1413b4befb28ea4f1234bd`:
  https://github.com/rust-lang/rust/blob/main/compiler/rustc_codegen_llvm/src/context.rs
  One context per CGU; separate type/intrinsic/instance caches. Here direct call
  LLVM function types reuse the checked declaration instead of constructing a
  cloned temporary Signature at each call. Never share handles across contexts.
- Ante `src/codegen/llvm/mod.rs`, blob
  `4e05510aef9969e897eebfd0bf3f19e4fb7bb896`:
  https://github.com/jfecher/ante/blob/master/src/codegen/llvm/mod.rs
  ModuleContext uses BlockId-indexed VecMap and caches pointer-sized type. Here
  already-dense (block,instruction) positions index a vector rather than a BTreeMap.
  This is invocation-local indexing, NOT stable cross-request symbol identity.
- Go compiler SSA Cache:
  https://go.dev/src/cmd/compile/internal/ssa/cache.go
  Reuses compiler scratch state across function work. Here a function reuses its
  local-use and dependency buffers across instructions instead of fresh allocations.
  Completed lowering fragments are moved into final items before the next function.
- LLVM Programmer's Manual:
  https://llvm.org/docs/ProgrammersManual.html
  StringRef/ArrayRef emphasize borrowing and avoiding unnecessary temporary objects.
  Here SSA/global/block name tables borrow from immutable machine IR; operand
  iteration allocates no temporary vector. Rust lifetimes bound these references.

No foreign source text or unsafe blocks were copied. Existing project license applies
only to original ZIL implementation; linked projects retain their own licenses.
Not adopted: disabling validation, default noalias, unchecked CGU internalization,
calling hostile native IR safe, shared mutable LLVM contexts, or fake cache hits.
