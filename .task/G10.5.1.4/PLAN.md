# Closure compile-cost experiment

Baseline: cadc656c0caefa70827f7ec1e608a04291c8ae9f (current main plus the exact scalar signature-facet production change). This task does not alter Types, Analyze, Verify, public language rules or allocation failure behavior. No external AI review is requested.

## First candidate
ByteBuffer allocation/publish transaction becomes a non-inlined internal slow helper; the existing negative and capacity checks remain in a small inlineable reserve wrapper. The original transaction body is retained exactly, including success-only pointer/capacity publication and failure returns. No cold probability, noalias, dereferenceable, nonnull, allocation success or resource permission is assumed. This attacks repeated LLVM work from inlining a buffer-growth transaction through many owning closure bodies; it does not yet solve local-helper co-location or general resource-aware CGU partitioning.

Reference mechanism: rust-lang/rust library/alloc/src/raw_vec/mod.rs keeps growth/error handling behind a call while preserving the capacity fast path; LLVM noinline is an optimizer boundary, not a semantic permission.

## Frozen measurements
Retain the eight prior cases from 95af0f0c205a0892e6087f5304ad9981461a7fed, add owned64 without dropping a prior case. Group frequencies remain graph45%, trait25%, resource30%, equal within each group. Check/O0/O2 weights remain interactive30/50/20% and release10/20/70%. Request history weights cold/warm/body/edit-warm/signature/restore remain edit10/15/45/10/10/10% and clean70/5/10/5/10/0%. Five formal randomized paired cache histories plus one warm-up; same two logical CPUs, separate release builds, same pinned toolchain. Fresh process through link; retain project caches only within a history. Frequencies are assumptions, not telemetry.

Rust is remeasured only for the owning-closure cases, O2; no new Rust ranking for the unchanged scalar workload families. Complete ZIL matrix remains mandatory. All native output checks must pass; no-codegen-change cases require identical .text, resource cases require independent outputs and runtime checks instead. Report means, medians, absolute saving, all cells and paired-history confidence. Severe compile tail: >2x and >1s extra. Runtime must not regress >10% in any measured resource case; also report the Rust runtime ratio separately. Include an already-reserved hot loop, negative/zero reserve, exact-fit, failed growth retaining data and real realloc/free counts so compilation gains cannot hide runtime or ownership regressions. Existing native allocator failure consumer remains required.

## Delivery
Only adopt a candidate with valid full evidence and weighted benefit; preserve failures and counterexamples. Changes to the general helper partition policy require their own evidence and are not implied by slow-path success. main is not modified by the experiment. The scalar-facet PR is a separate requested delivery.
