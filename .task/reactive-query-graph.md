# 响应式编译查询图专项

父范围：goal 的 G10.5.1 编译效率研究。源码分支：`research/reactive-query-graph`。基线：`99035931d358add0eaed6d4535035c38413701dd`。本任务不改写、合并或删除其他编译专项分支。

## 本轮交付

已核实旧 reverse-index 实现及撤回、CP25-C 和高度调度 JS 适配的范围；已实现并执行整体同步查询图模型；冻结 source blob `98762b26506c0e35ec11072b8d397c845b407eff`，3 个新进程分别通过同一组 2,730 次差分比较。研究源码、说明和精简收据已推送；完整原始样本随对话交付包提供。生产编译接入、Rust 对照和非作者独审未执行，不算生产验收。

## 当前写集与保护

本轮写集仅 `tools/research/reactive-query-graph/**`、本任务说明，以及 goal 上独立研究记录。不得修改根 Cargo/锁、阶段 schema、Analyze/Verify 或正在施工的机器后端。任务说明不进入 main，模型不默认接入产品。

## 完整接入合同

研究对象是稳定 QueryOwner + InputVersion + 语义结果分面 + 实际读取的双向 bond + 批量输入 + 按高度传播/去重 + 等值截止 + 动态依赖重绑，而不是向内容版本键再加一个索引。

生产实现需要独立认领 zil-driver/session/project、zil-db 与具体函数查询接入的写集；先核最新 main/goal 及其他作者写集再施工。使用当前函数 Types/Analyze 投影作为已有基础，统一 owner/validity 边界，禁止只删除 QueryKey.input 的哈希。源码/声明/实例/目标/规则/预算/ID映射/诊断位置、否定名称查询和 CT 选择都必须重新建立有效性证据。

语义缓存仍只提供原始候选；当前请求必须经过独立 Verify，不缓存验证权限。递归分析由 SCC 内固定点处理，外层高度队列不能用固定层数阈值代替真实环检测。

## 退出条件

1. 在真实当前源码上证明减少实际 Types/Analyze 工作量或依赖遍历，不以事件标签变化代替执行减少。
2. 单点、多点、广泛编辑、签名/布局/trait/CT/泛型/资源/递归场景，以及预算边界、位置变化、取消、错误恢复、驱逐和恢复旧源码，与全量路径逐项对拍。
3. 端到端 cold/unchanged/edit、内存、机器行为和频率明确的绝对等待收益均验收后，才讨论生产合入；保留全部负结果，不能只用最优稀疏格宣传。
4. 非作者按精确源码 SHA 复核；不能把本轮模型控制冒称产品测试。

## 恢复入口

读取本分支 README.md 与 receipt.json；复现 `node tools/research/reactive-query-graph/probe.js --large`。本轮沙盒没有 Rust/LLVM，Git clone DNS 失败；GitHub 核查和推送走连接器，实际运行仅 Node。任务停在“模型已交付，生产接入未执行”，没有后台施工。
