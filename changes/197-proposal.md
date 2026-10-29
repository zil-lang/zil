# 197 预检提案：普通 Copy 数据与单一拥有 callable State

状态：只读预检完成，建议登记下一片；本提交只有提案和证据，没有生产、规范、Task 或既有测试修改。目标是可复用控件状态 `package State<T,F>`；不扩展 196 的多个运行时环境角色，也不宣称 1.4 或 ZUI 全部前置条件已经完成。

## 实际依据与执行边界

静态源码和全部检查使用 194 的最终 clean 源码 `13df78f532f63874b61d56dc6067884dbac0fe9d`。实际 CLI 是 `targets178/author194/debug/zil`，大小 25,447,872 字节，SHA256 为 `029543198554cd97118e2f4bc4a5b75ef0767ea5cbbf872f5e4fa075439ab119`。已核对该哈希及 `run-logs/194-author/final-build01/command/command.json` 的空 dirty、真实 cargo build 成功记录。本轮没有重建编译器，没有把后续证据提交当作 CLI 来源，也没有使用旧 195 CLI。

规范读取 main `1152dc0893b89f7f7bbb4d4fabafcaf321b3cff7`，`_zil_design.md` SHA256 为 `48641079ef2ecc84aa50a0a9eafd83ae844f0d3de7924fcc8cb3e2fef3f9db5f`，Task194 注册 normative hash 为 `6488fa9f20121c4268f13153c6daecbfd20da5eb5e8aa86630620413d41edadb`。§18.1 的 194 profile 明确保留 `State<T,F>` 混合拥有形状的关闭边界；本次观察与该边界一致。

证据在 `evidence/preflight197/`，包括原输入、命令、完整 JSON、实际阶段产物、构建来源收据和文件 SHA256。累计 **24 次输入检查，4 次 Success、20 次 Unsupported，verification_runs 合计 4；native executed=0**。每条命令请求检查到 Verify，表中的阶段是实际停止阶段。没有执行本片实现、原生消费者、性能测量或全仓验收。

首轮恰好 12 条。9 条把直接 `T:Copy` 写在 package 声明上，先触发现有 aggregate bound solver 的 Unsupported。这是可解析语法上的真实前置边界，不能把它当成无意义语法错误，也不能声称已检查混合 State 的后续语义。另一个 `State<F>` 控制漏写既有 Package profile 要求的 `manages` 字段，是夹具遗漏。集成方批准仅更正原 12 个位置再跑、累计最多 24 次；首轮原字节全部保留。第二轮改用无 bound 的 `package State<T,F>`，Copy 约束只放在函数泛型上，并补齐遗漏的 manages 和构造初始化。先实际确认 01 / 02 两个控制成功，才执行其余 10 条。

### 首轮：未覆盖的内容明确保留

| 位置 | 实际结果 | verification_runs | 解释 |
|---|---|---:|---|
| 01 | Unsupported / Types：Package 至少需要一个 manages 字段且无自定义 whole Drop | 0 | 本条遗漏 manages；不归为 State<F> 能力回退 |
| 02–04、06–10、12 | Unsupported / Expand：`generic bound, lifetime or const parameter requires its semantic solver` | 0 | 合法 aggregate bound 语法仍关闭；未触达后续 mixed State 检查 |
| 05 | Unsupported / Expand：`Copy data and owning callable State combinations are not implemented` | 0 | 直接观测到 194 的拥有 State 与普通 T 组合门 |
| 11 | Success / Verify | 1 | 有 manages 的 State<F>、真实 Copy record 与 callback sibling 地址控制 |

### 更正轮：最小混合 State 路径

