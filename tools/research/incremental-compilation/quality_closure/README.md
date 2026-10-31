# Incremental O2 quality-closure research

This directory preserves the frozen incremental policy's runtime regressions and
tests a separate complete direct-call implementation-copy policy. It does not
change production defaults or the already measured compiler/library pair.

The accepted production implementation is committed at
`a1ab243fa5ba1b9841c904d251fea7c999aa6681`. It retains the explicit
`--incremental-o2` option and introduces the independent object-action protocol
`ZIL-OBJECT-ACTION-212-v2-complete-direct-call-closures`. The four-TU LLVM fixes
remain a separately rebuilt toolchain; installing the ordinary official SDK
does not install them. See `toolchain_combination/README.md` in the parent
directory for that rebuild.

For a fresh production compiler and authoritative source/build receipt, use:

```sh
python3 tools/research/incremental-compilation/quality_closure/build_production.py \
  --repo "$PWD" --revision a1ab243fa5ba1b9841c904d251fea7c999aa6681 \
  --toolchain-receipt "$LLVM_CANDIDATE/native-toolchain-receipt.json" \
  --target "$NEW_TARGET" --out "$NEW_BUILD" --clang "$LLVM_SDK/bin/clang"
"$NEW_BUILD/zil-quality" build path/to/main.zil --backend llvm-api --opt O2 \
  --function-merging after --incremental-o2 --codegen-units 8 --codegen-jobs 2 \
  --object-cache path/to/new-cache --json -o path/to/program
```

The worktree's tracked compiler sources must match the requested commit. The
producer verifies the same complete source inventory before and after building,
records compiler/library/clang identities, and verifies the actual compiler-PID
LLVM loader path. It never overwrites an existing target or build directory.
`supplement_build_environment.py` optionally records a bounded later observation
of Cargo/Rust tools, selected build variables and config identities. It does not
claim those inherited settings were captured at the original build launch.

`capture.c` only prints the real LLVMRunPasses input/output and forwards the call.
The compiler and loaded library paths must match explicit expected identities.
LD_PRELOAD disables the production object cache under its existing policy.
`capture.py` uses the exact prepared runtime source, command and five-seed oracle.
The first capture's incorrect sixteen-call assertion is retained beside a
correction: eight units invoke LLVMRunPasses eight times, with late MergeFunctions
enabled in the pass-builder options. Captured output includes that complete pass
pipeline. This is not the unpartitioned public program.ll artifact.

`prepare_ablation.py` creates four narrow actual-IR controls: bounded/full bodies
crossed with available-externally/real Internal copies. `run_ablation.py` replays
the exact target, PIC relocation, codegen level, PTO settings and link order. It
requires a byte-identical bounded baseline before interpreting any changed arm.
Only the root object changes between transformed arms; other object reuse is
explicit. Canonical symbols and five native results are checked. Its timings are
functional diagnostics, not formal compile/runtime comparisons. The scalar-only
IR transformer is a research harness, not a general compiler implementation.

The structured candidate is in `complete-clone.patch`, pinned by
`complete-clone.json`. It follows only direct source-function call edges for body
copies. All symbol references still determine ordinary declarations, validation
and cache dependencies. Each copy has an Internal name and remains an emitted
implementation when not inlined. Only direct callee operands are renamed;
canonical definitions, function arguments/values, globals and indirect calls
retain their identities. Extra clone cost over all units must not exceed the
original source graph's machine instruction/block cost. Unsupported local or
compiler-owned definition dependencies, or excess total cost, produce a reported
whole-module fallback without a partial closure. This is not a two-times-RSS
guarantee. Remaining-use distribution moves the final body use without retaining
an extra full imported-template collection.

For historical private-v1 reproduction, after building the separately documented
combined LLVM toolchain, create a new compiler snapshot and private target
directory (this is distinct from the production build above):

```sh
python3 tools/research/incremental-compilation/quality_closure/prepare.py \
  --repo "$PWD" --toolchain-receipt "$LLVM_CANDIDATE/native-toolchain-receipt.json" \
  --out "$NEW_SOURCE"
python3 tools/research/incremental-compilation/quality_closure/build.py \
  --repo "$PWD" --source "$NEW_SOURCE" --target "$NEW_TARGET" --out "$NEW_BUILD" \
  --clang "$LLVM_SDK/bin/clang" --jobs 2
"$NEW_BUILD/zil-quality" build path/to/main.zil --backend llvm-api --opt O2 \
  --function-merging after --incremental-o2 --codegen-units 8 --codegen-jobs 2 \
  --object-cache path/to/new-private-cache --json -o path/to/program
```

Each output directory must be fresh. The snapshot records the common Git
revision, exact compiler patch, library and separate policy tag. Build receipts
record the actual compiler-PID library load. The wrapper selects only that
process's library and compiler; it does not replace the environment's SDK.
The immutable historical `complete-clone.patch` includes the initially observed
test-only Rust borrow error. New private snapshots apply `test-fixture-fix.patch`
after it, record both patch identities in `REPRODUCER.json`, and capture the
corrected test source in their own new receipt. Original v1 receipts, failed
test output, and its release compiler remain unchanged.

The machine-model native tests include self addresses, equal-body distinct
addresses, C callbacks, one shared global, and direct/mutual recursion. Real
source, current Verify, complete edit invalidation, cache corruption/failure
cleanup and representative runtime/compile quality need separate execution and
review. A prepared source or successful build is not a claim that those controls
have passed.
