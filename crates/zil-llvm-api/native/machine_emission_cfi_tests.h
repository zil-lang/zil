// Explicit, input-bound MACHINE TEST fixtures. Default policy zero never
// changes an instruction or emits an extra directive. No SDK changes needed.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_MACHINE_EMISSION_CFI_TESTS_H
#define ZIL_MACHINE_EMISSION_CFI_TESTS_H
#include "machine_emission_products.h"
#include "llvm/CodeGen/MachineBasicBlock.h"
#include "llvm/CodeGen/MachineFunction.h"
#include "llvm/CodeGen/MachineInstr.h"
#include "llvm/CodeGen/TargetInstrInfo.h"
#include "llvm/CodeGen/TargetSubtargetInfo.h"
#include "llvm/CodeGen/TargetOpcodes.h"
#include "llvm/MC/MCAsmInfo.h"
#include "llvm/MC/MCContext.h"
#include "llvm/MC/MCDwarf.h"
#include "llvm/MC/MCExpr.h"
#include "llvm/MC/MCInst.h"
#include "llvm/MC/MCStreamer.h"
#include "llvm/MC/MCSymbol.h"
#include "llvm/Support/Casting.h"
#include <iterator>
#include <memory>
#include <string>
namespace zil::emission::testing {
inline thread_local int CFIPolicy=0;
inline thread_local uint32_t LastCFIBits=0;
inline thread_local std::string LastCFITrace;
constexpr const char *Terminal="zil-terminal-cfi-fixture-v1";
constexpr const char *Forward0="zil-forward-cfi-fixture-v1.begin";
constexpr const char *Forward1="zil-forward-cfi-fixture-v1.end";
inline llvm::StringRef marker(const llvm::MachineFunction &MF,const llvm::MachineInstr &MI) {
  if(MI.getOpcode()!=llvm::TargetOpcode::CFI_INSTRUCTION || MI.getNumOperands()!=1 ||
     !MI.getOperand(0).isCFIIndex() || MI.getOperand(0).getCFIIndex()>=MF.getFrameInstructions().size())return {};
  const auto &I=MF.getFrameInstructions()[MI.getOperand(0).getCFIIndex()];
  return I.getOperation()==llvm::MCCFIInstruction::OpEscape ?
    llvm::StringRef(I.getExtraFields<llvm::MCCFIInstruction::EscapeFields>().Comment) : llvm::StringRef();
}
inline llvm::MachineInstr *makeCFI(llvm::MachineFunction &MF,llvm::StringRef Values,llvm::StringRef Comment) {
  const auto *TII=MF.getSubtarget().getInstrInfo();
  auto *MI=MF.CreateMachineInstr(TII->get(llvm::TargetOpcode::CFI_INSTRUCTION),llvm::DebugLoc(),true);
  auto Index=MF.addFrameInst(llvm::MCCFIInstruction::createEscape(nullptr,Values,{},Comment));
  MI->addOperand(MF,llvm::MachineOperand::CreateCFIIndex(Index));return MI;
}
struct CFIOracle {
  unsigned policy;
  uint32_t bits=0;
  const llvm::MachineFunction &mf;
  const llvm::MachineInstr *branch=nullptr,*current=nullptr,*pending=nullptr;
  const llvm::MachineBasicBlock *target=nullptr;
  const llvm::MCSymbol *symbol=nullptr;
  size_t before=0;
  PeekSymbol peek;
  std::string targetName,labels[2];
  CFIOracle(unsigned P,const llvm::MachineFunction &F,PeekSymbol K):policy(P),mf(F),peek(K){}
  void beforeInstruction(const llvm::MachineInstr &MI,const llvm::MCStreamer &S) {
    flush(S);current=&MI;
    if(policy==2 && branch==&MI && target && !peek(target))bits|=4;
  }
  void observeMC(const llvm::MCInst &I) {
    if(policy!=2 || current!=branch || !(bits&4) || !target)return;
    for(const auto &O:I)if(O.isExpr()){
      const auto *E=llvm::dyn_cast<llvm::MCSymbolRefExpr>(O.getExpr());
      if(E && !E->getSpecifier() && peek(target)==&E->getSymbol() && !E->getSymbol().isDefined()){
        symbol=&E->getSymbol();targetName=symbol->getName().str();bits|=8;
      }
    }
  }
  bool activeFrame(const llvm::MCStreamer &S) const {
    auto Frames=S.getDwarfFrameInfos();
    return !Frames.empty() && Frames.back().Begin && !Frames.back().End &&
      S.getContext().getAsmInfo().getExceptionHandlingType()==llvm::ExceptionHandling::DwarfCFI;
  }
  void beforeCFI(const llvm::MachineInstr &MI,const llvm::MCStreamer &S) {
    flush(S);
    auto M=marker(mf,MI);
    if(M!=Terminal && M!=Forward0 && M!=Forward1)return;
    if(!activeFrame(S))return;
    pending=&MI;before=S.getDwarfFrameInfos().back().Instructions.size();
    if(policy==1 && M==Terminal){
      bits|=4|8;
      auto It=std::next(MI.getIterator());bool Transient=false;
      while(It!=MI.getParent()->instr_end() && It->isTransient()){Transient=true;++It;}
      if(Transient && It==MI.getParent()->instr_end() && MI.getParent()==&mf.back())bits|=32;
    }
  }
  void flush(const llvm::MCStreamer &S) {
    if(!pending)return;
    const auto *MI=pending;pending=nullptr;
    if(!activeFrame(S))return;
    const auto &I=S.getDwarfFrameInfos().back().Instructions;
    auto M=marker(mf,*MI);
    if(policy==1 && M==Terminal){if(I.size()==before)bits|=16;return;}
    if(policy!=2 || I.size()!=before+1 || !symbol || symbol->isDefined())return;
    const auto &CFI=I.back();
    if(CFI.getOperation()!=llvm::MCCFIInstruction::OpEscape || !CFI.getLabel() ||
       !CFI.getLabel()->isDefined() || CFI.getLabel()==symbol)return;
    const auto &E=CFI.getExtraFields<llvm::MCCFIInstruction::EscapeFields>();
    // The original AsmPrinter sends the source comment to AddComment, not to
    // emitCFIEscape. Match by the identified CURRENT MI plus exact frame-table
    // increment/escape payload; the streamer's frame record has no comment.
    unsigned Which=M==Forward0?0:1;
    if(E.Values.size()!=1 || static_cast<unsigned char>(E.Values[0])!=(Which?0x0b:0x0a))return;
    labels[Which]=CFI.getLabel()->getName().str();
    if(Which==0)bits|=16;
    else if(!labels[0].empty() && labels[0]!=labels[1])bits|=32;
  }
  void beginBlock(const llvm::MachineBasicBlock &B,const llvm::MCStreamer &S) {
    flush(S);
    if(policy==2 && &B==target && symbol && peek(target)==symbol && symbol->isDefined() &&
        !labels[0].empty() && !labels[1].empty())bits|=64;
  }
  void finish(const llvm::MCStreamer &S) {
    flush(S);LastCFIBits=bits;
    LastCFITrace=policy==2 ? targetName+"|"+labels[0]+"|"+labels[1] : "terminal-skip";
  }
};
inline std::unique_ptr<CFIOracle> fixture(llvm::MachineFunction &MF,unsigned Policy,bool Fresh,PeekSymbol Peek) {
  if(!Peek || MF.empty() || (Policy!=1 && Policy!=2))return {};
  if(Fresh && Policy==1){
    // DW_CFA_def_cfa_offset 0x123456, deliberately after the final real MI.
    const unsigned char Data[]={0x0e,0xd6,0xe8,0x48};
    MF.back().push_back(makeCFI(MF,llvm::StringRef(reinterpret_cast<const char *>(Data),sizeof(Data)),Terminal));
    MF.back().push_back(MF.CreateMachineInstr(MF.getSubtarget().getInstrInfo()->get(llvm::TargetOpcode::KILL),llvm::DebugLoc(),true));
  }
  auto O=std::make_unique<CFIOracle>(Policy,MF,Peek);
  bool Marked=false;
  for(auto &B:MF)for(auto &MI:B.instrs()){
    if(Policy==1 && marker(MF,MI)==Terminal)Marked=true;
    if(Policy!=2 || O->branch || MF.getSubtarget().getInstrInfo()->getName(MI.getOpcode())!="JCC_1")continue;
    for(const auto &V:MI.operands())if(V.isMBB()){
      bool Later=false,Seen=false;for(const auto &BB:MF){if(&BB==&B)Seen=true;else if(Seen && &BB==V.getMBB())Later=true;}
      if(Later && !Peek(V.getMBB())){O->branch=&MI;O->target=V.getMBB();break;}
    }
  }
  if(Policy==2 && O->branch && Fresh){
    auto *B=const_cast<llvm::MachineBasicBlock *>(O->branch->getParent());
    auto At=std::next(const_cast<llvm::MachineInstr *>(O->branch)->getIterator());
    auto *A=makeCFI(MF,llvm::StringRef("\x0a",1),Forward0);B->insert(At,A);
    auto *Z=makeCFI(MF,llvm::StringRef("\x0b",1),Forward1);B->insert(std::next(A->getIterator()),Z);
  }
  if(Policy==2 && O->branch && O->target){
    auto It=std::next(O->branch->getIterator());
    if(It!=O->branch->getParent()->instr_end() && marker(MF,*It)==Forward0){
      ++It;if(It!=O->branch->getParent()->instr_end() && marker(MF,*It)==Forward1)Marked=true;
    }
  }
  if(!Marked || !MF.verify(nullptr,"explicit CFI fixture",nullptr,false))return {};
  O->bits=1|2;return O;
}
} // namespace zil::emission::testing
#endif
