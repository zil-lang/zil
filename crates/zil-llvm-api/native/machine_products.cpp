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
#include <mutex>
#include <set>
#include <utility>
#include <vector>

namespace zil {
namespace {
using ID = const void *;
thread_local bool MachineDiagnostics = false;
void diagnose(const llvm::Function &F, llvm::StringRef Stage,
              llvm::StringRef Detail = {}) {
  if (!MachineDiagnostics) return;
  std::string Message;
  llvm::raw_string_ostream OS(Message);
  OS << "ZIL machine replay: function=";
  OS.write_escaped(F.getName());
  OS << " stage=" << Stage;
  if (!Detail.empty()) { OS << " detail="; OS.write_escaped(Detail); }
  OS << '\n';
  static std::mutex Lock;
  std::lock_guard<std::mutex> Guard(Lock);
  llvm::errs() << Message;
}
void diagnoseDifference(const llvm::Function &F, llvm::StringRef Expected,
                        llvm::StringRef Current) {
  if (!MachineDiagnostics) return;
  size_t At = 0;
  while (At < Expected.size() && At < Current.size() && Expected[At] == Current[At]) ++At;
  const size_t Start = At > 96 ? At - 96 : 0;
  std::string Detail;
  llvm::raw_string_ostream OS(Detail);
  OS << "offset=" << At << " expected_bytes=" << Expected.size()
     << " current_bytes=" << Current.size() << " expected_window=";
  OS.write_escaped(Expected.substr(Start, 768));
  OS << " current_window=";
  OS.write_escaped(Current.substr(Start, 768));
  diagnose(F, "mir-text-difference", Detail);
}

struct TargetInfoAPI {
  void *(*clone)(void *);
  int (*restore)(void *, const void *);
  void (*dispose)(void *);
  const void *(*printerID)();
  int (*restoreBlockNumbers)(void *, unsigned, unsigned, unsigned,
                             const unsigned *, const unsigned *, uint64_t);
  bool available() const {
    return clone && restore && dispose && printerID && restoreBlockNumbers;
  }
};
const TargetInfoAPI &targetInfoAPI() {
  static const TargetInfoAPI API{
      reinterpret_cast<void *(*)(void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMCloneX86ProductInfo")),
      reinterpret_cast<int (*)(void *, const void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMRestoreX86ProductInfo")),
      reinterpret_cast<void (*)(void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMDisposeX86ProductInfo")),
      reinterpret_cast<const void *(*)()>(dlsym(RTLD_DEFAULT, "ZilLLVMAsmPrinterPassID")),
      reinterpret_cast<int (*)(void *, unsigned, unsigned, unsigned,
                              const unsigned *, const unsigned *, uint64_t)>(
          dlsym(RTLD_DEFAULT, "ZilLLVMRestoreMachineBlockNumbers"))};
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
  // MIR's parser assigns dense IDs in layout order. Preserve both original
  // numbering domains and their complete table/next-ID/epoch state separately.
  // Only scalar IDs escape; the inverse table is rebuilt with CURRENT MBBs.
  unsigned blockIDCapacity = 0, nextAnalysisNumber = 0, analysisEpoch = 0;
  std::vector<unsigned> blockNumbers, analysisNumbers;
  std::vector<PassStep> pipeline, sequence;
  size_t firstMachine = 0;
  std::set<ID> emissionProviders;
};

// Optional cache capacity is independent of every compilation/work budget.
// This measures logical retained payload, not allocator capacity or process RSS.
constexpr size_t MaxMachinePayload = 64 * 1024 * 1024;
constexpr size_t MaxMachineSlots = 4096;
size_t boundedAdd(size_t A, size_t B) {
  if (A > MaxMachinePayload || B > MaxMachinePayload - A)
    return MaxMachinePayload + 1;
  return A + B;
}
size_t stepsPayload(const std::vector<PassStep> &Steps) {
  size_t N = 0;
  for (const auto &Step : Steps) {
    N = boundedAdd(N, sizeof(PassStep));
    for (ID Dependency : Step.dependencies) {
      (void)Dependency;
      N = boundedAdd(N, sizeof(ID));
    }
  }
  return N;
}
size_t snapshotPayload(const Snapshot &S, size_t NameBytes) {
  size_t N = boundedAdd(sizeof(Snapshot) + sizeof(llvm::X86MachineFunctionInfo), NameBytes);
  for (const auto *Text : {&S.input, &S.world, &S.mir}) N = boundedAdd(N, Text->size());
  // Reserve the full inverse table, including dead-ID holes, that replay
  // reconstructs. A tiny live-block vector must not authorize a huge table.
  if (S.blockIDCapacity > MaxMachinePayload / sizeof(llvm::MachineBasicBlock *))
    return MaxMachinePayload + 1;
  N = boundedAdd(N, size_t(S.blockIDCapacity) * sizeof(llvm::MachineBasicBlock *));
  for (const auto *Numbers : {&S.blockNumbers, &S.analysisNumbers})
    for (unsigned Number : *Numbers) { (void)Number; N = boundedAdd(N, sizeof(unsigned)); }
  N = boundedAdd(N, stepsPayload(S.pipeline));
  N = boundedAdd(N, stepsPayload(S.sequence));
  for (ID Provider : S.emissionProviders) { (void)Provider; N = boundedAdd(N, sizeof(ID)); }
  return N;
}
size_t recordingReservation(const Snapshot &S, size_t NameBytes) {
  // Until the function finishes, reserve its full remaining trace and the
  // largest possible dependency closure, before accumulating either of them.
  size_t N = boundedAdd(snapshotPayload(S, NameBytes), stepsPayload(S.pipeline));
  for (const auto &Step : S.pipeline)
    for (ID Dependency : Step.dependencies) { (void)Dependency; N = boundedAdd(N, sizeof(ID)); }
  return N;
}

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
  // MIR renumbers stack objects and drops dead slots. No frame index may enter
  // the opaque target-info copy until a separate relocation proof exists.
  if (Frame.getObjectIndexBegin() != 0 || Frame.getObjectIndexEnd() != 0 ||
      Frame.getStackSize() != 0 || Frame.hasCalls() ||
      MF.getJumpTableInfo() || !MF.getConstantPool()->getConstants().empty() ||
      !MF.getFrameInstructions().empty())
    return false;
  auto *X = MF.getInfo<llvm::X86MachineFunctionInfo>();
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
      if (Step.machine && !Step.preservesAll) return false;
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
    size_t reservation = 0;
    size_t replayPosition = 0;
    size_t position() const { return replay ? replayPosition : sequence.size(); }
  };
  llvm::Module &module;
  llvm::TargetMachine &machine;
  std::string config;
  std::shared_ptr<const MachineProducts> prior;
  ZilNativeProductStats &stats;
  bool &failed;
  std::map<const llvm::Function *, Current> current;
  size_t reservedBytes = 0;
  size_t reservedSlots = 0;

  bool reserve(Current &C, size_t Bytes) {
    const size_t Other = reservedBytes - C.reservation;
    if (Bytes > MaxMachinePayload || Other > MaxMachinePayload - Bytes ||
        (!C.reservation && reservedSlots == MaxMachineSlots))
      return false;
    if (!C.reservation) ++reservedSlots;
    reservedBytes = Other + Bytes;
    C.reservation = Bytes;
    return true;
  }
  void discardRecording(Current &C) {
    C.recording.reset();
    std::vector<PassStep>().swap(C.sequence);
    reservedBytes -= C.reservation;
    if (C.reservation) --reservedSlots;
    C.reservation = 0;
  }
  void fail() {
    if (!failed) ++stats.failed_replays;
    failed = true;
  }
};

std::unique_ptr<MachineRequest, void (*)(MachineRequest *)> createMachine(
    llvm::Module &M, llvm::TargetMachine &TM, const std::string &Config,
    std::shared_ptr<const MachineProducts> Prior, ZilNativeProductStats &Stats,
    bool &Failed) {
  if (!targetInfoAPI().available() || Config.size() > MaxMachinePayload)
    return {nullptr, [](MachineRequest *R) { delete R; }};
  return {new MachineRequest{M, TM, Config, std::move(Prior), Stats, Failed, {}, Config.size(), 0},
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
        R.reservedSlots < MaxMachineSlots && R.reservedBytes < MaxMachinePayload &&
        inputDomain(F)) {
      C.recording = std::make_shared<Snapshot>();
      if (!bindPipeline(P, *C.recording)) C.recording.reset();
    }
    if (C.recording) {
      C.recording->input = functionText(F);
      C.recording->world = worldText(R.module, &F);
      R.stats.mir_key_bytes += C.recording->input.size() + C.recording->world.size();
      if (!R.reserve(C, recordingReservation(*C.recording, F.getName().size())))
        R.discardRecording(C);
    }
    if (C.recording) {
      if (R.prior && R.prior->config == R.config) {
        auto Old = R.prior->functions.find(F.getName().str());

        if (Old != R.prior->functions.end() &&
            Old->second->input == C.recording->input &&
            Old->second->world == C.recording->world &&
            Old->second->pipeline == C.recording->pipeline &&
            Old->second->firstMachine == C.recording->firstMachine &&
            !Old->second->sequence.empty() &&
            Old->second->sequence.front() == describe(P, true) &&
            !MMI.getMachineFunction(F) &&
            R.reserve(C, snapshotPayload(*Old->second, F.getName().size()))) {
          C.replay = Old->second;
          // Current keys have proved the complete binding. Retain the shared
          // sealed snapshot, not a second copy of its identical input/world.
          C.recording.reset();
          auto Parser = llvm::createMIRParser(
              llvm::MemoryBuffer::getMemBufferCopy(C.replay->mir, "zil-current-machine"),
              R.module.getContext());
          // Required to initialize the YAML cursor. This header is empty;
          // parseMachineFunctions explicitly receives the CURRENT Module/MMI.
          auto EmptyHeader = Parser->parseIRModule();
          if (!EmptyHeader) {
            diagnose(F, "parse-header");
            R.fail();
          } else if (Parser->parseMachineFunctions(R.module, MMI)) {
            diagnose(F, "parse-machine");
            R.fail();
          } else {
            auto *MF = MMI.getMachineFunction(F);
            if (!MF) {
              diagnose(F, "missing-current-machine-function");
              R.fail();
            } else if (!targetInfoAPI().restore(MF, C.replay->targetInfo.get())) {
              diagnose(F, "restore-target-info");
              R.fail();
            } else if (!targetInfoAPI().restoreBlockNumbers(
                           MF, C.replay->blockIDCapacity, C.replay->nextAnalysisNumber,
                           C.replay->analysisEpoch, C.replay->blockNumbers.data(),
                           C.replay->analysisNumbers.data(), C.replay->blockNumbers.size())) {
              diagnose(F, "restore-machine-number-state");
              R.fail();
            } else {
              MF->getProperties() = C.replay->properties;
              if (!snapshotDomain(*MF)) {
                diagnose(F, "restored-snapshot-domain");
                R.fail();
              } else if (!MF->verify(nullptr, "ZIL current MIR replay", nullptr, false)) {
                diagnose(F, "current-machine-verifier");
                R.fail();
              } else {
                auto CurrentText = machineText(MMI, *MF);
                if (CurrentText != C.replay->mir) {
                  diagnoseDifference(F, C.replay->mir, CurrentText);
                  R.fail();
                }
              }
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
      (C.position() >= C.replay->sequence.size() ||
       !(C.replay->sequence[C.position()] == describe(P, true)))) {
    diagnose(F, "before-machine-pass-sequence", P.getPassName());
    R.fail();
  }
  if (R.failed) {
    C.skipped = &P;
    return 1;
  }
  if (Printer) {
    ++R.stats.asm_printer_runs;
    C.emitted = true;
    if (!C.replay && C.recording) {
      auto *MF = MMI.getMachineFunction(F);
      if (!MF || !snapshotDomain(*MF) ||
          C.recording->input != functionText(F) ||
          C.recording->world != worldText(R.module, &F)) {
        R.discardRecording(C);
      } else {
        void *Info = targetInfoAPI().clone(MF);
        if (!Info) {
          R.discardRecording(C);
        } else {
          C.recording->targetInfo = {Info, targetInfoAPI().dispose};
          C.recording->properties = MF->getProperties();
          C.recording->blockIDCapacity = MF->getNumBlockIDs();
          C.recording->nextAnalysisNumber = MF->getMaxAnalysisBlockNumber();
          C.recording->analysisEpoch = MF->getAnalysisBlockNumberEpoch();
          for (const auto &BB : *MF) {
            C.recording->blockNumbers.push_back(BB.getNumber());
            C.recording->analysisNumbers.push_back(BB.getAnalysisNumber());
          }
          C.recording->mir = machineText(MMI, *MF);
          R.stats.mir_snapshot_bytes += C.recording->mir.size();
          if (!R.reserve(C, recordingReservation(*C.recording, F.getName().size())))
            R.discardRecording(C);
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
      (C.position() >= C.replay->sequence.size() ||
       !(C.replay->sequence[C.position()] == Step))) {
    diagnose(F, "after-legacy-pass-sequence", P.getPassName());
    R.fail();
  }
  if (C.replay) {
    ++C.replayPosition;
  } else {
    // An unplanned nested/extra pass cannot issue a cold receipt. Stop its
    // optional trace immediately instead of growing past the reserved bound.
    const size_t Position = C.recording->firstMachine + C.sequence.size();
    auto ContainerStep = Step;
    ContainerStep.machine = false;
    if (Position >= C.recording->pipeline.size() ||
        !(ContainerStep == C.recording->pipeline[Position])) {
      R.discardRecording(C);
    } else {
      C.sequence.push_back(std::move(Step));
    }
  }
  return Skipped;
}

std::shared_ptr<const MachineProducts> finishMachine(MachineRequest &R) {
  auto Next = std::make_shared<MachineProducts>();
  Next->config = R.config;
  R.stats.retained_bytes += R.config.size();
  for (auto &[F, C] : R.current) {
    std::shared_ptr<const Snapshot> S;
    if (C.replay) {
      // The whole actual container was bound before any restoration. Finish
      // is publication only; an incomplete observed tail discards the product.
      if (C.emitted && C.replayPosition == C.replay->sequence.size()) S = C.replay;
    } else if (C.recording && C.emitted && C.recording->targetInfo) {
      C.recording->sequence = std::move(C.sequence);
      bool Complete = C.recording->firstMachine + C.recording->sequence.size() ==
                      C.recording->pipeline.size();
      for (size_t I = 0; Complete && I < C.recording->sequence.size(); ++I) {
        auto Step = C.recording->sequence[I];
        Step.machine = false;
        Complete = Step == C.recording->pipeline[C.recording->firstMachine + I];
      }
      if (Complete && providers(*C.recording)) S = std::move(C.recording);
    }
    if (S) {
      R.stats.retained_bytes += snapshotPayload(*S, F->getName().size());
      Next->functions.emplace(F->getName().str(), std::move(S));
    }
  }
  if (R.failed) return nullptr;
  return Next;
}
} // namespace zil

extern "C" int ZilNativeProductsMachineDiagnostics(int Enabled) {
  const bool Old = zil::MachineDiagnostics;
  zil::MachineDiagnostics = Enabled != 0;
  return Old;
}
