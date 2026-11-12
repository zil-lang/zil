// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#include "packed_stage.h"
#include "llvm/ADT/ArrayRef.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/Support/Compression.h"
#include "llvm/Support/Error.h"
#include "llvm/Support/SHA256.h"
#include <cstring>
#include <dlfcn.h>
#include <limits>
#include <utility>

namespace zil {
namespace {
constexpr char Magic[] = "ZILSTG01";
llvm::ArrayRef<uint8_t> bytes(const std::string &S) {
  return {reinterpret_cast<const uint8_t *>(S.data()), S.size()};
}
struct Compression {
  using Available = bool (*)();
  using Compress = void (*)(llvm::ArrayRef<uint8_t>, llvm::SmallVectorImpl<uint8_t> &, int, bool);
  using Decompress = llvm::Error (*)(llvm::ArrayRef<uint8_t>, uint8_t *, size_t &);
  Available available = nullptr;
  Compress compress = nullptr;
  Decompress decompress = nullptr;
  Compression() {
    // Exact public LLVM 23.1 C++ ABI from the pinned SDK. Optional dynamic
    // lookup adds no direct zstd library dependency or new SDK requirement.
    available = reinterpret_cast<Available>(dlsym(RTLD_DEFAULT,
        "_ZN4llvm11compression4zstd11isAvailableEv"));
    compress = reinterpret_cast<Compress>(dlsym(RTLD_DEFAULT,
        "_ZN4llvm11compression4zstd8compressENS_8ArrayRefIhEERNS_15SmallVectorImplIhEEib"));
    decompress = reinterpret_cast<Decompress>(dlsym(RTLD_DEFAULT,
        "_ZN4llvm11compression4zstd10decompressENS_8ArrayRefIhEEPhRm"));
  }
  bool usable() const { return available && compress && decompress && available(); }
};
const Compression &compression() { static const Compression C; return C; }
// The SDK decompressor reports output size but not consumed input. Admit only
// one ordinary no-dictionary zstd frame and walk its bounded block extents, so
// appended frames (including an empty frame) or unused trailing bytes miss.
// LLVM compress() here never supplies a dictionary. Any unfamiliar output can
// simply retain its original raw frame instead; this changes no pass policy.
bool singleZstdFrame(llvm::ArrayRef<uint8_t> Data, uint64_t ExpectedSize) {
  if (Data.size() < 6 || Data[0] != 0x28 || Data[1] != 0xb5 ||
      Data[2] != 0x2f || Data[3] != 0xfd) return false;
  uint8_t Flags = Data[4];
  if ((Flags & 0x18) || (Flags & 3)) return false;
  bool SingleSegment = Flags & 0x20, Checksum = Flags & 4;
  size_t Position = 5;
  if (!SingleSegment) {
    if (Position == Data.size()) return false;
    ++Position; // Window descriptor; the SDK still validates its semantics.
  }
  unsigned SizeFlag = Flags >> 6;
  size_t SizeBytes = SizeFlag == 0 ? (SingleSegment ? 1 : 0) : size_t(1) << SizeFlag;
  if (SizeBytes > Data.size() - Position) return false;
  uint64_t Size = 0;
  for (size_t I = 0; I != SizeBytes; ++I) Size |= uint64_t(Data[Position++]) << (8 * I);
  if (SizeBytes == 2) Size += 256;
  if (SizeBytes && Size != ExpectedSize) return false;
  for (;;) {
    if (Data.size() - Position < 3) return false;
    uint32_t Header = uint32_t(Data[Position]) | uint32_t(Data[Position + 1]) << 8 |
                      uint32_t(Data[Position + 2]) << 16;
    Position += 3;
    unsigned Kind = (Header >> 1) & 3;
    size_t BlockSize = Header >> 3;
    if (Kind == 3 || BlockSize > 128 * 1024) return false;
    size_t Stored = Kind == 1 ? 1 : BlockSize;
    if (Stored > Data.size() - Position) return false;
    Position += Stored;
    if (Header & 1) break;
  }
  if (Checksum) {
    if (Data.size() - Position < 4) return false;
    Position += 4;
  }
  return Position == Data.size();
}
struct Writer {
  std::string Data;
  size_t Maximum;
  bool append(const char *S, size_t N) {
    if (N > Maximum - Data.size()) return false;
    if (N) Data.append(S, N);
    return true;
  }
  bool number(uint64_t N) {
    char Encoded[8];
    for (unsigned I = 0; I != 8; ++I) Encoded[I] = static_cast<char>(N >> (8 * I));
    return append(Encoded, 8);
  }
  bool text(const std::string &S) { return number(S.size()) && append(S.data(), S.size()); }
};
struct Reader {
  const std::string &Data;
  size_t Position = 0;
  bool number(uint64_t &N) {
    if (Data.size() - Position < 8) return false;
    N = 0;
    for (unsigned I = 0; I != 8; ++I)
      N |= uint64_t(static_cast<uint8_t>(Data[Position++])) << (8 * I);
    return true;
  }
  bool text(std::string &S) {
    uint64_t N;
    if (!number(N) || N > std::numeric_limits<size_t>::max() ||
        N > Data.size() - Position) return false;
    S.assign(Data.data() + Position, static_cast<size_t>(N));
    Position += static_cast<size_t>(N); return true;
  }
};
bool decode(const std::string &Data, StageStrings &Out) {
  if (Data.size() < 8 || std::memcmp(Data.data(), Magic, 8)) return false;
  Reader R{Data, 8};
  StageStrings Current;
  for (auto *S : {&Current.Input, &Current.World, &Current.Pipeline, &Current.Output,
                  &Current.OutputText, &Current.EntryFacts, &Current.ExitFacts})
    if (!R.text(*S)) return false;
  uint64_t Count;
  if (!R.number(Count) || Count > (Data.size() - R.Position) / 16) return false;
  std::string Previous;
  for (uint64_t I = 0; I != Count; ++I) {
    std::string Key, Value;
    if (!R.text(Key) || !R.text(Value) || (I && !(Previous < Key))) return false;
    Previous = Key;
    if (!Current.OriginalDeclarations.emplace(std::move(Key), std::move(Value)).second) return false;
  }
  if (R.Position != Data.size()) return false;
  Out = std::move(Current); return true;
}
}

bool PackedStageStrings::compressionAvailable() { return compression().usable(); }
std::optional<PackedStageStrings> PackedStageStrings::pack(
    const StageStrings &Text, size_t MaxBytes, bool TryCompression) {
  Writer W{{}, MaxBytes};
  if (!W.append(Magic, 8)) return std::nullopt;
  for (const auto *S : {&Text.Input, &Text.World, &Text.Pipeline, &Text.Output,
                        &Text.OutputText, &Text.EntryFacts, &Text.ExitFacts})
    if (!W.text(*S)) return std::nullopt;
  if (!W.number(Text.OriginalDeclarations.size())) return std::nullopt;
  for (const auto &D : Text.OriginalDeclarations)
    if (!W.text(D.first) || !W.text(D.second)) return std::nullopt;
  PackedStageStrings Result;
  Result.DecodedBytes = W.Data.size();
  if (TryCompression && compression().usable()) {
    llvm::SmallVector<uint8_t, 0> Compressed;
    compression().compress(bytes(W.Data), Compressed,
                           llvm::compression::zstd::BestSpeedCompression, false);
    if (Compressed.size() < W.Data.size() && singleZstdFrame(Compressed, Result.DecodedBytes)) {
      Result.Encoding = Codec::Zstd;
      Result.Bytes.assign(reinterpret_cast<const char *>(Compressed.data()), Compressed.size());
    }
  }
  if (Result.Encoding == Codec::Raw) Result.Bytes = std::move(W.Data);
  Result.EncodedBytes = Result.Bytes.size();
  Result.Digest = llvm::SHA256::hash(bytes(Result.Bytes));
  return Result;
}
bool PackedStageStrings::unpack(StageStrings &Out, size_t MaxBytes) const {
  // These private immutable lengths/digest bind the entire retained transport,
  // including its end. They are integrity/accounting, not reuse authority.
  if (DecodedBytes > MaxBytes || DecodedBytes > std::numeric_limits<size_t>::max() ||
      EncodedBytes > MaxBytes || EncodedBytes != Bytes.size() || DecodedBytes < 72 ||
      llvm::SHA256::hash(bytes(Bytes)) != Digest) return false;
  if (Encoding == Codec::Raw) {
    return DecodedBytes == Bytes.size() && decode(Bytes, Out);
  }
  if (Encoding != Codec::Zstd || !compression().usable() ||
      !singleZstdFrame(bytes(Bytes), DecodedBytes)) return false;
  std::string Decoded(static_cast<size_t>(DecodedBytes), '\0');
  size_t Actual = Decoded.size();
  if (auto Error = compression().decompress(bytes(Bytes),
      reinterpret_cast<uint8_t *>(Decoded.data()), Actual)) {
    llvm::consumeError(std::move(Error)); return false;
  }
  return Actual == DecodedBytes && decode(Decoded, Out);
}
}

