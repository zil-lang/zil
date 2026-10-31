# Explicit incremental O2 production confirmation

The v3 scalar-leaf evaluator runs only when both O2 and `--incremental-o2` are selected. Ordinary O2 and O0 skip the new pass. The evaluator plans from the unchanged machine model; a nonempty plan requires complete original-model preflight before any mutation. Unknown arguments retain real calls and their complete imported-body dependencies. Canonical definitions, addresses, argument effects and current Verify remain required.

The separate v3 producer and consumers require the exact action protocol `ZIL-OBJECT-ACTION-212-v3-complete-call-closures-checked-scalar-v3` and record `machine_transform.requires_incremental_o2=true`. Historical v2 producers, receipts, tools and observations remain unchanged. This migration does not include the declaration-pruning experiment.

The frozen research v3 432-request comparison supported proceeding to explicit opt-in production confirmation: candidate/baseline elapsed GM 0.925389, warm GM 0.999047. All six opaque warm cells had adverse point estimates. Across all 18 opaque cells there were three adverse intervals: 512-body default early edit, 2048-branch closed cold, and 2048-branch closed warm (1.067735). Only the last is a warm request. These limits remain part of the decision. It is not a general speedup claim for no-fold workloads or an unconditional O2 rollout. The actual final source/ELF, 3,480 generated/opaque requests, 336 real-program requests, 900 native runtime observations and both LLVM CI configurations still require their own confirmation identities.

After the integration commit is frozen, build a fresh compiler without overwriting previous targets:

```sh
python3 -B tools/research/incremental-compilation/quality_closure/build_production_v3.py \
  --repo "$PWD" --revision <exact-production-commit> \
  --toolchain-receipt <four-TU-prefix>/native-toolchain-receipt.json \
  --clang <official-LLVM-23.1.4-prefix>/bin/clang \
  --target <new-target-directory> --out <new-build-receipt-directory>
```

The output `BUILD.json` binds committed source inventory before/after compilation, actual compiler and LLVM identities, build command and the compiler-PID loader probe. Its `zil-quality` wrapper fixes the LLVM and clang paths. The four-TU LLVM installation is independently reproducible through `toolchain_combination/rebuild.py`; the ordinary official SDK remains unchanged and does not contain those LLVM patches. A successful build receipt alone proves neither runtime quality nor performance.

The new tool entry points are `stage6_production_cli_v3.py`, `stage6_production_cli_analysis_v3.py`, `stage6_production_opaque_bench_v3.py`, `stage6_quality_runtime_v3.py`, and the two `stage6_production_real_*_v3.py` tools, sharing `stage6_production_identity_v3.py`. They must consume the new committed BUILD, not relabel a research ELF. Their formal runners still require a root-granted exclusive lease.

The real336 connection retains the original 14-case runner, all fifteen native input outcomes, six alternating blocks and the existing paired statistics. Its analyzer writes a durable failure receipt for missing input. Cache misses and whole-module fallback remain real observations: unexpected cache states are listed separately and never filter valid slow rows out of statistics. Syntax and synthetic control evidence must be distinguished from actual production identity admission, preparation and measurement.
