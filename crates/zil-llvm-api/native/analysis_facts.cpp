// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "analysis_facts.h"
#include "llvm/Analysis/AssumptionCache.h"
#include "llvm/Analysis/BasicAliasAnalysis.h"
#include "llvm/Analysis/LastRunTrackingAnalysis.h"
#include "llvm/Analysis/OptimizationRemarkEmitter.h"
#include "llvm/Analysis/ScopedNoAliasAA.h"
#include "llvm/Analysis/TypeBasedAliasAnalysis.h"
#include "llvm/Analysis/CGSCCPassManager.h"
#include "llvm/Analysis/LoopInfo.h"
#include "llvm/Analysis/PostDominators.h"
#include "llvm/Analysis/TargetLibraryInfo.h"
#include "llvm/Analysis/TargetTransformInfo.h"
#include "llvm/IR/Dominators.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/PassInstrumentation.h"
#include "llvm/Support/raw_ostream.h"
#include <dlfcn.h>
namespace zil {
bool analysisFacts(llvm::Function &F, llvm::FunctionAnalysisManager &AM,
                   const std::vector<const void *> &Keys, std::string &Out, AnalysisReplayState *ReplayState, AnalysisFactsMemo *Memo) {
  using namespace llvm;
  Out.clear();
  if (ReplayState) *ReplayState = {};
  // Refuse unknown result domains before serializing supported prefixes.
  for (const void *K : Keys)
    if (K != PassInstrumentationAnalysis::ID() && K != TargetIRAnalysis::ID() &&
        K != TargetLibraryAnalysis::ID() && K != DominatorTreeAnalysis::ID() &&
        K != PostDominatorTreeAnalysis::ID() && K != AssumptionAnalysis::ID() &&
        K != LoopAnalysis::ID() && K != ShouldNotRunFunctionPassesAnalysis::ID() &&
        K != BasicAA::ID() && K != ScopedNoAliasAA::ID() && K != TypeBasedAA::ID() &&
        K != LastRunTrackingAnalysis::ID() && K != OptimizationRemarkEmitterAnalysis::ID() &&
        K != ModuleAnalysisManagerFunctionProxy::ID() && K != AAManager::ID()) return false;
  raw_string_ostream OS(Out);
  // Preserve result-list order: dependencies and lazy availability can observe
  // it. The enclosing product separately binds exact IR, BB numbering, globals,
  // module metadata, target machine and every adapter-controlled pipeline flag.
  for (const void *K : Keys) {
    if (K == PassInstrumentationAnalysis::ID()) {
      // The native C API constructs only its fixed default callbacks, with
      // debug logging and verify-each disabled; no user callback registration.
      OS << "instrumentation:fixed-native-default\n";
    } else if (K == TargetIRAnalysis::ID()) {
      if (!AM.getCachedResult<TargetIRAnalysis>(F)) return false;
      // TTI is an immutable query interface constructed from the CURRENT
      // generic x86-64 target/config and current function attributes, all of
      // which are in the enclosing exact input. No TTI result is transported.
      OS << "target-ir:current-fixed-target-function\n";
    } else if (K == TargetLibraryAnalysis::ID()) {
      auto *TLI = AM.getCachedResult<TargetLibraryAnalysis>(F);
      if (!TLI) return false;
      using Overrides = uint64_t (*)(void *, void *, uint64_t *, uint64_t);
      static auto GetOverrides = reinterpret_cast<Overrides>(dlsym(RTLD_DEFAULT, "ZilLLVMTargetLibraryOverrideWords"));
      if (!GetOverrides) return false;
      uint64_t Count = GetOverrides(&AM, &F, nullptr, 0);
      if (!Count || Count > 4096) return false;
      std::vector<uint64_t> Words(Count);
      if (GetOverrides(&AM, &F, Words.data(), Words.size()) != Count) return false;
      // At pinned 7a941dfb, a TLI result has exactly const Impl* and the
      // mutable OverrideAsUnavailable bitset. The default registered TLA owns
      // an optional BaselineInfoImpl initialized once by run(); later runs do
      // not mutate it. A narrow SDK getter checks the actual registered typed
      // factory and exact cached Result.Impl association, or declines. A TLI
      // result pointer is NOT a basis: AM.clear destroys/reuses its storage.
      using BasisQuery = const void *(*)(void *, void *);
      static auto GetBasis = reinterpret_cast<BasisQuery>(
          dlsym(RTLD_DEFAULT, "ZilLLVMTargetLibraryImmutableBasis"));
      const void *Basis = Memo && GetBasis ? GetBasis(&AM, &F) : nullptr;
      // Query the complete current override bitset and these scalar fields on
      // EVERY call. Function no-builtins/no-builtin-* attrs affect the result
      // through that bitset; direct setUnavailable/disableAllFunctions do too.
      // The enclosing unchanged function/world projections remain independent.
      const std::array<uint64_t, 5> Scalars = {
          TLI->getIntSize(), TLI->getExtAttrForI32Param(false),
          TLI->getExtAttrForI32Param(true), TLI->getExtAttrForI32Return(false),
          TLI->getExtAttrForI32Return(true)};
      if (Memo && Basis) {
        if (Memo->Manager != &AM) { Memo->clear(); Memo->Manager = &AM; }
        const std::string *Cached = nullptr;
        for (const auto &Entry : Memo->TargetLibraries)
          if (Entry.Basis == Basis && Entry.OverrideWords == Words &&
              Entry.ScalarQueries == Scalars) { Cached = &Entry.Facts; break; }
        if (Cached) {
          ++Memo->TargetLibraryReuses;
          OS << *Cached;
          continue;
        }
      }
      auto Project = [&](raw_ostream &Stream) {
        Stream << "target-library-overrides:";
        for (uint64_t W : Words) Stream << W << ':';
        Stream << '\n';
        Stream << "target-library:" << Scalars[0] << ':' << Scalars[1] << ':'
               << Scalars[2] << ':' << Scalars[3] << ':' << Scalars[4] << '\n';
        // Preserve the original complete query sequence and exact bytes.
        // Vector-library queries stay outside the existing intrinsic-only
        // domain; this memo adds no newly admitted analysis or result fields.
        for (unsigned I = 0; I != static_cast<unsigned>(NumLibFuncs); ++I) {
          auto LF = static_cast<LibFunc>(I); StringRef Name = TLI->getName(LF);
          Stream << TLI->has(LF) << ':' << Name.size() << ':' << Name << ';';
        }
        Stream << '\n';
      };
      if (Memo) ++Memo->TargetLibraryProjections;
      if (!Memo || !Basis) {
        // Missing SDK getter, an unknown factory/result association or an old
        // SDK always executes the original full current projection.
        Project(OS);
      } else {
        std::string Facts; raw_string_ostream FactsOS(Facts); Project(FactsOS);
        OS << Facts;
        // Optional Request scratch has explicit entry/payload bounds. At a cap
        // only memo admission stops; original facts/quality checks still run.
        constexpr size_t MaxEntries = 32, MaxBytes = 4 * 1024 * 1024;
        size_t Bytes = sizeof(AnalysisFactsMemo::TargetLibraryEntry) +
                       Words.size() * sizeof(uint64_t);
        if (Memo->TargetLibraries.size() < MaxEntries && Bytes <= MaxBytes &&
            Facts.size() <= MaxBytes - Bytes &&
            Memo->RetainedBytes <= MaxBytes - Bytes - Facts.size()) {
          Memo->RetainedBytes += Bytes + Facts.size();
          Memo->TargetLibraries.push_back({Basis, std::move(Words), Scalars, std::move(Facts)});
        }
      }
    } else if (K == DominatorTreeAnalysis::ID()) {
      auto *DT = AM.getCachedResult<DominatorTreeAnalysis>(F);
      if (!DT) return false;
      // The pinned printer includes ordered children, immediate dominators,
      // root order, DFS validity/numbers and invalid-state slow-query count.
      std::string Tree; raw_string_ostream TreeOS(Tree); DT->print(TreeOS);
      if (ReplayState) ReplayState->dominatorDFSValid = Tree.find("DFSNumbers invalid:") == std::string::npos ? 1 : 0;
      OS << "dominator-tree:\n" << Tree;
    } else if (K == PostDominatorTreeAnalysis::ID()) {
      auto *DT = AM.getCachedResult<PostDominatorTreeAnalysis>(F);
      if (!DT) return false;
      std::string Tree; raw_string_ostream TreeOS(Tree); DT->print(TreeOS);
      if (ReplayState) ReplayState->postDominatorDFSValid = Tree.find("DFSNumbers invalid:") == std::string::npos ? 1 : 0;
      OS << "post-dominator-tree:\n" << Tree;
    } else if (K == AssumptionAnalysis::ID()) {
      using Get = int (*)(void *, void *);
      static auto GetState = reinterpret_cast<Get>(dlsym(RTLD_DEFAULT, "ZilLLVMEmptyAssumptionState"));
      int State = GetState ? GetState(&AM, &F) : -1;
      if (State < 0) return false;
      if (ReplayState) ReplayState->emptyAssumptions = State;
      OS << "assumptions:empty:" << State << '\n';
    } else if (K == LoopAnalysis::ID()) {
      auto *LI = AM.getCachedResult<LoopAnalysis>(F);
      if (!LI || !LI->empty()) return false;
      // eligibleFunction independently proves an acyclic current CFG. There
      // are no Loop objects, per-loop results or loop owner mappings here.
      OS << "loop-info:empty\n";
    } else if (K == LastRunTrackingAnalysis::ID()) {
      using Get = int (*)(void *, void *);
      static auto GetState = reinterpret_cast<Get>(dlsym(RTLD_DEFAULT, "ZilLLVMNoOptionTrackingState"));
      int State = GetState ? GetState(&AM, &F) : -1;
      if (State < 0) return false;
      if (ReplayState) ReplayState->noOptionTracking = State;
      OS << "last-run:no-options-instcombine:" << State << '\n';
    } else if (K == ModuleAnalysisManagerFunctionProxy::ID()) {
      using Get = int (*)(void *, void *);
      static auto GetState = reinterpret_cast<Get>(dlsym(RTLD_DEFAULT, "ZilLLVMModuleProxyProductState"));
      int State = GetState ? GetState(&AM, &F) : -1;
      if (State < 0) return false;
      OS << "module-proxy:current-fam:empty-deferred:local-empty-profile:" << State << '\n';
    } else if (K == AAManager::ID()) {
      using Get = int (*)(void *, void *);
      static auto GetState = reinterpret_cast<Get>(dlsym(RTLD_DEFAULT, "ZilLLVMLocalAAAggregateProductState"));
      if (!GetState || !GetState(&AM, &F)) return false;
      OS << "aa:actual-default-factory:current-local-3:ordered-dependencies\n";
    } else if (K == BasicAA::ID()) {
      using Get = int (*)(void *, void *);
      static auto GetState = reinterpret_cast<Get>(dlsym(RTLD_DEFAULT, "ZilLLVMBasicAAProductState"));
      int State = GetState ? GetState(&AM, &F) : -1;
      if (State < 0) return false;
      OS << "basic-aa:current-dependencies:visited-empty:dt=" << State << '\n';
    } else if (K == ScopedNoAliasAA::ID()) {
      if (!AM.getCachedResult<ScopedNoAliasAA>(F)) return false;
      OS << "scoped-noalias:stateless\n";
    } else if (K == TypeBasedAA::ID()) {
      using Get = int (*)(void *, void *);
      static auto GetState = reinterpret_cast<Get>(dlsym(RTLD_DEFAULT, "ZilLLVMTypeBasedAAProductState"));
      int State = GetState ? GetState(&AM, &F) : -1;
      if (State < 0) return false;
      OS << "type-based-aa:sanitizer=" << State << '\n';
    } else if (K == OptimizationRemarkEmitterAnalysis::ID()) {
      using Get = int (*)(void *, void *);
      static auto GetState = reinterpret_cast<Get>(dlsym(RTLD_DEFAULT, "ZilLLVMOREProductState"));
      if (!GetState || !GetState(&AM, &F)) return false;
      OS << "ore:current-function:no-bfi:no-remarks:no-hotness\n";
    } else if (K == ShouldNotRunFunctionPassesAnalysis::ID()) {
      if (!AM.getCachedResult<ShouldNotRunFunctionPassesAnalysis>(F)) return false;
      OS << "cgscc-revisit-marker:empty-result\n";
    } else {
      // In particular, LoopAMProxy is not an empty marker: it owns global LAM
      // clearing and references current LoopInfo. AA/MSSA/SE and opaque proxy
      // result states require their own reviewed projections before admission.
      return false;
    }
  }
  return true;
}
bool restoreAnalysisFacts(llvm::Function &F, llvm::FunctionAnalysisManager &AM, const AnalysisReplayState &State) {
  using Restore = int (*)(void *, void *, unsigned);
  static auto SetAssumptions = reinterpret_cast<Restore>(dlsym(RTLD_DEFAULT, "ZilLLVMRestoreEmptyAssumptionState"));
  static auto SetTracking = reinterpret_cast<Restore>(dlsym(RTLD_DEFAULT, "ZilLLVMRestoreNoOptionTrackingState"));
  if (State.emptyAssumptions >= 0 && (!SetAssumptions || !SetAssumptions(&AM, &F, State.emptyAssumptions))) return false;
  if (State.noOptionTracking >= 0 && (!SetTracking || !SetTracking(&AM, &F, State.noOptionTracking))) return false;
  // Reconstruct valid DFS labels using the CURRENT verified tree. This does
  // not relax comparison of child order, idoms, numbers, levels or counters:
  // the complete captured facts must still match immediately afterwards.
  if (State.dominatorDFSValid == 1) {
    auto *DT = AM.getCachedResult<llvm::DominatorTreeAnalysis>(F);
    if (!DT) return false;
    DT->updateDFSNumbers();
  }
  if (State.postDominatorDFSValid == 1) {
    auto *DT = AM.getCachedResult<llvm::PostDominatorTreeAnalysis>(F);
    if (!DT) return false;
    DT->updateDFSNumbers();
  }
  return true;
}
}

