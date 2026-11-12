// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
#ifndef ZIL_MACHINE_PRODUCTS_H
#define ZIL_MACHINE_PRODUCTS_H
#include "work_products.h"
#include <memory>
#include <string>
namespace llvm { class Function; class Module; class TargetMachine; class Pass; class MachineModuleInfo; }
namespace zil {
// These helpers read actual current IR; a digest is never an authorization.
bool eligibleFunction(const llvm::Function &F);
std::string functionText(const llvm::Function &F);
std::string worldText(const llvm::Module &M, const llvm::Function *Subject);
struct MachineProducts;
struct MachineRequest;
std::shared_ptr<const MachineProducts> finishMachine(MachineRequest &);
std::unique_ptr<MachineRequest, void(*)(MachineRequest*)> createMachine(
    llvm::Module &, llvm::TargetMachine &, const std::string &,
    std::shared_ptr<const MachineProducts>, ZilNativeProductStats &, bool &Failed);
int beforeMachine(MachineRequest &, llvm::Pass &, llvm::Function &, llvm::MachineModuleInfo &);
int machineWasSkipped(MachineRequest &, llvm::Pass &, llvm::Function &);
}
// Explicit synchronous test/probe scope only; returns the prior thread mode.
// Diagnostics never enable reuse, relax a guard, or change a work budget.
extern "C" int ZilNativeProductsMachineDiagnostics(int enabled);
// Private test-only scratch-MF assertions; no default execution or reuse authority.
extern "C" int ZilNativeProductsMachineNumberingTests(int enabled);
// Bit 31 means executed; bits 0..14 identify the fifteen independent checks.
extern "C" uint32_t ZilNativeProductsMachineNumberingTestResults();
// Test-only actual AsmPrinter padding policy: -1 disabled, otherwise align64
// non-entry blocks with this max padding. Bound into the machine request key.
extern "C" int ZilNativeProductsMachineAlignmentTest(int max_bytes);
// Extra assertions within the same explicit scratch scope: expected 0b111.
extern "C" uint32_t ZilNativeProductsMachineBundleTestResults();
// Typed-body current-owner reconstruction and rejection checks; expected 255.
extern "C" uint32_t ZilNativeProductsMachineTypedBodyTestResults();
#endif
