# 194 实施记录

作者 `/root/resume192`，主树 `.task/impl/194.md` 派工基线 `40abb53527aa45c89f6249162c9a69fea5fd80ad`。规范 SHA256 `48641079ef2ecc84aa50a0a9eafd83ae844f0d3de7924fcc8cb3e2fef3f9db5f`，normative hash `6488fa9f20121c4268f13153c6daecbfd20da5eb5e8aa86630620413d41edadb`。基线的 193 为明确候选依赖；schema/wire 24 不变。

## 首个生产保存点：开发中，未验收

按已接受提案增加有限结构 Copy bound，定义期和实际参数都核实际结构；普通 Copy 不由空 nominal trait 或 impl 制造。提取原 StoreElement 的有限 Copy 结构遍历到 `generic_copy.rs`，原 StoreElement 从同一具体入口进入，保留原资格、非零判定与预算计费。

唯一 callable F 可搭配唯一直接 Copy T，任意声明顺序。Expand 按规范化实际 T 独立特化，完整代入普通签名、F bound 和正文，保留 F 到 Types。依旧通过正常 lowering 取得独立 Core 节点；不改 Types 缓存或公有合同。定义期 nested helper 先代入 callee bound，再检查普通数据参数约束。双 F、双环境、混合模式和 owning State 加数据泛型继续关闭。

目前仅完成生产初稿与 `cargo check --offline -p zil-expand` 开发检查（`run-logs/194-author/dev-check01`，记录为 dirty 开发状态）。**尚未运行 194 源面、raw/Wire、预算、O0/O2 消费者或分配失败测试；不能作为通过或独审候选。** 接续首先复验原预检输入、同闭包不同 T/名义类型及 unused bad template，再补近反例和真实消费者。失败记录不得删除或改写。

## 源面与消费者进展、独审边界修复

上述保存点后的实际记录：`source01` 12/3 的三项为新夹具中枚举单位模式漏 `()` 及两个诊断文字断言不符，原失败保留，修复不改变生产或原审计 map 输入。clean `d8f62753` 的 `source02` 15/0，新 suite 有 44 次 O0/O2 原生执行、67 条编译记录。clean `a3c0e73` 的 `raw01` 7/0，随后 `adjacent01` 13 个限定 driver suites 146/0，覆盖 103/121/178/188/189/190/193/194。raw 错宽度/名义实参由独立 Verify 的实际类型检查拒绝；资源 Analyze 对这类纯 Copy 类型错误可能仍生成未信任候选，其结果明确另记，不把它说成类型认证。合同事件篡改分别由 Analyze 与 Verify 拒绝。

clean `4e719ced` 的四模块消费者 `consumer01` O0/O2 60 次 native，含 26 次实际分配失败，输出、实际分配/释放、零 live 与峰值均符合预期；不是性能或 Windows 测量。以上结果属于其实际提交，不自动改标后续修复冻结。

root 新增的有限 `Box<Box<i32>>` 边界在 `near-boundary01` 得到实证：无 Copy 的普通 identity 控制到 Verify；相同输入加入 Copy identity、duplicate 或单 F extra 参数后均在 Expand 误拒。根因是从 Store 复用的活动集合仅按名义名 Box 判断回访，把有限的不同实际类型实例误当递归。现仅 Copy profile 改为先支付 `type_key` 工作再使用完整实际实例键；Store concrete profile 继续使用原名义名键、原资格、原预算，单独加入不变控制。新测试覆盖有限嵌套、真正同实例递归及无限增长的工作/深度拒绝。

同轮检查 direct closure 作普通 Copy T 为 SafeReject/Expand、opaque closure 为 Unsupported/Expand，均未被当作无环境代码指针；原输入及报告保留，并加入显式 fn 指针正例。此处修复仍待新 clean freeze 的源/raw/消费者/Clippy 重验和非作者审查。
