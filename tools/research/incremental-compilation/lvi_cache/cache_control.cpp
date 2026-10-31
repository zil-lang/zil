// Research control: cache_impl.inc is extracted verbatim from the pinned
// original/indexed implementation, except exposing fields for relation checks.
// This tests real LLVM values/callbacks and cache methods, not a cache model.
#ifdef NDEBUG
#error "The cache control requires assertions"
#endif
#include "cache_impl.inc"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Verifier.h"
#include "llvm-c/Core.h"
#include <cstdlib>
#include <dlfcn.h>
#include <iostream>
#include <set>

static unsigned Checkpoints;

static void audit(LazyValueInfoCache &Cache, const char *Label) {
  std::set<std::pair<Value *, unsigned>> Pairs;
  auto Seen = [&](Value *V, unsigned Number) {
    assert(Cache.ValueHandles.find_as(V) != Cache.ValueHandles.end());
    Pairs.insert({V, Number});
#if LVI_INDEXED
    auto It = Cache.ValueBlocks.find(V);
    assert(It != Cache.ValueBlocks.end() && It->second.count(Number));
#endif
  };
  for (unsigned Number = 0; Number < Cache.BlockCache.size(); ++Number) {
    auto &Entry = Cache.BlockCache[Number];
    if (!Entry)
      continue;
    for (auto &KV : Entry->LatticeElements)
      Seen(KV.first, Number);
    for (Value *V : Entry->OverDefined)
      Seen(V, Number);
    if (Entry->NonNullPointers)
      for (Value *V : *Entry->NonNullPointers)
        Seen(V, Number);
    if (Entry->PredecessorLatticeElements)
      for (auto &KV : *Entry->PredecessorLatticeElements)
        Seen(KV.first, Number);
  }
#if LVI_INDEXED
  for (auto &KV : Cache.ValueBlocks) {
    assert(Cache.ValueHandles.find_as(KV.first) != Cache.ValueHandles.end());
    for (unsigned Number : KV.second)
      assert(Number < Cache.BlockCache.size());
  }
  for (const auto &Handle : Cache.ValueHandles)
    assert(Cache.ValueBlocks.count(Handle));
#endif
  ++Checkpoints;
  std::cout << Label << " pairs=" << Pairs.size()
            << " handles=" << Cache.ValueHandles.size() << '\n';
}

