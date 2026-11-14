# Scalar admission / Normalize–Distribute 融合：独立精确审查

**裁决：ACCEPT（所列精确源码的语义、费用与所有权独审）。** 未确认伪造费用收据、借旧 owner 跨上下文放行、遗漏必需检查或失败发布半成品的生产阻断。新路径减少了实际执行的重复遍历，确实形成新的 node/work 边界；不能表述为旧 Capture/Distribute 的费用完全不变，更不能仅凭计数下降宣称 5105 输入已通过或性能提高。

## 身份及范围

- 仓库：`/workspace/zil-push-main`。
- base：`ad81486b976634e71bf1c61544d0fd28a783fbae`。
- 冻结生产 source：`e005777aa63d8a3031a9024eaa12af5376d6904c`，tree `0136f049e870e19d71dc0a6f987fc38d12700344`。
- base→e005 canonical patch SHA256：`29b6f93a4276412edd811e1a43b165e2451670e92b2aac9ce9ed5e25826108af`。
- 一并实读的已提交测试后继：`f7b9a6a6d37837f42fa87aa2ea5dbdd283f8cc66`，tree `ca05f1b37192afef6ed77d08e1e0a9f6ce0d8e3e`。
- e005→f7 测试补丁 SHA256：`4a326a5cc0e2e5173f75470f1caaaa99bc7cfe30edd0ae09f34087cce10923a3`。
- base→f7 完整补丁 SHA256：`65fbcb53ebaa1de228657a2105049b297187361b6aae4b2361b30828790df3c9`。

哈希格式统一为：

```sh
git diff --no-color --binary --full-index --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/ BASE SOURCE | sha256sum
```

e005 实际有 **8 文件、1156+/91-**：用户列出的五个 Expand 文件，加 `ct_artifact/scalar_materialization.rs`、driver `lib.rs`、`session.rs`。后两文件还带 native capture 工作量选择；本审查亦覆盖其真实接线和状态提交边界，没有把整个 SHA 误写成仅五文件变化。f7 仅修改 `scalar_admission_tests.rs`，24+/6-，生产与 e005 完全相同。

已读 AGENTS/goal workflow；先读 READY 工作树，冻结后核完整提交差异及后继差异。只读源码、提交元数据和哈希，未构建、未运行测试或测量，未修改 tracked/Git；本文件为唯一报告产物。不重证此前已受审的完整编译器 oracle 算法。

## 1. Capture 的免扫证据确实来自 parser owner

`ParsedSource` 私有配对了真实 parser 的 `Arc<ParseResult>`、SourceUnit、parse limits 和 AttributeContext；`same_revision` 比较实际 parsed Arc。公开 `parsed_shared()` 只能经 COW 修改另一个树，不能替换该 owner 的解析结果。字节相同但独立 reparsing 不是相同 revision。

`parsed_scalar` 仍先拒绝解析错误、deferred attributes、cfg-removed 和 type-meta 候选；资格来源只有完整 bounded scalar walk、已有成功 entry 的精确 revision，或既有严格 placement 证明。新 `AdmissionCache` 没有接收公共 AST、fingerprint 或调用者布尔值来安装 Classified 的入口。memo 只记当前强 parser owner、完成的 classifier node 需求及深度域；消费时还检查当前 source bytes 上限及 parser `fits_limits`。较紧深度不复用宽域事实；这是保守 miss，不是把旧成功升级为当前预算许可。

封闭 classifier 遍历全部 items、未用函数及两臂，类型只准无参 primitive Named 或 Group；泛型、where、contract、nominal/life/dyn、meta/emit、复杂 pattern 和 task-result 所需结构均不在域内。将原 scalar shape 条件抽到三个 helper，没有扩大这些原条件。对照 `source_context_lineage::Discovery` 的实际触发点：Meta Emit、泛型 aggregate/factory、Dyn、引用返回 worker 和相应 task_create 组合，都不能由该封闭域引入。因此此处省去 Discovery 是复用真实“没有 Discovery 触发结构”的事实，不是将 `origins=None` 当作证明或把未知结构按纯 scalar 放行。

`capture_original_parsed` 仍用当前 Budget 做 module 绑定，核 source/module、有效 module span、唯一 file/module ID；绑定失败进入真实 raw capture，并传已经减去初步绑定工作的 remaining，未重置余额。缺少任何 owner fact、上下文非 Original、空输入或较紧限制，则直接走原 `capture_original`；rich/CT 输入没有靠 Unsupported 作为成功出口。

原 namespace 的 key 拷贝/格式化前，对 Closed aliases 或 Explicit package/module paths 的实际字符串、行逐项支付 metadata 工作。该新 key 是 admission 环境输入；它不是 nominal lineage、导入可见性或 CT 权限。路径只返回 None origins，未伪造 declaration rows。

## 2. 消费是单次、完整 owner 向量和上下文绑定

