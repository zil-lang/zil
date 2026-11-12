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
};
// A synchronous per-thread scope; callers must restore the previous pointer.
LLVM_ABI const ZilLLVMFunctionProductHooks *
ZilLLVMSetFunctionProductHooks(const ZilLLVMFunctionProductHooks *);
LLVM_ABI const ZilLLVMFunctionProductHooks *ZilLLVMGetFunctionProductHooks();
}
#endif
