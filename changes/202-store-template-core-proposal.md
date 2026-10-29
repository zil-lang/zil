# 202 候选：保留完整 StoreElement 模板的 Core 合同草案

**状态：只读审计后的具体接口提案，尚未进入规范或生产。** 本文件基于 `3b56b28a9a49877c838ea74f329774fc72bfa362`；其生产包含 201 Analyze 作者交付，201 的最终独立验收由 root 处理。本文没有运行新 source、raw、Wire、native 或预算测试，不提供新的 Pass 声明。正式编号、规范摘要、schema26 以及实施写集仍以 root 后续派工为准。

本提案选择路线 A：保留**完整的一个普通 `T: StoreElement`**，由 Types 绑定其实际完整类型。此前 `changes/202-state-bindings-audit.md` 的 F/G State 是另一项历史候选；本片不解决两个 callable 泛型及其声明模式来源。

## 1. 要修复的实际输入与边界

201 `helpers01` 的两个原件必须逐字保留：普通 push 源 SHA256 `101580f8d78e661011d3b73f731c4f2f2ae376a408c1cc02c8aa072d4f627bf9`，普通 empty/reserve 源 SHA256 `d8fd10675872542e99533ee23957db2c3150760a2f71ea8ff28150ad6c3a7a55`。它们在生产 `c9b6d757ca8039f562aa7352fdd2d7d799f9f378` 的真实停止点分别是 Types Unsupported 和 Types SafeReject；测试文件当时未提交，差异身份及诊断见 `changes/201-store-helper-binding-audit.md`。本提案没有重跑，也不把它们称为已通过。

第一片仅保留恰好一个普通类型参数、唯一 bound 为封闭 `StoreElement` 的自由函数。无 receiver、foreign、where、const 参数、opaque callable 返回，也不与 Copy/Fn/FnMut/FnOnce 泛型混合。普通值参数和 `result(create)`、`give`、错误联合继续遵守既有 StoreElement 定义时能力；不会因保存 T 而开放任意字段访问、回调调用、逃逸或捕获。

**只有实际 T 依赖调用者尚未解析的 callable binder，或依赖已保留普通模板的 T 时，才走新路径。** 原先完全具体的普通 StoreElement 调用继续 Expand eager 特化，保留旧 Core 形状和精确费用路径。一个源声明可以同时拥有旧具体实例及一个延后模板；二者不能使用同一个未区分的具体 FunctionId。

## 2. 选定的公开表达

在现有 Core 上新增下列字段/变体；`Call` 本身不改：

```rust
pub enum CoreTypeBound {
    StoreElement,
}

pub struct CoreTypeParameter {
    pub index: u32,
    pub name: String,
    pub bound: CoreTypeBound,
}

pub struct CoreFunction {
    // 现有字段继续保留。
    pub type_parameters: Vec<CoreTypeParameter>,
}

pub enum CoreExprKind {
    // 现有变体继续保留。
    TypeApply {
        callee: Box<CoreExpr>,
        type_arguments: Vec<Type>,
    },
}
```

这里的代码是字段增量示意，不是完整 Rust 定义。第一片允许的非空表只有 `[{ index: 0, name: <词法名字>, bound: StoreElement }]`，且 `callable_parameters` 必为空、`callable_return` 必为 None。旧具体函数、callable 模板、foreign、closure/thin-function 和 CT 合成函数的 `type_parameters=[]`。保持空表和非空表的真实区别，不把普通模板伪装为一个运行时 callable 角色。

**所有延后调用统一为 `Call(TypeApply(static_callee, [完整 T 约束]), values)`。** 源显式 `::<T>` 与由 Expand 从值参数/明确结果上下文唯一推导出的 T 都采用这一形状。TypeApply 是尚待核验的类型约束，不是类型正确、StoreElement 合法或安全的证书；不需要增加“来自用户/来自推导”的信任标记。

第一片的 TypeApply 只能是静态 Call 的直接 callee；其内部只能为正常解析的函数名或模块限定成员。禁止把它当普通值存储/返回、形成函数指针、间接调用、嵌套 TypeApply，或应用于非模板、foreign、method、callable 模板。没有 TypeApply 的裸调用不能命中非空普通模板表。缺实参、多实参、const 实参、未解析实参均明确拒绝，不能退回任取已见实例。