`State.project` 保留完整且有序的 ParsedSource 向量与原上下文 key。`take_project` 先 take 后比较，核完整长度、逐 ordinal 的 exact revision 和完整 context key；不匹配尝试也消费掉旧 admission。原 Closed→同 root/aliases 的 Closed-Unavailable 只表示 no-rewrite materializer 的保守 marker，可据此融合 scalar 生产，不能恢复不可用的 nominal lineage。Explicit 的 package/modules、Closed 的 root/aliases 改变不能匹配旧 key。

`expand_parsed` 的外层不论 Some/None/Err 都执行 `finish_request`，清 module facts/project；checkpoint clone 有独立 Mutex/State，互不消费对方的 admission，不共享一个可变授权槽。retained owners 有 source allowance 下的保守保留上限；清理/逐 module 替换不把另一个解析版本与旧 fact 配对。

环境新增 `fused_distribution`，并同时进入完整兼容、function-input 及 namespace 投影兼容。故 fused 与公开 unfused 调用不能共享不同费用合同的 Normalize/Restore/Lower 等旧产物、receipt 或库存。较紧 node/work 不被当成 exact domain 变化强制失效，但必须逐 receipt 检查余额；其余 source/token/depth/target 等域仍完整绑定。跨 target、import、header、attribute/位置变化也没有被这个新单 bit 盖过。

## 3. 融合不是删掉唯一的 formation 检查

核对真实路径：`modules::normalize` 首先执行/合法 replay 当前完整 Preflight，包括所有原 items、attributes、params、字面量和类型，以及 imports 的 node/text；其后才构造或复用绑定当前 imports/declarations 的 Index，再进入 `normalize_scalar`。

Normalize 只在此闭域中做原有 symbol/type canonicalization，或将 `Member(Name,Name)` 折为一个 Name；它不会产生新类型构造子或增加结构深度。替换的 canonical 字符串仍由 `symbol` / `declaration_name` 执行当前 `budget.text`，而非因省去后访就不检查新字符串。原 literals、attributes、参数和其他不变叶子的 formation 已由 Preflight 完整验证；不是只核一个顶层函数标签。

新增 `scalar_file` 在真实 Normalize 输出访问中核 item/type/pattern/block/statement/expression 的源 file 与封闭 shape。`expr_name` 快返回也改为经过输出检查；不能因为表达式被替换就绕过这道边界。普通非 scalar visitor 的字段是 None，原 rich/Restore 行为未改变。

原 Distribute 的完整 `budget.item(normalized)` 在 fused 路径不再执行，也不生成或 replay 它的虚拟 receipt；只在当前 file→module placement 中分派产物并增加明确的 `fused_distributed_items` 统计。公开 parsed API 没有 capture admission 时，原 Distribute visitor 和费用完整保留。因 actual repeated traversal 已消失，省掉相应旧费用成立；不能反称这部分工作仍执行了、却只换了一个计费名。

Deferred 不是公共可构造的 normalized AST：recipe 绑定当前 parser item 的 ordinal/span/text/attribute context 与完整 normalization environment。需要实体时仍在 replay Normalize 前保存的 allowance 下，用当前 Index 重建真实 Item，执行同一输出 shape/file 检查，并要求重建 node/work 与本次 Normalize receipt 相符。无法满足时返回错误，不用 Restore 后的树冒充 Normalize 输入，也不免掉未适用 receipt 的真实 Normalize。

## 4. 实际账本、首错和失败 warm 路径

本次允许减少的是封闭路径不再执行的 Discovery 和 Distribute demand；**默认 Limits 定义没有变化**。Capture 仍支付 module/namespace 绑定；outer Expand 仍支付 Preflight、实际 Normalize 和 Restore；Probe/Lifetime/header 仍用既有独立 plain ledger，未回写或借用 outer remainder；Lower 和 SourceMap 的需求也未取消。

materializer hook 将 `capture_remaining` 交给当前 `Environment::new_scoped`，其实际 scope、definition classification、materializer walk、usage 检查仍执行。返回 usage 仍加上真实 Capture node/work 差额；不是把被省掉的 Discovery 旧费用填进用量。只有无 runtime defer、preview、required import、self-constant/lineage rewrite 等实际效果时，才发布 unchanged materialization。不是读到 classifier true 就直接宣告 CT 已完成。

`Reuse::replay_inner` 对 node/work 先完整比余额；不够时 false 且不改 Budget，随后重入原 visitor，所以失败阶段/位置从当前完整实际顺序产生。新的 N/N−1 合同应比较 **同一 fused 真实入口的 fresh/warm**，不能要求它继续消耗旧 unfused 已被删掉的额外遍历费用。

