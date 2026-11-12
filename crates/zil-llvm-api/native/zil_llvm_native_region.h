// Private fixed7a NativeRegion SDK protocol. This is independent of ABI2
// function products, so the installed v15 SDK remains an ordinary fallback.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_LLVM_NATIVE_REGION_H
#define ZIL_LLVM_NATIVE_REGION_H
#include <cstdint>
extern "C" {
enum : uint64_t {
  ZilRegionOwnerGraph = 1,
  ZilRegionTerminalTake = 2,
  ZilRegionEmissionOverlay = 4,
  ZilRegionFreshProviders = 8,
  ZilRegionEmissionWriteSet = 16,
  ZilRegionCompleteAllocationAccounting = 32,
  ZilRegionRequiredCapabilities = 63
};
// Capability is a fixed SDK implementation promise, never an input digest.
// In particular the accounting bit MUST remain zero until the SDK/stdlib
// allocation closure (including escape/free/tag/ledger accounting) is proven.
using ZilRegionCapabilities = uint64_t (*)();
using ZilRegionCreate = void *(*)(void *Module, void *TargetMachine);
using ZilRegionDispose = void (*)(void *);
using ZilRegionEmit = int (*)(void *Emission, void **Buffer, char **Message);
// Only after complete successful run/finalizers, terminal wrapper extraction,
// PM destruction and resolver detachment. Returns an SDK-owned allocation
// lease, independent of the emission owner; no caller-constructed receipts.
using ZilRegionSeal = void *(*)(void *Emission, uint64_t Limit, uint64_t *Bytes);
using ZilRegionValidateLease = int (*)(void *Lease, void *Emission, void *Context,
                                     void *Module, void *TM, uint64_t Limit);
// Scoped getters used by patched SDK readers. Null overlay preserves original
// behavior. A current-Function binding covers providers/diagnostics, not just AP.
void *ZilLLVMNativeRegionGetContext(const void *MF);
void *ZilLLVMNativeRegionGetFunction(const void *MF);
int ZilLLVMNativeRegionGetFunctionNumber(const void *MF, unsigned *Out);
int ZilLLVMNativeRegionGetSection(const void *MF, void **Out);
int ZilLLVMNativeRegionSetSection(const void *MF, void *Section);
void *ZilLLVMNativeRegionGetBlockSymbol(const void *BB, unsigned Kind);
}
#endif
