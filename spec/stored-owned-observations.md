# 普通拥有字段观察与存储借入

普通 `Store<Item>` 的 lending 窗口可以与独立拥有结果的构造、观察、消费和错误传播共存。trait provider 仍须满足完整名义签名；返回新的拥有 `Item` 不改变原借入元素的来源，也不转交或复制其资源责任。

普通未限定 Package 的字段读取只接受完整纯 Copy 字段，结果类型必须与实际字段相同。读取不携带新的 stored target，但必须检查真实父对象、字段和当前活动 loan；独占窗口、已移动父壳或错误字段索引不能借声明类型绕过。qualified Package 的借入字段继续使用自己的投影和来源运输，不能替换成普通 Copy 字段读取。

共享 Copy 错误臂的借入仅限本地 ErrorUnion 的 `BorrowPlaceLocal`：路径必须恰为直接 `[ErrorArm(true)]`，且 `mutable=false`；成功侧必须为通过普通拥有形状检查的 Package，错误侧必须为完整纯 Copy，结果须为指向该实际错误类型的共享地址。这不授予任意深路径、独占错误臂或 Enum 成功臂的借入资格。地址权限仍按实际当前 tag、初始化状态和源对象检查；错误臂被取走、父值被销毁或 tag 未被正确证明时，地址不得继续读取。正常与错误分支只清理实际取得的拥有责任；一个 provider 返回的新对象不使原借入元素失效，也不豁免兄弟对象的责任检查。

无输入标量入口跨入存储借入程序时，Byte 查询效应必须由实际 callee 的完整求解结果证明。空效应不能仅由函数签名、入口名称或候选摘要推断；调用次序、原描述与后来独占窗口之间的代际关系仍由各核独立重建，循环中的未来委派不能授权较早的读取。

Analyze 与 Verify 分别检查实际 Source、raw 模块和 Wire 候选。普通拥有错误组合的 Sum 工作量及 CFG 调度保留完整状态、错误前缀和最后检查；后端在 LIR 绑定、Package 字段、ByteBuffer 和 ErrorUnion 消费前预付完整实际类型。默认限额不变，精确预算必须同时保留 N、N−1 与 fresh 重试结果。

原四模块程序与最近反例位于 `tests/language-readiness/owning-trait-hosts/owned_results/stored_composition/` 及相邻的 `stored_composition.rs`。存储 CFG 中的同步拥有回调与本地共享字节视图另见[有限组合合同](stored-owned-callback-pipeline.md)；一般存储捕获、外部 ABI 或新的借入结果来源仍须各自证明。
