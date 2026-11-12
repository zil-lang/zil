// Fixed LLVM 23.1.4 resident optimization products. The original pass managers
// construct and run their original pipelines; this adapter never reparses one.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "work_products.h"
#include "zil_llvm_work_products_hooks.h"
#include "machine_products.h"
#include "analysis_facts.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/Bitcode/BitcodeReader.h"
#include "llvm/Bitcode/BitcodeWriter.h"
#include "llvm/IR/CFG.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/IntrinsicInst.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/PassManager.h"
#include "llvm/IR/PassInstrumentation.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/Error.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/Target/TargetMachine.h"
#include "llvm/Transforms/Utils/Cloning.h"
#include "llvm/Transforms/Utils/PromoteMemToReg.h"
#include <algorithm>
#include <cstdio>
#include <dlfcn.h>
#include <map>
#include <limits>
#include <memory>
#include <set>
#include <string>
#include <utility>
#include <vector>
using namespace llvm;
namespace zil {
namespace {
thread_local bool AnalysisDiagnostics = false;
bool admissionFailure(const Function &F, const Instruction *I, const char *Reason) {
  if (AnalysisDiagnostics) {
    std::fprintf(stderr, "ZIL FPM admission %.*s: %s", static_cast<int>(F.getName().size()), F.getName().data(), Reason);
    if (I) {
      std::fprintf(stderr, " opcode=%s", I->getOpcodeName());
      if (const auto *C = dyn_cast<CallBase>(I)) if (const Function *Target = C->getCalledFunction())
        std::fprintf(stderr, " callee=%.*s", static_cast<int>(Target->getName().size()), Target->getName().data());
    }
    std::fprintf(stderr, "\n");
  }
  return false;
}
struct NameState { int present = -1; uint32_t counter = 0; int maximum = 0; };
using ReadNameState = int (*)(const void *, uint32_t *, int *);
using RestoreNameState = int (*)(void *, int, uint32_t, int);
ReadNameState readNameStateAPI() {
  static auto Read = reinterpret_cast<ReadNameState>(dlsym(RTLD_DEFAULT, "ZilLLVMFunctionSymbolState")); return Read;
}
NameState nameState(const Function &F) {
  NameState S; auto Read = readNameStateAPI();
  if (Read) S.present = Read(&F, &S.counter, &S.maximum);
  return S;
}
// Function::print(true) in the pinned LLVM does not initialize its use-list
// ordering map. Bind that observable state explicitly using current owned IDs.
// Debug records are a separate instruction stream in LLVM 23. A metadata
// attachment traversal cannot represent them; unsupported callers must reject
// the same way as subjects, rather than contribute an incomplete projection.
bool projectionAttachmentsSupported(const Function &F) {
  SmallVector<std::pair<unsigned, MDNode *>, 4> Attachments;
  std::set<const Metadata *> Seen;
  std::vector<const Metadata *> Pending;
  auto Add = [&](const auto &V) {
    Attachments.clear(); V.getAllMetadata(Attachments);
    for (const auto &E : Attachments) Pending.push_back(E.second);
  };
  Add(F);
  for (const BasicBlock &B : F) {
    // The pinned accessor performs only Context->getTrailingDbgRecords(this),
    // despite lacking a const overload. Do not materialize a marker here.
    if (const_cast<BasicBlock &>(B).getTrailingDbgRecords()) return false;
    for (const Instruction &I : B) {
      if (I.hasDbgRecords()) return false;
      Add(I);
      // Metadata operands (debug intrinsics etc.) are outside this attachment
      // schema. In particular LocalAsMetadata is not an ordinary Value Use.
      for (const Use &U : I.operands()) if (isa<MetadataAsValue>(U.get())) return false;
    }
  }
  while (!Pending.empty()) {
    const Metadata *MD = Pending.back(); Pending.pop_back();
    if (!MD || !Seen.insert(MD).second) continue;
    if (const auto *N = dyn_cast<MDNode>(MD)) {
      for (const MDOperand &O : N->operands()) Pending.push_back(O.get());
    } else if (const auto *C = dyn_cast<ConstantAsMetadata>(MD)) {
      // Do not hide global/body or block-address dependencies in metadata.
      if (!isa<ConstantInt>(C->getValue()) && !isa<ConstantFP>(C->getValue())) return false;
    } else if (!isa<MDString>(MD)) return false;
  }
  return true;
}
bool ownedUseSequence(const Function &F, std::vector<uint32_t> *Out) {
  if (!projectionAttachmentsSupported(F)) return false;
  DenseMap<const Value *, uint32_t> IDs;
  std::vector<const Value *> Values;
  auto Add = [&](const Value *V) {
    if (Values.size() >= std::numeric_limits<uint32_t>::max()) return false;
    IDs[V] = static_cast<uint32_t>(Values.size()); Values.push_back(V); return true;
  };
  for (const Argument &A : F.args()) if (!Add(&A)) return false;
  for (const BasicBlock &B : F) {
    // LLVM 23 BlockAddress stores its BB as a private pointer with zero Use
    // operands. It is observable hidden state outside this owned-use schema.
    if (B.hasAddressTaken()) return false;
    if (!Add(&B)) return false;
    for (const Instruction &I : B) if (!Add(&I)) return false;
  }
  if (Out) { Out->clear(); Out->push_back(static_cast<uint32_t>(Values.size())); }
  for (const Value *V : Values) {
    uint64_t Count = 0;
    size_t CountAt = Out ? Out->size() : 0;
    if (Out) Out->push_back(0);
    for (const Use &U : V->uses()) {
      auto It = IDs.find(U.getUser());
      if (It == IDs.end() || !isa<Instruction>(U.getUser()) ||
          ++Count > std::numeric_limits<uint32_t>::max()) return false;
      if (Out) { Out->push_back(It->second); Out->push_back(U.getOperandNo()); }
    }
    if (Out) (*Out)[CountAt] = static_cast<uint32_t>(Count);
  }
  return true;
}
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
  SmallVector<Type *, 4> Overloads;
  return Intrinsic::isSignatureValid(F->getIntrinsicID(), F->getFunctionType(), Overloads);
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
      F.hasPrefixData() || F.hasPrologueData() || F.hasComdat() || !acyclic(F)) return admissionFailure(F, nullptr, "function-signature-or-CFG");
  for (const Argument &A : F.args()) if (!A.getType()->isIntegerTy()) return admissionFailure(F, nullptr, "noninteger-argument");
  SmallVector<std::pair<unsigned, MDNode *>, 4> Metadata;
  F.getAllMetadata(Metadata);
  if (!Metadata.empty()) return admissionFailure(F, nullptr, "function-metadata");
  for (const User *U : F.users()) {
    const auto *C = dyn_cast<CallBase>(U);
    if (!C || C->getCalledOperand() != &F) return admissionFailure(F, nullptr, "noncall-function-use");
    const Function *Caller = C->getFunction();
    if (!Caller || Caller->getParent() != F.getParent() || nameState(*Caller).present < 0 ||
        !ownedUseSequence(*Caller, nullptr))
      return admissionFailure(F, nullptr, "unbound-incoming-caller");
  }
  std::set<const AllocaInst *> LocalSlots;
  for (const BasicBlock &B : F) for (const Instruction &I : B)
    if (const auto *A = dyn_cast<AllocaInst>(&I)) {
      const auto *Count = dyn_cast<ConstantInt>(A->getArraySize());
      if (&B != &F.getEntryBlock() || !A->isStaticAlloca() ||
          !A->getAllocatedType()->isIntegerTy() || !Count || !Count->isOne() ||
          !isAllocaPromotable(A)) return admissionFailure(F, A, "nonpromotable-local-slot");
      // A deliberately strict subset of LLVM's promotability predicate: the
      // only pointer identities are these nonescaping local slots. No lifetime
      // call, cast, GEP, address comparison or interprocedural pointer fact.
      for (const User *U : A->users()) {
        if (const auto *L = dyn_cast<LoadInst>(U)) {
          if (!L->isSimple() || L->getPointerOperand() != A ||
              !L->getType()->isIntegerTy()) return admissionFailure(F, L, "local-load-domain");
        } else if (const auto *S = dyn_cast<StoreInst>(U)) {
          if (!S->isSimple() || S->getPointerOperand() != A ||
              !S->getValueOperand()->getType()->isIntegerTy()) return admissionFailure(F, S, "local-store-domain");
        } else return admissionFailure(F, A, "escaping-local-slot");
      }
      LocalSlots.insert(A);
    }
  for (const BasicBlock &B : F) {
    if (B.hasAddressTaken()) return admissionFailure(F, nullptr, "block-address-taken");
    for (const Instruction &I : B) {
      Metadata.clear(); I.getAllMetadata(Metadata);
      if (!Metadata.empty() || I.getDebugLoc()) return admissionFailure(F, &I, "instruction-metadata");
      if (const auto *C = dyn_cast<CallBase>(&I)) {
        if (!approvedIntrinsic(*C)) return admissionFailure(F, &I, "call-outside-closed-domain");
      } else if (const auto *A = dyn_cast<AllocaInst>(&I)) {
        if (!LocalSlots.count(A)) return admissionFailure(F, &I, "unknown-local-slot");
      } else if (const auto *L = dyn_cast<LoadInst>(&I)) {
        if (!L->isSimple() || !LocalSlots.count(dyn_cast<AllocaInst>(L->getPointerOperand())))
          return admissionFailure(F, &I, "nonlocal-load");
      } else if (const auto *S = dyn_cast<StoreInst>(&I)) {
        if (!S->isSimple() || !LocalSlots.count(dyn_cast<AllocaInst>(S->getPointerOperand())))
          return admissionFailure(F, &I, "nonlocal-store");
      } else if (!(isa<BinaryOperator>(I) || isa<UnaryOperator>(I) ||
                   isa<ICmpInst>(I) || isa<PHINode>(I) || isa<SelectInst>(I) ||
                   isa<CondBrInst>(I) || isa<UncondBrInst>(I) || isa<SwitchInst>(I) || isa<ReturnInst>(I) ||
                   isa<UnreachableInst>(I) || isa<ExtractValueInst>(I) ||
                   isa<InsertValueInst>(I) || isa<TruncInst>(I) ||
                   isa<ZExtInst>(I) || isa<SExtInst>(I) || isa<FreezeInst>(I))) return admissionFailure(F, &I, "instruction-outside-closed-domain");
      if (!isa<AllocaInst>(I) && !numeric(I.getType())) return admissionFailure(F, &I, "instruction-type");
      for (const Use &U : I.operands()) {
        const Value *V = U.get();
        if (isa<BasicBlock>(V)) continue;
        if (const auto *A = dyn_cast<AllocaInst>(V)) if (LocalSlots.count(A)) continue;
        if (const auto *G = dyn_cast<GlobalValue>(V)) {
          if (!isa<CallBase>(I) || G != cast<CallBase>(I).getCalledFunction()) return admissionFailure(F, &I, "global-operand");
          continue;
        }
        if (isa<ConstantExpr>(V) || isa<BlockAddress>(V) ||
            (!numeric(V->getType()) && !V->getType()->isLabelTy())) return admissionFailure(F, &I, "operand-outside-closed-domain");
      }
    }
  }
  if (nameState(F).present < 0) return admissionFailure(F, nullptr, "unbound-value-name-map");
  if (!ownedUseSequence(F, nullptr)) return admissionFailure(F, nullptr, "unbound-owned-use");
  return true;
}
std::string functionText(const Function &F) {
  std::string S; raw_string_ostream OS(S); F.print(OS, nullptr, true);
  OS << "\n; block-number-state " << F.getMaxBlockNumber() << ':' << F.getBlockNumberEpoch();
  for (const BasicBlock &B : F) OS << ':' << B.getNumber();
  NameState Names = nameState(F);
  OS << "\n; value-name-state " << Names.present << ':' << Names.maximum << ':' << Names.counter;
  std::vector<uint32_t> Uses;
  if (!ownedUseSequence(F, &Uses)) return {};
  // Function::print writes attribute-group and metadata references without
  // their definitions. Bind the actual values, including nested MD payloads.
  OS << "\n; complete-function-attributes "; F.getAttributes().print(OS);
  SmallVector<std::pair<unsigned, MDNode *>, 4> Attachments;
  F.getAllMetadata(Attachments);
  for (const auto &E : Attachments) {
    OS << "\n; complete-function-metadata " << E.first << '\n';
    E.second->printTree(OS, F.getParent());
  }
  uint64_t Ordinal = 0;
  for (const BasicBlock &B : F) for (const Instruction &I : B) {
    if (const auto *C = dyn_cast<CallBase>(&I)) if (!C->getAttributes().isEmpty()) {
      OS << "\n; complete-call-attributes " << Ordinal << ' ';
      C->getAttributes().print(OS);
    }
    Attachments.clear(); I.getAllMetadata(Attachments);
    for (const auto &E : Attachments) {
      OS << "\n; complete-instruction-metadata " << Ordinal << ':' << E.first << '\n';
      E.second->printTree(OS, F.getParent());
    }
    ++Ordinal;
  }
  OS << "\n; owned-use-sequence-v1\n";
  // A private resident byte projection, never LLVM source or a disk format.
  OS.write(reinterpret_cast<const char *>(Uses.data()), Uses.size() * sizeof(uint32_t));
  OS << '\n'; return S;
}
std::string declarationText(const Function &F) {
  std::string S; raw_string_ostream OS(S);
  F.print(OS, nullptr, false);
  OS << "\n; complete-attribute-values "; F.getAttributes().print(OS);
  SmallVector<std::pair<unsigned, MDNode *>, 4> Metadata;
  F.getAllMetadata(Metadata);
  for (const auto &E : Metadata) { OS << "\n; metadata " << E.first << '\n'; E.second->printTree(OS, F.getParent()); }
  return S;
}
std::string worldText(const Module &M, const Function *Subject, ZilNativeProductStats *Stats) {
  using PrintHeaders = int (*)(const void *, void *);
  static auto Print = reinterpret_cast<PrintHeaders>(dlsym(RTLD_DEFAULT, "ZilLLVMPrintModuleHeaders"));
  std::string S; raw_string_ostream OS(S);
  if (Print && Print(&M, &OS)) {
    if (Stats) ++Stats->world_projection_direct;
  } else {
    // An unsupported header owner or missing SDK renderer must retain ALL
    // current bodies, not strip away the very owner the fast path rejected.
    if (Stats) ++Stats->world_projection_fallback;
    M.print(OS, nullptr, true);
  }
  for (const Function &F : M) {
    SmallVector<std::pair<unsigned, MDNode *>, 4> MD;
    OS << "\n; original body-presence " << F.getName().size() << ':' << F.getName() << ':' << F.isDeclaration() << '\n';
    F.getAllMetadata(MD);
    for (const auto &E : MD) {
      OS << "\n; original header metadata " << F.getName() << ':' << E.first << '\n';
      E.second->printTree(OS, &M);
    }
  }
  if (Subject) {
    // Passes can inspect direct users. A changed caller is an actual dependency
    // even when this closed arithmetic function has no outbound user calls.
    std::set<const Function *> Callers;
    for (const User *U : Subject->users())
      if (const auto *I = dyn_cast<Instruction>(U)) Callers.insert(I->getFunction());
    for (const Function &F : M)
      if (Callers.count(&F)) { OS << "\n; incoming caller\n" << functionText(F); }
    // User iteration order is observable to transformations. Bind the actual
    // outside-body use list as stable current IR positions, without retaining
    // historical Instruction/Use pointers.
    OS << "\n; ordered incoming uses\n";
    for (const Use &U : Subject->uses()) {
      const auto *I = dyn_cast<Instruction>(U.getUser());
      if (!I) { U.getUser()->print(OS); OS << '\n'; continue; }
      const Function *Caller = I->getFunction();
      size_t Block = 0, Inst = 0;
      for (const BasicBlock &B : *Caller) {
        if (&B == I->getParent()) {
          for (const Instruction &J : B) { if (&J == I) break; ++Inst; }
          break;
        }
        ++Block;
      }
      OS << Caller->getName().size() << ':' << Caller->getName() << ':'
         << Block << ':' << Inst << ':' << U.getOperandNo() << '\n';
    }
  }
  return S;
}
namespace {
struct Symbols {
  using Set = const ZilLLVMFunctionProductHooks *(*)(const ZilLLVMFunctionProductHooks *);
  using Visit = void (*)(void *, void *, void (*)(void *, const void *), void *);
  using Has = int (*)(void *, const void *);
  using Require = void (*)(void *, void *, const void *);
  using Name = const char *(*)(void *, const void *, uint64_t *);
  using RestoreNumbers = int (*)(void *, unsigned, unsigned, const unsigned *, uint64_t);
  Set set = nullptr; Visit visit = nullptr; Has has = nullptr; Require require = nullptr;
  Name name = nullptr; RestoreNumbers restoreNumbers = nullptr;
  RestoreNameState restoreNames = nullptr;
  bool available = false;
  Symbols() {
    auto Cap = [](const char *N) { auto F = reinterpret_cast<unsigned (*)()>(dlsym(RTLD_DEFAULT, N)); return F && F() == 1; };
    if (!Cap("ZilLLVMMachineProductCapability") || !Cap("ZilLLVMFunctionStageCapability") ||
        !Cap("ZilLLVMLegacyLifecycleCapability")) return;
    auto HookABI = reinterpret_cast<unsigned (*)()>(dlsym(RTLD_DEFAULT, "ZilLLVMFunctionProductHookABI"));
    if (!HookABI || HookABI() != 2) return;
    set = reinterpret_cast<Set>(dlsym(RTLD_DEFAULT, "ZilLLVMSetFunctionProductHooks"));
    visit = reinterpret_cast<Visit>(dlsym(RTLD_DEFAULT, "ZilLLVMVisitFunctionAnalysisKeys"));
    has = reinterpret_cast<Has>(dlsym(RTLD_DEFAULT, "ZilLLVMHasFunctionAnalysisKey"));
    require = reinterpret_cast<Require>(dlsym(RTLD_DEFAULT, "ZilLLVMRequireFunctionAnalysisKey"));
    name = reinterpret_cast<Name>(dlsym(RTLD_DEFAULT, "ZilLLVMFunctionAnalysisName"));
    restoreNumbers = reinterpret_cast<RestoreNumbers>(dlsym(RTLD_DEFAULT, "ZilLLVMRestoreFunctionBlockNumbers"));
    restoreNames = reinterpret_cast<RestoreNameState>(dlsym(RTLD_DEFAULT, "ZilLLVMRestoreFunctionSymbolState"));
    available = set && visit && has && require && restoreNumbers && readNameStateAPI() && restoreNames;
  }
};
Symbols &symbols() { static Symbols S; return S; }
using Keys = std::vector<const void *>;
Keys analysisKeys(FunctionAnalysisManager &AM, Function &F) {
  Keys K; symbols().visit(&AM, &F, [](void *P, const void *K) {
    static_cast<Keys *>(P)->push_back(K);
  }, &K); return K;
}
void describeAnalyses(FunctionAnalysisManager &AM, Function &F, const char *Point, const Keys &K) {
  if (!AnalysisDiagnostics) return;
  std::fprintf(stderr, "ZIL FPM %.*s %s:", static_cast<int>(F.getName().size()), F.getName().data(), Point);
  for (const void *ID : K) {
    uint64_t Length = 0;
    const char *Name = symbols().name ? symbols().name(&AM, ID, &Length) : nullptr;
    if (Name) std::fprintf(stderr, " %.*s;", static_cast<int>(Length), Name);
    else std::fprintf(stderr, " <name-unavailable>;");
  }
  std::fprintf(stderr, "\n");
}
struct Stage {
  std::string Input, World, Path, Pipeline, Output, OutputText, EntryFacts, ExitFacts;
  Keys EntryKeys, ExitKeys;
  unsigned OutputNextBlock = 0, OutputBlockEpoch = 0;
  AnalysisReplayState OutputAnalysisReplay;
  NameState OutputNames;
  std::map<std::string, std::string> OriginalDeclarations;
  std::vector<unsigned> OutputBlockNumbers;
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
  AnalysisFactsMemo AnalysisMemo;
  bool Failed = false;
  unsigned Phase = 0;
  size_t ProductBytes = 0;
  std::map<Pending *, std::unique_ptr<Pending>> PendingTokens;
  std::vector<std::pair<unsigned, unsigned>> Path;
  std::map<std::string, unsigned> Occurrences;
  ZilLLVMFunctionProductHooks Hooks{};
  const ZilLLVMFunctionProductHooks *Previous = nullptr;
  std::unique_ptr<MachineRequest, void (*)(MachineRequest *)> Machine{nullptr, nullptr};
  Request(Module &M, TargetMachine &TM, std::string C, const Products *P)
      : M(M), TM(TM), Config(std::move(C)), Prior(P) { Next.Config = Config; }
};
Pending *newPending(Request &R, std::string Slot = {}, std::shared_ptr<Stage> Product = {}) {
  auto P = std::make_unique<Pending>(); P->Slot = std::move(Slot); P->Product = std::move(Product);
  Pending *Raw = P.get(); R.PendingTokens.emplace(Raw, std::move(P)); return Raw;
}
// Optional resident products have a bounded encoded payload and slot count.
// Reaching either cap only discards a candidate; it never changes compilation,
// verification, optimization policy or the compiler's existing work budget.
constexpr size_t MaxStageProductBytes = 64 * 1024 * 1024;
constexpr size_t MaxStageProductSlots = 4096;
void retainStage(Request &R, const std::string &Slot, std::shared_ptr<const Stage> P) {
  if (R.Next.Stages.size() >= MaxStageProductSlots) return;
  size_t Bytes = sizeof(Stage);
  for (size_t N : {Slot.size(), P->Input.size(), P->World.size(), P->Path.size(),
                   P->Pipeline.size(), P->Output.size(), P->OutputText.size(), P->EntryFacts.size(), P->ExitFacts.size(),
                   P->EntryKeys.size() * sizeof(void *), P->ExitKeys.size() * sizeof(void *),
                   P->OutputBlockNumbers.size() * sizeof(unsigned)}) {
    if (N > MaxStageProductBytes - Bytes) return;
    Bytes += N;
  }
  for (const auto &D : P->OriginalDeclarations) {
    for (size_t N : {D.first.size(), D.second.size()}) {
      if (N > MaxStageProductBytes - Bytes) return;
      Bytes += N;
    }
  }
  if (Bytes > MaxStageProductBytes - R.ProductBytes) return;
  R.ProductBytes += Bytes;
  R.Next.Stages.emplace(Slot, std::move(P));
}
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
// CloneFunctionInto preserves values/CFG but does not promise use-list order.
// LLVM analyses and scheduling can observe that order. Transport only the
// function-owned values; constants and symbol users remain current owners.
bool preserveLocalUseOrder(const Function &Source, ValueToValueMapTy &Map) {
  auto Restore = [&](const Value *V) {
    Value *New = Map.lookup(V);
    if (!New) return false;
    std::map<const Use *, size_t> Ranks;
    for (const Use &U : V->uses()) {
      Value *MappedUser = Map.lookup(U.getUser());
      auto *NewUser = dyn_cast_or_null<User>(MappedUser);
      if (!NewUser || U.getOperandNo() >= NewUser->getNumOperands()) return false;
      const Use *NewUse = &NewUser->getOperandUse(U.getOperandNo());
      if (NewUse->get() != New) return false;
      Ranks.emplace(NewUse, Ranks.size());
    }
    if (Ranks.size() != New->getNumUses()) return false;
    New->sortUseList([&](const Use &A, const Use &B) { return Ranks.at(&A) < Ranks.at(&B); });
    return true;
  };
  for (const Argument &A : Source.args()) if (!Restore(&A)) return false;
  for (const BasicBlock &B : Source) {
    if (!Restore(&B)) return false;
    for (const Instruction &I : B) if (!Restore(&I)) return false;
  }
  return true;
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
  // DifferentModule creates llvm.dbg.cu even when this metadata-free function
  // contributes no compile units. Do not serialize an invented empty root.
  if (auto *CU = Out.getNamedMetadata("llvm.dbg.cu")) {
    if (CU->getNumOperands()) return {};
    Out.eraseNamedMetadata(CU);
  }
  if (!preserveLocalUseOrder(F, Map) || verifyModule(Out, nullptr)) return {};
  std::string S; raw_string_ostream OS(S); WriteBitcodeToFile(Out, OS, true); return S;
}
bool replay(Request &R, Function &F, FunctionAnalysisManager &AM, const Stage &P) {
  auto Fail = [&](const char *Site) {
    if (AnalysisDiagnostics) std::fprintf(stderr, "ZIL FPM replay-failure %.*s: %s\n", static_cast<int>(F.getName().size()), F.getName().data(), Site);
    return false;
  };
  auto Buffer = MemoryBuffer::getMemBufferCopy(P.Output, "zil.function.product");
  auto Parsed = parseBitcodeFile(Buffer->getMemBufferRef(), F.getContext());
  if (!Parsed) { consumeError(Parsed.takeError()); return Fail("bitcode-parse"); }
  std::unique_ptr<Module> Source = std::move(*Parsed);
  Function *SF = Source->getFunction(F.getName());
  if (!SF || !eligibleFunction(*SF) || SF->getFunctionType() != F.getFunctionType() ||
      verifyModule(*Source, nullptr)) return Fail("snapshot-function-or-verifier");
  ValueToValueMapTy Map; Map[SF] = &F;
  auto A = F.arg_begin();
  for (const Argument &SA : SF->args()) Map[&SA] = &*A++;
  for (Function &D : *Source) if (&D != SF) {
    Function *Current = R.M.getFunction(D.getName());
    auto Saved = P.OriginalDeclarations.find(D.getName().str());
    if (!D.isDeclaration() || !Current || Current->getFunctionType() != D.getFunctionType() ||
        !D.isIntrinsic() || D.getIntrinsicID() != Current->getIntrinsicID() ||
        Saved == P.OriginalDeclarations.end() || declarationText(*Current) != Saved->second)
      return Fail("current-original-declaration-binding");
    if (AnalysisDiagnostics && Current->getAttributes() != D.getAttributes()) {
      std::string A, B; raw_string_ostream AO(A), BO(B);
      Current->getAttributes().print(AO); D.getAttributes().print(BO);
      std::fprintf(stderr, "ZIL intrinsic serialization attributes %.*s current=[%s] parsed=[%s]\n", static_cast<int>(D.getName().size()), D.getName().data(), A.c_str(), B.c_str());
    }
    // Reading even current-version bitcode upgrades intrinsic declarations.
    // Rebind to the independently checked CURRENT original declaration state,
    // whose complete projection above is equal to the cold stage's original.
    D.setAttributes(Current->getAttributes());
    Map[&D] = Current;
  }
  // The cold group was retained only if its complete module/header projection
  // was unchanged. The subject's original attributes therefore equal CURRENT
  // entry attributes as well; do not transport parser-upgraded function attrs.
  SF->setAttributes(F.getAttributes());
  for (const void *K : P.ExitKeys) if (!symbols().has(&AM, K)) return Fail("exit-analysis-registration");
  // invalidate(none) deliberately does not suffice: some immutable analyses
  // self-preserve. Destroy the complete old function cache before body replace.
  AM.clear(F, F.getName());
  F.deleteBody(); F.setLinkage(SF->getLinkage()); F.copyAttributesFrom(SF);
  A = F.arg_begin(); for (const Argument &SA : SF->args()) (A++)->setName(SA.getName());
  SmallVector<ReturnInst *, 8> Returns;
  const bool HadCompileUnits = R.M.getNamedMetadata("llvm.dbg.cu") != nullptr;
  CloneFunctionInto(&F, SF, Map, CloneFunctionChangeType::DifferentModule, Returns);
  // Preserve every original module field. Only undo the empty root invented
  // by this clone; an unexpected nonempty contribution invalidates replay.
  if (!HadCompileUnits) {
    if (auto *CU = R.M.getNamedMetadata("llvm.dbg.cu")) {
      if (CU->getNumOperands()) return Fail("unexpected-module-debug-metadata");
      R.M.eraseNamedMetadata(CU);
    }
  }
  if (!symbols().restoreNumbers(&F, P.OutputNextBlock, P.OutputBlockEpoch,
                               P.OutputBlockNumbers.data(), P.OutputBlockNumbers.size())) return Fail("block-number-restore");
  if (!preserveLocalUseOrder(*SF, Map)) return Fail("local-use-order");
  if (verifyFunction(F, nullptr)) return Fail("current-function-verifier");
  if (!symbols().restoreNames(&F, P.OutputNames.present, P.OutputNames.counter, P.OutputNames.maximum)) return Fail("value-name-state-restore");
  if (functionText(F) != P.OutputText) return Fail("complete-output-text");
  for (const void *K : P.ExitKeys) { symbols().require(&AM, &F, K); ++R.Stats.analysis_reconstructions; }
  if (!restoreAnalysisFacts(F, AM, P.OutputAnalysisReplay)) return Fail("empty-analysis-state-restore");
  std::string Facts;
  if (analysisKeys(AM, F) != P.ExitKeys) return Fail("exit-analysis-availability-order");
  if (!analysisFacts(F, AM, P.ExitKeys, Facts, nullptr, &R.AnalysisMemo)) return Fail("exit-analysis-domain");
  if (Facts != P.ExitFacts) {
    if (AnalysisDiagnostics) {
      size_t At = 0;
      while (At < Facts.size() && At < P.ExitFacts.size() && Facts[At] == P.ExitFacts[At]) ++At;
      auto Fragment = [At](const std::string &S) {
        size_t Start = At > 120 ? At - 120 : 0;
        return S.substr(Start, 320);
      };
      std::fprintf(stderr, "ZIL FPM facts-difference %.*s offset=%zu expected-bytes=%zu actual-bytes=%zu\nEXPECTED [%s]\nACTUAL [%s]\n",
          static_cast<int>(F.getName().size()), F.getName().data(), At,
          P.ExitFacts.size(), Facts.size(), Fragment(P.ExitFacts).c_str(), Fragment(Facts).c_str());
    }
    return Fail("exit-analysis-facts");
  }
  R.Stats.ir_replay_bytes += P.Output.size(); return true;
}
int beforeStage(void *Context, void *PM, void *IR, void *Manager, void *, void *PA, void **Token) {
  auto &R = *static_cast<Request *>(Context); auto &F = *static_cast<Function *>(IR);
  auto &AM = *static_cast<FunctionAnalysisManager *>(Manager);
  *Token = nullptr;
  if (F.getParent() != &R.M || R.Phase != 1) return 0;
  if (R.Failed) { *static_cast<PreservedAnalyses *>(PA) = PreservedAnalyses::none(); return 1; }
  if (!eligibleFunction(F)) { ++R.Stats.optimizer_function_stage_executions; return 0; }
  // Bind actual supported result fields, not merely analysis names or IDs.
  Keys Entry = analysisKeys(AM, F);
  describeAnalyses(AM, F, "entry", Entry);
  std::string Facts;
  if (!analysisFacts(F, AM, Entry, Facts, nullptr, &R.AnalysisMemo)) {
    if (AnalysisDiagnostics) *Token = newPending(R);
    ++R.Stats.optimizer_function_stage_executions; return 0;
  }
  auto P = std::make_shared<Stage>();
  P->Path = path(R, F); P->Pipeline = pipeline(*static_cast<FunctionPassManager *>(PM));
  P->Input = functionText(F); P->World = worldText(R.M, &F, &R.Stats); P->EntryKeys = std::move(Entry); P->EntryFacts = std::move(Facts);
  R.Stats.ir_key_bytes += P->Input.size() + P->World.size() + P->Pipeline.size() + P->EntryFacts.size() + R.Config.size();
  std::string Slot = P->Path;
  if (R.Prior && R.Prior->Config == R.Config) {
    auto It = R.Prior->Stages.find(Slot);
    if (It != R.Prior->Stages.end()) {
      const auto &Old = *It->second;
      if (Old.Input == P->Input && Old.World == P->World && Old.Pipeline == P->Pipeline && Old.EntryKeys == P->EntryKeys && Old.EntryFacts == P->EntryFacts) {
        if (!replay(R, F, AM, Old)) {
          R.Failed = true; ++R.Stats.failed_replays;
          *static_cast<PreservedAnalyses *>(PA) = PreservedAnalyses::none(); return 1;
        }
        describeAnalyses(AM, F, "replayed-exit", analysisKeys(AM, F));
        *static_cast<PreservedAnalyses *>(PA) = Old.PA;
        retainStage(R, Slot, It->second); ++R.Stats.optimizer_function_stage_reuses; return 1;
      }
    }
  }
  ++R.Stats.optimizer_function_stage_executions;
  *Token = newPending(R, std::move(Slot), std::move(P)); return 0;
}
void afterStage(void *Context, void *, void *IR, void *Manager, const void *PA, void *Token) {
  auto &R = *static_cast<Request *>(Context);
  auto It = R.PendingTokens.find(static_cast<Pending *>(Token));
  if (It == R.PendingTokens.end()) return;
  auto P = std::move(It->second); R.PendingTokens.erase(It);
  if (R.Failed) return;
  auto &F = *static_cast<Function *>(IR);
  describeAnalyses(*static_cast<FunctionAnalysisManager *>(Manager), F, "exit", analysisKeys(*static_cast<FunctionAnalysisManager *>(Manager), F));
  if (!P->Product || R.Failed || !eligibleFunction(F) || worldText(R.M, &F, &R.Stats) != P->Product->World) return;
  P->Product->ExitKeys = analysisKeys(*static_cast<FunctionAnalysisManager *>(Manager), F);
  if (!analysisFacts(F, *static_cast<FunctionAnalysisManager *>(Manager),
                     P->Product->ExitKeys, P->Product->ExitFacts, &P->Product->OutputAnalysisReplay, &R.AnalysisMemo)) return;
  for (const BasicBlock &B : F) for (const Instruction &I : B)
    if (const auto *Call = dyn_cast<CallBase>(&I)) {
      const Function *D = Call->getCalledFunction();
      P->Product->OriginalDeclarations.emplace(D->getName().str(), declarationText(*D));
    }
  P->Product->OutputNames = nameState(F);
  if (P->Product->OutputNames.present < 0) return;
  P->Product->OutputText = functionText(F);
  P->Product->OutputNextBlock = F.getMaxBlockNumber();
  P->Product->OutputBlockEpoch = F.getBlockNumberEpoch();
  for (const BasicBlock &B : F) P->Product->OutputBlockNumbers.push_back(B.getNumber());
  P->Product->Output = snapshot(F);
  if (P->Product->Output.empty()) return;
  P->Product->PA = *static_cast<const PreservedAnalyses *>(PA);
  R.Stats.ir_snapshot_bytes += P->Product->Output.size() + P->Product->OutputText.size();
  retainStage(R, P->Slot, std::move(P->Product));
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
int requestFailedCallback(void *C) { return static_cast<Request *>(C)->Failed; }
void copyStats(Request &R, ZilNativeProductStats *Out) {
  R.Stats.tli_projection_builds = R.AnalysisMemo.targetLibraryProjections();
  R.Stats.tli_projection_reuses = R.AnalysisMemo.targetLibraryReuses();
  if (Out) *Out = R.Stats;
}
}
}
extern "C" void *ZilNativeProductsCreateRequest(void *M, void *TM, const void *Prior, const char *C, size_t N) {
  using namespace zil;
  if (!M || !TM || !symbols().available || (!C && N)) return nullptr;
  auto R = std::make_unique<Request>(*static_cast<Module *>(M), *static_cast<TargetMachine *>(TM), std::string(C ? C : "", N), static_cast<const Products *>(Prior));
  R->Stats.available = 1;
  R->Machine = createMachine(R->M, R->TM, R->Config,
      R->Prior && R->Prior->Config == R->Config ? R->Prior->Machine : nullptr, R->Stats, R->Failed);
  if (!R->Machine) return nullptr;
  R->Hooks = {2, R.get(), beforeMachineCallback, enterPass, leavePass, beforeStage, afterStage, skippedCallback, requestFailedCallback};
  R->Previous = symbols().set(&R->Hooks); return R.release();
}
extern "C" void ZilNativeProductsSetPhase(void *P, unsigned Phase) {
  if (!P) return;
  auto &R = *static_cast<zil::Request *>(P);
  // LLVMRunPasses owns its FAM/factory. Drop all borrowed identity tokens
  // before emission; a future second optimization invocation must also clear.
  if (Phase != 1 || R.Phase == 1) R.AnalysisMemo.clear();
  R.Phase = Phase;
}
extern "C" int ZilNativeProductsFailed(const void *P) { return P && static_cast<const zil::Request *>(P)->Failed; }
extern "C" void *ZilNativeProductsFinish(void *P, ZilNativeProductStats *S, int *Failed) {
  if (Failed) *Failed = 1;
  using namespace zil; std::unique_ptr<Request> R(static_cast<Request *>(P)); if (!R) return nullptr;
  symbols().set(R->Previous);
  if (!R->PendingTokens.empty()) { R->Failed = true; ++R->Stats.failed_replays; }
  if (R->Failed) { copyStats(*R, S); return nullptr; }
  if (R->Machine) R->Next.Machine = finishMachine(*R->Machine);
  if (R->Failed) {
    if (!R->Stats.failed_replays) ++R->Stats.failed_replays;
    copyStats(*R, S); return nullptr;
  }
  R->Stats.retained_bytes += R->ProductBytes;
  copyStats(*R, S);
  auto *Result = new Products(std::move(R->Next));
  if (Failed) *Failed = 0;
  return Result;
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

extern "C" int ZilNativeProductsAnalysisDiagnostics(int Enabled) {
  bool Old = zil::AnalysisDiagnostics; zil::AnalysisDiagnostics = Enabled != 0; return Old;
}

extern "C" int ZilNativeProductsOwnedUseOrderCounterexample() {
  LLVMContext Context; Module M("owned-use-order", Context);
  auto *I32 = Type::getInt32Ty(Context);
  auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "f", &M);
  F->getArg(0)->setName("x");
  IRBuilder<> Builder(BasicBlock::Create(Context, "entry", F));
  Value *A = Builder.CreateAdd(F->getArg(0), Builder.getInt32(1), "a");
  Value *B = Builder.CreateAdd(F->getArg(0), Builder.getInt32(2), "b");
  Builder.CreateRet(Builder.CreateAdd(A, B, "result"));
  if (verifyFunction(*F, nullptr)) return 0;
  auto Print = [&] { std::string S; raw_string_ostream OS(S); F->print(OS, nullptr, true); return S; };
  std::string Text = Print(), Key = zil::functionText(*F);
  F->getArg(0)->reverseUseList();
  if (Print() != Text || zil::functionText(*F) == Key || verifyFunction(*F, nullptr)) return 0;
  F->getArg(0)->reverseUseList();
  return Print() == Text && zil::functionText(*F) == Key;
}

