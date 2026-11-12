// Private ZIL adapter hook for LLVM 23.1.4 commit
// 7a941dfb2877a0b8cf30323fefa85ad019e0b0f5. No hook means original LLVM behavior.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_LLVM_WORK_PRODUCTS_HOOKS_H
#define ZIL_LLVM_WORK_PRODUCTS_HOOKS_H
#include "llvm/Support/Compiler.h"
#include <cstdint>

extern "C" {
struct ZilLLVMFunctionProductHooks {
  uint64_t abi_version;
  void *context;
  // Handles borrow current pass/function/MMI only for this call. Nonzero skips
  // this machine-function pass; the adapter owns the complete replay proof.
  int (*before_machine_pass)(void *, void *, void *, void *);
  // Original PM scopes identify construction position, including repeated
  // CGSCC visits. These are borrowed callbacks, never serialized pointers.
  void (*enter_pass)(void *, unsigned, unsigned);
  void (*leave_pass)(void *);
  int (*before_function_stage)(void *, void *, void *, void *, void *, void *, void **);
  void (*after_function_stage)(void *, void *, void *, void *, const void *, void *);
  int (*legacy_pass_was_skipped)(void *, void *, void *);
  // ABI 2 only. Bound to this synchronous native invocation, not a possibly
  // invalidated/deleted SCC. Call immediately after a child and before any
  // invalidation, instrumentation-after callback or further transformation.
  int (*request_failed)(void *);
};
// A synchronous per-thread scope; callers must restore the previous pointer.
LLVM_ABI const ZilLLVMFunctionProductHooks *
ZilLLVMSetFunctionProductHooks(const ZilLLVMFunctionProductHooks *);
LLVM_ABI const ZilLLVMFunctionProductHooks *ZilLLVMGetFunctionProductHooks();
LLVM_ABI unsigned ZilLLVMMachineProductCapability();
LLVM_ABI unsigned ZilLLVMFunctionStageCapability();
LLVM_ABI unsigned ZilLLVMLegacyLifecycleCapability();
LLVM_ABI unsigned ZilLLVMFunctionProductHookABI();
static inline bool ZilLLVMProductHookPrefix(const ZilLLVMFunctionProductHooks *H) {
  return H && (H->abi_version == 1 || H->abi_version == 2);
}
static inline bool ZilLLVMProductRequestFailed(const ZilLLVMFunctionProductHooks *H) {
  return H && H->abi_version == 2 && H->request_failed && H->request_failed(H->context);
}
}
#endif
