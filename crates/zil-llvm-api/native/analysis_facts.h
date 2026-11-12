// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_ANALYSIS_FACTS_H
#define ZIL_ANALYSIS_FACTS_H
#include "llvm/IR/PassManager.h"
#include <string>
#include <vector>
namespace zil {
struct AnalysisReplayState {
  int emptyAssumptions = -1;
  int noOptionTracking = -1;
  int dominatorDFSValid = -1;
  int postDominatorDFSValid = -1;
};
// A complete projection of supported CURRENT result states. Unknown results
// refuse reuse. Names and static analysis IDs alone are never result proofs.
bool analysisFacts(llvm::Function &, llvm::FunctionAnalysisManager &,
                   const std::vector<const void *> &, std::string &, AnalysisReplayState *ReplayState = nullptr);
bool restoreAnalysisFacts(llvm::Function &, llvm::FunctionAnalysisManager &, const AnalysisReplayState &);
}
#endif
