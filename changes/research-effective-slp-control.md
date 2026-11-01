# 有效 SLP 全局门控对照

前一次仅调用 LLVMPassBuilderOptionsSetSLPVectorization(false) 的隔离候选，被 LLVM 的 default<O2> alias 重设覆盖，保留为失败消融。本对照使用同一冻结编译器和同一研究 shim，通过已有 LLVMParseCommandLineOptions 显式选择 `-vectorize-slp=true` 或 `false`。精确官方 LLVM 23.1.4 commit `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5` 的 SLPVectorizer.cpp L125 定义全局 RunSLPVectorization；L27805 在实际 runImpl 入口读取它。该值不经过 PipelineTuningOptions，所以不会被 default alias 覆盖。

不改变生产 C API/unsafe 边界、默认策略或 CLI，也不改 LLVM。两个研究臂使用相同的实际 production pipeline、late MergeFunctions、目标及 ABI；只有全局 SLP 门控选择不同。SLP pass 仍可能出现在计时表，因为 run() 在 runImpl 门控前取得分析，不能要求“表中没有 SLP 名称”作为有效性标准。必须先通过真实 C API 的无循环、连续独立浮点加法阳性控制证明 on 产生向量 fadd、off 仍是标量，并用单标量负例与外部 C harness 检查原生结果。

research shim 只在明确指定且版本/库身份匹配的进程设置选项。请求环境、probe SHA、原生编译器/库 SHA、全局选择、实际管线收据一起构成研究身份。生产对象缓存按既有 LD_PRELOAD 规则禁用，必须在每次真实 CLI 请求中断言；不存在将未包含全局门控的生产 cache key 用来命中的路径。current Verify、结构化机器预检、前后 LLVMVerify 和失败发布合同仍由原生产编译器执行。

功能对照覆盖 opaque 输入的 512/2048/8192 helper 算术与分支，after 策略，default/closed 各自 on/off。记录实际 pass CPU、编译阶段 wall、对象/可执行文件尺寸、五个原生输入输出和运行时间，功能窗口计时不作为正式收益。任何正式比较须另开排他窗；不得把先前 ca79 scalar 改动混入43037同二进制 SLP 对照。