// A semantic counterexample entry for the native test harness. It compares
// every memo result with the original full current projection; a missing SDK
// recipe getter is reported distinctly rather than a vacuous test success.
extern "C" int ZilNativeAnalysisFactsTLIMemoCounterexample() {
  using namespace llvm;
  using BasisQuery = const void *(*)(void *, void *);
  auto GetBasis = reinterpret_cast<BasisQuery>(
      dlsym(RTLD_DEFAULT, "ZilLLVMTargetLibraryImmutableBasis"));
  if (!GetBasis) return -1;
  LLVMContext Context;
  Module M("tli-current-request-memo", Context);
  M.setTargetTriple(Triple("x86_64-unknown-linux-gnu"));
  auto NewFunction = [&](StringRef Name) {
    auto *F = Function::Create(FunctionType::get(Type::getVoidTy(Context), false),
                              GlobalValue::ExternalLinkage, Name, &M);
    IRBuilder<> Builder(BasicBlock::Create(Context, "entry", F));
    Builder.CreateRetVoid();
    return F;
  };
  auto *F = NewFunction("f"); auto *G = NewFunction("g");
  FunctionAnalysisManager AM;
  AM.registerPass([] { return PassInstrumentationAnalysis(); });
  AM.registerPass([] { return TargetLibraryAnalysis(); });
  AM.getResult<TargetLibraryAnalysis>(*F);
  AM.getResult<TargetLibraryAnalysis>(*G);
  zil::AnalysisFactsMemo Memo;
  const std::vector<const void *> Keys{TargetLibraryAnalysis::ID()};
  auto Check = [&](Function &Fn, FunctionAnalysisManager &Manager,
                   zil::AnalysisFactsMemo &Scratch, std::string &Facts) {
    std::string Full;
    return zil::analysisFacts(Fn, Manager, Keys, Full) &&
           zil::analysisFacts(Fn, Manager, Keys, Facts, nullptr, &Scratch) &&
           Full == Facts;
  };
  std::string Original, Current;
  const void *Basis = GetBasis(&AM, F);
  if (!Basis || !Check(*F, AM, Memo, Original) ||
      !Check(*G, AM, Memo, Current) || Original != Current ||
      Memo.targetLibraryProjections() != 1 || Memo.targetLibraryReuses() != 1)
    return 0;
  // The result object stays put, but a pass can mutate its complete overrides.
  AM.getResult<TargetLibraryAnalysis>(*F).setUnavailable(LibFunc_malloc);
  if (!Check(*F, AM, Memo, Current) || Current == Original ||
      Memo.targetLibraryProjections() != 2) return 0;
  F->addFnAttr("no-builtins");
  AM.clear(*F, F->getName());
  AM.getResult<TargetLibraryAnalysis>(*F);
  if (GetBasis(&AM, F) != Basis || !Check(*F, AM, Memo, Current) ||
      Memo.targetLibraryProjections() != 3) return 0;
  // AM.clear may recycle result addresses. Only the surviving factory basis
  // plus the newly queried complete override key can license reuse.
  F->removeFnAttr("no-builtins"); F->addFnAttr("no-builtin-malloc");
  AM.clear(*F, F->getName()); AM.getResult<TargetLibraryAnalysis>(*F);
  if (!Check(*F, AM, Memo, Current) || Memo.targetLibraryProjections() != 3 ||
      Memo.targetLibraryReuses() != 2) return 0;
  F->removeFnAttr("no-builtin-malloc");
  AM.clear(*F, F->getName()); AM.getResult<TargetLibraryAnalysis>(*F);
  if (!Check(*F, AM, Memo, Current) || Current != Original ||
      Memo.targetLibraryReuses() != 3) return 0;
  // This is mandatory at a new LLVMRunPasses boundary, even when a new FAM
  // could occupy exactly the old address. No result/basis token crosses it.
  Memo.clear();
  if (Memo.retainedBytes() || !Check(*F, AM, Memo, Current) ||
      Memo.targetLibraryProjections() != 4) return 0;

  TargetLibraryInfoImpl Baseline(Triple("x86_64-unknown-linux-gnu"));
  Baseline.setAvailableWithName(LibFunc_malloc, "custom-malloc");
  FunctionAnalysisManager OtherAM;
  OtherAM.registerPass([] { return PassInstrumentationAnalysis(); });
  OtherAM.registerPass([&] { return TargetLibraryAnalysis(Baseline); });
  OtherAM.getResult<TargetLibraryAnalysis>(*F);
  if (GetBasis(&OtherAM, F) == Basis || !Check(*F, OtherAM, Memo, Current) ||
      Current == Original || Memo.targetLibraryProjections() != 5) return 0;
  // A const result pointer alone does not prove its externally supplied Impl
  // immutable. The SDK recipe must decline this non-factory association.
  TargetLibraryInfoImpl Foreign(Triple("x86_64-unknown-linux-gnu"));
  auto &Info = OtherAM.getResult<TargetLibraryAnalysis>(*F);
  Info = TargetLibraryInfo(Foreign, F);
  if (GetBasis(&OtherAM, F) || !Check(*F, OtherAM, Memo, Current) ||
      Memo.targetLibraryProjections() != 6) return 0;
  const std::string Before = Current;
  Foreign.setAvailableWithName(LibFunc_malloc, "changed-foreign-malloc");
  if (!Check(*F, OtherAM, Memo, Current) || Current == Before ||
      Memo.targetLibraryProjections() != 7) return 0;

  // Bound even a request with many successful dynamic override combinations.
  // At the cap every uncached combination still gets the original full facts.
  OtherAM.clear(*F, F->getName());
  auto &BoundedInfo = OtherAM.getResult<TargetLibraryAnalysis>(*F);
  zil::AnalysisFactsMemo Bounded;
  for (unsigned I = 0; I != 40; ++I) {
    BoundedInfo.setUnavailable(static_cast<LibFunc>(I));
    if (!Check(*F, OtherAM, Bounded, Current)) return 0;
  }
  const auto Projections = Bounded.targetLibraryProjections();
  if (!Bounded.retainedBytes() || Bounded.retainedBytes() > 4 * 1024 * 1024 ||
      !Check(*F, OtherAM, Bounded, Current) ||
      Bounded.targetLibraryProjections() != Projections + 1) return 0;
  return 1;
}
