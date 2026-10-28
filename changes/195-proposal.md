# 195 提案：同步 runtime helper 的 break / continue

状态：**只读预检与待登记的最小实施提案**。本提交没有改变接受性、生产代码、公共 schema、默认额度或工单状态。190 的完整受测冻结仍为 `7491c212d392b12bb90f735b90f535066fa00a47`；其独立审核不依赖本提案。

## 目标与依据

为已经由 190 支持的同步 Fn / FnMut helper 补上真实 `while`、整数半开区间 `for` 中的 `break` / `continue`，包括嵌套循环、回调前后退出、每次循环的局部资源清理和最后一次借出使用后的退出。真实 CFG 中仍有环的 FnOnce helper 保持明确封闭，不能借 `continue` 跳过消费状态回边检查。

当前规范 `_zil_design.md` 已要求重复调用的真实前提、循环入口与每条回边义务、各出口的清理顺序；本片将已有普通控制语句组合进这一有限 helper 范围，不增加标签跳转、循环表达式返回值、借入逃逸或另一套资源权限。193 的显式切片入口和 194 的单 F + Copy T 研究不由本提案扩大。

## 预检身份与实际观察

原始 16 个源码、每次精确命令、JSON 报告、阶段、诊断、`verification_runs` 和普通程序的原生记录在 `evidence/preflight195/probes01`。`evidence/preflight195/PREFLIGHT.json` 给出汇总和逐文件 SHA256；执行文件没有收录。

实际 CLI 建于 `8644848b80f09a0507f47054830f56f1ef193c9d`，SHA256 为 `164ca2a261317c9db1c3e332aa359da765d6a9818c97cf99e1a21a74fb100a80`。其生产内容与 7491 相同，190 的 `source-and-consumer/SOURCE.json` 已记录逐字节身份；195 没有重建 CLI，不能把这些执行标成“新建的 7491 二进制”。预检时工作树提交为 `9679e3412a839311c930f42388d4501f6e7c7fbf`，190 生产没有未提交修改。

| 原始探针 | 实际结果 / 停止阶段 | verification_runs | 实际机器观察 |
|---|---|---:|---|
| 01 普通 while / break | Success / Verify | 1 | O0、O2 均 exit 0 |
| 02 普通 while / continue | Success / Verify | 1 | O0、O2 均 exit 0 |
| 03 普通 for / continue | Success / Verify | 1 | O0、O2 均 exit 0；索引正确步进 |
| 04 普通嵌套 for / 局部资源 | Success / Verify | 1 | O0、O2 均输出 `01A01B`、exit 0 |
| 05 现有 Fn runtime while | Success / Verify | 1 | 本轮未执行机器程序 |
| 06 Fn while / break | Unsupported / Expand | 0 | 未生成、未执行 |
| 07 Fn while / continue | Unsupported / Expand | 0 | 未生成、未执行 |
| 08 Fn for / continue | Unsupported / Expand | 0 | 未生成、未执行 |
| 09 FnMut + 显式 &mut i32 / break | Unsupported / Expand | 0 | 未生成、未执行 |
| 10 FnMut 嵌套 for / while | Unsupported / Expand | 0 | 未生成、未执行 |
| 11 回调后 break / 局部 Drop | Unsupported / Expand | 0 | 未生成、未执行 |
| 12 回调前 continue / 局部 Drop | Unsupported / Expand | 0 | 未生成、未执行 |
| 13 环境借出最后使用后 break | Unsupported / Expand | 0 | 未生成、未执行 |
| 14 保持旧环境借出后 continue / 再调用 | Unsupported / Expand | 0 | 当前语句门遮住后面的借用冲突 |
| 15 同类旧环境借出，无 continue | SafeReject / Verify | 1 | `E-BORROW-CONFLICT` |
| 16 FnOnce 调用后 continue | Unsupported / Expand | 0 | 当前语句门仍封闭 |

共 5 Success、10 Unsupported、1 SafeReject；4 个普通程序各 O0 / O2，**8 次实际原生执行、8 次符合预期**。06–14、16 的准确诊断均为 Expand `E-UNSUPPORTED`，消息 `this parametric statement requires additional symbolic checking`。15 的准确消息是 `access overlaps an active address window without its delegated authority`。

这些是有限预检结果。没有新 helper 准入、195 测试套或全仓测试通过的声明；也没有把 14 的 Expand 拒绝当作借用安全已经验证。

## 当前实现为何封闭

`crates/zil-expand/src/generic_check.rs` 的 `State` 保留词法局部、已移动位置、Store 拥有者、成功载荷、来源集合、失效集合和新鲜位置。`join_states` 已经对可能移动/失效及来源取并集，对新鲜状态取交集，并保留调用者的词法绑定；它不是只接受相同分支状态的简化检查。

但 `block` 的返回值目前只有一个布尔量，含义为函数返回导致正文无继续路径。`If` 和 `generic_check_enum.rs` 的 `enum_match` 只合并没有返回的分支。`While`、`for_range` 仅在正文没有返回时对最终状态检查回边不变量；正文返回时直接排除该状态。Break / Continue 因没有对应的符号控制出口而落入明确 Unsupported。

