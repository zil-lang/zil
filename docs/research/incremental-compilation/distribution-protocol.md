# 第五阶段冻结测量协议（2026-10-31，北京时间）

用户要求补足大模块分布、核实图切割/zit贡献、同工作负载对比Rust并针对差距继续优化。生产候选801933a9e，基线12fc97bde（生产9fe5f55e）。既有阶段2–4窗口不重标。

## 样本与计量边界

合成23族：单模块128/512/2048/4096/8192函数；4096个helper按8/64/512/1024个叶模块均匀分布和偏斜8模块；8192个helper按8×1024、4×2048和偏斜8模块分布；分支、泛型标量、CT、64语句函数体、名义类型泛型实例；16/32/64模块的类型实例梯度；较小函数体规模；64模块依赖链与高fanout。root/foreign/泛型生成函数不混入helper数，实际文件数、行数、字节及最大文件单列。资源/闭包/错误/借入/缓冲区/组件组合由14套已签入独立ZIL/Rust源码对照补充。它们是生成压力与已有小应用，不是随机抽取的企业大仓库；Rust宏/trait规模与多Cargo crate依赖分布没有因此覆盖。

合成每臂每区组固定cold→unchanged→early_edit→late_edit→restore。early改首叶首函数，late恢复early并改末叶末函数；均保留全部声明，4字节完整结果须等于生成器独立整数预期。所有冷启动中的OS页缓存不清空；每序列新的编译器增量目录，序列内持久。默认ZIL使用显式对象缓存/8CGU/jobs2，Rust使用直接rustc/8CGU/增量缓存/overflow checks/panic=abort；目标机器相同，Rust内置LLVM23.1.1与ZIL LLVM23.1.4单列。CLI包含启动、读源码、编译、链接和发布；原生运行在计时外。check为ZIL到当前Verify、Rust metadata，不是Cargo整个构建图或持久IDE会话。

真实源码每臂cold→unchanged；每次成功构建执行15个输入向量，exit/stdout/stderr逐项与独立Rust O0 oracle相等，合法应用非零退出保留，signal崩溃/超时无效。部分程序不消费输入，这15次不冒充15种独立路径。app沿其既有源码注释对Rust传--check，使双方均执行同一内部结果断言，不把打印wrapper差异认成编译错误。源码不修改。其余默认运行参数为空。

每窗6个区组，AB/BA交替，每区组每臂一条完整请求序列。CPU固定当前允许集合的前两个；独立审核/构建/其他性能测量在正式窗期间停止。wall由perf_counter_ns包围subprocess；CPU/RSS来自wait4，Rust子进程链接资源计入系统提供的rusage，峰值RSS不是同时各进程RSS之和。超时/启动失败/非JSON/输出丢失/原生不等均先保留行；任一臂有失败的单元不计算等价速度比。默认预算不改，预检已发现的body-64x16与types-64x16 ResourceLimit继续完整保留。

当前ZIL --cache-dir只读写未核原始源码bytes，不缓存跨进程Typed查询；不能把它当成Rust的持久语义增量缓存。主窗未启用这项无语义跳过能力的源码存储；代码边界见session.rs::disk_source，单独开关控制不并入主窗。

## 冻结窗口

1. generated-default：23族×check/O0/O2×5请求×6区组×2臂＝4140行，默认候选对Rust。
2. generated-closed：相同23族×O2×5请求×6区组×2臂＝1380行；候选显式--closed-executable，Rust不改选项。该模式放弃生成ZIL符号的外部导出合同，不能归作默认收益。
3. implementation-ablation：large-128/512/2048/4096/8192、even-64x64、skew-8、body-16x16共8族×O0/O2×5请求×6区组×2臂＝960行，9fe默认对801默认，使用同一生成器和native合同；没有Rust臂。
4. real-default：14族×check/O0/O2×2请求×6区组×2臂＝1008行，另14个Rust O0 oracle构建。
5. real-closed：14族×O2×2请求×6区组×2臂＝336行，另14个Rust O0 oracle构建。

共7824测量请求，另28个参考构建。所有单元保留，不按结果调整组成或权重。正式结束后报告每单元双臂中位值、比值、峰值RSS、完整逐区组数据；若给探索性区间，使用同区组配对bootstrap、固定种子并明确6区组样本量，不能称正式产品置信保证。不把不同窗口的最佳数相乘，也不将合成case平均值称为语言总体优势。

## 实现及已有反例

5189f0975：O0大文件每256个非foreign LIR函数固定分块，当前local-linkage引用仍合并；O2/count1/小文件保持原策略。仅LIR函数集合与顺序不变时有局部编辑稳定性。

c6a60d3ec：从旧CP13移植显式封闭Linux build；真实跨单元调用/取地址/global initializer仍保留External；默认/emit/public Wire/报告形状不变。

801933a9e：大CFG当前支配树的迭代DFS区间；保持旧不可达、phi及首错规则。独审用独立集合定点与旧walk以及完整旧/新preflight交叉验证，不用LLVM对象命中代替当前验证。

预检失败原样保留：初版Rust路径误解为rustup入口、mod127原生oracle被审查指出过弱后改完整4字节、测试helper把Clang多进程误断言为1次、一个测试枚举名过渡编译失败、第一次复制旧ABI测试目标目录缺失、API特性名称/测试Clippy修正、app未传--check的wrapper不等。这些都不是有效性能窗口或编译器性能收益。
