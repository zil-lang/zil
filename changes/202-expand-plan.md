# 202 Expand 只读实施计划

**状态：规划，未修改生产，未执行新编译。** 以 `3ff1288f264b14f732cdd571c053254e120afaf2` 的具体 Core 草案为接口；以下定位来自当前 `generics.rs`、`generic_enum.rs`、`generic_budget.rs`、`generic_trait_bounds.rs`、`generic_state.rs` 和 `modules.rs`。正式规范与 root 的 Core 基线发布后才进入实现。201 分支与原始失败证据不变。

## 1. 三种调用入口与唯一分流点

| 来源 | 当前实际路径 | 新路径应做什么 |
|---|---|---|
| 源显式 `helper::<T>(...)` | `Engine::expr_expected` 的 Call 分支先 `call_hints`，逐值实参检查；随后识别 callee 的语法 TypeApply，对每个 type argument 执行 `self.ty`，得到 `explicit`；普通模板最终调用 `instantiate` | 保存源显式约束，复用 `call_hints::constrain` 对实际参数/结果的一致性检查；只有延后分流命中才保留 TypeApply |
| 普通实参推导 `helper(store, value)` | 同一 Call 分支逐实参得到 `actual`；普通泛型分支对每个实际类型执行 `original_type`，用 `unify_argument` 约束 `found`，按声明泛型顺序提取 types，调用 `instantiate` | 不重新遍历/执行实参；最终唯一 T 进入与显式调用相同的分流点 |
| 明确结果上下文 `let s: Store<Item<i32,F>> = empty()` | `statement` 的绑定注解或 Return/Assign/arrow return 向 `expr_expected` 传 expected；Call 的 `stores::call_context` 只恢复合法直接 Store/Store-success 上下文；`call_hints` 及普通分支的 `unify(result, expected)` 约束 T，再调用 `instantiate` | 从明确上下文取得元素 T，不能绑定成外层 Store 或任意 E!T；无明确上下文且无显式参数仍无法推导，不任选先前实例 |

建议把唯一分流放在 **`instantiate` 已有的实际参数 `original_type` 规范化之后、`substitution`/`substitute_item` 之前**。这样三个入口不各写一套依赖扫描，普通数据名义参数也先还原成同一种表示。

具体私有改法：抽取共享 `instantiate_inner` 返回 `Eager(name)` 或 `Deferred { template, element }`；原类型/记录构造/callable 混合 Copy 参数使用只接受 Eager 的薄包装，普通函数 Call 使用允许 Deferred 的入口。旧共享步骤的收费顺序保留，不在调用点再做一次 canonicalize。当前直接调用 `instantiate` 的生产位置只有 `ty` 的名义类型实例、callable 普通 Copy 特化、普通泛型 Call、record 显式类型构造四处，迁移可以限于这些位置。

分流条件必须全部成立：目标为 exact 单 `T:StoreElement` 自由函数；canonical actual T 对**当前词法拥有者**的 callable binder 或已保留普通 T 有依赖；没有 const/where/foreign/receiver/opaque/mixed generics。不依赖时仍走原 Eager。依赖存在但不满足精确模板头时明确拒绝，不能把带游离变量的“具体函数”继续发布。

## 2. 如何保留旧精确预算

`original_type` 当前已经为每个输入节点执行 `budget.node`/`budget.ty`，展开 `instance_types` 中的生成名，再递归复制 canonical TypeRef。给这次现有遍历增加一个可选的借用观察器：在无实参的单段 Named 处，顺带比较当前 scope 的 binder 名；命中则置位。递归进入生成类型的 original 时携带同一观察器。**不另调 `contains/dependent` 做第二次全树扫描，不先 clone 后判断。**

观察器只记录私有依赖位，不存任意游离名字集合，不分配每节点记录。现有字符串预费覆盖当前节点名比较；scope 和实际 binder 名借用既有信息。没有 active 延后 scope 的调用走原函数包装；有 scope 但实际 T 完全具体时，同一原遍历返回 false，不增加 `node/text/ty` 收费调用。`type_key`、bound 检查、substituted item preflight 等旧 Eager 步骤及其顺序保持。

当前源码的 AST Item 没有 NodeId，Core NodeId 是 lowering 才分配的。Expand 私有 scope 可借用 Engine 已唯一登记的声明及其 canonical name/原 FileId+Span，不能声称 AST 中已有 Core binder ID。Core 发布后以 `(ModuleId, function NodeId, index)` 表示真实 binder 归属；程序归一化已保证声明来源与名字不冲突。scope 的切换/恢复必须跟函数处理配对，不能从生成名猜身份。

只有真正新增路径支付新增成本：保留模板表/排队记录、原函数复制、生成 TypeApply 节点及类型参数复制、调用者结果类型替换。原模板在复制前 `budget.item`；同声明只排队一次。新增节点与类型参数输出在构造前计费。默认额度保持。旧 Store 泛型、197 及普通无泛型控制须复测原精确 N；新延后路径另测 N/N−1/fresh。此处描述的是实现约束，尚无新的预算实证。

## 3. 模板保存与结果提示

不得为每个依赖型 T 复制出一个清掉 generics 的普通函数。保留集合按真实原声明登记，pending 中保存**未代入调用者 T 的原模板体**，其中 `generics=[T:StoreElement]`。同一声明的具体 Eager 实例继续使用旧 instance 表，保留模板另有分类，两者不抢同一身份。

