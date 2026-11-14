// Fixed LLVM 23.1.4 resident optimization products. The original pass managers
// construct and run their original pipelines; this adapter never reparses one.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "work_products.h"
#include "zil_llvm_work_products_hooks.h"
#include "machine_products.h"
#include "machine_emission_products.h"
#include "analysis_facts.h"
#include "packed_stage.h"
#include "stage_body.h"
#include "cohort_products.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/ADT/SmallPtrSet.h"
#include "llvm/Analysis/TargetLibraryInfo.h"
#include "llvm/Bitcode/BitcodeReader.h"
#include "llvm/Bitcode/BitcodeWriter.h"
#include "llvm/IR/CFG.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/Dominators.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/IntrinsicInst.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/Operator.h"
#include "llvm/IR/PassManager.h"
#include "llvm/IR/PassInstrumentation.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/Error.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/Support/xxhash.h"
#include "llvm/Target/TargetMachine.h"
#include "llvm/Target/CGPassBuilderOption.h"
#include "llvm/Transforms/Utils/Cloning.h"
#include "llvm/Transforms/Utils/PromoteMemToReg.h"
#include <algorithm>
#include <array>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <map>
#include <limits>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>
using namespace llvm;
namespace zil {
namespace {
// Observational scopes only: never enter keys, budgets, PA or admission.
// Keep parent timers alive until after owned locals are destroyed. Early returns
// and failures are charged by RAII; explicit stop separates key and parse work.
template <typename Clock = std::chrono::steady_clock> class IRScopeNS {
  uint64_t *Total;
  typename Clock::time_point Start = Clock::now();
public:
  explicit IRScopeNS(uint64_t &Counter) : Total(&Counter) {}
  IRScopeNS(const IRScopeNS &) = delete;
  IRScopeNS &operator=(const IRScopeNS &) = delete;
  void stop() {
    if (!Total) return;
    auto N = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - Start).count();
    uint64_t Amount = N > 0 ? static_cast<uint64_t>(N) : 0;
    *Total = Amount > std::numeric_limits<uint64_t>::max() - *Total
                 ? std::numeric_limits<uint64_t>::max() : *Total + Amount;
    Total = nullptr;
  }
  ~IRScopeNS() { stop(); }
};
thread_local bool AnalysisDiagnostics = false;
// Explicit scoped fixture control only; never enabled by a build configuration.
thread_local bool StageAlwaysForTests = false;
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
// A world projection may inspect incoming instructions. Establish their
// actual current owners first; a ConstantExpr or detached/foreign call is not
// a representable caller, even if the subject body's printed IR is unchanged.
bool incomingCallersHaveCurrentOwners(const Function &F) {
  if (!F.getParent()) return false;
  for (const User *U : F.users()) {
    const auto *C = dyn_cast<CallBase>(U);
    if (!C || C->getCalledOperand() != &F || !C->getParent()) return false;
    const Function *Caller = C->getParent()->getParent();
    if (!Caller || Caller->getParent() != F.getParent()) return false;
  }
  return true;
}
bool eligibleFunction(const Function &F) {
  if (!incomingCallersHaveCurrentOwners(F)) return admissionFailure(F, nullptr, "unbound-incoming-owner");
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
    const Function *Caller = C->getParent()->getParent();
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
// Function::print does not emit definitions of body-only identified types.
// Collect the actual reachable type graph once, in deterministic root/operand
// order. Type::print emits each named definition with all packed/opaque/element
// fields; separately visiting subtypes closes every referenced definition.
// No borrowed type or LLVM owner escapes this local projection.
struct FunctionTypeProjection {
  SmallPtrSet<const Type *, 16> Types;
  SmallPtrSet<const Constant *, 16> Constants;
  SmallVector<const StructType *, 4> Definitions;
  bool addType(Type *Root) {
    auto Leaf = [](Type *T) { return T->isIntegerTy() || T->isVoidTy() || T->isPointerTy() ||
                                    T->isFloatingPointTy() || T->isLabelTy() || T->isTokenTy(); };
    if (!Root) return false;
    // The overwhelmingly common scalar/opaque-pointer types have no subtype
    // definitions. Avoid even a pointer-set operation for these leaves.
    if (Leaf(Root)) return true;
    SmallVector<Type *, 8> Pending{Root};
    while (!Pending.empty()) {
      Type *T = Pending.pop_back_val();
      if (!T) return false;
      if (Leaf(T) || !Types.insert(T).second) continue;
      if (const auto *ST = dyn_cast<StructType>(T)) if (!ST->isLiteral()) {
        // The standalone Type printer gives an unnamed identified struct a
        // transient address. Refuse this optional product, not compilation.
        if (!ST->hasName()) return false;
        Definitions.push_back(ST);
      }
      for (Type *Sub : T->subtypes()) Pending.push_back(Sub);
    }
    return true;
  }
  bool addAttributes(AttributeList Attrs) {
    for (AttributeSet Set : Attrs) for (Attribute A : Set)
      if (A.isTypeAttribute() && !addType(A.getValueAsType())) return false;
    return true;
  }
  bool addValue(const Value *Root) {
    auto Scan = [&](const Value *V) {
      if (!V || !addType(V->getType()) || isa<BlockAddress>(V)) return false;
      if (const auto *GEP = dyn_cast<GEPOperator>(V))
        if (!addType(GEP->getSourceElementType())) return false;
      return true;
    };
    if (!Scan(Root)) return false;
    const auto *C = dyn_cast<Constant>(Root);
    // Global definitions are independently bound by the complete world.
    if (!C || isa<GlobalValue>(C) || !C->getNumOperands()) return true;
    SmallVector<const Constant *, 8> Pending{C};
    while (!Pending.empty()) {
      C = Pending.pop_back_val();
      if (!Constants.insert(C).second) continue;
      for (const Use &U : C->operands()) {
        const Value *V = U.get();
        if (!Scan(V)) return false;
        if (const auto *Sub = dyn_cast<Constant>(V))
          if (!isa<GlobalValue>(Sub) && Sub->getNumOperands()) Pending.push_back(Sub);
      }
    }
    return true;
  }
  bool addInstruction(const Instruction &I) {
    if (!addType(I.getType())) return false;
    if (const auto *A = dyn_cast<AllocaInst>(&I))
      if (!addType(A->getAllocatedType())) return false;
    if (const auto *GEP = dyn_cast<GetElementPtrInst>(&I))
      if (!addType(GEP->getSourceElementType())) return false;
    if (const auto *C = dyn_cast<CallBase>(&I))
      if (!addType(C->getFunctionType()) || !addAttributes(C->getAttributes())) return false;
    for (const Use &U : I.operands()) if (!addValue(U.get())) return false;
    return true;
  }
  void print(raw_ostream &OS) const {
    if (Definitions.empty()) return;
    OS << "\n; complete-body-type-definitions-v1\n";
    for (size_t I = 0; I != Definitions.size(); ++I) {
      OS << I << ':'; Definitions[I]->print(OS, false, false); OS << '\n';
    }
  }
};
std::string projectFunctionText(const Function &F, bool *EligibilityTypesBound) {
  if (EligibilityTypesBound) *EligibilityTypesBound = false;
  std::vector<uint32_t> Uses;
  if (!ownedUseSequence(F, &Uses)) return {};
  NameState Names = nameState(F);
  if (Names.present < 0) return {};
  FunctionTypeProjection Types;
  if (!Types.addType(F.getFunctionType()) || !Types.addAttributes(F.getAttributes())) return {};
  std::string S; raw_string_ostream OS(S); F.print(OS, nullptr, true);
  OS << "\n; block-number-state " << F.getMaxBlockNumber() << ':' << F.getBlockNumberEpoch();
  for (const BasicBlock &B : F) OS << ':' << B.getNumber();
  OS << "\n; value-name-state " << Names.present << ':' << Names.maximum << ':' << Names.counter;
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
    if (!Types.addInstruction(I)) return {};
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
  Types.print(OS);
  OS << "\n; owned-use-sequence-v1\n";
  // A private resident byte projection, never LLVM source or a disk format.
  OS.write(reinterpret_cast<const char *>(Uses.data()), Uses.size() * sizeof(uint32_t));
  OS << '\n';
  if (EligibilityTypesBound) *EligibilityTypesBound = true;
  return S;
}
std::string functionText(const Function &F) { return projectFunctionText(F, nullptr); }
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
  if (Subject && (!incomingCallersHaveCurrentOwners(*Subject) || Subject->getParent() != &M))
    return {};
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
      if (const auto *I = dyn_cast<Instruction>(U)) Callers.insert(I->getParent()->getParent());
    for (const Function &F : M)
      if (Callers.count(&F)) {
        std::string Caller = functionText(F);
        if (Caller.empty()) return {};
        OS << "\n; incoming caller\n" << Caller;
      }
    // User iteration order is observable to transformations. Bind the actual
    // outside-body use list as stable current IR positions, without retaining
    // historical Instruction/Use pointers.
    OS << "\n; ordered incoming uses\n";
    for (const Use &U : Subject->uses()) {
      const auto *I = dyn_cast<Instruction>(U.getUser());
      if (!I) { U.getUser()->print(OS); OS << '\n'; continue; }
      const Function *Caller = I->getParent()->getParent();
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
// This arena is an index, never a second owner/budget for a product. The
// request which completed a product owns it through its ordinary capped map.
std::vector<std::shared_ptr<const void>> CohortArena::candidates(
    unsigned Kind, uint64_t Hash, ZilNativeCohortStatsV1 *Local) {
  std::lock_guard<std::mutex> Lock(mutex);
  std::vector<std::shared_ptr<const void>> Result;
  if (disabled) { if (Local) Local->disabled = 1; return Result; }
  for (auto I = entries.begin(); I != entries.end();) {
    auto P = I->value.lock();
    if (!P) {
      ++stats.expired_entries; if (Local) ++Local->expired_entries;
      I = entries.erase(I); continue;
    }
    if (I->kind == Kind && I->hash == Hash) {
      Result.push_back(std::move(P));
      if (Kind == 0) { ++stats.ir_candidates; if (Local) ++Local->ir_candidates; }
      else { ++stats.machine_candidates; if (Local) ++Local->machine_candidates; }
    }
    ++I;
  }
  stats.indexed_entries = entries.size();
  return Result;
}
void CohortArena::publish(unsigned Kind, uint64_t Hash,
                          const std::shared_ptr<const void> &P,
                          ZilNativeCohortStatsV1 *Local) {
  if (!P) return;
  std::lock_guard<std::mutex> Lock(mutex);
  if (disabled) { if (Local) Local->disabled = 1; return; }
  for (auto I = entries.begin(); I != entries.end();) {
    auto Existing = I->value.lock();
    if (!Existing) {
      ++stats.expired_entries; if (Local) ++Local->expired_entries;
      I = entries.erase(I); continue;
    }
    if (I->kind == Kind && I->hash == Hash && Existing.get() == P.get()) return;
    ++I;
  }
  if (entries.size() == MaxEntries) {
    ++stats.declines; if (Local) ++Local->declines;
  } else {
    entries.push_back({Hash, Kind, P});
    if (Local) ++Local->indexed_entries; // local insertion events, not a gauge
  }
  stats.indexed_entries = entries.size();
}
void CohortArena::replayed(unsigned Kind, ZilNativeCohortStatsV1 *Local) {
  std::lock_guard<std::mutex> Lock(mutex);
  if (Kind == 0) { ++stats.ir_replays; if (Local) ++Local->ir_replays; }
  else { ++stats.machine_replays; if (Local) ++Local->machine_replays; }
}
void CohortArena::decline(ZilNativeCohortStatsV1 *Local) {
  std::lock_guard<std::mutex> Lock(mutex);
  ++stats.declines; if (Local) ++Local->declines;
}
void CohortArena::disable(ZilNativeCohortStatsV1 *Local) {
  std::lock_guard<std::mutex> Lock(mutex);
  disabled = true; entries.clear(); stats.disabled = 1; stats.indexed_entries = 0;
  if (Local) Local->disabled = 1;
}
ZilNativeCohortStatsV1 CohortArena::read() const {
  std::lock_guard<std::mutex> Lock(mutex); return stats;
}
uint64_t cohortHash(const std::string &A, const std::string &B,
                    const std::string &C, const std::string &D) {
  std::array<uint64_t, 8> Words{}; size_t I = 0;
  for (const auto *S : {&A, &B, &C, &D}) {
    Words[I++] = S->size();
    Words[I++] = xxh3_64bits(reinterpret_cast<const uint8_t *>(S->data()), S->size());
  }
  return xxh3_64bits(reinterpret_cast<const uint8_t *>(Words.data()), sizeof(Words));
}
namespace {
bool compilerCohortName(StringRef N) {
  if (!N.consume_front("zil.fn.") || N.empty()) return false;
  return llvm::all_of(N, [](char C) { return C >= '0' && C <= '9'; });
}
bool cohortType(Type *T) {
  if (T->isVoidTy() || T->isIntegerTy() || T->isPointerTy() || T->isLabelTy()) return true;
  auto *S = dyn_cast<StructType>(T);
  if (!S || !S->isLiteral() || S->isOpaque()) return false;
  return llvm::all_of(S->elements(), [](Type *E) { return E->isIntegerTy(); });
}
// Module flags may affect integer/ABI lowering. Bind the COMPLETE primitive
// metadata tree; no MDString containing a profile/target directive is ignored.
// ValueAsMetadata/DI nodes/unknown named metadata cannot enter this domain.
bool cohortMetadata(const Metadata *M, raw_ostream &OS, unsigned Depth = 0) {
  if (!M || Depth > 16) return false;
  if (const auto *S = dyn_cast<MDString>(M)) {
    OS << 's' << S->getString().size() << ':' << S->getString(); return true;
  }
  if (const auto *C = dyn_cast<ConstantAsMetadata>(M)) {
    const auto *I = dyn_cast<ConstantInt>(C->getValue());
    if (!I) return false;
    OS << 'i' << I->getBitWidth() << ':' << I->getValue(); return true;
  }
  const auto *N = dyn_cast<MDTuple>(M);
  if (!N) return false;
  OS << '[' << N->getNumOperands() << ':';
  for (const auto &O : N->operands()) if (!cohortMetadata(O.get(), OS, Depth + 1)) return false;
  OS << ']'; return true;
}
// Attribute #N references are ModuleSlotTracker serialization identities, not
// Attribute values. The complete function/call (or declaration) attributes are
// separately present in the unchanged projection footer. Alpha-renumber ONLY
// actual unquoted LLVM reference tokens, preserving their equality relation.
// Never scan the binary owned-use suffix, quoted names/strings, or comments.
std::string cohortAttributeSlots(const std::string &Text, StringRef Footer) {
  const size_t End = Text.find(Footer.str());
  if (End == std::string::npos) return {};
  if (Footer == "\n; block-number-state " &&
      Text.find("\n; complete-function-attributes ", End) == std::string::npos) return {};
  std::string Out; Out.reserve(Text.size());
  std::map<unsigned, unsigned> Slots;
  bool Quoted = false, Comment = false;
  for (size_t I = 0; I < End;) {
    const char C = Text[I];
    if (Comment) {
      Out.push_back(C); ++I;
      if (C == '\n') Comment = false;
      continue;
    }
    if (Quoted) {
      if (C == '\n' || C == '\r') return {};
      Out.push_back(C); ++I;
      if (C == '\\') {
        if (I == End) return {};
        Out.push_back(Text[I++]);
      } else if (C == '"') Quoted = false;
      continue;
    }
    if (C == ';') { Comment = true; Out.push_back(C); ++I; continue; }
    if (C == '"') { Quoted = true; Out.push_back(C); ++I; continue; }
    if (C != '#') { Out.push_back(C); ++I; continue; }
    if (!I || (Text[I - 1] != ' ' && Text[I - 1] != '\t')) return {};
    size_t Next = I + 1; unsigned ID = 0;
    if (Next == End || Text[Next] < '0' || Text[Next] > '9') return {};
    while (Next < End && Text[Next] >= '0' && Text[Next] <= '9') {
      const unsigned Digit = Text[Next++] - '0';
      if (ID > (std::numeric_limits<unsigned>::max() - Digit) / 10) return {};
      ID = ID * 10 + Digit;
    }
    if (Next != End && Text[Next] != ' ' && Text[Next] != '\t' &&
        Text[Next] != '\n' && Text[Next] != '\r' && Text[Next] != ',') return {};
    auto [At, Inserted] = Slots.emplace(ID, static_cast<unsigned>(Slots.size()));
    (void)Inserted;
    Out.push_back('#'); Out += std::to_string(At->second); I = Next;
  }
  if (Quoted) return {};
  Out.append(Text, End, std::string::npos);
  return Out;
}
}
bool cohortTarget(const TargetMachine &TM) {
  const auto &T = TM.getTargetTriple();
  const auto CG = getCGPassBuilderOption();
  const auto &Options = cl::getRegisteredOptions();
  const auto Outliner = Options.find("enable-machine-outliner");
  // Fixed7a X86TargetMachine unconditionally sets EnableMachineOutliner=true:
  // it advertises capability, not execution. TargetPassConfig adds the pass
  // only for an explicit non-default policy or SupportsDefaultOutlining.
  // Prove the actual default-disabled policy, including the public getter's
  // current value; reject every explicit override, even an explicit "never".
  const bool DefaultNoOutlining = Outliner != Options.end() &&
      Outliner->second->getNumOccurrences() == 0 &&
      CG.EnableMachineOutliner == RunOutliner::TargetDefault &&
      !TM.Options.SupportsDefaultOutlining;
  const bool Good = T.getArch() == Triple::x86_64 && T.isOSLinux() &&
      TM.getOptLevel() == CodeGenOptLevel::Default &&
      TM.getTargetCPU() == "x86-64" && TM.getTargetFeatureString().empty() &&
      TM.getRelocationModel() == Reloc::PIC_ && TM.getCodeModel() == CodeModel::Small &&
      !TM.getPGOOption() && !TM.Options.EnableIPRA && !CG.EnableIPRA.value_or(false) &&
      DefaultNoOutlining && !TM.Options.EnableMachineFunctionSplitter &&
      !CG.EnableMachineFunctionSplitter && !CG.EnableGlobalMergeFunc &&
      TM.getBBSectionsType() == BasicBlockSection::None && !TM.getBBSectionsFuncListBuf();
  if (!Good && AnalysisDiagnostics) {
    std::fprintf(stderr, "ZIL cohort target decline: triple=%s cpu=%s features=%s opt=%u reloc=%u model=%u pgo=%u ipra=%u/%u outliner-capability=%u default-outlining=%u policy=%u occurrences=%d splitter=%u/%u global-merge=%u bbsections=%u list=%u\n",
        T.str().c_str(), TM.getTargetCPU().str().c_str(), TM.getTargetFeatureString().str().c_str(),
        static_cast<unsigned>(TM.getOptLevel()), static_cast<unsigned>(TM.getRelocationModel()),
        static_cast<unsigned>(TM.getCodeModel()), unsigned(bool(TM.getPGOOption())),
        unsigned(TM.Options.EnableIPRA), unsigned(CG.EnableIPRA.value_or(false)),
        unsigned(TM.Options.EnableMachineOutliner), unsigned(TM.Options.SupportsDefaultOutlining),
        static_cast<unsigned>(CG.EnableMachineOutliner),
        Outliner == Options.end() ? -1 : static_cast<int>(Outliner->second->getNumOccurrences()),
        unsigned(TM.Options.EnableMachineFunctionSplitter), unsigned(CG.EnableMachineFunctionSplitter),
        unsigned(CG.EnableGlobalMergeFunc), static_cast<unsigned>(TM.getBBSectionsType()),
        unsigned(TM.getBBSectionsFuncListBuf() != nullptr));
  }
  return Good;
}
std::string cohortContext(const Function &F) {
  auto Refuse = [&](StringRef Reason) -> std::string {
    if (AnalysisDiagnostics)
      std::fprintf(stderr, "ZIL cohort domain %.*s: %.*s\n", int(F.getName().size()), F.getName().data(), int(Reason.size()), Reason.data());
    return {};
  };
  const Module *M = F.getParent();
  // External definitions remain callable with arbitrary arguments. In
  // particular, internal/specialized/address-taken functions cannot substitute
  // this proof for the original incoming-caller/world comparison.
  if (!M || !compilerCohortName(F.getName()) || !F.hasExternalLinkage() ||
      F.hasSection() || F.getSectionPrefix() || F.isUsedByMetadata() || F.getAddressSpace() != 0 ||
      F.getVisibility() != GlobalValue::DefaultVisibility ||
      !eligibleFunction(F) || !M->global_empty() || !M->alias_empty() || !M->ifunc_empty() ||
      !M->getModuleInlineAsm().empty() || (M->getProfileSummary(false) || M->getProfileSummary(true)) ||
      F.getContext().getDiagnosticsHotnessRequested()) return Refuse("function/module envelope");
  // Name-sensitive instrumentation and target overrides have no authority
  // here. Ordinary inferred enum/int/type attributes remain in the exact
  // function projection and are compared rather than reconstructed.
  for (Attribute A : F.getAttributes().getFnAttrs()) {
    if (A.isStringAttribute()) return Refuse("string function attribute: " + A.getKindAsString().str());
    StringRef N = Attribute::getNameFromAttrKind(A.getKindAsEnum());
    if (N != "mustprogress" && N != "nofree" && N != "norecurse" &&
        N != "nosync" && N != "nounwind" && N != "willreturn" &&
        N != "memory" && N != "uwtable" && N != "noinline" &&
        N != "alwaysinline" && N != "inlinehint" && N != "optsize" &&
        N != "minsize" && N != "noreturn" && N != "speculatable" &&
        N != "nocallback" && N != "nobuiltin" && N != "nonlazybind") return Refuse("function attribute: " + N.str());
  }
  std::string S; raw_string_ostream OS(S);
  OS << "closed-external-scalar-v2;" << M->getTargetTriple().str() << ';' << M->getDataLayoutStr() << ';';
  for (const NamedMDNode &N : M->named_metadata()) {
    if (N.getName() != "llvm.module.flags") return Refuse("named metadata: " + N.getName().str());
    OS << N.getName() << ':' << N.getNumOperands() << ':';
    for (const MDNode *V : N.operands()) if (!cohortMetadata(V, OS)) return Refuse("module flag tree");
  }
  std::map<std::string, const Function *> Declarations;
  for (const BasicBlock &B : F) for (const Instruction &I : B) {
    if (!cohortType(I.getType())) return Refuse("instruction type");
    for (const Use &U : I.operands()) {
      if (isa<Function>(U.get())) continue;
      if (!cohortType(U->getType())) return Refuse("operand type");
    }
    if (const auto *C = dyn_cast<CallBase>(&I)) {
      for (Attribute A : C->getAttributes().getFnAttrs()) if (A.isStringAttribute()) return Refuse("string call attribute: " + A.getKindAsString().str());
      const auto *D = C->getCalledFunction();
      if (!D || !D->isDeclaration() || D->hasPersonalityFn() || D->hasGC() ||
          D->hasPrefixData() || D->hasPrologueData() || D->hasComdat() ||
          D->hasMetadata()) return Refuse("called declaration envelope");
      Declarations.emplace(D->getName().str(), D);
    }
  }
  for (const Function &D : *M) if (D.isIntrinsic()) {
    if (!D.isDeclaration() || D.hasMetadata() || D.isUsedByMetadata() ||
        D.hasPersonalityFn() || D.hasGC() || D.hasPrefixData() || D.hasPrologueData() || D.hasComdat()) return Refuse("module intrinsic declaration envelope");
    Declarations.emplace(D.getName().str(), &D);
  }
  for (const auto &D : Declarations) {
    std::string Text = cohortAttributeSlots(declarationText(*D.second), "\n; complete-attribute-values ");
    if (Text.empty()) return Refuse("declaration attribute-slot projection");
    OS << Text.size() << ':' << Text;
  }
  return S;
}
std::string cohortRebindFunctionText(const std::string &Text,
                                    const std::string &From, const std::string &To) {
  if (!compilerCohortName(From) ||
      (!compilerCohortName(To) && To != "zil.cohort.subject")) return {};
  size_t Start = Text.compare(0, 7, "define ") == 0 ? 0 : Text.find("\ndefine ");
  if (Start == std::string::npos) return {};
  if (Start) ++Start;
  size_t End = Text.find('\n', Start);
  if (End == std::string::npos) return {};
  const std::string Needle = "@" + From + "(";
  size_t At = Text.find(Needle, Start);
  if (At == std::string::npos || At >= End || Text.find(Needle, At + 1) < End) return {};
  std::string Result = Text;
  Result.replace(At + 1, From.size(), To);
  return To == "zil.cohort.subject" ?
      cohortAttributeSlots(Result, "\n; block-number-state ") : Result;
}
std::string cohortFunctionText(const Function &F) {
  return cohortRebindFunctionText(functionText(F), F.getName().str(), "zil.cohort.subject");
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
// Explicit same-thread counterexample observation only; no public ABI or
// product statistics. It counts the actual FAM visitor, not inferred savings.
thread_local uint64_t *AnalysisKeyVisitsForTests = nullptr;
Keys analysisKeys(FunctionAnalysisManager &AM, Function &F) {
  if (AnalysisKeyVisitsForTests) ++*AnalysisKeyVisitsForTests;
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
// A complete entry-key NEGATIVE filter, never a reuse receipt. Four ordered
// length/digest pairs cover every text column used by the original comparison;
// ordered typed EntryKeys are compared separately without encoding addresses.
// Even a forced digest collision must still decode and pass the full old Eq.
struct StageEntrySummary {
  std::array<size_t, 4> Lengths{};
  std::array<uint64_t, 4> Digests{};
  static std::array<const std::string *, 4> columns(const StageStrings &P) {
    return {&P.Input, &P.World, &P.Pipeline, &P.EntryFacts};
  }
  bool sameLengths(const StageStrings &P) const {
    auto Text = columns(P);
    for (size_t I = 0; I != Text.size(); ++I)
      if (Lengths[I] != Text[I]->size()) return false;
    return true;
  }
  static StageEntrySummary create(const StageStrings &P) {
    StageEntrySummary Result;
    auto Text = columns(P);
    for (size_t I = 0; I != Text.size(); ++I) {
      Result.Lengths[I] = Text[I]->size();
      Result.Digests[I] = xxh3_64bits(
          reinterpret_cast<const uint8_t *>(Text[I]->data()), Text[I]->size());
    }
    return Result;
  }
};
struct StageState {
  ClosureProof Proof = ClosureProof::WorldBound;
  // ClosedBody omits retained World only after complete producer checks.
  // WorldBound keeps the original exact Input/World permission.
  std::string CohortName, CohortContext, CohortConfig;
  uint64_t CohortKey = 0;
  Keys EntryKeys, ExitKeys;
  // A full function projection equality (not PA::all or a digest) proves that
  // this stage has no function state to transport. Analyses remain separate.
  bool BodyUnchanged = false;
  std::shared_ptr<const StageBody> TypedBody;
  unsigned OutputNextBlock = 0, OutputBlockEpoch = 0;
  AnalysisReplayState OutputAnalysisReplay;
  NameState OutputNames;
  std::vector<unsigned> OutputBlockNumbers;
  PreservedAnalyses PA = PreservedAnalyses::none();
};
// Pending visitors keep ordinary complete text. Resident records encode only
// the lossless text frame; every analysis/PA/name/BB field remains typed state.
struct Stage : StageState, StageStrings {
  // Entry text does not change during the stage. Reuse a computed miss summary
  // at retention rather than hashing the same current entry a second time.
  // If this hint were stale it still could only cause a miss/extra full check.
  std::optional<StageEntrySummary> EntrySummary;
};
struct PackedStage : StageState {
  StageEntrySummary EntrySummary;
  PackedStageStrings Text;
  size_t UnpackedBytes = 0, StoredBytes = 0;
};
// Economics is a refusal policy, never an alternative replay proof. Frozen
// modes change only when a successful Products epoch is published.
using EconomyClock = std::chrono::steady_clock;
enum class EconomyMode : uint8_t { Probe, Reuse, Bypass };
struct GroupEconomy {
  EconomyMode Mode = EconomyMode::Probe;
  uint8_t FreshSamples = 0, CaptureSamples = 0, HitSamples = 0;
  uint64_t FreshMin = std::numeric_limits<uint64_t>::max();
  uint64_t CaptureMax = 0, HitMax = 0, ReprobeEpoch = 0;
};
struct GroupRuntime {
  GroupEconomy Learned;
  EconomyMode Frozen = EconomyMode::Probe;
  unsigned KeyAttempts = 0, Captures = 0, FreshTimes = 0, ConsecutiveMisses = 0;
  bool Closed = false;
};
struct Products {
  // Admission-policy provenance only, never input/closure/replay authority.
  // It may propagate without an arena solely under exact complete Config and
  // a newly checked current target policy.
  bool ClosedPolicy = false;
  std::string Config;
  std::map<std::string, std::shared_ptr<const PackedStage>> Stages;
  std::shared_ptr<const MachineProducts> Machine;
  uint64_t SuccessfulEpoch = 0;
  std::map<std::string, GroupEconomy> Economics;
};
struct Pending {
  enum class Kind { TimeOnly, Capture, Diagnostic };
  Kind Mode = Kind::Diagnostic;
  std::string Slot;
  std::shared_ptr<Stage> Product;
  GroupRuntime *Group = nullptr; // stable Request-owned map node
  bool SampleFresh = false, SuppressesChildren = false;
  EconomyClock::time_point FreshStart{};
  uint64_t ObserverAtStart = 0, EntryNS = 0;
};
struct Request {
  Module &M; std::string Config;
  const Products *Prior;
  Products Next;
  ZilNativeProductStats Stats{};
  ZilNativeStageBodyStatsV1 BodyStats{};
  bool ClosedPolicy = false;
  CohortArena *Arena = nullptr; // request lifetime is held by the caller
  ZilNativeCohortStatsV1 CohortStats{};
  AnalysisFactsMemo AnalysisMemo;
  bool Failed = false;
  const bool AlwaysForTests = StageAlwaysForTests;
  unsigned Phase = 0;
  size_t ProductBytes = 0, UnpackedProductBytes = 0;
  size_t EconomyBytes = 0, PipelineHintBytes = 0;
  std::map<std::string, GroupRuntime> Groups;
  // Addresses here are only cheap economy hints within this Request. A PM
  // address may be recycled: the actual replay Pipeline is ALWAYS reprinted.
  std::map<const void *, std::string> PipelineHints;
  unsigned SuppressNestedCache = 0, ObserverDepth = 0;
  uint64_t ObserverNS = 0;
  bool ObserverClockValid = true;
  std::map<Pending *, std::unique_ptr<Pending>> PendingTokens;
  std::vector<std::pair<unsigned, unsigned>> Path;
  std::map<std::string, unsigned> Occurrences;
  ZilLLVMFunctionProductHooks Hooks{};
  const ZilLLVMFunctionProductHooks *Previous = nullptr;
  std::unique_ptr<MachineRequest, void (*)(MachineRequest *)> Machine{nullptr, nullptr};
  Request(Module &M, std::string C, const Products *P)
      : M(M), Config(std::move(C)), Prior(P) {
    Next.Config = Config;
    uint64_t Old = P && P->Config == Config ? P->SuccessfulEpoch : 0;
    Next.SuccessfulEpoch = Old == std::numeric_limits<uint64_t>::max() ? Old : Old + 1;
  }
};
uint64_t addEconomyNS(uint64_t A, uint64_t B) {
  return B > std::numeric_limits<uint64_t>::max() - A
             ? std::numeric_limits<uint64_t>::max() : A + B;
}
std::optional<uint64_t> elapsedEconomyNS(EconomyClock::time_point Start,
                                       EconomyClock::time_point End) {
  if (End <= Start) return std::nullopt;
  auto N = std::chrono::duration_cast<std::chrono::nanoseconds>(End - Start).count();
  if (N <= 0) return std::nullopt;
  return static_cast<uint64_t>(N);
}
// Count the UNION of nested observer intervals. Summing inclusive intervals
// would subtract a nested callback twice from a sampled original FPM runtime.
class ObserverScope {
  Request &R;
  EconomyClock::time_point Start = EconomyClock::now();
  bool Outer = false, Counted = false, Active = true;
public:
  ObserverScope(const ObserverScope &) = delete;
  ObserverScope &operator=(const ObserverScope &) = delete;
  explicit ObserverScope(Request &R) : R(R) {
    if (R.ObserverDepth >= 64) { R.ObserverClockValid = false; return; }
    Counted = true; Outer = R.ObserverDepth++ == 0;
  }
  void stop() {
    if (!Active) return;
    Active = false;
    if (Counted) --R.ObserverDepth;
    if (Outer) {
      auto NS = elapsedEconomyNS(Start, EconomyClock::now());
      if (!NS) R.ObserverClockValid = false;
      else {
        R.ObserverNS = addEconomyNS(R.ObserverNS, *NS);
        if (R.ObserverNS == std::numeric_limits<uint64_t>::max()) R.ObserverClockValid = false;
      }
    }
  }
  ~ObserverScope() { stop(); }
};
Pending *newPending(Request &R, std::string Slot = {}, std::shared_ptr<Stage> Product = {},
                    GroupRuntime *Group = nullptr, bool SampleFresh = false) {
  if (R.PendingTokens.size() >= 128) return nullptr;
  auto P = std::make_unique<Pending>(); P->Slot = std::move(Slot); P->Product = std::move(Product);
  P->Mode = P->Product ? Pending::Kind::Capture : Group ? Pending::Kind::TimeOnly : Pending::Kind::Diagnostic;
  P->Group = Group; P->SampleFresh = SampleFresh;
  Pending *Raw = P.get(); R.PendingTokens.emplace(Raw, std::move(P)); return Raw;
}
// Optional resident products have a bounded encoded payload and slot count.
// Reaching either cap only discards a candidate; it never changes compilation,
// verification, optimization policy or the compiler's existing work budget.
constexpr size_t MaxStageProductBytes = 64 * 1024 * 1024;
constexpr size_t MaxStageProductSlots = 4096;
bool addStageBytes(size_t &Bytes, size_t N) {
  if (Bytes > MaxStageProductBytes || N > MaxStageProductBytes - Bytes) return false;
  Bytes += N; return true;
}
bool addStageElements(size_t &Bytes, size_t Count, size_t Width) {
  return Count <= MaxStageProductBytes / Width && addStageBytes(Bytes, Count * Width);
}
bool stageMetadataBytes(const StageState &P, size_t &Bytes) {
  return addStageBytes(Bytes, P.CohortName.size()) &&
         addStageBytes(Bytes, P.CohortContext.size()) &&
         addStageBytes(Bytes, P.CohortConfig.size()) &&
         (!P.TypedBody || addStageBytes(Bytes, P.TypedBody->ownedBytes())) &&
         addStageElements(Bytes, P.EntryKeys.size(), sizeof(void *)) &&
         addStageElements(Bytes, P.ExitKeys.size(), sizeof(void *)) &&
         addStageElements(Bytes, P.OutputBlockNumbers.size(), sizeof(unsigned));
}
bool unpackedStageBytes(const Stage &P, size_t &Bytes) {
  Bytes = 0;
  if (!addStageBytes(Bytes, sizeof(Stage)) || !stageMetadataBytes(P, Bytes)) return false;
  for (const auto *S : {&P.Input, &P.World, &P.Pipeline, &P.Output,
                        &P.OutputText, &P.EntryFacts, &P.ExitFacts})
    if (!addStageBytes(Bytes, S->size())) return false;
  for (const auto &D : P.OriginalDeclarations)
    if (!addStageBytes(Bytes, D.first.size()) || !addStageBytes(Bytes, D.second.size())) return false;
  return true;
}
bool stageProofShape(const Stage &P) {
  switch (P.Proof) {
  case ClosureProof::WorldBound: return !P.World.empty();
  case ClosureProof::ClosedBody:
    return P.World.empty() && !P.CohortContext.empty() && !P.CohortConfig.empty() &&
        !P.CohortName.empty() && P.Output.empty() && (P.BodyUnchanged || P.TypedBody);
  }
  return false;
}
bool unpackStage(const PackedStage &Packed, Stage &Out) {
  if (Packed.UnpackedBytes > MaxStageProductBytes || Packed.StoredBytes > MaxStageProductBytes) return false;
  Stage Current;
  if (!Packed.Text.unpack(Current, MaxStageProductBytes)) return false;
  static_cast<StageState &>(Current) = static_cast<const StageState &>(Packed);
  size_t Bytes;
  if (!stageProofShape(Current) || !unpackedStageBytes(Current, Bytes) || Bytes != Packed.UnpackedBytes) return false;
  Out = std::move(Current); return true;
}
bool entryMayMatch(const PackedStage &Prior, Stage &Current) {
  if (Prior.EntryKeys != Current.EntryKeys || !Prior.EntrySummary.sameLengths(Current)) return false;
  Current.EntrySummary = StageEntrySummary::create(Current);
  return Prior.EntrySummary.Digests == Current.EntrySummary->Digests;
}
bool exactStageEntry(const Stage &Prior, const Stage &Current) {
  return Prior.Proof == ClosureProof::WorldBound && Current.Proof == ClosureProof::WorldBound &&
         !Prior.World.empty() && !Current.World.empty() &&
         Prior.Input == Current.Input && Prior.World == Current.World &&
         Prior.Pipeline == Current.Pipeline && Prior.EntryKeys == Current.EntryKeys &&
         Prior.EntryFacts == Current.EntryFacts;
}
bool exactCohortStageEntry(const Stage &Prior, const Stage &Current,
                           const std::string &CurrentCanonical) {
  if (Prior.Proof != ClosureProof::ClosedBody || !stageProofShape(Prior) ||
      Current.CohortContext.empty() || Prior.CohortContext != Current.CohortContext ||
      Prior.CohortConfig != Current.CohortConfig || Prior.Pipeline != Current.Pipeline ||
      Prior.EntryKeys != Current.EntryKeys || Prior.EntryFacts != Current.EntryFacts) return false;
  return !CurrentCanonical.empty() &&
      cohortRebindFunctionText(Prior.Input, Prior.CohortName, "zil.cohort.subject") == CurrentCanonical;
}
bool exactCohortStageEntry(const Stage &Prior, const Stage &Current) {
  return exactCohortStageEntry(Prior, Current,
      cohortRebindFunctionText(Current.Input, Current.CohortName, "zil.cohort.subject"));
}
// Called only after exact current Config and Slot lookup. False leaves IR/AM
// untouched and executes the original stage. AttemptedDecode is a fixture-only
// observation proving a negative filter really avoided the full packed read.
bool decodeStageCandidate(const PackedStage &Prior, Stage &Current, Stage &Decoded,
                          ZilNativeProductStats &Stats, bool *AttemptedDecode = nullptr) {
  if (AttemptedDecode) *AttemptedDecode = false;
  if (Prior.Proof != ClosureProof::WorldBound || Current.Proof != ClosureProof::WorldBound || Current.World.empty()) return false;
  { IRScopeNS<> KeyTime(Stats.ir_key_ns); if (!entryMayMatch(Prior, Current)) return false; }
  if (AttemptedDecode) *AttemptedDecode = true;
  { IRScopeNS<> UnpackTime(Stats.ir_unpack_ns); if (!unpackStage(Prior, Decoded)) return false; }
  // A summary MATCH cannot authorize anything, including on a collision or a
  // malformed/private hint. Preserve every original complete entry comparison.
  return exactStageEntry(Decoded, Current);
}

void retainStage(Request &R, const std::string &Slot, std::shared_ptr<const PackedStage> P) {
  if (R.Next.Stages.size() >= MaxStageProductSlots || R.Next.Stages.count(Slot)) return;
  size_t Raw = P->UnpackedBytes, Stored = P->StoredBytes;
  if (!addStageBytes(Raw, Slot.size()) || !addStageBytes(Stored, Slot.size()) ||
      Raw > MaxStageProductBytes - R.UnpackedProductBytes ||
      Stored > MaxStageProductBytes - R.ProductBytes) return;
  auto Held = P;
  R.Next.Stages.emplace(Slot, std::move(P));
  if (R.Arena && Held->Proof == ClosureProof::ClosedBody && !Held->CohortContext.empty() &&
      (Held->BodyUnchanged || Held->TypedBody))
    R.Arena->publish(0, Held->CohortKey, Held, &R.CohortStats);
  // Preserve BOTH the original aggregate decoded admission and the bounded
  // actual encoded payload+typed metadata. This is not an RSS measurement.
  R.UnpackedProductBytes += Raw; R.ProductBytes += Stored;
}
void retainStage(Request &R, const std::string &Slot, std::shared_ptr<const Stage> Product) {
  // The caller charges publication to fresh capture or replay. The complete
  // packing/retention and local cleanup finish inside that caller's scope.
  auto P = std::move(Product);
  size_t Raw;
  if (R.Next.Stages.size() >= MaxStageProductSlots || R.Next.Stages.count(Slot) ||
      !stageProofShape(*P) || !unpackedStageBytes(*P, Raw)) return;
  size_t WithSlot = Raw;
  if (!addStageBytes(WithSlot, Slot.size()) ||
      WithSlot > MaxStageProductBytes - R.UnpackedProductBytes) return;
  auto Text = PackedStageStrings::pack(*P, MaxStageProductBytes);
  if (!Text) return;
  auto Packed = std::make_shared<PackedStage>();
  static_cast<StageState &>(*Packed) = static_cast<const StageState &>(*P);
  Packed->Text = std::move(*Text); Packed->UnpackedBytes = Raw;
  Packed->EntrySummary = P->EntrySummary ? *P->EntrySummary : StageEntrySummary::create(*P);
  size_t Stored = 0;
  if (!addStageBytes(Stored, sizeof(PackedStage)) ||
      !stageMetadataBytes(*Packed, Stored) || !addStageBytes(Stored, Packed->Text.encodedSize())) return;
  Packed->StoredBytes = Stored;
  retainStage(R, Slot, std::shared_ptr<const PackedStage>(std::move(Packed)));
}

std::string pipeline(FunctionPassManager &PM) {
  std::string S; raw_string_ostream OS(S);
  PM.printPipeline(OS, [](StringRef N) { return N; }); return S;
}
std::string path(Request &R, Function &F) {
  std::string P; raw_string_ostream OS(P);
  OS << "fpm-slot-v3/" << F.getName().size() << ':' << F.getName();
  for (auto E : R.Path) OS << '/' << E.first << ':' << E.second;
  OS.flush(); auto &N = R.Occurrences[P]; return P + "#" + std::to_string(N++);
}
// These are hard optional-cache bounds, not compiler work-budget changes.
constexpr size_t MaxEconomyGroups = 128;
constexpr size_t MaxEconomyBytes = 512 * 1024;
constexpr size_t MaxPipelineHints = 128;
constexpr size_t MaxPipelineHintBytes = 128 * 1024;
constexpr unsigned PilotKeys = 2, PilotCaptures = 2, FreshSamplesPerRequest = 4;

bool validEconomy(const GroupEconomy &P) {
  return (P.Mode == EconomyMode::Probe || P.Mode == EconomyMode::Reuse ||
          P.Mode == EconomyMode::Bypass) &&
         (P.Mode == EconomyMode::Probe ||
          (P.FreshSamples == 2 && P.CaptureSamples == 2 && P.HitSamples == 2)) &&
         P.FreshSamples <= 2 && P.CaptureSamples <= 2 && P.HitSamples <= 2 &&
         (!P.FreshSamples || (P.FreshMin && P.FreshMin != std::numeric_limits<uint64_t>::max())) &&
         (!P.CaptureSamples || P.CaptureMax) && (!P.HitSamples || P.HitMax);
}
GroupRuntime beginEconomy(const GroupEconomy *Prior, uint64_t Epoch) {
  GroupRuntime G;
  if (Prior && validEconomy(*Prior)) G.Learned = *Prior;
  if (G.Learned.Mode == EconomyMode::Bypass && Epoch >= G.Learned.ReprobeEpoch)
    G.Learned = GroupEconomy();
  G.Frozen = G.Learned.Mode;
  return G;
}
bool sampleFresh(GroupRuntime &G) {
  if (G.Frozen == EconomyMode::Bypass || G.FreshTimes >= FreshSamplesPerRequest) return false;
  ++G.FreshTimes;
  return true;
}
bool tryEconomyKey(GroupRuntime &G) {
  if (G.Closed || G.Frozen == EconomyMode::Bypass) return false;
  if (G.Frozen == EconomyMode::Probe && G.KeyAttempts >= PilotKeys) return false;
  if (G.Frozen == EconomyMode::Probe) ++G.KeyAttempts;
  return true;
}
bool tryEconomyCapture(GroupRuntime &G) {
  if (G.Captures >= PilotCaptures) return false;
  ++G.Captures; // failed admission does not refund the bounded opportunity
  return true;
}
void economyMiss(GroupRuntime &G) {
  G.ConsecutiveMisses = std::min(2U, G.ConsecutiveMisses + 1);
  if (G.ConsecutiveMisses >= 2 && G.Frozen == EconomyMode::Reuse) G.Closed = true;
}
void economyHit(GroupRuntime &G, std::optional<uint64_t> NS) {
  G.ConsecutiveMisses = 0;
  if (!NS) return;
  G.Learned.HitSamples = std::min<unsigned>(2, G.Learned.HitSamples + 1);
  G.Learned.HitMax = std::max(G.Learned.HitMax, *NS);
}
void economyFresh(GroupRuntime &G, std::optional<uint64_t> NS) {
  if (!NS) return;
  G.Learned.FreshSamples = std::min<unsigned>(2, G.Learned.FreshSamples + 1);
  G.Learned.FreshMin = std::min(G.Learned.FreshMin, *NS);
}
void economyCapture(GroupRuntime &G, uint64_t EntryNS, std::optional<uint64_t> CaptureNS) {
  if (!EntryNS || !CaptureNS) return;
  G.Learned.CaptureSamples = std::min<unsigned>(2, G.Learned.CaptureSamples + 1);
  G.Learned.CaptureMax = std::max(G.Learned.CaptureMax, addEconomyNS(EntryNS, *CaptureNS));
}
GroupEconomy freezeEconomy(const GroupRuntime &G, uint64_t Epoch) {
  auto P = G.Learned;
  if (!validEconomy(P)) return GroupEconomy();
  if (G.Frozen == EconomyMode::Bypass) return P;
  if (P.FreshSamples < 2 || P.CaptureSamples < 2 || P.HitSamples < 2) {
    P.Mode = EconomyMode::Probe;
    return P;
  }
  // Compare without multiplication overflow. Saturation can only refuse a
  // useful cache; it cannot grant permission to bypass any LLVM pass.
  const bool WorthReuse = P.HitMax <= P.FreshMin / 2 &&
      P.CaptureMax <= addEconomyNS(P.FreshMin - P.HitMax, P.FreshMin - P.HitMax);
  P.Mode = WorthReuse ? EconomyMode::Reuse : EconomyMode::Bypass;
  P.ReprobeEpoch = WorthReuse ? 0 : addEconomyNS(Epoch, 8);
  return P;
}
std::string economyGroupKey(Request &R, FunctionPassManager &PM, const Function &F) {
  auto It = R.PipelineHints.find(&PM);
  std::string Printed;
  const std::string *Hint;
  if (It != R.PipelineHints.end()) Hint = &It->second;
  else {
    Printed = pipeline(PM);
    Hint = &Printed;
    if (R.PipelineHints.size() < MaxPipelineHints &&
        Printed.size() <= MaxPipelineHintBytes - R.PipelineHintBytes) {
      R.PipelineHintBytes += Printed.size();
      Hint = &R.PipelineHints.emplace(&PM, Printed).first->second;
    }
  }
  if (Hint->size() > MaxEconomyBytes / 2) return {};
  uint64_t Count = F.getInstructionCount();
  unsigned Bucket = 0;
  while (Count > 1) { Count >>= 1; ++Bucket; }
  std::string Key;
  raw_string_ostream OS(Key);
  OS << "fpm-economy-v1/" << Hint->size() << ':' << *Hint << "/bucket:" << Bucket;
  for (auto E : R.Path) OS << '/' << E.first << ':' << E.second;
  OS.flush();
  return Key.size() <= MaxEconomyBytes / 2 ? Key : std::string();
}
GroupRuntime *economyGroup(Request &R, const std::string &Key) {
  if (Key.empty()) return nullptr;
  auto Found = R.Groups.find(Key);
  if (Found != R.Groups.end()) return &Found->second;
  size_t Bytes = sizeof(std::pair<const std::string, GroupEconomy>) + 4 * sizeof(void *);
  if (R.Groups.empty()) Bytes += sizeof(R.Next.SuccessfulEpoch) + sizeof(R.Next.Economics);
  if (!addStageBytes(Bytes, Key.size()) || R.Groups.size() >= MaxEconomyGroups ||
      Bytes > MaxEconomyBytes - R.EconomyBytes ||
      Bytes > MaxStageProductBytes - R.ProductBytes ||
      Bytes > MaxStageProductBytes - R.UnpackedProductBytes) return nullptr;
  const GroupEconomy *Prior = nullptr;
  if (R.Prior && R.Prior->Config == R.Config) {
    auto It = R.Prior->Economics.find(Key);
    if (It != R.Prior->Economics.end()) Prior = &It->second;
  }
  auto G = beginEconomy(Prior, R.Next.SuccessfulEpoch);
  R.EconomyBytes += Bytes; R.ProductBytes += Bytes; R.UnpackedProductBytes += Bytes;
  R.Next.Economics.emplace(Key, G.Learned);
  return &R.Groups.emplace(Key, std::move(G)).first->second;
}
void freezeEconomics(Request &R) {
  for (const auto &E : R.Groups)
    R.Next.Economics.at(E.first) = R.ObserverClockValid
        ? freezeEconomy(E.second, R.Next.SuccessfulEpoch) : GroupEconomy();
}

// CloneFunctionInto preserves values/CFG but does not promise use-list order.
// LLVM analyses and scheduling can observe that order. Transport only the
// function-owned values; constants and symbol users remain current owners.
bool preserveLocalUseOrder(const Function &Source, ValueToValueMapTy &Map,
                           size_t *RankTables = nullptr) {
  auto Restore = [&](const Value *V) {
    Value *New = Map.lookup(V);
    if (!New) return false;
    // Prove the complete current mapped sequence before allocating a rank
    // table. Most cloned values have zero/one use, or already have the exact
    // source order. Neither case needs sorting. Owner/operand checks remain
    // current, and an extra destination use cannot pass this equality.
    auto Next = New->use_begin();
    bool Same = true;
    for (const Use &U : V->uses()) {
      auto *NewUser = dyn_cast_or_null<User>(Map.lookup(U.getUser()));
      if (!NewUser || U.getOperandNo() >= NewUser->getNumOperands()) return false;
      const Use *NewUse = &NewUser->getOperandUse(U.getOperandNo());
      if (NewUse->get() != New) return false;
      if (Next == New->use_end() || &*Next != NewUse) { Same = false; break; }
      ++Next;
    }
    if (Same && Next == New->use_end()) return true;
    if (RankTables) ++*RankTables;
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
bool restoreStageAnalyses(Function &F, FunctionAnalysisManager &AM, const Stage &P,
                          AnalysisFactsMemo *Memo, uint64_t &Reconstructions,
                          bool CurrentIsEntry) {
  auto Fail = [&](const char *Site) {
    if (AnalysisDiagnostics) std::fprintf(stderr, "ZIL FPM replay-failure %.*s: %s\n", static_cast<int>(F.getName().size()), F.getName().data(), Site);
    return false;
  };
  // beforeStage has just bound the complete CURRENT entry keys/facts. If the
  // retained exit is byte-for-byte that same supported state, keeping those
  // current analyses is exact: no factory or historical owner is replayed.
  if (CurrentIsEntry && P.EntryKeys == P.ExitKeys && P.EntryFacts == P.ExitFacts)
    return true;
  for (const void *K : P.ExitKeys)
    if (!symbols().has(&AM, K)) return Fail("exit-analysis-registration");
  if (CurrentIsEntry) AM.clear(F, F.getName());
  for (const void *K : P.ExitKeys) { symbols().require(&AM, &F, K); ++Reconstructions; }
  if (!restoreAnalysisFacts(F, AM, P.OutputAnalysisReplay)) return Fail("empty-analysis-state-restore");
  std::string Facts;
  if (analysisKeys(AM, F) != P.ExitKeys) return Fail("exit-analysis-availability-order");
  if (!analysisFacts(F, AM, P.ExitKeys, Facts, nullptr, Memo)) return Fail("exit-analysis-domain");
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
  return true;
}

// Sufficient verification for the owner-free transport, not schema validation
// in place of LLVM. The old miniature module contains only this function and
// these canonical declarations, with identical triple/DL and no global,
// alias/ifunc, named metadata or COMDAT module-final work. The current verifier
// can be stricter because of additional actual users; refusal keeps the exact
// old snapshot/verifyModule/bitcode producer below.
bool typedProducerDeclarationsVerified(const Function &F) {
  std::set<const Function *> Seen;
  SmallVector<std::pair<unsigned, MDNode *>, 4> Metadata;
  for (const BasicBlock &B : F) for (const Instruction &I : B)
    if (const auto *Call = dyn_cast<CallBase>(&I)) {
      const Function *D = Call->getCalledFunction();
      if (!D) return false;
      if (!Seen.insert(D).second) continue;
      Metadata.clear(); D->getAllMetadata(Metadata);
      if (!D->isDeclaration() || !approvedIntrinsic(*Call) || !Metadata.empty() ||
          D->hasPersonalityFn() || D->hasGC() || D->hasPrefixData() || D->hasPrologueData() ||
          D->hasComdat() || D->isUsedByMetadata() || verifyFunction(*D, nullptr)) return false;
    }
  return true;
}

bool captureStageBody(Stage &P, Function &F, ZilNativeProductStats &Stats,
                      std::string Output = {}, ZilNativeStageBodyStatsV1 *BodyStats = nullptr) {
  if (Output.empty()) Output = functionText(F);
  if (Output.empty()) return false;
  if (Output == P.Input) {
    // Snapshot cloning used to verify the transported body. There is no
    // transport now, but retain the same current function validity check.
    if (verifyFunction(F, nullptr)) return false;
    // Keep only the input's complete bytes. No bitcode clone, serialization,
    // second output-text copy, old declaration map or numbering payload.
    P.BodyUnchanged = true; return true;
  }
  P.OutputText = std::move(Output);
  for (const BasicBlock &B : F) for (const Instruction &I : B)
    if (const auto *Call = dyn_cast<CallBase>(&I)) {
      const Function *D = Call->getCalledFunction();
      // This capture walk cannot mutate callees. emplace still evaluates an
      // expensive declarationText argument when the key already exists.
      auto Name = D->getName().str();
      if (!P.OriginalDeclarations.count(Name))
        P.OriginalDeclarations.emplace(std::move(Name), declarationText(*D));
    }
  P.OutputNames = nameState(F);
  if (P.OutputNames.present < 0) return false;
  P.OutputNextBlock = F.getMaxBlockNumber(); P.OutputBlockEpoch = F.getBlockNumberEpoch();
  for (const BasicBlock &B : F) P.OutputBlockNumbers.push_back(B.getNumber());
  if (BodyStats) {
    IRScopeNS<> CaptureTime(BodyStats->capture_ns);
    if (typedProducerDeclarationsVerified(F)) P.TypedBody = StageBody::capture(F, MaxStageProductBytes);
    if (P.TypedBody) {
      ++BodyStats->captures; BodyStats->captured_bytes += P.TypedBody->ownedBytes();
    } else ++BodyStats->declines;
  }
  if (!P.TypedBody) P.Output = snapshot(F);
  if (!P.TypedBody && P.Output.empty()) return false;
  Stats.ir_snapshot_bytes += P.Output.size() + P.OutputText.size() +
      (P.TypedBody ? P.TypedBody->ownedBytes() : 0);
  return true;
}

bool replay(Request &R, Function &F, FunctionAnalysisManager &AM, const Stage &P,
            std::string *CohortCurrentOutput = nullptr) {
  IRScopeNS<> ReplayTime(R.Stats.ir_replay_ns);
  auto Fail = [&](const char *Site) {
    if (AnalysisDiagnostics) std::fprintf(stderr, "ZIL FPM replay-failure %.*s: %s\n", static_cast<int>(F.getName().size()), F.getName().data(), Site);
    return false;
  };
  if (!stageProofShape(P) ||
      (CohortCurrentOutput ? P.Proof != ClosureProof::ClosedBody : P.Proof != ClosureProof::WorldBound))
    return Fail("replay-closure-permission");
  if (P.BodyUnchanged) {
    if (verifyFunction(F, nullptr)) return Fail("unchanged-current-function-verifier");
    // Current input/stage/config/entry analyses and the selected WorldBound
    // or ClosedBody permission were independently matched in beforeStage.
    // Retain the current Function and all of its Values/use-lists/name/BB
    // state; only the independently bound analysis transition may need work.
    return restoreStageAnalyses(F, AM, P, &R.AnalysisMemo,
                                R.Stats.analysis_reconstructions, true);
  }
  auto FinishRestored = [&](Function *Source, ValueToValueMapTy *Map, size_t Bytes) {
    if (!symbols().restoreNumbers(&F, P.OutputNextBlock, P.OutputBlockEpoch,
                                 P.OutputBlockNumbers.data(), P.OutputBlockNumbers.size())) return Fail("block-number-restore");
    if (Source && !preserveLocalUseOrder(*Source, *Map)) return Fail("local-use-order");
    if (verifyFunction(F, nullptr)) return Fail("current-function-verifier");
    if (!symbols().restoreNames(&F, P.OutputNames.present, P.OutputNames.counter, P.OutputNames.maximum)) return Fail("value-name-state-restore");
    std::string CurrentOutput = functionText(F);
    if (CohortCurrentOutput) {
      // Cross-identity transport changes only own symbol and module-local
      // attribute-reference numbers. Compare the COMPLETE alpha projection,
      // including every actual attribute value and the exact binary use order.
      auto Expected = cohortRebindFunctionText(P.OutputText, F.getName().str(), "zil.cohort.subject");
      auto Current = cohortRebindFunctionText(CurrentOutput, F.getName().str(), "zil.cohort.subject");
      if (Expected.empty() || Current != Expected) return Fail("complete-cohort-output-text");
    } else if (CurrentOutput != P.OutputText) return Fail("complete-output-text");
    if (!restoreStageAnalyses(F, AM, P, &R.AnalysisMemo,
                              R.Stats.analysis_reconstructions, false)) return false;
    if (CohortCurrentOutput) *CohortCurrentOutput = std::move(CurrentOutput);
    R.Stats.ir_replay_bytes += Bytes; return true;
  };
  if (P.TypedBody) {
    if (!P.Output.empty()) return Fail("mixed-typed-bitcode-transport");
    // Full current declaration projections are the same authority as the old
    // parsed miniature-module path. A name or intrinsic ID alone is insufficient.
    for (const auto &Entry : P.OriginalDeclarations) {
      Function *D = R.M.getFunction(Entry.first);
      if (!D || !D->isDeclaration() || !D->isIntrinsic() || declarationText(*D) != Entry.second)
        return Fail("typed-current-original-declaration-binding");
    }
    std::unique_ptr<StageBody::Prepared> Prepared;
    { IRScopeNS<> PrepareTime(R.BodyStats.prepare_ns); Prepared = P.TypedBody->prepare(F); }
    if (!Prepared) { ++R.BodyStats.failed_replays; return Fail("typed-prepare"); }
    for (const void *K : P.ExitKeys) if (!symbols().has(&AM, K)) return Fail("exit-analysis-registration");
    AM.clear(F, F.getName());
    bool Restored;
    { IRScopeNS<> RestoreTime(R.BodyStats.restore_ns); Restored = P.TypedBody->restore(F, *Prepared); }
    if (!Restored || !FinishRestored(nullptr, nullptr, P.TypedBody->ownedBytes())) {
      ++R.BodyStats.failed_replays; return Fail("typed-current-body-or-exit");
    }
    ++R.BodyStats.replays; return true;
  }
  IRScopeNS<> ParseTime(R.Stats.ir_parse_ns);
  // P owns immutable bytes throughout parsing and the parsed Module's lifetime.
  // The non-lazy reader needs a buffer wrapper, not another bitcode allocation.
  auto Buffer = MemoryBuffer::getMemBuffer(P.Output, "zil.function.product", false);
  auto Parsed = parseBitcodeFile(Buffer->getMemBufferRef(), F.getContext());
  ParseTime.stop(); // Parsed Module/Buffer cleanup remains in replay only.
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
  return FinishRestored(SF, &Map, P.Output.size());
}
// The wrapper measures after all entry locals have been destroyed. Successful
// replay has no matching after callback in LLVM; it must leave NO pending token.
int beforeStageImpl(Request &R, FunctionPassManager &PM, Function &F,
                    FunctionAnalysisManager &AM, PreservedAnalyses &PA,
                    void **Token, GroupRuntime **HitGroup, std::string Slot) {
  if (R.Failed) { PA = PreservedAnalyses::none(); return 1; }
  if (R.SuppressNestedCache) {
    ++R.Stats.optimizer_function_stage_executions;
    return 0; // sample the ORIGINAL nested pipeline, not a child replay
  }
  Keys Entry;
  // Diagnostics deliberately retains the original complete entry listing,
  // including unsupported and economically refused groups. The ordinary
  // refusal path does not enumerate FAM or manufacture an unused key set.
  if (AnalysisDiagnostics) {
    Entry = analysisKeys(AM, F);
    describeAnalyses(AM, F, "entry", Entry);
    if (!supportedAnalysisKeys(Entry)) {
      *Token = newPending(R);
      ++R.Stats.optimizer_function_stage_executions; return 0;
    }
  }
  IRScopeNS<> KeyTime(R.Stats.ir_key_ns);
  std::string ClosedContext = R.ClosedPolicy && (!R.Arena || !R.Arena->read().disabled) ? cohortContext(F) : std::string{};
  if (R.Arena && ClosedContext.empty()) R.Arena->decline(&R.CohortStats);
  const bool Closed = !ClosedContext.empty();
  GroupRuntime *Group = R.AlwaysForTests ? nullptr : economyGroup(R, economyGroupKey(R, PM, F));
  // Every refusal is ordinary execution. No missing/bad economic hint grants
  // replay permission or changes PA, the original pass list, or FAM state.
  if (!Closed && !R.AlwaysForTests && !R.ObserverClockValid) {
    ++R.Stats.optimizer_function_stage_executions; return 0;
  }
  if (!Closed && !R.AlwaysForTests && (!Group || !tryEconomyKey(*Group))) {
    if (Group && sampleFresh(*Group)) *Token = newPending(R, {}, {}, Group, true);
    else if (AnalysisDiagnostics) *Token = newPending(R);
    ++R.Stats.optimizer_function_stage_executions; return 0;
  }
  if (!AnalysisDiagnostics) {
    Entry = analysisKeys(AM, F);
    if (!supportedAnalysisKeys(Entry)) {
      ++R.Stats.optimizer_function_stage_executions; return 0;
    }
  }
  // Sampling suppresses child stages. A closed-cohort lookup/capture must not
  // silently make a child ineligible merely because its parent was sampled.
  const bool Sample = !Closed && Group && sampleFresh(*Group);
  auto FreshWithoutCapture = [&]() {
    if (Sample) *Token = newPending(R, {}, {}, Group, true);
    else if (AnalysisDiagnostics) *Token = newPending(R);
    ++R.Stats.optimizer_function_stage_executions;
    return 0;
  };
  if (!eligibleFunction(F)) {
    if (Group) economyMiss(*Group);
    return FreshWithoutCapture();
  }
  std::string Facts;
  if (!analysisFacts(F, AM, Entry, Facts, nullptr, &R.AnalysisMemo)) {
    if (Group) economyMiss(*Group);
    return FreshWithoutCapture();
  }
  auto P = std::make_shared<Stage>();
  // The address-indexed group Pipeline is ONLY an economic hint. This fresh
  // complete Pipeline remains the original authoritative entry comparison.
  P->Pipeline = pipeline(PM);
  P->Input = functionText(F);
  P->EntryKeys = std::move(Entry); P->EntryFacts = std::move(Facts);
  if (P->Input.empty()) {
    if (Group) economyMiss(*Group);
    return FreshWithoutCapture();
  }
  R.Stats.ir_key_bytes += P->Input.size() + P->Pipeline.size() + P->EntryFacts.size() + R.Config.size();
  std::string CanonicalInput;
  if (Closed) {
    P->CohortName = F.getName().str();
    P->CohortContext = std::move(ClosedContext); P->CohortConfig = R.Config;
    // Bind the actual enclosing pass-manager positions and repeated visit as
    // well as the complete FPM spelling. Only the function identity differs.
    P->CohortConfig += ";cohort-position-v1";
    for (auto E : R.Path) P->CohortConfig += "/" + std::to_string(E.first) + ":" + std::to_string(E.second);
    const auto Occurrence = Slot.rfind('#');
    if (Occurrence == std::string::npos) P->CohortContext.clear();
    else P->CohortConfig += Slot.substr(Occurrence);
    CanonicalInput = cohortRebindFunctionText(P->Input, P->CohortName, "zil.cohort.subject");
    if (CanonicalInput.empty()) P->CohortContext.clear();
    else P->CohortKey = cohortHash(CanonicalInput, P->CohortContext, P->CohortConfig, P->Pipeline);
  }
  KeyTime.stop();
  // Closed prior and peer products share the same complete consumer proof.
  // Origin affects only accounting: a prior hit must not become a peer event.
  auto TryClosed = [&](const std::shared_ptr<const PackedStage> &Candidate, bool Peer) {
    if (!R.ClosedPolicy || (R.Arena && R.Arena->read().disabled) || P->CohortContext.empty() || Candidate->Proof != ClosureProof::ClosedBody ||
        Candidate->CohortContext != P->CohortContext ||
        Candidate->CohortConfig != P->CohortConfig || Candidate->EntryKeys != P->EntryKeys ||
        (!Candidate->BodyUnchanged && !Candidate->TypedBody)) return false;
    Stage Old;
    // This projection belongs only to this unchanged entry and synchronous
    // lookup. It is still compared in full against every decoded candidate.
    if (!unpackStage(*Candidate, Old) || !exactCohortStageEntry(Old, *P, CanonicalInput)) return false;
    auto Output = Old.BodyUnchanged ? std::string{} :
        cohortRebindFunctionText(Old.OutputText, Old.CohortName, P->CohortName);
    if (!Old.BodyUnchanged && Output.empty()) return false;
    bool SameStoredText = Old.CohortName == P->CohortName && Old.Input == P->Input &&
        (Old.BodyUnchanged || Output == Old.OutputText);
    // The typed consumer's ordinary declaration guard stays exact after a
    // complete value comparison and rebinding to CURRENT serialized slots.
    for (auto &D : Old.OriginalDeclarations) {
      const Function *Current = R.M.getFunction(D.first);
      if (!Current || !Current->isDeclaration() || !Current->isIntrinsic()) return false;
      std::string Text = declarationText(*Current);
      auto Expected = cohortAttributeSlots(D.second, "\n; complete-attribute-values ");
      if (Expected.empty() || cohortAttributeSlots(Text, "\n; complete-attribute-values ") != Expected) return false;
      SameStoredText &= Text == D.second;
      D.second = std::move(Text);
    }
    // Selection is complete: every subsequent exit consumes this candidate,
    // including a dirty replay failure. Transfer current bindings, not copies.
    Old.Input = std::move(P->Input); Old.OutputText = std::move(Output);
    Old.CohortName = std::move(P->CohortName); Old.EntrySummary.reset();
    std::string CurrentOutput;
    // No pass executes on this path. The closed typed/unchanged transport and
    // supported analysis factories only update current F/FAM; complete output
    // and current context remain independently checked. No outside World is
    // read or synthesized as a replay receipt.
    if (!replay(R, F, AM, Old, &CurrentOutput) || cohortContext(F) != P->CohortContext) {
      R.Failed = true; ++R.Stats.failed_replays;
      if (R.Arena) R.Arena->disable(&R.CohortStats);
      PA = PreservedAnalyses::none(); return true;
    }
    if (!Old.BodyUnchanged) {
      SameStoredText &= CurrentOutput == Old.OutputText;
      Old.OutputText = std::move(CurrentOutput);
    }
    PA = Old.PA;
    { // Sequential with replay(): retention/repacking belongs to replay.
      IRScopeNS<> RetentionTime(R.Stats.ir_replay_ns);
      // Entry/output/declarations and every typed field are unchanged. Share
      // the original immutable frame only AFTER the complete current replay
      // and context checks; normalized equality alone cannot skip rebinding.
      if (SameStoredText) retainStage(R, Slot, Candidate);
      else retainStage(R, Slot, std::make_shared<const Stage>(std::move(Old)));
    }
    if (Peer) R.Arena->replayed(0, &R.CohortStats);
    else *HitGroup = Group;
    ++R.Stats.optimizer_function_stage_reuses; return true;
  };
  if (!P->CohortContext.empty()) {
    if (R.Prior && R.Prior->Config == R.Config) {
      auto It = R.Prior->Stages.find(Slot);
      if (It != R.Prior->Stages.end() && TryClosed(It->second, false)) return 1;
    }
    if (R.Arena) for (auto &Value : R.Arena->candidates(0, P->CohortKey, &R.CohortStats))
      if (TryClosed(std::static_pointer_cast<const PackedStage>(Value), true)) return 1;
  }
  // Only a miss/unsupported closure enters the original whole-World lane.
  // Pending captures always carry this complete pre-state; afterStage proves
  // the actual original FPM's side effects before any ClosedBody is sealed.
  {
    IRScopeNS<> MissWorldTime(R.Stats.ir_key_ns);
    P->World = worldText(R.M, &F, &R.Stats);
    R.Stats.ir_key_bytes += P->World.size();
  }
  if (P->World.empty()) {
    if (Group) economyMiss(*Group);
    return FreshWithoutCapture();
  }
  if (R.Prior && R.Prior->Config == R.Config) {
    auto It = R.Prior->Stages.find(Slot);
    if (It != R.Prior->Stages.end()) {
      Stage Old;
      if (decodeStageCandidate(*It->second, *P, Old, R.Stats)) {
        if (!replay(R, F, AM, Old)) {
          R.Failed = true; ++R.Stats.failed_replays;
          PA = PreservedAnalyses::none(); return 1;
        }
        describeAnalyses(AM, F, "replayed-exit", analysisKeys(AM, F));
        PA = Old.PA;
        {
          IRScopeNS<> RetentionTime(R.Stats.ir_replay_ns);
          retainStage(R, Slot, It->second);
        }
        *HitGroup = Group;
        ++R.Stats.optimizer_function_stage_reuses; return 1;
      }
    }
  }
  if (Group) economyMiss(*Group); // absent Slot is an economic miss too
  ++R.Stats.optimizer_function_stage_executions;
  if (Closed || R.AlwaysForTests || (Group && tryEconomyCapture(*Group)))
    *Token = newPending(R, std::move(Slot), std::move(P), Group, Sample);
  else if (Sample) *Token = newPending(R, {}, {}, Group, true);
  else if (AnalysisDiagnostics) *Token = newPending(R);
  return 0;
}
int beforeStage(void *Context, void *PM, void *IR, void *Manager, void *, void *PA, void **Token) {
  auto &R = *static_cast<Request *>(Context);
  const auto Start = EconomyClock::now();
  ObserverScope Observer(R);
  auto &F = *static_cast<Function *>(IR);
  *Token = nullptr;
  if (F.getParent() != &R.M || R.Phase != 1) return 0;
  // Advance every effective hook, even suppressed/unsupported/bypassed ones.
  // Omitting a refused group would silently renumber later repeated Slots.
  std::string Slot = path(R, F);
  GroupRuntime *Hit = nullptr;
  int Result = beforeStageImpl(R, *static_cast<FunctionPassManager *>(PM), F,
      *static_cast<FunctionAnalysisManager *>(Manager),
      *static_cast<PreservedAnalyses *>(PA), Token, &Hit, std::move(Slot));
  Observer.stop();
  auto EntryNS = elapsedEconomyNS(Start, EconomyClock::now());
  if (!EntryNS) R.ObserverClockValid = false;
  if (Hit && R.ObserverClockValid) economyHit(*Hit, EntryNS);
  auto It = R.PendingTokens.find(static_cast<Pending *>(*Token));
  if (It != R.PendingTokens.end()) {
    auto &P = *It->second;
    P.EntryNS = EntryNS.value_or(0);
    if (P.SampleFresh) {
      P.ObserverAtStart = R.ObserverNS;
      P.FreshStart = EconomyClock::now();
      P.SuppressesChildren = true;
      ++R.SuppressNestedCache;
    }
  }
  return Result;
}
// Entry eligibility was freshly established before the original stage. A
// complete current equality of every field it read is a producer proof, not a
// Function* identity memo. Unknown type projections refuse the product.
// Current owners/hidden projection state are checked
// independently before equality can be used.
bool projectAdmittedStageExit(const Module &M, const Function &F, const Stage &P,
                             ZilNativeProductStats *Stats, std::string &Output,
                             bool *ReusedEligibility = nullptr) {
  if (ReusedEligibility) *ReusedEligibility = false;
  if (P.Proof != ClosureProof::WorldBound || P.World.empty() || !incomingCallersHaveCurrentOwners(F)) return false;
  std::string World = worldText(M, &F, Stats);
  if (World.empty() || World != P.World) return false;
  bool TypeReadsBound = false;
  Output = projectFunctionText(F, &TypeReadsBound);
  if (Output.empty()) return false;
  if (TypeReadsBound && Output == P.Input) {
    if (ReusedEligibility) *ReusedEligibility = true;
    return true;
  }
  return eligibleFunction(F);
}

// The caller has already erased the token and ended the original-pass sample.
// TimeOnly cannot synthesize a missing entry after transformations have run.
bool afterStageImpl(Request &R, Function &F, FunctionAnalysisManager &AM,
                    const PreservedAnalyses &PA, Pending &P) {
  if (R.Failed) return false;
  if (P.Mode == Pending::Kind::TimeOnly) return false;
  Keys Exit = analysisKeys(AM, F);
  describeAnalyses(AM, F, "exit", Exit);
  if (P.Mode != Pending::Kind::Capture || !P.Product || !supportedAnalysisKeys(Exit)) return false;
  std::string Output;
  if (!projectAdmittedStageExit(R.M, F, *P.Product, &R.Stats, Output)) return false;
  P.Product->ExitKeys = std::move(Exit);
  if (!analysisFacts(F, AM, P.Product->ExitKeys, P.Product->ExitFacts,
                    &P.Product->OutputAnalysisReplay, &R.AnalysisMemo)) return false;
  if (!captureStageBody(*P.Product, F, R.Stats, std::move(Output), &R.BodyStats)) return false;
  P.Product->PA = PA;
  if (!P.Product->CohortContext.empty()) {
    if (cohortContext(F) == P.Product->CohortContext &&
        (P.Product->BodyUnchanged || P.Product->TypedBody) && P.Product->Output.empty()) {
      // projectAdmittedStageExit just proved COMPLETE pre/post World equality
      // around the actual original FPM. Seal only the owner-free closed body;
      // the large temporary World is neither packed nor retained as a fake key.
      P.Product->Proof = ClosureProof::ClosedBody;
      std::string().swap(P.Product->World);
      P.Product->EntrySummary.reset();
    } else P.Product->CohortContext.clear();
  }
  size_t Before = R.Next.Stages.size();
  { // Observer v3: packing is a child of this afterStage capture only.
    IRScopeNS<> PackTime(R.Stats.ir_pack_ns);
    retainStage(R, P.Slot, std::move(P.Product));
  }
  return R.Next.Stages.size() > Before;
}
void afterStage(void *Context, void *, void *IR, void *Manager, const void *PA, void *Token) {
  const auto End = EconomyClock::now(); // exclude THIS observer/capture from F
  auto &R = *static_cast<Request *>(Context);
  ObserverScope Observer(R);
  IRScopeNS<> CaptureTime(R.Stats.ir_capture_ns);
  auto It = R.PendingTokens.find(static_cast<Pending *>(Token));
  if (It == R.PendingTokens.end()) return;
  auto P = std::move(It->second); R.PendingTokens.erase(It);
  if (P->SuppressesChildren) {
    --R.SuppressNestedCache;
    P->SuppressesChildren = false;
    auto Elapsed = elapsedEconomyNS(P->FreshStart, End);
    // ObserverNS is the union, so nested before/after callbacks are deducted
    // once. All nested optional cache paths were suppressed during this FPM.
    if (!R.Failed && P->Group && Elapsed && R.ObserverClockValid &&
        R.ObserverNS >= P->ObserverAtStart) {
      uint64_t ChildNS = R.ObserverNS - P->ObserverAtStart;
      if (*Elapsed > ChildNS) economyFresh(*P->Group, *Elapsed - ChildNS);
    }
  }
  const auto CaptureStart = End;
  auto *Group = P->Group;
  uint64_t EntryNS = P->EntryNS;
  bool Captured = afterStageImpl(R, *static_cast<Function *>(IR),
      *static_cast<FunctionAnalysisManager *>(Manager),
      *static_cast<const PreservedAnalyses *>(PA), *P);
  P.reset(); // include full pending/source cleanup in C, even on early refusal
  if (Captured && Group && !R.Failed && R.ObserverClockValid)
    economyCapture(*Group, EntryNS, elapsedEconomyNS(CaptureStart, EconomyClock::now()));
}
void enterPass(void *C, unsigned Kind, unsigned Index) {
  auto &R = *static_cast<Request *>(C);
  std::optional<ObserverScope> Observer;
  if (R.SuppressNestedCache) Observer.emplace(R);
  R.Path.emplace_back(Kind, Index);
  if (Kind == 0) ++R.Stats.optimizer_module_pass_executions;
  if (Kind == 2) ++R.Stats.optimizer_cgscc_pass_executions;
}
void leavePass(void *C) {
  auto &R = *static_cast<Request *>(C);
  std::optional<ObserverScope> Observer;
  if (R.SuppressNestedCache) Observer.emplace(R);
  if (!R.Path.empty()) R.Path.pop_back();
}
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
  if(R.Machine) {
    emission::Stats MC;
    readMachineEmissionStats(*R.Machine,&MC,sizeof(MC));
    emission::saveLastStats(MC);
  }
  if (Out) *Out = R.Stats;
}
}
}
extern "C" int ZilNativeCohortCounterexamplesV1() {
  using namespace zil;
  auto Fail = [](const char *Site) { std::fprintf(stderr, "ZIL cohort counterexample: %s\n", Site); return 0; };
  LLVMContext Context; Module M("cohort-current-owners", Context);
  Type *I32 = Type::getInt32Ty(Context);
  auto Make = [&](const char *Name, unsigned N) {
    auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, Name, M);
    F->getArg(0)->setName("arg.0");
    auto *B = BasicBlock::Create(Context, "entry", F);
    auto *Add = BinaryOperator::CreateAdd(F->getArg(0), ConstantInt::get(I32, N), "sum", B);
    ReturnInst::Create(Context, Add, B); return F;
  };
  Function *A = Make("zil.fn.1", 7), *B = Make("zil.fn.2", 7), *Different = Make("zil.fn.3", 9);
  if (verifyModule(M, nullptr) || cohortContext(*A).empty() || cohortContext(*A) != cohortContext(*B) ||
      cohortFunctionText(*A).empty() || cohortFunctionText(*A) != cohortFunctionText(*B) ||
      cohortFunctionText(*A) == cohortFunctionText(*Different)) return Fail("complete-current-projection");
  Stage X, Y;
  X.Proof = ClosureProof::ClosedBody; X.BodyUnchanged = true;
  X.Input = functionText(*A); X.CohortName = A->getName().str(); X.CohortContext = cohortContext(*A);
  X.CohortConfig = "fixed-current-policy/position"; X.Pipeline = "actual-pipeline"; X.EntryFacts = "complete-analysis-facts";
  Y = X; Y.Input = functionText(*B); Y.CohortName = B->getName().str();
  if (!exactCohortStageEntry(X, Y)) return Fail("different-current-name");
  if (exactStageEntry(X, Y)) return Fail("closed-is-not-world-bound");
  Stage Unknown = X; Unknown.Proof = ClosureProof::WorldBound;
  if (stageProofShape(Unknown) || exactCohortStageEntry(Unknown, Y) || exactStageEntry(Unknown, Unknown))
    return Fail("empty-world-is-not-permission");
  Unknown.World = "complete current world";
  if (!stageProofShape(Unknown) || exactCohortStageEntry(Unknown, Y)) return Fail("world-bound-is-not-closed");
  Unknown.Proof = ClosureProof::ClosedBody;
  if (stageProofShape(Unknown) || exactCohortStageEntry(Unknown, Y)) return Fail("mixed-closure-world-shape");
  Request Transport(M, "closure-fixture", nullptr);
  retainStage(Transport, "closed", std::make_shared<const Stage>(X));
  auto Stored = Transport.Next.Stages.find("closed");
  Stage Decoded;
  if (Stored == Transport.Next.Stages.end() || !unpackStage(*Stored->second, Decoded) ||
      Decoded.Proof != ClosureProof::ClosedBody || !Decoded.World.empty() ||
      !exactCohortStageEntry(Decoded, Y)) return Fail("closed-pack-preserves-permission");
  Stage WorldCurrent = Y; WorldCurrent.Proof = ClosureProof::WorldBound;
  WorldCurrent.World = "complete current world";
  bool AttemptedDecode = true;
  if (decodeStageCandidate(*Stored->second, WorldCurrent, Decoded, Transport.Stats, &AttemptedDecode) || AttemptedDecode)
    return Fail("world-consumer-refuses-closed-before-unpack");
  // Actual fresh capture and no-arena replay: prove immutable frame sharing,
  // then force a same-name ModuleSlotTracker change. Alpha-equivalent input
  // still replays but MUST publish a new frame with current serialized slots.
  {
    Module OriginalModule("closed-frame-owner", Context), CurrentModule("closed-frame-current", Context);
    auto Leaf = [&](Module &Owner) {
      auto *F = Function::Create(FunctionType::get(I32, {I32}, false),
          GlobalValue::ExternalLinkage, "zil.fn.71", Owner);
      F->addFnAttr(Attribute::NoInline);
      auto *BB = BasicBlock::Create(Context, "entry", F);
      ReturnInst::Create(Context, F->getArg(0), BB); return F;
    };
    Function *Original = Leaf(OriginalModule);
    auto *Prefix = Function::Create(FunctionType::get(I32, false),
        GlobalValue::ExternalLinkage, "prefix", CurrentModule);
    Prefix->addFnAttr(Attribute::NoUnwind);
    ReturnInst::Create(Context, ConstantInt::get(I32, 0), BasicBlock::Create(Context, "entry", Prefix));
    Function *Current = Leaf(CurrentModule);
    if (functionText(*Original) == functionText(*Current) ||
        cohortFunctionText(*Original) != cohortFunctionText(*Current)) return Fail("same-name-slot-fixture");
    FunctionPassManager PM;
    FunctionAnalysisManager AM;
    AM.registerPass([] { return PassInstrumentationAnalysis(); });
    AM.getResult<PassInstrumentationAnalysis>(*Original);
    Request Producer(OriginalModule, "closed-retention", nullptr); Producer.ClosedPolicy = true;
    void *Token = nullptr; GroupRuntime *Hit = nullptr;
    PreservedAnalyses PA = PreservedAnalyses::none();
    const auto Slot = path(Producer, *Original);
    if (beforeStageImpl(Producer, PM, *Original, AM, PA, &Token, &Hit, Slot) || !Token)
      return Fail("closed-frame-fresh-entry");
    PA = PM.run(*Original, AM);
    afterStage(&Producer, nullptr, Original, &AM, &PA, Token);
    auto Product = Producer.Next.Stages.find(Slot);
    if (Producer.Failed || !Producer.PendingTokens.empty() || Product == Producer.Next.Stages.end() ||
        Product->second->Proof != ClosureProof::ClosedBody || !Producer.Stats.world_projection_direct)
      return Fail("closed-frame-fresh-world-proof");
    auto Consume = [&](Module &Owner, Function &F, bool SameFrame) {
      FunctionAnalysisManager CurrentAM;
      CurrentAM.registerPass([] { return PassInstrumentationAnalysis(); });
      CurrentAM.getResult<PassInstrumentationAnalysis>(F);
      Request Consumer(Owner, "closed-retention", &Producer.Next); Consumer.ClosedPolicy = true;
      void *Pending = nullptr; GroupRuntime *Group = nullptr;
      PreservedAnalyses Result = PreservedAnalyses::none();
      if (beforeStageImpl(Consumer, PM, F, CurrentAM, Result, &Pending, &Group, Slot) != 1 ||
          Consumer.Failed || Pending || Consumer.Stats.optimizer_function_stage_reuses != 1 ||
          Consumer.Stats.world_projection_direct || Consumer.Stats.world_projection_fallback ||
          Consumer.Next.Stages.size() != 1 ||
          (Consumer.Next.Stages.at(Slot).get() == Product->second.get()) != SameFrame) return false;
      Stage Rebound;
      return unpackStage(*Consumer.Next.Stages.at(Slot), Rebound) && Rebound.Input == functionText(F) &&
          Rebound.CohortName == F.getName() && !verifyFunction(F, nullptr);
    };
    if (!Consume(OriginalModule, *Original, true)) return Fail("closed-frame-shares-exact-product");
    if (!Consume(CurrentModule, *Current, false)) return Fail("closed-frame-rebinds-changed-slots");
  }
  Y.Input = functionText(*Different); Y.CohortName = Different->getName().str();
  if (exactCohortStageEntry(X, Y)) return Fail("forced-hash-collision-body");
  Y = X; Y.CohortConfig += "changed";
  if (exactCohortStageEntry(X, Y)) return Fail("configuration");
  Y = X; Y.EntryFacts += "changed";
  if (exactCohortStageEntry(X, Y)) return Fail("analysis-facts");
  Y = X; Y.Pipeline += "changed";
  if (exactCohortStageEntry(X, Y)) return Fail("pipeline");
  Y = X; Y.CohortContext += "changed";
  if (exactCohortStageEntry(X, Y)) return Fail("world-domain");
  const bool OriginalDSOLocal = B->isDSOLocal();
  B->setLinkage(GlobalValue::InternalLinkage);
  if (!cohortContext(*B).empty()) return Fail("internal-caller-specialization");
  // LLVM setLinkage(Internal) sets DSOLocal; switching back to External does
  // not clear it. Restore the fixture header before testing body transport,
  // whose contract deliberately preserves the CURRENT function's DSOLocal.
  B->setLinkage(GlobalValue::ExternalLinkage); B->setDSOLocal(OriginalDSOLocal);
  B->addFnAttr("instrument-function-entry", "hook");
  if (!cohortContext(*B).empty()) return Fail("name-sensitive-instrumentation");
  B->removeFnAttr("instrument-function-entry"); B->setSection("special");
  if (!cohortContext(*B).empty()) return Fail("section-policy");
  B->setSection("");
  if (cohortFunctionText(*A) != cohortFunctionText(*B)) return Fail("fixture-header-reset");
  auto Body = StageBody::capture(*A, MaxStageProductBytes);
  auto Prepared = Body ? Body->prepare(*B) : nullptr;
  auto Names = nameState(*A); std::vector<unsigned> Numbers;
  for (const BasicBlock &BB : *A) Numbers.push_back(BB.getNumber());
  if (!Body || !Prepared || !Body->restore(*B, *Prepared) ||
      !symbols().restoreNumbers || !symbols().restoreNames ||
      !symbols().restoreNumbers(B, A->getMaxBlockNumber(), A->getBlockNumberEpoch(), Numbers.data(), Numbers.size()) ||
      !symbols().restoreNames(B, Names.present, Names.counter, Names.maximum) ||
      verifyFunction(*B, nullptr) || B->getName() != "zil.fn.2" ||
      cohortFunctionText(*A) != cohortFunctionText(*B)) return Fail("typed-current-owner-restore");
  // A different actual Module's attribute table is not function state.
  // Prefix gets #0; the identical peer leaf gets #1 instead of A's #0.
  A->addFnAttr(Attribute::NoInline); A->getArg(0)->setName("arg.#7");
  Module PeerModule("cohort-attribute-slots", Context);
  auto *Prefix = Function::Create(FunctionType::get(I32, {}, false), GlobalValue::ExternalLinkage, "prefix", PeerModule);
  Prefix->addFnAttr(Attribute::NoUnwind);
  ReturnInst::Create(Context, ConstantInt::get(I32, 0), BasicBlock::Create(Context, "entry", Prefix));
  auto *Peer = Function::Create(A->getFunctionType(), GlobalValue::ExternalLinkage, "zil.fn.222", PeerModule);
  Peer->addFnAttr(Attribute::NoInline); Peer->getArg(0)->setName("arg.#7");
  auto *PeerBB = BasicBlock::Create(Context, "entry", Peer);
  auto *PeerAdd = BinaryOperator::CreateAdd(Peer->getArg(0), ConstantInt::get(I32, 7), "sum", PeerBB);
  ReturnInst::Create(Context, PeerAdd, PeerBB);
  auto *DA = Intrinsic::getOrInsertDeclaration(&M, Intrinsic::sadd_with_overflow, {I32});
  auto *DB = Intrinsic::getOrInsertDeclaration(&PeerModule, Intrinsic::sadd_with_overflow, {I32});
  if (verifyModule(M, nullptr) || verifyModule(PeerModule, nullptr) ||
      cohortRebindFunctionText(functionText(*A), A->getName().str(), Peer->getName().str()) == functionText(*Peer) ||
      declarationText(*DA) == declarationText(*DB) ||
      cohortFunctionText(*A).empty() || cohortFunctionText(*A) != cohortFunctionText(*Peer) ||
      cohortContext(*A).empty() || cohortContext(*A) != cohortContext(*Peer)) return Fail("module-local-attribute-slots");
  const auto BeforeQuote = cohortFunctionText(*Peer);
  Peer->getArg(0)->setName("arg.#8");
  if (cohortFunctionText(*Peer) == BeforeQuote) return Fail("quoted-name-not-attribute-reference");
  Peer->getArg(0)->setName("arg.#7"); Peer->addFnAttr(Attribute::NoUnwind);
  if (cohortFunctionText(*A) == cohortFunctionText(*Peer)) return Fail("actual-attribute-value-difference");
  const std::string Footer = "\n; block-number-state 1:0\n; complete-function-attributes value\n; owned-use-sequence-v1\n";
  const std::string PrefixText = "define i32 @zil.fn.1() #91 {\n  ret i32 7\n}\n";
  std::string Binary = Footer + std::string("\0 #91\0", 7);
  auto Normal = cohortAttributeSlots(PrefixText + Binary, "\n; block-number-state ");
  if (Normal.empty() || Normal.substr(Normal.find(Footer)) != Binary ||
      !cohortAttributeSlots("define i32 @zil.fn.1() #-1 {\n}" + Footer, "\n; block-number-state ").empty())
    return Fail("attribute-slots-binary-tail-and-invalid-ref");
  CohortArena Arena; ZilNativeCohortStatsV1 Local{};
  auto Owner = std::make_shared<const unsigned>(7);
  std::weak_ptr<const unsigned> Weak = Owner;
  Arena.publish(0, 42, Owner, &Local);
  if (Owner.use_count() != 1 || Arena.candidates(1, 42, &Local).size() ||
      Arena.candidates(0, 43, &Local).size()) return Fail("weak-kind-hash-index");
  { auto Found = Arena.candidates(0, 42, &Local);
    if (Found.size() != 1 || Found.front().get() != Owner.get()) return Fail("lookup-strong-lifetime"); }
  Owner.reset();
  if (!Weak.expired() || !Arena.candidates(0, 42, &Local).empty() || !Local.expired_entries)
    return Fail("expired-owner");
  auto Another = std::make_shared<const unsigned>(9);
  Arena.disable(&Local); Arena.publish(0, 42, Another, &Local);
  if (!Arena.candidates(0, 42, &Local).empty() || !Arena.read().disabled ||
      Arena.read().indexed_entries || !Local.disabled || Local.indexed_entries != 1)
    return Fail("disabled-publication-local-events");
  if (!cohortMachineHeaderCounterexamples()) return Fail("machine-header-rebinding");
  return 1;
}
extern "C" void *ZilNativeCohortArenaCreate() { return new zil::CohortArena(); }
extern "C" void ZilNativeCohortArenaDispose(void *P) { delete static_cast<zil::CohortArena *>(P); }
extern "C" int ZilNativeCohortArenaReadStatsV1(const void *P, void *Out, size_t N) {
  if (!P || !Out || N != sizeof(ZilNativeCohortStatsV1)) return 0;
  auto S = static_cast<const zil::CohortArena *>(P)->read(); std::memcpy(Out, &S, N); return 1;
}
extern "C" int ZilNativeProductsReadCohortStatsV1(const void *P, void *Out, size_t N) {
  if (!P || !Out || N != sizeof(ZilNativeCohortStatsV1)) return 0;
  const auto &R = *static_cast<const zil::Request *>(P);
  auto S = R.CohortStats;
  if (R.Arena) S.disabled = R.Arena->read().disabled;
  std::memcpy(Out, &S, N); return 1;
}
extern "C" void *ZilNativeProductsCreateRequest(void *M, void *TM, const void *Prior, const char *C, size_t N) {
  return ZilNativeProductsCreateRequestWithCohorts(M, TM, Prior, C, N, nullptr);
}
extern "C" void *ZilNativeProductsCreateRequestWithCohorts(void *M, void *TM, const void *Prior,
                                                         const char *C, size_t N, void *Arena) {
  using namespace zil;
  if (!M || !TM || !symbols().available || (!C && N)) return nullptr;
  auto R = std::make_unique<Request>(*static_cast<Module *>(M), std::string(C ? C : "", N), static_cast<const Products *>(Prior));
  R->Stats.available = 1;
  const bool InheritedPolicy = R->Prior && R->Prior->ClosedPolicy && R->Prior->Config == R->Config;
  if (Arena || InheritedPolicy) {
    if (cohortTarget(*static_cast<TargetMachine *>(TM))) {
      R->ClosedPolicy = true;
      R->Arena = static_cast<CohortArena *>(Arena);
    } else if (Arena) static_cast<CohortArena *>(Arena)->decline(&R->CohortStats);
  }
  R->Next.ClosedPolicy = R->ClosedPolicy;
  R->Machine = createMachine(R->M, *static_cast<TargetMachine *>(TM), R->Config,
      R->Prior && R->Prior->Config == R->Config ? R->Prior->Machine : nullptr,
      R->Stats, R->Failed, R->Arena, &R->CohortStats, R->ClosedPolicy);
  if (!R->Machine) return nullptr;
  R->Hooks = {2, R.get(), beforeMachineCallback, enterPass, leavePass, beforeStage, afterStage, skippedCallback, requestFailedCallback};
  R->Previous = symbols().set(&R->Hooks);
  installMachineEmission(*R->Machine,&R->Hooks);
  return R.release();
}
extern "C" int ZilNativeProductsStageAlwaysForTests(int Enabled) {
  bool Previous = zil::StageAlwaysForTests;
  zil::StageAlwaysForTests = Enabled != 0;
  return Previous;
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
  if (!R->PendingTokens.empty() || R->SuppressNestedCache || R->ObserverDepth) { R->Failed = true; ++R->Stats.failed_replays; }
  if (R->Failed) { if (R->Arena) R->Arena->disable(&R->CohortStats); copyStats(*R, S); return nullptr; }
  if (R->Machine) R->Next.Machine = finishMachine(*R->Machine);
  if (R->Failed) {
    if (R->Arena) R->Arena->disable(&R->CohortStats);
    if (!R->Stats.failed_replays) ++R->Stats.failed_replays;
    copyStats(*R, S); return nullptr;
  }
  freezeEconomics(*R);
  R->Stats.retained_bytes += R->ProductBytes;
  copyStats(*R, S);
  auto *Result = new Products(std::move(R->Next));
  if (Failed) *Failed = 0;
  return Result;
}
extern "C" void ZilNativeProductsAbort(void *P, ZilNativeProductStats *S) {
  using namespace zil; std::unique_ptr<Request> R(static_cast<Request *>(P)); if (!R) return;
  symbols().set(R->Previous);
  if (R->Arena) R->Arena->disable(&R->CohortStats);
  copyStats(*R, S);
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
  ValueToValueMapTy Map;
  // CloneFunction inserts into M: the module owns the clone's lifetime.
  Function *Copy = CloneFunction(F, Map);
  if (!zil::preserveLocalUseOrder(*F, Map)) return 0;
  size_t RankTables = 0;
  if (!zil::preserveLocalUseOrder(*F, Map, &RankTables) || RankTables ||
      verifyFunction(*Copy, nullptr)) return 0;
  Copy->getArg(0)->reverseUseList();
  if (!zil::preserveLocalUseOrder(*F, Map, &RankTables) || !RankTables) return 0;
  auto *MappedA = cast<Instruction>(Map.lookup(A));
  Map.erase(A);
  if (zil::preserveLocalUseOrder(*F, Map)) return 0;
  Map[A] = MappedA;
  auto *Operand = MappedA->getOperand(0);
  MappedA->setOperand(0, Builder.getInt32(3));
  if (zil::preserveLocalUseOrder(*F, Map)) return 0;
  MappedA->setOperand(0, Operand);
  auto *Extra = BinaryOperator::CreateAdd(Copy->getArg(0), Builder.getInt32(4),
                                         "extra", Copy->getEntryBlock().getTerminator()->getIterator());
  if (zil::preserveLocalUseOrder(*F, Map)) return 0;
  Extra->eraseFromParent();
  if (!zil::preserveLocalUseOrder(*F, Map) || verifyFunction(*Copy, nullptr)) return 0;
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
  const auto OriginalLinkage = F->getLinkage();
  const auto OriginalVisibility = F->getVisibility();
  const auto OriginalDLLStorage = F->getDLLStorageClass();
  const bool OriginalDSOLocal = F->isDSOLocal();
  F->setLinkage(GlobalValue::InternalLinkage);
  if (Headers() == Key) return 0;
  // Internal linkage also changes DSOLocal. Restore the entire header state
  // before asserting the same projection, rather than hiding a real change.
  F->setLinkage(OriginalLinkage);
  F->setVisibility(OriginalVisibility);
  F->setDLLStorageClass(OriginalDLLStorage);
  F->setDSOLocal(OriginalDSOLocal);
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

extern "C" int ZilNativeProductsUnchangedStageCounterexample() {
  if (!zil::symbols().available) return 0;
  LLVMContext Context; Module M("unchanged-stage", Context);
  auto *I32 = Type::getInt32Ty(Context);
  auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "f", &M);
  F->getArg(0)->setName("x");
  IRBuilder<> B(BasicBlock::Create(Context, "entry", F));
  auto *Return = B.CreateRet(F->getArg(0));
  auto Register = [](FunctionAnalysisManager &AM) {
    AM.registerPass([] { return PassInstrumentationAnalysis(); });
    AM.registerPass([] { return DominatorTreeAnalysis(); });
  };
  FunctionAnalysisManager Cold; Register(Cold);
  Cold.getResult<PassInstrumentationAnalysis>(*F);
  zil::Stage P; P.Input = zil::functionText(*F);
  P.EntryKeys = zil::analysisKeys(Cold, *F);
  if (!zil::analysisFacts(*F, Cold, P.EntryKeys, P.EntryFacts)) return 0;
  FunctionPassManager PM;
  PM.addPass(RequireAnalysisPass<DominatorTreeAnalysis, Function>());
  P.PA = PM.run(*F, Cold); // actual unchanged-body stage with a new analysis
  P.ExitKeys = zil::analysisKeys(Cold, *F);
  if (!zil::analysisFacts(*F, Cold, P.ExitKeys, P.ExitFacts, &P.OutputAnalysisReplay)) return 0;
  ZilNativeProductStats Stats{};
  if (!zil::captureStageBody(P, *F, Stats) || !P.BodyUnchanged ||
      !P.Output.empty() || !P.OutputText.empty() || !P.OriginalDeclarations.empty() ||
      !P.OutputBlockNumbers.empty() || Stats.ir_snapshot_bytes ||
      P.EntryKeys == P.ExitKeys || !P.PA.areAllPreserved()) return 0;
  FunctionAnalysisManager Current; Register(Current);
  Current.getResult<PassInstrumentationAnalysis>(*F);
  std::string Facts;
  if (zil::analysisKeys(Current, *F) != P.EntryKeys ||
      !zil::analysisFacts(*F, Current, P.EntryKeys, Facts) || Facts != P.EntryFacts) return 0;
  zil::AnalysisFactsMemo Memo; uint64_t Rebuilt = 0;
  auto *OriginalBlock = &F->getEntryBlock(); auto *OriginalArgument = F->getArg(0);
  if (!zil::restoreStageAnalyses(*F, Current, P, &Memo, Rebuilt, true) || !Rebuilt ||
      zil::functionText(*F) != P.Input || &F->getEntryBlock() != OriginalBlock ||
      F->getArg(0) != OriginalArgument || F->getEntryBlock().getTerminator() != Return) return 0;
  // A stage that changes neither IR nor supported analyses must retain the
  // current actual result object, not destroy/reconstruct it gratuitously.
  zil::Stage Identity = P; Identity.EntryKeys = P.ExitKeys; Identity.EntryFacts = P.ExitFacts;
  auto *CurrentTree = Current.getCachedResult<DominatorTreeAnalysis>(*F);
  const uint64_t Before = Rebuilt;
  if (!zil::restoreStageAnalyses(*F, Current, Identity, &Memo, Rebuilt, true) ||
      Rebuilt != Before || Current.getCachedResult<DominatorTreeAnalysis>(*F) != CurrentTree) return 0;
  // Different hidden naming state is NOT an unchanged product, even though
  // official Function::print does not expose this counter.
  auto Names = zil::nameState(*F);
  if (Names.present < 0 || !zil::symbols().restoreNames(F, Names.present, Names.counter + uint32_t(1), Names.maximum) ||
      zil::functionText(*F) == P.Input ||
      !zil::symbols().restoreNames(F, Names.present, Names.counter, Names.maximum)) return 0;
  if (zil::functionText(*F) != P.Input) return 0;
  // Equality of the function body does not authorize a different analysis
  // exit. Keep the exact facts comparison and let the full native caller retry.
  zil::Stage Corrupt = P; Corrupt.ExitFacts += "not-the-current-analysis-state";
  if (zil::restoreStageAnalyses(*F, Current, Corrupt, &Memo, Rebuilt, true) ||
      zil::functionText(*F) != P.Input || verifyModule(M, nullptr)) return 0;
  return 1;
}

extern "C" int ZilNativeProductsEligibilityProducerCounterexample() {
  if (!zil::symbols().available) return 0;
  LLVMContext Context; Module M("eligibility-producer", Context);
  auto *I32 = Type::getInt32Ty(Context);
  auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "f", &M);
  F->getArg(0)->setName("x");
  IRBuilder<> FB(BasicBlock::Create(Context, "entry", F)); FB.CreateRet(F->getArg(0));
  auto *Caller = Function::Create(FunctionType::get(I32, false), GlobalValue::ExternalLinkage, "caller", &M);
  IRBuilder<> CB(BasicBlock::Create(Context, "entry", Caller));
  auto *Call = CB.CreateCall(F, {CB.getInt32(7)}, "call"); CB.CreateRet(Call);
  if (verifyModule(M, nullptr) || !zil::eligibleFunction(*F)) return 0;
  zil::Stage P; P.Input = zil::functionText(*F); P.World = zil::worldText(M, F);
  bool Reused = false; std::string Output;
  if (P.Input.empty() || P.World.empty() ||
      !zil::projectAdmittedStageExit(M, *F, P, nullptr, Output, &Reused) || !Reused) return 0;
  Call->setArgOperand(0, CB.getInt32(8));
  if (zil::functionText(*F) != P.Input || !zil::eligibleFunction(*F) ||
      zil::projectAdmittedStageExit(M, *F, P, nullptr, Output, &Reused) || Reused) return 0;
  Call->setArgOperand(0, CB.getInt32(7));
  auto Names = zil::nameState(*F);
  if (!zil::symbols().restoreNames(F, Names.present, Names.counter + uint32_t(1), Names.maximum) ||
      !zil::projectAdmittedStageExit(M, *F, P, nullptr, Output, &Reused) || Reused ||
      !zil::symbols().restoreNames(F, Names.present, Names.counter, Names.maximum)) return 0;
  if (!zil::projectAdmittedStageExit(M, *F, P, nullptr, Output, &Reused) || !Reused) return 0;
  auto *Address = ConstantExpr::getPtrToInt(F, Type::getInt64Ty(Context));
  new GlobalVariable(M, Address->getType(), true, GlobalValue::ExternalLinkage, Address, "taken");
  if (zil::incomingCallersHaveCurrentOwners(*F) ||
      zil::projectAdmittedStageExit(M, *F, P, nullptr, Output, &Reused) || Reused) return 0;
  // Keep the original true/false predicate counterexample. Official function
  // text does not encode these elements; the GENERAL key must now bind them,
  // rather than only making the new fast eligibility branch conservative.
  auto BuildAggregate = [](LLVMContext &C, Module &Mod, bool PointerField) {
    auto *Int = Type::getInt32Ty(C);
    auto *Pair = StructType::create(C, "local.only");
    Pair->setBody({Int, PointerField ? static_cast<Type *>(PointerType::getUnqual(C)) : Int});
    auto *Fn = Function::Create(FunctionType::get(Int, {Int}, false), GlobalValue::ExternalLinkage, "aggregate", &Mod);
    Fn->getArg(0)->setName("x"); IRBuilder<> B(BasicBlock::Create(C, "entry", Fn));
    auto *Value = B.CreateInsertValue(UndefValue::get(Pair), Fn->getArg(0), {0}, "pair");
    B.CreateRet(B.CreateExtractValue(Value, {0}, "value")); return Fn;
  };
  LLVMContext A, B; Module MA("identified-domain", A), MB("identified-domain", B);
  auto *FA = BuildAggregate(A, MA, false); auto *FBad = BuildAggregate(B, MB, true);
  if (verifyModule(MA, nullptr) || verifyModule(MB, nullptr) ||
      !zil::eligibleFunction(*FA) || zil::eligibleFunction(*FBad)) return 0;
  zil::Stage Identified; Identified.Input = zil::functionText(*FA); Identified.World = zil::worldText(MA, FA);
  auto Official = [](const Function &Fn) { std::string S; raw_string_ostream OS(S); Fn.print(OS, nullptr, true); return S; };
  if (Official(*FA) != Official(*FBad) || Identified.Input.empty() ||
      Identified.Input == zil::functionText(*FBad) || Identified.World != zil::worldText(MB, FBad) ||
      zil::projectAdmittedStageExit(MB, *FBad, Identified, nullptr, Output, &Reused) || Reused) return 0;
  return !verifyModule(M, nullptr);
}

extern "C" int ZilNativeProductsTypeAndOwnerCounterexamples() {
  if (!zil::symbols().available) return 0;
  unsigned CurrentMode = 0;
  auto Fail = [&](const char *Site) {
    // This entry is called only by the explicit test harness. Preserve the
    // exact failed boundary on its first run without changing runtime logging.
    std::fprintf(stderr, "ZIL type/owner counterexample mode=%u: %s\n", CurrentMode, Site);
    return 0;
  };
  auto Official = [](const Function &F) {
    std::string S; raw_string_ostream OS(S); F.print(OS, nullptr, true); return S;
  };
  auto Build = [](LLVMContext &C, Module &M, unsigned Width, unsigned Mode) {
    auto *I32 = Type::getInt32Ty(C); auto *Ptr = PointerType::getUnqual(C);
    auto *T = StructType::create(C, "local.only"); T->setBody({I32, IntegerType::get(C, Width)});
    auto *Subject = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "subject", &M);
    Subject->getArg(0)->setName("x");
    auto *Entry = BasicBlock::Create(C, "entry", Subject); IRBuilder<> SB(Entry);
    if (!Mode) {
      // Force a real extract instruction: IRBuilder's constant folder would
      // otherwise erase the very body-only type reference under test.
      auto *Value = ExtractValueInst::Create(ConstantAggregateZero::get(T), {0}, "value", Entry);
      SB.CreateRet(Value); return std::make_pair(Subject, Subject);
    }
    SB.CreateRet(Subject->getArg(0));
    SmallVector<Type *, 2> Params;
    if (Mode == 2) Params.push_back(Ptr);
    if (Mode == 3) { Params.push_back(Ptr); Params.push_back(Ptr); }
    auto *Caller = Function::Create(FunctionType::get(Mode == 4 ? static_cast<Type *>(Ptr) : I32, Params, false),
                                    GlobalValue::ExternalLinkage, "caller", &M);
    unsigned Index = 0; for (Argument &A : Caller->args()) A.setName(Index++ ? "data" : "input");
    IRBuilder<> B(BasicBlock::Create(C, "entry", Caller));
    if (Mode == 1) B.CreateAlloca(T, nullptr, "storage");
    if (Mode == 2) B.CreateGEP(T, Caller->getArg(0), {B.getInt32(0), B.getInt32(1)}, "field");
    if (Mode == 3) {
      auto *Call = B.CreateCall(FunctionType::get(Type::getVoidTy(C), {Ptr}, false),
                                Caller->getArg(0), {Caller->getArg(1)});
      Call->addParamAttr(0, Attribute::getWithByValType(C, T));
    }
    auto *Call = B.CreateCall(Subject, {B.getInt32(7)}, "call");
    if (Mode == 4) {
      auto *Storage = new GlobalVariable(M, Type::getInt8Ty(C), false,
                                          GlobalValue::ExternalLinkage, nullptr, "storage");
      Constant *Indices[] = {B.getInt32(0), B.getInt32(1)};
      auto *GEP = ConstantExpr::getGetElementPtr(T, Storage, Indices);
      B.CreateRet(GEP);
    } else B.CreateRet(Call);
    return std::make_pair(Subject, Caller);
  };
  // Every pair is valid; both scalar subjects are admitted. The official body
  // and header-only world collide while a complete current type graph differs.
  // Cover ordinary aggregate operands, alloca allocated type, GEP source type,
  // call-site byval type and ConstantExpr GEP source independently.
  for (unsigned Mode = 0; Mode != 5; ++Mode) {
    CurrentMode = Mode;
    LLVMContext A, B; Module MA("complete-type-projection", A), MB("complete-type-projection", B);
    auto Left = Build(A, MA, 8, Mode), Right = Build(B, MB, 16, Mode);
    if (verifyModule(MA, nullptr) || verifyModule(MB, nullptr)) return Fail("module-verifier");
    if (!zil::eligibleFunction(*Left.first) || !zil::eligibleFunction(*Right.first)) return Fail("two-admitted-subjects");
    if (Official(*Left.second) != Official(*Right.second)) return Fail("official-body-must-collide");
    auto LP = zil::functionText(*Left.second), RP = zil::functionText(*Right.second);
    if (LP.empty() || RP.empty() || LP == RP) return Fail("complete-type-definitions-must-differ");
    if (zil::worldText(MA, nullptr) != zil::worldText(MB, nullptr)) return Fail("headers-must-collide");
    if (Mode && zil::worldText(MA, Left.first) == zil::worldText(MB, Right.first)) return Fail("incoming-world-must-differ");
  }
  // Unknown unnamed identified schemas take a real shared-projector fallback,
  // including when reached only through an otherwise valid incoming caller.
  CurrentMode = 5;
  LLVMContext U; Module MU("unnamed-schema", U);
  auto *UI32 = Type::getInt32Ty(U);
  auto *T = StructType::create(U); T->setBody({UI32, Type::getInt8Ty(U)});
  auto *US = Function::Create(FunctionType::get(UI32, {UI32}, false), GlobalValue::ExternalLinkage, "subject", &MU);
  IRBuilder<> UB(BasicBlock::Create(U, "entry", US)); UB.CreateRet(US->getArg(0));
  auto *UC = Function::Create(FunctionType::get(UI32, false), GlobalValue::ExternalLinkage, "caller", &MU);
  auto *UE = BasicBlock::Create(U, "entry", UC); IRBuilder<> UCB(UE);
  auto *UV = ExtractValueInst::Create(ConstantAggregateZero::get(T), {0}, "value", UE);
  UCB.CreateRet(UCB.CreateCall(US, {UV}, "call"));
  if (verifyModule(MU, nullptr) || !zil::eligibleFunction(*US) ||
      !zil::functionText(*UC).empty() || !zil::worldText(MU, US).empty()) return Fail("unnamed-schema-fallback");
  LLVMContext C; Module M("orphan-owners", C);
  auto *I32 = Type::getInt32Ty(C);
  auto *F = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "f", &M);
  IRBuilder<> B(BasicBlock::Create(C, "entry", F)); B.CreateRet(F->getArg(0));
  auto *Detached = CallInst::Create(F->getFunctionType(), F, {B.getInt32(7)});
  bool Refused = !zil::incomingCallersHaveCurrentOwners(*F) && !zil::eligibleFunction(*F) && zil::worldText(M, F).empty();
  Detached->deleteValue();
  if (!Refused) return Fail("parentless-call");
  auto *Orphan = BasicBlock::Create(C, "orphan");
  CallInst::Create(F->getFunctionType(), F, {B.getInt32(7)}, "call", Orphan);
  Refused = !zil::incomingCallersHaveCurrentOwners(*F) && !zil::eligibleFunction(*F) && zil::worldText(M, F).empty();
  Orphan->deleteValue();
  if (!Refused) return Fail("parentless-block");
  return !verifyModule(M, nullptr) && zil::eligibleFunction(*F);
}

