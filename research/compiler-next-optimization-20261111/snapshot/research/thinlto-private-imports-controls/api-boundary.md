# LLVM23 私有导入插件的实现边界

固定LLVM23.1.4 commit：[7a941dfb2877a0b8cf30323fefa85ad019e0b0f5](https://github.com/llvm/llvm-project/commit/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5)。此报告针对实验候选，不承诺运行收益或生产采用；作者只写源码并读SDK/官方源码，未编译、加载LLVM或运行native。

## 准确API与时序

23版plugin header是[llvm/Plugins/PassPlugin.h](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Plugins/PassPlugin.h)，LLVM_PLUGIN_API_VERSION=2。导出llvmGetPassPluginInfo，填PassPluginLibraryInfo的RegisterPassBuilderCallbacks，PreCodeGenCallback保持nullptr，不能替代标准后端。

[PassBuilder.h:506–514](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Passes/PassBuilder.h#L506-L514)的registerPipelineEarlySimplificationEPCallback准确签名为(ModulePassManager&, OptimizationLevel, ThinOrFullLTOPhase)。正式插件仅当Phase==ThinLTOPostLink且Level==O2插入pass；普通/prelink阶段不插入，不注册手动pipeline入口。

标准ThinLTO postlink管线在[1949–2014](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1949-L2014)调用module simplification；EP在[1218](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1218)，已晚于真实import及早期ICP/type处理，早于IPSCCP和[AlwaysInliner/标准inliner](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Passes/PassBuilderPipelines.cpp#L1323-L1328)。它不是回到原machine阶段，也不是跳过任何标准O2优化。

lld支持--load-pass-plugin（[Options.td](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/Options.td)）；[LTOBackend.cpp:202–218](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L202-L218)加载并注册插件，[297–299](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTOBackend.cpp#L297-L299)对当前PassBuilder执行注册。root已只读核实SDK ld.lld为静态含LLVM且导出所需host符号；实际SO加载仍是待执行功能验证，不能由nm代替。

## 绑定契约

环境变量四项：ZIL_PRIVATE_IMPORTS_MANIFEST、ZIL_PRIVATE_IMPORTS_MANIFEST_SHA256、ZIL_PRIVATE_IMPORTS_REQUEST_ID（32位lowercase hex）、ZIL_PRIVATE_IMPORTS_RECEIPT_DIR。manifest format固定zil-thinlto-private-imports-manifest/1，含request_id、protocol_sha256、plugin/rust_receipt/prelink_witness的path+sha256，以及8项units。

每unit字段为index、source_filename、input_bitcode{path,sha256}、canonical、prelink_definitions、required_prelink、allowed_original。source_filename来自actual prelink文本，8项唯一，插件与Module.getSourceFileName精确匹配，不猜ModuleID。插件读取并校验当前Rust完整preflight/原budget proof、actual prelink witness及自身SO；dladdr使用本地anchor并realpath对实际加载SO，无第二份LLVM。

输出每unit四个fresh create-new文件：unit-N.started.json、unit-N.before.ll、unit-N.after.ll、unit-N.json，零AE也同样执行。最终receipt固定format/request_id/unit/phase/source_filename/manifest_sha256/plugin_sha256/rust_receipt_sha256/prelink_witness_sha256/verify_before/verify_after/before/after/imports，imports按canonical字典序，仅排序收据，不移动Module函数列表。extra字段明记Internal自动visibility/DLL/dso_local规范化。

manifest或早期校验失败可能只有错误stderr而无完整unit文件；root必须缺任一8unit最终收据即拒绝，不把partial或started当成功。调用前后实际LLVM verify，实际module全文保存并hash；父wrapper还独立规范化对比全文，仅允许预定名字/合法linkage-header变化。

## 唯一变换与身份边界

只处理实际AvailableExternally且有body的函数，要求在当前Allowed、属于其它唯一canonical owner；其它definition必须是本unit prelink owner。任何global/alias/ifunc/inline asm、COMDAT/personality/prologue/prefix/GC、未知声明或definition、metadata地址使用及name collision都失败关闭。当前支持pure固定fixture，不扩大地址/状态场景。

所有Use必须分别检查为CallInst且CallInst::isCallee(&Use)、实际called Function和FunctionType一致，无operand bundles；不能仅看User是CallInst，否则call @f(ptr @f)的参数地址会被同一个User掩盖。官方Use角色API：[InstrTypes.h](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/InstrTypes.h)。

先gather/check全部目标，再唯一执行Function.setName(zil.import.<unit>.<canonical>)及setLinkage(Internal)。函数对象、block/instruction/operand指针序列、calling convention及AttributeList保持；不加声明、不clone、不加alwaysinline、不调阈值、不改body或metadata。所有direct调用仍指向原Function对象，递归/互递归边随名称打印自动保持。此pure场景本module已没有canonical地址引用，原canonical owner在其它module保持，所以无需新增无use声明。

[GlobalValue::setLinkage](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/IR/GlobalValue.h#L547)自动把local linkage的visibility/DLL storage规范为default并设置dso_local；插件不拷回旧AE属性，收据记录前后状态。这是唯一额外合法header打印差异，不是新函数属性优化。

## 构建与控制范围

Linux shared plugin仅使用同SDK headers、C++17、_GLIBCXX_USE_CXX11_ABI=1、-fno-exceptions/-fno-rtti；宿主解析未定义LLVM符号，系统-ldl允许。禁止-lLLVM、整包静态LLVM或另造LLVM DSO，避免第二套选项/运行时状态。plugin path+SHA、CPP/sourcecommit、完整编译命令、clang及构建log由root独立绑定。

原生ThinLTO cache key不覆盖插件字节和该环境manifest；本轮必须保持每请求初始empty cache和8份fresh pass receipts。不能让旧cache命中跳过该pass而仍算候选成功。

ROOT外控制SO仅测试inspect/rewrite两项合法LLVM输入，不覆盖正式manifest/file-binding/fresh8receipt。正式入口没有为控制扩展；必须由root授权窗口编译加载。后续无论功能控制还是6compile/54native质量首败都完整保存，不改原1.10/.90门，不从此设计推断已达收益。