延后调用的唯一 `type_arguments[0]` 为**元素类型 T 本身**，例如整个 `Item<i32,F>`，而非 `Store<Item<i32,F>>`、单 ClosureId 或字段路径。它在调用者 scope 下执行原 `self.ty`：mixed-State family 仍由 `generic_state::state_family` 依全部普通参数/唯一 F 槽产生，完整普通/phantom 名义身份不丢失。Core 初始 Type 可以通过该名义声明引用完整类型；Types 解析后必须以完整实际 Type 建实例键。

新结果提示使用 `enums::substituted(original_result, {callee_T -> actual_T}, budget)` 后在**当前调用者 scope**执行 `self.ty`。现有 `Substitution::ty` 是同时替换：替入值后立即返回，不将 caller 的同名 T 再替一遍；`generic_budget::ty_context` 已明确同名同时替换规则，继续复用。不能调用会根据普通 callee 把 `callable_name` 切成 None 的 `signature_result`，这正是原 empty 源名义族分叉的入口。

参数的 give/desc、结果 create、错误联合和表达式求值顺序不由这一类型提示改写。原始 call 的值实参保持一次求值；TypeApply 只新增静态约束。借用/拥有类别的最终转换仍交 Types 和独立责任检查。

## 4. child T 转发与定义时能力

处理保留函数体时，`Engine::item` 除原 callable scope 外建立一个独立的 `StoreElement` binder scope。例如保留 `outer<T:StoreElement>` 中的 `inner::<T>(items, value)`：callee 的 T 与 caller 的 T 即使同名也是两个 binder；类型实参仍指 caller 的 T。三种入口共同的原类型遍历因此必须识别**普通 T 依赖**，不能只识别 F。

新 Deferred 分支的 `traits.check_arguments` 使用 caller 当前 `Bounds` 和 variables；对 caller T，`store_element` 必须确实找到 `T:StoreElement`。旧 `instantiate` 使用空 Bounds/variables 的具体分支保留，不全局把任何 T 当合法 Store 元素。已存在 `generic_check` 在 `Engine::new` 对所有泛型定义检查，包含 unused 定义；这个阶段不因为新保留队列而删除、跳过或只验证被调用的函数。

child 类型应用可继续保留 caller T 到 Types，普通模板本体绝不引入调用者 F。一个简单链应成为 `outer<T>` 的 `TypeApply(inner, [Named(T)])`，外层具体化后再形成 child 的完整 T 实例。定义/实例递归门保持有限；新保留队列需要已登记/处理中区分，不能靠无限 clone 或复用正在构建的占位实例绕过递归检查。

`range_hint` 当前已经认识语法 TypeApply，但最后以 `check::dependent(result, callee_variables)` 判断结果可用性。caller T 与 callee T 同名时不能仅凭字符串认定未解析；实施时核对这一只读提示路径，必要时在新模板分支用已求出的绑定完备性判定。不要为旧路径重做所有范围提示或增加费用。

## 5. 跨模块与可见性

`modules::normalize` 已把每个模块声明转换为 canonical 名，在 combined module 执行 generics 特化，随后按**原 item.span.file**将输出归还定义模块。`functions` 表保留 owner ModuleId 和原本地名；publicness 来自声明，不来自调用者。保留模板沿用原 Item/Span/Publicness，因此可以复用这一分配机制，不需要将模板复制到调用者模块。

`Visitor::expr` 已对 TypeApply 递归处理 callee 和每个类型参数；`Visitor::item` 在处理 params/result/body 之前加入本函数泛型 binder；`Visitor::symbol` 对外部私有 generic 函数/类型在 specialization 前明确 `E-MODULE-PRIVATE`。Restore 模式能将 TypeApply 内的 canonical 函数名恢复成正常模块 Member，并补已有方式的 import edge。首选全部复用，只有实测原 TypeApply 暴露错误时才修改 `modules.rs`。

TypeApply 参数中的 caller 名义类型仍携带自己的 canonical 模块身份，不能因为 callee 在另一模块就按 callee 名字空间重新解析。原有 `protect_alias_nominals`/`protect_alias_binders` 以及 TypeApply 类型参数遍历继续执行；本片不新增无条件全项目 hygienic 扫描。具体控制包含两个模块同名 Item/T、私有 generic、局部值遮蔽函数名、模块 alias 遮蔽及 child ordinary T 转发。

## 6. 写集与前端约定

生产拟写 `generics.rs`、新私有 `generic_store_templates.rs`、必要的 `generic_store.rs`/`generic_budget.rs`、`receivers.rs` 的语义分类和 `lib.rs` 的 lowering；`modules.rs`、`generic_enum.rs` 的 range/hint 辅助及 generic-check/trait/graph 仅在上述实际入口需要时有限适配。所有新增/扩大文件由 root 正式登记。root 先做 foreign/receivers 公共字段默认值机械适配，作者随后只改已批准语义，避免同一构造器互相覆盖。

Types 作者已确认：先在 caller scope 用实际 callable/T 绑定替换 TypeApply.type_arguments，再按完整 Type 建普通实例；普通 T 使用逐函数 scope，不加入全局 callable_names。其最新 201 分支已有私有 `helper_plan::Shape::Store` 来追 callback 位置，它仍不等于完整 T，不能用该 Shape 替代新完整实例约束。这一点是对较早草案基线的后续接口核对，并非新增实现成绩。

交接时应给出 actual Core 中的保留模板表、三种入口的 TypeApply、同名 child T、跨模块路径及一个完全具体调用仍为旧 Eager 的控制。Analyze/Verify 的具体 Store 核不在本 Expand 写集内；如果 Types 完整实例暴露新的独立责任问题，按真实到达层回报，不调整 Core 元数据充当证明。