extern "C" int ZilNativePackedStageMetadataCounterexample() {
  using namespace zil;
  for (bool Unchanged : {false, true}) {
    Stage Original;
    Original.Input = "complete-current-input"; Original.World = "complete-world";
    Original.Pipeline = "actual-pipeline"; Original.EntryFacts = "entry-facts";
    Original.ExitFacts = "exit-facts";
    if (!Unchanged) {
      Original.Output = std::string("BC\0\xff", 4); Original.OutputText = "output-text";
      Original.OriginalDeclarations.emplace("llvm.test", "complete-declaration");
    }
    Original.EntryKeys = {TargetLibraryAnalysis::ID(), DominatorTreeAnalysis::ID()};
    Original.ExitKeys = {DominatorTreeAnalysis::ID(), TargetLibraryAnalysis::ID()};
    Original.BodyUnchanged = Unchanged;
    Original.OutputNextBlock = 123; Original.OutputBlockEpoch = 9;
    Original.OutputBlockNumbers = {3, 8, 17};
    Original.OutputAnalysisReplay.emptyAssumptions = 1;
    Original.OutputAnalysisReplay.noOptionTracking = 0;
    Original.OutputAnalysisReplay.dominatorDFSValid = 1;
    Original.OutputAnalysisReplay.postDominatorDFSValid = 0;
    Original.OutputNames.present = 1; Original.OutputNames.counter = 37;
    Original.OutputNames.maximum = 255;
    Original.PA.preserve<TargetLibraryAnalysis>();
    Original.PA.abandon<DominatorTreeAnalysis>();
    auto Text = PackedStageStrings::pack(Original, MaxStageProductBytes, false);
    if (!Text) return 0;
    PackedStage Packed;
    static_cast<StageState &>(Packed) = static_cast<const StageState &>(Original);
    Packed.Text = std::move(*Text);
    if (!unpackedStageBytes(Original, Packed.UnpackedBytes)) return 0;
    Packed.StoredBytes = sizeof(PackedStage);
    if (!stageMetadataBytes(Packed, Packed.StoredBytes) ||
        !addStageBytes(Packed.StoredBytes, Packed.Text.encodedSize())) return 0;
    Stage Restored;
    if (!unpackStage(Packed, Restored) || Restored.Input != Original.Input ||
        Restored.World != Original.World || Restored.Pipeline != Original.Pipeline ||
        Restored.Output != Original.Output || Restored.OutputText != Original.OutputText ||
        Restored.EntryFacts != Original.EntryFacts || Restored.ExitFacts != Original.ExitFacts ||
        Restored.OriginalDeclarations != Original.OriginalDeclarations ||
        Restored.EntryKeys != Original.EntryKeys || Restored.ExitKeys != Original.ExitKeys ||
        Restored.BodyUnchanged != Unchanged || Restored.OutputNextBlock != 123 ||
        Restored.OutputBlockEpoch != 9 || Restored.OutputBlockNumbers != Original.OutputBlockNumbers ||
        Restored.OutputAnalysisReplay.emptyAssumptions != 1 ||
        Restored.OutputAnalysisReplay.noOptionTracking != 0 ||
        Restored.OutputAnalysisReplay.dominatorDFSValid != 1 ||
        Restored.OutputAnalysisReplay.postDominatorDFSValid != 0 ||
        Restored.OutputNames.present != 1 || Restored.OutputNames.counter != 37 ||
        Restored.OutputNames.maximum != 255 ||
        !Restored.PA.getChecker<TargetLibraryAnalysis>().preserved() ||
        Restored.PA.getChecker<DominatorTreeAnalysis>().preserved()) return 0;
    Restored.Input = "sentinel";
    ++Packed.UnpackedBytes;
    if (unpackStage(Packed, Restored) || Restored.Input != "sentinel") return 0;
    Packed.UnpackedBytes = std::numeric_limits<size_t>::max();
    if (unpackStage(Packed, Restored)) return 0;
  }
  size_t Bytes = MaxStageProductBytes;
  if (addStageBytes(Bytes, 1) || Bytes != MaxStageProductBytes) return 0;
  Bytes = std::numeric_limits<size_t>::max();
  if (addStageBytes(Bytes, 1) || addStageElements(Bytes, std::numeric_limits<size_t>::max(), 8)) return 0;
  return 1;
}