## 3. 作用域与完整身份

普通 T 继续在 Core Type 树中表示为 `Type::Named(name)`，但其解释必须来自**当前 CoreFunction 的声明表**。私有 binder 身份是 `(ModuleId, function NodeId, index)`；名字仅用于该词法范围内的解析。重复 binder 名、非零/重复 index、普通/ callable binder 冲突及定义身份冲突在插表前拒绝，不能由后插入项覆盖。

TypeApply 的类型实参先在**调用者**作用域解析，再作为实参替换被调用模板的 T。例如调用者名为 F 的 callable binder、另一个调用者名为 T 的普通 binder，均不能被被调用函数同名 T 捕获。泛型作用域优先于同名名义声明；越过函数边界的游离 `Named(F)`、未声明 T、跨模块错误别名都不是合法的延后变量。嵌套普通 helper 的 TypeApply 可以暂时含外层 T，但最终实例发布到 TypedIR 前必须完全具体。

`aggregates::Resolver` 目前会收集 callable 名字；新增普通 T 不得加入一个全模块字符串白名单。需要独立的逐函数普通 binder scope，并在函数结束时恢复。`callable_fields::specialize` 必须遍历 TypeApply 的 `type_arguments`，使外层真实 F 被替换后，子 Store 模板才形成完整键。

实例语义键固定为 `(TemplateKey=(ModuleId, NodeId), full_actual_Type)`。完整类型保留名义名中的模块身份、全部普通泛型实参造成的 family 身份、字段及顺序、资源头、数组长度、错误两侧以及真实 ClosureId；不能用布局、一个 ClosureId、短函数名、生成次序或未核碰撞的 hash 代替。现有 `Type` 已实现完整 Eq/Ord/Hash；无需为本片改其公开形状。可实施的首版私有存储为每个 TemplateKey 下的有界实例表，逐项先计费完整类型比较，再比较 full Type；命中后复用 FunctionId，未命中时先计费复制和插入。这样无需把调试字符串当身份，也不会隐藏容器内部的未计费深比较。

源身份同时保留 `SourceUnit` 的 ModuleId/FileId、实际源字节、原声明 NodeId/Span、展开映射与模块导入可见性。TypeApply 的新节点有独立 NodeId；源显式应用保留其 span，推导插入的应用映射回原 call/callee。不能复用子节点 ID，或仅凭 `$generic_*` 名字认定其拥有模板权限。

## 4. Expand 的具体实施路径

当前失败来自 `generics::instantiate` 先把 T 替成 `Item<i32,F>`、再清除普通泛型；新的普通 helper 没有 F 声明，`item` 和 `signature_result` 又把 active callable scope 切成 None。新路径必须在这次擦除前分流，而非随后修补生成名。

1. `Engine::new` 继续执行已有 `generic_check`、trait-bound 与 helper-graph 定义检查，包含**从未调用**的定义。先核 exact 单 StoreElement 头，不取消旧非 StoreElement 检查，也不让合法调用掩盖非法定义。
2. 在现有 call 的显式参数处理、`original_type`/unify/type-key 工作中识别实际类型对当前词法 binder 的依赖。Engine 私有作用域需要记住真实定义身份及 binder 类别；不能只测试某个字符串是否为 F/T。完全具体类型仍进入原 `instantiate`。
3. 对依赖型实参，确保原 StoreElement 声明以**仍含 T 的模板体**进入一次 pending 队列。按原声明身份记录已排队模板；不把调用者的 `Item<F>` 先代入模板体。模板表、队列、复制与新增遍历均先计费。
4. 在调用者作用域里正规化实际 TypeRef 及 mixed-State family，保留为 `TypeApply` 的唯一参数。源隐式调用也插入相同约束。已有源显式参数不能被推导结果覆盖；实际参数/明确结果产生的其他约束仍须一致。
5. 返回类型提示使用原模板结果与本次完整 T 做替换，并在**当前调用者** scope 中正规化。不能复用会切到普通被调用者 `callable_name=None` 的 `signature_result`。`empty()` 只从明确 `Store<...>` 结果上下文或显式 T 获得约束；没有唯一约束仍报错。
6. 处理保留模板体时建立其普通 T scope。其 child helper 调用可形成 `TypeApply<外层 T>`；后续 Types 替换后才能建具体实例。所有值实参仍按旧顺序只遍历/求值一次；额外类型约束处理不复制或重新执行表达式。
7. `receivers::function` 对合法保留模板发布 `CoreFunction.type_parameters`；`lib::expr_expected` 对正规化后的语法 TypeApply 降为 Core TypeApply。其他未消除泛型仍拒绝，foreign/method/opaque 的现有门不放开。

