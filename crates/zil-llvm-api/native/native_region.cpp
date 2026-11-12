// NativeRegion ownership prototype, fixed LLVM7a. SDK capability gaps are
// ordinary fresh fallback; they are not counted as successful region reuse.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "native_region.h"
#include "zil_llvm_native_region.h"
#include "llvm/IR/Module.h"
#include "llvm/Target/TargetMachine.h"
#include <dlfcn.h>
#include <utility>
namespace zil {
namespace {
struct API {
  ZilRegionCapabilities capabilities = reinterpret_cast<ZilRegionCapabilities>(dlsym(RTLD_DEFAULT, "ZilLLVMNativeRegionCapabilities"));
  ZilRegionCreate create = reinterpret_cast<ZilRegionCreate>(dlsym(RTLD_DEFAULT, "ZilLLVMNativeRegionCreate"));
  ZilRegionDispose dispose = reinterpret_cast<ZilRegionDispose>(dlsym(RTLD_DEFAULT, "ZilLLVMNativeRegionDispose"));
  ZilRegionEmit emit = reinterpret_cast<ZilRegionEmit>(dlsym(RTLD_DEFAULT, "ZilLLVMNativeRegionEmit"));
  ZilRegionSeal seal = reinterpret_cast<ZilRegionSeal>(dlsym(RTLD_DEFAULT, "ZilLLVMNativeRegionSealAllocationLease"));
  ZilRegionValidateLease validate = reinterpret_cast<ZilRegionValidateLease>(dlsym(RTLD_DEFAULT, "ZilLLVMNativeRegionValidateAllocationLease"));
  ZilRegionDispose releaseLease = reinterpret_cast<ZilRegionDispose>(dlsym(RTLD_DEFAULT, "ZilLLVMNativeRegionReleaseAllocationLease"));
  bool ready() const {
    return capabilities && create && dispose && emit && seal && validate && releaseLease &&
        (capabilities() & ZilRegionRequiredCapabilities) == ZilRegionRequiredCapabilities;
  }
};
const API &api() { static const API A; return A; }
}
struct NativeRegionSession::State {
  llvm::Module *Module;
  llvm::TargetMachine *Machine;
  void *Emission;
  bool Emitted = false, Adopted = false;
  State(llvm::Module &M, llvm::TargetMachine &TM, void *E) : Module(&M), Machine(&TM), Emission(E) {}
  ~State() { if (Emission) api().dispose(Emission); }
};
struct NativeRegion::State {
  void *Context = nullptr, *Module = nullptr, *Machine = nullptr;
  void *Emission = nullptr, *Lease = nullptr;
  NativeRegionSession::DisposeOwner DisposeContext = nullptr, DisposeModule = nullptr, DisposeMachine = nullptr;
  uint64_t Bytes = 0;
  ~State() {
    // Emission destroys the ORIGINAL-address MMIWrapper/MFs first, then the
    // external MCContext. Module/Context/TM remain alive throughout that step.
    if (Emission) api().dispose(Emission);
    if (Machine) DisposeMachine(Machine);
    if (Module) DisposeModule(Module);
    if (Context) DisposeContext(Context);
    // Allocation ledger/arena ownership must outlive every matching destructor.
    if (Lease) api().releaseLease(Lease);
  }
};
NativeRegionSession::NativeRegionSession(std::unique_ptr<State> P) : S(std::move(P)) {}
NativeRegionSession::~NativeRegionSession() = default;
NativeRegion::NativeRegion(std::unique_ptr<State> P) : S(std::move(P)) {}
NativeRegion::~NativeRegion() = default;
uint64_t NativeRegion::ownedBytes() const { return S->Bytes; }
std::unique_ptr<NativeRegionSession> NativeRegionSession::create(bool Enabled,
    llvm::Module &M, llvm::TargetMachine &TM) {
  if (!Enabled || !api().ready() || !TM.getTargetTriple().isOSLinux() ||
      TM.getTargetTriple().getArch() != llvm::Triple::x86_64) return {};
  void *Emission = api().create(&M, &TM);
  if (!Emission) return {};
  return std::unique_ptr<NativeRegionSession>(new NativeRegionSession(
      std::make_unique<State>(M, TM, Emission)));
}
int NativeRegionSession::emit(void **Buffer, char **Message) {
  if (!Buffer || !Message || S->Emitted || S->Adopted) return 1;
  *Buffer = nullptr; *Message = nullptr;
  const int Failed = api().emit(S->Emission, Buffer, Message);
  S->Emitted = !Failed && *Buffer;
  return Failed;
}
std::shared_ptr<const NativeRegion> NativeRegionSession::adopt(void *Context,
    void *Module, void *TM, DisposeOwner DC, DisposeOwner DM, DisposeOwner DT,
    uint64_t Limit) {
  if (!S->Emitted || S->Adopted || !Context || !DC || !DM || !DT || !Limit ||
      Module != S->Module || TM != S->Machine || Context != &S->Module->getContext()) return {};
  uint64_t Bytes = 0;
  void *Lease = api().seal(S->Emission, Limit, &Bytes);
  if (!Lease) return {};
  if (!Bytes || Bytes > Limit || !api().validate(Lease, S->Emission, Context, Module, TM, Limit)) {
    api().releaseLease(Lease); return {};
  }
  auto Owned = std::make_unique<NativeRegion::State>();
  Owned->Context = Context; Owned->Module = Module; Owned->Machine = TM;
  Owned->DisposeContext = DC; Owned->DisposeModule = DM; Owned->DisposeMachine = DT;
  Owned->Emission = S->Emission; Owned->Lease = Lease; Owned->Bytes = Bytes;
  auto Result = std::shared_ptr<const NativeRegion>(new NativeRegion(std::move(Owned)));
  S->Emission = nullptr; S->Adopted = true;
  return Result;
}
}
