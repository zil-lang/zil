# 194 提案：有限 Copy 数据泛型与单一 callable helper 组合

状态：只读预检完成，建议正式派工；本提交不修改生产。目标是可复用控件的数据类型与回调类型组合，不开展编译速度专项，不宣称完整 1.4 或 ZUI 前置条件已经全部完成。

## 依据与真实身份

规范读取主树快照 `df4546b15d98647a0a677aa9ac279b55868bfe46` 的 `_zil_design.md`，SHA256 `d66bf6d16f2c1adecc4fe8e03d4898560acc88e6406e798c1c13f424287f7f64`，注册 normative hash `d20398d8a0291e1e8eef74bf74c557e992fa176e7e5abe09f924071fd1cc2a25`。主要约束来自 §9.3–9.4 的实际环境/调用模式，§16.1 与 §18 的定义期符号检查和结构能力，§19.3 的声明/实例身份分离，以及 §19.6 不按同尺寸合并语义不同的实例。

19 条真实源面观察使用已冻结 193 CLI，实际 clean build source 是 `f594c57665aaf34d0c1d54df11d5047d4d042f0d`，CLI SHA256 `3830f924b7fa1223aeec88ecfd406f0a3012095ebd5be42a10aae0a7357ae33e`。构建命令、空 dirty 状态、现存最终 Linux 消费者记录中的 CLI 哈希都已核对。它不是上述 main 快照，也不是本证据 worktree 的源码。两者相关 Expand 文件及 public IR/wire 内容相同；Types 的 193 slice 地址增量和 main 的 192 Package 借址增量有实际差异，已按文件保存两个快照的 SHA。

证据位于 `evidence/preflight194/`：`IDENTITY.json`、原构建收据、每条原输入/命令/完整 JSON/阶段产物、`SUMMARY.json` 和复现脚本 `run.py`。执行 19 条，3 条 Success，15 条 Unsupported，1 条 SafeReject；累计 `verification_runs=3`，native executed=0。未运行性能测试、原生消费者或 194 实现。本次源码探针没有重写失败、没有把前端拒绝记成 Verify 安全核验。

## 观察矩阵

所有请求均要求停在 Verify；表内阶段是实际停止阶段。代码/完整诊断见每条 `stdout.json`。

| 编号 | 原始输入目的 | 实际结果 / 停止阶段 | verification_runs |
|---|---|---|---:|
| 01 | 普通无约束 T 单次转交，分别 i32/i64 | Success / Verify | 1 |
| 02 | 现有单 F、`Fn(i32)->i32` | Success / Verify | 1 |
| 03 | 独立的 `fn id<T:Copy>(x:T)->T` | SafeReject / Expand：`E-GENERIC-ARGUMENT`，Copy 不是已声明 nominal trait | 0 |
| 04 | 原审计 `T:Copy,F:Fn(T)->T` 输入，逐字保留 | Unsupported / Expand：只实现一个 callable 泛型参数 | 0 |
| 05 | 调换声明顺序为 F、T | 同 04 | 0 |
| 06 | 同 ClosureId、F 签名不含 T，extra T 分别 i32/i64 | 同 04 | 0 |
| 07 | 同 ClosureId、同布局但不同名义 struct A/B | 同 04 | 0 |
| 08 | T 与 F 经 outer→leaf 同模式转发 | 同 04 | 0 |
| 09 | `FnMut(T)->T` 与 `&mut F` | 同 04 | 0 |
| 10 | `FnOnce(T)->T` 与拥有 F | 同 04 | 0 |
| 11 | 189 已有的显式 `&mut T` callable 实参 | 同 04 | 0 |
| 12 | 未使用的坏模板：仅 Copy 却做 `x+1` | 同 04；尚未到定义期能力判断，不记最近语义反例通过 | 0 |
| 13 | 未声明 Copy 的 T 与 F 组合 | 同 04 | 0 |
| 14 | 把拥有 Package 当作 Copy T | 同 04；尚未验证实际形状 | 0 |
| 15 | 原审计同一个 F 的两个环境参数 a/b | Unsupported / Expand：环境参数必须仅出现一次 | 0 |
| 16 | 原审计两个 callable 类型 F/G | Unsupported / Expand：只实现一个 callable 泛型参数 | 0 |
| 17 | 一个 F 的 Fn 父 helper 调用 FnMut 子 helper | Unsupported / Expand：嵌套 helper 需相同声明模式 | 0 |
| 18 | 同一具体 Fn 闭包分别传独立 Fn、FnMut helper | Success / Verify | 1 |
| 19 | Copy T 经已有 runtime while/Flow 反复更新 | 同 04 | 0 |