namespace zil {
namespace {
struct IRScopeTestClock {
  using time_point = std::chrono::time_point<IRScopeTestClock, std::chrono::nanoseconds>;
  static thread_local uint64_t Tick;
  static time_point now() { return time_point(std::chrono::nanoseconds(Tick++)); }
};
thread_local uint64_t IRScopeTestClock::Tick = 0;
}
}
extern "C" int ZilNativeIRScopeTimerCounterexample() {
  using namespace zil;
  IRScopeTestClock::Tick = 0;
  uint64_t Total = 0;
  { IRScopeNS<IRScopeTestClock> Timer(Total); }
  if (Total != 1) return 0;
  { IRScopeNS<IRScopeTestClock> Timer(Total); Timer.stop(); Timer.stop(); }
  if (Total != 2) return 0;
  auto Failure = [&]() { IRScopeNS<IRScopeTestClock> Timer(Total); return false; };
  if (Failure() || Total != 3) return 0;
  uint64_t Parent = 0, Child = 0;
  {
    IRScopeNS<IRScopeTestClock> Outer(Parent);
    { IRScopeNS<IRScopeTestClock> Inner(Child); }
  }
  if (Parent != 3 || Child != 1) return 0;
  struct Cleanup { ~Cleanup() { (void)IRScopeTestClock::now(); } };
  uint64_t WithCleanup = 0;
  { IRScopeNS<IRScopeTestClock> Timer(WithCleanup); Cleanup Owned; }
  if (WithCleanup != 2) return 0;
  Total = std::numeric_limits<uint64_t>::max();
  { IRScopeNS<IRScopeTestClock> Timer(Total); }
  return Total == std::numeric_limits<uint64_t>::max();
}