extern "C" int ZilNativeProductsIncomingCallerCounterexample() {
  LLVMContext Context; Module M("incoming-use-order", Context);
  auto *I32 = Type::getInt32Ty(Context);
  auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "f", &M);
  F->getArg(0)->setName("x");
  IRBuilder<> FB(BasicBlock::Create(Context, "entry", F)); FB.CreateRet(F->getArg(0));
  auto *G = Function::Create(FunctionType::get(I32, false), GlobalValue::ExternalLinkage, "caller", &M);
  auto *Entry = BasicBlock::Create(Context, "entry", G);
  auto *Target = BasicBlock::Create(Context, "target", G);
  IRBuilder<> GB(Entry); GB.CreateBr(Target); GB.SetInsertPoint(Target);
  auto *Call = GB.CreateCall(F, {GB.getInt32(7)}, "call"); GB.CreateRet(Call);
  if (verifyModule(M, nullptr) || !zil::ownedUseSequence(*G, nullptr)) return 0;
  if (zil::readNameStateAPI() && !zil::eligibleFunction(*F)) return 0;
  auto *Address = BlockAddress::get(G, Target);
  new GlobalVariable(M, Address->getType(), true, GlobalValue::ExternalLinkage, Address, "taken");
  if (verifyModule(M, nullptr) || zil::ownedUseSequence(*G, nullptr) ||
      !zil::functionText(*G).empty() || zil::eligibleFunction(*F)) return 0;
  Call->setArgOperand(0, GB.getInt32(8));
  // Two unrepresentable caller bodies would both produce an empty string.
  // Neither is admitted as a native product input for the unchanged callee.
  return !verifyModule(M, nullptr) && zil::functionText(*G).empty() && !zil::eligibleFunction(*F);
}

