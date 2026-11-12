// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_PACKED_STAGE_H
#define ZIL_PACKED_STAGE_H
#include <array>
#include <cstddef>
#include <cstdint>
#include <map>
#include <optional>
#include <string>

namespace zil {
// Complete textual payload only. Analysis IDs, PA and replay/name/BB state stay
// in their original typed metadata; no execution address enters this encoding.
struct StageStrings {
  std::string Input, World, Pipeline, Output, OutputText, EntryFacts, ExitFacts;
  std::map<std::string, std::string> OriginalDeclarations;
};

// Immutable resident transport, never a Verify or analysis-result authority.
// The original decoded admission cap is checked separately by the caller.
// Only a selected candidate is decoded, under an explicit complete frame cap.
class PackedStageStrings {
  enum class Codec : uint8_t { Raw = 0, Zstd = 1 };
  Codec Encoding = Codec::Raw;
  uint64_t DecodedBytes = 0, EncodedBytes = 0;
  std::array<uint8_t, 32> Digest{};
  std::string Bytes;
  friend int packedStageFrameCounterexample();
  friend int packedStageCodecCounterexample();
public:
  static std::optional<PackedStageStrings> pack(const StageStrings &, size_t MaxBytes,
                                               bool TryCompression = true);
  bool unpack(StageStrings &, size_t MaxBytes) const;
  size_t encodedSize() const { return Bytes.size(); }
  uint64_t decodedSize() const { return DecodedBytes; }
  bool compressed() const { return Encoding == Codec::Zstd; }
  static bool compressionAvailable();
};
}
#endif
