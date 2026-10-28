# 普通 StoreElement helper：完整类型绑定的只读接口审计

**状态：下一阶段接口建议，未修改生产、公共 Core、规范或 schema；本审计未执行新编译。** 201 Analyze 已独立交付于 `f05c60b05a155feb5d74b51eff8cdcd0e2bf6f65`。本报告不得替代 201 验收，也不把当前失败输入降为已完成。

## 已观察输入和真实停止位置

前端作者的 `helpers01` 在生产 `c9b6d757ca8039f562aa7352fdd2d7d799f9f378` 上执行；当时新增两个 probe 测试尚未提交，实际 command 收据记录测试差异 SHA256 `175d598b2b1fe11f7249ee67eb3df0775a36890a94df46c2b39e9ab43d479e08`。这不是干净整树运行，本审计也没有重跑。与审计树相比，Expand/Types/Analyze/Verify/LLVM 生产目录没有差异。

两份源均包含普通定义：

```zil
fn empty<T: StoreElement>() -> Store<T> contract { result(create) } { return store_new() }
fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }
fn push<T: StoreElement>(items: Store<T>, value: T) -> T!void { return store_push(items, value) }
fn pop<T: StoreElement>(items: Store<T>) -> void!T { return store_pop(items) }
```

实际调用者仍是 201 的单 `Pair<F>` 参数函数，在函数体建立 `Store<Item<i32,F>>`、拆成两个 Item 并调用字段；没有引入两个 State 参数的新能力。

| 原 probe | source SHA256 | 实际诊断 |
|---|---|---|
| 用普通 `push(items, Item{...})` 替换 builtin push | `101580f8d78e661011d3b73f731c4f2f2ae376a408c1cc02c8aa072d4f627bf9` | Types Unsupported `E-UNSUPPORTED`：错误联合载荷尚含未绑定 callback，不能成为有限 Copy/完整拥有 Package |
| 用 `empty()` 和 `reserve(items,2)` 替换 builtin 构造/reserve | `d8fd10675872542e99533ee23957db2c3150760a2f71ea8ff28150ad6c3a7a55` | Types SafeReject `E-TYPE-MISMATCH`：调用者需要 `Item_$data0:closure0`，实际工厂结果为 `Item_3{callback:Named(F)}` |

两例 `verification_runs=0`，Analyze/Verify/native 未执行。原始文件在 `/dev/shm/zil201-frontend/helpers01/store201-generic-push-types-572054-0/` 和 `store201-generic-factory-types-572054-1/`；各自 `main.zil`、`compile.json` 和 Core 阶段输出应作为失败原件保留。

## 丢失的不是第二个运行时 callback 角色

`generic_callable::template_with_state` 只识别 Fn/FnMut/FnOnce 泛型。普通 `T:StoreElement` 不产生 callable 模板。`generics::Engine::instantiate` 把 T 替换为源层的 `Item<i32,F>` 后，仅保留 callable 泛型，清掉普通 T 的 generic 声明。这个新 helper 本身没有 F 的声明；随后 `Engine::item` 的 `callable_name` 为 None，不能恢复调用者的 F 绑定和 mixed-State family。源层的 `Named(F)` 已进入一个被当作普通具体函数的签名。

`generic_state::state_family` 的数据名义键依赖当前 callable binder；它不能从任意字符串 F 或任意生成名反推词法来源。这解释了第二个 probe 中 `$data0` 和无 family 的 `Item_3` 分离。只删除这两个名字的差异会掩盖未绑定字段，无法修复第一个错误联合问题。

现有 `CoreFunction` 只有 `callable_parameters:Vec<CoreCallableParameter>`，没有保留普通 StoreElement 类型参数的表。Types 的 `Signature.template`、普通函数编号、helper 安装和预先推导也都以该表是否为空来分派。因此必须给普通类型模板提供明确的公开表达，不能仅松一个私有检查。

## 两种路线的比较

