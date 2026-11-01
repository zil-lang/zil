# Actual API pass timing

By default this research constructor enables only LLVM's existing -time-passes option. It never replaces an LLVM function or changes pass pipeline composition. It activates only in the explicitly recorded compiler executable and verifies both the loaded LLVM library path and version 23.1.4. It does not link LLVM into child tools. Read changes/research-api-pass-timing.md.

The effective SLP follow-up additionally supports explicit `--slp on|off`, using the existing global `-vectorize-slp=true|false` option. This changes the SLP implementation's execution gate, while leaving the pass pipeline composition intact. Both arms must use the same shim and frozen compiler. The requested global option is part of the research receipt; production object caching remains disabled by the existing loader contract. Read changes/research-effective-slp-control.md. `slp_vector_control.py` verifies real vector/scalar IR and native results through the same pinned LLVM C API before workload comparisons. A pass-table SLP row can remain when disabled because the pass acquires analyses before checking its execution gate.

The original CLI does not flush LLVM's ManagedStatic timer groups. The probe registers an exit callback calling the existing LLVMShutdown C API after the CLI has returned and its per-compilation owned LLVM handles are gone. This research-process cleanup is recorded, not added to production. The first constructor-only no-output control is preserved as a failed diagnostic.

In an authorized functional window:

    python3 build_probe.py --clang <pinned-clang> --llvm-library <pinned-libLLVM.so> --out <new-probe-dir>
    python3 run_probe.py --zil <frozen-compiler> --probe-manifest <probe-dir>/manifest.json --source <real-main.zil> --out <new-request-dir> --merge off --expected-hex <full-output-hex>

Use --closed for the separate closed contract, --merge after for the late candidate, and --native-input for opaque programs. Default jobs=1 avoids concurrent CGU work during the diagnostic; units remains explicit. LLVM aggregates timers across the process rather than emitting one table per CGU. The requested object cache must visibly disable itself under the existing LD_PRELOAD rule. Verify, actual Builder input and native execution still belong to the original compiler. The shim is removed from the environment used to run the final executable.

All commands, compiler/probe/LLVM identities, source fingerprint/actual source files, current verification, native receipts, stderr timing tables and output are saved. Missing pass tables are a failed diagnostic, never a zero cost. Parsed rows retain each original line and CPU/wall columns. The pass report includes both NPM IR passes and legacy machine passes, so it does not directly equal either optimize_ns or emit_ns. Analysis timers overlap pass work and must not be added to pass totals. Additional unnamed timer groups retain the most recent report heading in the simple parser; use full stderr for group boundaries and do not sum rows or repeated Total entries. Timer overhead and jobs1 make these diagnostic runs unsuitable as formal performance results.

Start with the 8192 scalar case in off/after × default/closed and the SLP-off ablation. The already observed before-and-after + closed regression takes roughly 100 seconds; do not expand it into a matrix. Profile it at most once after smaller controls have demonstrated the mechanism.
