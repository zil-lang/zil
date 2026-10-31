# Explicit incremental O2 chain and scalar strategy

The private checked-add chain pass runs only for O2 with `--incremental-o2`, before the existing checked scalar-leaf evaluator. Ordinary O2 and O0 skip both passes. Each nonempty plan validates its own complete unchanged input before mutation; current ZIL Verify and per-unit LLVM model validation remain mandatory. No exported definition, function address, source limit, unsafe permission, diagnostic ordering or checked-arithmetic contract is relaxed.

A strict same-direction chain can replace several checked additions by one checked addition of their representable total. A chain containing an intervening effect, a live intermediate, incompatible trap or signed direction change is retained. Phi endpoints keep the final checked block and its original edge. The two v4 necessary-condition gates avoid full planning when the existing planner cannot form a chain: they reject invalid constant-prefix shapes and functions without even one compatible Check-to-Check edge. These gates have exact v3/v4 plan/Module/count/error controls; they are not a new arithmetic rewrite.

The new action protocol is `ZIL-OBJECT-ACTION-212-v4-complete-closures-chain-v4-scalar-v3`. The v4 producer/identity record ordered passes `checked-add-chain-v4`, then `checked-scalar-leaves-v3`, require `incremental_o2=true`, and bind both implementation files and the exact O2-and-flag hook. Old production/research tools and receipts remain unchanged. This migration does not include declaration pruning, early MergeFunctions or a larger scalar evaluator budget/opcode whitelist. In particular, a helper ending in srem remains outside scalar-leaf eligibility after contraction.

Research v3 compile432 and the paired runtime900 keep their exact frozen candidate identity. The compile comparison has an overall candidate/baseline elapsed GM of 0.830655 and warm GM of 0.943820, with adverse closed2048-branch cold/warm intervals retained. The runtime comparison is against production84f6 with the same opt-in options: default GM 0.969717 [0.937855, 0.992876], closed GM 0.987730 [0.964357, 1.037019]. Closed2048-branch remains adverse even though that program's paired ELF bytes are identical. Counts from a diagnostic early-exit compiler do not establish the cause or percentage cost of those regressions. Only the two 512-body executable pairs changed bytes in that runtime comparison; all output oracles still passed.

Those research outcomes support further production confirmation, not a promise that all workloads improve. V4 gate equivalence preserves the tested transformation plans, but its production namespace, formatted sources, compiler ELF and final compile/runtime results require new evidence. The final generated/opaque Rust comparison, real336, quality/mono/Rust900 and both LLVM CI configurations must use the actual final BUILD; historical results are not relabeled.

After the integration commit is frozen, create a fresh compiler and receipt:

```sh
python3 -B tools/research/incremental-compilation/quality_closure/build_production_v4.py \
  --repo "$PWD" --revision <exact-production-commit> \
  --toolchain-receipt <four-TU-prefix>/native-toolchain-receipt.json \
  --clang <official-LLVM-23.1.4-prefix>/bin/clang \
  --target <new-target-directory> --out <new-build-receipt-directory>
```

Use its `zil-quality` wrapper with `build <input.zil> --backend llvm-api --opt O2 --incremental-o2 --function-merging after --codegen-units 8 --codegen-jobs 2`. The BUILD binds all selected Git source bytes before/after compilation, compiler/LLVM/clang identities, command and actual compiler-PID loader probe. It is not a runtime/performance acceptance receipt. The separate four-TU LLVM prefix is reproducible through `toolchain_combination/rebuild.py`; ordinary official SDK installations remain unchanged and do not include those LLVM fixes.

The new v4 CLI, opaque, real-program and runtime consumers share `stage6_production_identity_v4.py`. They retain the original source oracles, timers, paired statistics, failures and NotRun records. Real-program cache misses/fallback remain measured observations, with anomalies audited separately rather than filtered out. Formal runners require a root-granted exclusive lease. Research candidate receipts cannot pass as a production BUILD.
