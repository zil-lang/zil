// Fixed LLVM 23.1.4 resident optimization products. The original pass managers
// construct and run their original pipelines; this adapter never reparses one.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "work_products.h"
#include "zil_llvm_work_products_hooks.h"
#include "machine_products.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/Bitcode/BitcodeReader.h"
#include "llvm/Bitcode/BitcodeWriter.h"
#include "llvm/IR/CFG.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/IntrinsicInst.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/PassManager.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/Error.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/Target/TargetMachine.h"
#include "llvm/Transforms/Utils/Cloning.h"
#include <algorithm>
#include <dlfcn.h>
#include <map>
#include <memory>
#include <set>
#include <string>
#include <utility>
#include <vector>
using namespace llvm;
namespace zil {
namespace {
bool numeric(Type *T) {
  if (T->isIntegerTy() || T->isVoidTy()) return true;
  if (auto *S = dyn_cast<StructType>(T)) {
    if (S->isOpaque()) return false;
    for (Type *E : S->elements()) if (!E->isIntegerTy()) return false;
    return true;
  }
  return false;
}
bool approvedIntrinsic(const CallBase &C) {
  const Function *F = C.getCalledFunction();
  if (!F || !F->isIntrinsic() || !F->isDeclaration() ||
      C.hasOperandBundles() || C.isIndirectCall() || C.getCallingConv() != CallingConv::C)
    return false;
  switch (F->getIntrinsicID()) {
  case Intrinsic::sadd_with_overflow: case Intrinsic::ssub_with_overflow:
  case Intrinsic::smul_with_overflow: case Intrinsic::uadd_with_overflow:
  case Intrinsic::usub_with_overflow: case Intrinsic::umul_with_overflow:
  case Intrinsic::trap: break;
  default: return false;
  }
  if (F->isVarArg() || !numeric(C.getType())) return false;
  for (const Use &A : C.args()) if (!A->getType()->isIntegerTy()) return false;
  // Intrinsic IDs come from LLVM's symbol table; also require the canonical
  // overloaded signature, so a declaration merely spelling llvm.* cannot pass.
  SmallVector<Intrinsic::IITDescriptor, 16> Infos;
  Intrinsic::getIntrinsicInfoTableEntries(F->getIntrinsicID(), Infos);
  SmallVector<Type *, 4> Overloads;
  auto R = Intrinsic::matchIntrinsicSignature(F->getFunctionType(), Infos, Overloads);
  return R == Intrinsic::MatchIntrinsicTypes_Match &&
         !Intrinsic::matchIntrinsicVarArg(F->isVarArg(), Infos);
}
bool acyclic(const Function &F) {
  std::map<const BasicBlock *, unsigned> Degree;
  std::vector<const BasicBlock *> Ready;
  for (const BasicBlock &B : F) Degree[&B] = 0;
  for (const BasicBlock &B : F)
    for (const BasicBlock *S : successors(&B)) ++Degree[S];
  for (const auto &E : Degree) if (!E.second) Ready.push_back(E.first);
  size_t Seen = 0;
  while (!Ready.empty()) {
    const BasicBlock *B = Ready.back(); Ready.pop_back(); ++Seen;
    for (const BasicBlock *S : successors(B)) if (!--Degree[S]) Ready.push_back(S);
  }
  return Seen == F.size();
}
}
bool eligibleFunction(const Function &F) {
  if (F.isDeclaration() || F.isVarArg() || F.getCallingConv() != CallingConv::C ||
      !F.getReturnType()->isIntegerTy() || F.hasPersonalityFn() || F.hasGC() ||
      F.hasPrefixData() || F.hasPrologueData() || F.hasComdat() || !acyclic(F)) return false;
  for (const Argument &A : F.args()) if (!A.getType()->isIntegerTy()) return false;
  SmallVector<std::pair<unsigned, MDNode *>, 4> Metadata;
  F.getAllMetadata(Metadata);
  if (!Metadata.empty()) return false;
  for (const User *U : F.users()) {
    const auto *C = dyn_cast<CallBase>(U);
    if (!C || C->getCalledOperand() != &F) return false;
  }
  for (const BasicBlock &B : F) {
    if (B.hasAddressTaken()) return false;
    for (const Instruction &I : B) {
      Metadata.clear(); I.getAllMetadata(Metadata);
      if (!Metadata.empty() || I.getDebugLoc()) return false;
      if (const auto *C = dyn_cast<CallBase>(&I)) {
        if (!approvedIntrinsic(*C)) return false;
      } else if (!(isa<BinaryOperator>(I) || isa<UnaryOperator>(I) ||
                   isa<ICmpInst>(I) || isa<PHINode>(I) || isa<SelectInst>(I) ||
                   isa<BranchInst>(I) || isa<SwitchInst>(I) || isa<ReturnInst>(I) ||
                   isa<UnreachableInst>(I) || isa<ExtractValueInst>(I) ||
                   isa<InsertValueInst>(I) || isa<TruncInst>(I) ||
                   isa<ZExtInst>(I) || isa<SExtInst>(I) || isa<FreezeInst>(I))) return false;
      if (!numeric(I.getType())) return false;
      for (const Use &U : I.operands()) {
        const Value *V = U.get();
        if (isa<BasicBlock>(V)) continue;
        if (const auto *G = dyn_cast<GlobalValue>(V)) {
          if (!isa<CallBase>(I) || G != cast<CallBase>(I).getCalledFunction()) return false;
          continue;
        }
        if (isa<ConstantExpr>(V) || isa<BlockAddress>(V) ||
            (!numeric(V->getType()) && !V->getType()->isLabelTy())) return false;
      }
    }
  }
  return true;
}
std::string functionText(const Function &F) {
  std::string S; raw_string_ostream OS(S); F.print(OS, nullptr, true); return S;
}
std::string worldText(const Module &M, const Function *Subject) {
  ValueToValueMapTy Map;
  auto World = CloneModule(M, Map, [](const GlobalValue *G) { return !isa<Function>(G); });
  // CloneModule canonicalizes stripped definitions to external declarations.
  // Undo that canonicalization, and explicitly bind header fields/metadata it
  // intentionally omits when not cloning a body. This projection is a key,
  // never submitted as an executable LLVM module.
  for (const Function &F : M) {
    auto *D = cast<Function>(Map[&F]);
    D->setLinkage(F.getLinkage());
    if (F.hasPersonalityFn()) D->setPersonalityFn(MapValue(F.getPersonalityFn(), Map));
    if (F.hasComdat()) {
      auto *C = World->getOrInsertComdat(F.getComdat()->getName());
      C->setSelectionKind(F.getComdat()->getSelectionKind()); D->setComdat(C);
    }
  }
  // Keep all function headers, including the subject. Its body is separately
  // bound and its header participates in module-level analysis/configuration.
  std::string S; raw_string_ostream OS(S); World->print(OS, nullptr, true);
  for (const Function &F : M) {
    SmallVector<std::pair<unsigned, MDNode *>, 4> MD;
    F.getAllMetadata(MD);
    for (const auto &E : MD) {
      OS << "\n; original header metadata " << F.getName() << ':' << E.first << '\n';
      E.second->print(OS, &M);
    }
  }
  if (Subject) {
    // Passes can inspect direct users. A changed caller is an actual dependency
    // even when this closed arithmetic function has no outbound user calls.
    std::set<const Function *> Callers;
    for (const User *U : Subject->users())
      if (const auto *I = dyn_cast<Instruction>(U)) Callers.insert(I->getFunction());
    for (const Function &F : M)
      if (Callers.count(&F)) { OS << "\n; incoming caller\n"; F.print(OS, nullptr, true); }
  }
  return S;
}
namespace {
struct Symbols {
  using Set = const ZilLLVMFunctionProductHooks *(*)(const ZilLLVMFunctionProductHooks *);
  using Visit = void (*)(void *, void *, void (*)(void *, const void *), void *);
  using Has = int (*)(void *, const void *);
  using Require = void (*)(void *, void *, const void *);
  Set set = nullptr; Visit visit = nullptr; Has has = nullptr; Require require = nullptr;
  bool available = false;
  Symbols() {
    auto Cap = [](const char *N) { auto F = reinterpret_cast<unsigned (*)()>(dlsym(RTLD_DEFAULT, N)); return F && F() == 1; };
    if (!Cap("ZilLLVMMachineProductCapability") || !Cap("ZilLLVMFunctionStageCapability") ||
        !Cap("ZilLLVMLegacyLifecycleCapability")) return;
    set = reinterpret_cast<Set>(dlsym(RTLD_DEFAULT, "ZilLLVMSetFunctionProductHooks"));
    visit = reinterpret_cast<Visit>(dlsym(RTLD_DEFAULT, "ZilLLVMVisitFunctionAnalysisKeys"));
    has = reinterpret_cast<Has>(dlsym(RTLD_DEFAULT, "ZilLLVMHasFunctionAnalysisKey"));
    require = reinterpret_cast<Require>(dlsym(RTLD_DEFAULT, "ZilLLVMRequireFunctionAnalysisKey"));
    available = set && visit && has && require;
  }
};
Symbols &symbols() { static Symbols S; return S; }
using Keys = std::vector<const void *>;
Keys analysisKeys(FunctionAnalysisManager &AM, Function &F) {
  Keys K; symbols().visit(&AM, &F, [](void *P, const void *K) {
    static_cast<Keys *>(P)->push_back(K);
  }, &K); return K;
}
struct Stage {
  std::string Input, World, Path, Pipeline, Output;
  Keys EntryKeys, ExitKeys;
  PreservedAnalyses PA = PreservedAnalyses::none();
};
struct Products {
  std::string Config;
  std::map<std::string, std::shared_ptr<const Stage>> Stages;
  std::shared_ptr<const MachineProducts> Machine;
};
struct Pending { std::string Slot; std::shared_ptr<Stage> Product; };
struct Request {
  Module &M; TargetMachine &TM; std::string Config;
  const Products *Prior;
  Products Next;
  ZilNativeProductStats Stats{};
  bool Failed = false;
  unsigned Phase = 0;
  std::vector<std::pair<unsigned, unsigned>> Path;
  std::map<std::string, unsigned> Occurrences;
  ZilLLVMFunctionProductHooks Hooks{};
  const ZilLLVMFunctionProductHooks *Previous = nullptr;
  std::unique_ptr<MachineRequest, void (*)(MachineRequest *)> Machine{nullptr, nullptr};
  Request(Module &M, TargetMachine &TM, std::string C, const Products *P)
      : M(M), TM(TM), Config(std::move(C)), Prior(P) { Next.Config = Config; }
};
std::string pipeline(FunctionPassManager &PM) {
  std::string S; raw_string_ostream OS(S);
  PM.printPipeline(OS, [](StringRef N) { return N; }); return S;
}
std::string path(Request &R, Function &F) {
  std::string P; raw_string_ostream OS(P);
  OS << F.getName().size() << ':' << F.getName();
  for (auto E : R.Path) OS << '/' << E.first << ':' << E.second;
  OS.flush(); auto &N = R.Occurrences[P]; return P + "#" + std::to_string(N++);
}
std::string snapshot(Function &F) {
  Module Out("zil.function.product", F.getContext());
  Out.setTargetTriple(F.getParent()->getTargetTriple()); Out.setDataLayout(F.getParent()->getDataLayout());
  ValueToValueMapTy Map;
  Function *NF = Function::Create(F.getFunctionType(), F.getLinkage(), F.getAddressSpace(), F.getName(), &Out);
  NF->copyAttributesFrom(&F); Map[&F] = NF;
  auto NA = NF->arg_begin();
  for (const Argument &A : F.args()) { NA->setName(A.getName()); Map[&A] = &*NA++; }
  for (BasicBlock &B : F) for (Instruction &I : B) if (auto *C = dyn_cast<CallBase>(&I)) {
    Function *Callee = C->getCalledFunction();
    if (!Map.count(Callee)) {
      Function *D = Function::Create(Callee->getFunctionType(), Callee->getLinkage(), Callee->getAddressSpace(), Callee->getName(), &Out);
      D->copyAttributesFrom(Callee); Map[Callee] = D;
    }
  }
  SmallVector<ReturnInst *, 8> Returns;
  CloneFunctionInto(NF, &F, Map, CloneFunctionChangeType::DifferentModule, Returns);
  if (verifyModule(Out, nullptr)) return {};
  std::string S; raw_string_ostream OS(S); WriteBitcodeToFile(Out, OS, true); return S;
}
bool replay(Request &R, Function &F, FunctionAnalysisManager &AM, const Stage &P) {
  auto Buffer = MemoryBuffer::getMemBufferCopy(P.Output, "zil.function.product");
  auto Parsed = parseBitcodeFile(Buffer->getMemBufferRef(), F.getContext());
  if (!Parsed) { consumeError(Parsed.takeError()); return false; }
  std::unique_ptr<Module> Source = std::move(*Parsed);
  Function *SF = Source->getFunction(F.getName());
  if (!SF || !eligibleFunction(*SF) || SF->getFunctionType() != F.getFunctionType() ||
      verifyModule(*Source, nullptr)) return false;
  ValueToValueMapTy Map; Map[SF] = &F;
  auto A = F.arg_begin();
  for (const Argument &SA : SF->args()) Map[&SA] = &*A++;
  for (Function &D : *Source) if (&D != SF) {
    Function *Current = R.M.getFunction(D.getName());
    if (!D.isDeclaration() || !Current || Current->getFunctionType() != D.getFunctionType() ||
        Current->getAttributes() != D.getAttributes()) return false;
    Map[&D] = Current;
  }
  for (const void *K : P.ExitKeys) if (!symbols().has(&AM, K)) return false;
  // invalidate(none) deliberately does not suffice: some immutable analyses
  // self-preserve. Destroy the complete old function cache before body replace.
  AM.clear(F, F.getName());
  F.deleteBody(); F.setLinkage(SF->getLinkage()); F.copyAttributesFrom(SF);
  A = F.arg_begin(); for (const Argument &SA : SF->args()) (A++)->setName(SA.getName());
  SmallVector<ReturnInst *, 8> Returns;
  CloneFunctionInto(&F, SF, Map, CloneFunctionChangeType::DifferentModule, Returns);
  if (verifyFunction(F, nullptr)) return false;
  for (const void *K : P.ExitKeys) { symbols().require(&AM, &F, K); ++R.Stats.analysis_reconstructions; }
  if (analysisKeys(AM, F) != P.ExitKeys) return false;
  R.Stats.ir_replay_bytes += P.Output.size(); return true;
}
int beforeStage(void *Context, void *PM, void *IR, void *Manager, void *, void *PA, void **Token) {
  auto &R = *static_cast<Request *>(Context); auto &F = *static_cast<Function *>(IR);
  auto &AM = *static_cast<FunctionAnalysisManager *>(Manager);
  *Token = nullptr;
  if (F.getParent() != &R.M || R.Phase != 1 || R.Failed || !eligibleFunction(F)) return 0;
  auto P = std::make_shared<Stage>();
  P->Path = path(R, F); P->Pipeline = pipeline(*static_cast<FunctionPassManager *>(PM));
  P->Input = functionText(F); P->World = worldText(R.M, &F); P->EntryKeys = analysisKeys(AM, F);
  R.Stats.ir_key_bytes += P->Input.size() + P->World.size() + P->Pipeline.size() + R.Config.size();
  std::string Slot = P->Path;
  if (R.Prior && R.Prior->Config == R.Config) {
    auto It = R.Prior->Stages.find(Slot);
    if (It != R.Prior->Stages.end()) {
      const auto &Old = *It->second;
      if (Old.Input == P->Input && Old.World == P->World && Old.Pipeline == P->Pipeline && Old.EntryKeys == P->EntryKeys) {
        if (!replay(R, F, AM, Old)) { R.Failed = true; ++R.Stats.failed_replays; return 0; }
        *static_cast<PreservedAnalyses *>(PA) = Old.PA;
        R.Next.Stages[Slot] = It->second; ++R.Stats.optimizer_function_stage_reuses; return 1;
      }
    }
  }
  ++R.Stats.optimizer_function_stage_executions;
  *Token = new Pending{std::move(Slot), std::move(P)}; return 0;
}
void afterStage(void *Context, void *, void *IR, void *Manager, const void *PA, void *Token) {
  std::unique_ptr<Pending> P(static_cast<Pending *>(Token));
  if (!P) return;
  auto &R = *static_cast<Request *>(Context); auto &F = *static_cast<Function *>(IR);
  if (R.Failed || !eligibleFunction(F) || worldText(R.M, &F) != P->Product->World) return;
  P->Product->Output = snapshot(F);
  if (P->Product->Output.empty()) return;
  P->Product->PA = *static_cast<const PreservedAnalyses *>(PA);
  P->Product->ExitKeys = analysisKeys(*static_cast<FunctionAnalysisManager *>(Manager), F);
  R.Stats.ir_snapshot_bytes += P->Product->Output.size();
  R.Next.Stages[P->Slot] = std::move(P->Product);
}
void enterPass(void *C, unsigned Kind, unsigned Index) {
  auto &R = *static_cast<Request *>(C); R.Path.emplace_back(Kind, Index);
  if (Kind == 0) ++R.Stats.optimizer_module_pass_executions;
  if (Kind == 2) ++R.Stats.optimizer_cgscc_pass_executions;
}
void leavePass(void *C) { auto &P = static_cast<Request *>(C)->Path; if (!P.empty()) P.pop_back(); }
int beforeMachineCallback(void *C, void *P, void *F, void *MMI) {
  auto &R = *static_cast<Request *>(C);
  return static_cast<Function *>(F)->getParent() == &R.M && R.Machine ? beforeMachine(*R.Machine, *static_cast<Pass *>(P), *static_cast<Function *>(F), *static_cast<MachineModuleInfo *>(MMI)) : 0;
}
int skippedCallback(void *C, void *P, void *F) {
  auto &R = *static_cast<Request *>(C);
  return static_cast<Function *>(F)->getParent() == &R.M && R.Machine ? machineWasSkipped(*R.Machine, *static_cast<Pass *>(P), *static_cast<Function *>(F)) : 0;
}
void copyStats(Request &R, ZilNativeProductStats *Out) { if (Out) *Out = R.Stats; }
}
}
extern "C" void *ZilNativeProductsCreateRequest(void *M, void *TM, const void *Prior, const char *C, size_t N) {
  using namespace zil;
  if (!M || !TM || !symbols().available || (!C && N)) return nullptr;
  auto R = std::make_unique<Request>(*static_cast<Module *>(M), *static_cast<TargetMachine *>(TM), std::string(C ? C : "", N), static_cast<const Products *>(Prior));
  R->Stats.available = 1;
  R->Machine = createMachine(R->M, R->TM, R->Config,
      R->Prior && R->Prior->Config == R->Config ? R->Prior->Machine : nullptr, R->Stats, R->Failed);
  R->Hooks = {1, R.get(), beforeMachineCallback, enterPass, leavePass, beforeStage, afterStage, skippedCallback};
  R->Previous = symbols().set(&R->Hooks); return R.release();
}
extern "C" void ZilNativeProductsSetPhase(void *P, unsigned Phase) { if (P) static_cast<zil::Request *>(P)->Phase = Phase; }
extern "C" int ZilNativeProductsFailed(const void *P) { return P && static_cast<const zil::Request *>(P)->Failed; }
extern "C" void *ZilNativeProductsFinish(void *P, ZilNativeProductStats *S) {
  using namespace zil; std::unique_ptr<Request> R(static_cast<Request *>(P)); if (!R) return nullptr;
  symbols().set(R->Previous);
  if (R->Failed) { copyStats(*R, S); return nullptr; }
  if (R->Machine) R->Next.Machine = finishMachine(*R->Machine);
  for (const auto &E : R->Next.Stages) R->Stats.retained_bytes += E.first.size() + E.second->Input.size() + E.second->World.size() + E.second->Output.size() + E.second->Pipeline.size();
  copyStats(*R, S); return new Products(std::move(R->Next));
}
extern "C" void ZilNativeProductsAbort(void *P, ZilNativeProductStats *S) {
  using namespace zil; std::unique_ptr<Request> R(static_cast<Request *>(P)); if (!R) return;
  symbols().set(R->Previous); copyStats(*R, S);
}
extern "C" void ZilNativeProductsDispose(void *P) { delete static_cast<zil::Products *>(P); }

extern "C" void *ZilNativeProductsSuspend(void) {
  auto &S = zil::symbols();
  return S.available ? const_cast<ZilLLVMFunctionProductHooks *>(S.set(nullptr)) : nullptr;
}
extern "C" void ZilNativeProductsResume(void *P) {
  auto &S = zil::symbols();
  if (S.available) S.set(static_cast<const ZilLLVMFunctionProductHooks *>(P));
}