如果仅将两种语句当作空语句，退出后的源码将被错误检查为实际路径。如果仅把它们当作 `Return`，`continue` 之前对非 Copy 值或 Store 责任的消费会绕过回边检查，`break` 的消费也可能从循环后的合流中丢失；循环外的控制甚至可能错误满足非 Unit 函数的返回要求。因此不能通配放行或只增加一个匹配分支。

`crates/zil-expand/src/generics.rs` 已按词法环境展开 While / For 正文，并为 For 保留 190 的端点单次求值与宽度上下文。Break / Continue 没有需要替换的表达式载荷；这里的无操作分支不承担安全证明。本片没有预检证据要求修改该展开器或 194 的 callable 类型变量/约束识别。

## 现有公共表示能够表达

`crates/zil-types/src/lib.rs` 的普通循环上下文已有最近一层循环的 `continue_target`、`break_target`、scope 深度和 unsafe 深度。实际 Break / Continue 先结束相应 unsafe 区域和局部 scope，再生成 `Terminator::Jump`。清理使用既有 `CleanupExitKind::Break` / `Continue`、Drop / StorageDead / ScopeExit 指令。

While 的 continue 指向条件头；整数 For 的 continue 指向独立 **step** 块。step 在需要时执行一次索引加一，再回到条件头，端点不会重算。04 的实际输出同时验证最近一层循环的退出和每次内层资源清理：内层 0 的 continue 销毁 `0`，内层 1 的 break 销毁 `1`，然后才销毁外层 `A` 或 `B`。

schema24 的 `CallableFlowNext::Edges` 已能记录这些实际 CFG 跳转，Return / Trap 仍是独立出口。190 的 Types、Analyze、Verify 分别读取真实 Jump / Branch 并独立构造或核验图；不需要新增 Break / Continue 图标签或伪造有限调用次数。Analyze 的 scope 状态和 Verify 的 cleanup 状态独立检查活跃 scope、仍存活的 storage 以及退出后的控制边；Flow 声明不能替代这些检查。

**建议 schema24 保持原样**，不改 Wire、JSON、后端 ABI、opcode 或 190 的三套图语义。真正的前端继续目标由原生行为检验；raw / Wire 反例必须构造实际 scope、责任或合同不一致。只把两个同样要求 Jump 的清理标签互换，并不当然构成安全违规；不能据此写一个缺乏语义依据的拒绝断言。

## 建议的最小实现

1. 在私有符号检查器中增加有界的词法循环帧。每帧分别收集 break 出口状态与 continue 回边状态，目标永远是栈顶的最近循环。保存状态和增长集合之前支付既有 Budget 费用；保留原 AST 深度、节点和工作额度，不提高默认值。
2. 正文结果明确表达是否存在普通 fallthrough，函数 Return 仍独立记录。Break / Continue 只终止当前路径并向对应循环帧提交当前状态；循环外使用明确拒绝。If / Match 仅合并 fallthrough 分支，其它出口必须完整保留，不把混合 Return / Break / Continue 压成“全部函数返回”。
3. While 保留条件的入口不变量。正文普通 fallthrough 和**每个 continue 状态**都必须满足外层非 Copy / carrier / Store 回边不变量；break 状态没有回边，但必须与可能的条件为假/零次出口一起合入循环后状态。不能因某一分支退出而恢复成未消费的入口状态。
4. For 保留端点左到右各一次、真实整数宽度及不可变词法索引。回边检查使用进入正文前的外层责任状态；continue 的机器目标仍由既有 Types step 处理。循环后的词法绑定恢复外层，影子索引和正文局部不泄漏；已消费的外层 id、字段位置和失效来源不能随局部名称丢弃。
5. 出口合流沿用已有可能移动/来源并集和新鲜状态交集。若某种拥有状态不能用当前有限域安全表达，保持明确 Unsupported，不能清空状态或选择一个好分支。循环局部的新资源可以每轮建造并在出口销毁；它不能让外层旧责任在 continue 上被重复消费。
6. 本片面向已有 Fn / FnMut runtime helper 和相同普通泛型控制核。真实仍有环的 FnOnce helper 继续受 190 拒绝，包括零事件环；新控制语句不能把消费效果隐去。未调用模板也检查正文，不通过某个实例的 Copy 实参放宽无 Copy 约束的定义。一般 callable + Copy T 准入由 194 单独负责。
7. 现有 concrete Types / Analyze / Verify 继续证明真正 scope、borrow、初始化、Drop 和来源权限。13 应在真实最后使用后接受；14 必须在控制语句准入后仍由真实旧借出与后续调用冲突拒绝，不能把 continue 等同于借用自动结束。

实现时可以将循环帧、出口收集和合流放到 `generic_loop_control.rs` 私有子模块，保持 `generic_check.rs` 只负责正文分派和既有类型/状态操作。是否采用结构体或枚举承载正文结果由实现决定，但每条退出边的状态必须可单独核对。

