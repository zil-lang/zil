# 223：完整 Product 作为动态 Store 元素

作者实现和限定验证绑定 clean `7061edabbb620840cf26e0fbd343efef1fde405a`。本片关闭 217 的 `product-store-element` 原始探针及 206 的未用 `Store<Root>` 声明边界；独立审核、223+224 共享窗口组合和整仓验收由 root 另行记录。本报告不是这些后续验收的 Pass。

主树 `_zil_design.md` §8.2.4、`changes/223-product-store.md` 和 `.task/impl/223.md` 是施工合同。作者分树按派工从 `a8e8831a` 建立，未把主树尚未提交的规范变更伪写成受测源码提交的一部分。收据分别保存分树实际规范摘要 `de431dc8…` 与施工时主树规范摘要 `58e6c031…`；规范和工单快照随证据保存。后续集成须绑定 root 实际提交的规范身份。

## 有限完整合同

Store 元素现在可以是 §8.1.1 的普通完整有限 Product：递归名义 Package child、有限 Copy 伴随字段、直接 Managed ByteBuffer 或有限 Copy 头的用户资源叶。每层 Package 都必须拥有传递的 manages 叶，根实际内联布局必须非零。仅根含 Copy 存储时，零字节的拥有 child 仍有真实销毁责任。

```zil
resource Ticket { code:i32 }
package Leaf { manages ticket:Ticket; value:i32 }
package Root { child:Leaf; tag:i32 }

fn main()->void!i32 {
    let items:Store<Root>=store_new()
    if !store_reserve(items,1) { return () }
    store_push(items,Root{child:Leaf{ticket:Ticket{code:65},value:7},tag:9})
    let value=store_pop(items)?
    return value.tag-9
}
```

只有完整元素可以交给 push。成功后原值不再可用；失败把同一完整值留在 `Product!Unit` 的错误分支。pop 的 `Unit!Product` 在成功时先缩短初始化前缀，再交出原尾部的完整责任；空 Store 只产生 Unit 错误。reserve 失败保留原指针、容量和初始化前缀，负值与溢出容量也不触碰其中元素。

为贯通既有 push/pop 协议，只增加精确的 `Product!Unit` 和 `Unit!Product` 壳。它们使用原错误联合的实际 tag 与唯一 active payload 规则。任意 `E!Product`、`Product!i32` 和嵌套 owning Sum 没有随之开放。

普通 `T:StoreElement` 实例化也检查完整实际 Product 树，因而非 Store primitive 的普通泛型 identity/helper 可以保留完整 T。定义展开、Types、Analyze 和 Verify 各自处理自己的实际输入；语法资格不是安全证明。

## 安全核与机器表示

Types 的 Product Store 入口显式复用普通有限 Product 的结构资格，并单独要求非零存储。旧 direct-Managed 和 callback Store 路径仍保持。捕获 Store 的旧叶 profile 没有改成新 Product profile，`store_at_mut` 对 nested Product 仍明确 Unsupported。

Analyze 在原有原始类型深度和尺寸预检完成后，独立检查递归字段身份、传递拥有叶、非零存储和实际 canonical Store resource 定义。实际定义必须绑定完整相同元素且没有用户 teardown；只改 payload、不改 canonical 名称的旧 raw mutation 会被 `E-ANALYZE-IR` 拒绝。只有顶层 sealed Store 定义把其 element 当作普通值；把 Store 藏进用户资源头不会消除该头的有限 Copy 约束。

Verify 从 raw TypedIR/Wire 的实际元素、资源表和布局重建完整资格，不调用 Types/Analyze 求证。它独立拒绝无拥有叶的 child、重复字段名、Product 内 Store、完整实际布局为零、canonical 定义不一致和重复 push 同一已消费 SSA。测试重算 module fingerprint 后分别走 raw 与 Wire 入口，断言实际规则诊断，而非依靠旧 fingerprint 不匹配拒绝。

