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
#endif
