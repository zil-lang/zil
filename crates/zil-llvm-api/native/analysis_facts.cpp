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
#include "llvm/IR/PassInstrumentation.h"
#include "llvm/Support/raw_ostream.h"
#include <dlfcn.h>
namespace zil {
bool analysisFacts(llvm::Function &F, llvm::FunctionAnalysisManager &AM,
                   const std::vector<const void *> &Keys, std::string &Out, AnalysisReplayState *ReplayState) {
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
      OS << "target-library-overrides:";
      for (uint64_t W : Words) OS << W << ':';
      OS << '\n';
      OS << "target-library:" << TLI->getIntSize() << ':'
         << TLI->getExtAttrForI32Param(false) << ':' << TLI->getExtAttrForI32Param(true)
         << ':' << TLI->getExtAttrForI32Return(false) << ':' << TLI->getExtAttrForI32Return(true) << '\n';
      // Bind per-function overrides as well as target implementation names.
      // Vector-library calls are outside the admitted intrinsic-only domain.
      for (unsigned I = 0; I != static_cast<unsigned>(NumLibFuncs); ++I) {
        auto LF = static_cast<LibFunc>(I); StringRef Name = TLI->getName(LF);
        OS << TLI->has(LF) << ':' << Name.size() << ':' << Name << ';';
      }
      OS << '\n';
    } else if (K == DominatorTreeAnalysis::ID()) {
      auto *DT = AM.getCachedResult<DominatorTreeAnalysis>(F);
      if (!DT) return false;
      // The pinned printer includes ordered children, immediate dominators,
      // root order, DFS validity/numbers and invalid-state slow-query count.
      OS << "dominator-tree:\n"; DT->print(OS);
    } else if (K == PostDominatorTreeAnalysis::ID()) {
      auto *DT = AM.getCachedResult<PostDominatorTreeAnalysis>(F);
      if (!DT) return false;
      OS << "post-dominator-tree:\n"; DT->print(OS);
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
  return true;
}
}
