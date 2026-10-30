# 218：单一自含 callable 的公共拥有工厂

状态：2026-10-30 root 已冻结的施工提案；尚未实现或验收。源码基线 `63b5d064f443e9ed7989e0ec87d4e18b540a370a`。本文件不代表生产修改、已通过测试或旧 218 恢复；旧本地候选及证据继续按 NotRecovered 保留。

## 为什么

现有语言可创建具体 opaque 工厂、可按 F 约束调用一个具体环境、可把自含 F 存进拥有 State，却不能用普通公共泛型工厂把 F 转入新的 opaque callback。UI 公共回调装饰器、带事件映射的注册入口因此只能暴露具名 State 或重复内联。补齐这项组合，复用既有所有权与内联环境，无需新增动态擦除或隐式分配。

## 是什么

一个直接 F 类型绑定、一个按值 F 输入、一个新自含 opaque 返回环境；支持 Fn、FnMut、FnOnce 的实际声明模式，及既有有限普通伴随参数。准入范围与明确保留拒绝以当前 `_zil_design.md` §9.4.1 为准；输入 F 可按实际声明使用、消费或在退出清理，不强制结果保存原输入，input/result ClosureId 须不同。

最小公共 API：

```zil
pub fn wrap<F:Fn(i32)->i32>(f:F, bias:i32)->impl Fn(i32)->i32 {
    return move (x:i32)=>f(x)+bias
}
pub fn once<F:FnOnce(i32)->i32>(f:F)->impl FnOnce(i32)->i32 {
    return move (x:i32)=>f(x)
}
pub fn mutable<F:FnMut(i32)->i32>(f:F)->impl FnMut(i32)->i32 {
    var inner=f
    return move (x:i32)=>inner(x)
}
```

这些源码须施工前冻结预期，再运行；当前没有执行结果。两层 `wrap(wrap(f))` 在普通调用方按值参数求值顺序组合；循环/递归实例不能通过 Unknown 占位或空依赖放行。

## 怎么做

1. Expand 只为该有界头开放 opaque + callable 组合，并为返回闭包增加真正的定义期符号检查；未调用非法定义仍拒绝。保留直接 `return f`、borrowed F、多 F、State、普通 T 混合等原拒绝。
2. Types 将模板与普通预先 grounding 的 factory 图区分。按完整 F 身份生成有界实例，读取实例完成后的真实 Closure 返回型和导出签名。F 的声明模式按参数/局部/捕获位置传播，不能以 ClosureId 全局覆盖。
3. 具体实例使用普通拥有边界 + CallableReturnContract；不发布空 nonescaping callable_contract。Analyze 和 Verify 各自检查恰好一个按值自含 Closure 输入、完整捕获图、grounded 新结果、每次实际转交与清理。公共字段、opcode、预算默认值保持。
4. 复用现有内联 nested Closure ABI、Take/Read/BorrowCapture 以及递归清理；若真实 O0/O2 输入暴露后端缺口，先记录失败再扩展最小写集，不预先修改 LLVM 路线。

### 实施前端精确写集

本片需要的 Expand 文件为：

- `crates/zil-expand/src/receivers.rs`
- `crates/zil-expand/src/generic_callable.rs`
- `crates/zil-expand/src/generic_check.rs`
- 新私有文件 `crates/zil-expand/src/generic_factory.rs`，如拆分定义期检查可再有 `generic_factory_flow.rs`
- `crates/zil-expand/src/generic_helper_graph.rs` 仅在新模板依赖/递归检查确有必要时修改
- `crates/zil-expand/src/generics.rs` 仅为新增私有模块注册，由 root 协调单写

不需要 `conditional.rs` 或 source-level `#if` 的生产文件；219 可以独立推进。共享 `lib.rs`/模块注册由 root 协调，双方不整文件相互覆盖。

Types 拟写集为 `src/{helpers,helper_plan,helper_flow,retained,lib,program,inference,closures,state_origins}.rs`，并可在私有新模块承载 factory profile/来源检查。Analyze 与 Verify 各占自身 `src/{retained,helpers}.rs` 和新的专属私有核验模块；独立实现，不共享求证函数。规范、schema/登记与主树工单由 root 单写。

## 必须保留的最近反例

- F:FnOnce 实际传 Fn 或 FnMut，捕获后调用两次、先消费后捕获、别名/move 复活、循环回边重复消费；未调用定义也必须失败。
- 同 ClosureId 的两个运行时值、另一个本地工厂产物不能串用声明模式或责任。
- 直接/递归 child 的外部借入，borrowed F、lending 结果、多 F/State 混合、不同 return closure 身份、直接 return F、增长递归与 CT 调用保持拒绝。
- 从真实合法源导出的 raw 模块，删除/伪造返回合同、变成普通空 helper 合同、错输入类型/捕获身份、假自含、重复 Take、ReadCapture 复制、错误 cleanup/layout/控制流，须在 Analyze 或独立 Verify 的目标层真实拒绝，不能靠更早 Types 拒绝代替。

## 验收与证据

先冻结源、oracle、失败注入计划和 CLI/LLVM23 工具链身份，再执行跨模块 Fn/FnMut/FnOnce、未调用销毁、提前/错误返回、State/Store 保存/移除、O0/O2 和逐实际分配点失败。普通原 CLI 产物与额外链接 wrapper 的故障产物分开记账。新工作额分别核 N/N−1/fresh N，旧 helper/State/opaque 路径的精确预算保持。root 最后运行组合全量，作者不自审或自行合入。

阶段源码达到可编译或限定测试检查点即推送任务分支并读回 SHA，不等待大型运行归档结束。现有测试次数与旧 218 聊天数字均不是本片通过证据。

## Rust 四目标

安全目标是实际责任转交、声明能力保持和独立核闭环；运行/内存保持已有内联布局，不添加隐式分配，但没有 Rust 性能成绩；表达改善是公共泛型 callback 工厂可构造新 opaque 环境；编译工作受原预算约束，编译加速仍未测。有限增量不能宣称完整 1.4、全部 ZUI 前置或产品四目标合取已完成。

## 正式派工补充

218由language218实施前端，220由root实施Analyze与集成，221由language_review_plan实施Verify，219由language_parallel实施源选择。精确独占写集以主树工单为准；219的session/project/ct_program接点已纳入其独占写集，私有模块注册随所属单一文件施工并由root统一发布。交叉独审与组合回归保持。

## Copy 签名消歧

这里关闭的 callable 参数/结果指具体 Closure、F 及拥有或动态环境值；既有有限无环境 FunctionPointer/ForeignCallback 仍按原 Copy 形状和原调用/FFI 许可处理，不由本条新增调用能力。Store 叶形状对三类 callable 的既有显式限制保持。 这是既有 Copy 与拥有环境边界的文字澄清；生产接受性、schema 和默认限额不变，三阶段继续独立核验。