| 位置 | 原始输入目的 | 实际结果 / 停止阶段 | verification_runs |
|---|---|---|---:|
| 01 | State<F> 的 callback 与具体 i32 字段分别建立可变地址 | Success / Verify | 1 |
| 02 | 不含 callback 的 Cell<T>，函数 T:Copy，分别 i32 / i64 | Success / Verify | 1 |
| 03 | State<T,F> 工厂返回与实际 i32 字段调用 | Unsupported / Expand：escaping callable result | 0 |
| 04 | 未使用的 State<T,F> 输入函数 | Unsupported / Expand：callable 参数只接受单一 F、&F、&mut F 或 State<F> | 0 |
| 05 | 原 State<F> 加一个额外函数 T:Copy | Unsupported / Expand：Copy data 与 owning State 组合 | 0 |
| 06 | F、T 反向声明；不保存 T 的 phantom State | Unsupported / Expand：escaping callable result | 0 |
| 07 | 同一 factory F，T 不在 F bound 中，分别 i32 / i64 | 同 03 | 0 |
| 08 | 同一 F、同布局名义 struct A / B | 同 03 | 0 |
| 09 | main / model / state 三个真实模块，公开 Copy record | 同 03；诊断精确位于 state 模块 | 0 |
| 10 | State<T,F> 中 callback 与 T 直接 sibling 借址，另有 ByteBuffer 责任字段 | 同 03 | 0 |
| 11 | State<F> 与具体 Copy record sibling 地址，原输入不变 | Success / Verify | 1 |
| 12 | 保有 value 字段共享窗口，再建立同字段独占窗口并调用 callback | 同 04；仍被前门遮蔽，不能记为已证明的借用反例 | 0 |

两轮分别为 1 / 11 和 3 / 9，结果表没有把同一个 11 控制的重复运行算成新增能力。所有 Unsupported 的 verification_runs 都是 0。06–10 当前既未创建混合 State 的实际实例，也未检验其身份、跨模块执行或资源清理；这些是后续验收义务。

## 静态根因

### 1. 当前拥有 State 的语法分类只有一个类型实参

194 冻结的 `crates/zil-expand/src/generic_callable.rs::state_type` 只识别 `State<F>`：名义类型必须有恰好一个类型实参，且该实参是直接 F。`State<T,F>` / `State<F,T>` 因而不会被标为 owned_state。工厂返回走到 escaping callable result 门，输入走到只允许直接 F / 地址 / State<F> 的门。已经被识别为 State<F> 的函数若还含 T，则另有 `owned_state && function.generics.len() != 1` 的明确关闭门；05 直接复现它。

`expanded_template` 在泛型展开后读取实际 Package 字段恢复 F 的位置，并支付扫描费用；生成名称本身不是拥有或可调用资格。新入口仍应根据实际 Package 形状证明 callback 字段，不能只扩一个字符串匹配或把所有含 F 的任意容器视为 State。

聚合声明上的 `T:Copy` 则更早被 `generics.rs` 的模板参数 gate 拒绝。194 的直接 Copy bound 入口限普通函数；本片最小实现可以沿已经通过 02 的无 bound package 加函数 bound 路径，不必顺带开放所有 aggregate bound solver。若未来要求 `package State<T:Copy,F>` 本身承担约束，应另行登记定义期、字段与实际实例的完整规则，不能通过略过 gate 达成。

### 2. T 的推导不能跳过整个 State 参数

194 的 `generics.rs` 在单 F + Copy T 调用分支中，从普通实参或明确结果上下文推导 T，随后对完整函数和 F bound 代入 T、只保留 F。它跳过 `callable.index` 的整个实参，这对直接 F 参数合理，但在 `run(state:State<T,F>)` 中会跳过唯一可供推导 T 的名义外壳。

新入口需要从实际 State 的原始名义应用中约束普通数据 T，同时把 F 留给既有真实 ClosureId 实例化。不能从可见 Fn 签名、闭包布局或字节宽度猜 F 的身份，也不能为获得 T 重新求值 state。`generic_enum.rs::CallHints` / `symbolic_result` 当前刻意只向后传播词法 F；opaque factory 的可见签名不是环境实例键。这一原则必须保留，并补齐 State 外壳中的普通类型信息。

`original_type` 在 13df 中已经递归规范化完整名义参数，`instantiate` 的键包含模板及完整规范化实际类型。后续必须复用这一修复，不能把 `Box<i32>`、`Box<Box<i32>>`、同布局 A / B 或不同模块同名类型合并。完成 T 部分特化后沿正常 lowering 分配新的 Core 节点，现有 `(ModuleId,NodeId)` 模板身份仍可复用；无需为了本片改成多个 callable 环境角色。

### 3. 保留普通 T 的名义族身份是实施前置

Types 现有 `callable_fields.rs::environment` 对比 Package 的 `nominal_base`、字段名和所有普通字段类型，再取得唯一实际 F。`specialize` 把直接 F 字段替换为 ClosureId，并用 `nominal_base + :closureN` 命名具体 State。当前 `nominal_base` 对 `$generic_..._<数字>` 会剥掉最后的实例尾号。这服务于旧 State<F> 的不同符号 F 展开后绑定同一实际 F。

