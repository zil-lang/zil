// Fixed LLVM 7a941df /23.1.4 owner-free frame transport. Public full-value copy
// assignment preserves every private MachineFrameInfo field; YAML is not a
// proof of frame completeness. This header does not alter any LLVM class ABI.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_MACHINE_FRAME_PRODUCTS_H
#define ZIL_MACHINE_FRAME_PRODUCTS_H
#include "llvm/CodeGen/MachineFrameInfo.h"
#include "llvm/CodeGen/MachineFunction.h"
#include "llvm/CodeGen/MIRYamlMapping.h"
#include "llvm/MC/MCDwarf.h"
#include "llvm/Support/YAMLTraits.h"
#include "llvm/Target/TargetMachine.h"
#include <cstdint>
#include <limits>
#include <string>
#include <map>
#include <memory>
#include <tuple>
#include <type_traits>
#include <vector>
namespace zil {
namespace frame {
using Points = std::map<unsigned, std::vector<llvm::CalleeSavedInfo>>;
struct State {
  // All Alloca pointers are null; both BB-pointer maps are empty in this copy.
  // The maps are stored separately as current-layout ordinals and scalar CSI.
  std::shared_ptr<const llvm::MachineFrameInfo> values;
  Points saves, restores;
  std::vector<llvm::MCCFIInstruction> cfi;
  unsigned blocks = 0;
  size_t chargedBytes = 0;
};
static_assert(std::is_copy_assignable_v<llvm::MachineFrameInfo>,
              "complete public frame assignment is required; no partial field substitute");
static_assert(std::is_copy_constructible_v<llvm::MCCFIInstruction>);

inline bool ownerFreeCFI(const llvm::MCCFIInstruction &I) {
  using C = llvm::MCCFIInstruction;
  if (I.getLabel() || I.getLoc().isValid()) return false;
  switch (I.getOperation()) {
  case C::OpLabel: return false; // an emission symbol, never a transferable owner
  case C::OpEscape: return !I.getExtraFields<C::EscapeFields>().Values.empty();
  case C::OpSameValue: case C::OpRememberState: case C::OpRestoreState:
  case C::OpOffset: case C::OpLLVMDefAspaceCfa: case C::OpDefCfaRegister:
  case C::OpDefCfaOffset: case C::OpDefCfa: case C::OpRelOffset:
  case C::OpAdjustCfaOffset: case C::OpRestore: case C::OpUndefined:
  case C::OpRegister: case C::OpWindowSave: case C::OpNegateRAState:
  case C::OpNegateRAStateWithPC: case C::OpGnuArgsSize: case C::OpValOffset:
  case C::OpLLVMRegisterPair: case C::OpLLVMVectorRegisters:
  case C::OpLLVMVectorOffset: case C::OpLLVMVectorRegisterMask: return true;
  }
  return false;
}
inline size_t cfiDynamicBytes(const llvm::MCCFIInstruction &I) {
  using C = llvm::MCCFIInstruction;
  if (I.getOperation() == C::OpEscape) {
    const auto &E = I.getExtraFields<C::EscapeFields>(); return E.Values.size() + E.Comment.size();
  }
  if (I.getOperation() == C::OpLLVMVectorRegisters)
    return I.getExtraFields<C::VectorRegistersFields>().VectorRegisters.size() * sizeof(C::VectorRegisterWithLane);
  return 0;
}
inline bool sameCFI(const llvm::MCCFIInstruction &A, const llvm::MCCFIInstruction &B) {
  using C = llvm::MCCFIInstruction;
  if (!ownerFreeCFI(A) || !ownerFreeCFI(B) || A.getOperation() != B.getOperation()) return false;
  switch (A.getOperation()) {
  case C::OpEscape: {
    const auto &X = A.getExtraFields<C::EscapeFields>(), &Y = B.getExtraFields<C::EscapeFields>();
    return X.Values == Y.Values && X.Comment == Y.Comment;
  }
  case C::OpLLVMRegisterPair: {
    const auto &X = A.getExtraFields<C::RegisterPairFields>(), &Y = B.getExtraFields<C::RegisterPairFields>();
    return std::tie(X.Register, X.Reg1, X.Reg2, X.Reg1SizeInBits, X.Reg2SizeInBits) ==
           std::tie(Y.Register, Y.Reg1, Y.Reg2, Y.Reg1SizeInBits, Y.Reg2SizeInBits);
  }
  case C::OpLLVMVectorRegisters: {
    const auto &X = A.getExtraFields<C::VectorRegistersFields>(), &Y = B.getExtraFields<C::VectorRegistersFields>();
    if (X.Register != Y.Register || X.VectorRegisters.size() != Y.VectorRegisters.size()) return false;
    for (size_t I = 0; I != X.VectorRegisters.size(); ++I) {
      const auto &L = X.VectorRegisters[I], &R = Y.VectorRegisters[I];
      if (std::tie(L.Register, L.Lane, L.SizeInBits) != std::tie(R.Register, R.Lane, R.SizeInBits)) return false;
    }
    return true;
  }
  case C::OpLLVMVectorOffset: {
    const auto &X = A.getExtraFields<C::VectorOffsetFields>(), &Y = B.getExtraFields<C::VectorOffsetFields>();
    return std::tie(X.Register, X.RegisterSizeInBits, X.Offset, X.MaskRegister, X.MaskRegisterSizeInBits) ==
           std::tie(Y.Register, Y.RegisterSizeInBits, Y.Offset, Y.MaskRegister, Y.MaskRegisterSizeInBits);
  }
  case C::OpLLVMVectorRegisterMask: {
    const auto &X = A.getExtraFields<C::VectorRegisterMaskFields>(), &Y = B.getExtraFields<C::VectorRegisterMaskFields>();
    return std::tie(X.Register, X.SpillRegister, X.SpillRegisterLaneSizeInBits, X.MaskRegister, X.MaskRegisterSizeInBits) ==
           std::tie(Y.Register, Y.SpillRegister, Y.SpillRegisterLaneSizeInBits, Y.MaskRegister, Y.MaskRegisterSizeInBits);
  }
  case C::OpLabel: return false;
  default: {
    const auto &X = A.getExtraFields<C::CommonFields>(), &Y = B.getExtraFields<C::CommonFields>();
    return std::tie(X.Register, X.Offset, X.Register2, X.AddressSpace) ==
           std::tie(Y.Register, Y.Offset, Y.Register2, Y.AddressSpace);
  }
  }
}
inline bool validIndex(const llvm::MachineFrameInfo &F, int FI) {
  return FI >= F.getObjectIndexBegin() && FI < F.getObjectIndexEnd() && !F.isDeadObjectIndex(FI);
}
inline bool domain(const llvm::MachineFunction &MF, size_t Cap) {
  const auto &F = MF.getFrameInfo();
  if (F.getNumObjects() > Cap / sizeof(llvm::MachineFrameInfo) ||
      F.getNumFixedObjects() > F.getNumObjects() ||
      F.hasVarSizedObjects() || F.isFrameAddressTaken() || F.isReturnAddressTaken() ||
      F.hasStackMap() || F.hasPatchPoint() || F.hasCalls() || F.hasTailCall() ||
      F.hasVAStart() || F.hasMustTailInVarArgFunc() || F.hasOpaqueSPAdjustment() ||
      F.hasStackProtectorIndex() || F.hasFunctionContextIndex() || F.getUnsafeStackSize()) return false;
  for (int FI = F.getObjectIndexBegin(); FI != F.getObjectIndexEnd(); ++FI)
    if (F.getObjectAllocation(FI) || F.isStatepointSpillSlotObjectIndex(FI) ||
        (!F.isDeadObjectIndex(FI) && !F.isFixedObjectIndex(FI) && !F.isSpillSlotObjectIndex(FI))) return false;
  for (const auto &I : MF.getFrameInstructions()) if (!ownerFreeCFI(I)) return false;
  return true;
}
inline bool capture(const llvm::MachineFunction &MF, State &Out, size_t Cap) {
  if (!domain(MF, Cap) || MF.size() > std::numeric_limits<unsigned>::max()) return false;
  const auto &F = MF.getFrameInfo();
  std::map<const llvm::MachineBasicBlock *, unsigned> Blocks;
  for (const auto &B : MF) Blocks.emplace(&B, Blocks.size());
  size_t N = sizeof(State) + sizeof(llvm::MachineFrameInfo);
  auto Charge = [&](size_t Bytes) { if (N > Cap || Bytes > Cap - N) return false; N += Bytes; return true; };
  // The pinned private StackObject is smaller than MachineFrameInfo (the
  // latter embeds 32 local-map rows). Conservatively charge one entire frame
  // per object rather than guess private padding or report allocator RSS.
  if (!Charge(size_t(F.getNumObjects()) * sizeof(llvm::MachineFrameInfo)) ||
      !Charge(F.getCalleeSavedInfo().size() * sizeof(llvm::CalleeSavedInfo)) ||
      !Charge(size_t(F.getLocalFrameObjectCount()) * sizeof(std::pair<int, int64_t>))) return false;
  auto CapturePoints = [&](const llvm::SaveRestorePoints &Source, Points &Dest) {
    for (const auto &P : Source) {
      auto It = Blocks.find(P.first);
      if (It == Blocks.end() || !Charge(sizeof(Points::value_type)) ||
          !Charge(P.second.size() * sizeof(llvm::CalleeSavedInfo))) return false;
      Dest.emplace(It->second, P.second);
    }
    return true;
  };
  State S;
  if (!CapturePoints(F.getSavePoints(), S.saves) || !CapturePoints(F.getRestorePoints(), S.restores)) return false;
  for (const auto &I : MF.getFrameInstructions()) {
    if (!Charge(sizeof(llvm::MCCFIInstruction)) || !Charge(cfiDynamicBytes(I))) return false;
    S.cfi.push_back(I); // complete active variant, strings and vectors included
  }
  auto Values = std::make_shared<llvm::MachineFrameInfo>(llvm::Align(1), false, false);
  *Values = F; // complete member-wise deep copy, including EVERY private scalar
  Values->clearSavePoints(); Values->clearRestorePoints(); // no old MBB owner escapes
  S.values = std::move(Values); S.blocks = MF.size(); S.chargedBytes = N;
  Out = std::move(S); return true;
}
inline bool restore(llvm::MachineFunction &MF, const State &S, size_t Cap) {
  if (!S.values || S.chargedBytes > Cap || MF.size() != S.blocks ||
      !S.values->getSavePoints().empty() || !S.values->getRestorePoints().empty() ||
      !MF.getFrameInstructions().empty()) return false;
  for (int FI = S.values->getObjectIndexBegin(); FI != S.values->getObjectIndexEnd(); ++FI)
    if (S.values->getObjectAllocation(FI)) return false;
  std::vector<llvm::MachineBasicBlock *> Blocks;
  for (auto &B : MF) Blocks.push_back(&B);
  auto Rebind = [&](const Points &Points, llvm::SaveRestorePoints &Out) {
    for (const auto &P : Points) {
      if (P.first >= Blocks.size()) return false;
      Out.try_emplace(Blocks[P.first], P.second);
    }
    return true;
  };
  llvm::SaveRestorePoints Saves, Restores;
  if (!Rebind(S.saves, Saves) || !Rebind(S.restores, Restores)) return false;
  for (const auto &I : S.cfi) if (!ownerFreeCFI(I)) return false;
  MF.getFrameInfo() = *S.values;
  MF.getFrameInfo().setSavePoints(std::move(Saves));
  MF.getFrameInfo().setRestorePoints(std::move(Restores));
  for (unsigned I = 0; I != S.cfi.size(); ++I)
    if (MF.addFrameInst(S.cfi[I]) != I) return false;
  return true;
}
// Save/restore point strings refer to MBBs, which the tiny parser intentionally
// has not created. Parse the fixed official YAML schema and remove ONLY those
// maps; their complete scalar CSI values/current BB mapping are restored above.
inline std::string headerForEmptyBody(llvm::StringRef Header, const llvm::TargetMachine &TM,
                                      bool HasPoints) {
  if (!HasPoints) return Header.str();
  llvm::yaml::MachineFunction Y;
  Y.MachineFuncInfo.reset(TM.createDefaultFuncInfoYAML());
  llvm::yaml::Input In(Header); In >> Y;
  if (In.error()) return {};
  Y.FrameInfo.SavePoints.clear(); Y.FrameInfo.RestorePoints.clear();
  std::string S; llvm::raw_string_ostream OS(S);
  { llvm::yaml::Output Out(OS); Out << Y; }
  return S;
}
} // namespace frame
} // namespace zil
#endif
