# 044 — bounded scalar C ABI imports and explicit unsafe calls

Parent: B0. Baseline: fb85a4537a618909637efb5000043328fdc5cf3f.
Authority: `_zil_design.md` ZIL-MODES §2.4 and ZIL-ASYNC §12.4; main-tree `.task/impl/044.md`.

## Proposed acceptance and ABI boundary

Admit private, bodyless `extern "C" fn` declarations with `i32`/`i64` value parameters and `i32`/`i64`/unit results on the existing checked x86_64 Linux LLVM target. Imported symbols are C identifiers. Reject public extern exports, unknown ABIs, bodies, generic/where/contract annotations, `give`, receivers, bool, pointers/references, aggregates, variadic or function-pointer signatures. The ordinary ZIL module exports safe wrappers whose source explicitly contains `unsafe { ... }`.

The imported declaration supplies the ABI and symbol, not proof about the foreign implementation. The wrapper's documented C contract is a trusted boundary: correct C signature; no unwinding across ZIL frames; no retained ZIL storage, callbacks, cancellation or resource transfer; global stream effects follow the C library contract. A hostile or mismatched foreign implementation can violate that boundary, just as with Rust FFI. No static isolation claim is made.

## Raw IR and independent checking

`CoreFunction` and `TypedFunction` gain `foreign: Option<ForeignDeclaration { abi, symbol }>`; a foreign function has only parameter locals and no executable CFG. Core retains lexical `Unsafe` blocks. Typed CFG adds `UnsafeEnter { region }` (tag 31), `UnsafeExit { region }` (32), and `ForeignCall { function, arguments, region }` (33). Ordinary calls and ordinary function references must reject foreign declarations. Unsafe regions are local to each function, so closure creation cannot authorize the closure's later body. Compile-time checking rejects foreign declarations and all three operations.

The independent verifier validates the full scalar declaration and argument/result signature, symbol collisions, and reconstructs balanced unsafe-region stacks over every reachable CFG edge. All incoming stacks at a join must match; a call must match the active innermost region. Scope exits for return/break/continue are explicit. Unsafe/foreign operations in unreachable blocks are rejected. A terminal trap may retain its lexical stack because it cannot return or transfer authority. Enter/exit markers have no runtime representation. A candidate boolean is never accepted as a safety proof. Foreign implementation compliance remains explicitly outside this checker. The existing work budget charges foreign metadata, region-table visits, stack copies, edge comparisons and nested-stack searches before the corresponding work; no budget limit is raised.

The new function record changes the wire shape. Root integration owns the coordinated schema/wire revision 5; no stale cache format is retained. No runtime, print keyword, new `@` candidate, or hidden heap allocation is introduced.

## Intended evidence and limits

Real ZIL module wrappers use libc `getchar` and `putchar`. The first executable reads bytes from stdin, processes them in ZIL, writes stdout, and reports empty/input-validation/output failures through exit codes. O0/O2 dynamic input and failure behavior are compared with a Rust program under the same scalar libc contract. Tests include closest source rejections and forged raw declarations/CFG/CT candidates. Tests are bounded, not the formal full-language or product gates. libc and its stream buffering are explicit external dependencies; the absence of allocations in emitted ZIL LLVM does not claim that libc itself never allocates.

Rust-goal report is recorded with the actual acceptance evidence. Behavioral parity alone establishes no performance, memory, compile-time, maintenance or breakthrough claim.