Expand 的 entries、inventory、environment 和 epoch 暂存在局部；全部生产/Lower 完成后才写回成功状态。None/Err 不提交部分新 epoch。driver 仍有 producer/materializer checkpoint 及原失败回滚。rich/不满足 scalar materializer 条件时仍有完整 `materialize_scoped`，最终 producer 返回 None 时也进入真实 `expand_program_with_limits_target_context` / runtime CT 入口，而非把 None 转成空程序或成功。旧成功仅供合法原始输入与费用复用，不作为失败尝试的输出权限。

## 5. import、位置和 Map 的当前性

当前 imports 是真实 inventory/环境输入，不属于 Capture 的“无 CT 结构”事实。import 目标、alias、顺序等变化不会沿 old environment 复用旧输出。位置转移仍使用既有完整 placement/attribute/context 证明，imports 的当前 span 也要匹配其转移结果；新 admission 没有赋予任意重标旧 Map 的权限。

冷/重新 Lower 的 `finish_lowering` 保留完整 SourceMap.validate_iter；placement 搬迁的 retained Lower 仍经 `lower_for_sources` 对当前所有 sources 执行 validate_iter，失败返回真实错误。原位 exact owner 下的已验证私有 Map 复用不应误报成每次重新走全图；本差异没有改这条已受审边界。

特别核对 import99 反例：Expand 确实可以保留 unresolved 普通 import/value call；`zil-types/src/program.rs::imports_for` 才按实际 signatures 表拒绝缺失 module，报 E-MODULE-NOT-FOUND。不能把 Expand 成功本身当作没有检查的实现漏洞。

f7 test-only 修复因此合理，并且没有只把 expect(Err) 换成 expect(Ok) 就结束：它要求当前 environment 保存 changed imports，全部模块重 Lower / reused Lower=0，产物保留 import99，与旧完整输出不等且 core Arc 不同；再比较 fresh fused 与 raw 的完整输出，逐 Map 验证当前源，重复坏 import 请求，最后恢复原 import 并要求回到原完整结果。该测试不冒称亲自运行了 Types 错误 oracle；上述 Types 拒绝边界由实际消费者源码核定。

## 6. 同 SHA 的 native capture 选择

driver 新选择器只在 push_reuse + native-products feature + LLVM API O2 下，从**当前 LIR 的函数/block 长度**计算平均 instructions≥64；不遍历/hash 旧 body，不读取旧验证结果，也不返回 capability。加法/乘法溢出或无定义函数时禁用 capture。阈值是 optional capture 策略，没有任何已测“最优”保证。

选择发生在当前 verified/LIR 已取得后的 BuildOptions 构造；只改变 `native_products`，当前 Verify、LIR binding、machine preflight、LLVM 检查及发布事务均沿原入口。完整 final-product Configuration 包含该 BuildOptions 字段，因此 heavy→cheap 切换不能因旧 final product exact hit 而跳过首次 cheap 的 native-history 退役；后端成功提交 native_products(false) 清历史。driver 仍在 staged backend 上构建，取消、dump/目标校核或 publication 失败时不会写回 staged session。

新增边界/overflow 与 heavy→cheap→heavy 测试断言有实际质量含义：cheap 无 retained native epoch/payload，后续 heavy 不借 intervening cheap 之前的 history，执行输出与 fresh 及当前 verify 比较，非法 source 不能进入 selection 并保留旧 backend。此处只审其源码，未执行这两项测试。

## 新测试覆盖与未测边界

六项新增 Expand 测试的实读覆盖：cold/warm/raw 完整 Core+Map 相等；真实分类节点免重复扫描；fused 无 Distribute receipt；无 admission 返回旧公开路径；缺 fact、独立相等 reparsing、COW/raw AST、深度变化；rich/generic/未用分支 meta/task 的真实 raw discovery 回退；owner/context 改变与 admission 单次消费；node/work 的 N、N−1、fresh 首诊断完整相等、失败 epoch 不变及恢复；位置/current Map/import 变化与恢复。

owner/COW 测试中的 forged Arc 是只读 ParsedSource 的外部 COW 控制，不能作为能够注入 parser-owned AST 的声称；Map/import 后继的完整断言已如上单独核定。测试未逐一动态执行所有 context/target/shape 组合，静态拒绝边界不冒充这些组合的实测计数。

root 报告首次 Expand 为 121 pass/1 fail，失败是原 import99 错把拒绝阶段放在 Expand；原失败应保留。本人没有执行任何测试，也没有把 f7 后继的最终门记为通过。8 文件差异及测试后继均没有修改/缩短 5105 原输入、默认限额或 Verify 实现；这仅支持“源码没有裁输入”，不证明该原输入当前预算可过。5105 完整输入的 default-limit、完整 Verify 及原生/oracle 结果仍由 root 的实际统一门裁决，不得用 425 窗口或局部测试替代。

**需要修复的确认生产项：无。f7 测试合同修复：接受并可复用 e005 全部生产裁决。剩余：精确后继的实际门与完整 5105 原输入结果，未作性能、规模或 main 合入结论。**