int main(int argc, char **argv) {
  assert(argc == 4);
  Dl_info LibraryInfo;
  assert(dladdr(reinterpret_cast<void *>(LLVMGetVersion), &LibraryInfo));
  char *LibraryPath = realpath(LibraryInfo.dli_fname, nullptr);
  assert(LibraryPath && std::string(LibraryPath) == argv[3]);
  free(LibraryPath);
  unsigned Major, Minor, Patch;
  LLVMGetVersion(&Major, &Minor, &Patch);
  assert(Major == 23 && Minor == 1 && Patch == 4);
  PerPredRanges = std::string(argv[1]) == "true";
  bool RejectEpoch = std::string(argv[2]) == "reject-epoch";
  LLVMContext Context;
  Module M("lvi-cache-control", Context);
  Type *I32 = Type::getInt32Ty(Context);
  Function *F = Function::Create(FunctionType::get(I32, {I32}, false),
                                 GlobalValue::ExternalLinkage, "f", M);
  BasicBlock *Entry = BasicBlock::Create(Context, "entry", F);
  BasicBlock *A = BasicBlock::Create(Context, "a", F);
  BasicBlock *B = BasicBlock::Create(Context, "b", F);
  BasicBlock *Exit = BasicBlock::Create(Context, "exit", F);
  IRBuilder<> Builder(Entry);
  auto *V1 = cast<Instruction>(Builder.CreateAdd(F->getArg(0), Builder.getInt32(1), "v1"));
  auto *V2 = cast<Instruction>(Builder.CreateAdd(F->getArg(0), Builder.getInt32(2), "v2"));
  auto *V3 = cast<Instruction>(Builder.CreateAdd(F->getArg(0), Builder.getInt32(3), "v3"));
  auto *P1 = Builder.CreateAlloca(I32, nullptr, "p1");
  auto *P2 = Builder.CreateAlloca(I32, nullptr, "p2");
  Builder.CreateCondBr(Builder.CreateICmpEQ(F->getArg(0), Builder.getInt32(0)), A, B);
  BranchInst::Create(Exit, A);
  BranchInst::Create(Exit, B);
  ReturnInst::Create(Context, F->getArg(0), Exit);
  ValueLatticeElement Constant = ValueLatticeElement::get(Builder.getInt32(42));
  ValueLatticeElement Overdefined = ValueLatticeElement::getOverdefined();
  {
    LazyValueInfoCache Cache(F);
    Cache.insertResult(V1, A, Constant);
    if (RejectEpoch) {
      F->renumberBlocks();
      Cache.insertResult(V1, A, Constant);
      std::abort(); // The preceding original epoch assertion must reject first.
    }
    Cache.insertResult(V1, A, Constant);
    Cache.insertResult(V1, B, Overdefined);
    Cache.insertResult(V2, A, Constant);
    BBLatticeElementMap Preds;
    Preds.insert({Entry, Constant});
    if (PerPredRanges) {
      Cache.insertPredecessorResults(V1, A, Preds);
      Cache.insertPredecessorResults(V3, B, Preds);
      assert(Cache.getCachedPredecessorInfo(V3, B).has_value());
    }
    audit(Cache, "three-value-insertions");
    unsigned InitCalls = 0;
    auto InitP1 = [&](BasicBlock *) {
      ++InitCalls;
      NonNullPointerSet Set;
      Set.insert(P1);
      return Set;
    };
    assert(Cache.isNonNullAtEndOfBlock(P1, Exit, InitP1));
    assert(Cache.isNonNullAtEndOfBlock(P1, Exit, InitP1));
    assert(InitCalls == 1);
    audit(Cache, "nonnull-only");
    P1->replaceAllUsesWith(P2);
    assert(!Cache.isNonNullAtEndOfBlock(P1, Exit, InitP1));
    assert(!Cache.isNonNullAtEndOfBlock(P2, Exit, InitP1));
    assert(InitCalls == 1);
    audit(Cache, "nonnull-rauw-does-not-transfer");
    P1->eraseFromParent();
    V1->replaceAllUsesWith(V2);
    assert(!Cache.getCachedValueInfo(V1, A));
    assert(!Cache.getCachedValueInfo(V1, B));
    if (PerPredRanges)
      assert(!Cache.getCachedPredecessorInfo(V1, A));
    assert(Cache.getCachedValueInfo(V2, A));
    Cache.eraseValue(V1); // An already-forgotten value is a no-op.
    Cache.eraseValue(F->getArg(0)); // A never-cached value is a no-op.
    audit(Cache, "value-rauw-and-forget-uncached");
    V1->eraseFromParent();
    Cache.clear();
    audit(Cache, "clear-first");
    Cache.insertResult(V3, A, Constant);
    if (PerPredRanges)
      Cache.insertPredecessorResults(V3, B, Preds);
    V3->eraseFromParent();
    assert(Cache.ValueHandles.empty());
    audit(Cache, "delete-cached-value");
    Cache.clear();
    Cache.insertResult(V2, A, Overdefined);
    Cache.insertResult(V2, B, Overdefined);
    Cache.insertResult(V2, Exit, Overdefined);
    Cache.threadEdgeImpl(A, B);
    assert(!Cache.getCachedValueInfo(V2, A));
    assert(!Cache.getCachedValueInfo(V2, Exit));
    assert(Cache.getCachedValueInfo(V2, B)->isOverdefined());
    audit(Cache, "thread-edge-invalidates-successors");
    for (unsigned N = 0; N < 1000; ++N)
      Cache.insertResult(V2, A, Overdefined);
#if LVI_INDEXED
    assert(Cache.ValueBlocks.find(V2)->second.size() == 3);
#endif
    audit(Cache, "repeated-registration-is-idempotent");
    if (PerPredRanges) {
      Cache.insertPredecessorResults(V2, A, Preds);
      assert(Cache.getCachedPredecessorInfo(V2, A));
    }
    Cache.eraseBlock(B);
    assert(!Cache.getCachedValueInfo(V2, B));
    if (PerPredRanges)
      assert(!Cache.getCachedPredecessorInfo(V2, A));
    Cache.insertResult(V2, B, Constant);
    audit(Cache, "erase-block-repopulate");
    Entry->getTerminator()->eraseFromParent();
    BranchInst::Create(A, Entry);
    Cache.eraseBlock(B);
    B->eraseFromParent();
    BasicBlock *New = BasicBlock::Create(Context, "new", F);
    BranchInst::Create(Exit, New);
    Cache.insertResult(V2, New, Constant);
    audit(Cache, "delete-block-add-new-block");
    Cache.clear();
    audit(Cache, "clear-after-stale-memberships");
    Cache.insertResult(V2, A, Constant);
    auto InitP2 = [&](BasicBlock *) {
      NonNullPointerSet Set;
      Set.insert(P2);
      return Set;
    };
    assert(Cache.isNonNullAtEndOfBlock(P2, Exit, InitP2));
    P2->eraseFromParent();
    assert(Cache.ValueHandles.size() == 1);
    audit(Cache, "nonnull-delete-and-live-cache-destruction");
    // Cache destruction intentionally occurs with one remaining live value.
  }
  F->renumberBlocks();
  {
    LazyValueInfoCache Fresh(F);
    Fresh.insertResult(V2, A, Constant);
    audit(Fresh, "fresh-cache-new-epoch");
    Fresh.clear();
  }
  assert(!verifyModule(M, &errs()));
  std::cout << "Pass perpred=" << PerPredRanges << " checkpoints=" << Checkpoints << '\n';
}