extern "C" int ZilNativeProductsAttributeMetadataCounterexample() {
  LLVMContext Context; Module M("attribute-metadata-projection", Context);
  auto *I32 = Type::getInt32Ty(Context);
  auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "f", &M);
  IRBuilder<> FB(BasicBlock::Create(Context, "entry", F)); FB.CreateRet(F->getArg(0));
  auto *G = Function::Create(FunctionType::get(I32, false), GlobalValue::ExternalLinkage, "caller", &M);
  IRBuilder<> GB(BasicBlock::Create(Context, "entry", G));
  auto *Call = GB.CreateCall(F, {GB.getInt32(7)}, "call");
  auto *Sum = cast<Instruction>(GB.CreateAdd(Call, GB.getInt32(1), "sum")); GB.CreateRet(Sum);
  Call->addFnAttr(Attribute::NoUnwind);
  auto Print = [&] { std::string S; raw_string_ostream OS(S); G->print(OS, nullptr, true); return S; };
  if (verifyModule(M, nullptr) || !zil::ownedUseSequence(*G, nullptr)) return 0;
  if (zil::readNameStateAPI() && !zil::eligibleFunction(*F)) return 0;
  std::string Text = Print(), Key = zil::functionText(*G);
  Call->removeFnAttr(Attribute::NoUnwind); Call->addFnAttr(Attribute::NoFree);
  if (verifyModule(M, nullptr) || Print() != Text || zil::functionText(*G) == Key) return 0;
  Call->removeFnAttr(Attribute::NoFree); Call->addFnAttr(Attribute::NoUnwind);
  if (Print() != Text || zil::functionText(*G) != Key) return 0;
  auto Payload = [&](unsigned Value) {
    auto *Child = MDNode::get(Context, {ConstantAsMetadata::get(GB.getInt32(Value))});
    return MDNode::get(Context, {Child});
  };
  Sum->setMetadata("zil.payload", Payload(7));
  Text = Print(); Key = zil::functionText(*G);
  Sum->setMetadata("zil.payload", Payload(8));
  if (verifyModule(M, nullptr) || Print() != Text || zil::functionText(*G) == Key) return 0;
  Sum->setMetadata("zil.payload", Payload(7));
  if (Print() != Text || zil::functionText(*G) != Key || !zil::ownedUseSequence(*G, nullptr)) return 0;
  // A local metadata reference is not represented by Value::uses(). It must
  // reject, including when reached through an otherwise ordinary caller.
  Sum->setMetadata("zil.payload", MDNode::get(Context, {ValueAsMetadata::get(Call)}));
  if (zil::ownedUseSequence(*G, nullptr) || !zil::functionText(*G).empty()) return 0;
  Sum->setMetadata("zil.payload", nullptr);
  return !verifyModule(M, nullptr) && zil::ownedUseSequence(*G, nullptr);
}