如果只删除 mixed State 的前门，而仍把所有 `State<T,F>` 展开成同一 `$generic_State_<全实例尾号>` 家族，尾号剥离会连普通 T 的区分一起去掉。T 真正存于字段时，普通字段比较可能继续挡住某些错误；06 的 phantom T 没有这样的字段区分，不能靠同布局来恢复名义身份。这里是由冻结代码得出的**未来实现风险**，不是本轮发现已接受程序碰撞或安全漏洞；当前相关输入全在 Expand 被拒绝。

最小方向是明确两层身份：`原始 State 声明 + 完整规范化普通 T` 决定数据名义族，具体 F 再由真实 ClosureId 决定。Expand 可以先建立保留 F 的数据特化名义族，使后续 F 符号展开的可归一尾号不再承担 T 的身份；另一种私有实现是给现有名义字符串编码可明确解析的数据族信息，在 Types 仅移除 F 占位展开部分。实现必须选定并验证一种有界方案，不应把任意生成名字当能力证明。

同一 T 的别名、不同函数内 F / G 词法变量名或不同调用次序必须回到同一数据族；同一 F 配不同 T 必须区分，即便 T 不在 callback 签名中或完全不存储。名义族不能按 factory/helper 函数名随意分裂，因为不同合法构造路径应能传递同一个 State 类型。所有键构造、复制、原始类型恢复和字段扫描需在现有预算中预先收费。

## 建议实施范围

首先只开放一个拥有 State 名义容器、一个 callable 类型 F 与一个直接函数 `T:Copy`；类型参数次序任意。典型定义是无 bound 的 `package State<T,F>{manages owner:ByteBuffer;callback:F;value:T}`，工厂与消费函数各自写明 T:Copy。State 本身保持拥有 Package，不因为内部 T 可复制而获得 Copy。F 的实际环境须满足现有自含、能力、所有权和清理规则；直接资源字段、取出/替换及 whole transfer 继续按原 profile。

补齐原 State<F> 搭配函数普通 T 的相邻组合，使 05 不再成为意外残留门。混合函数定义即使未调用也必须经过 Copy 能力与实际 State 字段符号检查；Copy 只能授权复制和普通传递，不能凭 T:Copy 在定义中做 `value.n`、算术、任意 trait 或调用。

真实 State 绑定 F 后，Types 的旧 owned_state 路径会具体化完整函数，并按普通拥有函数检查真正字段调用；现有 Type::Package、ClosureId、参数具体类型和 cleanup 表示有机会原样承载本片。**目前没有证据要求扩大 public schema24、Wire、JSON 或 ABI。**这是最小表示方案，不代表独立 Analyze / Verify 的新增输入验收已经完成。若 raw 反例暴露具体缺失，先报告反例与最小文件，不能用共享前端判定取代两阶段独立核验。

192 的实际 sibling 地址路径需要保持：`&mut current.callback` 与 `&mut current.value` 只在真正不同字段上得到分离，T 已经具体化为有限 Copy 载荷。12 的共享 value 窗口仍在后续使用时，不得同时授权同一字段的独占 callback 实参。资源/Managed/Desc/Address 不能通过 T:Copy 获得数据类型资格；借用整个 State、让地址逃逸、任意资源字段借址都不由本片增加权限。

190 的普通 helper Flow、195 的 break / continue 及原有 State 的循环行为继续作为相关邻接；本片不改变其图语义或控制流状态。FnOnce 的实际消费后复用和仍有环的调用规则保持原限制，不以 State 包装隐藏消费。两个 F 实参、F / G 多角色、容器中多个独立 callable、动态擦除与单环境混合调用模式仍属于另外的范围，不能随本片进入。

## 最小建议写集与最近验收

下列是请求集成方登记的建议，不是本预检已经获得的生产写权限。

