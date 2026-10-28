# 203 Expand：独立 State 绑定和双声明 StoreElement

设计 `41ceb53bf8c7f83070972fd6a66669c251e7dbf3b4244f7c18ad1f9507c0866c`；规范 `1355ae0d2351199391d65ca4faa4a84ce768ece5c8aaaca465b8de5b20aad65a`；公共骨架 `ffb52a4c3b8004a6347891067eff9cae157ce924`。

## 实现

- 私有 callable classifier 将真实 direct 参数 roles 与一个拥有 State 的字段 bindings 分开。Core 只发出一条 State owner 记录、两条按声明 generic_index 排序且由实际完整 Package 字段重建的 selectors；不制造两个运行时参数或 noescape 联合轨迹。
- 新双声明拥有定义的两个结果独立要求有限 Copy，旧单 F/lending 路径保持。字段调用先解析真实字段类型，再选其 F/G 声明模式；重复 FnOnce、交换 F/G 模式、借入 FnOnce 沿原责任门拒绝。
- 普通完整 T 保留，family 键遮去 aggregate 的实际 callable 参数槽；函数泛型声明顺序不会改变 family。`Item<F>` 出现在双根作用域仍使用旧单根 family 名。Core selectors 连接声明顺序与真实字段顺序，具体双 Closure 向量由 Types 按固定字段顺序一次替换。
- StoreElement 在新 F/G 作用域按两个声明分别认可最多两个直接 callback 字段；不要求将来的实际 ClosureId 相等。普通 `identity<T:StoreElement>` 仍保留完整 T/TypeApply，最终布局与实际环境责任由 Types/Analyze/Verify 独立检查。
- 原 04 marker 源的显式 unsafe 增量见 `203-expand-unsafe-markers.md`。签名来自独立借入 Extern 索引；不作为普通 helper 或环境擦除入口。

## 已执行的作者限定检查

`before01` 在骨架上真实 0/4（旧 escaping callback 门）；`check01` 有一次内部 StoreCallbacks 类型迁移编译失败；`after01` 旧 202 17/0、新源 3/1（原 04 的 Unsafe 旧门）；均保留。

`after04` 在骨架加作者工作改动上真实执行新 203 **13/0**、旧 202 **17/0**。旧精确预算 1731、2255、8399、12755、12291 均保持；新双 State 原 01 的 N 为 **3753**，原 04 marker 为 **4120**，均实际 N/N−1/新鲜 N。此组包含原四份字节、unsafe 与 unused 邻居、真实外部实参与结果不匹配、unsafe 内消费和词法隔离、字段 selectors/倒序绑定、完整 T identity、两参数伪造/三 callback/嵌套 callback 拒绝、双结果 lending 拒绝、单根 child family 兼容。

这些是 Expand 层结果，生产冻结后的限定复跑另有精确 commit 收据。Types、Verify、机器执行、故障注入和实际 `MPBQZA` 输出尚不能由此宣称通过；旧源预检 NotRun 历史不改。
