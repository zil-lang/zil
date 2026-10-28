# 208 pre-acceptance plan

Goal: usable optional in-process LLVM API object generation integrated with 177+207+Ante-R1.
Not a claim to outperform Rust; no change to runtime safety, budgets, Wire or product gates.

Validate enabled and disabled feature builds, full workspace regression, target/layout and
pre/post LLVM verifier errors, per-context concurrency, cancellation and link failure preserving
old artifacts. Re-run current State/Store/callback consumers under O0/O2 and serial/parallel;
allocation-failure injection must link API-produced objects, NOT recompile published text via
Clang and count that as API evidence. Compare actual emitted LLVM text and oracle outputs.
Record full failures and exact source/tool binaries. Frozen benchmark is not required for the
v1 integration gate and no statistical speed claim follows from smoke timings.

Clang remains default and reference; scalar API results do not establish full language safety.
No non-author review tool is available here; author rechecks are explicitly not independent audit.