namespace zil {
namespace {
bool equal(const StageStrings &A, const StageStrings &B) {
  return A.Input == B.Input && A.World == B.World && A.Pipeline == B.Pipeline &&
         A.Output == B.Output && A.OutputText == B.OutputText &&
         A.EntryFacts == B.EntryFacts && A.ExitFacts == B.ExitFacts &&
         A.OriginalDeclarations == B.OriginalDeclarations;
}
}
int packedStageFrameCounterexample() {
  StageStrings Original;
  Original.Input = std::string("input\0body", 10); Original.World = "world";
  Original.Pipeline = "actual-default-pipeline"; Original.Output = std::string("BC\0\xff", 4);
  Original.OutputText = "output-current-text"; Original.EntryFacts = "entry-facts";
  Original.ExitFacts = "exit-facts";
  Original.OriginalDeclarations.emplace("a", std::string("decl\0value", 10));
  Original.OriginalDeclarations.emplace("b", "second-value");
  auto Packed = PackedStageStrings::pack(Original, 4096, false);
  StageStrings Out;
  if (!Packed || Packed->compressed() || !Packed->unpack(Out, 4096) || !equal(Original, Out)) return 0;
  const size_t N = Packed->decodedSize();
  if (!PackedStageStrings::pack(Original, N, false) ||
      PackedStageStrings::pack(Original, N - 1, false) || Packed->unpack(Out, N - 1)) return 0;
  auto RepairDigest = [](PackedStageStrings &P) {
    P.EncodedBytes = P.Bytes.size(); P.Digest = llvm::SHA256::hash(bytes(P.Bytes));
  };
  auto Broken = *Packed;
  Broken.Encoding = static_cast<PackedStageStrings::Codec>(99);
  if (Broken.unpack(Out, 4096)) return 0;
  Broken = *Packed; Broken.DecodedBytes = std::numeric_limits<uint64_t>::max();
  if (Broken.unpack(Out, 4096)) return 0;
  Broken = *Packed; ++Broken.EncodedBytes;
  if (Broken.unpack(Out, 4096)) return 0;
  Broken = *Packed; Broken.Bytes.back() ^= 1;
  if (Broken.unpack(Out, 4096)) return 0;
  Broken = *Packed; Broken.Bytes.pop_back(); --Broken.DecodedBytes; RepairDigest(Broken);
  if (Broken.unpack(Out, 4096)) return 0;
  Broken = *Packed; Broken.Bytes.push_back('x'); ++Broken.DecodedBytes; RepairDigest(Broken);
  if (Broken.unpack(Out, 4096)) return 0;
  Broken = *Packed;
  for (unsigned I = 0; I != 8; ++I) Broken.Bytes[8 + I] = static_cast<char>(0xff);
  RepairDigest(Broken);
  if (Broken.unpack(Out, 4096)) return 0;
  // Duplicate and out-of-order maps are malformed even when their length and
  // transport checksum are correct. Never silently overwrite or sort a frame.
  StageStrings Empty;
  auto Base = PackedStageStrings::pack(Empty, 4096, false);
  if (!Base) return 0;
  for (const auto &Keys : {std::pair<std::string, std::string>{"a", "a"},
                           std::pair<std::string, std::string>{"b", "a"}}) {
    Writer W{{}, 4096};
    if (!W.append(Magic, 8)) return 0;
    for (unsigned I = 0; I != 7; ++I) if (!W.text("")) return 0;
    if (!W.number(2) || !W.text(Keys.first) || !W.text("one") ||
        !W.text(Keys.second) || !W.text("two")) return 0;
    Broken = *Base; Broken.Bytes = std::move(W.Data);
    Broken.DecodedBytes = Broken.Bytes.size(); RepairDigest(Broken);
    if (Broken.unpack(Out, 4096)) return 0;
  }
  // Empty output/OutputText/declarations are legitimate BodyUnchanged payloads.
  if (!Base->unpack(Out, 4096) || !equal(Out, Empty)) return 0;
  return 1;
}
int packedStageCodecCounterexample() {
  if (!PackedStageStrings::compressionAvailable()) return -1;
  StageStrings Original;
  Original.Input.assign(32 * 1024, 'i'); Original.World.assign(32 * 1024, 'w');
  Original.EntryFacts.assign(32 * 1024, 'f'); Original.ExitFacts = Original.EntryFacts;
  Original.Pipeline = "pipeline"; Original.Output = std::string("bitcode\0data", 12);
  Original.OutputText = "exact-current-text";
  Original.OriginalDeclarations.emplace("decl", "complete-declaration");
  auto Packed = PackedStageStrings::pack(Original, 256 * 1024);
  StageStrings Out;
  if (!Packed || !Packed->compressed() || Packed->encodedSize() >= Packed->decodedSize() ||
      !Packed->unpack(Out, 256 * 1024) || !equal(Out, Original)) return 0;
  auto Broken = *Packed;
  Broken.Bytes.pop_back(); Broken.EncodedBytes = Broken.Bytes.size();
  Broken.Digest = llvm::SHA256::hash(bytes(Broken.Bytes));
  if (Broken.unpack(Out, 256 * 1024)) return 0;
  Broken = *Packed; --Broken.DecodedBytes;
  if (Broken.unpack(Out, 256 * 1024)) return 0;
  Broken = *Packed; ++Broken.DecodedBytes;
  if (Broken.unpack(Out, 256 * 1024)) return 0;
  Broken = *Packed; Broken.Bytes[0] ^= 1;
  Broken.Digest = llvm::SHA256::hash(bytes(Broken.Bytes));
  if (Broken.unpack(Out, 256 * 1024)) return 0;
  Broken = *Packed; Broken.Bytes.push_back('x'); Broken.EncodedBytes = Broken.Bytes.size();
  Broken.Digest = llvm::SHA256::hash(bytes(Broken.Bytes));
  if (Broken.unpack(Out, 256 * 1024)) return 0;
  // Concatenated valid frames also are not this one retained record, even if
  // the appended frame is empty and does not change decompressed output size.
  llvm::SmallVector<uint8_t, 0> EmptyFrame;
  compression().compress({}, EmptyFrame, llvm::compression::zstd::BestSpeedCompression, false);
  Broken = *Packed;
  Broken.Bytes.append(reinterpret_cast<const char *>(EmptyFrame.data()), EmptyFrame.size());
  Broken.EncodedBytes = Broken.Bytes.size();
  Broken.Digest = llvm::SHA256::hash(bytes(Broken.Bytes));
  if (Broken.unpack(Out, 256 * 1024)) return 0;
  return 1;
}
}
extern "C" int ZilNativePackedStageFrameCounterexample() { return zil::packedStageFrameCounterexample(); }
extern "C" int ZilNativePackedStageCodecCounterexample() { return zil::packedStageCodecCounterexample(); }