Lower 与后端仅贯通两种确切 Unit 协议。Store 元素仍内联在原初始化前缀；没有新增隐式 Box、RC、锁或全局表。后端清理从每层字段声明序构造真实完整 leaf path，使用完整布局生成 GEP 并调用相应 teardown。普通叶、ByteBuffer、零字节用户资源和原 callback 元素的清理都沿其实际资源定义进行；Store 中只存完整元素，不引入每个 nested child 的部分状态。

共享 Store 元素窗口需要 224 的独立普通 Product 地址和 nested AddressField 支持。本作者分树没有私自编辑 224 的 Types/Analyze/Verify 地址入口，也没有把 Store 元素谓词当作普通借入权限。组合验收应覆盖 `store_at(s,0).child.owner.n`、`&window.child`、最后使用后的 reserve/pop，以及活跃窗口期间的结构操作拒绝。

## 精确基线与预算

基线 CLI 从实际 `a8e8831a9e28ac2ec83be6eac740a8f04a00af7d` 重新构建，SHA256 为 `63310ba14e42499b1a66045c447b652a4ba3c5fb497a1b3685c1f2a3c90a4dda`。217 原始 Product Store 源和 206 `23_store_parent.zil` 原字节都曾在 Types 以 Unsupported 拒绝；7061 对相同输入执行一次独立 Verify 后成功。206 `25_sum_parent.zil` 原字节仍 Unsupported。旧冻结 fixture MANIFEST 未改。

新增预算测试固定源 SHA256 为 `d82f945c15d4117b460395b3ecba8df8c34b51e8fe25b4d6f3e63a71cf8999ed`。每阶段独立二分实际最小额度，再执行 N 成功、N−1 ResourceLimit、fresh N 成功：

| 固定输入 | Types | Analyze | Verify |
|---|---:|---:|---:|
| 新 Product Store | 731 | 11820 | 17398 |
| 206 原 old-flat 控制 | 418 | 8673 | 本次该控制未测 |

206 既有 mixed、whole、wrappers 的 Types/Analyze 精确值仍由原测试通过。新增树遍历、字段表和 canonical lookup 明确收费，默认额度、schema/Wire 27 和产品门未改。逻辑配额是有界工作的记录，不是编译速度比较。

## 作者实测

`evidence/impl/223/author/RESULT.json` 及同目录 `raw-evidence.tar.gz` 保存准确命令、源码和编译器身份、阶段产物、raw/Wire 候选、native 输出、预算与原始失败。源和编译器均绑定 clean 7061；CLI SHA256 为 `e82924ae16901416019b93f8416a004e09629c42a4ba58c9268bfec156b95412`。工具链为 Rust/Cargo 1.99.1 与 LLVM/Clang 23.1.4。

限定作者测试 **49 通过、0 失败、0 忽略**：新 `impl223_product_store` 12 项与 `impl223_budget` 1 项，旧 `impl172_owned_store` 18 项、`impl201_source` 16 项、`impl206_budget` 2 项。新测试内含 16 次实际 O0/O2 原生执行；作者命令显式启用 `llvm-api`，构建结果断言该 backend。无 feature 的 reference profile 显式选择原 Clang 路径，结果不会冒称 API builder。

六个修改生产 crate 的 lib Clippy 和两个新 driver 测试目标 Clippy 均 `-D warnings` 通过；格式与 diff 检查通过。作者未运行正式全仓门，也没有把这些与独审或后续组合测试数量相加。

### 三模块、四个叶缓冲区与逐分配点失败

`examples/mainline/product_store223` 的 model 构造拥有 ByteBuffer 与 Marker 的 Leaf，两份 Leaf 组成 Pair，再组成 Root。storage 通过普通函数创建 Store 并返回 pop 的 Product；main 根据实际 stdin 选择保留一个元素、保留两个元素、弹出尾部或扩容。同一个 Store 中的四个用户 Marker 和四份 ByteBuffer 都有独立责任。

