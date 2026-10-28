# 203 后续只读审计：有限拥有 Store 捕获

本记录是下一切片建议及当前实际预检，不修改规范或生产准入，不将下一切片记为已派工或完成。203 最终消费者另等包含移动账本修复的冻结 CLI；本文 CLI03 不用于其终验。

## 当前源面结论

建议紧接 203 推进**显式 move 捕获有限 owned Store，以及显式 move 捕获含这类 Store 的完整 Package**。现有 174 的 Package 拥有 Store、201 的 Store 保存回调元素、202 的完整 T 普通模板，以及 203 的双 F/G 声明模式是不同的组合边界，不能替代捕获环境保存容器的能力。

四个源均实际交给同一 CLI03 的 `check --stop-after verify --emit-dir ... --json`：

| 原始输入 | 实际结果 | 到达阶段与含义 |
| --- | --- | --- |
| `00_package_owned_control.zil` | Success，Verify 1 次 | 同样构造 Bag/Store，直接返回完整 Bag；既有拥有组合仍工作 |
| `01_direct_store_capture.zil` | Unsupported | Types：`Store capture requires a separate retained storage profile` |
| `02_original201_package_store_capture.zil` | Unsupported | Types：`Package capture with Store fields requires a retained storage protocol` |
| `03_mutating_store_capture.zil` | Unsupported | Types：`Store capture requires a separate retained storage profile`；FnMut 正文包含真实 reserve/push |

所有源的 Native 均为 **NotRun**。后三个 Verify 均为 0；不能将其计成独立安全核或机器层反例通过。第二个 Package 源逐字保留旧 `impl201_analyze_store.rs::PACKAGE_STORE_CAPTURE`，SHA256 为 `5d430f79813e995268368bcbe3c3918e1de093007e5802f79d4f88f196905853`。其他源和实际命令、状态、诊断完整列在小收据及分离证据中。

CLI 来源 clean `80c21b072b6abce47f9ae4adb2f440631eab738a`，SHA256 `6b9434e8bcf17e7010bfff98ab9cba82b2ab22caa4344f2f3bd939a6f9f95a71`。已核原 build receipt 的 clean/Pass/exit 0 与实际二进制摘要；其规范为 203 的 `a3557b7a000baed3d381b2f8f53eafd24f03c65aa8a94a3fdbf31708d69a9e52`。本文源码预检不声称等于尚未冻结的最终 203 生产。

## 对 ZUI 前置的价值与第一片边界建议

拥有动态子项集合的控制器工厂，需要将集合的管理责任随回调环境返回、保存及最终关闭。当前可把 Store 留在回调之外，通过同步参数传递使用，但还不能让这组责任随同一个保存的环境移动。这个差异直接体现在三个原源的 Types 拒绝，未依赖拟议框架或性能结论。

建议第一片只处理有限叶子元素的 `Store<Item>`：Item 使用当前已支持的完整有限 Copy/直接 Managed 字段，无嵌套 Store、回调字段或自递归类型。直接 Store 捕获和含这种 Store 的完整 Package 捕获共同覆盖原源；Fn/FnMut 正文只产生 Copy/Unit 结果，保持显式 move、自含环境、真实调用模式和现有退出责任。是否同片开放 FnOnce 转交 Store，应由正式规范列明；不能由 FnMut 捕获成功推成任意转交已支持。

活动元素窗口可在调用体内创建和结束，但不随返回逃逸。Store/Closure 交替依赖环、被借入的 Store 描述符或元素地址、含回调的 Package 捕获、长期外部回调、异步和 HandlerSlot 替换协议仍是独立义务。该建议没有删去完整 1.4 的 trait/关联项/life/HRTB/lending/dyn、卫生 Syntax/Type CT、取消/并发或产品量化条件。

## 已定位的实现接口

| 模块 | 真实当前入口 | 下一片需要证明的内容 |
| --- | --- | --- |
| Types | `closures::State` 的推断后 capture admission；`retained` 自含环境；`closures/package_effects` 的真实字段写与部分退出 | 仅新拥有捕获形状通过；捕获时只转交一次；Store 写操作决定真实模式；Package 完整性、失败退出与来源继续核验 |
| Analyze | `resources::Place::Capture` / `CaptureField`、Store 指令和真实描述符来源 | 从实际 MakeClosure/DescribeCapture/PackageView 与 Store 操作重建责任及活动窗口，不消费 Types 分类证明 |
| Verify | `closures::check` 对直接 Store/含 Store Package 的独立门；资源、借入和 Store 核 | 独立检查真实捕获图、非逃逸及 Store 前缀责任；伪造捕获、重复转交、父失效和坏类型图仍拒绝 |
| LLVM | `emit/resource_closures::drop_functions` | 这里对直接 Store 捕获和 Package Store 字段仍各有显式 Unsupported；须生成真实 Store 前缀析构入口，核所有 live flags 与声明/捕获顺序 |

