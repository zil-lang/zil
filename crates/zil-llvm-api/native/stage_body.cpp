// Fixed LLVM 7a941df: typed transport for the already verified arithmetic domain.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "stage_body.h"
#include "llvm/ADT/APInt.h"
#include "llvm/ADT/DenseMap.h"
#include "llvm/IR/Attributes.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/DerivedTypes.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/Intrinsics.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/Operator.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/xxhash.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/TargetParser/Triple.h"
#include <algorithm>
#include <array>
#include <cstring>
#include <cstdio>
#include <limits>
#include <map>
#include <new>
#include <string>
#include <type_traits>
#include <vector>
using namespace llvm;
namespace zil {
namespace {
constexpr uint32_t Magic = 0x3152495a; // ZIR1, resident only, fixed LLVM ABI.
constexpr size_t Limit = 64 * 1024 * 1024;
enum : uint32_t { VoidTy, IntegerTy, PointerTy, StructTy };
enum : uint32_t { ArgumentRef, BlockRef, InstructionRef, ConstantRef, CalleeRef };
enum : uint32_t { IntegerConstant, PoisonConstant, UndefConstant, ZeroConstant };
enum : uint32_t { EnumAttribute, IntAttribute, StringAttribute };
struct Span { uint32_t Start = 0, Count = 0; };
struct Ref { uint32_t Kind = 0, ID = 0; };
struct TypeRow { uint32_t Kind = 0, Detail = 0; Span Elements, Name; };
struct ConstantRow { uint32_t Kind = 0, Type = 0; Span Words; };
struct BlockRow { Span Name, Instructions; };
struct AttributeRow {
  uint32_t Index = 0, Form = 0, Kind = 0, Reserved = 0;
  Span Name, Value;
  uint64_t Integer = 0;
};
struct InstructionRow {
  uint32_t Opcode = 0, Type = 0;
  Span Name, Operands, Auxiliary, Attributes;
  uint32_t Flags = 0, Detail = 0, Detail2 = 0, Reserved = 0;
  uint64_t Wide = 0;
};
struct UseRow { uint32_t User = 0, Operand = 0; };
static_assert(sizeof(Span) == 8 && sizeof(Ref) == 8 && sizeof(TypeRow) == 24);
static_assert(sizeof(ConstantRow) == 16 && sizeof(BlockRow) == 16);
static_assert(sizeof(AttributeRow) == 40 && sizeof(InstructionRow) == 64 && sizeof(UseRow) == 8);
struct Header {
  uint32_t Version = Magic, Arguments = 0, ResultType = 0, Reserved = 0;
  Span ArgumentTypes;
  std::array<Span, 12> Sections{};
};
enum Section { Types, Constants, Blocks, Instructions, Operands, Auxiliary,
               Attributes, Words, Strings, Callees, Uses, ArgumentNames };
struct UseList { Span Entries; };
struct Payload {
  uint32_t Arguments = 0, ResultType = 0;
  Span ArgumentTypes;
  std::vector<TypeRow> Types;
  std::vector<ConstantRow> Constants;
  std::vector<BlockRow> Blocks;
  std::vector<InstructionRow> Instructions;
  std::vector<Ref> Operands;
  std::vector<uint32_t> Auxiliary;
  std::vector<AttributeRow> Attributes;
  std::vector<uint64_t> Words;
  std::string Strings;
  std::vector<Span> Callees, ArgumentNames;
  // One list for each argument, then each BB and its instructions, matching
  // the original preserveLocalUseOrder traversal exactly.
  std::vector<UseList> UseLists;
  std::vector<UseRow> Uses;
};
bool add(size_t &Bytes, size_t Count, size_t Width = 1) {
  if (Width && Count > Limit / Width) return false;
  size_t N = Count * Width;
  if (Bytes > Limit || N > Limit - Bytes) return false;
  Bytes += N; return true;
}
bool payloadBytes(const Payload &P, size_t &Bytes) {
  Bytes = sizeof(Header);
  return add(Bytes, P.Types.size(), sizeof(TypeRow)) &&
    add(Bytes, P.Constants.size(), sizeof(ConstantRow)) &&
    add(Bytes, P.Blocks.size(), sizeof(BlockRow)) &&
    add(Bytes, P.Instructions.size(), sizeof(InstructionRow)) &&
    add(Bytes, P.Operands.size(), sizeof(Ref)) &&
    add(Bytes, P.Auxiliary.size(), sizeof(uint32_t)) &&
    add(Bytes, P.Attributes.size(), sizeof(AttributeRow)) &&
    add(Bytes, P.Words.size(), sizeof(uint64_t)) && add(Bytes, P.Strings.size()) &&
    add(Bytes, P.Callees.size(), sizeof(Span)) && add(Bytes, P.ArgumentNames.size(), sizeof(Span)) &&
    add(Bytes, P.UseLists.size(), sizeof(UseList)) && add(Bytes, P.Uses.size(), sizeof(UseRow));
}
struct Encoder {
  Payload P;
  size_t Cap;
  DenseMap<const Type *, uint32_t> Types;
  DenseMap<const Constant *, uint32_t> Constants;
  DenseMap<const Value *, Ref> Values;
  DenseMap<const Function *, uint32_t> Callees;
  bool OK = true;
  explicit Encoder(size_t C) : Cap(std::min(C, Limit)) {}
  bool bounded() {
    size_t N; return OK && payloadBytes(P, N) && N <= Cap;
  }
  Span text(StringRef S) {
    Span R{static_cast<uint32_t>(P.Strings.size()), static_cast<uint32_t>(S.size())};
    if (S.size() > Cap || P.Strings.size() > Cap - S.size()) { OK = false; return {}; }
    if (!S.empty()) P.Strings.append(S.data(), S.size()); return R;
  }
  uint32_t type(const Type *T) {
    auto It = Types.find(T); if (It != Types.end()) return It->second;
    TypeRow R;
    if (T->isVoidTy()) R.Kind = VoidTy;
    else if (const auto *I = dyn_cast<IntegerType>(T)) { R.Kind = IntegerTy; R.Detail = I->getBitWidth(); }
    else if (const auto *Pointer = dyn_cast<PointerType>(T)) { R.Kind = PointerTy; R.Detail = Pointer->getAddressSpace(); }
    else if (const auto *S = dyn_cast<StructType>(T)) {
      if (S->isOpaque() || (!S->isLiteral() && !S->hasName())) { OK = false; return 0; }
      R.Kind = StructTy; R.Detail = S->isPacked() | (uint32_t(!S->isLiteral()) << 1);
      R.Name = text(S->isLiteral() ? StringRef() : S->getName());
      std::vector<uint32_t> Elements;
      for (Type *E : S->elements()) {
        if (!E->isIntegerTy()) { OK = false; return 0; }
        Elements.push_back(type(E));
      }
      R.Elements = {static_cast<uint32_t>(P.Auxiliary.size()), static_cast<uint32_t>(Elements.size())};
      P.Auxiliary.insert(P.Auxiliary.end(), Elements.begin(), Elements.end());
    } else { OK = false; return 0; }
    uint32_t ID = P.Types.size(); Types[T] = ID; P.Types.push_back(R); return ID;
  }
  Ref value(const Value *V) {
    auto It = Values.find(V); if (It != Values.end()) return It->second;
    if (const auto *F = dyn_cast<Function>(V)) {
      auto Existing = Callees.find(F);
      if (Existing != Callees.end()) return {CalleeRef, Existing->second};
      if (!F->isDeclaration() || !F->isIntrinsic() || F->getCallingConv() != CallingConv::C || F->isVarArg()) {
        OK = false; return {};
      }
      switch (F->getIntrinsicID()) {
      case Intrinsic::sadd_with_overflow: case Intrinsic::ssub_with_overflow:
      case Intrinsic::smul_with_overflow: case Intrinsic::uadd_with_overflow:
      case Intrinsic::usub_with_overflow: case Intrinsic::umul_with_overflow:
      case Intrinsic::trap: break;
      default: OK = false; return {};
      }
      uint32_t ID = P.Callees.size(); Callees[F] = ID; P.Callees.push_back(text(F->getName()));
      return {CalleeRef, ID};
    }
    const auto *C = dyn_cast<Constant>(V);
    if (!C || C->getNumOperands()) { OK = false; return {}; }
    auto Old = Constants.find(C); if (Old != Constants.end()) return {ConstantRef, Old->second};
    ConstantRow R; R.Type = type(C->getType());
    if (const auto *I = dyn_cast<ConstantInt>(C)) {
      R.Kind = IntegerConstant;
      const APInt &A = I->getValue();
      R.Words = {static_cast<uint32_t>(P.Words.size()), A.getNumWords()};
      P.Words.insert(P.Words.end(), A.getRawData(), A.getRawData() + A.getNumWords());
    } else if (isa<PoisonValue>(C)) R.Kind = PoisonConstant;
    else if (isa<UndefValue>(C)) R.Kind = UndefConstant;
    else if (isa<ConstantAggregateZero>(C)) R.Kind = ZeroConstant;
    else { OK = false; return {}; }
    uint32_t ID = P.Constants.size(); Constants[C] = ID; P.Constants.push_back(R);
    return {ConstantRef, ID};
  }
  bool attributes(const AttributeList &A, Span &Out) {
    Out.Start = P.Attributes.size();
    for (unsigned Index : A.indexes()) for (Attribute Attr : A.getAttributes(Index)) {
      AttributeRow R; R.Index = Index;
      if (Attr.isStringAttribute()) {
        R.Form = StringAttribute; R.Name = text(Attr.getKindAsString()); R.Value = text(Attr.getValueAsString());
      } else if (Attr.isEnumAttribute()) { R.Form = EnumAttribute; R.Kind = Attr.getKindAsEnum(); }
      else if (Attr.isIntAttribute()) { R.Form = IntAttribute; R.Kind = Attr.getKindAsEnum(); R.Integer = Attr.getValueAsInt(); }
      else return false; // type/range/initializes and future forms keep bitcode.
      P.Attributes.push_back(R); if (!bounded()) return false;
    }
    Out.Count = P.Attributes.size() - Out.Start; return OK;
  }
  bool instruction(const Instruction &I) {
    InstructionRow R; R.Opcode = I.getOpcode(); R.Type = type(I.getType()); R.Name = text(I.getName());
    R.Flags = I.getRawSubclassOptionalData();
    uint32_t Allowed = 0;
    if (isa<OverflowingBinaryOperator>(I) || isa<TruncInst>(I)) Allowed = 3;
    else if (isa<PossiblyExactOperator>(I) || isa<PossiblyDisjointInst>(I) || isa<ICmpInst>(I) || isa<ZExtInst>(I)) Allowed = 1;
    if (R.Flags & ~Allowed) return false;
    if (const auto *C = dyn_cast<CallInst>(&I)) {
      if (C->hasOperandBundles() || !C->getCalledFunction()) return false;
      Ref D = value(C->getCalledFunction()); if (D.Kind != CalleeRef) return false;
      R.Detail = D.ID; R.Detail2 = C->getCallingConv(); R.Wide = C->getTailCallKind();
      if (!attributes(C->getAttributes(), R.Attributes)) return false;
    } else if (const auto *A = dyn_cast<AllocaInst>(&I)) {
      if (!A->isStaticAlloca() || !A->getAllocatedType()->isIntegerTy()) return false;
      R.Detail = type(A->getAllocatedType()); R.Detail2 = A->getAddressSpace();
      R.Reserved = A->isUsedWithInAlloca() | (uint32_t(A->isSwiftError()) << 1); R.Wide = A->getAlign().value();
    } else if (const auto *L = dyn_cast<LoadInst>(&I)) {
      if (!L->isSimple()) return false; R.Wide = L->getAlign().value();
    } else if (const auto *S = dyn_cast<StoreInst>(&I)) {
      if (!S->isSimple()) return false; R.Wide = S->getAlign().value();
    } else if (const auto *C = dyn_cast<ICmpInst>(&I)) R.Detail = C->getPredicate();
    else if (const auto *Phi = dyn_cast<PHINode>(&I)) {
      R.Auxiliary.Start = P.Auxiliary.size();
      for (unsigned N = 0; N != Phi->getNumIncomingValues(); ++N) {
        Ref B = value(Phi->getIncomingBlock(N)); if (B.Kind != BlockRef) return false;
        P.Auxiliary.push_back(B.ID);
      }
      R.Auxiliary.Count = Phi->getNumIncomingValues();
    } else if (const auto *Switch = dyn_cast<SwitchInst>(&I)) {
      // LLVM23 stores case values after the Uses, like PHI incoming blocks.
      // They are not raw operands and must be transported separately.
      R.Auxiliary.Start = P.Auxiliary.size();
      for (const auto &Case : Switch->cases()) {
        Ref K = value(Case.getCaseValue());
        if (K.Kind != ConstantRef || P.Constants[K.ID].Kind != IntegerConstant) return false;
        P.Auxiliary.push_back(K.ID);
        if (!bounded()) return false;
      }
      R.Auxiliary.Count = Switch->getNumCases();
    } else if (const auto *E = dyn_cast<ExtractValueInst>(&I)) {
      R.Auxiliary = {static_cast<uint32_t>(P.Auxiliary.size()), static_cast<uint32_t>(E->getNumIndices())};
      P.Auxiliary.insert(P.Auxiliary.end(), E->idx_begin(), E->idx_end());
    } else if (const auto *E = dyn_cast<InsertValueInst>(&I)) {
      R.Auxiliary = {static_cast<uint32_t>(P.Auxiliary.size()), static_cast<uint32_t>(E->getNumIndices())};
      P.Auxiliary.insert(P.Auxiliary.end(), E->idx_begin(), E->idx_end());
    } else if (!(isa<BinaryOperator>(I) || isa<SelectInst>(I) || isa<CondBrInst>(I) || isa<UncondBrInst>(I) ||
                 isa<SwitchInst>(I) || isa<ReturnInst>(I) || isa<UnreachableInst>(I) ||
                 isa<TruncInst>(I) || isa<ZExtInst>(I) || isa<SExtInst>(I) || isa<FreezeInst>(I))) return false;
    R.Operands.Start = P.Operands.size();
    for (const Use &U : I.operands()) {
      P.Operands.push_back(value(U.get())); if (!bounded()) return false;
    }
    R.Operands.Count = P.Operands.size() - R.Operands.Start;
    P.Instructions.push_back(R); return bounded();
  }
  bool capture(const Function &F) {
    if (!F.getReturnType()->isIntegerTy() || F.isVarArg() || F.getCallingConv() != CallingConv::C) return false;
    if (F.getInstructionCount() > Cap / sizeof(InstructionRow) || F.size() > Cap / sizeof(BlockRow)) return false;
    P.Arguments = F.arg_size(); P.ResultType = type(F.getReturnType());
    P.ArgumentTypes.Start = P.Auxiliary.size();
    for (const Argument &A : F.args()) {
      if (!A.getType()->isIntegerTy()) return false;
      P.Auxiliary.push_back(type(A.getType())); P.ArgumentNames.push_back(text(A.getName()));
    }
    P.ArgumentTypes.Count = P.Arguments;
    uint32_t ArgID = 0, BlockID = 0, InstID = 0;
    std::vector<const Value *> Ordered;
    for (const Argument &A : F.args()) {
      if (A.isUsedByMetadata()) return false;
      Values[&A] = {ArgumentRef, ArgID++}; Ordered.push_back(&A);
    }
    for (const BasicBlock &B : F) {
      if (B.hasAddressTaken()) return false;
      if (B.isUsedByMetadata()) return false;
      Values[&B] = {BlockRef, BlockID++}; Ordered.push_back(&B);
      for (const Instruction &I : B) {
        if (I.isUsedByMetadata() || I.hasDbgRecords()) return false;
        Values[&I] = {InstructionRef, InstID++}; Ordered.push_back(&I);
      }
    }
    if (!bounded() || InstID > Cap / sizeof(InstructionRow) || Ordered.size() > Cap / sizeof(UseList)) return false;
    for (const BasicBlock &B : F) {
      BlockRow R; R.Name = text(B.getName()); R.Instructions.Start = P.Instructions.size();
      for (const Instruction &I : B) if (!instruction(I)) return false;
      R.Instructions.Count = P.Instructions.size() - R.Instructions.Start;
      P.Blocks.push_back(R);
    }
    for (const Value *V : Ordered) {
      UseList L; L.Entries.Start = P.Uses.size();
      for (const Use &U : V->uses()) {
        auto It = Values.find(U.getUser());
        if (It == Values.end() || It->second.Kind != InstructionRef) return false;
        P.Uses.push_back({It->second.ID, U.getOperandNo()}); if (!bounded()) return false;
      }
      L.Entries.Count = P.Uses.size() - L.Entries.Start; P.UseLists.push_back(L);
      if (!bounded()) return false;
    }
    return bounded();
  }
};
}
namespace {
struct View {
  Header H;
  ArrayRef<TypeRow> Types;
  ArrayRef<ConstantRow> Constants;
  ArrayRef<BlockRow> Blocks;
  ArrayRef<InstructionRow> Instructions;
  ArrayRef<Ref> Operands;
  ArrayRef<uint32_t> Auxiliary;
  ArrayRef<AttributeRow> Attributes;
  ArrayRef<uint64_t> Words;
  StringRef Strings;
  ArrayRef<Span> Callees, ArgumentNames;
  ArrayRef<UseList> UseLists;
  ArrayRef<UseRow> Uses;
};
bool range(Span S, size_t N) { return S.Start <= N && S.Count <= N - S.Start; }
StringRef text(const View &V, Span S) { return V.Strings.substr(S.Start, S.Count); }
uint32_t allowedFlags(unsigned Op) {
  switch (Op) {
  case Instruction::Add: case Instruction::Sub: case Instruction::Mul: case Instruction::Shl:
  case Instruction::Trunc: return 3;
  case Instruction::UDiv: case Instruction::SDiv: case Instruction::LShr: case Instruction::AShr:
  case Instruction::Or: case Instruction::ICmp: case Instruction::ZExt: return 1;
  default: return 0;
  }
}
bool sameType(const View &V, uint32_t ID, const Type *T) {
  if (ID >= V.Types.size()) return false;
  const TypeRow &R = V.Types[ID];
  switch (R.Kind) {
  case VoidTy: return T->isVoidTy();
  case IntegerTy: return T->isIntegerTy(R.Detail);
  case PointerTy: return T->isPointerTy() && T->getPointerAddressSpace() == R.Detail;
  case StructTy: {
    const auto *S = dyn_cast<StructType>(T);
    if (!S || S->isOpaque() || S->isPacked() != bool(R.Detail & 1) ||
        S->isLiteral() == bool(R.Detail & 2) || S->getNumElements() != R.Elements.Count ||
        (S->isLiteral() ? StringRef() : S->getName()) != text(V, R.Name)) return false;
    for (unsigned I = 0; I != R.Elements.Count; ++I)
      if (!sameType(V, V.Auxiliary[R.Elements.Start + I], S->getElementType(I))) return false;
    return true;
  }
  default: return false;
  }
}
template <typename T> bool readSection(const uint8_t *Data, size_t Size, Span S, ArrayRef<T> &Out) {
  if (S.Start > Size || S.Count > (Size - S.Start) / sizeof(T) || S.Start % alignof(T)) return false;
  Out = ArrayRef<T>(reinterpret_cast<const T *>(Data + S.Start), S.Count); return true;
}
bool decode(const uint8_t *Data, size_t Size, View &V) {
  if (Size < sizeof(Header)) return false;
  std::memcpy(&V.H, Data, sizeof(Header));
  if (V.H.Version != Magic) return false;
  if (!readSection(Data, Size, V.H.Sections[Types], V.Types) ||
      !readSection(Data, Size, V.H.Sections[Constants], V.Constants) ||
      !readSection(Data, Size, V.H.Sections[Blocks], V.Blocks) ||
      !readSection(Data, Size, V.H.Sections[Instructions], V.Instructions) ||
      !readSection(Data, Size, V.H.Sections[Operands], V.Operands) ||
      !readSection(Data, Size, V.H.Sections[Auxiliary], V.Auxiliary) ||
      !readSection(Data, Size, V.H.Sections[Attributes], V.Attributes) ||
      !readSection(Data, Size, V.H.Sections[Words], V.Words) ||
      !readSection(Data, Size, V.H.Sections[Callees], V.Callees) ||
      !readSection(Data, Size, V.H.Sections[ArgumentNames], V.ArgumentNames)) return false;
  Span S = V.H.Sections[Strings];
  if (!range(S, Size)) return false;
  V.Strings = StringRef(reinterpret_cast<const char *>(Data + S.Start), S.Count);
  size_t Owned = size_t(V.H.Arguments) + V.Blocks.size() + V.Instructions.size();
  S = V.H.Sections[Uses];
  if (Owned > Limit / sizeof(UseList) || !readSection(Data, Size, Span{S.Start, uint32_t(Owned)}, V.UseLists)) return false;
  size_t UseStart = S.Start + Owned * sizeof(UseList);
  if (UseStart > UINT32_MAX || !readSection(Data, Size, Span{uint32_t(UseStart), S.Count}, V.Uses)) return false;
  for (size_t I = 0; I != V.Types.size(); ++I) {
    const TypeRow &T = V.Types[I];
    if (!range(T.Name, V.Strings.size()) || !range(T.Elements, V.Auxiliary.size())) return false;
    switch (T.Kind) {
    case VoidTy: if (T.Detail || T.Elements.Count || T.Name.Count) return false; break;
    case IntegerTy:
      if (!T.Detail || T.Detail > IntegerType::MAX_INT_BITS || T.Elements.Count || T.Name.Count) return false;
      break;
    case PointerTy: if (T.Elements.Count || T.Name.Count) return false; break;
    case StructTy:
      if (T.Detail > 3 || bool(T.Detail & 2) != bool(T.Name.Count)) return false;
      for (uint32_t E : V.Auxiliary.slice(T.Elements.Start, T.Elements.Count))
        if (E >= I || V.Types[E].Kind != IntegerTy) return false;
      break;
    default: return false;
    }
  }
  if (V.H.ResultType >= V.Types.size() || V.Types[V.H.ResultType].Kind != IntegerTy ||
      V.H.ArgumentTypes.Count != V.H.Arguments || !range(V.H.ArgumentTypes, V.Auxiliary.size())) return false;
  for (uint32_t T : V.Auxiliary.slice(V.H.ArgumentTypes.Start, V.H.ArgumentTypes.Count))
    if (T >= V.Types.size() || V.Types[T].Kind != IntegerTy) return false;
  for (const ConstantRow &C : V.Constants) {
    if (C.Type >= V.Types.size() || !range(C.Words, V.Words.size())) return false;
    const TypeRow &T = V.Types[C.Type];
    if (T.Kind != IntegerTy && T.Kind != StructTy) return false;
    if (C.Kind == IntegerConstant) {
      if (T.Kind != IntegerTy || C.Words.Count != (uint64_t(T.Detail) + 63) / 64) return false;
      if (T.Detail % 64 && (V.Words[C.Words.Start + C.Words.Count - 1] >> (T.Detail % 64))) return false;
    } else if (C.Kind > ZeroConstant || C.Words.Count || (C.Kind == ZeroConstant && T.Kind != StructTy)) return false;
  }
  size_t Next = 0;
  for (const BlockRow &B : V.Blocks) {
    if (!range(B.Name, V.Strings.size()) || !range(B.Instructions, V.Instructions.size()) ||
        B.Instructions.Start != Next || !B.Instructions.Count) return false;
    Next += B.Instructions.Count;
  }
  if (Next != V.Instructions.size() || V.Blocks.empty()) return false;
  if (V.ArgumentNames.size() != V.H.Arguments) return false;
  for (Span Name : V.ArgumentNames) if (!range(Name, V.Strings.size())) return false;
  for (Span C : V.Callees) if (!range(C, V.Strings.size()) || !C.Count) return false;
  for (const Ref &R : V.Operands) {
    size_t N = 0;
    switch (R.Kind) {
    case ArgumentRef: N = V.H.Arguments; break;
    case BlockRef: N = V.Blocks.size(); break;
    case InstructionRef: N = V.Instructions.size(); break;
    case ConstantRef: N = V.Constants.size(); break;
    case CalleeRef: N = V.Callees.size(); break;
    default: return false;
    }
    if (R.ID >= N) return false;
  }
  for (const InstructionRow &I : V.Instructions) {
    if (I.Type >= V.Types.size() || !range(I.Name, V.Strings.size()) ||
        !range(I.Operands, V.Operands.size()) || !range(I.Auxiliary, V.Auxiliary.size()) ||
        !range(I.Attributes, V.Attributes.size()) || (I.Flags & ~allowedFlags(I.Opcode))) return false;
    auto IsBlock = [&](unsigned N) { return N < I.Operands.Count && V.Operands[I.Operands.Start + N].Kind == BlockRef; };
    switch (I.Opcode) {
    case Instruction::Add: case Instruction::Sub: case Instruction::Mul: case Instruction::UDiv:
    case Instruction::SDiv: case Instruction::URem: case Instruction::SRem: case Instruction::Shl:
    case Instruction::LShr: case Instruction::AShr: case Instruction::And: case Instruction::Or:
    case Instruction::Xor: case Instruction::ICmp:
      if (I.Operands.Count != 2) return false;
      if (I.Opcode == Instruction::ICmp && (I.Detail < CmpInst::FIRST_ICMP_PREDICATE || I.Detail > CmpInst::LAST_ICMP_PREDICATE)) return false;
      break;
    case Instruction::Trunc: case Instruction::ZExt: case Instruction::SExt: case Instruction::Freeze:
    case Instruction::Load: case Instruction::ExtractValue: case Instruction::Ret:
      if (I.Operands.Count != 1) return false; break;
    case Instruction::Store: case Instruction::InsertValue:
      if (I.Operands.Count != 2) return false; break;
    case Instruction::Select: if (I.Operands.Count != 3) return false; break;
    case Instruction::UncondBr: if (I.Operands.Count != 1 || !IsBlock(0)) return false; break;
    case Instruction::CondBr: if (I.Operands.Count != 3 || !IsBlock(1) || !IsBlock(2)) return false; break;
    case Instruction::Unreachable: if (I.Operands.Count) return false; break;
    case Instruction::PHI:
      if (I.Operands.Count != I.Auxiliary.Count) return false;
      for (uint32_t B : V.Auxiliary.slice(I.Auxiliary.Start, I.Auxiliary.Count)) if (B >= V.Blocks.size()) return false;
      break;
    case Instruction::Alloca:
      if (I.Operands.Count != 1 || I.Detail >= V.Types.size() || V.Types[I.Detail].Kind != IntegerTy ||
          V.Types[I.Type].Kind != PointerTy || V.Types[I.Type].Detail != I.Detail2 || I.Reserved > 3) return false;
      break;
    case Instruction::Switch:
      if (I.Operands.Count < 2 || I.Auxiliary.Count != I.Operands.Count - 2 || !IsBlock(1)) return false;
      for (unsigned N = 2; N < I.Operands.Count; ++N) if (!IsBlock(N)) return false;
      for (uint32_t K : V.Auxiliary.slice(I.Auxiliary.Start, I.Auxiliary.Count))
        if (K >= V.Constants.size() || V.Constants[K].Kind != IntegerConstant) return false;
      break;
    case Instruction::Call:
      if (!I.Operands.Count || I.Detail >= V.Callees.size() || I.Detail2 != CallingConv::C ||
          I.Wide > CallInst::TCK_NoTail) return false;
      if (V.Operands[I.Operands.Start + I.Operands.Count - 1].Kind != CalleeRef ||
          V.Operands[I.Operands.Start + I.Operands.Count - 1].ID != I.Detail) return false;
      break;
    default: return false;
    }
    if (I.Opcode == Instruction::Alloca || I.Opcode == Instruction::Load || I.Opcode == Instruction::Store)
      if (!I.Wide || (I.Wide & (I.Wide - 1)) || I.Wide > Value::MaximumAlignment) return false;
    if ((I.Opcode == Instruction::ExtractValue || I.Opcode == Instruction::InsertValue) && !I.Auxiliary.Count) return false;
    if (I.Opcode != Instruction::Call && I.Attributes.Count) return false;
  }
  for (const AttributeRow &A : V.Attributes) {
    if (!range(A.Name, V.Strings.size()) || !range(A.Value, V.Strings.size()) || A.Form > StringAttribute) return false;
    if (A.Form != StringAttribute && (A.Kind <= Attribute::None || A.Kind >= Attribute::EndAttrKinds)) return false;
  }
  for (const UseList &L : V.UseLists) if (!range(L.Entries, V.Uses.size())) return false;
  for (const UseRow &U : V.Uses)
    if (U.User >= V.Instructions.size() || U.Operand >= V.Instructions[U.User].Operands.Count) return false;
  return true;
}
}
struct StageBody::Storage {
  std::unique_ptr<uint8_t[]> Bytes;
  size_t Size = 0;
  uint64_t Digest = 0;
};
struct StageBody::Prepared::State {
  View V;
  const Storage *Owner = nullptr;
  Function *Current = nullptr;
  std::vector<Function *> Callees;
};
StageBody::StageBody(std::unique_ptr<Storage> S) : S(std::move(S)) {}
StageBody::~StageBody() = default;
StageBody::Prepared::Prepared(std::unique_ptr<State> S) : S(std::move(S)) {}
StageBody::Prepared::~Prepared() = default;
size_t StageBody::ownedBytes() const { return sizeof(StageBody) + sizeof(Storage) + S->Size; }
std::shared_ptr<const StageBody> StageBody::capture(const Function &F, size_t MaxBytes) {
  // Actual LLVM function verifier, not a typed-schema substitute. The caller
  // additionally verifies every uniquely referenced canonical declaration and
  // rejects module-wide special states before selecting this closed producer.
  if (!F.getParent() || verifyFunction(F, nullptr) || MaxBytes > Limit) return {};
  Encoder E(MaxBytes);
  if (!E.capture(F)) return {};
  size_t Bytes;
  if (!payloadBytes(E.P, Bytes) || !add(Bytes, 128)) return {};
  size_t Charged = Bytes;
  if (!add(Charged, sizeof(StageBody) + sizeof(Storage)) || Charged > MaxBytes) return {};
  auto S = std::make_unique<Storage>();
  // One allocation owns every dynamic resident field, including use ranks.
  S->Bytes = std::make_unique<uint8_t[]>(Bytes);
  Header H; H.Arguments = E.P.Arguments; H.ResultType = E.P.ResultType; H.ArgumentTypes = E.P.ArgumentTypes;
  size_t At = sizeof(Header);
  auto Write = [&](Section Index, const auto &Rows) {
    using T = typename std::decay_t<decltype(Rows)>::value_type;
    static_assert(std::is_trivially_copyable_v<T>);
    At = (At + alignof(T) - 1) & ~(alignof(T) - 1);
    H.Sections[Index] = {uint32_t(At), uint32_t(Rows.size())};
    for (size_t I = 0; I != Rows.size(); ++I) new (S->Bytes.get() + At + I * sizeof(T)) T(Rows[I]);
    At += Rows.size() * sizeof(T);
  };
  Write(Types, E.P.Types); Write(Constants, E.P.Constants); Write(Blocks, E.P.Blocks);
  Write(Instructions, E.P.Instructions); Write(Operands, E.P.Operands); Write(Auxiliary, E.P.Auxiliary);
  Write(Attributes, E.P.Attributes); Write(Words, E.P.Words); Write(Strings, E.P.Strings); Write(Callees, E.P.Callees); Write(ArgumentNames, E.P.ArgumentNames);
  At = (At + alignof(UseList) - 1) & ~(alignof(UseList) - 1);
  H.Sections[Uses] = {uint32_t(At), uint32_t(E.P.Uses.size())};
  for (size_t I = 0; I != E.P.UseLists.size(); ++I)
    new (S->Bytes.get() + At + I * sizeof(UseList)) UseList(E.P.UseLists[I]);
  At += E.P.UseLists.size() * sizeof(UseList);
  for (size_t I = 0; I != E.P.Uses.size(); ++I)
    new (S->Bytes.get() + At + I * sizeof(UseRow)) UseRow(E.P.Uses[I]);
  At += E.P.Uses.size() * sizeof(UseRow);
  if (At > Bytes) return {};
  // Keep the complete allocated size charged, including alignment/tail slack.
  S->Size = Bytes; new (S->Bytes.get()) Header(H);
  S->Digest = xxh3_64bits(S->Bytes.get(), S->Size);
  View Check; if (!decode(S->Bytes.get(), S->Size, Check)) return {};
  return std::shared_ptr<const StageBody>(new StageBody(std::move(S)));
}
std::unique_ptr<StageBody::Prepared> StageBody::prepare(Function &F) const {
  if (S->Digest != xxh3_64bits(S->Bytes.get(), S->Size)) return {};
  auto P = std::make_unique<Prepared::State>();
  if (!decode(S->Bytes.get(), S->Size, P->V) || P->V.H.Arguments != F.arg_size() || !F.getParent()) return {};
  if (F.isVarArg() || F.getCallingConv() != CallingConv::C || !sameType(P->V, P->V.H.ResultType, F.getReturnType())) return {};
  for (const Argument &A : F.args())
    if (!sameType(P->V, P->V.Auxiliary[P->V.H.ArgumentTypes.Start + A.getArgNo()], A.getType())) return {};
  for (const TypeRow &T : P->V.Types) if (T.Kind == StructTy && (T.Detail & 2)) {
    StructType *Current = StructType::getTypeByName(F.getContext(), text(P->V, T.Name));
    uint32_t ID = &T - P->V.Types.data();
    if (!Current || !sameType(P->V, ID, Current)) return {};
  }
  for (Span Name : P->V.Callees) {
    Function *D = F.getParent()->getFunction(text(P->V, Name));
    if (!D || !D->isDeclaration() || !D->isIntrinsic() || D->isVarArg()) return {};
    P->Callees.push_back(D);
  }
  // The sealed payload came from valid actual IR. Bind every varying current
  // signature before construction; no new Context object is made here.
  for (const InstructionRow &I : P->V.Instructions) if (I.Opcode == Instruction::Call) {
    Function *D = P->Callees[I.Detail];
    if (I.Operands.Count != D->arg_size() + 1 || !sameType(P->V, I.Type, D->getReturnType())) return {};
    for (unsigned N = 0; N != D->arg_size(); ++N) {
      Ref R = P->V.Operands[I.Operands.Start + N]; uint32_t T;
      if (R.Kind == ArgumentRef) T = P->V.Auxiliary[P->V.H.ArgumentTypes.Start + R.ID];
      else if (R.Kind == InstructionRef) T = P->V.Instructions[R.ID].Type;
      else if (R.Kind == ConstantRef) T = P->V.Constants[R.ID].Type;
      else return {};
      if (!sameType(P->V, T, D->getFunctionType()->getParamType(N))) return {};
    }
    for (const AttributeRow &A : P->V.Attributes.slice(I.Attributes.Start, I.Attributes.Count))
      if (A.Index != AttributeList::FunctionIndex && A.Index > D->arg_size()) return {};
  }
  P->Owner = S.get(); P->Current = &F;
  return std::unique_ptr<Prepared>(new Prepared(std::move(P)));
}
bool StageBody::restore(Function &F, Prepared &PreparedBody) const {
  auto &P = *PreparedBody.S;
  if (P.Owner != S.get() || P.Current != &F) return false;
  const View &V = P.V;
  LLVMContext &C = F.getContext();
  std::vector<Type *> Types; Types.reserve(V.Types.size());
  for (const TypeRow &T : V.Types) {
    Type *R = nullptr;
    switch (T.Kind) {
    case VoidTy: R = Type::getVoidTy(C); break;
    case IntegerTy: R = IntegerType::get(C, T.Detail); break;
    case PointerTy: R = PointerType::get(C, T.Detail); break;
    case StructTy:
      if (T.Detail & 2) R = StructType::getTypeByName(C, text(V, T.Name));
      else {
        SmallVector<Type *, 4> Elements;
        for (uint32_t E : V.Auxiliary.slice(T.Elements.Start, T.Elements.Count)) Elements.push_back(Types[E]);
        R = StructType::get(C, Elements, T.Detail & 1);
      }
      break;
    default: return false;
    }
    if (!R) return false; Types.push_back(R);
  }
  std::vector<Constant *> Constants; Constants.reserve(V.Constants.size());
  for (const ConstantRow &K : V.Constants) {
    Type *T = Types[K.Type]; Constant *R = nullptr;
    switch (K.Kind) {
    case IntegerConstant: R = ConstantInt::get(C, APInt(cast<IntegerType>(T)->getBitWidth(), V.Words.slice(K.Words.Start, K.Words.Count))); break;
    case PoisonConstant: R = PoisonValue::get(T); break;
    case UndefConstant: R = UndefValue::get(T); break;
    case ZeroConstant: R = ConstantAggregateZero::get(T); break;
    default: return false;
    }
    Constants.push_back(R);
  }
  const auto Linkage = F.getLinkage(); const bool DSOLocal = F.isDSOLocal();
  // The consumer has cleared FAM and bound the complete current header. Do not
  // import attributes from another Module or leave deleteBody's External change.
  F.deleteBody(); F.setLinkage(Linkage); F.setDSOLocal(DSOLocal);
  for (Argument &A : F.args()) A.setName("");
  for (Argument &A : F.args()) A.setName(text(V, V.ArgumentNames[A.getArgNo()]));
  std::vector<BasicBlock *> Blocks; Blocks.reserve(V.Blocks.size());
  for (const BlockRow &B : V.Blocks) Blocks.push_back(BasicBlock::Create(C, text(V, B.Name), &F));
  std::vector<Instruction *> Instructions(V.Instructions.size(), nullptr);
  auto ValueFor = [&](Ref R, bool Forward) -> Value * {
    switch (R.Kind) {
    case ArgumentRef: return F.getArg(R.ID);
    case BlockRef: return Blocks[R.ID];
    case InstructionRef:
      if (Instructions[R.ID]) return Instructions[R.ID];
      if (Forward && !Types[V.Instructions[R.ID].Type]->isVoidTy()) return PoisonValue::get(Types[V.Instructions[R.ID].Type]);
      return nullptr;
    case ConstantRef: return Constants[R.ID];
    case CalleeRef: return P.Callees[R.ID];
    default: return nullptr;
    }
  };
  for (size_t B = 0; B != V.Blocks.size(); ++B) {
    const BlockRow &Block = V.Blocks[B];
    for (uint32_t N = Block.Instructions.Start; N != Block.Instructions.Start + Block.Instructions.Count; ++N) {
      const InstructionRow &R = V.Instructions[N];
      auto Operand = [&](unsigned I) { return ValueFor(V.Operands[R.Operands.Start + I], true); };
      auto Indices = V.Auxiliary.slice(R.Auxiliary.Start, R.Auxiliary.Count);
      StringRef Name = text(V, R.Name);
      Instruction *I = nullptr;
      switch (R.Opcode) {
      case Instruction::Add: case Instruction::Sub: case Instruction::Mul: case Instruction::UDiv:
      case Instruction::SDiv: case Instruction::URem: case Instruction::SRem: case Instruction::Shl:
      case Instruction::LShr: case Instruction::AShr: case Instruction::And: case Instruction::Or: case Instruction::Xor:
        I = BinaryOperator::Create(static_cast<Instruction::BinaryOps>(R.Opcode), Operand(0), Operand(1), Name); break;
      case Instruction::ICmp: I = new ICmpInst(static_cast<CmpInst::Predicate>(R.Detail), Operand(0), Operand(1), Name); break;
      case Instruction::PHI: {
        auto *Phi = PHINode::Create(Types[R.Type], R.Operands.Count, Name);
        for (unsigned K = 0; K != R.Operands.Count; ++K) Phi->addIncoming(Operand(K), Blocks[Indices[K]]);
        I = Phi; break;
      }
      case Instruction::Select: I = SelectInst::Create(Operand(0), Operand(1), Operand(2), Name); break;
      case Instruction::Trunc: I = new TruncInst(Operand(0), Types[R.Type], Name); break;
      case Instruction::ZExt: I = new ZExtInst(Operand(0), Types[R.Type], Name); break;
      case Instruction::SExt: I = new SExtInst(Operand(0), Types[R.Type], Name); break;
      case Instruction::Freeze: I = new FreezeInst(Operand(0), Name); break;
      case Instruction::UncondBr: I = UncondBrInst::Create(cast<BasicBlock>(Operand(0))); break;
      // The pinned CondBr raw operand order is Cond, False, True.
      case Instruction::CondBr: I = CondBrInst::Create(Operand(0), cast<BasicBlock>(Operand(2)), cast<BasicBlock>(Operand(1))); break;
      case Instruction::Switch: {
        auto *Switch = SwitchInst::Create(Operand(0), cast<BasicBlock>(Operand(1)), Indices.size());
        for (unsigned K = 0; K != Indices.size(); ++K)
          Switch->addCase(cast<ConstantInt>(Constants[Indices[K]]), cast<BasicBlock>(Operand(K + 2)));
        I = Switch; break;
      }
      case Instruction::Ret: I = ReturnInst::Create(C, Operand(0)); break;
      case Instruction::Unreachable: I = new UnreachableInst(C); break;
      case Instruction::ExtractValue: I = ExtractValueInst::Create(Operand(0), Indices, Name); break;
      case Instruction::InsertValue: I = InsertValueInst::Create(Operand(0), Operand(1), Indices, Name); break;
      case Instruction::Alloca: {
        auto *A = new AllocaInst(Types[R.Detail], R.Detail2, Operand(0), Align(R.Wide), Name);
        A->setUsedWithInAlloca(R.Reserved & 1); A->setSwiftError(R.Reserved & 2); I = A; break;
      }
      case Instruction::Load: I = new LoadInst(Types[R.Type], Operand(0), Name, false, Align(R.Wide)); break;
      case Instruction::Store: I = new StoreInst(Operand(0), Operand(1), false, Align(R.Wide)); break;
      case Instruction::Call: {
        Function *D = P.Callees[R.Detail]; SmallVector<Value *, 4> Args;
        for (unsigned K = 0; K + 1 < R.Operands.Count; ++K) Args.push_back(Operand(K));
        auto *Call = CallInst::Create(D->getFunctionType(), D, Args, Name);
        Call->setCallingConv(R.Detail2); Call->setTailCallKind(static_cast<CallInst::TailCallKind>(R.Wide));
        AttributeList Attributes;
        for (const AttributeRow &A : V.Attributes.slice(R.Attributes.Start, R.Attributes.Count)) {
          Attribute Attr;
          if (A.Form == StringAttribute) Attr = Attribute::get(C, text(V, A.Name), text(V, A.Value));
          else if (A.Form == IntAttribute) Attr = Attribute::get(C, static_cast<Attribute::AttrKind>(A.Kind), A.Integer);
          else Attr = Attribute::get(C, static_cast<Attribute::AttrKind>(A.Kind));
          Attributes = Attributes.addAttributeAtIndex(C, A.Index, Attr);
        }
        Call->setAttributes(Attributes); I = Call; break;
      }
      default: return false;
      }
      I->insertInto(Blocks[B], Blocks[B]->end()); Instructions[N] = I;
      if (auto *T = dyn_cast<TruncInst>(I)) { T->setHasNoUnsignedWrap(R.Flags & 1); T->setHasNoSignedWrap(R.Flags & 2); }
      else if (isa<OverflowingBinaryOperator>(I)) { I->setHasNoUnsignedWrap(R.Flags & 1); I->setHasNoSignedWrap(R.Flags & 2); }
      else if (isa<PossiblyExactOperator>(I)) I->setIsExact(R.Flags & 1);
      else if (auto *D = dyn_cast<PossiblyDisjointInst>(I)) D->setIsDisjoint(R.Flags & 1);
      else if (auto *Compare = dyn_cast<ICmpInst>(I)) Compare->setSameSign(R.Flags & 1);
      else if (isa<ZExtInst>(I)) I->setNonNeg(R.Flags & 1);
      if (I->getRawSubclassOptionalData() != R.Flags || I->getNumOperands() != R.Operands.Count) return false;
    }
  }
  // Exactly reproduce pinned CloneFunction.cpp RemapInstruction: every operand
  // is assigned in physical BB/Inst/operand order, EVEN when pointer-equal.
  // Values with use-lists (the current callees) get this function's reverse
  // prefix and the untouched external suffix. LLVM23 ConstantData leaves have
  // no use-list; setting their operands preserves the current interned value.
  for (size_t N = 0; N != Instructions.size(); ++N) {
    Instruction *I = Instructions[N]; const InstructionRow &R = V.Instructions[N];
    for (unsigned K = 0; K != R.Operands.Count; ++K) {
      Value *Current = ValueFor(V.Operands[R.Operands.Start + K], false);
      if (!Current) return false; I->setOperand(K, Current);
    }
  }
  std::vector<Value *> Owned;
  for (Argument &A : F.args()) Owned.push_back(&A);
  for (BasicBlock *B : Blocks) { Owned.push_back(B); for (Instruction &I : *B) Owned.push_back(&I); }
  if (Owned.size() != V.UseLists.size()) return false;
  for (size_t N = 0; N != Owned.size(); ++N) {
    Value *Value = Owned[N]; const UseList &L = V.UseLists[N];
    DenseMap<const Use *, uint32_t> Ranks;
    for (unsigned K = 0; K != L.Entries.Count; ++K) {
      const UseRow &U = V.Uses[L.Entries.Start + K];
      Use *Current = &Instructions[U.User]->getOperandUse(U.Operand);
      if (Current->get() != Value || !Ranks.try_emplace(Current, K).second) return false;
    }
    if (Ranks.size() != Value->getNumUses()) return false;
    Value->sortUseList([&](const Use &A, const Use &B) { return Ranks.lookup(&A) < Ranks.lookup(&B); });
  }
  return true; // The actual consumer performs the unchanged current verifier.
}
}
namespace zil {
int StageBody::counterexamples() {
  auto Fail = [](const char *Site) { std::fprintf(stderr, "ZIL typed IR fixture: %s\n", Site); return 0; };
  LLVMContext C; Module M("typed-stage-body", C);
  M.setTargetTriple(Triple("x86_64-unknown-linux-gnu"));
  auto *I130 = IntegerType::get(C, 130); auto *I32 = Type::getInt32Ty(C);
  auto *F = Function::Create(FunctionType::get(I130, {I130, Type::getInt1Ty(C)}, false), GlobalValue::ExternalLinkage, "wide", &M);
  F->getArg(0)->setName("x"); F->getArg(1)->setName("condition");
  auto *Entry = BasicBlock::Create(C, "entry", F);
  auto *Left = BasicBlock::Create(C, "left", F); auto *Right = BasicBlock::Create(C, "right", F);
  auto *Merge = BasicBlock::Create(C, "merge", F); auto *Exit = BasicBlock::Create(C, "exit", F);
  auto *Special = BasicBlock::Create(C, "special", F);
  auto *Slot = new AllocaInst(I130, 0, ConstantInt::get(I32, 1), Align(32), "slot", Entry);
  new StoreInst(F->getArg(0), Slot, false, Align(32), Entry);
  auto *Load = new LoadInst(I130, Slot, "load", false, Align(32), Entry);
  auto *Trunc = new TruncInst(Load, I32, "trunc", Entry); Trunc->setHasNoUnsignedWrap(true); Trunc->setHasNoSignedWrap(true);
  auto *Ext = new ZExtInst(Trunc, I130, "extend", Entry); Ext->setNonNeg(true);
  auto *Signed = new SExtInst(Trunc, I130, "signed", Entry);
  auto *Poison = new FreezeInst(PoisonValue::get(I130), "freeze", Entry);
  auto *Unused = new FreezeInst(UndefValue::get(I130), "undef", Entry); (void)Unused;
  auto *Pair = StructType::get(C, {I130, I130});
  auto *Inserted = InsertValueInst::Create(ConstantAggregateZero::get(Pair), Ext, {0}, "inserted", Entry);
  ExtractValueInst::Create(Inserted, {0}, "extracted", Entry);
  CondBrInst::Create(F->getArg(1), Left, Right, Entry);
  APInt Wide(130, {uint64_t(3), uint64_t(7), uint64_t(2)});
  auto *Add = BinaryOperator::CreateAdd(Load, ConstantInt::get(C, Wide), "add", Left);
  Add->setHasNoUnsignedWrap(true); Add->setHasNoSignedWrap(true);
  auto *Exact = BinaryOperator::CreateLShr(Add, ConstantInt::get(I130, 1), "exact", Left); Exact->setIsExact(true);
  auto *Compare = new ICmpInst(CmpInst::ICMP_ULT, Exact, Signed, "compare"); Compare->setSameSign(true); Compare->insertInto(Left, Left->end());
  auto *Selected = SelectInst::Create(Compare, Exact, Poison, "select", Left);
  UncondBrInst::Create(Merge, Left);
  auto *Or = BinaryOperator::CreateOr(Load, ConstantInt::get(I130, 8), "disjoint", Right);
  cast<PossiblyDisjointInst>(Or)->setIsDisjoint(true); UncondBrInst::Create(Merge, Right);
  auto *Phi = PHINode::Create(I130, 2, "phi", Merge); Phi->addIncoming(Selected, Left); Phi->addIncoming(Or, Right);
  auto *Switch = SwitchInst::Create(Phi, Exit, 2, Merge);
  Switch->addCase(ConstantInt::get(C, APInt(130, 1)), Exit);
  // Two ordered cases, including a multiword value and distinct successor,
  // cover the real non-Use table as well as the actual operand/BB use order.
  Switch->addCase(ConstantInt::get(C, APInt(130, {uint64_t(5), uint64_t(9), uint64_t(1)})), Special);
  ReturnInst::Create(C, Phi, Exit); ReturnInst::Create(C, F->getArg(0), Special);
  if (verifyModule(M, &errs())) return Fail("initial-wide-verifier");
  // LLVM23 ConstantData has no use-list. Its outside operand must keep the
  // same current interned leaf; the real external use-list oracle is below.
  auto *Outside = Function::Create(FunctionType::get(I130, {I130}, false), GlobalValue::ExternalLinkage, "outside", &M);
  auto *OutsideBB = BasicBlock::Create(C, "entry", Outside);
  auto *OutsideAdd = BinaryOperator::CreateAdd(Outside->getArg(0), ConstantInt::get(C, Wide), "outside.add", OutsideBB);
  ReturnInst::Create(C, OutsideAdd, OutsideBB);
  auto Body = capture(*F, Limit); if (!Body || Body->ownedBytes() > Limit) return Fail("wide-capture");
  auto Printed = [](Function &F) { std::string S; raw_string_ostream OS(S); F.print(OS); return S; };
  std::string Before = Printed(*F);
  const Use *ExternalUse = &OutsideAdd->getOperandUse(1);
  const Value *ExternalLeaf = ExternalUse->get();
  if (ExternalLeaf->hasUseList()) return Fail("leaf-unexpected-use-list");
  auto PreparedBody = Body->prepare(*F); if (!PreparedBody) return Fail("wide-prepare");
  if (!Body->restore(*F, *PreparedBody)) return Fail("wide-restore");
  if (verifyModule(M, &errs())) return Fail("restored-wide-verifier");
  if (Printed(*F) != Before) { std::fprintf(stderr, "EXPECTED [%s]\nACTUAL [%s]\n", Before.c_str(), Printed(*F).c_str()); return Fail("restored-wide-text"); }
  auto Recaptured = capture(*F, Limit);
  if (!Recaptured || Recaptured->S->Size != Body->S->Size ||
      std::memcmp(Recaptured->S->Bytes.get(), Body->S->Bytes.get(), Body->S->Size)) return Fail("wide-recapture-payload");
  if (&OutsideAdd->getOperandUse(1) != ExternalUse || ExternalUse->get() != ExternalLeaf ||
      ConstantInt::get(C, Wide) != ExternalLeaf) return Fail("outside-leaf-operand-identity");
  if (capture(*F, Body->ownedBytes() - 1)) return Fail("owned-byte-boundary");
  uint8_t Original = Body->S->Bytes[0]; Body->S->Bytes[0] ^= 1;
  if (Body->prepare(*F) || Printed(*F) != Before) return Fail("corrupt-blob-mutation");
  Body->S->Bytes[0] = Original;
  if (!Body->prepare(*F)) return Fail("restored-blob-prepare");
  // A same-arity changed signature must refuse before modifying the current IR.
  auto *Other = Function::Create(FunctionType::get(I32, {I32, Type::getInt1Ty(C)}, false), GlobalValue::ExternalLinkage, "other", &M);
  auto *OB = BasicBlock::Create(C, "entry", Other); ReturnInst::Create(C, Other->getArg(0), OB);
  if (Body->prepare(*Other)) return Fail("changed-signature-admitted");
  // Non-leaf constant DAGs remain available via the original bitcode path.
  auto *Unsupported = Function::Create(FunctionType::get(I130, false), GlobalValue::ExternalLinkage, "dag", &M);
  auto *UB = BasicBlock::Create(C, "entry", Unsupported);
  auto *Aggregate = ConstantStruct::get(Pair, {ConstantInt::get(I130, 1), ConstantInt::get(I130, 2)});
  auto *Extract = ExtractValueInst::Create(Aggregate, {0}, "extract", UB); ReturnInst::Create(C, Extract, UB);
  if (verifyModule(M, &errs()) || capture(*Unsupported, Limit)) return Fail("constant-dag-refusal");
  // Calls bind CURRENT declarations; full callsite attrs and tail kind survive.
  auto *D = Intrinsic::getOrInsertDeclaration(&M, Intrinsic::sadd_with_overflow, {I32});
  auto *Calls = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "calls", &M);
  auto *CB = BasicBlock::Create(C, "entry", Calls);
  auto *Call = CallInst::Create(D->getFunctionType(), D, {Calls->getArg(0), ConstantInt::get(I32, 1)}, "call", CB);
  Call->setTailCallKind(CallInst::TCK_NoTail);
  Call->addFnAttr(Attribute::get(C, "typed-test", "complete-value"));
  auto *Result = ExtractValueInst::Create(Call, {0}, "result", CB); ReturnInst::Create(C, Result, CB);
  auto *Caller = Function::Create(FunctionType::get(I32, {I32}, false), GlobalValue::ExternalLinkage, "outside.caller", &M);
  auto *CallerBB = BasicBlock::Create(C, "entry", Caller);
  auto *ExternalCall = CallInst::Create(D->getFunctionType(), D, {Caller->getArg(0), ConstantInt::get(I32, 1)}, "outside.call", CallerBB);
  auto *ExternalResult = ExtractValueInst::Create(ExternalCall, {0}, "outside.result", CallerBB);
  ReturnInst::Create(C, ExternalResult, CallerBB);
  const Use *ExternalCalleeUse = &ExternalCall->getOperandUse(ExternalCall->getNumOperands() - 1);
  if (!D->hasUseList() || D->getNumUses() != 2) return Fail("callee-use-oracle-shape");
  if (verifyModule(M, &errs())) return Fail("initial-call-verifier");
  std::string CallBefore = Printed(*Calls);
  auto CallBody = capture(*Calls, Limit); if (!CallBody) return Fail("intrinsic-call-capture");
  auto CallPrepared = CallBody->prepare(*Calls);
  if (!CallPrepared) return Fail("intrinsic-call-prepare");
  if (!CallBody->restore(*Calls, *CallPrepared)) return Fail("intrinsic-call-restore");
  if (verifyModule(M, &errs())) return Fail("restored-call-verifier");
  if (Printed(*Calls) != CallBefore) return Fail("restored-call-text");
  const auto *RestoredCall = dyn_cast<CallInst>(&Calls->getEntryBlock().front());
  if (!RestoredCall || D->getNumUses() != 2) return Fail("restored-callee-use-shape");
  const Use *First = nullptr, *Last = nullptr;
  for (const Use &U : D->uses()) { if (!First) First = &U; Last = &U; }
  if (First != &RestoredCall->getOperandUse(RestoredCall->getNumOperands() - 1) ||
      Last != ExternalCalleeUse || ExternalCalleeUse->get() != D) return Fail("callee-external-use-prefix-and-suffix");
  return 1;
}
}
