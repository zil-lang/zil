// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_ANALYSIS_FACTS_H
#define ZIL_ANALYSIS_FACTS_H
#include "llvm/IR/PassManager.h"
#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>
namespace zil {
struct AnalysisReplayState {
  int emptyAssumptions = -1;
  int noOptionTracking = -1;
  int dominatorDFSValid = -1;
  int postDominatorDFSValid = -1;
};
// Scratch projection memo for the CURRENT Request/FAM only. The native
// adapter creates Request immediately before its single LLVMRunPasses call;
// phase 2 is emission and must clear this memo. A future second LLVMRunPasses
// must explicitly clear it, even if a new FAM reuses the same address. Never
// move this state into resident Products or use a result object's address as
// an identity. Only the SDK's checked immutable factory basis is a token.
class AnalysisFactsMemo {
  struct TargetLibraryEntry {
    const void *Basis;
    std::vector<uint64_t> OverrideWords;
    std::array<uint64_t, 5> ScalarQueries;
    std::string Facts;
  };
  const void *Manager = nullptr;
  std::vector<TargetLibraryEntry> TargetLibraries;
  size_t RetainedBytes = 0;
  uint64_t TargetLibraryProjections = 0;
  uint64_t TargetLibraryReuses = 0;
  friend bool analysisFacts(llvm::Function &, llvm::FunctionAnalysisManager &,
      const std::vector<const void *> &, std::string &, AnalysisReplayState *, AnalysisFactsMemo *);
public:
  AnalysisFactsMemo() = default;
  AnalysisFactsMemo(const AnalysisFactsMemo &) = delete;
  AnalysisFactsMemo &operator=(const AnalysisFactsMemo &) = delete;
  // Counters describe this Request, including full projections on fallback.
  // Clearing lifetime-scoped tokens does not reset those actual work counts.
  void clear() { Manager = nullptr; TargetLibraries.clear(); RetainedBytes = 0; }
  uint64_t targetLibraryProjections() const { return TargetLibraryProjections; }
  uint64_t targetLibraryReuses() const { return TargetLibraryReuses; }
  size_t retainedBytes() const { return RetainedBytes; }
};
// Pure identity whitelist only: it never queries/builds an analysis result or
// inspects IR. The facts checker and optional product scheduler share it.
bool supportedAnalysisKeys(const std::vector<const void *> &);
// A complete projection of supported CURRENT result states. Unknown results
// refuse reuse. Names and static analysis IDs alone are never result proofs.
bool analysisFacts(llvm::Function &, llvm::FunctionAnalysisManager &,
                   const std::vector<const void *> &, std::string &, AnalysisReplayState *ReplayState = nullptr, AnalysisFactsMemo *Memo = nullptr);
bool restoreAnalysisFacts(llvm::Function &, llvm::FunctionAnalysisManager &, const AnalysisReplayState &);
}
#endif
