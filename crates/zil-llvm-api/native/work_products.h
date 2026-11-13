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
  uint64_t world_projection_direct;
  uint64_t world_projection_fallback;
  uint64_t tli_projection_builds;
  uint64_t tli_projection_reuses;
  // Appended observer v3 scopes; parse is inside replay, pack inside capture.
  uint64_t ir_key_ns;
  uint64_t ir_unpack_ns;
  uint64_t ir_replay_ns;
  uint64_t ir_parse_ns;
  uint64_t ir_capture_ns;
  uint64_t ir_pack_ns;
};
#ifdef __cplusplus
static_assert(sizeof(ZilNativeProductStats) == 32 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, world_projection_direct) == 22 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, world_projection_fallback) == 23 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, tli_projection_builds) == 24 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, tli_projection_reuses) == 25 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, ir_key_ns) == 26 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, ir_unpack_ns) == 27 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, ir_replay_ns) == 28 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, ir_parse_ns) == 29 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, ir_capture_ns) == 30 * sizeof(uint64_t));
static_assert(offsetof(ZilNativeProductStats, ir_pack_ns) == 31 * sizeof(uint64_t));
#endif
// Independent optional observer; the original 32-u64 ABI remains unchanged.
struct ZilNativeStageBodyStatsV1 {
  uint64_t captures, replays, declines, captured_bytes;
  uint64_t capture_ns, prepare_ns, restore_ns, failed_replays;
};
int ZilNativeProductsReadStageBodyStatsV1(const void *request, void *out, size_t bytes);
int ZilNativeStageBodyCounterexamplesV1(void);
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
int ZilNativeProductsAttributeMetadataCounterexample(void);
int ZilNativeProductsModuleHeaderCounterexample(void);
int ZilNativeProductsUnchangedStageCounterexample(void);
int ZilNativeProductsEligibilityProducerCounterexample(void);
int ZilNativeProductsTypeAndOwnerCounterexamples(void);
int ZilNativeAnalysisUnsupportedKeysCounterexample(void);
int ZilNativeIRScopeTimerCounterexample(void);
int ZilNativePackedStageEntryFilterCounterexample(void);
int ZilNativeStageEconomicsCounterexample(void);
// Explicit same-thread fixture override; bypasses economics only, never proofs.
int ZilNativeProductsStageAlwaysForTests(int enabled);
#ifdef __cplusplus
}
#endif
#endif