当前 TypedIR 已有 `Managed<OwnedStore<T>>`、`MakeClosure`、`DescribeCapture`、`TakeCapture` 和 Package view 指令；这些表示可以表达候选第一片。**这不是无需新事实或已能正确降级的结论**：若来源或失效不能由既有表示独立重建，应在实现前补公共合同。尤其 Store 头部布局固定，不能据此忽略 Closure → Store 元素 → Closure 的语义依赖环。

### 后续静态细化：同一回调在既有保存外壳中的资格

为了让新捕获能力继续使用既有持久 State 与外层 Store，建议在同一片明确：**内捕叶子 Store 的自含 callback** 可继续放入当前已经支持的 callback 字段/元素形状；外层元素和各实际环境的责任仍分别存在。这里限制的是所捕获的内层 Store 的元素，不是把已经支持的外层 callback Store 重新关闭。

相应的额外私有入口是 `zil-analyze::callable_fields::self_contained` 和 `zil-verify::dynamic_store::closure_layouts/layout`：前者当前明确排除 Store carrier/含 Store Package；后者在计算外层元素环境布局时再次独立拒绝这些捕获。仅修改 Types 的最早门会在这些后续入口失败。内层 Store 元素无 callback/嵌套 Store，因此新边不能回到 Closure 图；既有直接 Closure 子环境仍使用原有有界自含检查。所有实际 Store 仍须有完整精确的 canonical resource definition，不能因为头部表示固定而省略元素资格核。

LLVM 可考虑复用已有 `dynamic_store::helper(payload, resources, "drop")` 选择精确元素对应的 Store 析构入口，分别用于直接 capture 与 Package field；它不接受用户自定义 teardown 或同布局替代。捕获标志、Package 壳与字段标志及 Store 内部前缀长度各控制自己的责任，不能用单个“环境仍活”标志替代实际元素清理。

模式和来源不需要先发明新的空合同：Types 的 Store primitive 分支已经对 reserve/push/pop/at_mut 调用 `mark_resource_write`；Analyze 的 `DescribeCapture` 产生 `Source::OwnedField`，Verify 独立产生 `Root::OwnedCapture`，两核的 Store 写入口已有 shared-entry 写检查。StoreBorrow 的父根还必须穿过实际 capture/Package view 链；这是下一片 raw/来源派生测试需证明的链路，静态存在这些分支不算执行验证。

## 拟议验证与预算约束

正式施工应先保留这四源原字节并加入有标记的独立清理 oracle：元素/容器/旁置资源顺序、空/非空 Store、创建和增长失败、调用早退、环境移动、丢弃未调用环境、重复 FnMut 调用。Linux 和 Win64/Wine 分别实际执行；原生 Windows 未运行仍单列 NotRun。当前四个预检只证明到达门，不提供上述机器成绩。

raw/Wire 需修改真实源产生的 TypedIR：借来的 Desc 冒作拥有捕获、重复捕获同一已移动源、坏第二字段、Fn 下写容器、FnMut 返回后环境未恢复、活动元素窗口期间 reserve/pop/give/drop、伪造或循环环境类型。由 Analyze 与 Verify 独立核验，不能只让生成器先拒绝。

旧非 Store 捕获路径的收费保持。新捕获的完整类型、源键、图节点和队列须先计费后复制/分配；只在实际新形状入口增加扫描，不对全部闭包重做新图遍历。新路径以 N / N−1 / fresh N 证明可恢复的 ResourceLimit，默认额度不升。已有精确预算仍按原源与原提交复验。

## 证据位置与结论等级

源码和小收据在 `evidence/impl/203/next-preflight/`；阶段 IR、命令及输出作为独立文本证据归档，不放执行产物。`run-logs/post203-store-capture-preflight/PREPARED.json` 保留最初 NotRun 身份，`check01/RESULT.json` 记录实际结果，两者不覆盖彼此。

本次是只读接口审计加 4 次真实源阶段预检，未修改生产、预算、规范或验收状态。建议的组合可减少用户手工分开管理回调与动态子项的负担；维护成本、相对 Rust 时间/峰值内存/尾延迟及完整 ZUI 产品收益均未测。完整 1.4 继续 NotSatisfied，产品继续 NotRun。