| 选择 | 能解决的内容 | 必须处理的不足 |
|---|---|---|
| **A：保留完整 T:StoreElement 模板，到 Types 绑定完整实际 T** | `push` 同时从描述符的元素和拥有实参核 T；`pop/reserve` 从实际 Store 元素核 T；无参数 `empty()` 从明确期望结果核 T；完整名义类型、普通字段、callback 身份一起保留 | 新普通类型 binder/模板分类、实例键、预推导与依赖计划；显式类型应用如何到达 Core 需冻结 |
| B：新增只提取 callback ClosureId 的有限 State selector | 可以为已知 `State<F>` 形状找到 F；适合另项一个 State 含 F/G 的绑定来源问题 | 不能独自表达 T 的完整名义身份；无参数工厂没有参数可选；不同普通 T、错误联合/Store 包装、两个实参一致性仍缺信息 |
| B 扩展成“选择完整 T 的参数/期望结果路径” | 可准确描述 `Store<T>` 元素、直接 T、错误载荷、预期结果等约束来源 | 此时已经是 A 的普通类型 binder 的来源描述；不应继续叫 callable 角色，也不能只保存两个 ClosureId 槽 |

建议采用 **A**；有限 selector 可以作为其内部约束来源，不作为虚构 Fn 能力。对一个完整的 `T:StoreElement`，helper 没有凭空调用某字段的权限。它只获得现有 StoreElement 有限构造、完整转交和 Store 操作能力；函数体仍需定义时检查，实际实例还要进入通常的 Types/Analyze/Verify。

## 建议先冻结的最小 Core 合同

以下是待 root 决策的语义，不是已经存在的字段：

1. 为普通 StoreElement 模板保留独立的泛型 binder 标识、名字和精确 bound；与 `CoreCallableParameter` 分开。允许的泛型数及与其他 Copy/callable 泛型混合边界需明确冻结，不能由本修复自动扩张。
2. 实例的类型键必须含真实定义 `TemplateKey=(ModuleId,NodeId)` 和完整有序实际 T。实际 T 包含完整名义身份、所有普通类型实参、具体 ClosureId、字段顺序及资源类型。不能以字段布局、一个 ClosureId、调用者顺序或未经碰撞核验的 hash 代替完整相等性。
3. `Store<T>` 实参、直接 T 实参和明确期望结果产生相等约束。每处约束都必须一致；无约束的 `empty()` 保持无法推导，不能挑选第一个已见 callback。描述符/拥有头之间只允许现有参数模式转换，不借类型绑定消费或复制额外责任。
4. `empty()` 的 `result(create)`、`give` 参数/结果、`push` 失败 T 载荷及 `pop` 成功 T 载荷完整保留。类型特化不授予 create/give 的责任证明。
5. Core 的 `Call` 目前只有 callee 和值实参，没有 `TypeApply`。若此片需要保留源显式 `::<T>`，必须提供可核的类型应用元数据，或明确让 Expand 形成仍保留 T 约束的模板实例；不能悄悄丢掉显式类型实参。已存在的 `Ascribed` 只能提供实际值的类型上下文，不能伪装为任意函数泛型绑定。
6. 不产生空 `CallableParameterContract`，不把 Store/Package 参数登记成直接 Closure 角色，也不把 T 的内部字段号填进 `CallEvent.parameter`。无需为普通 push 发明回调调用事件。

这会改变公开 Core 模板表示/含义，应按公共协议冻结 schema/指纹域及所有构造/复制入口。最终 TypedIR 即使不新增字段，也不等于 Core 接口未改变。Wire 当前主要载荷为具体 TypedModule/Candidate；仍需明确新版本头、旧 schema 拒绝及缓存/恢复身份。

## 实现影响的精确位置