| 路径 | 必要作用与限制 |
|---|---|
| `crates/zil-expand/src/generic_callable.rs` | 精确混合 State 语法分类与单环境准入；展开后仍核实际 Package 字段 |
| `crates/zil-expand/src/generics.rs` | 从 State 外壳推导 T、部分特化和普通 T 名义族；保留完整原始类型键与单次求值 |
| `crates/zil-expand/src/generic_enum.rs` | CallHints / symbolic_result 的普通数据部分传播，不把 opaque F 签名作身份 |
| `crates/zil-expand/src/generic_check.rs` | 仅混合 owned_state 的定义期字段/嵌套转发及普通 bound 规则；不重写 195 循环出口 |
| `crates/zil-expand/src/generic_state.rs` | 可选私有模块，承载有界 State 外壳、类型参数位置与名义族辅助，在上述既有模块内注册 |
| `crates/zil-types/src/callable_fields.rs` | 配合完整数据族的 nominal 匹配与具体 F 绑定；不得只按字段布局放行 |
| `crates/zil-driver/tests/impl197*` | 定义期、实际源面、类型身份、raw / Wire、预算与失败后 fresh 请求 |
| `examples/mainline/generic_state197/**`、`tools/language/197/**` | 四模块消费者和实际 O0 / O2、分配失败复现 |
| `changes/197-*`、`evidence/impl/197/**` | 真实实现收据、独审所需上下文；执行文件排除 |

优先不改 generic_trait_bounds / generic_copy、Types helper 缓存或多角色合同，不改根资源借址、Analyze / Verify / LLVM 生产文件。若不能用上述身份方案完整表示，停止该具体扩展并提交可复核反例，不扩大默许写集。和 196 如有同文件功能块相交，由集成方按真实提交协调，不能整文件覆盖另一片。

实施后的必要验收包括：

- 原 03–10 更正轮输入逐字贯通；04 的未调用定义同样验证。工厂与消费函数间 T 只出现于 State 输入时必须正确推导，F / T 声明次序都覆盖。
- 同一实际 F 分别 i32 / i64、名义 A / B、不同模块同名类型、完整嵌套类型、phantom T；相同 T 的别名与重复构造应合流。核对真实 Core / Typed 身份、返回 Package 与 Wire roundtrip，不能只看编译成功。
- 构造实际 wrong-instance 调用/返回/名义包篡改，分别运行 raw Analyze 和独立 raw / Wire Verify。最近反例必须使用真实不同 T，尤其相同布局/phantom T，不能只改一个无语义名字期待安全拒绝。
- 10 的不同字段地址成功，12 在准入后由真正冲突规则拒绝；最近最后使用后复借成功。同一 callback 与普通 T 只复制数据，不复制 Package 或环境清理责任。
- 原来直接 T:Copy 的非 Copy 实参、未声明 Copy、未调用坏模板、伪造 Copy impl 继续拒绝。aggregate bound 的首轮边界若不在登记范围，保持明确 Unsupported。
- 四模块控件状态消费者以真实 stdin 驱动零次、多次 callback 和退出，核对拥有字段/环境的零调用和正常清理、Copy Error 路径、每个实际分配失败点。O0 / O2 的实际 stdout、退出码、分配/释放、live=0 留证；未执行平台不宣称通过。
- 新增名义键、原始类型遍历、字段扫描和特化工作做 N / N−1 及失败后 fresh 请求验证；旧 194 Copy 函数、State<F>、192 sibling 与相关 190 / 195 邻接保持，默认额度不提高。

此预检没有改旧断言。如果后续精确发现旧测试刻意断言该组合 Unsupported，应逐条向 root 报原输入、原阶段和唯一迁移内容，登记后再改；不能为能力打开批量删除负例。

## 四目标与阶段裁决

| Rust 对照目标 | 本轮事实 | 尚未证明 |
|---|---|---|
| 同类安全 | 现有三个控制在 Verify 通过，关闭门明确；找到数据名义身份和字段权限的具体义务 | 197 新准入及完整语言安全；12 尚被前门遮蔽 |
| 运行与内存 | 没有新增执行路径或运行时机制，本轮 native=0 | 消费者真实清理及相对 Rust 运行/内存成本 |
| 表达与维护 | 可复现单 F + Copy T 仍不能组成可复用 owning State | 新组合的完整实现与产品维护收益 |
| 编译成本 | 没有改预算或编译器，提案要求保留有界计费 | 新组合成本与任何编译提速 |
| 至少一项净突破 | 只完成可复核预检和最小后续方案 | 产品级可重复净突破 |

建议先由 root 登记 197 的有限规范增量、基线、作者与写集，再实施并交非作者审核。完整语言 **NotSatisfied**、产品 **NotRun**、B0 **Doing** 保持；本提案不把 24 次预检或测试数量折算成产品终态。