04、15、16 与 `evidence/audits/14/probes01/` 对应原输入相同。旧 `docs/language14/terminal-matrix.md` 的组合缺口确实仍可重现，但其“需扩合同参数索引和来源域”不能统一套到单环境 F 加普通 T：T 不增加运行时环境角色。

## 根因与最小实现路径

### 1. Copy bound 是真实前置，不能当作已经存在

`generic_trait_bounds.rs::declaration` 把 Copy 等结构能力列为不能由 nominal trait 制造的保留能力；普通 `T:Copy` 当前报未声明 nominal trait。`bounds` 对 callable 模板整体直接返回空表，`generic_check.rs::known_copy` 把符号 T 当作非 Copy。仅放宽 `generic_callable.rs::template_with_state` 的泛型数量限制不能得到正确实现。

建议增加有限结构 Copy bound：实例必须是当前支持的有限普通 Copy Value，按真实标量/tuple/固定数组/struct/enum/Copy 错误联合/受支持函数指针结构检查，保留名义类型。此片不以 Copy 为名接纳资源 Desc、Managed、Package、Closure 或 Address 类型参数。显式 `&T`/`&mut T` 仍由原地址规则处理，T 自身是普通 Copy Value；这不会给资源或地址赋予新的拥有/复制权限。更一般的 Copy 类别仍可明确 Unsupported。

定义期记录 T 的真实 Copy 约束，只允许依赖这一能力的复制和普通传递，不授予数值运算、字段、任意 trait 方法或 callable 能力。未使用模板也要检查。实际参数必须重新验证有限结构，用户空 trait/同名 impl 不能制造 Copy。普通泛型的这一 Copy 前置与 callable 组合共用规则，避免“同一 bound 只有多参数 helper 才有意义”。既有有限结构检查位于 `generic_store.rs::Shape::copy`，可复用/提取其有界遍历，但不能连带改变 StoreElement 的非零布局、拥有形状或计费。

### 2. 先在 Expand 特化 T，保留唯一 F 到 Types

首片保留一个 callable 类型参数和一个直接 `T:Copy` 参数，支持任意声明顺序。F 仍仅作为一个 F、&F 或 &mut F 环境实参；既有仅 F 的 State<F> 路径保持原准入，不借本片增加 State<T,F> 等混合拥有名义形状。Fn/FnMut/FnOnce 的原传递、清理、循环和同模式嵌套限制继续生效。

普通参数或明确结果上下文推导具体 T；不从一个未解析闭包的布局猜 T，不把无法推导的 T 留给任意具体调用蒙混通过。对具体 T 用既有 `original_type` 与 `type_key` 规范化，键包含模板声明及完整实际 T。把正文、实参/返回类型和 F bound 的参数/结果一起代入；生成只剩 F 的独立 retained template。`substitute_item` 当前不替换 function.generics 的 bound，必须显式补齐，不能只替换函数实参。

`generic_check.rs` 的 nested callable 分支现在比较尚未代入的 callee bound，且跳过普通 trait 参数约束；新路径必须先用实际符号代换比较 bound，再验证普通 Copy 参数的约束。未使用模板、same-mode 嵌套、普通外层泛型与多模块导入都要经过这条定义期检查。递归依赖检查与当前源深度/工作/实例限额保持。

### 3. 身份修复边界：不必先改 TemplateKey

静态阅读最初提出“AST 克隆可能保留 NodeId”的风险，进一步追踪后已排除该具体假设：语法 `Function` 没有 NodeId；`receivers.rs::function` 对每个展开后的声明执行 `self.id()`，分配 fresh Core NodeId。因此按 T 生成独立语法声明并正常 lowering 后，现有 Types `TemplateKey=(ModuleId,NodeId)`、`Instance=(TemplateKey,ClosureId)` 已能区分同一个 ClosureId 的多个 T。

必须避免的是把多个 T 仍留在同一个未特化 Core template，或者在 Core 层直接复用同一个 NodeId。当前前端拒绝所有这些组合，**没有观察到已接受程序发生缓存碰撞**。应以实例化位置和测试保证身份，而不是为了假设风险改公有 schema 或把签名哈希误当唯一身份。