| 区域 | 需要改变或核对的接口 |
|---|---|
| Expand `generic_store.rs` / `generic_trait_bounds.rs` | 保持 StoreElement 的定义时有限能力，不把 callback F 当 Copy；新增模板保留判定要认识完整 T 的依赖 |
| Expand `generics::instantiate/item` | 当实参或明确结果仍依赖已保留 callable binder 时，不提前把普通 T 擦除为含游离 F 的具体函数；完整生成名义 family 不丢失 |
| Core / Expand lowering | 发布独立普通类型 binder 及必要的类型应用约束；符号 T 的作用域以声明为准，不能从 `Named("T")` 字符串猜 |
| Types `signatures`、`program::check_program`、`inference::headers` | 普通模板不能抢占具体 FunctionId 或在未绑定时走具体签名/错误联合门；单模块/多模块和 visibility 同步 |
| Types `inference` | 保留完整 Type 约束，确保无参数工厂从调用点明确上下文得到 T；不能把同布局或同工厂的不同实例混为一个值 |
| Types `helpers::Template/Instance` 或新的普通模板组件 | `Bindings([Option<ClosureId>;2])` 不足以存完整 T；新实例键必须完整、确定且预费。保留定义模块上下文，不在调用者上下文误解名字 |
| Types `helper_plan::Shape/order` | 当前只追 Unknown/Environment/直接 Package callback 字段，不能自然传递 `Store<T>` 和完整 T。要新增有限类型规划或独立实例计划，处理 child helper 的具体 T 依赖和递归拒绝 |
| Types 参数准备 | 值实参按现有顺序求值、暂存一次；从实际类型提取绑定不能再次执行 callee/实参。错误参数准备继续结算真实已持有责任 |
| CT source/materialize、driver/Core 构造、metadata preflight | 复制/重建新模板信息；未冻结 CT 功能保持关闭；新 binder/selector/Type 树在 clone/hash/keys 前计费 |

为了保持旧非 callback Store 预算，优先只对确实含未解析 callable 依赖的普通 helper 延后特化；原来已完全具体的普通 Store helper 继续原路径。若决定把全部 StoreElement 模板统一移到 Types，必须明确列为更大迁移并重新核对旧精确预算，不能默认旧收费仍相同。

## 具体 TypedIR 和独立核边界

现有 `Type::Package`、`Type::Closure`、`OwnedStore`、`Make/TakeErrorUnion`、`Call` 和 `PassingMode` 能表达完全具体的 empty/reserve/push/pop 签名与函数体。这里缺的是进入它们前的类型绑定，静态审计没有发现新增运行时 opcode 的必要性；也没有运行证据证明所有组合已经可用。

Analyze `helpers::check` 只把直接 Closure/Address<Closure> 参数作为 callback 候选；Verify `helpers::parameter_type` 同样如此。完整 Package 或 Store 普通 helper 应继续走资源摘要、描述符来源、实际 Store 前缀和所有权核。不能扩大 200 的双 Closure 构造例外来绕过普通 T helper。

独立 Analyze/Verify 仍需从最终具体元素、资源定义、真实捕获、参数模式、push/pop 错误载荷、来源/清理重建。新增 Types type binder 不成为这些检查的证书。201 中非零布局/捕获环/父窗口的边界仍保持，直接 Store 或含 Store Package 捕获不因此开放。

## 下一阶段必须保留的正反例（本审计均未运行）

| 配对 | 必须区分的事实 |
|---|---|
| helpers01 两原源逐字成功；同源 builtin 对照 | 实际补上普通函数边界，不能靠内联替代 |
| empty 的明确 Store 结果、显式类型应用、无上下文反例 | 返回上下文和显式 T 的真正约束；歧义仍拒绝 |
| `push(Store<A>, B)`，同布局但不同名义/普通 T，不同 ClosureId | 同一个 T 的多处约束必须完整一致 |
| 跨函数 reserve/push/pop、create/give、两次真实输入、失败 push 后回收载荷 | 特化后的具体类型不抹掉独立责任和失败转交 |
| 普通 helper 内结构失效、lending 活动窗口、错误 callee 实参准备 | 父 Store loan 不能因为多一层 Call 而失效或丢失 |
| 声明模块相同名、alpha 改名、phantom 普通 T、调用顺序交换 | 词法 binder 与完整实例键独立于文本拼写/生成顺序 |
| 未用错误泛型函数、非法 opaque 字段访问、递归/深链/多实例 | 定义时能力及实例计划有限，不只检已调用的方便例子 |
| raw Core 错 binder/缺约束/错误路径，具体 raw/Wire 错元素/责任/名义头 | public 元数据不授信；最终独立核仍拒绝 |
| 新界 N/N−1/fresh、原普通 Store 精确预算、196/197/199/200/201控制 | 费用先于遍历/复制/类型键与计划分配，默认额度不增加 |

应先冻结普通类型绑定的 Core 表达，再按 Expand、Types/规划、独立核/消费者分工实施。此前 F/G State 审计保留为另一项：它涉及一个 State 的多个 callable 声明模式来源，不能因为本片绑定完整 T 就宣称已经解决。
