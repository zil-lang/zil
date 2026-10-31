# 编译7：CP25、ZIT、Rust 与 Blaze 的可用思路

检索与核对日期：2026-10-31。此文说明研究归因和实现取舍；本轮测量、采用结论另见 COMPILE7-RESULT.md。外部项目的数字不作为 ZIL 的性能证据。

## CP25 是否已经有性能对比

没有。`CP25-FEASIBILITY.md` 是只读可行性研究，未实现生产版稳定 owner / 事务依赖收集器，也没有其独立 A/B。CP22 的解析 memo、CP26/27 的修复组合、CP28 的独立所有权修复属于不同切片，不能把它们的计时改记为 CP25。

历史归因：CP22/原485的完整混合 wall 比为1.020513；CP26/CP22为0.993843；CP22+26+27最终组合/原485为1.075838。后者未获完整成本采用依据。当前编译7基线保留的是CP28两处临时文件所有权修复，完整crates树为`a7ae8da1862a69cf673b8db50431b4991cb0d1c5`。CP28的正确性通过不等于单独测得加速。

来源：本仓库 `CP25-FEASIBILITY.md`、`COMPILE6-RESULT.md`、`CP28-CACHE-OWNERSHIP.md`，以及本轮独立CP25审计和ZIT机制重跑收据。

## ZIT 有价值的部分

本轮再核原上传 `zit.ts` 的731行源文件，并重跑五组同步机制控制。原文件SHA256：`719a914356a24ac1edbeb1ffe6637d22311c9299e6be9ddbcdf3f7fdddf1474f`。

| 机制 | 可迁移的价值 | 编译器需要补足的条件 |
| --- | --- | --- |
| bond双向依赖边、读游标 | 稳定读序减少边分配，删掉不再读取的依赖 | 稳定查询owner与当前输入身份分开；读序变化不能冒充全部复用 |
| write/trig结果相等截止 | 私有实现变动后，接口结果不变便不继续通知接口消费者 | 比较真实语义结果面；不能拿全输入hash或构造过程hash替代 |
| read/flush确保上游当前 | 下游暂时clean时仍不读到过期上游 | 完整ensure-current协议，特别是条件分支与负查找 |
| 高度队列与去重 | DAG菱形两侧稳定后汇合节点只重算一次 | 递归依赖需要SCC/固定点；高度不是正确的循环判定证明 |
| 重算后修剪动态边 | 删除已经不成立的依赖 | 结果与新边事务提交；失败或取消保持旧成功版本 |

一个实测反例：1100次交替切换两种各自无环的依赖方向，会把高度推到1101/1102/1103，出现103次`zit_dep_cycle`警告。这证明固定高度阈值不能直接成为编译器的cycle规则。此处测试只覆盖数值同步路径，不代表ZIT所有DOM/异步功能已验证，更不是ZIL性能测量。

ZIL现有resource-effects worklist已经有直接调用者通知、去重和相等截止，本轮不重复记为新增成果。当前更大的缺口是查询粒度：文件级Parse之后仍有整项目Expand/CT/Types/Analyze，以及全局编号、位置和共享预算约束。只替换调度队列不会自动减少这些工作。

## Rust 与 Blaze 对本轮的启发

Rust的红绿查询机制区分输入变动和结果是否改变，并保留依赖的实际读取顺序。遇到控制流改变，旧分支的后续依赖不应盲目重算。稳定指纹也有CPU成本，应连同查询和序列化成本测量，不能把命中率当成净收益。[Rust incremental](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html)、[incremental details](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html)。

Blaze把常驻数据库、函数粒度编译和机器码缓存结合起来。其磁盘缓存与驻留数据库承担不同范围：磁盘条目绑定稳定名称及编译条件，编辑后的函数级复用主要发生在同一daemon内。这提示ZIL必须分别测首次请求、常驻私有编辑、跨进程恢复，不能把三者混为一个“增量”。[Architecture](https://github.com/indexable-inc/blaze/blob/main/docs/architecture.md)、[Cache](https://github.com/indexable-inc/blaze/blob/main/docs/cache.md)。

更直接的差距是源码位置：Blaze的函数增量报告把函数体相对语法与文件中的anchor位置分开，减少在文件前方插入一行时对后续函数的无谓失效。ZIL本轮CP29仍以完整Core及绝对位置相等为条件，因此是保守的模块级复用；函数级稳定身份及位置重映射仍未完成。[Function incremental report](https://github.com/indexable-inc/blaze/blob/main/docs/reports/INCREMENTAL.md)。

Blaze自己的基准同时报告冷启动、daemon内存、磁盘和产物运行速度，并按平台区分；摘要默认Cranelift，LLVM另列。我们借鉴这一完整成本口径，不迁移其倍数到ZIL，也不把常驻内存或首次装载成本藏在编辑计时之外。[Benchmarks](https://github.com/indexable-inc/blaze/blob/main/docs/benchmarks.md)。

## 本轮落地与尚未实现的部分

CP29把有限普通标量模块的Types Builder结果保留在进程内，重放精确资源收据，失败不提交epoch，后续Analyze/Verify照常执行。改动模块整体重建；foreign/unsafe模块仍走原Builder。该边界便于先测“实际少做Types是否值得”。它不是完整CP25稳定owner架构，也没有完成跨CLI进程的函数级缓存。

CP30使用已经成立的发布图不变量，减少无active查询读取时的重复闭包检查；有active求值时保留原路径。它同样需要完整会话成本对照，局部操作变少不能代替收益证明。

下一层架构候选是稳定声明/函数owner、相对位置与迟绑定诊断、signature/layout/effect/body分别形成依赖结果面、成功事务发布，以及真实项目语料上的缓存容量与序列化成本控制。是否推进具体实现，由本轮独立候选计量中暴露的瓶颈决定。