### Expand 建议写集

| 路径 | 精确用途 |
|---|---|
| `crates/zil-expand/src/generics.rs` | Engine 普通 binder/保留队列；call 分流、类型约束正规化、调用者上下文结果提示；保留旧 eager 分支 |
| 新私有 `crates/zil-expand/src/generic_store_templates.rs` | exact 单 StoreElement 模板分类、词法依赖扫描和有界保留辅助；注册前由 root 在任务写集确认 |
| `crates/zil-expand/src/generic_store.rs` | 复用现有 Store/result 上下文形状，必要时暴露给新私有辅助；不改 builtin 责任语义 |
| `crates/zil-expand/src/generic_budget.rs` | 新保留工作及输出 TypeApply 的预费；原语法 TypeApply 已有遍历，避免重复收费旧路径 |
| `crates/zil-expand/src/receivers.rs` | exact 模板头降为普通 binder 表、保持方法/opaque 边界；与 root 的字段机械适配错开提交 |
| `crates/zil-expand/src/lib.rs` | 注册新私有模块、受限 TypeApply lowering；使用既有 `expr_node`/source-map 身份 |
| `crates/zil-expand/src/modules.rs` | 只在现有 TypeApply 类型实参重命名/词法保护不能保真时改；已有语法遍历应优先复用 |
| `generic_check.rs` / `generic_trait_bounds.rs` / `generic_helper_graph.rs` | 先作只读控制；只有新保留形式确实需要时作有限适配，不删除未调用定义检查或递归门 |
| 新 `crates/zil-expand/tests/impl202_store_templates.rs` | 原两输入 Core 结构、显式/隐式一致、scope/模块/unused 定义、旧 eager 与预算控制；正式路径待派工登记 |

`generic_callable.rs` 的 callable 角色语义和 `generic_state.rs` 的名义 family 算法不是本片重写目标；新保留路径应带着正确调用者 scope 复用它们。实际若需越过上述写点，先以根因报 root。

## 5. Types、实例规划与效应合同

建议新增私有 `store_templates.rs`，独立保存普通模板/完整 T 绑定；不扩写 `helpers::Bindings([Option<ClosureId>;2])` 以假装它能携带任意类型。`Signature` 需要明确区分具体函数、callable 模板、普通 StoreElement 模板；普通模板尚未实例化时没有可调用的具体 FunctionId。具体实例的 `callable_contract=None`，不产生空角色表、字段充参数号或虚构 CallEvent。

`lib.rs`/`program.rs` 的签名计数、`Builder::resolve_callee`、`expr_raw` Call 分派、`known_type` 结果提示与 `functions::pointer_type` 必须统一认出新类别。实例化前核 TypeApply 的 placement、静态目标/可见性、唯一完整 T、所有值参数和明确结果约束；以后仍走真实参数模式、错误准备和资源门。`push(Store<A>, B)` 不能只相信 TypeApply 声称的 A；最终实际签名与实参必须一致。

`helper_plan` 的现有 Unknown/Environment/Package-callback Shape 不足以表达 Store<T>。本方案从 TypeApply 显式完整约束规划普通实例，并在外层 callable/T 替换后建键；不发明 Store 的 Environment。有限依赖图需同时包含 callable 与普通实例节点；同源递归、不同 T 扩张递归、混合循环在复用“正在构建”的占位实例前拒绝。

`inference::{headers, prepare, expr, known_term}` 必须看见完整的普通类型约束及其结果原型，不能只添加 Core 枚举就假设现有 ClosurePackage 的单 environment 项能承载所有信息。保留模板 T 在定义内是 opaque，真正来源推导在调用者 scope 及具体实例中完成；当前同 F 的有限 State 仍是本片边界。`retained`/`closures`/`names`/`managed_slots` 的所有 Core 遍历需要穿过 TypeApply；Managed local 改写只改值表达式，不改同名的类型 binder。

