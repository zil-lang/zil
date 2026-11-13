// Current-build, weak-only index of completed owner-free native products.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_COHORT_PRODUCTS_H
#define ZIL_COHORT_PRODUCTS_H
#include "work_products.h"
#include <array>
#include <memory>
#include <mutex>
#include <string>
#include <vector>
namespace llvm { class Function; class TargetMachine; }
namespace zil {
class CohortArena {
  struct Entry { uint64_t hash; unsigned kind; std::weak_ptr<const void> value; };
  mutable std::mutex mutex;
  std::vector<Entry> entries;
  ZilNativeCohortStatsV1 stats{};
  bool disabled = false;
public:
  static constexpr size_t MaxEntries = 4096;
  std::vector<std::shared_ptr<const void>> candidates(unsigned, uint64_t, ZilNativeCohortStatsV1 *);
  void publish(unsigned, uint64_t, const std::shared_ptr<const void> &, ZilNativeCohortStatsV1 *);
  void replayed(unsigned, ZilNativeCohortStatsV1 *);
  void decline(ZilNativeCohortStatsV1 *);
  void disable(ZilNativeCohortStatsV1 *);
  ZilNativeCohortStatsV1 read() const;
};
// These projections are a separate closed external-function domain. They do
// not weaken the original function/world product keys. Empty means decline.
bool cohortMachineHeaderCounterexamples();
bool cohortTarget(const llvm::TargetMachine &);
std::string cohortContext(const llvm::Function &);
std::string cohortFunctionText(const llvm::Function &);
// Substitute exactly the leading definition's own unquoted compiler symbol.
// The canonical subject also alpha-renumbers lexical attribute #N references;
// complete attribute VALUE footers and the binary owned-use suffix stay exact.
// No global replace, IR mutation, Value pointer or restored body is authority.
std::string cohortRebindFunctionText(const std::string &, const std::string &,
                                    const std::string &);
uint64_t cohortHash(const std::string &, const std::string &, const std::string &,
                    const std::string &);
}
#endif
