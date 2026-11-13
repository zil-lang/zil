// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "machine_emission_products.h"
#include "llvm/ADT/DenseMap.h"
#include "llvm/ADT/StringSwitch.h"
#include "llvm/CodeGen/MachineBasicBlock.h"
#include "llvm/CodeGen/MachineFunction.h"
#include "llvm/CodeGen/MachineInstr.h"
#include "llvm/CodeGen/TargetInstrInfo.h"
#include "llvm/CodeGen/TargetSubtargetInfo.h"
#include "llvm/CodeGen/TargetOpcodes.h"
#include "llvm/MC/MCExpr.h"
#include "llvm/MC/MCDwarf.h"
#include "llvm/MC/MCInst.h"
#include "llvm/Support/Casting.h"
#include "llvm/Target/TargetMachine.h"
#include <algorithm>
#include <cassert>
#include <chrono>
#include <cstring>
#include <limits>
#include <new>
#include <type_traits>
#include <thread>
#include <vector>
namespace zil::emission {
namespace {
thread_local Stats LastStats;
thread_local int TestMode=0;
thread_local uint32_t CounterexampleBits=0;
constexpr uint32_t Version = 1;
constexpr size_t MaxBytes = 64u * 1024 * 1024;
using Clock = std::chrono::steady_clock;
struct Timer {
  uint64_t &out;
  Clock::time_point start = Clock::now();
  ~Timer() { out += std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now()-start).count(); }
};
struct Header {
  uint32_t version = Version, bytes = 0, blocks = 0;
  uint32_t eventsOffset = 0, operandsOffset = 0, maxEvents = 0, maxOperands = 0;
  uint32_t events = 0, operands = 0;
};
struct Block {
  uint32_t first = 0, count = 0, sourceInstructions = 0, instructions = 0, realCode = 0;
};
struct Event {
  enum Kind : uint32_t { Instruction, CFI };
  uint32_t kind, source, opcode, flags, first, count;
};
struct Operand {
  enum Kind : uint32_t { Register, Immediate, BlockSymbol };
  uint64_t value;
  uint32_t kind, reserved;
};
static_assert(std::is_trivially_copyable_v<Header> && std::is_trivially_copyable_v<Block> &&
              std::is_trivially_copyable_v<Event> && std::is_trivially_copyable_v<Operand>);
static_assert(alignof(Operand) <= alignof(std::max_align_t));
size_t align(size_t N, size_t A) { return (N + A - 1) & ~(A - 1); }
Header &header(Blob &B) { return *reinterpret_cast<Header *>(B.data()); }
const Header &header(const Blob &B) { return *reinterpret_cast<const Header *>(B.data()); }
Block *blocks(Blob &B) { return reinterpret_cast<Block *>(B.data()+sizeof(Header)); }
const Block *blocks(const Blob &B) { return reinterpret_cast<const Block *>(B.data()+sizeof(Header)); }
Event *events(Blob &B) { return reinterpret_cast<Event *>(B.data()+header(B).eventsOffset); }
const Event *events(const Blob &B) { return reinterpret_cast<const Event *>(B.data()+header(B).eventsOffset); }
Operand *operands(Blob &B) { return reinterpret_cast<Operand *>(B.data()+header(B).operandsOffset); }
const Operand *operands(const Blob &B) { return reinterpret_cast<const Operand *>(B.data()+header(B).operandsOffset); }
bool harmless(const llvm::MachineInstr &I) {
  return I.getOpcode()==llvm::TargetOpcode::KILL || I.getOpcode()==llvm::TargetOpcode::IMPLICIT_DEF;
}
bool isCFI(const llvm::MachineInstr &I) { return I.getOpcode()==llvm::TargetOpcode::CFI_INSTRUCTION; }
// Fixed7a X86MCInstLower.cpp2273-2689: all explicit emitInstruction switch
// paths except these two must decline. Names are read from the bound CURRENT
// opcode descriptor table, not supplied by disk or used to identify a Pass.
bool special(llvm::StringRef N) {
  return llvm::StringSwitch<bool>(N)
      .Case("ASAN_CHECK_MEMACCESS", true)
      .Case("CALL64m", true)
      .Case("CALL64pcrel32", true)
      .Case("CALL64r", true)
      .Case("CATCHRET", true)
      .Case("CLEANUPRET", true)
      .Case("DBG_VALUE", true)
      .Case("EH_RETURN", true)
      .Case("EH_RETURN64", true)
      .Case("ENDBR32", true)
      .Case("ENDBR64", true)
      .Case("FAULTING_OP", true)
      .Case("FENTRY_CALL", true)
      .Case("JCC_SELF", true)
      .Case("JMP16m", true)
      .Case("JMP16r", true)
      .Case("JMP32m", true)
      .Case("JMP32r", true)
      .Case("JMP64m", true)
      .Case("JMP64r", true)
      .Case("KCFI_CHECK", true)
      .Case("MORESTACK_RET", true)
      .Case("MORESTACK_RET_RESTORE_R10", true)
      .Case("MOVPC32r", true)
      .Case("PATCHABLE_EVENT_CALL", true)
      .Case("PATCHABLE_FUNCTION_ENTER", true)
      .Case("PATCHABLE_OP", true)
      .Case("PATCHABLE_RET", true)
      .Case("PATCHABLE_TAIL_CALL", true)
      .Case("PATCHABLE_TYPED_EVENT_CALL", true)
      .Case("PATCHPOINT", true)
      .Case("SEH_BeginEpilogue", true)
      .Case("SEH_EndEpilogue", true)
      .Case("SEH_EndPrologue", true)
      .Case("SEH_Push2Regs", true)
      .Case("SEH_PushFrame", true)
      .Case("SEH_PushReg", true)
      .Case("SEH_SaveReg", true)
      .Case("SEH_SaveXMM", true)
      .Case("SEH_SetFrame", true)
      .Case("SEH_SplitChained", true)
      .Case("SEH_SplitChainedAtEndOfBlock", true)
      .Case("SEH_StackAlign", true)
      .Case("SEH_StackAlloc", true)
      .Case("SEH_UnwindV2Start", true)
      .Case("SEH_UnwindVersion", true)
      .Case("STACKMAP", true)
      .Case("STATEPOINT", true)
      .Case("TAILJMPd", true)
      .Case("TAILJMPd64", true)
      .Case("TAILJMPd64_CC", true)
      .Case("TAILJMPd_CC", true)
      .Case("TAILJMPm", true)
      .Case("TAILJMPm64", true)
      .Case("TAILJMPm64_REX", true)
      .Case("TAILJMPr", true)
      .Case("TAILJMPr64", true)
      .Case("TAILJMPr64_REX", true)
      .Case("TLS_addr32", true)
      .Case("TLS_addr64", true)
      .Case("TLS_addrX32", true)
      .Case("TLS_base_addr32", true)
      .Case("TLS_base_addr64", true)
      .Case("TLS_base_addrX32", true)
      .Case("TLS_desc32", true)
      .Case("TLS_desc64", true)
      .Case("UBSAN_UD1", true)
      .Default(false);
}
bool instructionDomain(const llvm::MachineInstr &I, const llvm::MachineFunction &MF) {
  if (I.isBundledWithPred() || I.isBundledWithSucc() || I.isDebugInstr() ||
      I.isCall() || I.isInlineAsm() || I.getDebugLoc() || I.getPreInstrSymbol() ||
      I.getPostInstrSymbol() || I.getPCSections() || I.getHeapAllocMarker() ||
      I.getMMRAMetadata() || I.getDeactivationSymbol() || I.peekDebugInstrNum()) return false;
  if (harmless(I)) return true;
  if (isCFI(I)) return I.getNumOperands()==1 && I.getOperand(0).isCFIIndex() &&
      I.getOperand(0).getCFIIndex()<MF.getFrameInstructions().size();
  const auto *TII=MF.getSubtarget().getInstrInfo();
  if (I.getOpcode()>=TII->getNumOpcodes() || &I.getDesc()!=&TII->get(I.getOpcode()) ||
      I.getOpcode()<llvm::TargetOpcode::GENERIC_OP_END || I.getDesc().isPseudo() ||
      special(TII->getName(I.getOpcode()))) return false;
  unsigned Targets=0;
  for (const auto &O:I.operands()) {
    if (O.isReg()) { if (O.getReg().isVirtual()) return false; }
    else if (O.isMBB()) { if (++Targets>1 || O.getTargetFlags() || !O.getMBB() || O.getMBB()->getParent()!=&MF) return false; }
    else if (!O.isImm() || O.getTargetFlags()) return false;
  }
  return true;
}
bool structurallyValid(const Blob &B) {
  if (B.capacity()<sizeof(Header)) return false;
  const auto &H=header(B);
  const size_t BlockEnd=sizeof(Header)+size_t(H.blocks)*sizeof(Block);
  if (H.version!=Version || H.bytes!=B.capacity() || H.bytes>MaxBytes ||
      BlockEnd>B.capacity() || H.eventsOffset!=align(BlockEnd,alignof(Event)) ||
      H.operandsOffset!=align(size_t(H.eventsOffset)+size_t(H.maxEvents)*sizeof(Event),alignof(Operand)) ||
      H.events>H.maxEvents || H.operands>H.maxOperands ||
      size_t(H.operandsOffset)+size_t(H.maxOperands)*sizeof(Operand)!=B.capacity()) return false;
  size_t NextEvent=0,NextOperand=0;
  for (size_t I=0;I<H.blocks;++I) {
    const auto &K=blocks(B)[I];
    if (K.first!=NextEvent || K.count>H.events-NextEvent || K.realCode>1) return false;
    uint32_t Prev=0; bool First=true;
    for (size_t J=0;J<K.count;++J) {
      const auto &E=events(B)[K.first+J];
      if (E.source>=K.sourceInstructions || (!First && E.source<=Prev) || E.first!=NextOperand || E.count>H.operands-NextOperand) return false;
      First=false; Prev=E.source;
      if (E.kind==Event::CFI) { if(E.count || E.opcode || E.flags) return false; }
      else if(E.kind!=Event::Instruction) return false;
      NextOperand+=E.count;
    }
    NextEvent+=K.count;
  }
  if (NextEvent!=H.events || NextOperand!=H.operands) return false;
  for(size_t I=0;I<H.operands;++I) {
    const auto &O=operands(B)[I];
    if(O.reserved || O.kind>Operand::BlockSymbol ||
       (O.kind==Operand::Register && O.value>std::numeric_limits<uint32_t>::max()) ||
       (O.kind==Operand::BlockSymbol && O.value>=H.blocks)) return false;
  }
  return true;
}
}
llvm::IntrusiveRefCntPtr<Blob> Blob::allocate(size_t Capacity) {
  if(Capacity>MaxBytes || sizeof(Blob)>MaxBytes-Capacity) return llvm::IntrusiveRefCntPtr<Blob>{};
  void *P=::operator new(sizeof(Blob)+Capacity,std::nothrow);
  if(!P) return llvm::IntrusiveRefCntPtr<Blob>{};
  return llvm::IntrusiveRefCntPtr<Blob>(::new(P) Blob(Capacity));
}
void Blob::Retain() const { references.fetch_add(1,std::memory_order_relaxed); }
void Blob::Release() const { if(references.fetch_sub(1,std::memory_order_acq_rel)==1) delete this; }
void Blob::operator delete(void *P) noexcept { ::operator delete(P); }
size_t payloadBytes(const Product &P) { return P?P->allocationBytes():0; }
bool domain(const llvm::MachineFunction &MF) {
  const auto &T=MF.getTarget().getTargetTriple();
  if(!T.isX86_64() || !T.isOSLinux() || !T.isOSBinFormatELF() || MF.empty() ||
      MF.getSubtarget().checkFeatures("+branch-hint") || MF.hasBBSections() ||
      MF.getTarget().Options.BBAddrMap || MF.getTarget().Options.MCOptions.ShowMCEncoding ||
      !MF.getPrefetchTargets().empty()) return false;
  for(const auto &B:MF) for(const auto &I:B.instrs()) if(!instructionDomain(I,MF)) return false;
  return true;
}
struct Capture::State {
  const llvm::MachineFunction &mf;
  const llvm::MachineBasicBlock *block=nullptr;
  const llvm::MachineInstr *instruction=nullptr;
  llvm::IntrusiveRefCntPtr<Blob> blob;
  llvm::DenseMap<const llvm::MachineBasicBlock *,uint32_t> ordinals;
  PeekSymbol peek;
  Stats &stats;
  uint32_t blockIndex=0, instructionIndex=0, emitted=0;
  bool bad=false;
  State(const llvm::MachineFunction &F,PeekSymbol P,Stats &S):mf(F),peek(P),stats(S){}
  void decline() { if(!bad)++stats.declined_functions; bad=true; blob.reset(); }
  bool closeInstruction() {
    if(!instruction) return true;
    if((!harmless(*instruction)&&!isCFI(*instruction)) ? emitted!=1 : emitted!=0) { decline();return false; }
    instruction=nullptr; ++instructionIndex; return true;
  }
};
Capture::Capture(std::unique_ptr<State> S):state(std::move(S)){}
Capture::~Capture()=default;
std::unique_ptr<Capture> Capture::begin(const llvm::MachineFunction &MF,Reserve R,
                                      void *Owner,PeekSymbol Peek,Stats &Stats) {
  Timer T{Stats.capture_ns};
  if(TestMode==2) {++Stats.budget_declines;return {};}
  if(!R || !Peek || !domain(MF)) { ++Stats.declined_functions;return {}; }
  size_t N=0,O=0;
  for(const auto &B:MF) for(const auto &I:B.instrs()) {
    if(++N>MaxBytes/sizeof(Event) || O>=MaxBytes/sizeof(Operand) || I.getNumOperands()>MaxBytes/sizeof(Operand)-O-1) return {};
    O+=I.getNumOperands()+1; // exact finite buffer cap; excess output declines
  }
  size_t EO=align(sizeof(Header)+MF.size()*sizeof(Block),alignof(Event));
  size_t OO=align(EO+N*sizeof(Event),alignof(Operand));
  if(OO>MaxBytes || O>(MaxBytes-OO)/sizeof(Operand)) {++Stats.budget_declines;return {};}
  size_t Bytes=OO+O*sizeof(Operand);
  if(Bytes>MaxBytes-sizeof(Blob) || !R(Owner,sizeof(Blob)+Bytes)) {++Stats.budget_declines;return {};}
  auto S=std::make_unique<State>(MF,Peek,Stats); S->blob=Blob::allocate(Bytes);
  if(!S->blob) {++Stats.budget_declines;return {};}
  Header H;H.bytes=Bytes;H.blocks=MF.size();H.eventsOffset=EO;H.operandsOffset=OO;H.maxEvents=N;H.maxOperands=O;
  ::new (S->blob->data()) Header(H);
  // Start actual array lifetimes, including unused pool capacity, without
  // initializing trivial Event/Operand scalars. All consumed rows are written
  // before their counts advance; no uninitialized tail is read or published.
  ::new (blocks(*S->blob)) Block[H.blocks];
  ::new (events(*S->blob)) Event[H.maxEvents];
  ::new (operands(*S->blob)) Operand[H.maxOperands];
  for(const auto &B:MF) S->ordinals.try_emplace(&B,S->ordinals.size());
  return std::unique_ptr<Capture>(new Capture(std::move(S)));
}
bool Capture::active() const { return state&&!state->bad&&state->blob; }
bool Capture::beginBlock(const llvm::MachineBasicBlock &B) {
  if(!active())return false;auto &S=*state;Timer T{S.stats.capture_ns};
  auto Ordinal=S.ordinals.find(&B);
  if(S.block || S.blockIndex>=header(*S.blob).blocks || Ordinal==S.ordinals.end() || Ordinal->second!=S.blockIndex || B.getParent()!=&S.mf) {S.decline();return false;}
  S.block=&B;S.instructionIndex=0;blocks(*S.blob)[S.blockIndex].first=header(*S.blob).events;return true;
}
bool Capture::beginInstruction(const llvm::MachineInstr &I) {
  if(!active())return false;auto &S=*state;Timer T{S.stats.capture_ns};
  if(!S.block || I.getParent()!=S.block || !S.closeInstruction()) {S.decline();return false;}
  S.instruction=&I;S.emitted=0;return true;
}
void Capture::recordInstruction(const llvm::MCInst &I,const llvm::MCSubtargetInfo &STI) {
  if(!active())return;auto &S=*state;Timer T{S.stats.capture_ns};
  if(!S.instruction || harmless(*S.instruction) || isCFI(*S.instruction) ||
      &STI!=static_cast<const llvm::MCSubtargetInfo *>(&S.mf.getSubtarget()) || I.getLoc().isValid() || ++S.emitted!=1) {S.decline();return;}
  auto &H=header(*S.blob);
  if(H.events==H.maxEvents || I.getNumOperands()>H.maxOperands-H.operands) {S.decline();return;}
  Event E{};E.source=S.instructionIndex;E.opcode=I.getOpcode();E.flags=I.getFlags();E.first=H.operands;E.count=I.getNumOperands();
  for(const auto &MO:I) {
    Operand O{};
    if(MO.isReg()){O.kind=Operand::Register;O.value=MO.getReg();}
    else if(MO.isImm()){O.kind=Operand::Immediate;auto V=MO.getImm();std::memcpy(&O.value,&V,sizeof(V));}
    else if(MO.isExpr()) {
      const auto *E=llvm::dyn_cast<llvm::MCSymbolRefExpr>(MO.getExpr());
      if(!E || E->getSpecifier()!=0 || E->getLoc().isValid()) {S.decline();return;}
      const llvm::MachineBasicBlock *Found=nullptr;
      for(const auto &P:S.instruction->operands()) if(P.isMBB() &&
          S.peek(P.getMBB())==&E->getSymbol()) {if(Found && Found!=P.getMBB()){S.decline();return;}Found=P.getMBB();}
      auto It=S.ordinals.find(Found);
      if(!Found || It==S.ordinals.end()) {S.decline();return;}
      O.kind=Operand::BlockSymbol;O.value=It->second;
    } else {S.decline();return;}
    operands(*S.blob)[H.operands++]=O;
  }
  events(*S.blob)[H.events++]=E;
}
void Capture::recordCFI(const llvm::MachineInstr &I) {
  if(!active())return;auto &S=*state;Timer T{S.stats.capture_ns};
  if(S.instruction!=&I || !isCFI(I)) {S.decline();return;}
  auto &H=header(*S.blob);if(H.events==H.maxEvents){S.decline();return;}
  Event E{};E.kind=Event::CFI;E.source=S.instructionIndex;E.first=H.operands;
  events(*S.blob)[H.events++]=E;
}
void Capture::endBlock(const llvm::MachineBasicBlock &B,Summary Summary) {
  if(!active())return;auto &S=*state;Timer T{S.stats.capture_ns};
  if(S.block!=&B || !S.closeInstruction()){S.decline();return;}
  auto &K=blocks(*S.blob)[S.blockIndex];K.count=header(*S.blob).events-K.first;
  K.sourceInstructions=S.instructionIndex;K.instructions=Summary.instructions;K.realCode=Summary.realCode;
  S.block=nullptr;++S.blockIndex;++S.stats.captured_blocks;
}
Product Capture::finish() {
  if(!active())return Product{};auto &S=*state;Timer T{S.stats.capture_ns};
  if(S.block || S.blockIndex!=header(*S.blob).blocks || !structurallyValid(*S.blob)){S.decline();return Product{};}
  ++S.stats.captured_functions;S.stats.captured_capacity_bytes+=S.blob->allocationBytes();
  Product P(S.blob.get());S.blob.reset();return P;
}
bool preflight(const Product &P,const llvm::MachineFunction &MF,Stats &Stats) {
  Timer T{Stats.preflight_ns};if(!P || !structurallyValid(*P) || !domain(MF) || MF.size()!=header(*P).blocks)return false;
  std::vector<const llvm::MachineBasicBlock *> BBs;for(const auto &B:MF)BBs.push_back(&B);
  size_t BI=0;
  for(const auto &B:MF){const auto &K=blocks(*P)[BI++];size_t MI=0,EI=0,Count=0;bool Real=false;
    for(const auto &I:B.instrs()){
      Real|=!I.isPosition()&&!I.isImplicitDef()&&!I.isKill()&&!I.isDebugInstr();
      const bool Plain=!harmless(I)&&!isCFI(I);Count+=Plain&&!I.isMetaInstruction();
      if(!harmless(I)){
        if(EI==K.count)return false;const auto &E=events(*P)[K.first+EI++];
        if(E.source!=MI || (isCFI(I)?E.kind!=Event::CFI:E.kind!=Event::Instruction))return false;
        if(E.kind==Event::Instruction && E.opcode>=MF.getSubtarget().getInstrInfo()->getNumOpcodes()) return false;
        for(size_t OI=0;OI<E.count;++OI){const auto &O=operands(*P)[E.first+OI];if(O.kind==Operand::BlockSymbol){
          bool Found=false;for(const auto &MO:I.operands())if(MO.isMBB()&&MO.getMBB()==BBs[O.value])Found=true;
          if(!Found)return false;
        }}
      }
      ++MI;
    }
    if(MI!=K.sourceInstructions || EI!=K.count || Count!=K.instructions || Real!=bool(K.realCode))return false;
  }
  return true;
}
bool replay(const Product &P,const llvm::MachineBasicBlock &B,void *AP,EmitMC Emit,
            EmitCFI CFI,Summary &Summary,Stats &Stats,ObserveEvent Observer) {
  Timer T{Stats.replay_ns};if(!P||!Emit||!CFI)return false;
  const auto &MF=*B.getParent();std::vector<const llvm::MachineBasicBlock *> BBs;
  size_t BI=0;for(const auto &BB:MF){if(&BB==&B)BI=BBs.size();BBs.push_back(&BB);}
  if(BI>=header(*P).blocks || BBs[BI]!=&B)return false;
  const auto &K=blocks(*P)[BI];std::vector<const llvm::MachineInstr *> MIs;for(const auto &I:B.instrs())MIs.push_back(&I);
  for(size_t J=0;J<K.count;++J){const auto &E=events(*P)[K.first+J];
    if(Observer)Observer(AP,*MIs[E.source]);
    if(E.kind==Event::CFI){if(!CFI(AP,*MIs[E.source]))return false;++Stats.current_cfi_events;continue;}
    llvm::MCInst I;I.setOpcode(E.opcode);I.setFlags(E.flags);
    for(size_t OI=0;OI<E.count;++OI){const auto &O=operands(*P)[E.first+OI];
      if(O.kind==Operand::Register)I.addOperand(llvm::MCOperand::createReg(llvm::MCRegister::from(O.value)));
      else if(O.kind==Operand::Immediate){int64_t V;std::memcpy(&V,&O.value,sizeof(V));I.addOperand(llvm::MCOperand::createImm(V));}
      else I.addOperand(llvm::MCOperand::createExpr(llvm::MCSymbolRefExpr::create(BBs[O.value]->getSymbol(),MF.getContext())));
    }
    if(!Emit(AP,I))return false;++Stats.mc_replay_events;
    if(TestMode==3) return false; // deliberate dirty failure: whole attempt must be discarded
  }
  Summary.instructions=K.instructions;Summary.realCode=K.realCode;
  ++Stats.replayed_blocks;Stats.machine_instructions_skipped+=K.instructions;return true;
}
void saveLastStats(const Stats &S) { LastStats=S; }
int testMode() { return TestMode; }
uint32_t counterexamples(const Product &P,const llvm::MachineFunction &MF,PeekSymbol Peek) {
  if(!P || !Peek || !preflight(P,MF,LastStats))return 0;
  std::vector<const void *> Symbols;
  for(const auto &B:MF)Symbols.push_back(Peek(&B));
  auto Clone=[&](){auto B=Blob::allocate(P->capacity());if(B) {
    ::new (B->data()) Header(header(*P));
    ::new (blocks(*B)) Block[header(*P).blocks];
    ::new (events(*B)) Event[header(*P).maxEvents];
    ::new (operands(*B)) Operand[header(*P).maxOperands];
    for(size_t I=0;I<header(*P).blocks;++I)blocks(*B)[I]=blocks(*P)[I];
    for(size_t I=0;I<header(*P).events;++I)events(*B)[I]=events(*P)[I];
    for(size_t I=0;I<header(*P).operands;++I)operands(*B)[I]=operands(*P)[I];
  }return B;};
  Stats S; uint32_t Bits=0;
  auto B=Clone();if(!B)return 0;
  header(*B).eventsOffset=std::numeric_limits<uint32_t>::max();
  if(!preflight(Product(B.get()),MF,S))Bits|=1;
  B=Clone();if(!B)return 0;
  if(header(*B).events){events(*B)[0].source=std::numeric_limits<uint32_t>::max();
    if(!preflight(Product(B.get()),MF,S))Bits|=2;}
  B=Clone();if(!B)return 0;
  if(header(*B).operands){auto &O=operands(*B)[0];O.kind=Operand::BlockSymbol;O.value=header(*B).blocks;
    if(!preflight(Product(B.get()),MF,S))Bits|=4;}
  size_t I=0;bool Intact=true;for(const auto &BB:MF)Intact&=Peek(&BB)==Symbols[I++];
  if(Intact)Bits|=8;
  // Shared immutable product lifetimes must support real concurrent retain /
  // release, independently of any serialized driver session convention.
  std::vector<std::thread> Readers;
  for(unsigned I=0;I<4;++I)Readers.emplace_back([&](){for(unsigned J=0;J<1024;++J){Product Copy=P;assert(payloadBytes(Copy)==payloadBytes(P));}});
  for(auto &T:Readers)T.join();
  Bits|=16;CounterexampleBits=Bits;return Bits;
}
} // namespace zil::emission

extern "C" int ZilNativeProductsLastMachineEmissionStatsV1(void *Out,size_t Bytes) {
  if(!Out || Bytes!=sizeof(zil::emission::Stats))return 0;
  std::memcpy(Out,&zil::emission::LastStats,Bytes);return 1;
}
extern "C" int ZilNativeProductsMachineEmissionModeForTestsV1(int Mode) {
  int Old=zil::emission::TestMode;zil::emission::TestMode=Mode;zil::emission::CounterexampleBits=0;return Old;
}
extern "C" uint32_t ZilNativeProductsMachineEmissionCounterexamplesV1() {return zil::emission::CounterexampleBits;}
