# 请求内 canonical prelink 复用

当前是显式研究入口，不改变默认编译路径。前一条外部 ThinLTO 路径证明了受限私有导入的运行质量，但请求级身份去重和线性 IR 检查后完整编译仍慢于主线；因此本实现将 canonical prelink、裁剪、导入和 postlink O2 放入既有 LLVM C API 进程内，以实测判断总成本。

启用 `ZIL_RESEARCH_PRELINK_RECEIPT=/绝对/新路径`，仅接受 Linux x64、LLVM API、main、O2、incremental-o2、function-merging after、8 units/2 jobs、无 closed/object cache。它与旧 ThinLTO exporter 互斥。主线普通入口、Config/Receipt/机器模型、语言 Verify 和原导入额度均保持；不生成 ThinLTO summary/index，也不冒充官方 importer 或跨请求缓存。

两阶段：先对全部 canonical units 执行当前机器预检→Builder→LLVM verify→thinlto-pre-link<O2>→verify→内存 bitcode；全部 worker join 后，目的 unit 按当前实际调用闭包算 Required⊆原 Allowed，逐 donor 解析、globaldce 裁剪且核全部定义集合、链接、逐 Use 证明后私有改名，再完整 thinlto<O2>→verify→当前对象发射器。只传 owned bytes，LLVM handles 留在原线程，无跨阶段锁。

验收必须先冻结精确源码、独立复核新增 C ABI/所有权/闭包/地址反例，再实际构建与原两个纯多文件用例的完整请求和机器输出。运行门为逐 case B/A≤1.10 且 Rust/B≥0.90，编译 wall/CPU 必须真实净胜；不从旧研究耗时中扣除工具开销。未过门不扩大矩阵、不采用。此原型不声称多点修改或并发候选已经获益。
