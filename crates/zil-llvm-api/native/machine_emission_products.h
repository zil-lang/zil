// Fixed LLVM 7a/23.1.4 per-block MI-to-MC products. These are subordinate to
// an already authorized, current-MachineVerifier-checked machine snapshot.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_MACHINE_EMISSION_PRODUCTS_H
#define ZIL_MACHINE_EMISSION_PRODUCTS_H
#include "llvm/ADT/IntrusiveRefCntPtr.h"
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <memory>
namespace llvm {
class MachineFunction; class MachineBasicBlock; class MachineInstr;
class MCInst; class MCStreamer; class MCSubtargetInfo;
}
namespace zil::emission {
struct Stats {
  uint64_t available = 0, captured_functions = 0, captured_blocks = 0;
  uint64_t replayed_functions = 0, replayed_blocks = 0, machine_instructions_skipped = 0;
  uint64_t mc_replay_events = 0, current_cfi_events = 0, declined_functions = 0;
  uint64_t budget_declines = 0, captured_capacity_bytes = 0;
  uint64_t capture_ns = 0, preflight_ns = 0, replay_ns = 0, failed_replays = 0;
};
struct Summary { uint32_t instructions = 0; bool realCode = false; };
// Exactly one allocation, including the intrusive control word and all side
// records. No MCInst/MCExpr/MCSymbol/IR/target/context/AP owner is retained.
class alignas(std::max_align_t) Blob {
  mutable std::atomic<uint32_t> references{0};
  size_t capacity_;
  explicit Blob(size_t Capacity) : capacity_(Capacity) {}
public:
  static llvm::IntrusiveRefCntPtr<Blob> allocate(size_t Capacity);
  void Retain() const;
  void Release() const;
  void operator delete(void *P) noexcept;
  size_t allocationBytes() const { return sizeof(Blob) + capacity_; }
  size_t capacity() const { return capacity_; }
  std::byte *data() { return reinterpret_cast<std::byte *>(this + 1); }
  const std::byte *data() const { return reinterpret_cast<const std::byte *>(this + 1); }
};
using Product = llvm::IntrusiveRefCntPtr<const Blob>;
// All callbacks borrow current native owners synchronously; no pointer enters
// Blob. Reserve receives COMPLETE additional allocation bytes, not just used
// event bytes, before allocation. The existing snapshot cap remains authority.
using Reserve = bool (*)(void *, size_t);
using EmitMC = bool (*)(void *, llvm::MCInst &);
using EmitCFI = bool (*)(void *, const llvm::MachineInstr &);
using PeekSymbol = const void *(*)(const void *);
class Capture {
  struct State;
  std::unique_ptr<State> state;
  explicit Capture(std::unique_ptr<State>);
public:
  ~Capture();
  Capture(const Capture &) = delete;
  static std::unique_ptr<Capture> begin(const llvm::MachineFunction &, Reserve,
                                        void *, PeekSymbol, Stats &);
  bool beginBlock(const llvm::MachineBasicBlock &);
  bool beginInstruction(const llvm::MachineInstr &);
  void recordInstruction(const llvm::MCInst &, const llvm::MCSubtargetInfo &);
  void recordCFI(const llvm::MachineInstr &);
  void endBlock(const llvm::MachineBasicBlock &, Summary);
  Product finish();
  bool active() const;
};
// Pure preflight creates only temporary CURRENT pointer lookup tables. It
// validates every range/role/MI/CFI position before any getSymbol/MC operation.
bool preflight(const Product &, const llvm::MachineFunction &, Stats &);
bool replay(const Product &, const llvm::MachineBasicBlock &, void *, EmitMC,
            EmitCFI, Summary &, Stats &);
size_t payloadBytes(const Product &);
bool domain(const llvm::MachineFunction &);
} // namespace zil::emission
#endif
