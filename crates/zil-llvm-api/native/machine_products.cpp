// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "machine_products.h"
#include "machine_frame_products.h"
#include "X86MachineFunctionInfo.h"
#include "llvm/CodeGen/MIRParser/MIRParser.h"
#include "llvm/CodeGen/MIRPrinter.h"
#include "llvm/CodeGen/MachineConstantPool.h"
#include "llvm/CodeGen/MachineDominators.h"
#include "llvm/CodeGen/MachineFrameInfo.h"
#include "llvm/CodeGen/MachineFunction.h"
#include "llvm/CodeGen/MachineInstr.h"
#include "llvm/CodeGen/MachineModuleInfo.h"
#include "llvm/CodeGen/MachineRegisterInfo.h"
#include "llvm/CodeGen/TargetInstrInfo.h"
#include "llvm/CodeGen/TargetRegisterInfo.h"
#include "llvm/CodeGen/TargetOpcodes.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/LegacyPassManagers.h"
#include "llvm/IR/Module.h"
#include "llvm/Pass.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Target/TargetMachine.h"
#include <algorithm>
#include <chrono>
#include <cstring>
#include <dlfcn.h>
#include <map>
#include <limits>
#include <mutex>
#include <set>
#include <tuple>
#include <type_traits>
#include <utility>
#include <vector>

namespace zil {
namespace {
using ID = const void *;
using Clock = std::chrono::steady_clock;
uint64_t elapsedNS(Clock::time_point Start) {
  return std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - Start).count();
}
class ScopedNS {
  uint64_t &Total;
  Clock::time_point Start = Clock::now();
  bool Active = true;
public:
  explicit ScopedNS(uint64_t &Out) : Total(Out) {}
  ScopedNS(const ScopedNS &) = delete;
  ScopedNS &operator=(const ScopedNS &) = delete;
  void pause() { if (Active) { Total += elapsedNS(Start); Active = false; } }
  void resume() { Start = Clock::now(); Active = true; }
  ~ScopedNS() { pause(); }
};
thread_local bool MachineDiagnostics = false;
thread_local int MachineAlignmentTestMax = -1;
thread_local bool MachineNumberingTests = false;
thread_local bool MachineNumberingTestsRan = false;
thread_local uint32_t MachineNumberingTestBits = 0;
thread_local uint32_t MachineBundleTestBits = 0;
thread_local uint32_t MachineTypedBodyTestBits = 0;
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
                        llvm::StringRef Current,
                        llvm::StringRef Stage = "mir-text-difference") {
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
  diagnose(F, Stage, Detail);
}

