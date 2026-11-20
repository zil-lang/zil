# 语义内核与证书核验架构合同

本合同定义编译器中段的终态职责。它取代“Analyze 与 Verify 各自重建同一安全事实”的旧约束（见 `_zil_design.md` §19 修订）。语言规则仍以 `_zil_design.md` 正文与具名附件为准；本合同只规定这些规则在编译器中的唯一承载位置和核验方式。

## 1. 原则

1. **规则唯一。** 每个操作的结构、类型约束、状态转移、安全前提和资源效果只有一份权威定义。
2. **职责解耦。** Analyze 负责求解与诊断；Verify 负责核验候选证明并授予安全准入。
3. **独立性的含义。** Verify 不信任 Analyze 的结论，只依据原始 IR、规范语义和可核查证据确认安全。Verify 不要求用第二套算法重新推导相同事实。
4. **授权唯一。** 只有 Verify 可以构造 `VerifiedModule`。缓存命中、反序列化、指纹相同或 Analyze 的成功结果，都不能单独构成授权。
5. **共享不等于免检。** 共享规则消除漂移，但不消除规则自身的错误。语义内核必须由不调用内核的独立测试约束（§8）。

## 2. 层次与依赖

| crate | 职责 | 禁止 |
|---|---|---|
| `zil-ir` | `body` 模块：类型与名义定义、规范 Place、合并后的指令集、OpSpec、证书与 SourceMap schema、Wire 编解码 | 求解、诊断策略、授权 |
| `zil-semantics` | 状态域、≤ 与合流、转移规则（多后继、分支条件）、安全前提、边语义、出口义务、调用合同和区域约束的合法性 | 不动点调度、诊断流程、`VerifiedModule` |
| `zil-typeck` | 名字解析、类型推导、泛型实例、类型约束；降为 body IR；产出规范 SourceMap | 安全判定 |
| `zil-solve` | 状态不动点、区域求解、来源推断、证书生成、完整诊断 | 构造授权 |
| `zil-check` | IR 结构校验、证书检查、Source→IR 保真核验、CT 原定义核验、唯一授权 | 信任候选结论 |
| Lower / Backend | 只消费已授权的 body IR 与 OpSpec | 自带准入白名单 |

`zil-check` 的 `fidelity` 与 `ct_original` 模块可以依赖 `zil-syntax`，以便对照原始源码独立核验；其他模块不得依赖它。

## 3. Body IR

- **类型。** 类型驻留在表中，用 `TyId` 引用。名义类型通过 `AdtId` 指向唯一的 `AdtDef`：类别为 Record/Package/Resource/Enum，含变体和字段声明（每个字段带类型与拥有标记）。`Managed`、`Desc`、`Ref{region, mut}`、`ErrorUnion`、`Tuple`、`Array`、`Slice`、`Closure`、`Dyn`、`Task` 等都是类型构造子。区域是一等参数。
- **字段身份。** `FieldRef = {adt, variant, field, args}`，表示一个字段声明。名字只用于诊断。
- **位置身份。** `Place = Root + [Field(FieldRef) | Variant(VariantRef) | ErrorArm(bool) | Index(Operand) | ConstIndex(u32) | Deref]`，其中 `Root ∈ {Local, Capture(u32), Param(u32), Static}`。不同实例上的同一 FieldRef 是不同的 Place。
- **操作集合。** 按语义族合并为：Place 访问（读、拷贝、取走、写入、借用、描述、标签、丢弃、预留/提交/放弃替换）、值构造（聚合、变体、错误联合、资源头、闭包）、标量运算与转换、调用（`Callee = Direct | Indirect | Closure(mode) | DynSlot | Foreign`）、内建协议 `Intrinsic{id}`（Task、Registration、Mutex、Store、ByteBuffer、Sync），以及作用域与存储标记。旧 IR 中 Local/Address 成对、逐容器、逐类型的变体不再保留。

## 4. OpSpec

OpSpec 是 `InstructionKind` 上的一组穷尽静态方法，不在运行时构造对象：

| 部分 | 内容 |
|---|---|
| operands | 每个操作数的角色：Consume、Copy、Borrow{mut}、PlaceBase、Index、Callee、Arg(n)、Receiver |
| places | 指令作用的规范 Place 及其投影路径 |
| constraints | 只依赖声明与类型的结构约束：字段存在、字段类型允许该访问模式、结果类型规则 |
| effects | 静态效果，即该指令“尝试”做的操作：Read、Write、Init、Move、Borrow、Describe、Drop、ChangeVariant、Relocate、ReplaceInstance、EndLifetime、Create、Call、Protocol、Scope、Storage（对应 `_zil_design.md` §4.1） |

**覆盖规则。** OpSpec 与 `zil-semantics` 的转移函数对全部指令穷尽匹配，在模块级 `deny(clippy::wildcard_enum_match_arm)`。每条与安全相关的指令必须给出转移规则或明确的拒绝规则。只关心部分效果的检查器可以按效果类别显式忽略其余操作。

效果只描述尝试，不证明安全。安全由 §5 的转移与前提决定。

## 5. 语义内核