`tools/language/223/consumer.py` 在编译和执行前固定 source-derived oracle：Linux x86-64 自然布局 Leaf=32、Pair=72、Root=80 字节；容量 2/4 的 Store 实际请求为 160/320 字节。缓冲区请求为 2、3、4、5 字节。普通程序用真实 LLVM API builder；分配探针只给同一已核 LLVM IR 链接既有 realloc/free observer。C shim 仅记录和注入真实分配失败，没有实现语言或代写清理。

| 实测项 | 结果 |
|---|---:|
| 实际源码模块 | 3 |
| LLVM API 构建 | O0、O2 各 1 |
| 原生执行 | 74 |
| 普通可执行文件执行 | 12 |
| 分配观察/故障探针执行 | 62 |
| 实际分配失败 | 38 |
| 最终 live | 全部 0 |
| 最大峰值请求 payload | 334 字节 |

每个真实分配点和最后一个分配点之后均有控制，另有非法输入和 EOF。保留前缀的清理为 `ABCD`，弹出尾部后为 `CDAB`；创建叶失败依次得到 `A`、`BA`、`CAB`、`DCAB`，首次 Store 分配失败无 marker，最后 Store 扩容失败仍按 `ABCD` 清理。alloc/frees/live/peak 的全部字段与预先 oracle 一致。334 字节是这个有限消费者的请求 payload 峰值，不是 RSS 或相对 Rust 的成本结论。

## 原失败与待迁移的旧断言

首次新测试有 7/8 通过，普通 `T:StoreElement` helper 仍被 Expand 的旧直接 manages 资格拒绝。实现补全其递归名义 Product 实例资格后，原源通过；没有通过删除 Types/Verify 守卫掩盖问题。

一次 raw 零布局测试只改资源表，却期待零布局诊断，测试自身先被 canonical 一致性规则截获；现在该 mutant 协调所有实际类型出现，使其只能按真正的零布局拒绝。另一次显式 API 测试命令未启用 Cargo feature，Codegen 明确报 `E-LLVM-API-UNAVAILABLE`；最终命令显式 `--features llvm-api` 并断言实际 backend。原失败日志保留。

两处旧 206 当前测试断言需要 root 登记后的精确迁移：`source_23_store_parent` 应接受原字节；`impl206_analyze` 的 `label=store` 仍必须拒绝其错误 canonical 定义，但结果从 Unsupported 改为 SafeReject / `E-ANALYZE-IR`。作者额外跑旧 Analyze 组实际得到 6/7，通过部分未算入上述 49，原失败日志保留；没有改变旧 fixture 或冻结 MANIFEST。后续全仓验收不能忽略这项已知迁移。

## 仍未开放及 Rust 四目标

Product 内 Store、递归 Store、Product 树中的 Closure/异构 dyn、拥有数组、任意 owning Sum、拥有捕获 Store<Product>、Product 独占窗口、长期借入、挂起/取消和并发继续保留各自未完成边界。217 的 `product-owning-store` 与 `nested-store-element` 不随此片完成。未开发完整 ZUI 框架。

| 目标 | 本片实证与限制 |
|---|---|
| Rust 同类安全 | 完整转交、同型不同叶、失败 active tag、递归清理及 raw/Wire 伪造拒绝有实测；仅覆盖有限同步 profile，完整语言安全未证明。 |
| 运行与内存同级 | 真实内联布局和每个分配点失败平衡有机器证据，无新增隐式 Box/RC/锁；未做同语义 Rust 运行时间、RSS 或尾延迟比较。 |
| 表达与维护更简单 | child-only Product 可以进入动态 Store、通过普通完整 T helper 和跨模块 API 流动；完整框架维护成本尚未测量。 |
| 编译成本更低 | 旧具名预算保持，新实际遍历有 N/N−1/fresh 记录，默认额度未升；没有本片编译性能胜出结论。 |
| 至少一项实质突破 | 原 217 缺口原字节晋升到完整原生清理纵切；产品四目标合取与产品级可重复净突破仍未建立。 |

完整语言 `NotSatisfied`、B0 `Doing`、产品和原生 Windows `NotRun` 保持。独审和最终整仓裁决须另行绑定精确提交。
