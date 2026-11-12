// Private, process-local native work-product boundary. No LLVM owner escapes a
// request. Products retain immutable byte snapshots and fixed-DSO scalar info.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_NATIVE_WORK_PRODUCTS_H
#define ZIL_NATIVE_WORK_PRODUCTS_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
struct ZilNativeProductStats {
  uint64_t available;
  uint64_t optimizer_function_stage_executions;
  uint64_t optimizer_function_stage_reuses;
  uint64_t optimizer_module_pass_executions;
  uint64_t optimizer_cgscc_pass_executions;
  uint64_t analysis_reconstructions;
  uint64_t ir_key_bytes;
  uint64_t ir_snapshot_bytes;
  uint64_t ir_replay_bytes;
  uint64_t machine_function_executions;
  uint64_t machine_function_reuses;
  uint64_t machine_pass_skips;
  uint64_t asm_printer_runs;
  uint64_t mir_key_bytes;
  uint64_t mir_snapshot_bytes;
  uint64_t mir_replay_bytes;
  uint64_t mir_key_ns;
  uint64_t mir_parse_ns;
  uint64_t mir_restore_validate_ns;
  uint64_t mir_capture_ns;
  uint64_t retained_bytes;
  uint64_t failed_replays;
};
// config is a length-delimited complete adapter policy (LLVM pin, triple,
// pipeline/options/merging, CPU/features, relocation/model, layout). No env opt-in.
void *ZilNativeProductsCreateRequest(void *module, void *target_machine,
    const void *prior, const char *config, size_t config_len);
void ZilNativeProductsSetPhase(void *request, unsigned phase);
int ZilNativeProductsFailed(const void *request);
// Finish/Abort both consume request and restore the previous per-thread scope.
void *ZilNativeProductsFinish(void *request, struct ZilNativeProductStats *stats,
    int *failed);
void ZilNativeProductsAbort(void *request, struct ZilNativeProductStats *stats);
void ZilNativeProductsDispose(void *products);
// Reentrant ordinary compilations temporarily suspend the outer TLS hooks.
void *ZilNativeProductsSuspend(void);
void ZilNativeProductsResume(void *previous_hooks);
// Explicit per-thread test diagnostics; disabled by default. Returns old mode.
int ZilNativeProductsAnalysisDiagnostics(int enabled);
// Test-only caller: exercise the real pinned LLVM text/use-order collision.
int ZilNativeProductsOwnedUseOrderCounterexample(void);
int ZilNativeProductsIncomingCallerCounterexample(void);
#ifdef __cplusplus
}
#endif
#endif