- **状态域。** 每个规范 Place 的初始化/移出状态；活动借用与描述（来源、投影、模式、依赖）；责任载体状态；协议状态；区域变量。基数分 One/Many，强更新只作用于 One（`_zil_design.md` §3.1）。
- **转移。** `transition(op, state, ctx) → Result<Successors, Violation>`。后继可以多于一个，并可带分支条件，例如容量满、错误分支、Pending/Ready。
- **边语义。** 正常、错误、取消、挂起、隐式清理和循环出口都是显式边，各自带边上的效果。
- **出口义务。** 每个函数出口必须结算全部责任、借用与协议状态。
- **调用。** 用 callee 摘要实例化调用合同；摘要本身是证书的一部分。
- **区域。** 内核给出约束的生成规则和约束成立的判定，不负责搜索区域解。

### 5.1 借用（loan）模型

- **loan 身份。** 每个 `Ref`/`Describe` 赋值位置产生一个 loan，记录 Place、种类（Shared / Exclusive / Desc）、是否锚定本帧存储（路径上没有 Deref），以及被借存储能否再保存地址。另设伪 loan `EXTERNAL`，代表本帧以外的存储。loan 表与局部活跃性只由 IR 推出，求解器和核验器各自独立构建，结果相同。证书只能用创建位置引用 loan，不能改写 loan 的 Place 或种类。
- **携带关系（may 事实）。** 记录每个局部可能让哪些 loan 保持存活；合流取并集，`⊑` 为逐局部的子集关系。参数和捕获只要类型可能含地址，就携带 `EXTERNAL`。
- **存活判定。** 先取活跃局部，再闭包加入“被已保留局部的 loan 所借的局部”，得到保留集。loan 存活，当且仅当保留集中有局部携带它。每条指令之后、每条边上都按此修剪。
- **冲突判定。** 只比较同一 root 上相交的 Place。跨 root 的别名一律经由地址所携带的 loan 体现，而每次创建 loan 都要与全部存活 loan 核对。读、共享借用、描述只与 Exclusive 冲突；读 tag 还与可写 loan（Exclusive、Desc）冲突；移动、写、释放、独占借用、存储开始或结束与任何 loan 冲突。
- **流动。** 写入值所携带的 loan 流向被写存储：
  - 写整个局部是强更新，写局部的部分是弱更新。
  - 经地址写入时，流向该地址可写 loan 的锚定局部；若可达 `EXTERNAL`，不得写入锚定本帧的 loan。
  - 调用时，实参 loan 流入其可写、可存地址的锚定存储。任一实参只要可达 `EXTERNAL` 且类型可存地址，就不得同时传入锚定本帧的 loan，直到 P4 的区域合同提供更精确的摘要。
- **事实失效。** 创建可写 loan 时，被借 Place 上的 variant/tag 事实立即失效；loan 存活期间，冲突规则阻止重新建立这些事实。
- **出口。** 返回值的 loan 闭包中不得含锚定本帧的 loan。

## 6. 证书与健全性义务

证书由 Analyze 生成，经 Wire 传输时一律视为不可信。内容包括：若干块的入口不变量（默认取汇合点与循环头，也可以给出任意块）、区域解、函数摘要与调用合同实例化、来源声明，以及取消与协议所需的额外证据。

Verify 必须逐项核验：

1. 入口不变量覆盖初始状态。
2. 每条指令的转移都满足安全前提。
3. 对所有边 e，均有 \(F_e(I_{src}) \sqsubseteq I_{dst}\)，包括隐式清理、错误、取消、挂起和循环出口边。
4. 所有出口义务都已结算。
5. 不可达只能由语义推出，不接受断言。
6. 调用效果符合摘要；递归 SCC 按归纳合同核验。
7. 区域解满足全部约束。

证书缺项、出现 Unknown 或任一核验失败，结果都是 SafeReject。证书的字节数、解码与核验成本全部计入预算。

## 7. Source→IR 保真

IR 安全与 Source→IR 保真是两个独立合同：

- typeck 是规范 SourceMap 的唯一生产者。SourceMap 只是候选证据。
- `zil-check::fidelity` 用原始 AST 和声明事实（例如源码中的字段名经声明解析得到的 FieldRef）重新解析关键关系，再与 IR 中的 Place 和操作比对。关键关系包括字段选择、求值顺序和资源操作。
- solve 不重建原始语义。

## 8. 内核的独立约束

以下测试资产不得调用 `zil-semantics`、`zil-solve` 或 `zil-check` 的实现：

1. 独立手写的语言语义反例。
2. IR 恶意变异（手写，或按 IR 结构枚举）。
3. 合法程序的等价变换，裁决必须不变。
4. 原生执行对照：Rust oracle 加清理观察。

## 9. 迁移

- 新管线由 `zil-typeck`、`zil-solve`、`zil-check` 和 body IR 构成。旧 `zil-types`、`zil-analyze`、`zil-verify` 冻结为参考，并作为基线收据的来源，不再修改。
- 新管线在冻结基线上不回退之后，整体删除旧 crate。两者之间不建桥接层，也不做零分歧差分。
- 同一语义族只由一套生产实现承担。规范、policy、接口声明随代码在同一交付中更新。