## 最近反例与验收重点

实际最近反例是 14 / 15：两者均保有第一次 FnMut 环境借出的地址，然后再次调用环境并同时使用旧地址；14 仅多一个 continue 分支。开放控制后不能使旧地址失效事实消失。16 的 FnOnce 消费后 continue 也是必须保留的实际原始输入。04 是普通清理对照，11 / 12 是尚未准入的相同 helper 清理形状。

下列是**实施后的待执行要求**，没有在本轮 16 个预检之外冒充已经运行：

- while / for、Fn / FnMut，stdin 决定 0 / 1 / 多轮，回调前后 break / continue，嵌套最近循环，分支与穷尽 match 的混合出口。
- For continue 经 step 一次，索引不逃逸、不能写；端点求值次序不变，最大整数边界不溢出；continue 不能使有限循环原生挂起。
- 块内拥有资源在 continue、break、普通返回及 Copy Error 出口按既有顺序各清理一次；零次循环没有局部构造；拥有环境的零调用和有调用清理保持。实际分配失败只在包含分配的消费者中按真实点注入。
- 未实例化 generic 的循环外控制拒绝；无 Copy 约束的外层值消费后 continue 拒绝；外层 Store 责任/字段取走、替换和仍活跃观察在回边或退出后不能复活。后者先使用已有可表达源形状，不为测试新增权限。
- 13 的最后使用接受、14 / 15 的旧借出冲突拒绝；显式 &mut Copy 当前参数与环境窗口联合核验保持；循环 FnOnce 仍拒绝。
- 同一源生成的实际 Typed Flow 做 raw / Wire 变异：删除实际清理、留下活 storage、关闭错误 scope、改变 CFG 而保留旧 Flow 声明、删去调用或回边事件，都须命中相应独立门。合法 IR 变体不为满足负例计数而强行预期拒绝。
- 新状态复制和分支/循环出口集合的工作额度做 N / N−1 及失败后 fresh 请求核验；旧 190 Flow、ForRange 和有限 helper 的相关邻接保持。

`impl190_for_ranges.rs::unsupported_loop_control_is_not_treated_as_an_ordinary_statement` 的两个未调用原始模板目前刻意断言 Unsupported。若195正式准入它们，需要 root 另行登记此文件的精确迁移；保留原输入，改变为真实符号检查正例并配最近反例，不能删除旧用例。本提案没有修改190测试。

## 请求登记的最小写集

| 路径 | 用途与限制 |
|---|---|
| `crates/zil-expand/src/generic_check.rs` | 循环帧/正文出口、条件与回边、退出后状态合流；不改194模板签名准入 |
| `crates/zil-expand/src/generic_loop_control.rs` | 可选私有模块，只承载上述控制状态；在 generic_check 内注册 |
| `crates/zil-expand/src/generic_check_enum.rs` | Match 出口传播，保留现有模式覆盖与类型约束 |
| `crates/zil-driver/tests/impl195*` | 原始源码、真正机器行为、raw / Wire、预算最近反例 |
| `examples/mainline/loop_control195/**` | 真实多模块消费者 |
| `tools/language/195/**` | 消费者原生/失败观察和紧凑证据工具 |
| `changes/195-*`、`evidence/impl/195/**` | 实施与真实收据，执行文件排除 |
| `crates/zil-driver/tests/impl190_for_ranges.rs` | 仅上述原 break / continue 两输入的获准准入迁移；其余断言保持 |

本次只读交付路径是 `changes/195-proposal.md` 和 `evidence/preflight195/**`。如果出现必须修改 Types、Analyze、Verify、公共接口或借用/资源语义的具体反例，先向 root 提交真实源码、失败阶段和精确文件申请，不能用扩大私有符号检查范围掩盖缺失证明。

## 分阶段交付与四目标

先登记当前规范、基线、写集与195作者；随后实现符号控制状态和源面纵切，再冻结同一源跑原生清理、独立 raw / Wire 与必要邻接。有限作者结果交非作者审核，主线合流和正式全仓门仍由 root 承担。本预检不阻塞独立审核190，也不改变父任务 B0 的状态。

| Rust 对照目标 | 本预检已有证据 | 尚未证明 |
|---|---|---|
| 同类安全 | 旧持有借出反例仍由 Verify 拒绝；定位了每条控制出口必须保存的状态 | 195 新准入安全、完整语言安全 |
| 运行与内存 | 普通控制对照的 8 次 O0 / O2 行为与清理顺序正确 | helper 新路径、相对 Rust 运行/内存成本 |
| 表达与维护 | 找到真实 helper 缺口，可复用既有控制/清理表示 | 实现后消费者组合及维护成本优势 |
| 编译成本 | 提案使用现有预算，未改变默认阈值 | 新增状态工作量与任何编译提速 |
| 至少一项净突破 | 本轮仅完成可复核的定位和最小实施方案 | 产品级可重复净突破未证明 |

完整语言 NotSatisfied、产品 NotRun、B0 Doing 保持；没有运行时间、内存或编译速度采样，也不从测试数量推导产品终态。
