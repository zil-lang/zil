// SDK-only implementation: include from a fixed7a CodeGen TU, never link a
// second LLVM runtime. Header-reader/TU coverage and allocation capability
// remain separate prerequisites; this file alone does NOT enable admission.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "llvm/ADT/DenseMap.h"
#include "llvm/CodeGen/MachineBasicBlock.h"
#include "llvm/CodeGen/MachineFunction.h"
#include "llvm/IR/Function.h"
#include "llvm/MC/MCContext.h"
#include "llvm/Target/TargetMachine.h"
#include "zil_llvm_native_region.h"
namespace {
struct ZilRegionEmissionOverlay {
  const llvm::MachineFunction *MF;
  llvm::Function *CurrentFunction;
  const llvm::TargetMachine *CurrentTarget;
  llvm::MCContext *CurrentContext;
  unsigned FunctionNumber;
  llvm::MCSection *Section = nullptr;
  llvm::DenseMap<const llvm::MachineBasicBlock *, std::array<llvm::MCSymbol *, 3>> Symbols;
  ZilRegionEmissionOverlay *Previous;
};
thread_local ZilRegionEmissionOverlay *ZilRegionOverlay = nullptr;
ZilRegionEmissionOverlay *zilRegionOverlayFor(const void *MF) {
  return ZilRegionOverlay && ZilRegionOverlay->MF == MF ? ZilRegionOverlay : nullptr;
}
}
extern "C" LLVM_ABI void *ZilLLVMNativeRegionBeginOverlay(void *MF, void *F,
    void *TM, void *Context, unsigned Number) {
  if (!MF || !F || !TM || !Context) return nullptr;
  // The private caller must have checked current complete keys, actual live MF
  // verifier, provider closure and domain BEFORE opening this scope. Cold also
  // uses it: original MBB caches/section never acquire first-object pointers.
  auto *O = new ZilRegionEmissionOverlay{static_cast<llvm::MachineFunction *>(MF),
      static_cast<llvm::Function *>(F), static_cast<llvm::TargetMachine *>(TM),
      static_cast<llvm::MCContext *>(Context), Number, nullptr, {}, ZilRegionOverlay};
  ZilRegionOverlay = O; return O;
}
extern "C" LLVM_ABI int ZilLLVMNativeRegionEndOverlay(void *Scope) {
  auto *O = static_cast<ZilRegionEmissionOverlay *>(Scope);
  if (!O || O != ZilRegionOverlay) return 1; // caller aborts whole attempt
  ZilRegionOverlay = O->Previous; delete O; return 0;
}
extern "C" LLVM_ABI void *ZilLLVMNativeRegionSuspendOverlay() {
  auto *Old = ZilRegionOverlay; ZilRegionOverlay = nullptr; return Old;
}
extern "C" LLVM_ABI void ZilLLVMNativeRegionResumeOverlay(void *Old) {
  // Native invocation guard restores this only after all nested owners die.
  ZilRegionOverlay = static_cast<ZilRegionEmissionOverlay *>(Old);
}
extern "C" LLVM_ABI void *ZilLLVMNativeRegionGetContext(const void *MF) {
  auto *O = zilRegionOverlayFor(MF); return O ? O->CurrentContext : nullptr;
}
extern "C" LLVM_ABI void *ZilLLVMNativeRegionGetFunction(const void *MF) {
  auto *O = zilRegionOverlayFor(MF); return O ? O->CurrentFunction : nullptr;
}
extern "C" LLVM_ABI const void *ZilLLVMNativeRegionGetTarget(const void *MF) {
  auto *O = zilRegionOverlayFor(MF); return O ? O->CurrentTarget : nullptr;
}
extern "C" LLVM_ABI int ZilLLVMNativeRegionGetFunctionNumber(const void *MF, unsigned *Out) {
  auto *O = zilRegionOverlayFor(MF); if (!O || !Out) return 0;
  *Out = O->FunctionNumber; return 1;
}
extern "C" LLVM_ABI int ZilLLVMNativeRegionGetSection(const void *MF, void **Out) {
  auto *O = zilRegionOverlayFor(MF); if (!O || !Out) return 0;
  *Out = O->Section; return 1;
}
extern "C" LLVM_ABI int ZilLLVMNativeRegionSetSection(const void *MF, void *Section) {
  auto *O = zilRegionOverlayFor(MF); if (!O) return 0;
  O->Section = static_cast<llvm::MCSection *>(Section); return 1;
}
extern "C" LLVM_ABI void *ZilLLVMNativeRegionGetBlockSymbol(const void *Block, unsigned Kind) {
  auto *BB = static_cast<const llvm::MachineBasicBlock *>(Block);
  if (!BB || Kind > 2) return nullptr;
  auto *O = zilRegionOverlayFor(BB->getParent()); if (!O) return nullptr;
  auto &Symbol = O->Symbols[BB][Kind];
  if (!Symbol) {
    // Initial admitted domain has no BB sections, inline asm or EH. End/EH
    // spellings are still exact, but those features do not gain admission.
    if (Kind == 0)
      Symbol = O->CurrentContext->createBlockSymbol("BB" + llvm::Twine(O->FunctionNumber) +
          "_" + llvm::Twine(BB->getNumber()), BB->hasLabelMustBeEmitted());
    else if (Kind == 1)
      Symbol = O->CurrentContext->getOrCreateSymbol("$ehgcr_" + llvm::Twine(O->FunctionNumber) +
          "_" + llvm::Twine(BB->getNumber()));
    else
      Symbol = O->CurrentContext->createBlockSymbol("BB_END" + llvm::Twine(O->FunctionNumber) +
          "_" + llvm::Twine(BB->getNumber()), false);
  }
  return Symbol;
}