extern "C" int ZilNativePackedStageEntryFilterCounterexample() {
  using namespace zil;
  Stage Original;
  Original.Input = std::string("input\0tail", 10); Original.World = "world";
  Original.Pipeline = "pipeline"; Original.EntryFacts = "complete-entry-facts";
  Original.Output = std::string("BC\0data", 7); Original.OutputText = "output";
  Original.ExitFacts = "complete-exit-facts";
  Original.EntryKeys = {TargetLibraryAnalysis::ID(), DominatorTreeAnalysis::ID()};
  Original.ExitKeys = {DominatorTreeAnalysis::ID()};
  auto Text = PackedStageStrings::pack(Original, MaxStageProductBytes, false);
  if (!Text) return 0;
  PackedStage Packed;
  static_cast<StageState &>(Packed) = static_cast<const StageState &>(Original);
  Packed.Text = std::move(*Text); Packed.EntrySummary = StageEntrySummary::create(Original);
  if (!unpackedStageBytes(Original, Packed.UnpackedBytes)) return 0;
  Packed.StoredBytes = sizeof(PackedStage);
  if (!stageMetadataBytes(Packed, Packed.StoredBytes) ||
      !addStageBytes(Packed.StoredBytes, Packed.Text.encodedSize())) return 0;
  ZilNativeProductStats Stats{};
  Stage Decoded, Current = Original;
  bool Attempted = false;
  if (!decodeStageCandidate(Packed, Current, Decoded, Stats, &Attempted) || !Attempted ||
      !exactStageEntry(Decoded, Original) || Decoded.Output != Original.Output ||
      Decoded.ExitFacts != Original.ExitFacts) return 0;
  // Each whole entry column is an independent length/hash input, including
  // bytes after NUL. Both length and same-length-content misses skip decode.
  for (unsigned I = 0; I != 4; ++I) {
    for (bool ChangeLength : {false, true}) {
      Current = Original;
      auto Columns = std::array<std::string *, 4>{&Current.Input, &Current.World,
                                                 &Current.Pipeline, &Current.EntryFacts};
      if (ChangeLength) Columns[I]->push_back('x'); else Columns[I]->back() ^= 1;
      Attempted = true;
      if (decodeStageCandidate(Packed, Current, Decoded, Stats, &Attempted) || Attempted) return 0;
    }
  }
  Current = Original; std::reverse(Current.EntryKeys.begin(), Current.EntryKeys.end());
  Attempted = true;
  if (decodeStageCandidate(Packed, Current, Decoded, Stats, &Attempted) || Attempted || Current.EntrySummary) return 0;
  Current = Original; Current.EntryKeys.pop_back(); Attempted = true;
  if (decodeStageCandidate(Packed, Current, Decoded, Stats, &Attempted) || Attempted) return 0;
  // A deliberate false positive/forced collision still reaches the ORIGINAL
  // exact byte comparison, so no stale body may be replayed on a digest match.
  Current = Original; Current.Input.back() ^= 1;
  PackedStage Collision = Packed; Collision.EntrySummary = StageEntrySummary::create(Current);
  Attempted = false;
  if (decodeStageCandidate(Collision, Current, Decoded, Stats, &Attempted) || !Attempted) return 0;
  // Current output/exit state is not an entry key and cannot be used to reject
  // a legitimate candidate before the original replay reconstructs/checks it.
  Current = Original; Current.OutputText = "not-an-entry-column"; Current.ExitFacts = "current-exit";
  Attempted = false;
  if (!decodeStageCandidate(Packed, Current, Decoded, Stats, &Attempted) || !Attempted) return 0;
  // A negative may reject an otherwise broken packet without inspecting it;
  // a positive still must attempt bounded decode and safely miss that packet.
  PackedStage Broken = Packed; Broken.Text = PackedStageStrings();
  Current = Original; Current.Input += "changed"; Attempted = true;
  if (decodeStageCandidate(Broken, Current, Decoded, Stats, &Attempted) || Attempted) return 0;
  Current = Original; Attempted = false;
  if (decodeStageCandidate(Broken, Current, Decoded, Stats, &Attempted) || !Attempted) return 0;
  return 1;
}