extern "C" int ZilNativeProductsModuleHeaderCounterexample() {
  using PrintHeaders = int (*)(const void *, void *);
  auto Print = reinterpret_cast<PrintHeaders>(dlsym(RTLD_DEFAULT, "ZilLLVMPrintModuleHeaders"));
  if (!Print) return 0;
  LLVMContext Context; Module M("header-renderer", Context);
  auto *I32 = Type::getInt32Ty(Context);
  auto *Global = new GlobalVariable(M, I32, true, GlobalValue::InternalLinkage,
                                    ConstantInt::get(I32, 7), "global");
  auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "f", &M);
  F->getArg(0)->setName("x"); F->addFnAttr(Attribute::NoUnwind);
  IRBuilder<> B(BasicBlock::Create(Context, "entry", F));
  auto *Add = cast<Instruction>(B.CreateAdd(F->getArg(0), B.getInt32(1), "sum"));
  B.CreateRet(Add);
  F->setMetadata("zil.header", MDNode::get(Context, {MDString::get(Context, "payload")}));
  auto Headers = [&] {
    std::string S; raw_string_ostream OS(S);
    if (!Print(&M, &OS)) return std::string(); return S;
  };
  if (verifyModule(M, nullptr)) return 0;
  std::string Full; raw_string_ostream FullOS(Full); M.print(FullOS, nullptr, true);
  // Independent oracle: normal, unmodified LLVM module rendering, with the
  // one actual body removed. No body-only attributes/types/MD exist here.
  size_t Begin = Full.find("define "), Body = Full.find(" {", Begin);
  size_t End = Full.find("}\n", Body);
  if (Begin == std::string::npos || Body == std::string::npos || End == std::string::npos) return 0;
  std::string Reference = Full.substr(0, Body) + "\n" + Full.substr(End + 2);
  std::string Key = Headers();
  if (Key.empty() || Key != Reference) return 0;
  Add->setOperand(1, B.getInt32(2));
  if (Headers() != Key) return 0; // the excluded body is a separate actual key
  F->setLinkage(GlobalValue::InternalLinkage);
  if (Headers() == Key) return 0;
  F->setLinkage(GlobalValue::ExternalLinkage);
  if (Headers() != Key) return 0;
  F->addFnAttr(Attribute::NoFree);
  if (Headers() == Key) return 0;
  F->removeFnAttr(Attribute::NoFree);
  F->setMetadata("zil.header", MDNode::get(Context, {MDString::get(Context, "changed")}));
  if (Headers() == Key) return 0;
  F->setMetadata("zil.header", MDNode::get(Context, {MDString::get(Context, "payload")}));
  Global->setInitializer(B.getInt32(8));
  if (Headers() == Key) return 0;
  Global->setInitializer(B.getInt32(7));
  if (Headers() != Key || verifyModule(M, nullptr)) return 0;
  // Header references into function bodies cannot be rendered by this mode.
  auto *Target = BasicBlock::Create(Context, "target", F);
  IRBuilder<> TB(Target); TB.CreateRet(TB.getInt32(0));
  auto *Address = BlockAddress::get(F, Target);
  new GlobalVariable(M, Address->getType(), true, GlobalValue::ExternalLinkage, Address, "taken");
  std::string Rejected; raw_string_ostream RejectedOS(Rejected);
  if (verifyModule(M, nullptr) || Print(&M, &RejectedOS) || !Rejected.empty()) return 0;
  ZilNativeProductStats Stats{};
  const std::string Before = zil::worldText(M, F, &Stats);
  cast<ReturnInst>(Target->getTerminator())->setOperand(0, TB.getInt32(1));
  const std::string After = zil::worldText(M, F, &Stats);
  return !Before.empty() && Before != After && Stats.world_projection_fallback == 2 &&
         Stats.world_projection_direct == 0 && !verifyModule(M, nullptr);
}
