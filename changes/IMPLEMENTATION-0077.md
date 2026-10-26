# 077 拥有 Package 闭包的前端与独立 Analyze

2026-10-25（Asia/Shanghai）。生产基线 draft15 `fc7aaca720591f2361813ed5c03f684b388a2ffa`；消费公共提交 `f13ce1588d8105491ea6f78a2f8a40aa6aeee67f` 的 schema/wire 13，规范锁 `fadd560193bd9253adbb09e4763867b2740cb7d0db48ad0e35afd89185185c8b`。本提案先于本片源码，语言依据为 `_zil_design` §§8、9；公共边界以 079 与具名接口附件为准。

## 目标与边界

让完整有限 Package 经显式 `move` 进入闭包实际环境，支持纯读 Fn、反复更新且退出完整的 FnMut，以及部分或整体转交的 FnOnce。普通工厂可接收完整 Package 并返回自包含 callable。仍不开放默认借用 Package、嵌套拥有字段、逃逸 Desc/Address、整体自定义 Drop、闭包显式 Package 参数或结果。普通 source whole 赋值仅沿用 whole-moved 后补回，不扩大活 whole 替换子集。

## 前端实现决定

- 拥有闭包及 affine 分类覆盖 Package，避免按值 Copy 调用整个环境。捕获创建沿用真正的 TakeLocal；捕获索引仍按首次自由使用固定。
- materialize 在 root ScopeEnter 后先连续绑定全部 Package views，随后初始化其余捕获局部。view 声明不产生 StorageLive；根 scope 退场发 End，并跳过普通 Drop/StorageDead。临时槽预留位置移到 Bind 前缀之后。
- 字段操作复用现有 Package 指令和 prepare/commit 语义；字段 Desc 来源传到对应捕获根，使直接、别名和写 helper 的 payload 写都影响模式分类。
- 新增私有、有预算的源级 Package 槽流：whole 与直接 Managed 字段记录 may-live/may-empty；分支合流，循环固定点，break/continue 分开；普通 Return、隐式返回和每个 `?` 错误边登记出口。值位置 whole 转交清 whole/字段，give 清字段；赋值只在 RHS 完成后恢复。所有可返回出口完整的写为 FnMut，任一缺槽为 FnOnce，纯读为 Fn。保留 lexical shadowing 与源求值顺序；不把简单“含 give”当作 FnOnce 证据。
- 槽流只影响调用模式推断，不导出安全凭证；没有 Package capture 的闭包不创建该流的状态。节点、槽、拷贝、合流、迭代和名称访问都消耗既有 Types 预算，耗尽确定返回 Limit。

## Analyze 独立重建

- preflight 准入有限 Package capture；独立校验全部 capture 与 view 一一绑定、精确类型/索引、入口连续前缀、无入口入边、非参数 view、无 StorageLive/Dead 或 BorrowLocal(view)。
- scope 流由 Bind 注册根 scope view，由 End 移除；initialized 流同样由 Bind 建立、End 清除，允许 Consume whole-moved 结束。跨路径激活不一致与重复结束都拒绝。
- 私有资源状态增加 capture whole/field cells；Bind/End 在 capture 与 view 间交接事实。End 不 Drop，不建立普通 local newer 关系。真正 LLVM flags 由 079 负责同址保持。
- Shared 拒绝 view 所有责任、Copy、payload 写，包含 Desc 别名和 callee effect。End 与 whole/field 变更检查真实逆向未来 Desc 使用。FnMut 每个正常/Error 返回检查 whole/字段确定完整；FnOnce 可把部分状态交回环境，普通局部与 SSA 责任仍必须清算。
- Package 的 ReadCapture/TakeCapture 不可绕过 view；完整 SSA 转移规则和 factory cleanup 索引独立重建，新增槽及扫描真实收费。Verify 不调用本片任何求证函数。

## 限定验证计划

源码正例：保存及返回完整组件、纯读重复调用、Copy/payload/Managed 字段重复更新、give/refill、分支及循环恢复、whole move/refill、partial/whole FnOnce；失败与 Error 出口模式。最近反例：默认借用、不可写 FnMut 调用、重复消费、旧 Desc 跨变更、错误出口缺槽却宣称 FnMut、非法 view 生命周期/索引/绑定/Shared 写。预算覆盖相关流与无 Package 路径；作者只执行本片限定测试，root 统一正式门与原生应用验证，独审绑定精确提交。

## 证据状态

本提交为施工前提案，未宣称任何新增能力已通过测试。后续实际命令、失败记录与候选源码身份写入 `evidence/impl/077/`。

## 复核后的预算修正预告

非作者复核指出 scopes 的新增 Package view 表虽已对扫描收费，但 expected 集合插入、boundary 表查找和两个独立绑定表插入没有各自完整收取树查找费用。修正先于对应代码提交：每次实际 BTreeSet/BTreeMap 查找或插入分别使用既有 `Budget.lookup(size)`，扫描费用保留；无 Package view 的 boundary 路径不增加此费用。接受性、ABI、清理顺序及错误类别不变，精确分析预算 N 会增加，旧 N/旧收据保留为旧提交证据，重新执行 N/N−1 限定测试。