最小路径继续用 `(模板, 规范化 T 参数)` 的 Expand 实例键；每个实际声明获得不同 Core 节点，Types 缓存仍局限同一请求。T 未出现在 F 签名时也参与键；相同布局 A/B 不可合并；透明别名应与相同实际类型合流。重复出现同一个 T 才可复用。跨请求稳定声明/CGU 的完整 §19.3 目标不是本片宣称已完成的内容。

### 4. 单一公开合同足够；其他组合分别处理

| 组合 | 当前表示是否足够 | 后续边界 |
|---|---|---|
| 一个 F 环境参数 + 普通 Copy T | 足够。T 在 Core 前具体化，TypedFunction 实参与返回仍携带具体类型；合同仍描述唯一环境的 Hold/Call/Cleanup。 | 本片实现；无需改 IR/wire/public contract 或增加 schema 版本。原 Analyze/Verify 独立核实际具体 IR。 |
| 同一个 F 的两个环境参数 | 不足以一般表达。ClosureId 是闭包类型身份，不保证 a/b 是同一运行时环境；两个参数可持不同资源/捕获。 | 需显式环境角色/参数来源和逐角色调用事件。不能把两个参数折成一个合同，即便一个示例传的是同一变量。 |
| F 与 G 两个 callable 类型 | 不足以一般表达。现有单 `parameter`/`closure` 和只记录 kind 的事件无法表明被调用的环境。 | 单独公开合同/IR/wire 设计与独立 Analyze/Verify 多角色实现。 |
| 同一环境跨 Fn/FnMut/FnOnce 混用 | Finite/Flow 的事件已含 CallableKind，数据布局未必需要新字段；但当前语义要求每次事件等于合同 kind，Analyze/Verify 明确独立拒绝其他模式。 | 需先定义模式适配、消费后不可再次使用和清理语义，经过公开合同语义审查，不能简单放松等值检查。19 条探针中的 18 仅证明分别实例化独立 helper 的已有适配。 |

## 建议派工写集与验收边界

最小生产写集建议：`crates/zil-expand/src/{generics.rs,generic_callable.rs,generic_check.rs,generic_trait_bounds.rs}`，新增 `generic_copy.rs`；仅当复用有限形状检查确有必要时定向修改 `generic_store.rs`，不改变其原资格。优先不动 Types 生产缓存、Analyze、Verify、LLVM 或任何 public schema。若事实证明必须增加生产文件，先向集成方报告新原因和范围。

新增 `crates/zil-driver/tests/impl194_copy_callable.rs` 与 `impl194_copy_callable_raw.rs`；`examples/mainline/generic_callbacks194/**` 四模块消费者；`tools/language/194/**` 有界复现/故障运行；`changes/194-proposal.md`、`evidence/impl/194/**`。普通 Copy bound 被正式开放后，`impl103_generic_aliases.rs` 中那一对仅用于“别名与直接写法同样拒绝 T:Copy”的历史断言需要明确迁移为同样成功，不能静默删掉其它失败断言。

验收应覆盖：原 04 不改输入贯通；同 ClosureId 分别 i32/i64、同布局不同名义 A/B、别名归一及重复相同 T；两个实例的 Core NodeId/Typed FunctionId/具体签名和 Wire roundtrip；wrong-instance 调用/结果/名义类型篡改分别由 raw Analyze 与独立 Verify 拒绝。原 12 必须真正到定义期能力检查拒绝，原 14 必须按实际非 Copy 资格拒绝，未约束 T 和用户伪造 Copy impl 不得通过。

功能组合需覆盖同模式有限嵌套及 Flow、显式 Copy 地址当前调用权限、借入/拥有环境与提前/Copy 错误退出清理。四模块消费者应让同一真实闭包处理不同 Copy 数据实例，在 O0/O2 正常及真实分配失败下核对唯一释放。所有阶段仍支付原逻辑预算，至少验证有界工作/类型深度/实例边界，不能提高默认预算。独立审查者使用最终 clean freeze；本 19 条预检只建立现状，不替代这些验收。

## 四目标与当前裁决

安全方面已识别不能偷渡 Copy 或合并实例的具体边界，尚未编写 194 生产实现；运行/内存成本没有比较数据；表达能力的预期增量是数据类型与单环境回调可复用；编译成本仅维持真实计费，不主张提速。没有完整 Rust 四目标合取或产品净突破证据。建议立刻登记以上有限范围并进入实施，而非在建议处结束。
