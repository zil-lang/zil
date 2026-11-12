// Independent private V1 extension. Never reads/extends the ABI2 hook layout.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_LLVM_MACHINE_EMISSION_HOOKS_H
#define ZIL_LLVM_MACHINE_EMISSION_HOOKS_H
#include "llvm/Support/Compiler.h"
#include <cstdint>
struct ZilLLVMFunctionProductHooks;
extern "C" {
struct ZilLLVMMachineEmissionSummaryV1 { uint32_t instructions; uint32_t real_code; };
struct ZilLLVMMachineEmissionHooksV1 {
  uint64_t abi;
  // Must equal the currently installed ABI2 pointer. Suspension/reentrant
  // ordinary compiles therefore disable the outer emitter without new guards.
  const ZilLLVMFunctionProductHooks *stage_owner;
  void *context;
  // Called after the original function prologue, before blockStart/first event.
  // SDK must supply the strict CURRENT AP state gate; false runs original code.
  int (*begin_function)(void *, void *, void *, void *, const void *);
  // 0 original+optional cold observation; 1 complete replay; -1 dirty failure.
  int (*block)(void *, void *, void *, ZilLLVMMachineEmissionSummaryV1 *,
               int (*)(void *, void *), int (*)(void *, const void *));
  void (*before_instruction)(void *, const void *);
  void (*cfi_instruction)(void *, const void *);
  void (*end_block)(void *, const void *, ZilLLVMMachineEmissionSummaryV1);
  void (*end_function)(void *, void *);
  // Only the outermost actual MCObjectStreamer::emitInstruction entry calls
  // this. Cold original EmitAndCount/MC/LFI execution ALWAYS continues.
  void (*observe_mc)(void *, const void *, const void *, const void *);
};
LLVM_ABI const ZilLLVMMachineEmissionHooksV1 *ZilLLVMSetMachineEmissionHooksV1(
    const ZilLLVMMachineEmissionHooksV1 *);
LLVM_ABI const ZilLLVMMachineEmissionHooksV1 *ZilLLVMGetMachineEmissionHooksV1();
LLVM_ABI int ZilLLVMX86MachineEmissionStateV1(const void *);
LLVM_ABI int ZilLLVMEmitAndCountX86InstructionV1(void *, void *);
LLVM_ABI const void *ZilLLVMPeekMachineBlockSymbolV1(const void *);
}
#endif
