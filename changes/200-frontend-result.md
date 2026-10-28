# 200 前端与 Types 交付：同 F 双字段完整 State

本轮把原始双回调 State 输入从 Expand Unsupported 推进到完整 Verify 与实际 O0/O2 执行。作者最终受测提交为 `5c2fbc50135d6ca12ee0481153b954194ad39b9d`，包含已接受的 199 基底、200 两个独立安全核以及本轮前端修复。交付文档和摘要不改变该受测生产身份；正式全仓、非作者审核和主线接受仍由 root 登记。

## 身份

- 正式实施基底：`fff8ed85a10bb977cd7d71fa5d3a134b96107f0c`。
- 200 规范：`_zil_design.md` SHA-256 `13ea0962a2bf73893a0b03eac133456216856f3748152392c925e95028059c7d`。
- normative hash：`4402e04fd45551cbdd8dff92c4ac51af8d8ddd6f87296c17ce3690f55359bff3`；公开 schema / Wire 仍为 25。
- 最终 CLI SHA-256：`4eaf5291d867f0ebc64390f5ab3850ea6726570216ab688ae971c44e457a2fc9`；实际 `cli-build02` 来自干净的 5c2fbc50。
- 初次预检保持原 `bbdb967d` 源码和 f360 编译器身份。最终测试的三份文件与其逐字相同，没有把新成绩倒填到旧预检。

## 具体行为与实现

一个完整 `State<F>`、`State<T,F>` 或 `State<F,T>` 可以保存两个直接、自含、同 F 的回调字段。有限拥有构造可以按值接收两个同 F 实参；新增双输入入口的其余参数只允许有限 Copy 或直接 Managed。已有单 State 操作与普通 Copy T 的名义族、透明别名和 phantom 身份保持。

Expand 将“唯一 callable 类型”与“实际参数出现两次”分开处理。两个 F 的构造入口必须返回完整 State，不能把 State 加 replacement F、两个 State 输入或 F/G State 当作同一例外。混合 T/F 的数据族沿既有键和预收费；旧 `State<F>` 的 `state_family` 快速返回已恢复，没有为新功能给旧单参数族增加整字段扫描。

Types 复用一个 F 类型绑定与已有逐字段专门化。两个运行时环境各自准备一次，按原顺序提交。拥有环境先真实移动并暂存；结构 Copy 环境先复制到各自独立的真实临时，再分别通过 `TakeLocal` 发布到 State。普通 Load 仍是访问视图，不能直接冒充拥有输入。旧单输入构造继续使用原有路径；数组槽规则和 Verify 的唯一发布门没有放松。

新增双输入构造的具体 Managed 参数与局部资源生产者在符号检查中记录实际拥有资格。显式 give 形参、ByteBuffer 创建、资源构造和合法 give 转交可以建立该资格；读取资源描述符或绑定其别名不会继承资格。未调用定义中的描述符 give、别名 give 和重复 give 也实际拒绝。这个私有事实只服务源面检查，实际责任仍由 Types、Analyze、Verify 各自核验。

199 的真实字段调用位置和声明模式完整保留。FnMut 的两个字段可以独立反复调用；声明 FnOnce 即使接收实际 Fn，也消费选定字段并在下一语句之前结算其责任。两个字段可以分别 lending，借出第一字段的视图期间允许操作第二字段；同字段重入、取走、替换和整 State 移动仍被真实窗口检查拒绝。196 同步双角色 helper 的 lending 限制保持；拥有值边界不伪造空的 noescape 合同。

没有新增公共字段或机器布局。构造实例是普通完整 Package 函数，拥有参数、字段装配、实际 Take、Drop 和位置投影使用原有 IR。Analyze 与 Verify 的新增双输入边界由各自作者独立实现，不读取前端分类证明。

## 最终实际验证

| 范围 | 5c2fbc50 的实际结果 |
|---|---|
| 原三份 CLI 输入 | 00 单字段 Success / Verify 1；01 双字段 Success / Verify 1；02 Store callback 仍 Expand SafeReject / Verify 0 |
| 200 作者 source | 12 / 0；包含 58 次真实 O0/O2 程序执行 |
| 200 Analyze 来源派生 suite | 8 / 0；归属 Analyze 作者的独立检查，不冒称前端作者的独审 |
| 直接受影响旧 source | 197：14 / 0；199：9 / 0 |
| 上述组合 | 43 / 0 |
| Expand / Types 库 Clippy | 同一提交 `-D warnings` 通过 |
| 四模块 Linux 消费者 | 100 次通过：14 普通产物执行、86 分配探针执行，其中 66 次真实分配失败 |
| 四模块 Win64 / Wine 消费者 | 100 次通过：14 普通产物执行、86 分配探针执行，其中 66 次真实分配失败 |
| 原生 Windows | NotRun；Wine 结果不替代原生 Windows |

