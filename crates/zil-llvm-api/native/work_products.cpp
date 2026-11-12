// Fixed LLVM 23.1.4 resident optimization products. The original pass managers
// construct and run their original pipelines; this adapter never reparses one.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "work_products.h"
#include "zil_llvm_work_products_hooks.h"
#include "machine_products.h"
#include "analysis_facts.h"
#include "packed_stage.h"
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
#include "llvm/Transforms/Utils/Cloning.h"
#include "llvm/Transforms/Utils/PromoteMemToReg.h"
#include <algorithm>
#include <array>
#include <chrono>
#include <cstdio>
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
  Keys EntryKeys, ExitKeys;
  // A full function projection equality (not PA::all or a digest) proves that
  // this stage has no function state to transport. Analyses remain separate.
  bool BodyUnchanged = false;
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
struct Products {
  std::string Config;
  std::map<std::string, std::shared_ptr<const PackedStage>> Stages;
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
  size_t ProductBytes = 0, UnpackedProductBytes = 0;
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
bool addStageBytes(size_t &Bytes, size_t N) {
  if (Bytes > MaxStageProductBytes || N > MaxStageProductBytes - Bytes) return false;
  Bytes += N; return true;
}
bool addStageElements(size_t &Bytes, size_t Count, size_t Width) {
  return Count <= MaxStageProductBytes / Width && addStageBytes(Bytes, Count * Width);
}
bool stageMetadataBytes(const StageState &P, size_t &Bytes) {
  return addStageElements(Bytes, P.EntryKeys.size(), sizeof(void *)) &&
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
bool unpackStage(const PackedStage &Packed, Stage &Out) {
  if (Packed.UnpackedBytes > MaxStageProductBytes || Packed.StoredBytes > MaxStageProductBytes) return false;
  Stage Current;
  if (!Packed.Text.unpack(Current, MaxStageProductBytes)) return false;
  static_cast<StageState &>(Current) = static_cast<const StageState &>(Packed);
  size_t Bytes;
  if (!unpackedStageBytes(Current, Bytes) || Bytes != Packed.UnpackedBytes) return false;
  Out = std::move(Current); return true;
}
bool entryMayMatch(const PackedStage &Prior, Stage &Current) {
  if (Prior.EntryKeys != Current.EntryKeys || !Prior.EntrySummary.sameLengths(Current)) return false;
  Current.EntrySummary = StageEntrySummary::create(Current);
  return Prior.EntrySummary.Digests == Current.EntrySummary->Digests;
}
bool exactStageEntry(const Stage &Prior, const Stage &Current) {
  return Prior.Input == Current.Input && Prior.World == Current.World &&
         Prior.Pipeline == Current.Pipeline && Prior.EntryKeys == Current.EntryKeys &&
         Prior.EntryFacts == Current.EntryFacts;
}
// Called only after exact current Config and Slot lookup. False leaves IR/AM
// untouched and executes the original stage. AttemptedDecode is a fixture-only
// observation proving a negative filter really avoided the full packed read.
bool decodeStageCandidate(const PackedStage &Prior, Stage &Current, Stage &Decoded,
                          ZilNativeProductStats &Stats, bool *AttemptedDecode = nullptr) {
  if (AttemptedDecode) *AttemptedDecode = false;
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
  R.Next.Stages.emplace(Slot, std::move(P));
  // Preserve BOTH the original aggregate decoded admission and the bounded
  // actual encoded payload+typed metadata. This is not an RSS measurement.
  R.UnpackedProductBytes += Raw; R.ProductBytes += Stored;
}
void retainStage(Request &R, const std::string &Slot, std::shared_ptr<const Stage> Product) {
  IRScopeNS<> PackTime(R.Stats.ir_pack_ns);
  // This local dies before the timer, including on every admission failure.
  auto P = std::move(Product);
  size_t Raw;
  if (R.Next.Stages.size() >= MaxStageProductSlots || R.Next.Stages.count(Slot) ||
      !unpackedStageBytes(*P, Raw)) return;
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

bool captureStageBody(Stage &P, Function &F, ZilNativeProductStats &Stats,
                      std::string Output = {}) {
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
      P.OriginalDeclarations.emplace(D->getName().str(), declarationText(*D));
    }
  P.OutputNames = nameState(F);
  if (P.OutputNames.present < 0) return false;
  P.OutputNextBlock = F.getMaxBlockNumber(); P.OutputBlockEpoch = F.getBlockNumberEpoch();
  for (const BasicBlock &B : F) P.OutputBlockNumbers.push_back(B.getNumber());
  P.Output = snapshot(F);
  if (P.Output.empty()) return false;
  Stats.ir_snapshot_bytes += P.Output.size() + P.OutputText.size();
  return true;
}

bool replay(Request &R, Function &F, FunctionAnalysisManager &AM, const Stage &P) {
  IRScopeNS<> ReplayTime(R.Stats.ir_replay_ns);
  auto Fail = [&](const char *Site) {
    if (AnalysisDiagnostics) std::fprintf(stderr, "ZIL FPM replay-failure %.*s: %s\n", static_cast<int>(F.getName().size()), F.getName().data(), Site);
    return false;
  };
  if (P.BodyUnchanged) {
    if (verifyFunction(F, nullptr)) return Fail("unchanged-current-function-verifier");
    // Input/world/stage/config/entry analyses were matched in beforeStage.
    // Retain the current Function and all of its Values/use-lists/name/BB
    // state; only the independently bound analysis transition may need work.
    return restoreStageAnalyses(F, AM, P, &R.AnalysisMemo,
                                R.Stats.analysis_reconstructions, true);
  }
  IRScopeNS<> ParseTime(R.Stats.ir_parse_ns);
  auto Buffer = MemoryBuffer::getMemBufferCopy(P.Output, "zil.function.product");
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
  if (!symbols().restoreNumbers(&F, P.OutputNextBlock, P.OutputBlockEpoch,
                               P.OutputBlockNumbers.data(), P.OutputBlockNumbers.size())) return Fail("block-number-restore");
  if (!preserveLocalUseOrder(*SF, Map)) return Fail("local-use-order");
  if (verifyFunction(F, nullptr)) return Fail("current-function-verifier");
  if (!symbols().restoreNames(&F, P.OutputNames.present, P.OutputNames.counter, P.OutputNames.maximum)) return Fail("value-name-state-restore");
  if (functionText(F) != P.OutputText) return Fail("complete-output-text");
  if (!restoreStageAnalyses(F, AM, P, &R.AnalysisMemo,
                            R.Stats.analysis_reconstructions, false)) return false;
  R.Stats.ir_replay_bytes += P.Output.size(); return true;
}
int beforeStage(void *Context, void *PM, void *IR, void *Manager, void *, void *PA, void **Token) {
  auto &R = *static_cast<Request *>(Context); auto &F = *static_cast<Function *>(IR);
  auto &AM = *static_cast<FunctionAnalysisManager *>(Manager);
  *Token = nullptr;
  if (F.getParent() != &R.M || R.Phase != 1) return 0;
  if (R.Failed) { *static_cast<PreservedAnalyses *>(PA) = PreservedAnalyses::none(); return 1; }
  // Unknown analysis domains can never authorize a product. Reject them
  // before scanning this function's CFG, callers, owned uses or name map.
  Keys Entry = analysisKeys(AM, F);
  describeAnalyses(AM, F, "entry", Entry);
  if (!supportedAnalysisKeys(Entry)) {
    if (AnalysisDiagnostics) *Token = newPending(R);
    ++R.Stats.optimizer_function_stage_executions; return 0;
  }
  IRScopeNS<> KeyTime(R.Stats.ir_key_ns);
  if (!eligibleFunction(F)) { ++R.Stats.optimizer_function_stage_executions; return 0; }
  // Bind actual supported result fields, not merely analysis names or IDs.
  std::string Facts;
  if (!analysisFacts(F, AM, Entry, Facts, nullptr, &R.AnalysisMemo)) {
    if (AnalysisDiagnostics) *Token = newPending(R);
    ++R.Stats.optimizer_function_stage_executions; return 0;
  }
  auto P = std::make_shared<Stage>();
  std::string Slot = path(R, F);
  P->Pipeline = pipeline(*static_cast<FunctionPassManager *>(PM));
  P->Input = functionText(F); P->World = worldText(R.M, &F, &R.Stats); P->EntryKeys = std::move(Entry); P->EntryFacts = std::move(Facts);
  if (P->Input.empty() || P->World.empty()) {
    ++R.Stats.optimizer_function_stage_executions; return 0;
  }
  R.Stats.ir_key_bytes += P->Input.size() + P->World.size() + P->Pipeline.size() + P->EntryFacts.size() + R.Config.size();
  KeyTime.stop(); // Lookup/decode/replay are not key construction.
  if (R.Prior && R.Prior->Config == R.Config) {
    auto It = R.Prior->Stages.find(Slot);
    if (It != R.Prior->Stages.end()) {
      Stage Old;
      // A malformed/unavailable codec is an optional candidate miss. No IR or
      // AM state has changed; the original stage still executes below.
      if (decodeStageCandidate(*It->second, *P, Old, R.Stats)) {
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
// Entry eligibility was freshly established before the original stage. A
// complete current equality of every field it read is a producer proof, not a
// Function* identity memo. Unknown type projections refuse the product.
// Current owners/hidden projection state are checked
// independently before equality can be used.
bool projectAdmittedStageExit(const Module &M, const Function &F, const Stage &P,
                             ZilNativeProductStats *Stats, std::string &Output,
                             bool *ReusedEligibility = nullptr) {
  if (ReusedEligibility) *ReusedEligibility = false;
  if (!incomingCallersHaveCurrentOwners(F)) return false;
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

void afterStage(void *Context, void *, void *IR, void *Manager, const void *PA, void *Token) {
  auto &R = *static_cast<Request *>(Context);
  IRScopeNS<> CaptureTime(R.Stats.ir_capture_ns);
  auto It = R.PendingTokens.find(static_cast<Pending *>(Token));
  if (It == R.PendingTokens.end()) return;
  auto P = std::move(It->second); R.PendingTokens.erase(It);
  if (R.Failed) return;
  auto &F = *static_cast<Function *>(IR);
  auto &AM = *static_cast<FunctionAnalysisManager *>(Manager);
  Keys Exit = analysisKeys(AM, F);
  describeAnalyses(AM, F, "exit", Exit);
  // An unsupported exit cannot be retained, regardless of body/world values.
  // Keep diagnostic-only pending tokens, but avoid their deep projections.
  if (!P->Product || R.Failed || !supportedAnalysisKeys(Exit)) return;
  std::string Output;
  if (!projectAdmittedStageExit(R.M, F, *P->Product, &R.Stats, Output)) return;
  P->Product->ExitKeys = std::move(Exit);
  if (!analysisFacts(F, *static_cast<FunctionAnalysisManager *>(Manager),
                     P->Product->ExitKeys, P->Product->ExitFacts, &P->Product->OutputAnalysisReplay, &R.AnalysisMemo)) return;
  if (!captureStageBody(*P->Product, F, R.Stats, std::move(Output))) return;
  P->Product->PA = *static_cast<const PreservedAnalyses *>(PA);
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
