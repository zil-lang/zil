# 0108: Linux foreign-call binding policy

## Implementation and contract

Production candidate: `7a219f2d782532324e1ea7401cab68c9eada7c82`.
Production baseline: `fe2e650038062641dc2b467018243f1566fecff4`.
The main-tree `.task/impl/108.md` is authoritative.

The production change adds LLVM `nonlazybind` only to existing foreign
declarations for the exact checked target `x86_64-unknown-linux-gnu`.
The source syntax, symbol names, finite C ABI, `zeroext` handling and call-site
arguments remain on the ordinary pipeline. No function-effect property is
invented. Checked conversion, overflow, indexing and ownership obligations
remain intact. Windows foreign declarations retain their original spelling.

The proposal was committed before this production change. Current Native and
foreign contracts do not guarantee ELF lazy binding. The proposed backend
policy can therefore change symbol resolution time, but cannot be described
as equivalence of every loader observation. The actual IFUNC control prints
152 with eager binding and 52 with the one-attribute ablation at O0/O2.
External function bodies still run at their source call sites.

## Correctness evidence

Author run 003 passed all four `impl108_foreign_binding` groups, the existing
087 actual C ABI/static-callback group, formatting and targeted Clippy. The
tests retain source-through-Lower reports, then use public ordinary Types,
Analyze, independent Verify, lower and LLVM emission APIs for explicit real C
links. Candidate Core is never granted a fabricated verified capability.

The finite controls cover unsigned narrow arguments/results, two stateful
calls, mutable borrowed bytes, object/shared/full static links, `LD_PRELOAD`
interposition, checked EOF conversion failure, a used unresolved symbol and
an unused declaration. Machine records distinguish the selected symbol's
`GLOB_DAT` from the ablation's `JUMP_SLOT`. The IFUNC observation is recorded
as a timing difference. The existing Win64 emission/ABI control is retained.

The first two author attempts are preserved: the first requested LLVM from a
Driver stop before LLVM emission; the second omitted the target on its
standalone missing-symbol link and hit a different diagnostic. Neither is
counted as a successful negative control. Run 003 corrects those fixtures.
Root has also reported an independent pass of the four new groups and the
087 actual C ABI group on the unchanged production candidate.

## Runtime and memory experiment

The experiment keeps all original 104 sources, independent model and
allocation observer byte-identical, and reuses the exact sealed original
Rust plain executables. BASE and candidate ZIL compilers are built from Git
in the same source path with pinned identical release settings. Fresh plain
and observer executions cover 270 O0/O2 cases before any sampling.

The frozen matrix has all three consumers at 64 KiB and 4 MiB, plus empty and
single-byte startup cases: twelve cells. Two warmup triplets and thirty
retained triplets produce 1,152 observations. All six arm orders appear five
times in each retained cell. The separate native-launcher memory profile has
five observations per arm/cell, 180 target peaks, with twelve parent-padding
controls. None of these measurements substitutes for Types or Verify.

The tooling's first nonauthor static review returned `NeedsFix`. That review
and the exact original commit remain preserved. The revised protocol binds
each physical observation to its registered slot, uses type-strict JSON joins,
verifies external compiler receipts against embedded identities, controls
hidden Cargo flags and wrappers, refuses optimized Python, and prevents a
summary from overwriting original evidence. Targeted controls retain the
nearest counterexamples found by review.

Every median, p95, paired ratio and descriptive median interval is reported
for all twelve cells. Existing Rust efficiency thresholds and candidate/BASE
regressions are separate facts. Eager resolution and possible startup cost
remain visible even if the Rust numerical thresholds pass. The original 104
production-24ca failure remains unchanged.

## Current decision boundary

The complete three-arm preparation, controller refusal controls and fixed
1,152-time/180-memory campaigns passed their integrity requirements. The
production candidate is NoGo: the 4 MiB buffer efficiency is 0.896641529, below
the fixed per-case 0.9. All startup and p95 regressions remain reported.
Production and crate tests are restored to the exact baseline for the research
delivery; candidate code remains in history. This does not establish a runtime
breakthrough or whole-product conformance. The main-tree product gates and
thresholds are unmodified. See `docs/research/108/RESULTS.md` for every cell.

See `docs/research/108/CONTRACT.md` for exact matrices, order, limits and
reproduction commands, and `docs/research/108/BINDING-OBLIGATIONS.md` for the
semantic scope and retained loader counterexample.