200 source 包括两份不同拥有环境、普通与混合 State、0/1/多轮、break/continue、完整移动/返回、FnOnce 选定字段消费与未调用字段清理、较强模式适配、同一 Copy FnMut 的两个副本及原变量继续调用、真实非 Copy 重复实参、不同工厂身份、完整普通 T 和 phantom 身份、真实 sibling 与同字段/父壳窗口，以及实参准备中第二回调未获得或两回调已获得时的错误清理。

Store 输入的 SafeReject 仍只是元素资格前门，不能计为经过 Verify 的清理安全反例。未调用模板与 Types 阶段控制也单列，不能把它们计作真实值发布或机器执行。

## 四模块消费者与失败位置

`examples/mainline/callback_state200/` 的 portable、factory、state、main 四模块组合完整持久状态。工厂产生 A、B 两个不同环境，每个环境有自己的 Ticket 和字节缓冲；State 另有自己的 S 缓冲和名义 Copy tag。State 跨函数完整转交，在循环中把真实 sibling 字段以 `(A,B)` 和 `(B,A)` 交给既有双角色 helper，得到每有效轮 `ABBA`。

输入 0、1、2、3、7 分别覆盖零轮、一轮、continue、两轮和 break 后三轮。每个回调核对自己的缓冲长度，main 核对累计字节更新与完整 tag。State 按声明顺序清理自身缓冲、first、second，输出结尾为 `ab`；这与实参准备失败时按已获得临时责任清理的顺序分别验证。

独立故障 oracle 显式追踪 `S + ABBA…` 的每一次真实分配。每个实际位置都注入失败，并执行最后一次分配之后的控制。所有探针均满足 `frees == allocated`、`live == 0`；Wine 额外满足 `bad_operations == 0`。C shim 只观察和注入外部分配，不实现任何 ZIL State、字段或调用行为。普通未插桩产物的执行与探针结果分别留证。

Wine 初次 `TMPDIR` 环境错误、同一 PE 的修正控制和最终重新运行均保留。运行 Wine 必须让消费父进程的 `TMPDIR` 不存在，而不是把它设置为 `/tmp`；完整命令及 WINEPREFIX、loader、server、动态库路径在收据中。该环境修复不涉及生产代码或重新解释首次失败。

## 实际预算

以下均在原默认额度内二分找到实际 N，核 N 成功、N−1 ResourceLimit、fresh N 成功，没有提高额度。

| 来源 | Expand work N | Types work N | Types locals N |
|---|---:|---:|---:|
| 原始混合双字段拥有输入 | 2617 | 3562 | 19 |
| 两份 Copy 发布临时 | 1210 | 1724 | 13 |
| 直接 Managed 与局部资源准备 | 1239 | 2433 | 15 |

恢复前单 F 路径的额外扫描及非作者对旧固定源的精确预算比较，另见 200 独审收据；本作者没有重复构建旧基底或把其比较转记为自己的测量。最终源面预算和首次版本的预算分别按实际提交保存。

## 证据与交付

小摘要、精确身份及文件摘要在 `evidence/impl/200/frontend/`。独立附件 `200-author-evidence.zip` 为 **17,319,983 字节**，SHA-256：

`2ea3e332e54e79d6036e550d529e3e6e6fc5fcf7933a165fbb13da0cedca844f`

附件包含 7,258 个原始命令、失败、阶段文件及执行日志，另有完整 MANIFEST；所有载荷逐一校验 SHA-256。318 个执行产物或对象文件已排除，只在清单中保留其身份和大小。ZIP 不放入 Git；源交付不携带 target、程序或该证据附件。前三轮的 8/4、10/2、12/0 和最终 43/0 各自保留，不把首轮失败重写为通过。原因和修复对应见 `changes/200-frontend-validation-corrections.md`。

## Rust 四目标与未完成范围

| 目标 | 本轮证据与判断 |
|---|---|
| 同类安全 | 同 F 双拥有字段的源码、独立核、真实窗口与失败责任闭合；不能据有限 profile 宣称完整语言安全 |
| 运行与内存成本 | 有实际普通执行、逐分配失败与零存活记录；没有本轮同语义 Rust 时间、峰值内存或尾延迟比较 |
| 表达与维护 | 一个普通组件能持久保存两份具体处理器并跨模块复用；完整框架维护成本尚未测量 |
| 编译成本 | 实际新增工作和精确预算有记录，旧单 F 扫描回退；没有本轮编译性能优势结论 |

至少一项产品级可重复净突破仍未证明。完整语言 NotSatisfied、产品 NotRun、B0 Doing 保持。callback Store 元素、不同 F/G 的持久 State、一般 trait / lending、卫生 Syntax、外部保留和挂起并发等仍由后续具体切片承接；200 不替代这些终态要求。
