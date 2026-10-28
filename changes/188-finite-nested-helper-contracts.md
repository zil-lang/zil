# 188 有限嵌套同步 helper 合同

## 先行提案与范围

本提案先于生产修改。权威工单为主线 `.task/impl/188.md`；准备基线 `91feaa919bf3f02a36b50bf5e13cbddec4ede87b` 组合明确的 182/183/185、184 `b1461dc` 与 186 `0fb8aaae` 候选，准备合流不表示各片已获主线最终验收。设计 SHA256 `8df5f5bb3cb25fdea8670124b09941713529c859ab18864d0fc87090864786d5`，规范集 SHA256 `1068cda3fe79b906f49eeee4bd3f62863f9db4ae85e8a0dbaa3c439d310777b8`。

1.4 §5.3、§9.3、§9.7 要求高阶合同从真实调用组合，未知或隐藏调用不能变成空效果。实际最小输入 `inner<F:Fn(i32)->i32>(f:&F,x:i32)=>f(x)` 再由 `outer` 调用 `inner(f,x)`，当前 Expand 明确报告嵌套同步 helper 未实现。184 的泛型 State 调用另一个泛型借入 helper 也有同类真实失败。

本片支持同一实际环境参数、同一 bound kind 的有限无环 helper 组合：Fn/FnMut 仍按已有借入权限，FnOnce 仍最多一次消费。保留零调用、普通分支、提前返回与 Trap 前缀，组合可以跨多层与模块。对真实递归 SCC、helper CFG 循环、混合调用 kind、未知或混合环境来源、外部借入逃逸、多 callback 参数继续明确拒绝。Address 显式回调实参和循环合同另片，不借本片放开。

## 公共合同不增加权威

既有 `CallableParameterContract` 的参数索引、具体 ClosureId、Hold/Call/writes/Cleanup 与 `CallableTrace { calls, exit }` 已能表达本片有限序列。保留 schema 23 和现有 lending 来源字段，预计不加公开字段、wire opcode 或后端 ABI。

callee 的参数类型或同一 ClosureId 不能证明传入的是当前 callback 实例。每一层都从 Load/Take/Borrow/Reborrow/Store 的实际来源重建环境是当前参数还是本地 factory/未知混合来源。只有明确的同一参数来源能贡献父合同。已核 callee 的 Return trace 接父指令后缀，Trap trace 立即形成父的终止路径；不能把所有子路径当成返回，也不能漏掉零调用出口。

Types 按已有实例化栈拒绝真实递归，再从实际具体 helper 的已生成合同组合生产候选。Analyze 与 Verify 各自从真实 Call 及参数类型构建有界依赖 DAG，callee 先行，先独立重建并核对子合同，再计算父合同；Verify 不调用前两阶段求证。公共声明即使有正确指纹，也必须与真实 CFG 逐项相符。

单一 kind 的内部计数只用于精确有限序列的压缩；对外仍保存完整 calls 顺序。不同 kind 不偷偷归一化成一个计数。所有依赖边、origin 表、路径/前缀集合、跨路径组合、克隆和公开序列物化先按实际工作收费，使用原默认额度；超限返回 ResourceLimit，不截断合同或发射目标文件。

## 写集与检验

生产限工单列出的 Expand `generic_check.rs` / `generic_callable.rs`，Types `helpers.rs` / 必要 `closures.rs` 状态，以及 Analyze/Verify 各自 `helpers.rs`。保留当前调用实参与捕获效果、lending 贷款、父壳生命周期、FnOnce 未调用清理等现有检查。确需 lib 入口接线时先报告实际阻断再请求 root 补写集，不改公共登记。

验证从原最小反例推进一层与多层、Fn/FnMut/FnOnce、分支的 0/1/多次调用、正常/错误/Trap 提前出口和未调用清理。至少一个真实跨模块 State/动态存储消费者从 ZIL 源生成并运行 O0/O2；源反例保留递归、循环、错误权限、消费后复用、不同实例及当前调用别名冲突。raw/Wire 从合法候选改调用图、参数来源、子/父 traces 或出口，不能重跑 Analyze 生成伪权限。准备基线、首次失败、最终冻结和非作者独审分别绑定精确源码，不把有限片声明成完整 1.4 或 ZUI 产品完成。

## 实际接线

root 已授权 `generics.rs` 和独立私有 `generic_helper_graph.rs`。每个泛型正文仍走原有的一次符号检查，在词法解析确认的 helper 调用处记录节点和边，统一检查结束后运行一次有界拓扑遍历；未实例化的自递归和互递归同样拒绝。名字查找、字符串复制、节点、边、前沿和表容量均进入原 Expand 预算，没有提高默认额度。

Types 仅组合已经完成实例化的 callee；借入传递保留真实 Load/Borrow/Reborrow/Store 来源。Analyze 和 Verify 各有私有调用图与先子后父次序，各自重建 callee 正文，核对后才能将该 callee 的完整有限 trace 用于父层。不同实际实例即使共用 ClosureId 也不能贡献当前参数的调用轨迹。Return 子路径继续父后缀，Trap 子路径立即记录终止前缀；公共表示仍是完整 calls 向量。

新增来源和 trace 处理不改变责任核。零调用 FnOnce 子函数仍接收并结清被移交的环境；将同一已移交值再次传给另一个零调用子函数，不能靠空 trace 逃过资源检查。嵌套 helper 返回的 lending 值仍带真实环境 Hold，下一次冲突可变调用继续被贷款核拒绝。普通 owning State 的既有循环不是有限 callback helper 的循环合同，不外推为后者已实现。

本片生产写集最终只使用 Expand 三文件与 Types/Analyze/Verify 各自的 helpers.rs；未修改 captures、资源求解、公共 schema、Wire、运行库或 LLVM。187 的局部 ByteBuffer 捕获效果与 189 的显式 Address 回调实参是独立候选，需后续组合验证，不能由本片的通过结论代替。
