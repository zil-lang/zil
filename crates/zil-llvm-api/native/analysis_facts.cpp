// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "analysis_facts.h"
#include "llvm/Analysis/AssumptionCache.h"
#include "llvm/Analysis/CGSCCPassManager.h"
#include "llvm/Analysis/LoopInfo.h"
#include "llvm/Analysis/PostDominators.h"
#include "llvm/Analysis/TargetLibraryInfo.h"
#include "llvm/Analysis/TargetTransformInfo.h"
#include "llvm/IR/Dominators.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/PassInstrumentation.h"
#include "llvm/Support/raw_ostream.h"
namespace zil {
bool analysisFacts(llvm::Function &F, llvm::FunctionAnalysisManager &AM,
                   const std::vector<const void *> &Keys, std::string &Out) {
  using namespace llvm;
  Out.clear(); raw_string_ostream OS(Out);
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
      auto *AC = AM.getCachedResult<AssumptionAnalysis>(F);
      if (!AC || !AC->assumptions().empty()) return false;
      // Admit only the truly empty state, including no stale/null handles.
      for (Argument &A : F.args()) if (!AC->assumptionsFor(&A).empty()) return false;
      for (BasicBlock &B : F) {
        if (!AC->assumptionsFor(&B).empty()) return false;
        for (Instruction &I : B) if (!AC->assumptionsFor(&I).empty()) return false;
      }
      OS << "assumptions:empty\n";
    } else if (K == LoopAnalysis::ID()) {
      auto *LI = AM.getCachedResult<LoopAnalysis>(F);
      if (!LI || !LI->empty()) return false;
      // eligibleFunction independently proves an acyclic current CFG. There
      // are no Loop objects, per-loop results or loop owner mappings here.
      OS << "loop-info:empty\n";
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
}
