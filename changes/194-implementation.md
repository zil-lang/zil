# 194 实施记录

作者 `/root/resume192`，主树 `.task/impl/194.md` 派工基线 `40abb53527aa45c89f6249162c9a69fea5fd80ad`。规范 SHA256 `48641079ef2ecc84aa50a0a9eafd83ae844f0d3de7924fcc8cb3e2fef3f9db5f`，normative hash `6488fa9f20121c4268f13153c6daecbfd20da5eb5e8aa86630620413d41edadb`。基线的 193 为明确候选依赖；schema/wire 24 不变。

## 首个生产保存点：开发中，未验收

按已接受提案增加有限结构 Copy bound，定义期和实际参数都核实际结构；普通 Copy 不由空 nominal trait 或 impl 制造。提取原 StoreElement 的有限 Copy 结构遍历到 `generic_copy.rs`，原 StoreElement 从同一具体入口进入，保留原资格、非零判定与预算计费。

唯一 callable F 可搭配唯一直接 Copy T，任意声明顺序。Expand 按规范化实际 T 独立特化，完整代入普通签名、F bound 和正文，保留 F 到 Types。依旧通过正常 lowering 取得独立 Core 节点；不改 Types 缓存或公有合同。定义期 nested helper 先代入 callee bound，再检查普通数据参数约束。双 F、双环境、混合模式和 owning State 加数据泛型继续关闭。

目前仅完成生产初稿与 `cargo check --offline -p zil-expand` 开发检查（`run-logs/194-author/dev-check01`，记录为 dirty 开发状态）。**尚未运行 194 源面、raw/Wire、预算、O0/O2 消费者或分配失败测试；不能作为通过或独审候选。** 接续首先复验原预检输入、同闭包不同 T/名义类型及 unused bad template，再补近反例和真实消费者。失败记录不得删除或改写。
