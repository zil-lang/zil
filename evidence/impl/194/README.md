# 194 作者证据：普通 Copy 数据与唯一 callable 环境

**作者冻结：`13df78f532f63874b61d56dc6067884dbac0fe9d`。** 本目录是有限切片作者结果，不是非作者验收或完整 1.4 终态。公开 schema/wire 仍为 24；Types 缓存、Analyze、Verify 和 LLVM 生产代码未改。

## 已实现与实际核验

直接 `T:Copy` 按当前有限普通 Copy Value 的真实结构检查，定义期不授予数值、字段或任意 trait 方法；未使用模板也检查。唯一 F 可搭配唯一 Copy T，按规范化实际 T 在 Expand 生成只剩 F 的独立声明，完整代入正文、普通签名及 F bound。正常 lowering 为每个声明分配新的 Core NodeId，因此现有 helper 实例键不需改成签名哈希。

同 ClosureId 的 i32/i64、同布局不同名义记录及透明别名都有实际源/IR/Wire验证。root 发现的有限 `Box<Box<i32>>` 误拒已经在 Copy 路径修复，活动键包含完整实例且先支付 type_key 工作；StoreElement 的旧具体检查保持原名义键、资格和计费。真正循环与无限增长分别拒绝或返回 ResourceLimit。

| 检查 | 精确结果 |
|---|---|
| 13 个限定 driver suites | 150/0，实际命令在 `runs/final-tests01/command/` |
| 新 194 源面 / raw tests | 18/0 + 8/0，已包含于上述 150 项，不重复累加 |
| 新 194 编译 / 原生记录 | 80 条编译记录、54 次 O0/O2 native |
| 四模块消费者 | 同 freeze 60 次 native，其中 26 次实际分配失败 |
| Expand 与 driver 新测试 Clippy | `-D warnings` Pass |
| 工作区格式检查 | Pass |
| 原生 Windows / 本作者 Wine | NotRun |

四模块消费者让同一个拥有 ByteBuffer 与 Ticket 的真实闭包跨两种 extra-data T 调用，同时运行 `Fn(T)->T`、不同名义值与普通 Copy 复制控制。正常输出为 A、各次成功回调字符、最后唯一 Z；每个实际 reserve 失败位置都会提前退出并结清资源。普通程序与另行链接的 allocator probe 分开记录，没有用手写 C 代替语言执行。

## 独立 Verify 与 Analyze 的职责

纯 Copy 的错误实参宽度、错误名义实例和错误结果类型，资源 Analyze 可能仍生成未信任候选；这不是类型授权。测试明确记录该观察，随后把模块哈希按实际篡改内容重绑定，要求 raw Verify 和 Wire Verify 都按 `E-VERIFY-CALL` / `E-VERIFY-TYPE` 拒绝。每个实例的 callback 合同事件被抹去时，Analyze 与独立 Verify 均拒绝。没有把前端拒绝或 Analyze 的候选生成当作独立安全认证。

预算边界来自实际新源程序：Expand work 2351、nodes 669、depth 10、instances 4；Types work 2095、Analyze work 4357、Verify work 11846。各项只调整该受测维度，N 成功、N−1 ResourceLimit；不是修改默认预算，也不是性能比较。

## 来源与保存策略

- `SOURCE.json` 保存规范/源码/CLI身份与本片变更文件 SHA；`SUMMARY.json` 保存分层计数、边界与限制。
- `final/tests/` 保存最终新增源面、raw/Wire 正反例及阶段产物；`final/consumer/` 保存最终四模块普通/失败执行、阶段材料和真实分配账目。
- `runs/` 保存实际 Cargo/Clippy/fmt 命令及完整输出。邻接 suite 的成功阶段重复件不重复打包。
- `failures/` 保留首轮三项新测试夹具失败；`observations/near-boundary01/` 保留真实 Box 嵌套误拒及原控制；Clippy 首次失败亦保留。
- `FULL_LOG_INDEX.json` 列出全部 15098 个原日志文件的 SHA、大小、打包位置或未收录原因。原日志没有删除。
- 不收录执行程序、目标对象和编译缓存。源码交付应整体排除此证据目录，证据作为独立附件收录。

旧轮次的 146/0、15/0 与初次消费者 60 次仍绑定其实际旧 source，未改标为最终冻结。后续仅证据/文档提交同样不改标实际 CLI 来源。

本片未开放双 F/双环境、单环境混合调用模式、State<T,F>、普通 Copy 范围外的 owning/Desc/Address/裸 Slice 数据类型。B0 Doing、完整语言 NotSatisfied、产品 NotRun；非作者审查待完成。
