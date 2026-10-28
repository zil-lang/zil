# 147 — 有限、本地、名义 trait 实现

状态：工程候选，独审未完成；不变更规范、wire18、内存模型或产品门。
基线：c1ec559d313c2bbee25f4b68a28da402c8a367c1。

动机：架构对比指出，不能用更多 ByteBuffer 个案替代一般语言组合。先落地已解析语法的有限子集，不发明万能 Store 或新的运行时。

接受性变化：同一模块内、无泛型的用户 trait 声明和本地具体普通 struct 的名义 impl，从 Unsupported 变为可验证的普通方法。必须完整实现声明的方法，接收者/形参类型/传递模式/返回类型逐项一致。方法体经现有 Core→Types→Analyze→独立 Verify→LLVM；声明一致性检查不授予 Copy/Transfer/Share 或任何安全权限。

不支持：泛型约束、关联类型、默认方法、supertrait、动态派发、跨模块 trait 解析、资源/Package 目标、冲突方法名的限定解析。遇到这些情况保留 Unsupported，不静默丢弃约束。

已知 receiver_regression 的 r005_03 把所有 trait impl 写死为 Unsupported；这一断言恰为本切片有意扩张的边界，改为相同源文本成功并保留全路径检查。原词法诊断四例、预算优先级和安全拒绝期望不变；新增泛型 trait 的 Unsupported 测试。不是把失败的安全测试改成通过。

无新增 ABI/layout/opcode、默认额度、隐式分配、RC、锁或 runtime trait 表。自测不算独立审核，不进入生产 main。