namespace zil {
namespace {
// Test passes deliberately run through the original patched FPM lifecycle.
struct EconomyCountingPass : PassInfoMixin<EconomyCountingPass> {
  unsigned *Count;
  explicit EconomyCountingPass(unsigned &N) : Count(&N) {}
  PreservedAnalyses run(Function &, FunctionAnalysisManager &) {
    ++*Count;
    return PreservedAnalyses::all();
  }
};
struct EconomyNestedPass : PassInfoMixin<EconomyNestedPass> {
  FunctionPassManager Inner;
  explicit EconomyNestedPass(unsigned &N) { Inner.addPass(EconomyCountingPass(N)); }
  PreservedAnalyses run(Function &F, FunctionAnalysisManager &AM) { return Inner.run(F, AM); }
};
class EconomyTestHooks {
  const ZilLLVMFunctionProductHooks *Previous;
public:
  explicit EconomyTestHooks(const ZilLLVMFunctionProductHooks &Hooks)
      : Previous(symbols().set(&Hooks)) {}
  ~EconomyTestHooks() { symbols().set(Previous); }
};
}
}
extern "C" int ZilNativeStageEconomicsCounterexample() {
  using namespace zil;
  if (!symbols().available) return 0;
  struct KeyVisitScope {
    uint64_t Count = 0;
    uint64_t *Previous = AnalysisKeyVisitsForTests;
    bool Diagnostics = AnalysisDiagnostics;
    bool Always = StageAlwaysForTests;
    KeyVisitScope() { AnalysisKeyVisitsForTests = &Count; AnalysisDiagnostics = false; StageAlwaysForTests = false; }
    ~KeyVisitScope() {
      AnalysisKeyVisitsForTests = Previous;
      AnalysisDiagnostics = Diagnostics; StageAlwaysForTests = Always;
    }
  } Visits;
  GroupRuntime Cold = beginEconomy(nullptr, 1);
  for (unsigned I = 0; I != 2; ++I) {
    if (!tryEconomyKey(Cold) || !tryEconomyCapture(Cold)) return 0;
    economyMiss(Cold); economyFresh(Cold, 200); economyCapture(Cold, 40, 60);
  }
  if (tryEconomyKey(Cold) || tryEconomyCapture(Cold) || Cold.Closed) return 0;
  auto Pilot = freezeEconomy(Cold, 1);
  if (Pilot.Mode != EconomyMode::Probe || Pilot.FreshSamples != 2 ||
      Pilot.CaptureSamples != 2 || Pilot.HitSamples) return 0;
  GroupRuntime Warm = beginEconomy(&Pilot, 2);
  if (!tryEconomyKey(Warm)) return 0;
  economyHit(Warm, 80);
  if (!tryEconomyKey(Warm)) return 0;
  economyHit(Warm, 90);
  // Promotion is not visible to the rest of this invocation.
  if (Warm.Frozen != EconomyMode::Probe || tryEconomyKey(Warm)) return 0;
  auto Profitable = freezeEconomy(Warm, 2);
  if (Profitable.Mode != EconomyMode::Reuse) return 0;
  GroupRuntime Reuse = beginEconomy(&Profitable, 3);
  economyMiss(Reuse);
  if (!tryEconomyKey(Reuse) || Reuse.Closed) return 0;
  economyHit(Reuse, 90); // a real hit resets the consecutive miss breaker
  economyMiss(Reuse);
  if (Reuse.Closed) return 0;
  economyMiss(Reuse);
  if (!Reuse.Closed || tryEconomyKey(Reuse)) return 0;
  if (!tryEconomyCapture(Reuse) || !tryEconomyCapture(Reuse) || tryEconomyCapture(Reuse)) return 0;
  for (unsigned I = 0; I != FreshSamplesPerRequest; ++I) if (!sampleFresh(Reuse)) return 0;
  if (sampleFresh(Reuse)) return 0;
  GroupRuntime Slow = Warm;
  economyHit(Slow, 101); // just outside F >= 2H
  auto Rejected = freezeEconomy(Slow, 2);
  if (Rejected.Mode != EconomyMode::Bypass || Rejected.ReprobeEpoch != 10) return 0;
  auto Wait = beginEconomy(&Rejected, 9);
  if (tryEconomyKey(Wait) || sampleFresh(Wait) || Wait.Frozen != EconomyMode::Bypass) return 0;
  auto Reprobe = beginEconomy(&Rejected, 10);
  if (Reprobe.Frozen != EconomyMode::Probe || Reprobe.Learned.HitSamples || !tryEconomyKey(Reprobe)) return 0;
  Slow = Warm; Slow.Learned.CaptureMax = 221; // C > 2(F-H) = 220
  if (freezeEconomy(Slow, 2).Mode != EconomyMode::Bypass) return 0;
  Slow = Warm; economyHit(Slow, std::numeric_limits<uint64_t>::max());
  if (freezeEconomy(Slow, 2).Mode != EconomyMode::Bypass ||
      addEconomyNS(std::numeric_limits<uint64_t>::max(), 1) != std::numeric_limits<uint64_t>::max()) return 0;
  auto Invalid = Profitable; Invalid.HitSamples = 3;
  if (beginEconomy(&Invalid, 3).Frozen != EconomyMode::Probe) return 0;
  auto Before = Warm.Learned;
  economyFresh(Warm, std::nullopt); economyHit(Warm, std::nullopt);
  economyCapture(Warm, 0, 99);
  if (Warm.Learned.FreshMin != Before.FreshMin || Warm.Learned.HitMax != Before.HitMax ||
      Warm.Learned.CaptureMax != Before.CaptureMax) return 0;
  auto Now = EconomyClock::now();
  if (elapsedEconomyNS(Now, Now) || elapsedEconomyNS(Now, Now - std::chrono::nanoseconds(1))) return 0;

  LLVMContext Context; Module M("economy", Context);
  auto *F = Function::Create(FunctionType::get(Type::getInt32Ty(Context), false),
                             GlobalValue::ExternalLinkage, "ordinary", &M);
  IRBuilder<> B(BasicBlock::Create(Context, "entry", F)); B.CreateRet(B.getInt32(7));
  Request Bounded(M, "config", nullptr);
  for (size_t I = 0; I != MaxEconomyGroups; ++I)
    if (!economyGroup(Bounded, "group:" + std::to_string(I))) return 0;
  if (economyGroup(Bounded, "overflow") || Bounded.Groups.size() != MaxEconomyGroups ||
      Bounded.Next.Economics.size() != MaxEconomyGroups ||
      Bounded.ProductBytes != Bounded.EconomyBytes ||
      Bounded.UnpackedProductBytes != Bounded.EconomyBytes) return 0;
  Request Full(M, "config", nullptr);
  Full.ProductBytes = MaxStageProductBytes;
  if (economyGroup(Full, "no-room") || !Full.Groups.empty()) return 0;
  Full.ProductBytes = 0; Full.UnpackedProductBytes = MaxStageProductBytes;
  if (economyGroup(Full, "no-raw-room")) return 0;
  Request PendingBound(M, "config", nullptr);
  for (unsigned I = 0; I != 128; ++I) if (!newPending(PendingBound)) return 0;
  if (newPending(PendingBound)) return 0;

  // A TimeOnly parent runs both original passes. The nested FPM advances its
  // Slot but has no cache key, replay, capture, policy row or pending token.
  FunctionAnalysisManager AM;
  AM.registerPass([] { return PassInstrumentationAnalysis(); });
  AM.getResult<PassInstrumentationAnalysis>(*F);
  unsigned OuterRan = 0, InnerRan = 0;
  FunctionPassManager PM;
  PM.addPass(EconomyCountingPass(OuterRan)); PM.addPass(EconomyNestedPass(InnerRan));
  Request Current(M, "config", nullptr); Current.Phase = 1;
  auto *TimeGroup = economyGroup(Current, economyGroupKey(Current, PM, *F));
  if (!TimeGroup) return 0;
  TimeGroup->KeyAttempts = PilotKeys;
  Current.Hooks = {2, &Current, nullptr, enterPass, leavePass, beforeStage, afterStage, nullptr, requestFailedCallback};
  {
    EconomyTestHooks Hooks(Current.Hooks);
    PM.run(*F, AM);
  }
  unsigned Slots = 0;
  for (const auto &E : Current.Occurrences) {
    if (E.first.find("fpm-slot-v3/") != 0) return 0;
    Slots += E.second;
  }
  if (OuterRan != 1 || InnerRan != 1 || Slots != 2 || Current.Groups.size() != 1 ||
      !Current.PendingTokens.empty() || Current.SuppressNestedCache || Current.ObserverDepth ||
      !Current.Next.Stages.empty() || Current.Stats.ir_key_bytes || Current.Stats.ir_snapshot_bytes ||
      Current.Stats.optimizer_function_stage_executions != 2 || Current.Failed || Visits.Count) return 0;
  // Actual hooked FPM: Bypass and bad-clock refusals leave FAM completely
  // unenumerated, but retain original pass execution, Slots and token cleanup.
  auto Refused = [&](bool Diagnostics, bool BadClock) {
    unsigned Ran = 0;
    FunctionPassManager Fresh; Fresh.addPass(EconomyCountingPass(Ran));
    Request R(M, "config", nullptr); R.Phase = 1;
    auto *G = economyGroup(R, economyGroupKey(R, Fresh, *F));
    if (!G) return false;
    G->Frozen = EconomyMode::Bypass; R.ObserverClockValid = !BadClock;
    R.Hooks = {2, &R, nullptr, enterPass, leavePass, beforeStage, afterStage, nullptr, requestFailedCallback};
    const auto BeforeVisits = Visits.Count;
    AnalysisDiagnostics = Diagnostics;
    { EconomyTestHooks Hooks(R.Hooks); Fresh.run(*F, AM); }
    AnalysisDiagnostics = false;
    unsigned Slots = 0;
    for (const auto &E : R.Occurrences) Slots += E.second;
    return Ran == 1 && Slots == 1 && R.PendingTokens.empty() && !R.SuppressNestedCache &&
        !R.ObserverDepth && !R.Failed && R.Next.Stages.empty() && !R.Stats.ir_key_bytes &&
        R.Stats.optimizer_function_stage_executions == 1 &&
        Visits.Count - BeforeVisits == (Diagnostics ? (BadClock ? 1u : 2u) : 0u);
  };
  if (!Refused(false, false) || !Refused(false, true) ||
      !Refused(true, false) || !Refused(true, true)) return 0;
  // An actually admitted capture still observes COMPLETE current entry/exit
  // analysis sets. Force only economics, as existing strict quality scopes do.
  StageAlwaysForTests = true;
  Request Capturing(M, "config", nullptr); Capturing.Phase = 1;
  StageAlwaysForTests = false;
  unsigned CaptureRan = 0;
  FunctionPassManager CapturePM; CapturePM.addPass(EconomyCountingPass(CaptureRan));
  Capturing.Hooks = {2, &Capturing, nullptr, enterPass, leavePass, beforeStage, afterStage, nullptr, requestFailedCallback};
  const auto BeforeCaptureVisits = Visits.Count;
  { EconomyTestHooks Hooks(Capturing.Hooks); CapturePM.run(*F, AM); }
  if (CaptureRan != 1 || Capturing.Failed || !Capturing.PendingTokens.empty() ||
      Capturing.Next.Stages.size() != 1 || !Capturing.Stats.ir_key_bytes ||
      Visits.Count - BeforeCaptureVisits != 2) return 0;
  // PM address reuse is only a potentially bad economic hint. The complete
  // authorized Pipeline is still freshly printed after this hint is poisoned.
  Current.PipelineHints[&PM] = "deliberately-wrong-economic-recipe";
  if (pipeline(PM) == Current.PipelineHints.at(&PM)) return 0;

  Products Parent; Parent.Config = "config"; Parent.SuccessfulEpoch = 1;
  Parent.Economics.emplace("publication", Pilot);
  Request WrongConfig(M, "changed-config", &Parent);
  auto *ChangedGroup = economyGroup(WrongConfig, "publication");
  if (!ChangedGroup || ChangedGroup->Learned.FreshSamples || WrongConfig.Next.SuccessfulEpoch != 1) return 0;
  auto Publish = [&](bool Fail, bool Abort) {
    auto R = std::make_unique<Request>(M, "config", &Parent);
    auto *G = economyGroup(*R, "publication");
    if (!G) return false;
    *G = Warm;
    R->Failed = Fail;
    R->Previous = symbols().set(nullptr);
    if (Abort) {
      ZilNativeProductsAbort(R.release(), nullptr);
      return Parent.Economics.at("publication").Mode == EconomyMode::Probe;
    }
    int Failed = -1;
    auto *P = static_cast<Products *>(ZilNativeProductsFinish(R.release(), nullptr, &Failed));
    std::unique_ptr<Products> Owned(P);
    if (Fail) return !P && Failed == 1 && Parent.Economics.at("publication").Mode == EconomyMode::Probe;
    return P && Failed == 0 && P->SuccessfulEpoch == 2 &&
        P->Economics.at("publication").Mode == EconomyMode::Reuse &&
        Parent.Economics.at("publication").Mode == EconomyMode::Probe;
  };
  return Publish(true, false) && Publish(false, true) && Publish(false, false) && !verifyModule(M, nullptr);
}

// Independent V1 observer, never appended to ABI2 or the public Stats layout.
// Caller queries a LIVE request before Finish/Abort consumes it.
extern "C" int ZilNativeProductsReadMachineEmissionStatsV1(const void *P,void *Out,size_t Bytes) {
  if(!P || !Out || Bytes!=sizeof(zil::emission::Stats))return 0;
  const auto &R=*static_cast<const zil::Request *>(P);
  if(!R.Machine)return 0;
  zil::readMachineEmissionStats(*R.Machine,Out,Bytes);return 1;
}

// Independent live-request transport observer, read before Finish/Abort.
extern "C" int ZilNativeProductsReadStageBodyStatsV1(const void *P, void *Out, size_t Bytes) {
  if (!P || !Out || Bytes != sizeof(ZilNativeStageBodyStatsV1)) return 0;
  *static_cast<ZilNativeStageBodyStatsV1 *>(Out) = static_cast<const zil::Request *>(P)->BodyStats;
  return 1;
}
extern "C" int ZilNativeStageBodyCounterexamplesV1() { return zil::StageBody::counterexamples(); }
