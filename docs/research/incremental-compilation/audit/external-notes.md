> 2026-10-31 开工只读审计的冻结记录，保留原分支/SHA/当时路径作为来源。本文不是当前实现状态；已修项目和最终实测见 ../SLICE1.md、../SLICE2.md 与 ../RESULTS.md。

# External evidence for ZIL compilation research

Read-only source collection, 2026-10-31. No external project performance numbers have been reproduced on ZIL. Files downloaded under external/; no project files changed.

## Rust: prioritize semantic projections over a bigger memo table

Source: https://github.com/rust-lang/rustc-dev-guide/blob/69e3b706eac15d804c75ce6558cc8355aac4e3f5/src/queries/incremental-compilation-in-detail.md

- Lines 94–190: red/green stops propagation when a recomputed query's RESULT is unchanged. Merely observing that an input changed and transitively dirtying everything is less precise.
- Lines 217–260: session-local sequential IDs cannot be persisted as semantic identity; DefPath/DefPathHash map persistent identity to current-session DefId.
- Lines 300–315: stable fingerprinting is costly and is a principal reason incremental compilation may be slower than non-incremental.
- Lines 450–479: no_hash on monolithic collections plus small projection queries avoids duplicate hashing and lets projections form an invalidation firewall.
- Lines 390–413: cached codegen units are work products; the query graph can prove their dependencies unchanged without computing an LLVM-module fingerprint.

Source: https://github.com/rust-lang/rustc-dev-guide/blob/69e3b706eac15d804c75ce6558cc8355aac4e3f5/src/backend/monomorph.md

CGUs distinguish stable nongeneric code from volatile monomorphized/specialized instances. Generic instantiation has compile-time and code-size cost; faster compiler data structures do not eliminate semantic duplication.

ZIL implication: adopt stable declaration and instance identity and separate signature/layout/borrow-effect/CT-value/body facets before caching whole-project types harder. Preserve dependencies on generic/inline/CT bodies where they really matter. A public-signature-only invalidation policy is unsound for those uses.

## Zig: explicit dependency kinds and two-stage staleness

Sources:
https://github.com/ziglang/zig/blob/738d2be9d6b6ef3ff3559130c05159ef53336224/src/InternPool.zig#L836
https://github.com/ziglang/zig/blob/738d2be9d6b6ef3ff3559130c05159ef53336224/src/Zcu.zig#L250
https://github.com/ziglang/zig/blob/738d2be9d6b6ef3ff3559130c05159ef53336224/src/Zcu.zig#L3061

InternPool.Dependee explicitly distinguishes src_hash, nav_val, nav_ty, interned, zon_file, embed_file, namespace, namespace_name, memoized_state. Zcu tracks potentially_outdated versus outdated, updates dependency counts after analysis, and schedules ready units. TrackedInst handles continuity across source revisions; source positions cannot simply be retained forever.

ZIL implication: a lookup of a single name should depend on that name, including absence; namespace enumeration must depend on the namespace set. Track file reads/CT inputs/target/rules/budget explicitly. Anonymous/generic instance identity and deletion/readdition require deliberate policy. Reusing a function while forgetting a negative lookup dependency can compile stale semantics.

## Salsa/Adapton: purity, demand, stable names

Sources:
https://github.com/salsa-rs/salsa/blob/d30612afc4237f2b0f1d3e132ad9bc27fe48b03d/README.md
https://github.com/Adapton/adapton.rust/blob/ea903dd961bee3568c483a18a72983d9f4ef5ed2/README.md
https://arxiv.org/abs/1503.07792 (Incremental Computation with Names, OOPSLA 2015; identified by Adapton's source README)

Salsa models queries as deterministic input-to-value functions and tracks dependency changes. Adapton provides demand-driven computation with explicit names. A framework does not by itself repair coarse query boundaries. ZIL query execution must return its diagnostics/cost receipts rather than hide effects behind mutation; replayed cached work must preserve existing logical-budget outcomes and diagnostic ordering. Do not turn independent Verify into an ordinary trusted cache lookup.

## Build Systems a la Carte: scheduling versus rebuilding

Sources:
https://github.com/snowleopard/build/blob/43b18b9a362d7d27b64679ea4122e4b8c5dfedd9/README.md
https://github.com/snowleopard/build/blob/43b18b9a362d7d27b64679ea4122e4b8c5dfedd9/src/Build/System.hs
https://doi.org/10.1145/3236774

The executable framework separates topological/restarting/suspending schedulers from dirty-bit/verifying/constructive trace rebuilders. Its comments identify correct but non-minimal systems; correctness does not imply useful incremental performance. The word verifying trace is about build input validity, not ZIL's language-safety Verify.

ZIL implication: separate request admission, dependency validation, query recomputation, codegen work products, and final link/publication. A no-op action cache, semantic incrementality, and object-cache hit are three different capabilities; measure all three explicitly.

## Cranelift: useful developmental backend experiment, no free four-goal win

Sources:
https://github.com/rust-lang/rustc_codegen_cranelift/blob/74bc4c2cc696bbb7499bb6104e872314d590a8ed/Readme.md
https://github.com/bytecodealliance/wasmtime/blob/c57fe5644237b90edda3132a14e1ca25b4e0cf72/cranelift/README.md

rustc_codegen_cranelift explicitly targets improvement of debug compilation and lists support boundaries. Cranelift documents differential fuzzing, a separately checked register allocator, and production use. Its README cites old Wasm-specific results for compilation/quality; those are NOT a promised ZIL speedup or evidence of Rust-level release performance.

ZIL implication: use the shared verified machine IR for an optional fast dev backend, retain LLVM for optimized output, and benchmark complete edit-build-run under equivalent runtime semantics. Preserve overflow checks, ABI, unwind/abort, resource cleanup and unsupported-feature behavior. Add a backend only after profiling establishes enough codegen time to repay maintenance and compile-quality cost.

## Differential dataflow: reserve for measured recursive dataflow hotspots

Source: https://github.com/TimelyDataflow/differential-dataflow/blob/aa8745f93ea8abe131104fc7885ba4fd47e63902/README.md

Functional collection transformations include join/reduce/iterate; changes carry positive/negative weights and can propagate through iterative analyses. This is relevant to repeated points-to/effect/callgraph fixed points with additions AND deletions, but state/index memory and bookkeeping have real cost. First compare simple dirty-SCC recomputation and per-function summaries. Do not repeat ZIL's graph-theory experiments without a production hotspot and end-to-end win.

## Most important architecture constraint

If every request must scan independently verified input of size N, full-build latency retains an Omega(N) floor even when objects all hit. Achieving delta-proportional latency requires either a smaller independently checked certificate/summary boundary or a deliberately designed trusted long-lived incremental verifier; the latter changes trust/lifetime assumptions. Both need explicit dependency completeness, invalidation controls, independent checking and the same acceptance/budget behavior. Hash equality is not safety authorization.
