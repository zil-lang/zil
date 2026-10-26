# IMPLEMENTATION-0014：Copy 标量闭包的真实环境纵切片

本切片属于 B0 的可执行子集，不解除 T08/W1 的完整资源、借入、逃逸和高阶义务。输入基线与独立审核绑定由主树 `.task/impl/014.md` 记录。

## 接受性与表示

接受无捕获普通闭包、显式 `move` 的 Copy 标量捕获、局部具体环境和只读 `Fn` 重复调用。参数与结果仅 Unit/bool/i32/i64；有捕获普通闭包仍需要尚未实现的借入核验，不能自动改成快照。可变捕获、资源、Address/Managed/Desc、闭包环境捕获、返回/参数逃逸和动态调用仍明确 Unsupported。

捕获在闭包创建处按已解析词法绑定物化一次，后续推导不能重新读取外部变量。参数可由注记、正文与同一具体闭包的全部调用约束确定；冲突或无解必须诊断，不改变捕获策略或默认装箱。`move` 和 Fn/FnMut/FnOnce 是两个独立维度。

独立审核发现：闭包正文局部量的推导约束在生成 TypedIR 时丢失；首个无类型整数调用过早默认 i32；CT 在上下文收集之前把 `#(1)` 固定成 i32，导致操作数顺序影响接受性。修正为在原始 Core 停止点之后、CT 执行之前进行有界整函数约束收集，约束含闭包参数、结果、局部变量和全部调用；只有未约束整数在收集后默认。返回仍是无权限 Core，`#` 只参与类型关系收集而不被执行；CT 消费已解上下文，经 verify_ct/VM 实际执行，随后完整 types/analyze/verify 不省略。程序入口累计全图预算，无闭包函数保持原链路。不能对已经计算出的 CT 值重标类型来掩盖中间溢出。

## 同批公共接口变更

- 公共 `SCHEMA_VERSION` 与 raw transport `WIRE_VERSION` 从 1 升为 2；旧 wire 显式拒绝，不尝试猜测旧布局。CT 操作集不变，但统一 transport 版本升级。typed 指纹域为 `ZIL-TYPED-MODULE-V2`。
- Core 的闭包参数使用 `CoreClosureParameter { name, ty: Option<Type>, mode, span }`，闭包增加 `return_type: Option<Type>`。不存在用 Unsupported 文本作推导变量的私有协议。
- TypedModule/LirModule 增加 `closures: Vec<ClosureDefinition>`；每个定义具有规范索引 `id`、唯一入口 `function`、有序 `captures: Vec<Type>`、`parameters`、`result` 和 `kind`。
- TypedFunction 增加 `environment: Option<ClosureId>`。`Some` 入口只可通过匹配的 CallClosure 提供环境，不能普通 Call 或 native --entry 进入。
- 指令增加 MakeClosure、ReadCapture 和 CallClosure；所有 captures/callee/arguments 都是实际 SSA operands，进入支配、定义与预算检查。
- LLVM 按 ClosureId 发出具体只读值环境类型，入口隐藏首参传环境值。MakeClosure 构造环境、ReadCapture 提取字段、CallClosure 向已核入口传环境。局部存储为值存储；没有堆盒、RC、虚表或宏式正文替换。

环境 ABI 保留字段索引：Unit capture 用 `i8 0`，bool/i32/i64 分别用 `i1/i32/i64`；普通 Unit 参数和返回保持现有擦除规则。空环境是具体空 struct；所有环境按值传隐藏首参，局部对齐为字段的最大自然对齐（空环境为 1）。这只是 C1 后端表示，不规定未来资源环境或动态调用 ABI。

生产者：syntax/expand、types。消费者：CT source traversal、analyze、verify、wire/cache、lower、LLVM backend、driver/CLI、测试夹具。`is_b0_scalar` 保持原定义，closure 局部允许性与标量算术/比较/分支/CT 允许性分开。

## 独立核验与最近反例

verify 不调用 types/analyze。独立检查 ClosureId、函数与环境一一对应；capture/参数/结果确为受支持 Copy 标量；只有 Fn；入口签名一致；capture 索引与结果类型、构造字段数/顺序/类型、callee 的具体环境和参数/返回一致。空 Hold/Cleanup 来自受限结构检查，而不是信任空字符串合同。没有可写环境操作，也没有挂起路径。

必测反例包括：普通 Call/CLI 直入环境函数、孤立/重复/交叉环境身份、在普通函数 ReadCapture、越界或错类型 capture、错 arity、未定义/不支配的 capture 与 callee、Address/Managed 伪装 Copy 环境、FnMut 冒充 Fn、closure 值进入算术/CT、预算耗尽、LIR closure 表篡改及旧 wire 版本。源码正例包括运行时参数捕获、同站点不同环境值、重复调用、外部修改后的 move 快照、遮蔽、参数/结果推导与 Unit 混合。

## 迁移与证据

013 的语法/设计先独立审核合入；014 随后吸收该提交、更新权威任务摘要，适配 expand 消费者。已合入旧切片保留其当时规范哈希，不追改历史。正式全量由集成方运行，施工方仅限定测试；未运行或不支持不记 Pass。性能产品门不在本片改动。

新增 `tests/integration/closures.py` 是独立 Python 状态模型与真实源码/原生程序的有界差分语料，固定 seed，保留每例输入、源码/IR/机器摘要及 O0/O2 输出；两个实际编译的捕获时序变异必须以值差异被检测，编译失败不算检测成功。它不是 Rust Roam 的替代实现；既有 Rust 原生状态路径门继续运行。集成时将该子集检查作为必需回归，不把 C1 子集通过改称完整语言或产品通过。

审核还发现新约束层遗漏既有 CT Unit 相等/不等关系，以及 raw Type 深度和 exports 配额在克隆/收集前未生效。修复保留 CT 既有操作域，运行期 Unit 比较仍按原类型门拒绝；所有 Type、完整正文及 metadata 在递归克隆或收集前有界检查，公共 check/check_program 同样先经过预检。修复前最小失败和独立复验保留，不能把后续 Verify 能拒绝当作前置配额失效的理由。源码 oracle 固定使用本轮构建的编译器路径与摘要，并核对开始/结束源码、dirty 状态和 harness 身份，避免继承旧 ZIL_BIN 伪造当前提交的成绩。