当前 `resource_effects` 以具体 FunctionId 建图并过滤 callable 模板。建议为其私有目标键增加 `EffectTarget::{Concrete(FunctionId), TypeTemplate(TemplateKey)}`：对保留模板按真实参数位置计算抽象 Store 写效应及 child 调用传播，T 保持 opaque；实际具体实例再走正常类型与责任核。不得把模板 Signature 的占位 id 当具体函数，也不得因跳过模板而默认 ReadOnly。`reserve/push/pop` 的跨 helper 结构写入必须使活窗口反例继续失败。

## 6. 公共构造、CT、IDL、Wire 与编辑器

| 区域 | 精确适配/结论 |
|---|---|
| Syntax / parser / `spec/grammar.ebnf` | 语法 AST 已有 `ExprKind::TypeApply`；parser 已解析 `::<...>` 后的 Call。本片不需要新增语法，只补保真控制，避免多余解析器重写 |
| `zil-ir/src/lib.rs` | CoreFunction 新表及 CoreExprKind 新变体；现有 Call、Type 和 TypedIR 指令暂不加字段 |
| `zil-expand/src/{receivers,foreign}.rs` | 前者发精确表；foreign 发空表。root 做公共构造机械适配，Expand 作者负责语义分流 |
| `zil-types/src/lib.rs` | thin function/closure materialization 的合成 CoreFunction 发空表；原函数实例化复制/清空表须明确区分模板与具体实例 |
| `zil-ct/src/source.rs` | 源规范化复制函数普通 binder、保存 TypeApply 及全部类型参数；遍历和复制前收费；query 的运行时名/函数资格检查认识新节点 |
| `zil-ct/src/materialize.rs` | 所有 CoreExpr 遍历保持应用约束；该文件已见 CoreFunction 字面构造位于测试，按构造器迁移处理 |
| `zil-driver/src/ct_program.rs` | header/expression collector/hint/resolve/strip 遍历不抹 TypeApply；合成 query 函数空表；入队前先核资格/费用 |
| 其他 Core 测试构造器 | `zil-types` 的 names/resource-effects 测试、CT source/materialize 测试、driver impl088 等机械补空表，不改测试语义掩盖回归 |
| `spec/interfaces.json` | 现有文档 registry 的 `schema_version:1` 是格式版本，不是编译器 schema。需登记 Core 普通表/TypeApply、scope/信任/消费者规则，并更新 `implemented_partial_schema` 的 compiler/raw/fingerprint 版本及有限边界 |
| `zil-base::SCHEMA_VERSION` / `zil-ir::wire::WIRE_VERSION` / 指纹域 | 建议由 root 统一冻结为 26 / 26 / `ZIL-TYPED-MODULE-V26`，并更新兼容性声明及版本断言；本文不直接改这些值 |
| `zil-ir/src/wire.rs` | 当前 payload 为具体 TypedModule/Candidate/CtModule，无 Core 传输编码；不把 TypeApply 编成新运行时 opcode。版本头拒旧、全部现有 typed 内容往返仍需实测 |
| `zil-driver/src/session.rs` | query key 已含 RULE/SCHEMA/version 与源输入；schema26 失效旧缓存。新归一化路径不能只凭函数短名/旧生成名复用未绑定实例 |
| `zil-ide/src/lib.rs` | 当前 EditorSession 直接用同一 CompilationSession/driver，停止在 Verify；本片没有独立 LSP 泛型检查器需要另实现。新增语义应通过共享 driver 到达编辑器，并实测 CLI/IDE 诊断一致；不能宣称新增补全、导航或其他未实现 LSP 功能 |

CT 的运行时源规范化必须完整保存新表/类型实参；但 **CT 执行普通 StoreElement 模板或 TypeApply 本片保持关闭**，collector/header 在 clone/执行前明确 `E-CT-FORBIDDEN`。新包装不得隐藏 Store primitive、foreign 或运行时资源操作；不能先去掉 TypeApply 再把裸 callee 当安全 CT 函数。

