// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "machine_products.h"
#include "X86MachineFunctionInfo.h"
#include "llvm/CodeGen/MIRParser/MIRParser.h"
#include "llvm/CodeGen/MIRPrinter.h"
#include "llvm/CodeGen/MachineConstantPool.h"
#include "llvm/CodeGen/MachineFrameInfo.h"
#include "llvm/CodeGen/MachineFunction.h"
#include "llvm/CodeGen/MachineInstr.h"
#include "llvm/CodeGen/MachineModuleInfo.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/LegacyPassManagers.h"
#include "llvm/IR/Module.h"
#include "llvm/Pass.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Target/TargetMachine.h"
#include <dlfcn.h>
#include <map>
#include <set>
#include <utility>
#include <vector>

namespace zil {
namespace {
using ID = const void *;
struct TargetInfoAPI {
  void *(*clone)(void *);
  int (*restore)(void *, const void *);
  void (*dispose)(void *);
  const void *(*printerID)();
  bool available() const { return clone && restore && dispose && printerID; }
};
const TargetInfoAPI &targetInfoAPI() {
  static const TargetInfoAPI API{
      reinterpret_cast<void *(*)(void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMCloneX86ProductInfo")),
      reinterpret_cast<int (*)(void *, const void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMRestoreX86ProductInfo")),
      reinterpret_cast<void (*)(void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMDisposeX86ProductInfo")),
      reinterpret_cast<const void *(*)()>(dlsym(RTLD_DEFAULT, "ZilLLVMAsmPrinterPassID"))};
  return API;
}
struct PassStep {
  ID id;
  std::vector<ID> dependencies;
  bool machine;
  bool preservesAll;
  bool operator==(const PassStep &Other) const {
    return id == Other.id && dependencies == Other.dependencies &&
           machine == Other.machine && preservesAll == Other.preservesAll;
  }
};
PassStep describe(llvm::Pass &P, bool Machine) {
  llvm::AnalysisUsage AU;
  P.getAnalysisUsage(AU);
  PassStep S{P.getPassID(), {}, Machine, AU.getPreservesAll()};
  for (ID D : AU.getRequiredSet()) S.dependencies.push_back(D);
  for (ID D : AU.getRequiredTransitiveSet()) S.dependencies.push_back(D);
  // Optional available analyses must not silently disappear on replay either.
  for (ID D : AU.getUsedSet()) S.dependencies.push_back(D);
  return S;
}
struct Snapshot {
  std::string input, world, mir;
  std::shared_ptr<const void> targetInfo;
  llvm::MachineFunctionProperties properties;
  std::vector<PassStep> pipeline, sequence;
  size_t firstMachine = 0;
  std::set<ID> emissionProviders;
};

// Inspect the actual current legacy container before touching current MIR.
// IDs name fixed-DSO passes; no Pass/PM instance pointer is retained.
bool bindPipeline(llvm::Pass &P, Snapshot &S) {
  if (!P.getResolver()) return false;
  auto &PM = P.getResolver()->getPMDataManager();
  if (PM.getPassManagerType() != llvm::PMT_FunctionPassManager) return false;
  auto &FPM = static_cast<llvm::FPPassManager &>(PM);
  bool Found = false;
  for (unsigned I = 0; I < FPM.getNumContainedPasses(); ++I) {
    auto *Current = FPM.getContainedPass(I);
    if (Current == &P) { S.firstMachine = I; Found = true; }
    S.pipeline.push_back(describe(*Current, false));
  }
  return Found;
}

bool inputDomain(const llvm::Function &F) {
  if (!eligibleFunction(F) || !F.getReturnType()->isIntegerTy() ||
      F.getReturnType()->getIntegerBitWidth() > 64)
    return false;
  for (const auto &A : F.args())
    if (!A.getType()->isIntegerTy() || A.getType()->getIntegerBitWidth() > 64)
      return false;
  for (const auto &BB : F)
    for (const auto &I : BB) {
      // eligibleFunction admits only canonical overflow/trap intrinsics.
      // Fixed7a X86ISelLowering custom-lowers scalar i8/i16/i32/i64 overflow
      // to ADD/SUB/SMUL/UMUL + EFLAGS (LowerXALUO; scalar LowerMULO delegates).
      // The ordinary trap is legal ISD::TRAP -> UD2, with no CallLoweringInfo.
      // An explicit trap-func-name would instead lower a call: exclude it.
      if (const auto *Call = llvm::dyn_cast<llvm::CallBase>(&I))
        if (!Call->getAttributes().getFnAttr("trap-func-name").getValueAsString().empty())
          return false;
      if (I.getType()->isIntegerTy() && I.getType()->getIntegerBitWidth() > 64)
        return false;
      for (const auto &U : I.operands())
        if (U->getType()->isIntegerTy() && U->getType()->getIntegerBitWidth() > 64)
          return false;
    }
  return true;
}

bool snapshotDomain(llvm::MachineFunction &MF) {
  const auto &Frame = MF.getFrameInfo();
  llvm::errs() << "ZIL MIR domain " << MF.getName() << " frame=" << Frame.getObjectIndexBegin() << ":" << Frame.getObjectIndexEnd() << " stack=" << Frame.getStackSize() << " calls=" << Frame.hasCalls() << " jt=" << bool(MF.getJumpTableInfo()) << " cp=" << MF.getConstantPool()->getConstants().size() << " cfi=" << MF.getFrameInstructions().size() << "\n";
  // MIR renumbers stack objects and drops dead slots. No frame index may enter
  // the opaque target-info copy until a separate relocation proof exists.
  if (Frame.getObjectIndexBegin() != 0 || Frame.getObjectIndexEnd() != 0 ||
      Frame.getStackSize() != 0 || Frame.hasCalls() ||
      MF.getJumpTableInfo() || !MF.getConstantPool()->getConstants().empty() ||
      !MF.getFrameInstructions().empty())
    return false;
  auto *X = MF.getInfo<llvm::X86MachineFunctionInfo>();
  if (X) llvm::errs() << "ZIL MIR target " << MF.getName() << " ra=" << X->getRAIndex() << " fa=" << X->getFAIndex() << " va=" << X->getVarArgsFrameIndex() << " regsave=" << X->getRegSaveFrameIndex() << " seh=" << X->getSEHFramePtrSaveIndex() << " sret=" << unsigned(X->getSRetReturnReg()) << " global=" << unsigned(X->getGlobalBaseReg()) << "\n";
  if (!X || X->getRAIndex() || X->getFAIndex() || X->getVarArgsFrameIndex() ||
      X->getRegSaveFrameIndex() || X->getSEHFramePtrSaveIndex() ||
      X->getSwiftAsyncContextFrameIdx() || X->getSRetReturnReg() ||
      X->getGlobalBaseReg() || !X->getForwardedMustTailRegParms().empty() ||
      X->getStackPtrSaveMI() || X->hasPreallocatedCall() ||
      X->hasSwiftAsyncContext() || X->getHasSEHFramePtrSave())
    return false;
  for (const auto &BB : MF)
    for (const auto &MI : BB) {
      // X86 TRAP/UD2 deliberately carries mayLoad for scheduling, although it
      // has no memory operand. Other trapping forms (UD1 memory etc.) have
      // explicit operands and remain outside this no-memory snapshot domain.
      const bool PlainTrap = MI.getDesc().isTrap() && MI.getNumExplicitOperands() == 0;
      if (MI.isCall() || MI.isInlineAsm() || (MI.mayLoad() && !PlainTrap) || MI.mayStore() ||
          !MI.memoperands_empty() || MI.getDebugLoc())
        return false;
      // References are exclusively current physical registers, scalar values
      // and current MBBs. No symbol, global, metadata, regmask or pooled owner.
      for (const auto &MO : MI.operands()) {
        if (MO.isReg()) {
          if (MO.getReg().isVirtual()) return false;
        } else if (!MO.isImm() && !MO.isCImm() && !MO.isMBB()) {
          return false;
        }
      }
    }
  return true;
}

std::string machineText(const llvm::MachineModuleInfo &MMI,
                        const llvm::MachineFunction &MF) {
  std::string Text;
  llvm::raw_string_ostream OS(Text);
  // Deliberately omit embedded IR. MIR's local slot lookup is reconstructed
  // from MF.getFunction(), never from a parsed historical Module.
  llvm::printMIR(OS, MMI, MF);
  return Text;
}

bool providers(Snapshot &S) {
  std::vector<ID> Pending;
  for (const auto &Step : S.sequence)
    if (!Step.machine || Step.id == targetInfoAPI().printerID())
      Pending.insert(Pending.end(), Step.dependencies.begin(), Step.dependencies.end());
  while (!Pending.empty()) {
    ID Current = Pending.back();
    Pending.pop_back();
    if (!S.emissionProviders.insert(Current).second) continue;
    for (const auto &Step : S.sequence) {
      if (Step.id != Current) continue;
      // Required providers run on restored final MIR. Only genuinely
      // non-transforming providers are admissible, not registry analysis tags.
      if (Step.machine && !Step.preservesAll) { llvm::errs() << "ZIL MIR provider rejected " << Current << "\n"; return false; }
      Pending.insert(Pending.end(), Step.dependencies.begin(), Step.dependencies.end());
    }
  }
  return true;
}
} // namespace

struct MachineProducts {
  std::string config;
  std::map<std::string, std::shared_ptr<const Snapshot>> functions;
};
struct MachineRequest {
  struct Current {
    std::shared_ptr<Snapshot> recording;
    std::shared_ptr<const Snapshot> replay;
    std::vector<PassStep> sequence;
    llvm::Pass *pendingMachine = nullptr;
    llvm::Pass *skipped = nullptr;
    bool emitted = false;
  };
  llvm::Module &module;
  llvm::TargetMachine &machine;
  std::string config;
  std::shared_ptr<const MachineProducts> prior;
  ZilNativeProductStats &stats;
  bool &failed;
  std::map<const llvm::Function *, Current> current;

  void fail() {
    if (!failed) ++stats.failed_replays;
    failed = true;
  }
};

std::unique_ptr<MachineRequest, void (*)(MachineRequest *)> createMachine(
    llvm::Module &M, llvm::TargetMachine &TM, const std::string &Config,
    std::shared_ptr<const MachineProducts> Prior, ZilNativeProductStats &Stats,
    bool &Failed) {
  if (!targetInfoAPI().available())
    return {nullptr, [](MachineRequest *R) { delete R; }};
  return {new MachineRequest{M, TM, Config, std::move(Prior), Stats, Failed, {}},
          [](MachineRequest *R) { delete R; }};
}

int beforeMachine(MachineRequest &R, llvm::Pass &P, llvm::Function &F,
                  llvm::MachineModuleInfo &MMI) {
  const bool Printer = P.getPassID() == targetInfoAPI().printerID();
  auto [It, New] = R.current.try_emplace(&F);
  auto &C = It->second;
  C.pendingMachine = &P;
  if (R.failed) {
    C.skipped = &P;
    return 1;
  }
  if (New) {
    const auto &Triple = R.machine.getTargetTriple();
    if (Triple.getArch() == llvm::Triple::x86_64 && Triple.isOSLinux() &&
        inputDomain(F)) {
      C.recording = std::make_shared<Snapshot>();
      if (!bindPipeline(P, *C.recording)) C.recording.reset();
    }
    llvm::errs() << "ZIL MIR begin " << F.getName() << " pass=" << P.getPassName() << " recording=" << bool(C.recording) << " prior=" << bool(R.prior) << "\n";
    if (C.recording) {
      C.recording->input = functionText(F);
      C.recording->world = worldText(R.module, &F);
      R.stats.mir_key_bytes += C.recording->input.size() + C.recording->world.size();
      if (R.prior && R.prior->config == R.config) {
        auto Old = R.prior->functions.find(F.getName().str());
        llvm::errs() << "ZIL MIR lookup " << F.getName() << " old=" << (Old != R.prior->functions.end()) << " mf=" << bool(MMI.getMachineFunction(F)) << "\n";
        if (Old != R.prior->functions.end() &&
            Old->second->input == C.recording->input &&
            Old->second->world == C.recording->world &&
            Old->second->pipeline == C.recording->pipeline &&
            Old->second->firstMachine == C.recording->firstMachine &&
            !Old->second->sequence.empty() &&
            Old->second->sequence.front() == describe(P, true) &&
            !MMI.getMachineFunction(F)) {
          C.replay = Old->second;
          auto Parser = llvm::createMIRParser(
              llvm::MemoryBuffer::getMemBufferCopy(C.replay->mir, "zil-current-machine"),
              R.module.getContext());
          // Required to initialize the YAML cursor. This header is empty;
          // parseMachineFunctions explicitly receives the CURRENT Module/MMI.
          auto EmptyHeader = Parser->parseIRModule();
          if (!EmptyHeader || Parser->parseMachineFunctions(R.module, MMI)) {
            R.fail();
          } else {
            auto *MF = MMI.getMachineFunction(F);
            if (!MF || !targetInfoAPI().restore(MF, C.replay->targetInfo.get())) {
              R.fail();
            } else {
              MF->getProperties() = C.replay->properties;
              if (!snapshotDomain(*MF) ||
                  !MF->verify(nullptr, "ZIL current MIR replay", nullptr, false) ||
                  machineText(MMI, *MF) != C.replay->mir)
                R.fail();
            }
          }
          if (!R.failed) {
            ++R.stats.machine_function_reuses;
            R.stats.mir_replay_bytes += C.replay->mir.size();
          }
        }
      }
    }
    if (!C.replay) ++R.stats.machine_function_executions;
  }
  if (C.replay &&
      (C.sequence.size() >= C.replay->sequence.size() ||
       !(C.replay->sequence[C.sequence.size()] == describe(P, true))))
    R.fail();
  if (R.failed) {
    C.skipped = &P;
    return 1;
  }
  if (Printer) {
    ++R.stats.asm_printer_runs;
    C.emitted = true;
    if (!C.replay && C.recording) {
      llvm::errs() << "ZIL MIR printer " << F.getName() << " inputeq=" << (C.recording->input == functionText(F)) << " worldeq=" << (C.recording->world == worldText(R.module, &F)) << "\n";
      auto *MF = MMI.getMachineFunction(F);
      if (!MF || !snapshotDomain(*MF) ||
          C.recording->input != functionText(F) ||
          C.recording->world != worldText(R.module, &F)) {
        C.recording.reset();
      } else {
        void *Info = targetInfoAPI().clone(MF);
        if (!Info) {
          C.recording.reset();
        } else {
          C.recording->targetInfo = {Info, targetInfoAPI().dispose};
          C.recording->properties = MF->getProperties();
          C.recording->mir = machineText(MMI, *MF);
          R.stats.mir_snapshot_bytes += C.recording->mir.size();
        }
      }
    }
    return 0; // The actual target AsmPrinter/streamer always emits this object.
  }
  if (C.replay && !C.replay->emissionProviders.count(P.getPassID())) {
    C.skipped = &P;
    ++R.stats.machine_pass_skips;
    return 1;
  }
  return 0;
}

int machineWasSkipped(MachineRequest &R, llvm::Pass &P, llvm::Function &F) {
  auto It = R.current.find(&F);
  if (It == R.current.end()) return 0;
  auto &C = It->second;
  const bool Machine = C.pendingMachine == &P;
  C.pendingMachine = nullptr;
  const bool Skipped = C.skipped == &P;
  C.skipped = nullptr;
  // Unsupported current functions execute the original pipeline without
  // allocating a duplicate per-pass dependency trace that cannot be reused.
  if (!C.recording && !C.replay) return Skipped;
  auto Step = describe(P, Machine);
  if (C.replay &&
      (C.sequence.size() >= C.replay->sequence.size() ||
       !(C.replay->sequence[C.sequence.size()] == Step)))
    R.fail();
  C.sequence.push_back(std::move(Step));
  return Skipped;
}

std::shared_ptr<const MachineProducts> finishMachine(MachineRequest &R) {
  auto Next = std::make_shared<MachineProducts>();
  Next->config = R.config;
  for (auto &[F, C] : R.current) {
    std::shared_ptr<const Snapshot> S;
    if (C.replay) {
      // The whole actual container was bound before any restoration. Finish
      // is publication only; an incomplete observed tail discards the product.
      if (C.emitted && C.sequence.size() == C.replay->sequence.size()) S = C.replay;
    } else if (C.recording && C.emitted && C.recording->targetInfo) {
      C.recording->sequence = std::move(C.sequence);
      bool Complete = C.recording->firstMachine + C.recording->sequence.size() ==
                      C.recording->pipeline.size();
      for (size_t I = 0; Complete && I < C.recording->sequence.size(); ++I) {
        auto Step = C.recording->sequence[I];
        Step.machine = false;
        Complete = Step == C.recording->pipeline[C.recording->firstMachine + I];
      }
      const bool Providers = providers(*C.recording);
      llvm::errs() << "ZIL MIR finish " << F->getName() << " complete=" << Complete << " providers=" << Providers << " first=" << C.recording->firstMachine << " seq=" << C.recording->sequence.size() << " pipe=" << C.recording->pipeline.size() << "\n";
      if (Complete && Providers) S = std::move(C.recording);
    }
    if (S) {
      R.stats.retained_bytes += S->input.size() + S->world.size() + S->mir.size() +
                                sizeof(llvm::X86MachineFunctionInfo);
      for (const auto *Steps : {&S->sequence, &S->pipeline})
        for (const auto &Step : *Steps)
          R.stats.retained_bytes += sizeof(PassStep) + Step.dependencies.size() * sizeof(ID);
      R.stats.retained_bytes += S->emissionProviders.size() * sizeof(ID);
      Next->functions.emplace(F->getName().str(), std::move(S));
    }
  }
  if (R.failed) return nullptr;
  return Next;
}
} // namespace zil
