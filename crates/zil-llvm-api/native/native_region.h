// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_NATIVE_REGION_H
#define ZIL_NATIVE_REGION_H
#include <cstdint>
#include <memory>
namespace llvm { class Module; class TargetMachine; }
namespace zil {
class NativeRegion;
// The object is request-local until the exact successful owner handoff. It is
// never serialized and does not grant any function input/world/pipeline proof.
class NativeRegionSession {
  struct State;
  std::unique_ptr<State> S;
  explicit NativeRegionSession(std::unique_ptr<State>);
public:
  using DisposeOwner = void (*)(void *);
  static std::unique_ptr<NativeRegionSession> create(bool ExplicitlyEnabled,
      llvm::Module &, llvm::TargetMachine &);
  ~NativeRegionSession();
  NativeRegionSession(const NativeRegionSession &) = delete;
  // Same full original emitter; successful output is still subject to the
  // Rust current-LLVM/object/cancellation checks before adoption/publication.
  int emit(void **Buffer, char **Message);
  // On false, Rust still owns ALL three handles. On true, null exactly those
  // guards; the opaque region owns their original addresses/destructors.
  std::shared_ptr<const NativeRegion> adopt(void *Context, void *Module, void *TM,
      DisposeOwner DisposeContext, DisposeOwner DisposeModule, DisposeOwner DisposeTM,
      uint64_t OwnerLimit);
};
class NativeRegion {
  friend class NativeRegionSession;
  struct State;
  std::unique_ptr<State> S;
  explicit NativeRegion(std::unique_ptr<State>);
public:
  ~NativeRegion();
  NativeRegion(const NativeRegion &) = delete;
  uint64_t ownedBytes() const;
};
}
#endif
