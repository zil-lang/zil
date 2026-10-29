# 206 后语言优先缺口建议

此文是冻结 `7b2b1f58...` 后的只读工程建议，不登记 207、不修改正式规范或验收状态。排序依据是 ZUI 先以显式函数/类型构建实际拥有状态，再增加可复用接口和 DSL 的开发路线。

## 1. 让组合状态可保存、可进入容器

206 已完成普通完整 Product，但 `packages::ordinary` 与旧 `packages::supported`、`owned_array_supported` 分离。`closures` 的外部捕获推导及 Analyze `legacy_value` 仍保留旧资格；作者最终 paired raw controls 实证新 Product 不进入 Address/array/ErrorUnion/Store/capture。因而组合子状态虽然可以普通传参和移动，仍不能自然进入现有保存回调的环境或动态元素集合。

建议先具名开放 Product capture/保存环境，再单独登记 StoreElement、拥有数组、ErrorUnion 父容器；每片都带实际独立 child debt、深来源、窗口失效、递归清理与 fault oracle。不要将所有旧资格调用统一替换为 `ordinary`。现有 201/204 有限 callback 元素和 Store capture 已存在，后续应组合它们，不能把这些已实现能力描述成缺失。

依据：[packages.rs](../crates/zil-types/src/packages.rs) 的 `supported`、`ordinary`、`owned_array_supported`；[budget.rs](../crates/zil-analyze/src/budget.rs) 的 `legacy_value` / `local_value`；[impl206_analyze.rs](../crates/zil-driver/tests/impl206_analyze.rs) 的 raw parent paired controls。

## 2. 普通拥有值与泛型接口的组合

有限 Copy 泛型、静态名义 trait、StoreElement 模板与 callable/State 模板已有实现。当前 delayed ordinary binder 仍由 `generic_store_templates::parameter` / `retain_store_template` 限为恰好一个 `T:StoreElement`，`CoreTypeBound` 也只登记该保留约束。新非 Copy Product 不应靠某个具体实例碰巧成功来取得未声明的 Copy、字段访问或调用权限。

建议以一个实际 ZUI 组件构造/持有/返回接口冻结输入，逐步开放普通拥有数据 T 与显式策略/回调参数的组合；明确声明期可用操作、实例身份、完整返回及 move/cleanup 合同。不能把补一个已有特殊入口称为“真正普通泛型全部完成”。

依据：[generic_store_templates.rs](../crates/zil-expand/src/generic_store_templates.rs)、[generic_check.rs](../crates/zil-expand/src/generic_check.rs) 的 `validate_parametric`、[zil-ir/lib.rs](../crates/zil-ir/src/lib.rs) 的 `CoreTypeBound`。

## 3. traits、关联类型与 lending 组合

`static_traits::collect_static_traits` 和 `generic_trait_bounds::Index::declaration` 明确拒绝一般关联项、泛型/默认/带合同方法等后续形状。现有 closure lending 已支持特定共享结果及 Fn/FnMut、自含 Copy/ByteBuffer 来源；`lending::populate` 的实际来源核验仍不允许把任意 Package 内部或外部 Address/Desc 视为自含 lending 根。

ZUI 的统一测量/布局、模型访问和视图借用接口会需要更广的关联类型与逐调用依赖。建议在实际保存状态与泛型数据接口贯通后，按一个名义关联投影或多根 lending 合同逐片实施，保持固定候选选择和真实来源核验；不借安全失败回溯选择另一 impl。

依据：[static_traits.rs](../crates/zil-expand/src/static_traits.rs)、[generic_trait_bounds.rs](../crates/zil-expand/src/generic_trait_bounds.rs)、[lending.rs](../crates/zil-types/src/lending.rs)。

## 4. 卫生 Syntax 与 ui.component

AST/parser 已保留 `SyntaxIsland` 定界入口，但 `generic_budget` 对未执行 syntax island 明确 Unsupported，`Expander::unexecuted_attributes` 会拒绝仍未执行的属性。这个事实不等于已经实现了第三方卫生扩展执行。

`ui.component` DSL 最终需要锁定扩展清单、卫生身份、来源映射和有界生成；生成结果只能重新进入未核 Syntax/CoreAST 和完整安全流水线。建议与前述语义入口并行准备固定原始输入，但在显式 API 的拥有状态与集合能跑通后再优先扩展糖层，避免用新语法掩盖底层能力缺口。

依据：[ast.rs](../crates/zil-syntax/src/ast.rs) 的 `ExprKind::SyntaxIsland`、[generic_budget.rs](../crates/zil-expand/src/generic_budget.rs) 的对应拒绝、[expand/lib.rs](../crates/zil-expand/src/lib.rs) 的 `unexecuted_attributes`。
