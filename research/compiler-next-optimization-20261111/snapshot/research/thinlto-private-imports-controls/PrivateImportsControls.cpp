// Separate, test-only opt plugin. Never loaded into a formal lld invocation.
// Reuses the exact formal implementation, without adding any formal pass entry.
#define ZIL_PRIVATE_IMPORTS_NO_ENTRY
#include "../thinlto-private-imports/PrivateImports.cpp"
#include "llvm/AsmParser/Parser.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/Support/SourceMgr.h"

namespace {
using namespace zil_private_imports;
static std::unique_ptr<Module> fixture(LLVMContext &Context, bool Address) {
  StringRef Direct = R"(
    define available_externally i32 @callee(ptr %p) {
    entry:
      ret i32 7
    }
    define i32 @caller() {
    entry:
      %r = call i32 @callee(ptr null)
      ret i32 %r
    }
  )";
  StringRef SameUserAddress = R"(
    define available_externally i32 @callee(ptr %p) {
    entry:
      ret i32 7
    }
    define i32 @caller() {
    entry:
      %r = call i32 @callee(ptr @callee)
      ret i32 %r
    }
  )";
  SMDiagnostic Error;
  auto M = parseAssemblyString(Address ? SameUserAddress : Direct, Error, Context);
  if (!M) {
    Error.print("PrivateImportsControls", errs());
    reject("control fixture parse failed");
  }
  M->setSourceFileName("/zil-private-imports-controls/unit-0.ll");
  if (verifyModule(*M, &errs()))
    reject("control fixture must be valid LLVM IR before inspect");
  return M;
}
static Selection selection() {
  Selection S;
  S.Index = 0;
  S.SourceFilename = "/zil-private-imports-controls/unit-0.ll";
  S.Own = {"caller"};
  S.PrelinkDefinitions = S.Own;
  S.Required = {"callee"};
  S.Allowed = {"callee"};
  S.Owners = {{"caller", 0}, {"callee", 1}};
  return S;
}
struct ControlsPass : PassInfoMixin<ControlsPass> {
  PreservedAnalyses run(Module &, ModuleAnalysisManager &) {
    LLVMContext Context;
    auto S = selection();
    auto Direct = fixture(Context, false);
    auto *Old = Direct->getFunction("callee");
    auto Attributes = Old->getAttributes();
    std::vector<FunctionSnapshot> Plans;
    std::string Reason;
    if (!inspect(*Direct, S, Plans, Reason) || Plans.size() != 1 ||
        Plans.front().Uses != 1)
      reject(Twine("valid direct control rejected: ") + Reason);
    auto Changes = rewrite(*Direct, Plans);
    auto *Private = Direct->getFunction("zil.import.0.callee");
    auto *Call = dyn_cast<CallInst>(&Direct->getFunction("caller")->front().front());
    if (Changes.size() != 1 || Direct->getFunction("callee") ||
        Private != Old || !Private->hasInternalLinkage() ||
        Private->getAttributes() != Attributes || !Call ||
        Call->getCalledFunction() != Private || verifyModule(*Direct, &errs()))
      reject("direct-use rename/linkage/body/verify control failed");

    auto Address = fixture(Context, true);
    std::string Before = moduleText(*Address);
    Plans.clear();
    Reason.clear();
    if (inspect(*Address, S, Plans, Reason) ||
        Reason != "AE has a non-direct-callee Use" ||
        moduleText(*Address) != Before || verifyModule(*Address, &errs()))
      reject("same CallInst callee-and-address Use was not rejected unchanged");
    outs() << "{\"format\":\"zil-private-imports-micro-controls/1\","
              "\"status\":\"Pass\",\"controls\":2,"
              "\"valid_direct_rename_verified\":true,"
              "\"valid_same_user_address_rejected_unchanged\":true,"
              "\"formal_manifest_receipt_path_exercised\":false}\n";
    return PreservedAnalyses::all();
  }
};
} // namespace

extern "C" LLVM_ATTRIBUTE_WEAK PassPluginLibraryInfo llvmGetPassPluginInfo() {
  PassPluginLibraryInfo Info{};
  Info.APIVersion = LLVM_PLUGIN_API_VERSION;
  Info.PluginName = "ZilPrivateImportsControls";
  Info.PluginVersion = "controls-1";
  Info.RegisterPassBuilderCallbacks = [](PassBuilder &PB) {
    PB.registerPipelineParsingCallback(
        [](StringRef Name, ModulePassManager &MPM,
           ArrayRef<PassBuilder::PipelineElement> Inner) {
          if (Name != "zil-private-imports-controls" || !Inner.empty())
            return false;
          MPM.addPass(ControlsPass());
          return true;
        });
  };
  return Info;
}
