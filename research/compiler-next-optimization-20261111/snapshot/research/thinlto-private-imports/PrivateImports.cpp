// Research-only LLVM 23.1.4 pass plugin. Link against the host's unresolved LLVM
// symbols, never a second libLLVM. No thresholds, bodies, or attributes change.
#include "llvm/ADT/ArrayRef.h"
#include "llvm/ADT/StringExtras.h"
#include "llvm/Config/llvm-config.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/PassManager.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Passes/OptimizationLevel.h"
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Plugins/PassPlugin.h"
#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/JSON.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/SHA256.h"
#include "llvm/Support/raw_ostream.h"
#include <cerrno>
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <limits.h>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>

using namespace llvm;
namespace zil_private_imports {
using Names = std::set<std::string>;
constexpr StringLiteral ManifestFormat = "zil-thinlto-private-imports-manifest/1";
constexpr StringLiteral ReceiptFormat = "zil-thinlto-private-imports-receipt/1";

[[noreturn]] static void reject(const Twine &Message) {
  report_fatal_error(Twine("ZIL_PRIVATE_IMPORTS_ERROR: ") + Message, false);
}
static std::string field(const json::Object &Object, StringRef Key) {
  auto Value = Object.getString(Key);
  if (!Value)
    reject(Twine("missing string: ") + Key);
  return Value->str();
}
static int64_t integer(const json::Object &Object, StringRef Key) {
  auto Value = Object.getInteger(Key);
  if (!Value || *Value < 0)
    reject(Twine("missing nonnegative integer: ") + Key);
  return *Value;
}
static const json::Object &object(const json::Object &Object, StringRef Key) {
  auto *Value = Object.getObject(Key);
  if (!Value)
    reject(Twine("missing object: ") + Key);
  return *Value;
}
static const json::Array &array(const json::Object &Object, StringRef Key) {
  auto *Value = Object.getArray(Key);
  if (!Value)
    reject(Twine("missing array: ") + Key);
  return *Value;
}
static const json::Object &asObject(const json::Value &Value) {
  auto *Object = Value.getAsObject();
  if (!Object)
    reject("expected JSON object");
  return *Object;
}
static Names names(const json::Object &Object, StringRef Key) {
  Names Result;
  for (const auto &Value : array(Object, Key)) {
    auto Name = Value.getAsString();
    if (!Name || Name->empty() || !Result.insert(Name->str()).second)
      reject(Twine("invalid/duplicate name in ") + Key);
  }
  return Result;
}
static void requireTrue(const json::Object &Object, StringRef Key) {
  if (Object.getBoolean(Key) != std::optional<bool>(true))
    reject(Twine("required true: ") + Key);
}
static void requireHex(StringRef Text, size_t Length) {
  if (Text.size() != Length ||
      !std::all_of(Text.begin(), Text.end(), [](char C) {
        return (C >= '0' && C <= '9') || (C >= 'a' && C <= 'f');
      }))
    reject("invalid fixed-width lowercase hex identity");
}
static std::string environment(const char *Name) {
  const char *Value = std::getenv(Name);
  if (!Value || !*Value)
    reject(Twine("missing environment: ") + Name);
  return Value;
}
static std::string absolute(const std::string &Path) {
  if (Path.empty() || Path.front() != '/')
    reject("research paths must be absolute Linux paths");
  return Path;
}
static std::string realPath(const std::string &Path) {
  char Buffer[PATH_MAX];
  if (!::realpath(absolute(Path).c_str(), Buffer))
    reject(Twine("cannot resolve path: ") + Path);
  return Buffer;
}
static std::string readFile(const std::string &Path) {
  auto Buffer = MemoryBuffer::getFile(absolute(Path));
  if (!Buffer)
    reject(Twine("cannot read bound file: ") + Path);
  if ((*Buffer)->getBufferSize() > 32 * 1024 * 1024)
    reject("research binding file exceeds 32MiB scope");
  return (*Buffer)->getBuffer().str();
}
static std::string sha256(StringRef Bytes) {
  auto Digest = SHA256::hash(
      ArrayRef<uint8_t>(reinterpret_cast<const uint8_t *>(Bytes.data()),
                        Bytes.size()));
  return toHex(ArrayRef<uint8_t>(Digest), true);
}
static std::string checkedFile(const json::Object &Binding) {
  std::string Digest = field(Binding, "sha256");
  requireHex(Digest, 64);
  std::string Bytes = readFile(field(Binding, "path"));
  if (sha256(Bytes) != Digest)
    reject("bound file SHA256 mismatch");
  return Bytes;
}
static json::Value parseJSON(StringRef Bytes) {
  auto Value = json::parse(Bytes);
  if (!Value)
    reject(Twine("invalid bound JSON: ") + toString(Value.takeError()));
  return std::move(*Value);
}
static std::string jsonText(const json::Object &Object) {
  std::string Text;
  raw_string_ostream Stream(Text);
  json::Object Copy = Object;
  Stream << json::Value(std::move(Copy));
  Stream << '\n';
  return Text;
}
static void writeNew(const std::string &Path, StringRef Bytes) {
  FILE *File = std::fopen(absolute(Path).c_str(), "wx");
  if (!File)
    reject(Twine("cannot create fresh receipt/artifact: ") + Path);
  bool Good = std::fwrite(Bytes.data(), 1, Bytes.size(), File) == Bytes.size();
  Good = std::fclose(File) == 0 && Good;
  if (!Good)
    reject(Twine("cannot finish receipt/artifact: ") + Path);
}
static std::string moduleText(const Module &M) {
  std::string Text;
  raw_string_ostream Stream(Text);
  M.print(Stream, nullptr);
  return Text;
}
struct Selection {
  unsigned Index = 0;
  std::string SourceFilename;
  Names Own, PrelinkDefinitions, Required, Allowed, Foreign;
  std::map<std::string, unsigned> Owners;
};
struct Binding {
  std::string RequestID, ManifestSHA, PluginSHA, RustSHA, PrelinkSHA, ProtocolSHA;
  std::string ReceiptDir;
  Selection Selected;
};
static void selfAnchor() {}

static Binding loadBinding(const Module &M) {
  Binding B;
  B.RequestID = environment("ZIL_PRIVATE_IMPORTS_REQUEST_ID");
  requireHex(B.RequestID, 32);
  B.ManifestSHA = environment("ZIL_PRIVATE_IMPORTS_MANIFEST_SHA256");
  requireHex(B.ManifestSHA, 64);
  auto Bytes = readFile(environment("ZIL_PRIVATE_IMPORTS_MANIFEST"));
  if (sha256(Bytes) != B.ManifestSHA)
    reject("manifest SHA256 mismatch");
  auto ManifestValue = parseJSON(Bytes);
  const auto &Manifest = asObject(ManifestValue);
  if (field(Manifest, "format") != ManifestFormat ||
      field(Manifest, "request_id") != B.RequestID)
    reject("manifest format/request binding mismatch");
  B.ProtocolSHA = field(Manifest, "protocol_sha256");
  requireHex(B.ProtocolSHA, 64);
  B.ReceiptDir = realPath(environment("ZIL_PRIVATE_IMPORTS_RECEIPT_DIR"));

  const auto &Plugin = object(Manifest, "plugin");
  B.PluginSHA = field(Plugin, "sha256");
  checkedFile(Plugin);
  Dl_info Info{};
  if (!::dladdr(reinterpret_cast<const void *>(&selfAnchor), &Info) ||
      !Info.dli_fname ||
      realPath(Info.dli_fname) != realPath(field(Plugin, "path")))
    reject("loaded plugin is not the bound plugin object");

  const auto &RustBinding = object(Manifest, "rust_receipt");
  B.RustSHA = field(RustBinding, "sha256");
  auto RustValue = parseJSON(checkedFile(RustBinding));
  const auto &Rust = asObject(RustValue);
  if (field(Rust, "format") != "zil-thinlto-export-preflight/1" ||
      field(Rust, "request") != "O2+incremental-o2+after" ||
      integer(Rust, "actual_units") != 8 ||
      integer(Rust, "original_preflight_ns") == 0 ||
      integer(Rust, "partition_input_preflight_ns") == 0)
    reject("current checked exporter receipt missing");
  const auto &Proof = object(Rust, "visibility_proof");
  if (field(Proof, "format") != "zil-thinlto-complete-visibility/1")
    reject("wrong original-planner proof");
  requireTrue(Proof, "current_complete_planner");
  requireTrue(Proof, "canonical_owners_equal");
  requireTrue(Proof, "original_required_exactly_allowed");

  const auto &PrelinkBinding = object(Manifest, "prelink_witness");
  B.PrelinkSHA = field(PrelinkBinding, "sha256");
  auto PrelinkValue = parseJSON(checkedFile(PrelinkBinding));
  const auto &Prelink = asObject(PrelinkValue);
  if (field(Prelink, "format") != "zil-thinlto-actual-visibility/1")
    reject("wrong actual prelink witness");
  requireTrue(Prelink, "prelink_complete_checked");

  const auto &Units = array(Manifest, "units");
  const auto &RustUnits = array(Proof, "units");
  const auto &CheckedUnits = array(Rust, "units");
  const auto &PrelinkUnits = array(Prelink, "units");
  if (Units.size() != 8 || RustUnits.size() != 8 || CheckedUnits.size() != 8 ||
      PrelinkUnits.size() != 8)
    reject("exact eight unit bindings required");
  Names SeenFilenames;
  uint64_t CopyCost = 0;
  std::map<std::string, uint64_t> Costs;
  unsigned Matches = 0;
  for (unsigned I = 0; I != 8; ++I) {
    const auto &Unit = asObject(Units[I]);
    const auto &RU = asObject(RustUnits[I]);
    const auto &CU = asObject(CheckedUnits[I]);
    const auto &PU = asObject(PrelinkUnits[I]);
    if (integer(Unit, "index") != I || integer(RU, "unit") != I ||
        integer(CU, "unit") != I || integer(CU, "preflight_ns") == 0 ||
        integer(PU, "unit") != I)
      reject("unit indices/preflight are not exact");
    std::string Filename = absolute(field(Unit, "source_filename"));
    if (!SeenFilenames.insert(Filename).second)
      reject("ambiguous source_filename");
    Names Own = names(Unit, "canonical");
    if (Own.empty() || Own != names(RU, "canonical"))
      reject("canonical set differs from current Rust planner proof");
    for (const auto &Name : Own)
      if (!B.Selected.Owners.emplace(Name, I).second)
        reject("canonical owners overlap");
    Names Allowed;
    uint64_t UnitCost = 0;
    for (const auto &Import : array(RU, "allowed")) {
      const auto &Object = asObject(Import);
      auto Name = field(Object, "name");
      uint64_t Cost = integer(Object, "cost");
      if (Cost == 0 || !Allowed.insert(Name).second ||
          Own.count(Name) || Name == "main")
        reject("invalid original allowed import/cost");
      auto Entry = Costs.emplace(Name, Cost);
      if (!Entry.second && Entry.first->second != Cost)
        reject("inconsistent original function cost");
      if (UINT64_MAX - UnitCost < Cost || UINT64_MAX - CopyCost < Cost)
        reject("copy cost overflow");
      UnitCost += Cost;
      CopyCost += Cost;
    }
    if (Allowed != names(Unit, "allowed_original") ||
        Allowed != names(RU, "required_original") ||
        Allowed != names(PU, "allowed_original") ||
        UnitCost != static_cast<uint64_t>(integer(RU, "copy_cost")))
      reject("Allowed/cost does not match original current proof");
    Names Required = names(Unit, "required_prelink");
    if (Required != names(PU, "required_prelink"))
      reject("Required differs from actual checked prelink graph");
    for (const auto &Name : Required)
      if (!Allowed.count(Name))
        reject("prelink Required exceeds original Allowed");
    Names PrelinkDefs = names(Unit, "prelink_definitions");
    for (const auto &Name : PrelinkDefs)
      if (!Own.count(Name))
        reject("prelink defines a non-owner symbol");
    if (Filename == M.getSourceFileName()) {
      ++Matches;
      B.Selected.Index = I;
      B.Selected.SourceFilename = Filename;
      B.Selected.Own = std::move(Own);
      B.Selected.Allowed = std::move(Allowed);
      B.Selected.Required = std::move(Required);
      B.Selected.PrelinkDefinitions = std::move(PrelinkDefs);
      checkedFile(object(Unit, "input_bitcode"));
    }
  }
  if (Matches != 1)
    reject("module source_filename has no unique manifest owner");
  if (CopyCost != static_cast<uint64_t>(integer(Proof, "total_copy_cost")) ||
      CopyCost > static_cast<uint64_t>(integer(Proof, "original_source_cost")))
    reject("original total copy budget mismatch");
  for (const auto &Entry : Costs)
    if (!B.Selected.Owners.count(Entry.first))
      reject("allowed import has no unique canonical owner");
  B.Selected.Foreign = names(Proof, "foreign");
  for (const auto &Name : B.Selected.Foreign)
    if (B.Selected.Owners.count(Name))
      reject("foreign symbol collides with canonical owner");
  return B;
}

struct Change {
  std::string Canonical, Private;
  uint64_t DirectUses;
  int VisibilityBefore, VisibilityAfter, DLLBefore, DLLAfter;
  bool DSOLocalBefore, DSOLocalAfter;
};
struct FunctionSnapshot {
  Function *F;
  std::string OldName, NewName;
  AttributeList Attributes;
  CallingConv::ID CC;
  std::vector<const BasicBlock *> Blocks;
  std::vector<const Instruction *> Instructions;
  std::vector<const Value *> Operands;
  uint64_t Uses;
  int Visibility, DLL;
  bool DSOLocal;
};

// The complete gather/check phase precedes every mutation. A failed nearest
// address-use/collision/coverage control leaves the current Module untouched.
static bool inspect(Module &M, const Selection &S,
                    std::vector<FunctionSnapshot> &Plans, std::string &Reason) {
  auto Fail = [&](const char *Text) { Reason = Text; return false; };
  if (M.getSourceFileName() != S.SourceFilename || !M.global_empty() ||
      !M.alias_empty() || !M.ifunc_empty() || !M.getModuleInlineAsm().empty())
    return Fail("non-pure module or wrong source identity");
  Names Definitions;
  for (Function &F : M) {
    auto Name = F.getName().str();
    if (F.hasComdat() || F.hasPersonalityFn() || F.hasPrefixData() ||
        F.hasPrologueData() || F.hasGC())
      return Fail("COMDAT/personality/prologue/GC outside pure scope");
    if (F.isDeclaration()) {
      if (!S.Owners.count(Name) && !S.Foreign.count(Name) && !F.isIntrinsic())
        return Fail("unknown declaration");
      continue;
    }
    if (!Definitions.insert(Name).second)
      return Fail("duplicate definition");
    if (!F.hasAvailableExternallyLinkage()) {
      if (!S.Own.count(Name) || !S.PrelinkDefinitions.count(Name))
        return Fail("non-AE definition is not this prelink owner");
      continue;
    }
    auto Owner = S.Owners.find(Name);
    if (!S.Allowed.count(Name) || Owner == S.Owners.end() ||
        Owner->second == S.Index || S.Own.count(Name) || F.isIntrinsic() ||
        F.isUsedByMetadata() || F.getAddressSpace() != 0)
      return Fail("AE definition is not an allowed cross-owner pure source");
    std::string NewName = "zil.import." + std::to_string(S.Index) + "." + Name;
    if (M.getNamedValue(NewName))
      return Fail("private import name collision");
    uint64_t DirectUses = 0;
    for (Use &U : F.uses()) {
      auto *Call = dyn_cast<CallInst>(U.getUser());
      if (!Call || !Call->isCallee(&U) || Call->getCalledFunction() != &F ||
          Call->getFunctionType() != F.getFunctionType() ||
          Call->getNumOperandBundles() != 0)
        return Fail("AE has a non-direct-callee Use");
      ++DirectUses;
    }
    FunctionSnapshot P{&F, Name, NewName, F.getAttributes(), F.getCallingConv(),
                       {}, {}, {}, DirectUses,
                       static_cast<int>(F.getVisibility()),
                       static_cast<int>(F.getDLLStorageClass()), F.isDSOLocal()};
    for (BasicBlock &BB : F) {
      P.Blocks.push_back(&BB);
      for (Instruction &I : BB) {
        P.Instructions.push_back(&I);
        for (const Use &U : I.operands())
          P.Operands.push_back(U.get());
      }
    }
    Plans.push_back(std::move(P));
  }
  for (const auto &Name : S.Required)
    if (!S.Allowed.count(Name) || !Definitions.count(Name))
      return Fail("required prelink body missing before O2 inlining");
  return true;
}

static std::vector<Change>
rewrite(Module &M, const std::vector<FunctionSnapshot> &Plans) {
  std::vector<Change> Changes;
  for (const auto &P : Plans) {
    P.F->setName(P.NewName);
    P.F->setLinkage(GlobalValue::InternalLinkage);
    if (P.F->getName() != P.NewName || M.getNamedValue(P.OldName) ||
        !P.F->hasInternalLinkage() || P.F->getAttributes() != P.Attributes ||
        P.F->getCallingConv() != P.CC)
      reject("rename/linkage/attribute invariant failed");
    std::vector<const BasicBlock *> Blocks;
    std::vector<const Instruction *> Instructions;
    std::vector<const Value *> Operands;
    for (BasicBlock &BB : *P.F) {
      Blocks.push_back(&BB);
      for (Instruction &I : BB) {
        Instructions.push_back(&I);
        for (const Use &U : I.operands())
          Operands.push_back(U.get());
      }
    }
    if (Blocks != P.Blocks || Instructions != P.Instructions ||
        Operands != P.Operands)
      reject("plugin changed body structure or operand identity");
    Changes.push_back({P.OldName, P.NewName, P.Uses,
                       P.Visibility, static_cast<int>(P.F->getVisibility()),
                       P.DLL, static_cast<int>(P.F->getDLLStorageClass()),
                       P.DSOLocal, P.F->isDSOLocal()});
  }
  return Changes;
}

struct PrivateImportsPass : PassInfoMixin<PrivateImportsPass> {
  PreservedAnalyses run(Module &M, ModuleAnalysisManager &) {
    Binding B = loadBinding(M);
    std::string Prefix = B.ReceiptDir + "/unit-" + std::to_string(B.Selected.Index);
    json::Object Receipt{
        {"format", ReceiptFormat}, {"request_id", B.RequestID},
        {"unit", static_cast<int64_t>(B.Selected.Index)},
        {"phase", "ThinLTOPostLink"}, {"source_filename", M.getSourceFileName()},
        {"manifest_sha256", B.ManifestSHA}, {"plugin_sha256", B.PluginSHA},
        {"rust_receipt_sha256", B.RustSHA}, {"prelink_witness_sha256", B.PrelinkSHA},
        {"protocol_sha256", B.ProtocolSHA}, {"verify_before", false},
        {"verify_after", false}};
    writeNew(Prefix + ".started.json", jsonText(Receipt));
    std::string Before = moduleText(M);
    writeNew(Prefix + ".before.ll", Before);
    if (verifyModule(M, &errs()))
      reject("actual LLVM verify-before failed");
    Receipt["verify_before"] = true;
    std::vector<FunctionSnapshot> Plans;
    std::string Reason;
    if (!inspect(M, B.Selected, Plans, Reason))
      reject(Reason);
    auto Changes = rewrite(M, Plans);
    // Receipt order is deterministic; never reorder the Module function list.
    std::sort(Changes.begin(), Changes.end(),
              [](const Change &A, const Change &B) {
                return A.Canonical < B.Canonical;
              });
    std::string After = moduleText(M);
    writeNew(Prefix + ".after.ll", After);
    if (verifyModule(M, &errs()))
      reject("actual LLVM verify-after failed");
    Receipt["verify_after"] = true;
    Receipt["before"] =
        json::Object{{"path", Prefix + ".before.ll"}, {"sha256", sha256(Before)}};
    Receipt["after"] =
        json::Object{{"path", Prefix + ".after.ll"}, {"sha256", sha256(After)}};
    json::Array Imports;
    for (const auto &C : Changes)
      Imports.push_back(json::Object{
          {"canonical", C.Canonical}, {"private", C.Private},
          {"direct_uses", static_cast<int64_t>(C.DirectUses)},
          {"visibility_before", C.VisibilityBefore},
          {"visibility_after", C.VisibilityAfter}, {"dll_before", C.DLLBefore},
          {"dll_after", C.DLLAfter}, {"dso_local_before", C.DSOLocalBefore},
          {"dso_local_after", C.DSOLocalAfter}});
    Receipt["imports"] = std::move(Imports);
    Receipt["automatic_internal_header_normalization"] =
        "Internal sets default visibility/DLL storage and dso_local=true";
    writeNew(Prefix + ".json", jsonText(Receipt));
    return Changes.empty() ? PreservedAnalyses::all() : PreservedAnalyses::none();
  }
};
} // namespace zil_private_imports

#ifndef ZIL_PRIVATE_IMPORTS_NO_ENTRY
extern "C" LLVM_ATTRIBUTE_WEAK PassPluginLibraryInfo llvmGetPassPluginInfo() {
  PassPluginLibraryInfo Info{};
  Info.APIVersion = LLVM_PLUGIN_API_VERSION;
  Info.PluginName = "ZilPrivateImports";
  Info.PluginVersion = "research-1-llvm-" LLVM_VERSION_STRING;
  Info.RegisterPassBuilderCallbacks = [](PassBuilder &PB) {
    PB.registerPipelineEarlySimplificationEPCallback(
        [](ModulePassManager &MPM, OptimizationLevel Level,
           ThinOrFullLTOPhase Phase) {
          if (Phase == ThinOrFullLTOPhase::ThinLTOPostLink) {
            if (Level != OptimizationLevel::O2)
              zil_private_imports::reject("private imports require standard O2");
            MPM.addPass(zil_private_imports::PrivateImportsPass());
          }
        });
  };
  return Info;
}
#endif
