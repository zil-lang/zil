// Research-only ABI smoke, not an optimization or a production pass.
#include "llvm/IR/PassManager.h"
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Plugins/PassPlugin.h"

using namespace llvm;

namespace {
struct ZilJumpThreadingAbiProbe : PassInfoMixin<ZilJumpThreadingAbiProbe> {
  PreservedAnalyses run(Module &, ModuleAnalysisManager &) {
    return PreservedAnalyses::all();
  }
};
} // namespace

extern "C" LLVM_ATTRIBUTE_WEAK PassPluginLibraryInfo llvmGetPassPluginInfo() {
  return {LLVM_PLUGIN_API_VERSION, "zil-jt-abi-probe", LLVM_VERSION_STRING,
          [](PassBuilder &PB) {
            PB.registerPipelineParsingCallback(
                [](StringRef Name, ModulePassManager &MPM,
                   ArrayRef<PassBuilder::PipelineElement>) {
                  if (Name != "zil-jt-abi-probe")
                    return false;
                  MPM.addPass(ZilJumpThreadingAbiProbe());
                  return true;
                });
          }};
}