静态审计认为现有 Package/Closure/OwnedStore、ErrorUnion、普通 Call 及参数模式足以承载最终具体函数，无新增 TypedIR opcode 的必要；这是待实现验证的结论。Analyze/Verify 不读普通模板授信：仍从最终具体元素、真实捕获/资源、Store 前缀、参数模式、错误转交及 loan/清理重建。201 的布局、捕获环、父窗口，以及直接 Store/含 Store Package 捕获关闭边界保持。

## 7. 预算与未可信元数据

默认限额和产品门不增加。完全具体旧路径应没有新收费遍历：无 active 延后 binder 时常量分支返回旧逻辑；需要识别依赖时尽量在原本已付费的 original-type/unify/type-key 遍历中携带信息，而不是先重扫所有旧类型。新增模板保存、TypeApply 节点/类型参数、作用域表、完整类型复制/相等比较、实例规划/效应队列均在分配或展开前扣现有额度。

raw Core 入口先核长度、深度、字符串、身份和分类，再 clone/hash/归一化。新增元数据的全局预算纳入 `inference` 的 source preflight/snapshot、CT source/query preflight 和所有直接 check_program/check 入口；不能仅保护正常 Expand 产物。旧精确预算控制和新 N/N−1/fresh 分列，不拿新预算阈值冒充旧收费不变。

| 未可信构造 / 最近反例 | 必须核验的点 |
|---|---|
| 普通 binder 2 项、index 非 0/重复、空名、重复定义身份 | 有界 exact 头；插表前拒绝，不覆盖映射 |
| 同时普通/callable 表、foreign/receiver/opaque 返回、错误 bound | 明确模板类别；metadata 不能制造新能力 |
| TypeApply 裸值/嵌套/非静态目标/错误目标类别 | placement 与静态解析；不掉到 closure/函数指针 fallback |
| 0/2 个 type_arguments、自由 Named、错误模块 alias、调用者/被调用者同名 T | 调用者先解释实参，完整替换，无动态作用域或全局名字白名单 |
| TypeApply<A> 配实际 Store<B>/值 B，phantom 或普通数据参数不同、同布局异名 | 完整名义 T 相等，真实参数约束独立；不得压成单 ClosureId |
| 巨量/深层类型、重复长名称、实例扇出、递归与混合 helper 环 | 所有扫描/复制/键比较/队列先计费，旧默认值不增加 |
| 未调用泛型错误字段访问、非法 give/create、错误 builtin/返回能力 | 既有定义时检查继续生效，未实例化不等于未检查 |
| CT wrapper 调普通模板、TypeApply 遮蔽 Store/foreign | CT 明确关闭，不把新节点当纯函数证明 |
| 具体 raw/Wire 改元素、捕获、Call 参数模式、错误载荷、父 Store 结构写 | Analyze/Verify 按实际 IR 独立拒绝，Types metadata 不充安全证据 |

## 8. 实施交接与验收计划

root 拟持规范、Core/schema、全仓接口机械迁移及集成；本作者接 Expand 模板保留/TypeApply 正规化；frontend 持 Types 完整 T 绑定、实例计划、源推导及效应；impl197 持非作者 source/raw 审核。正式派工前，本文件仅为可审查草案。

实现后必须逐字运行两个 helpers01 原输入，并覆盖 empty 显式 T/明确结果/无上下文、reserve/push/pop、失败 push 载荷回收、跨模块私有访问、同名 binder、同实际 ClosureId 不同普通名义 T、helper 内活 Store 窗口、未调用错误定义及 raw malformed metadata。验证要区分源成功、独立 raw/Wire、实际机器执行和未运行层级；最终具体 Call/OwnedStore IR 应证明不需要新 opcode，不能只靠本静态判断。

Rust 四目标的本轮反思：完整名义 T 和失败载荷的保真对应可表达性与正确性；旧 eager/精确预算对应可预测编译成本；单次值求值、普通静态 Call 和现有 Store 表示避免新增运行时机制；独立核与精确作用域使生成代码保持可审计。拟突破点是普通 StoreElement 函数可跨越尚未解析的具体 callback State，而非靠内联或虚构 noescape 角色绕过函数边界。以上均为设计目标，本草案未提供其新实现成绩。