struct TargetInfoAPI {
  void *(*clone)(void *);
  int (*restore)(void *, const void *);
  void (*dispose)(void *);
  const void *(*printerID)();
  int (*hasBlockOwners)(const void *);
  int (*readBlockProbabilities)(const void *, uint32_t *, uint64_t);
  int (*restoreBlockNumbers)(void *, unsigned, unsigned, unsigned,
                             const unsigned *, const unsigned *, uint64_t);
  bool available() const {
    return clone && restore && dispose && printerID && hasBlockOwners && restoreBlockNumbers;
  }
};
const TargetInfoAPI &targetInfoAPI() {
  static const TargetInfoAPI API{
      reinterpret_cast<void *(*)(void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMCloneX86ProductInfo")),
      reinterpret_cast<int (*)(void *, const void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMRestoreX86ProductInfo")),
      reinterpret_cast<void (*)(void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMDisposeX86ProductInfo")),
      reinterpret_cast<const void *(*)()>(dlsym(RTLD_DEFAULT, "ZilLLVMAsmPrinterPassID")),
      reinterpret_cast<int (*)(const void *)>(dlsym(RTLD_DEFAULT, "ZilLLVMHasMachineProductBlockOwners")),
      reinterpret_cast<int (*)(const void *, uint32_t *, uint64_t)>(
          dlsym(RTLD_DEFAULT, "ZilLLVMReadMachineBlockProbabilities")),
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
struct BlockEmissionState {
  unsigned maxAlignmentBytes;
  bool forceLabel;
  std::vector<unsigned> predecessors;
};
// Values only. In particular, neither MachineOperand's parent/use-list links
// nor its ConstantInt/MBB pointers may escape the current compilation.
struct BodyOperand {
  enum Kind : uint8_t { Reg, Imm, CImm, Block, FrameIndex, CFIIndex };
  // Reg: low32 register, high32 subregister; extra holds all register flags.
  // Imm: exact signed immediate bits. Block: current-layout ordinal.
  // CImm: index into BodyState's owned, encounter-ordered APInt pool.
  // Every non-Reg kind stores its complete target flags in extra.
  uint64_t payload = 0;
  uint32_t extra = 0;
  Kind kind = Imm;

  unsigned reg() const { return uint32_t(payload); }
  unsigned subReg() const { return uint32_t(payload >> 32); }
  void setRegister(unsigned RegNo, unsigned SubReg) {
    payload = uint64_t(RegNo) | (uint64_t(SubReg) << 32);
  }
  void setImmediate(int64_t Value) {
    // Do not rely on implementation-defined unsigned-to-signed conversion.
    std::memcpy(&payload, &Value, sizeof(Value));
  }
  int64_t immediate() const {
    int64_t Value;
    std::memcpy(&Value, &payload, sizeof(Value));
    return Value;
  }
  bool operator==(const BodyOperand &O) const {
    return std::tie(kind, payload, extra) == std::tie(O.kind, O.payload, O.extra);
  }
};
static_assert(sizeof(BodyOperand) == 16, "machine operand product must remain compact");
static_assert(sizeof(unsigned) == sizeof(uint32_t), "LLVM register fields must fit their full encoding");
static_assert(std::is_trivially_copyable_v<BodyOperand>, "operand records must contain values only");
struct BodyInstruction {
  unsigned opcode = 0, flags = 0, asmFlags = 0, cfiType = 0;
  std::vector<BodyOperand> operands;
  std::vector<frame::Memory> memory;
  std::vector<std::pair<unsigned, unsigned>> ties;
  bool operator==(const BodyInstruction &O) const {
    return std::tie(opcode, flags, asmFlags, cfiType, operands, memory, ties) ==
           std::tie(O.opcode, O.flags, O.asmFlags, O.cfiType, O.operands, O.memory, O.ties);
  }
};
struct BodyBlock {
  // -1 denotes a synthetic block, otherwise a CURRENT IR block's layout index.
  int64_t irBlock = -1;
  uint64_t alignment = 1;
  bool probabilities = false;
  std::vector<std::pair<unsigned, uint32_t>> successors;
  std::vector<llvm::MachineBasicBlock::RegisterMaskPair> liveins;
  std::vector<BodyInstruction> instructions;
  bool operator==(const BodyBlock &O) const {
    return std::tie(irBlock, alignment, probabilities, successors, liveins, instructions) ==
           std::tie(O.irBlock, O.alignment, O.probabilities, O.successors, O.liveins, O.instructions);
  }
};
struct BodyState {
  std::vector<BodyBlock> blocks;
  std::vector<llvm::APInt> integers;
  // Physical register -> original ordered (global MI ordinal, operand index).
  std::map<unsigned, std::vector<std::pair<unsigned, unsigned>>> registerUses;
  llvm::BitVector usedRegisters, reservedRegisters;
  bool reservedFrozen = false, subRegLiveness = false;
  bool operator==(const BodyState &O) const {
    if (integers.size() != O.integers.size()) return false;
    for (size_t I = 0; I < integers.size(); ++I)
      if (integers[I].getBitWidth() != O.integers[I].getBitWidth() ||
          integers[I] != O.integers[I]) return false;
    return blocks == O.blocks && registerUses == O.registerUses &&
           usedRegisters == O.usedRegisters && reservedRegisters == O.reservedRegisters &&
           reservedFrozen == O.reservedFrozen && subRegLiveness == O.subRegLiveness;
  }
};
struct Snapshot {
  std::string input, world, mir, mirHeader;
  BodyState body;
  frame::State frame;
  std::shared_ptr<const void> targetInfo;
  llvm::MachineFunctionProperties properties;
  // MIR's parser assigns dense IDs in layout order. Preserve both original
  // numbering domains and their complete table/next-ID/epoch state separately.
  // Only scalar IDs escape; the inverse table is rebuilt with CURRENT MBBs.
  unsigned blockIDCapacity = 0, nextAnalysisNumber = 0, analysisEpoch = 0;
  std::vector<unsigned> blockNumbers, analysisNumbers;
  std::vector<BlockEmissionState> blockEmission;
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
size_t integerPayload(const llvm::APInt &I) {
  // APInt owns its single-word value inline; only wider values allocate words.
  return sizeof(llvm::APInt) + (I.getBitWidth() > 64 ? I.getNumWords() * sizeof(uint64_t) : 0);
}
size_t snapshotPayload(const Snapshot &S, size_t NameBytes) {
  size_t N = boundedAdd(sizeof(Snapshot) + sizeof(llvm::X86MachineFunctionInfo), NameBytes);
  for (const auto *Text : {&S.input, &S.world, &S.mir, &S.mirHeader}) N = boundedAdd(N, Text->size());
  for (const auto &BB : S.body.blocks) {
    N = boundedAdd(N, sizeof(BodyBlock));
    N = boundedAdd(N, BB.successors.size() * sizeof(BB.successors.front()));
    N = boundedAdd(N, BB.liveins.size() * sizeof(BB.liveins.front()));
    for (const auto &MI : BB.instructions) {
      N = boundedAdd(N, sizeof(BodyInstruction));
      N = boundedAdd(N, MI.ties.size() * sizeof(MI.ties.front()));
      N = boundedAdd(N, MI.operands.size() * sizeof(BodyOperand));
      N = boundedAdd(N, MI.memory.size() * sizeof(frame::Memory));
    }
  }
  N = boundedAdd(N, S.frame.chargedBytes);
  for (const auto &I : S.body.integers) N = boundedAdd(N, integerPayload(I));
  for (const auto &Uses : S.body.registerUses) {
    N = boundedAdd(N, sizeof(Uses));
    N = boundedAdd(N, Uses.second.size() * sizeof(Uses.second.front()));
  }
  N = boundedAdd(N, (S.body.usedRegisters.size() + 7) / 8);
  N = boundedAdd(N, (S.body.reservedRegisters.size() + 7) / 8);
  // Reserve the full inverse table, including dead-ID holes, that replay
  // reconstructs, and the analysis-number span used by fresh graph analyses.
  // A tiny live-block vector must not authorize either huge sparse domain.
  for (unsigned Span : {S.blockIDCapacity, S.nextAnalysisNumber}) {
    if (Span > MaxMachinePayload / sizeof(llvm::MachineBasicBlock *))
      return MaxMachinePayload + 1;
    N = boundedAdd(N, size_t(Span) * sizeof(llvm::MachineBasicBlock *));
  }
  for (const auto *Numbers : {&S.blockNumbers, &S.analysisNumbers})
    for (unsigned Number : *Numbers) { (void)Number; N = boundedAdd(N, sizeof(unsigned)); }
  for (const auto &Block : S.blockEmission) {
    N = boundedAdd(N, sizeof(BlockEmissionState));
    for (unsigned Pred : Block.predecessors) { (void)Pred; N = boundedAdd(N, sizeof(unsigned)); }
  }
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

// These fixed X86 register push/pop descriptors have implicit RSP memory and
// the official frame lowering builds them without MMOs (7a, lines3215/3349).
// getName comes from the current TII's unique generated opcode table; both the
// opcode/descriptor identity and complete pass/target/config are also bound.
bool implicitRegisterStackAccess(const llvm::MachineFunction &MF, const llvm::MachineInstr &MI) {
  const auto *TII = MF.getSubtarget().getInstrInfo();
  if (MI.getOpcode() >= TII->getNumOpcodes() || &MI.getDesc() != &TII->get(MI.getOpcode())) return false;
  auto Name = TII->getName(MI.getOpcode());
  return Name == "PUSH64r" || Name == "POP64r";
}
bool snapshotDomain(llvm::MachineFunction &MF) {
  const auto &Frame = MF.getFrameInfo();
  // The current product does not retain EH/section/symbol owners or BB-ID
  // profile state. Keep this exclusion explicit at the machine consumer.
  if (targetInfoAPI().hasBlockOwners(&MF) || MF.getSection() || MF.hasInlineAsm() || MF.hasWinCFI() ||
      MF.callsEHReturn() || MF.callsUnwindInit() || MF.hasEHContTarget() ||
      MF.hasEHScopes() || MF.hasEHFunclets() || MF.hasBBSections() ||
      MF.getTarget().Options.BBAddrMap ||
      MF.getTarget().Options.BBSections != llvm::BasicBlockSection::None ||
      !MF.getPrefetchTargets().empty() || !MF.getLandingPads().empty() ||
      !MF.getTypeInfos().empty() || !MF.getFilterIds().empty() ||
      !MF.getLongjmpTargets().empty() || !MF.getEHContTargets().empty() ||
      !MF.getCodeViewAnnotations().empty() || !MF.getCallSitesInfo().empty() ||
      !MF.getCalledGlobals().empty() || MF.getObserver() || MF.hasFakeUses() ||
      !MF.VariableDbgInfos.empty() || !MF.getEntryValueVariableDbgInfo().empty() ||
      !MF.DebugValueSubstitutions.empty() ||
      !MF.DebugPHIPositions.empty() || MF.DebugInstrNumberingCount)
    return false;
  // A complete value copy preserves the full original FI table, including
  // fixed negative IDs and dead holes. Every owner is null or separately
  // rebound; a bodyless YAML header alone never authorizes these fields.
  if (!frame::domain(MF, MaxMachinePayload) || MF.getJumpTableInfo() ||
      !MF.getConstantPool()->getConstants().empty()) return false;
  auto *X = MF.getInfo<llvm::X86MachineFunctionInfo>();
  if (!X || X->getRAIndex() || X->getFAIndex() || X->getVarArgsFrameIndex() ||
      X->getRegSaveFrameIndex() || X->getSEHFramePtrSaveIndex() ||
      X->getSwiftAsyncContextFrameIdx() || X->getSRetReturnReg() ||
      X->getGlobalBaseReg() || !X->getForwardedMustTailRegParms().empty() ||
      X->getStackPtrSaveMI() || X->hasPreallocatedCall() ||
      X->hasSwiftAsyncContext() || X->getHasSEHFramePtrSave())
    return false;
  for (const auto &BB : MF) {
    if (BB.hasAddressTaken() || BB.isEHPad() || BB.isEHScopeEntry() ||
        BB.isEHContTarget() || BB.isEHFuncletEntry() || BB.isCleanupFuncletEntry() ||
        BB.isBeginSection() || BB.isEndSection() || BB.getBBID() ||
        BB.getSectionID() != llvm::MBBSectionID(0) || BB.getIrrLoopHeaderWeight() ||
        BB.getCallFrameSize() != 0 || BB.isInlineAsmBrIndirectTarget())
      return false;
    // The ordinary MBB iterator skips bundle members. Admission must inspect
    // every instruction that MIR serializes and the actual printer consumes.
    for (const auto &MI : BB.instrs()) {
      // X86 TRAP/UD2 deliberately carries mayLoad for scheduling, although it
      // has no memory operand. Other trapping forms (UD1 memory etc.) have
      // explicit operands and remain outside this no-memory snapshot domain.
      const bool PlainTrap = MI.getDesc().isTrap() && MI.getNumExplicitOperands() == 0;
      if (MI.isCall() || MI.isInlineAsm() || MI.isDebugInstr() ||
          ((MI.mayLoad() || MI.mayStore()) && !PlainTrap && MI.memoperands_empty() &&
           !implicitRegisterStackAccess(MF, MI)) ||
          MI.getDebugLoc() || MI.getPreInstrSymbol() ||
          MI.getPostInstrSymbol() || MI.getPCSections() || MI.getHeapAllocMarker() ||
          MI.getMMRAMetadata() || MI.getDeactivationSymbol() || MI.peekDebugInstrNum())
        return false;
      for (const auto *M : MI.memoperands()) if (!frame::captureMemory(MF, *M)) return false;
      // FI/CFI references are scalar indices into complete owned value tables.
      // No symbol, IR value, metadata, regmask or pooled owner may escape.
      for (const auto &MO : MI.operands()) {
        if (MO.isReg()) {
          if (MO.getReg().isVirtual()) return false;
        } else if (MO.isFI()) {
          if (!frame::validIndex(Frame, MO.getIndex())) return false;
        } else if (MO.isCFIIndex()) {
          if (MO.getCFIIndex() >= MF.getFrameInstructions().size()) return false;
        } else if (!MO.isImm() && !MO.isCImm() && !MO.isMBB()) return false;
      }
    }
  }
  return true;
}

enum OperandFlags : unsigned {
  Def = 1, Implicit = 2, DeadOrKill = 4, Undef = 8, Early = 16,
  Debug = 32, Internal = 64, Renamable = 128
};

bool captureBody(const llvm::MachineFunction &MF, BodyState &Out) {
  if (!targetInfoAPI().readBlockProbabilities) return false;
  const auto &MRI = MF.getRegInfo();
  // VirtRegRewriter clears the final vreg domain. Do not infer that fact from
  // NoVRegs (which only describes instructions), or ignore unused vreg state.
  // Inactive allocation hints/names after clearVirtRegs are not read by the
  // fixed, preserves-all emission-provider closure; no allocator runs again.
  if (MRI.getNumVirtRegs() || !MRI.getPendingVirtRegMapEntries().empty()) return false;
  for (auto LI : MRI.liveins()) if (LI.second) return false;
  Out.reservedFrozen = MRI.reservedRegsFrozen();
  Out.subRegLiveness = MRI.subRegLivenessEnabled();
  Out.usedRegisters = MRI.getUsedPhysRegsMask();
  if (Out.reservedFrozen) Out.reservedRegisters = MRI.getReservedRegs();
  std::map<const llvm::BasicBlock *, unsigned> IRBlocks;
  for (const auto &BB : MF.getFunction()) IRBlocks.emplace(&BB, IRBlocks.size());
  std::map<const llvm::MachineBasicBlock *, unsigned> Blocks;
  for (const auto &BB : MF) Blocks.emplace(&BB, Blocks.size());
  std::map<const llvm::MachineOperand *, std::pair<unsigned, unsigned>> Operands;
  const auto *TII = MF.getSubtarget().getInstrInfo();
  unsigned InstructionIndex = 0;
  size_t Bytes = sizeof(BodyState);
  auto Charge = [&](size_t N) { Bytes = boundedAdd(Bytes, N); return Bytes <= MaxMachinePayload; };
  for (const auto &BB : MF) {
    if (!Charge(sizeof(BodyBlock))) return false;
    BodyBlock B;
    if (const auto *IR = BB.getBasicBlock()) {
      auto It = IRBlocks.find(IR);
      if (It == IRBlocks.end()) return false;
      B.irBlock = It->second;
    }
    B.alignment = BB.getAlignment().value();
    B.probabilities = BB.hasSuccessorProbabilities();
    // Preserve unknown sentinels, not getSuccProbability's normalized answer.
    // Also distinguish an absent probability vector from a populated one.
    std::vector<uint32_t> RawProbabilities(B.probabilities ? BB.succ_size() : 0);
    if (!targetInfoAPI().readBlockProbabilities(&BB, RawProbabilities.data(), RawProbabilities.size()))
      return false;
    size_t SuccessorIndex = 0;
    for (auto SI = BB.succ_begin(); SI != BB.succ_end(); ++SI) {
      auto It = Blocks.find(*SI);
      if (It == Blocks.end() || !Charge(sizeof(B.successors.front()))) return false;
      B.successors.emplace_back(It->second, B.probabilities ? RawProbabilities[SuccessorIndex] : 0);
      ++SuccessorIndex;
    }
    for (auto LI : BB.liveins()) {
      if (!Charge(sizeof(LI))) return false;
      B.liveins.push_back(LI);
    }
    bool PreviousBundledSucc = false;
    for (const auto &MI : BB.instrs()) {
      if (!Charge(sizeof(BodyInstruction)) || MI.getOpcode() >= TII->getNumOpcodes() ||
          &MI.getDesc() != &TII->get(MI.getOpcode()) ||
          MI.isBundledWithPred() != PreviousBundledSucc)
        return false;
      PreviousBundledSucc = MI.isBundledWithSucc();
      BodyInstruction I;
      I.opcode = MI.getOpcode(); I.flags = MI.getFlags();
      I.asmFlags = MI.getAsmPrinterFlags(); I.cfiType = MI.getCFIType();
      for (const auto *M : MI.memoperands()) {
        if (!Charge(sizeof(frame::Memory)) || !frame::captureMemory(MF, *M, &I.memory)) return false;
      }
      bool Implicits = false;
      for (const auto &MO : MI.operands()) {
        if (!Charge(sizeof(BodyOperand))) return false;
        BodyOperand O;
        if (MO.isReg()) {
          if (MO.getReg().isVirtual()) return false;
          // Read the raw renamable bit without retaining this temporary copy's
          // original parent/links. The contextual accessor masks that bit for
          // descriptors with additional allocation constraints.
          // NoReg has no legal renamable query. Its inactive bit is normalized
          // to false: the bound tail contains no register-changing transforms
          // or allocator, only preserves-all providers and the actual printer.
          auto Detached = MO; Detached.clearParent();
          O.kind = BodyOperand::Reg; O.setRegister(MO.getReg().id(), MO.getSubReg());
          O.extra = (MO.isDef() ? Def : 0) | (MO.isImplicit() ? Implicit : 0) |
                    ((MO.isDef() ? MO.isDead() : MO.isKill()) ? DeadOrKill : 0) |
                    (MO.isUndef() ? Undef : 0) | (MO.isEarlyClobber() ? Early : 0) |
                    (MO.isDebug() ? Debug : 0) | (MO.isInternalRead() ? Internal : 0) |
                    (MO.getReg().isPhysical() && Detached.isRenamable() ? Renamable : 0);
          if (MO.isUse() && MO.isTied()) {
            if (!Charge(sizeof(I.ties.front()))) return false;
            I.ties.emplace_back(MI.findTiedOperandIdx(I.operands.size()), I.operands.size());
          }
          Operands.emplace(&MO, std::make_pair(InstructionIndex, I.operands.size()));
          Out.registerUses.try_emplace(O.reg());
        } else if (MO.isImm()) {
          O.kind = BodyOperand::Imm; O.setImmediate(MO.getImm());
        } else if (MO.isCImm()) {
          O.kind = BodyOperand::CImm;
          if (!Charge(integerPayload(MO.getCImm()->getValue()))) return false;
          O.payload = Out.integers.size();
          Out.integers.push_back(MO.getCImm()->getValue());
        } else if (MO.isMBB()) {
          O.kind = BodyOperand::Block;
          auto It = Blocks.find(MO.getMBB());
          if (It == Blocks.end()) return false;
          O.payload = It->second;
        } else if (MO.isFI()) {
          if (!frame::validIndex(MF.getFrameInfo(), MO.getIndex())) return false;
          O.kind = BodyOperand::FrameIndex; O.setImmediate(MO.getIndex());
        } else if (MO.isCFIIndex()) {
          if (MO.getCFIIndex() >= MF.getFrameInstructions().size()) return false;
          O.kind = BodyOperand::CFIIndex; O.payload = MO.getCFIIndex();
        } else return false;
        if (!MO.isReg()) O.extra = MO.getTargetFlags();
        const bool IsImplicit = MO.isReg() && MO.isImplicit();
        if (Implicits && !IsImplicit) return false;
        Implicits |= IsImplicit;
        I.operands.push_back(std::move(O));
      }
      B.instructions.push_back(std::move(I));
      ++InstructionIndex;
    }
    if (PreviousBundledSucc) return false;
    Out.blocks.push_back(std::move(B));
  }
  size_t UseCount = 0;
  for (auto &Entry : Out.registerUses) {
    for (const auto &MO : MRI.reg_operands(llvm::Register(Entry.first))) {
      auto It = Operands.find(&MO);
      if (It == Operands.end() || !Charge(sizeof(Entry.second.front()))) return false;
      Entry.second.push_back(It->second);
      ++UseCount;
    }
  }
  return UseCount == Operands.size();
}

bool restoreBody(llvm::MachineFunction &MF, const BodyState &S, const frame::State *Frame = nullptr) {
  // Pool indices are a lossless local encoding, not independent authority.
  // Prove the canonical one-to-one encounter order before allocating current
  // instructions; reject dangling, aliased, reordered, or unused pool entries.
  size_t IntegerIndex = 0;
  for (const auto &B : S.blocks)
    for (const auto &I : B.instructions)
      for (const auto &O : I.operands)
        if (O.kind == BodyOperand::CImm) {
          if (O.payload != IntegerIndex || IntegerIndex >= S.integers.size()) return false;
          ++IntegerIndex;
        }
  if (IntegerIndex != S.integers.size()) return false;
  if (!MF.empty() || MF.getRegInfo().getNumVirtRegs() ||
      !MF.getRegInfo().getPendingVirtRegMapEntries().empty()) return false;
  auto &MRI = MF.getRegInfo();
  if (MRI.subRegLivenessEnabled() != S.subRegLiveness ||
      MRI.reservedRegsFrozen() != S.reservedFrozen ||
      (S.reservedFrozen && MRI.getReservedRegs() != S.reservedRegisters)) return false;
  std::vector<const llvm::BasicBlock *> IR;
  for (const auto &BB : MF.getFunction()) IR.push_back(&BB);
  std::vector<llvm::MachineBasicBlock *> Blocks;
  for (const auto &B : S.blocks) {
    if (B.irBlock < -1 || (B.irBlock >= 0 && uint64_t(B.irBlock) >= IR.size())) return false;
    auto *BB = MF.CreateMachineBasicBlock(B.irBlock < 0 ? nullptr : IR[B.irBlock]);
    MF.push_back(BB); BB->setAlignment(llvm::Align(B.alignment));
    for (auto LI : B.liveins) BB->addLiveIn(LI);
    Blocks.push_back(BB);
  }
  if (Frame && !frame::restore(MF, *Frame, MaxMachinePayload)) return false;
  const auto *TII = MF.getSubtarget().getInstrInfo();
  std::vector<llvm::MachineInstr *> Instructions;
  for (size_t BI = 0; BI < S.blocks.size(); ++BI) {
    const auto &B = S.blocks[BI];
    auto *BB = Blocks[BI];
    for (auto Edge : B.successors) {
      if (Edge.first >= Blocks.size()) return false;
      if (B.probabilities) BB->addSuccessor(Blocks[Edge.first], llvm::BranchProbability::getRaw(Edge.second));
      else BB->addSuccessorWithoutProb(Blocks[Edge.first]);
    }
    for (const auto &I : B.instructions) {
      if (I.opcode >= TII->getNumOpcodes()) return false;
      auto *MI = MF.CreateMachineInstr(TII->get(I.opcode), llvm::DebugLoc(), true);
      BB->push_back(MI); Instructions.push_back(MI);
      for (const auto &O : I.operands) {
        llvm::MachineOperand MO = llvm::MachineOperand::CreateImm(0);
        switch (O.kind) {
        case BodyOperand::Reg:
          MO = llvm::MachineOperand::CreateReg(llvm::Register(O.reg()), O.extra & Def,
              O.extra & Implicit, !(O.extra & Def) && (O.extra & DeadOrKill),
              (O.extra & Def) && (O.extra & DeadOrKill), O.extra & Undef,
              O.extra & Early, O.subReg(), O.extra & Debug, O.extra & Internal,
              O.extra & Renamable);
          break;
        case BodyOperand::Imm: MO = llvm::MachineOperand::CreateImm(O.immediate()); break;
        case BodyOperand::CImm:
          if (O.payload >= S.integers.size()) return false;
          MO = llvm::MachineOperand::CreateCImm(llvm::ConstantInt::get(
              MF.getFunction().getContext(), S.integers[O.payload]));
          break;
        case BodyOperand::Block:
          if (O.payload >= Blocks.size()) return false;
          MO = llvm::MachineOperand::CreateMBB(Blocks[O.payload]); break;
        case BodyOperand::FrameIndex:
          if (O.immediate() < std::numeric_limits<int>::min() || O.immediate() > std::numeric_limits<int>::max() ||
              !frame::validIndex(MF.getFrameInfo(), static_cast<int>(O.immediate()))) return false;
          MO = llvm::MachineOperand::CreateFI(static_cast<int>(O.immediate())); break;
        case BodyOperand::CFIIndex:
          if (O.payload >= MF.getFrameInstructions().size()) return false;
          MO = llvm::MachineOperand::CreateCFIIndex(static_cast<unsigned>(O.payload)); break;
        default: return false;
        }
        if (!MO.isReg()) MO.setTargetFlags(O.extra);
        MI->addOperand(MF, MO);
      }
      // Descriptor constraints can create ties during append. Replace those
      // with the exact captured relation, which may also contain implicit ties.
      for (unsigned OI = 0; OI < MI->getNumOperands(); ++OI)
        if (MI->getOperand(OI).isReg()) MI->untieRegOperand(OI);
      for (auto Tie : I.ties) MI->tieOperands(Tie.first, Tie.second);
      MI->setFlags(I.flags);
      MI->setAsmPrinterFlag(I.asmFlags);
      if (I.cfiType) MI->setCFIType(MF, I.cfiType);
      llvm::SmallVector<llvm::MachineMemOperand *, 2> Memory;
      for (const auto &M : I.memory) {
        auto *Current = frame::restoreMemory(MF, M);
        if (!Current) return false;
        Memory.push_back(Current);
      }
      if (!Memory.empty()) MI->setMemRefs(MF, Memory);
    }
    // Bundling is restored only after insertion, so list insertion cannot
    // accidentally treat a partly reconstructed bundle as an existing bundle.
    auto II = BB->instr_begin();
    for (const auto &I : B.instructions) {
      if (I.flags & llvm::MachineInstr::BundledPred) II->setFlag(llvm::MachineInstr::BundledPred);
      if (I.flags & llvm::MachineInstr::BundledSucc) II->setFlag(llvm::MachineInstr::BundledSucc);
      ++II;
    }
  }
  // Rebind original use-def order using current operand coordinates. LLVM
  // prepends defs and appends uses, so insert defs reversed and uses forward.
  for (const auto &Entry : S.registerUses) {
    std::vector<llvm::MachineOperand *> Uses;
    std::set<llvm::MachineOperand *> Remaining;
    for (auto &MO : MRI.reg_operands(llvm::Register(Entry.first))) Remaining.insert(&MO);
    for (auto Pos : Entry.second) {
      if (Pos.first >= Instructions.size() || Pos.second >= Instructions[Pos.first]->getNumOperands()) return false;
      auto *MO = &Instructions[Pos.first]->getOperand(Pos.second);
      if (!MO->isReg() || MO->getReg().id() != Entry.first || Remaining.erase(MO) != 1) return false;
      Uses.push_back(MO);
    }
    if (!Remaining.empty()) return false;
    for (auto *MO : Uses) MRI.removeRegOperandFromUseList(MO);
    for (auto It = Uses.rbegin(); It != Uses.rend(); ++It)
      if ((*It)->isDef()) MRI.addRegOperandToUseList(*It);
    for (auto *MO : Uses) if (MO->isUse()) MRI.addRegOperandToUseList(MO);
  }
  if (MRI.getUsedPhysRegsMask().size() != S.usedRegisters.size()) return false;
  std::vector<uint32_t> Preserved((S.usedRegisters.size() + 31) / 32, ~uint32_t(0));
  for (int Bit = S.usedRegisters.find_first(); Bit >= 0; Bit = S.usedRegisters.find_next(Bit))
    Preserved[Bit / 32] &= ~(uint32_t(1) << (Bit % 32));
  MRI.addPhysRegsUsedFromRegMask(Preserved.data());
  return MRI.getUsedPhysRegsMask() == S.usedRegisters;
}

// Validate all predecessor multisets before changing any current block. The
// replacement list contains exactly the current owners, in the proved order;
// successors, probabilities and CFG membership remain unchanged.
bool restoreBlockEmission(llvm::MachineFunction &MF, const Snapshot &S) {
  if (MF.size() != S.blockEmission.size()) return false;
  std::map<unsigned, llvm::MachineBasicBlock *> Current;
  for (auto &BB : MF) Current.emplace(BB.getNumber(), &BB);
  std::vector<std::vector<llvm::MachineBasicBlock *>> Ordered;
  size_t I = 0;
  for (auto &BB : MF) {
    const auto &State = S.blockEmission[I++];
    std::vector<unsigned> Actual, Expected = State.predecessors;
    for (const auto *Pred : BB.predecessors()) Actual.push_back(Pred->getNumber());
    std::sort(Actual.begin(), Actual.end());
    std::sort(Expected.begin(), Expected.end());
    if (Actual != Expected || (BB.hasLabelMustBeEmitted() && !State.forceLabel))
      return false;
    std::vector<llvm::MachineBasicBlock *> Preds;
    for (unsigned Number : State.predecessors) {
      auto It = Current.find(Number);
      if (It == Current.end()) return false;
      Preds.push_back(It->second);
    }
    Ordered.push_back(std::move(Preds));
  }
  I = 0;
  for (auto &BB : MF) {
    const auto &State = S.blockEmission[I];
    BB.setMaxBytesForAlignment(State.maxAlignmentBytes);
    if (State.forceLabel) BB.setLabelMustBeEmitted();
    std::copy(Ordered[I].begin(), Ordered[I].end(), BB.pred_begin());
    ++I;
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

void testTypedBody(llvm::MachineFunction &Current) {
  auto Make = [&]() {
    auto MF = std::make_unique<llvm::MachineFunction>(Current.getFunction(), Current.getTarget(),
        Current.getSubtarget(), Current.getContext(), 91);
    MF->initTargetMachineFunctionInfo(Current.getSubtarget());
    MF->getRegInfo().freezeReservedRegs();
    return MF;
  };
  auto Source = Make();
  auto *A = Source->CreateMachineBasicBlock(&Current.getFunction().front());
  auto *B = Source->CreateMachineBasicBlock();
  auto *C = Source->CreateMachineBasicBlock();
  Source->push_back(A); Source->push_back(B); Source->push_back(C);
  // A single unknown edge queries as probability one. The product must still
  // preserve the raw unknown sentinel rather than that effective answer.
  A->addSuccessor(B, llvm::BranchProbability::getUnknown());
  B->addSuccessorWithoutProb(C);
  A->addLiveIn(llvm::MCRegister::from(1), llvm::LaneBitmask::getAll());
  A->setAlignment(llvm::Align(16));
  const auto *TII = Source->getSubtarget().getInstrInfo();
  auto *MI = Source->CreateMachineInstr(TII->get(llvm::TargetOpcode::KILL), llvm::DebugLoc(), true);
  A->push_back(MI);
  MI->addOperand(*Source, llvm::MachineOperand::CreateReg(llvm::Register(1), true, false,
      false, true, true, true, 0, false, true, true));
  MI->addOperand(*Source, llvm::MachineOperand::CreateReg(llvm::Register(1), false, false,
      true, false, true, false, 0, false, true, true));
  MI->addOperand(*Source, llvm::MachineOperand::CreateReg(llvm::Register(0), false));
  MI->addOperand(*Source, llvm::MachineOperand::CreateImm(-173));
  const uint64_t Words[] = {0xfedcba9876543210ULL, 0x1234567890abcdefULL};
  MI->addOperand(*Source, llvm::MachineOperand::CreateCImm(llvm::ConstantInt::get(
      Current.getFunction().getContext(), llvm::APInt(128, Words))));
  MI->addOperand(*Source, llvm::MachineOperand::CreateMBB(B, 7));
  MI->addOperand(*Source, llvm::MachineOperand::CreateImm(std::numeric_limits<int64_t>::min()));
  MI->addOperand(*Source, llvm::MachineOperand::CreateImm(std::numeric_limits<int64_t>::max()));
  const uint64_t WideWords[] = {0xfedcba9876543210ULL, 0x1234567890abcdefULL,
                               0x8899aabbccddeeffULL, 0x0123456789abcdefULL, 1};
  for (unsigned Width : {1u, 65u, 129u, 257u})
    MI->addOperand(*Source, llvm::MachineOperand::CreateCImm(llvm::ConstantInt::get(
        Current.getFunction().getContext(), llvm::APInt(Width, WideWords))));
  MI->tieOperands(0, 1);
  MI->setFlag(llvm::MachineInstr::NoUWrap);
  MI->setAsmPrinterFlag(1);
  const auto ReadProbabilities = targetInfoAPI().readBlockProbabilities;
  uint32_t Untouched[] = {123, 456};
  if (!ReadProbabilities || ReadProbabilities(nullptr, nullptr, 0) ||
      ReadProbabilities(A, Untouched, 2) || Untouched[0] != 123 || Untouched[1] != 456 ||
      ReadProbabilities(A, nullptr, 1) || ReadProbabilities(A, nullptr, 0) ||
      !ReadProbabilities(B, nullptr, 0)) return;
  BodyState Saved;
  if (!captureBody(*Source, Saved) ||
      Saved.blocks[0].successors[0].second != llvm::BranchProbability::getUnknown().getNumerator() ||
      Saved.blocks[1].probabilities || Saved.blocks[1].successors.size() != 1) return;
  MachineTypedBodyTestBits |= 1;
  auto Restored = Make();
  BodyState Again;
  if (restoreBody(*Restored, Saved) && captureBody(*Restored, Again) && Saved == Again)
    MachineTypedBodyTestBits |= 2;
  // Destroy every historical MBB/MI before following restored pointers.
  Source.reset();
  if (Restored->size() == 3 && !Restored->front().empty()) {
    auto &NewMI = Restored->front().front();
    auto *NewB = &*std::next(Restored->begin());
    if (NewMI.getParent() == &Restored->front() &&
        NewMI.getOperand(5).getMBB() == NewB &&
        Restored->front().isSuccessor(NewB) && NewB->isSuccessor(&Restored->back()) &&
        NewMI.findTiedOperandIdx(1) == 0 &&
        NewMI.getOperand(4).getCImm()->getValue() == llvm::APInt(128, Words) &&
        NewMI.getOperand(6).getImm() == std::numeric_limits<int64_t>::min() &&
        NewMI.getOperand(7).getImm() == std::numeric_limits<int64_t>::max() &&
        NewMI.getOperand(11).getCImm()->getValue() == llvm::APInt(257, WideWords))
      MachineTypedBodyTestBits |= 4;
  }
  auto Changed = Saved;
  Changed.blocks[0].instructions[0].operands[1].extra ^= Undef;
  bool EncodingChecks = !(Changed == Saved);
  Changed = Saved; Changed.integers[0] = Saved.integers[0].zext(129);
  EncodingChecks &= !(Changed == Saved); // Distinct width, same numeric value.
  Changed = Saved; Changed.integers.back().flipBit(256);
  EncodingChecks &= !(Changed == Saved); // High bits must not be truncated.
  Changed = Saved; Changed.blocks[0].instructions[0].operands[4].payload = Saved.integers.size();
  auto InvalidInteger = Make();
  EncodingChecks &= !restoreBody(*InvalidInteger, Changed) && InvalidInteger->empty();
  Changed = Saved; Changed.integers.push_back(llvm::APInt(1, 1));
  auto UnusedInteger = Make();
  EncodingChecks &= !restoreBody(*UnusedInteger, Changed) && UnusedInteger->empty();
  Changed = Saved; Changed.blocks[0].instructions[0].operands[8].payload = 0;
  auto AliasedInteger = Make();
  EncodingChecks &= !restoreBody(*AliasedInteger, Changed) && AliasedInteger->empty();
  // Exercise the real capacity ledger: a non-CImm operand no longer reserves
  // a dummy APInt or an imaginary heap word. The wide pool is still charged.
  Snapshot Accounted; Accounted.body = Saved;
  const size_t BeforeOperand = snapshotPayload(Accounted, 0);
  Accounted.body.blocks[0].instructions[0].operands.push_back(BodyOperand{});
  EncodingChecks &= snapshotPayload(Accounted, 0) == BeforeOperand + sizeof(BodyOperand);
  const size_t BeforeInteger = snapshotPayload(Accounted, 0);
  Accounted.body.integers.push_back(llvm::APInt(257, WideWords));
  EncodingChecks &= snapshotPayload(Accounted, 0) == BeforeInteger + sizeof(llvm::APInt) + 5 * sizeof(uint64_t);
  if (EncodingChecks) MachineTypedBodyTestBits |= 8;
  Changed = Saved; Changed.blocks[0].irBlock = Current.getFunction().size();
  auto InvalidIR = Make();
  if (!restoreBody(*InvalidIR, Changed) && InvalidIR->empty()) MachineTypedBodyTestBits |= 16;
  Changed = Saved; Changed.blocks[0].successors[0].first = Saved.blocks.size();
  auto InvalidEdge = Make();
  if (!restoreBody(*InvalidEdge, Changed)) MachineTypedBodyTestBits |= 32;
  Changed = Saved; Changed.reservedRegisters.flip(0);
  auto InvalidMask = Make();
  if (!restoreBody(*InvalidMask, Changed) && InvalidMask->empty()) MachineTypedBodyTestBits |= 64;
  auto ExtraVReg = Make();
  ExtraVReg->getRegInfo().createGenericVirtualRegister(llvm::LLT::scalar(32));
  BodyState Ignored;
  if (!captureBody(*ExtraVReg, Ignored)) MachineTypedBodyTestBits |= 128;
}

// Invoked only by an explicit test scope. The scratch MF is never registered
// with current MMI, and none of its blocks enter the actual emitted function.
void testMachineNumbering(llvm::MachineFunction &Current,
                          llvm::MachineModuleInfo &MMI) {
  if (!MachineNumberingTests || MachineNumberingTestsRan) return;
  MachineNumberingTestsRan = true;
  testTypedBody(Current);
  llvm::MachineFunction MF(Current.getFunction(), Current.getTarget(),
                           Current.getSubtarget(), Current.getContext(), 37);
  // Match MachineModuleInfo's construction contract: the constructor creates
  // generic state only; target MIR printing requires initialized target info.
  MF.initTargetMachineFunctionInfo(Current.getSubtarget());
  auto *A = MF.CreateMachineBasicBlock();
  auto *B = MF.CreateMachineBasicBlock();
  auto *C = MF.CreateMachineBasicBlock();
  MF.push_back(A); MF.push_back(B); MF.push_back(C);
  A->addSuccessor(B); B->addSuccessor(C);
  const unsigned Numbers[] = {6, 1, 4};
  const unsigned Analysis[] = {8, 2, 6};
  const auto Restore = targetInfoAPI().restoreBlockNumbers;
  auto Check = [](unsigned Bit, bool Good) {
    if (Good) MachineNumberingTestBits |= uint32_t(1) << Bit;
  };
  auto Intact = [&]() {
    return MF.size() == 3 && MF.getFunctionNumber() == 37 &&
           MF.getNumBlockIDs() == 9 && MF.getMaxAnalysisBlockNumber() == 12 &&
           MF.getAnalysisBlockNumberEpoch() == 7 &&
           &MF.front() == A && &MF.back() == C &&
           A->getNumber() == 6 && B->getNumber() == 1 && C->getNumber() == 4 &&
           A->getAnalysisNumber() == 8 && B->getAnalysisNumber() == 2 &&
           C->getAnalysisNumber() == 6 && MF.getBlockNumbered(6) == A &&
           MF.getBlockNumbered(1) == B && MF.getBlockNumbered(4) == C;
  };
  if (!Restore(&MF, 9, 12, 7, Numbers, Analysis, 3) || !Intact()) return;
  Check(0, true);
  llvm::MachineDominatorTree DT;
  DT.recalculate(MF);
  Check(1, DT.dominates(A, C) && DT.dominates(B, C) &&
           DT.getNode(A) && DT.getNode(C) &&
           llvm::GraphTraits<llvm::MachineFunction *>::getMaxNumber(&MF) == 12 &&
           llvm::GraphTraits<llvm::MachineFunction *>::getNumberEpoch(&MF) == 7);
  const std::string Before = machineText(MMI, MF);
  auto Reject = [&](unsigned Bit, unsigned Capacity, unsigned Next,
                    const unsigned *IDs, const unsigned *AnalysisIDs, uint64_t Count) {
    const bool Refused = !Restore(&MF, Capacity, Next, 99, IDs, AnalysisIDs, Count);
    Check(Bit, Refused && Intact() && machineText(MMI, MF) == Before);
  };
  const unsigned DuplicateIDs[] = {6, 6, 4}, DuplicateAnalysis[] = {8, 8, 6};
  const unsigned OutsideIDs[] = {9, 1, 4}, OutsideAnalysis[] = {12, 2, 6};
  const unsigned NarrowingIDs[] = {~unsigned(0), 1, 4};
  const unsigned TooLarge = MaxMachinePayload / sizeof(llvm::MachineBasicBlock *) + 1;
  Reject(2, 9, 12, DuplicateIDs, Analysis, 3);
  Reject(3, 9, 12, Numbers, DuplicateAnalysis, 3);
  Reject(4, 9, 12, OutsideIDs, Analysis, 3);
  Reject(5, 9, 12, Numbers, OutsideAnalysis, 3);
  Reject(6, 9, 12, Numbers, Analysis, 2);
  Reject(7, 9, 12, nullptr, Analysis, 3);
  Reject(8, 9, 12, Numbers, nullptr, 3);
  Reject(9, TooLarge, 12, Numbers, Analysis, 3);
  Reject(10, 9, TooLarge, Numbers, Analysis, 3);
  Reject(11, 9, 12, NarrowingIDs, Analysis, 3);
  // Exercise allocation and analysis consumers after sparse restoration.
  auto *D = MF.CreateMachineBasicBlock(); MF.push_back(D); C->addSuccessor(D);
  DT.recalculate(MF);
  Check(12, D->getNumber() == 9 && D->getAnalysisNumber() == 12 &&
            MF.getBlockNumbered(9) == D && MF.getNumBlockIDs() == 10 &&
            MF.getMaxAnalysisBlockNumber() == 13 && DT.dominates(B, D));
  A->removeSuccessor(B); B->removeSuccessor(C); A->addSuccessor(C); MF.erase(B);
  auto *E = MF.CreateMachineBasicBlock(); MF.push_back(E); D->addSuccessor(E);
  DT.recalculate(MF);
  Check(13, E->getNumber() == 10 && E->getAnalysisNumber() == 13 &&
            MF.getBlockNumbered(6) == A && MF.getBlockNumbered(4) == C &&
            MF.getBlockNumbered(9) == D && MF.getBlockNumbered(10) == E &&
            MF.getNumBlockIDs() == 11 && MF.getMaxAnalysisBlockNumber() == 14 &&
            MF.getAnalysisBlockNumberEpoch() == 7 && DT.dominates(A, E) &&
            DT.dominates(C, E) && DT.dominates(D, E));
  for (auto &BB : MF)
    while (!BB.succ_empty()) BB.removeSuccessor(BB.succ_begin());
  while (!MF.empty()) MF.erase(MF.begin());
  const bool Empty = Restore(&MF, 0, 0, 9, nullptr, nullptr, 0);
  auto *Fresh = MF.CreateMachineBasicBlock(); MF.push_back(Fresh);
  DT.recalculate(MF);
  Check(14, Empty && Fresh->getNumber() == 0 && Fresh->getAnalysisNumber() == 0 &&
            MF.getBlockNumbered(0) == Fresh && MF.getNumBlockIDs() == 1 &&
            MF.getMaxAnalysisBlockNumber() == 1 && MF.getAnalysisBlockNumberEpoch() == 9 &&
            MF.getFunctionNumber() == 37 && DT.getNode(Fresh));
  if (snapshotDomain(MF)) MachineBundleTestBits |= 1;
  const auto *TII = MF.getSubtarget().getInstrInfo();
  auto *Head = MF.CreateMachineInstr(TII->get(llvm::TargetOpcode::BUNDLE), llvm::DebugLoc(), true);
  auto *Member = MF.CreateMachineInstr(TII->get(llvm::TargetOpcode::KILL), llvm::DebugLoc(), true);
  Fresh->push_back(Head); Fresh->push_back(Member);
  Head->setFlag(llvm::MachineInstr::BundledSucc);
  Member->setFlag(llvm::MachineInstr::BundledPred);
  Member->setDeactivationSymbol(MF, &MF.getFunction());
  if (std::distance(Fresh->begin(), Fresh->end()) == 1 &&
      std::distance(Fresh->instr_begin(), Fresh->instr_end()) == 2 &&
      !snapshotDomain(MF)) MachineBundleTestBits |= 2;
  Member->setDeactivationSymbol(MF, nullptr);
  if (snapshotDomain(MF)) MachineBundleTestBits |= 4;
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
  int alignmentTestMax;
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
  std::string BoundConfig = Config;
  if (MachineAlignmentTestMax >= 0)
    BoundConfig += "; explicit-test-block-align64-max=" + std::to_string(MachineAlignmentTestMax);
  if (!targetInfoAPI().available() || BoundConfig.size() > MaxMachinePayload)
    return {nullptr, [](MachineRequest *R) { delete R; }};
  const size_t ConfigBytes = BoundConfig.size();
  return {new MachineRequest{M, TM, std::move(BoundConfig), std::move(Prior), Stats, Failed,
                             MachineAlignmentTestMax, {}, ConfigBytes, 0},
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
    const auto KeyStart = Clock::now();
    bool KeyRecorded = false;
    const auto &Triple = R.machine.getTargetTriple();
    if (Triple.getArch() == llvm::Triple::x86_64 && Triple.isOSLinux() &&
        R.reservedSlots < MaxMachineSlots && R.reservedBytes < MaxMachinePayload &&
        inputDomain(F)) {
      C.recording = std::make_shared<Snapshot>();
      if (!bindPipeline(P, *C.recording)) C.recording.reset();
    }
    if (C.recording) {
      C.recording->input = functionText(F);
      C.recording->world = worldText(R.module, &F, &R.stats);
      R.stats.mir_key_bytes += C.recording->input.size() + C.recording->world.size();
      if (C.recording->input.empty() || C.recording->world.empty() ||
          !R.reserve(C, recordingReservation(*C.recording, F.getName().size())))
        R.discardRecording(C);
    }
    if (C.recording) {
      if (R.prior && R.prior->config == R.config) {
        auto Old = R.prior->functions.find(F.getName().str());

        if (MachineDiagnostics && Old != R.prior->functions.end()) {
          if (Old->second->input != C.recording->input)
            diagnoseDifference(F, Old->second->input, C.recording->input,
                               "machine-input-key-difference");
          if (Old->second->world != C.recording->world)
            diagnoseDifference(F, Old->second->world, C.recording->world,
                               "machine-world-key-difference");
          if (Old->second->pipeline != C.recording->pipeline ||
              Old->second->firstMachine != C.recording->firstMachine)
            diagnose(F, "machine-pipeline-key-difference");
        }

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
          R.stats.mir_key_ns += elapsedNS(KeyStart);
          KeyRecorded = true;
          {
            // mir_parse_ns is combined reconstruction time: tiny MIR header
            // parsing, typed current MI/MBB/use-chain construction, and the
            // parser/header destruction. It is not pure YAML parsing time.
            // Pause around restoration/validation so these scopes do not overlap.
            ScopedNS ParseTime(R.stats.mir_parse_ns);
            auto Parser = llvm::createMIRParser(
                llvm::MemoryBuffer::getMemBufferCopy(C.replay->mirHeader, "zil-current-machine-header"),
                R.module.getContext());
            // Required to initialize the YAML cursor. This header is empty;
            // parseMachineFunctions explicitly receives the CURRENT Module/MMI.
            auto EmptyHeader = Parser->parseIRModule();
            const bool ParseFailed = EmptyHeader && Parser->parseMachineFunctions(R.module, MMI);
            bool BodyFailed = false;
            if (EmptyHeader && !ParseFailed) {
              auto *CurrentMF = MMI.getMachineFunction(F);
              BodyFailed = !CurrentMF || !restoreBody(*CurrentMF, C.replay->body, &C.replay->frame);
            }
            ParseTime.pause();
            if (!EmptyHeader) {
              diagnose(F, "parse-header");
              R.fail();
            } else if (ParseFailed) {
              diagnose(F, "parse-machine");
              R.fail();
            } else if (BodyFailed) {
              diagnose(F, "restore-typed-machine-body");
              R.fail();
            } else {
              ScopedNS RestoreTime(R.stats.mir_restore_validate_ns);
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
              } else if (!restoreBlockEmission(*MF, *C.replay)) {
                diagnose(F, "restore-block-emission-state");
                R.fail();
              } else {
                MF->getProperties() = C.replay->properties;
                BodyState CurrentBody;
                if (!captureBody(*MF, CurrentBody) || !(CurrentBody == C.replay->body)) {
                  diagnose(F, "typed-machine-body-difference");
                  R.fail();
                } else if (!snapshotDomain(*MF)) {
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
            ParseTime.resume();
          }
          if (!R.failed) {
            ++R.stats.machine_function_reuses;
            R.stats.mir_replay_bytes += C.replay->mir.size();
          }
        }
      }
    }
    if (!KeyRecorded) R.stats.mir_key_ns += elapsedNS(KeyStart);
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
    // A test can exercise actual padding emission. This policy is bound in
    // request.config; warm replay must restore it, never reapply the setter.
    if (!C.replay && R.alignmentTestMax >= 0)
      if (auto *MF = MMI.getMachineFunction(F))
        for (auto &BB : *MF)
          if (!BB.isEntryBlock()) BB.setAlignment(llvm::Align(64), R.alignmentTestMax);
    if (MachineNumberingTests)
      if (auto *MF = MMI.getMachineFunction(F)) testMachineNumbering(*MF, MMI);
    ++R.stats.asm_printer_runs;
    C.emitted = true;
    if (!C.replay && C.recording) {
      ScopedNS CaptureTime(R.stats.mir_capture_ns);
      auto *MF = MMI.getMachineFunction(F);
      if (!MF || !snapshotDomain(*MF) ||
          C.recording->input != functionText(F) ||
          C.recording->world != worldText(R.module, &F, &R.stats) ||
          !frame::capture(*MF, C.recording->frame, MaxMachinePayload) ||
          !captureBody(*MF, C.recording->body)) {
        if (MachineDiagnostics && MF) {
          diagnose(F, "cold-typed-body-not-retained",
                   "virtual-registers=" + std::to_string(MF->getRegInfo().getNumVirtRegs()));
        }
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
            BlockEmissionState State{BB.getMaxBytesForAlignment(), BB.hasLabelMustBeEmitted(), {}};
            for (const auto *Pred : BB.predecessors()) State.predecessors.push_back(Pred->getNumber());
            C.recording->blockEmission.push_back(std::move(State));
          }
          C.recording->mir = machineText(MMI, *MF);
          // The fixed MIR writer emits the literal body as its final field.
          // Preserve every header field verbatim, but let LLVM construct only
          // the empty current MF/MRI/frame/target state. Instruction parsing is
          // replaced by the typed, current-owner reconstruction above.
          const auto BodyAt = C.recording->mir.find("\nbody:");
          if (BodyAt == std::string::npos) {
            R.discardRecording(C);
            return 0;
          }
          const std::string EmptyBody = C.recording->mir.substr(0, BodyAt) + "\nbody:             |\n\n...\n";
          C.recording->mirHeader = frame::headerForEmptyBody(EmptyBody, MF->getTarget(),
              !C.recording->frame.saves.empty() || !C.recording->frame.restores.empty());
          if (C.recording->mirHeader.empty()) {
            R.discardRecording(C); return 0;
          }
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
      ScopedNS CaptureTime(R.stats.mir_capture_ns);
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

extern "C" int ZilNativeProductsMachineNumberingTests(int Enabled) {
  const bool Old = zil::MachineNumberingTests;
  zil::MachineNumberingTests = Enabled != 0;
  if (Enabled) {
    zil::MachineNumberingTestsRan = false;
    zil::MachineNumberingTestBits = 0;
    zil::MachineBundleTestBits = 0;
    zil::MachineTypedBodyTestBits = 0;
  }
  return Old;
}
extern "C" uint32_t ZilNativeProductsMachineNumberingTestResults() {
  return zil::MachineNumberingTestBits |
         (zil::MachineNumberingTestsRan ? uint32_t(0x80000000) : 0);
}

extern "C" int ZilNativeProductsMachineAlignmentTest(int MaxBytes) {
  const int Old = zil::MachineAlignmentTestMax;
  if (MaxBytes >= -1) zil::MachineAlignmentTestMax = MaxBytes;
  return Old;
}

extern "C" uint32_t ZilNativeProductsMachineBundleTestResults() {
  return zil::MachineBundleTestBits;
}

extern "C" uint32_t ZilNativeProductsMachineTypedBodyTestResults() {
  return zil::MachineTypedBodyTestBits;
}
