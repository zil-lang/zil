# LLVM 优化与原生发射增量化：固定源码参考

这些引用用于实现和审查，不代表 ZIL 已完成逐函数优化或发射。模块对象命中、函数输入不变、优化结果可复用与原生片段可重放是四个不同边界。必须保留当前独立 Verify、原优化合同、LLVM 验证和完整 AOT 可执行文件消费者。

## 固定版本及许可证

| 工程 | 固定提交 | 许可证 |
|---|---|---|
| LLVM23.1.4，与当前运行库安装收据一致 | `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5` | Apache-2.0 WITH LLVM-exception |
| LLVM19.1.7，历史比较参考 | `cd708029e0b2869e80abe31ddb175f7c35361f90` | Apache-2.0 WITH LLVM-exception |
| Zig0.15.1 | `3db960767d12b6214bcf43f1966a037c7a586a12` | MIT |
| Souper，固定研究实现 | `963d4df436f3dc0b039cc0e47ada0577a26f5c4e` | Apache-2.0 |

官方 Git tag 已解引用为上述提交。源码、许可证与下载文件 SHA-256 见 [references.json](references.json)；原文证据当前在本地，另行归档前不能称为远端可恢复证据。没有将第三方实现复制到生产代码。

## 真正可以借鉴的边界

**ThinLTO：全局摘要加后端模块缓存。** [LLVM23 cache key](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTO.cpp#L138) 绑定当前模块、实际导入成员及来源内容、导出/符号决议、global属性、type/CFI解析、target/优化配置等。导入决定还依赖 [callee指令数、inline属性](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L307) 和 [调用热度/阈值](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/FunctionImport.cpp#L887)。函数由“不导入”变为“导入”也是实际依赖变化。模块命中在后端 parse/import/opt/codegen 前停止；未命中的后端模块仍整体处理，不能称为模块内函数优化增量。

LLVM23 的 [FirstRoundThinBackend](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTO.cpp#L1737) 分别缓存优化 IR 与对象，任一缺失仍运行后端；[第二轮](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTO.cpp#L1830) 从优化 IR 继续 codegen，最终key还包含 CombinedCGDataHash。可借鉴优化与发射产物分层，不能忽略全局机器优化数据。

**New PassManager：合法重放必须参与分析失效。** [真实PM执行顺序](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/PassManagerImpl.h#L61) 是 pass.run→AM.invalidate→after instrumentation；optional before callback返回false会把三步全部绕过。[Instrumentation](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/PassInstrumentation.h#L233) 不提供替换 PreservedAnalyses 的接口。应使用真正pass wrapper重放已证明结果并返回合法失效信息，不能在skip callback偷偷修改IR。

函数pass不等于仅依赖本函数。默认O2的 [GlobalsAA与CGSCC函数阶段](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L994) 可读取模块/跨函数分析。相同函数文本和callee声明attrs不足以授权通用缓存。还需覆盖global initializer、alias、module flags、data layout、TLI/TTI、profile、阶段参数、重复出现/CGSCC重访、额外声明和metadata副作用。

[自定义嵌套pipeline parser](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilder.cpp#L2360) 可作为合法wrapper接入口，但默认pipeline打印/重新解析是否保留全部构造状态必须实证。不能用减少pass、optnone/noinline或删除跨函数阶段制造命中。完整module作为所有key的输入虽保守，却会在私有编辑时全失效，不能当函数增量终态。

**真正的优化内部push：Attributor/NewGVN。** [Attributor::runTillFixpoint](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/IPO/Attributor.cpp#L2182) 记录动态依赖，CHANGED触发dependents，区分required/optional失效并清理旧边；达到迭代上限时不把未稳定状态当证明。[NewGVN](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Transforms/Scalar/NewGVN.cpp#L2091) 除SSA users还追踪额外、memory和predicate依赖。这些是在当前优化运行内的实际工作队列，不能直接声称其状态已支持跨编辑持久化。跨编辑保留需要删除/重建、分析和固定点失效合同。

**Clang incremental/ORC：JIT边界。** [IncrementalExecutor](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/clang/lib/Interpreter/IncrementalExecutor.cpp) 将部分翻译单元加入LLJIT并用resource tracker管理生命周期；[SimpleCompiler](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/ExecutionEngine/Orc/CompileUtils.cpp#L38) 的object cache仍缓存完整Module。可参考资源撤销和层间接口；不能将它当作完整AOT exe或默认O2逐函数增量实现。

**Zig：真实逐函数原生codegen/ELF更新。** [updateFunc](https://github.com/ziglang/zig/blob/3db960767d12b6214bcf43f1966a037c7a586a12/src/link.zig#L752) 明确要求 `llvm_object == null`。原生 [ZigObject.updateFunc](https://github.com/ziglang/zig/blob/3db960767d12b6214bcf43f1966a037c7a586a12/src/link/Elf/ZigObject.zig#L1414) 重建函数relocations/机器码/DWARF，通过 [updateNavCode](https://github.com/ziglang/zig/blob/3db960767d12b6214bcf43f1966a037c7a586a12/src/link/Elf/ZigObject.zig#L1254) 管理atom容量、对齐、移动和实际写入。其移动函数trampoline路径仍有过对齐更新所有引用的TODO。不能把它描述为不受限完成的LLVM优化方案。

[ELF flush](https://github.com/ziglang/zig/blob/3db960767d12b6214bcf43f1966a037c7a586a12/src/link/Elf.zig#L763) 仍做全局符号、GC、relocation、布局及synthetic sections等工作。稳定atom/重定位依赖可借鉴，但trampoline布局不等于当前fresh canonical ELF同字节合同。

**Souper：真正细粒度优化query缓存。** [MemCachingSolver](https://github.com/google/souper/blob/963d4df436f3dc0b039cc0e47ada0577a26f5c4e/lib/Extractor/Solver.cpp#L657) 用规范化BPCs/PCs/LHS缓存推断RHS；[LLVM pass消费者](https://github.com/google/souper/blob/963d4df436f3dc0b039cc0e47ada0577a26f5c4e/lib/Pass/Pass.cpp#L269) 将结果映射到当前LLVM values/插入点。可借鉴封闭语义query和typed replay，但它不是整个默认O2的增量化。该源码外部cache缺少升级版本管理，错误/空RHS和多结果缓存也有局限；不能照抄其失败、负结果与持久化策略。

## 原生发射重放的额外证明

优化后函数相同也不自动证明机器产物独立。[AsmPrinter初始化](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/CodeGen/AsmPrinter/AsmPrinter.cpp#L529) 读取模块metadata，并扫描所有函数的CFI/unwind要求；[finalization](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/CodeGen/AsmPrinter/AsmPrinter.cpp#L2884) 汇总全局状态并结束MCStreamer。若用serialized MIR作为产物，应重绑当前IR/MMI/MachineFunction编号、符号、常量池、frame/unwind等，并保留当前AsmPrinter/MC流程。解析成功和正文机器码相同都不足以证明完整对象相同。

实现须单列真实优化pass执行/重放、优化结果相等、真实ISel/regalloc等发射工作复用、fallback与全局组装计数。必须覆盖callee/global/context变化、SCC重构、不同阶段、删除、metadata/声明副作用、变长机器码、错误/取消/预算恢复，以及原完整路径的LLVM/object/exe和运行质量对照。严格首域可以成为工程步骤，不能作为完成一般优化/发射目标的理由。
