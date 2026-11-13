// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_STAGE_BODY_H
#define ZIL_STAGE_BODY_H
#include <cstddef>
#include <cstdint>
#include <memory>
namespace llvm { class Function; }
namespace zil {
// Private immutable value transport. The allocation contains no LLVM owner,
// address, Attribute/Type handle, or allocator-owned IR object.
class StageBody {
  struct Storage;
  std::unique_ptr<Storage> S;
  explicit StageBody(std::unique_ptr<Storage>);
public:
  class Prepared;
  ~StageBody();
  StageBody(const StageBody &) = delete;
  static std::shared_ptr<const StageBody> capture(const llvm::Function &, size_t MaxBytes);
  size_t ownedBytes() const;
  // Read-only with respect to IR/Context: no type/constant factories, names,
  // symbols or Uses are materialized until restore. Current declaration
  // projections remain independently checked by the work-product consumer.
  std::unique_ptr<Prepared> prepare(llvm::Function &) const;
  // Caller clears the current FAM before replacing the body. Failure after
  // entry is dirty and must discard the complete native attempt.
  bool restore(llvm::Function &, Prepared &) const;
  static int counterexamples();
};
class StageBody::Prepared {
  friend class StageBody;
  struct State;
  std::unique_ptr<State> S;
  explicit Prepared(std::unique_ptr<State>);
public:
  ~Prepared();
  Prepared(const Prepared &) = delete;
};
}
#endif
