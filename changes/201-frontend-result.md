# 201 前端交付：Store 中的具体回调、真实窗口和完整清理

201 将原始 callback Store 输入推进到完整 Verify 与真实机器执行。已有单个 `State<F>` / `Pair<F>` 参数的函数，可以在函数体内建立有限 callback Store；元素保存一或两个直接、自含、同 F 的回调字段，普通 Copy T 及原直接 Managed 字段继续保留完整身份。

## 实际身份

- 生产及四模块消费者的干净受测提交：`d55c3d9bd0f59745b331757f422e6d1793554907`。
- 最终作者测试提交：`f484589dc4c08ee695f93d36171542482b113134`；相对 d55 只改作者测试，没有生产变化。
- `source06` 实际运行于 d55 加未提交测试差异；差异 SHA256 为 `70643818c63708aac89bd83f878e10ced8584ec0c2decfd7c39faf8f4aabab2c`，已与提交后的 `git diff d55 f484` 逐字核同。没有把原运行的 HEAD 改记成 f484。
- 设计 SHA256：`4c57f670cae6805444e0531485deee9c3a4a9486f533fc97e49f0b5104bccfbf`。最终 normative：`b72c7a0f8d62814324d7660cfc2b4655aae12bd5272b0511075f1aa7d93accb3`；此前来源运行仍保留当时的 `93055a7c…` 摘要。
- 最终 CLI SHA256：`e49b7fb470835f6a759978191c061cc33b61ee268ac8d53fcd6368bce218eb7c`，实际构建收据 `cli01`。Linux、Wine 两组报告记录同一 CLI 和同一组四模块源码摘要。
- 公共 schema / Wire 仍为 25。规范登记、作者结果、非作者审核和正式全仓是不同事实；后两项由 root 另行收口。

## 实现范围

Expand 沿已有 owning callable 上下文识别有限 Store 元素及普通 Copy T，不把 F 视为 Copy。新增字段与类型遍历在工作前收费；原非 callback Store 分支保持原路径。Types 在所有实际闭包定义完成后独立检查环境图和元素结构，拒绝地址、直接 Store、含 Store 的 Package 和本片不支持的嵌套 callable Package 捕获。

实际借入字段调用使用原有 `OwnedStoreBorrow[Mut] → AddressField → ReborrowAddress → CallClosureBorrowed`。callee 的真实位置在实参求值前建立；共享窗口只允许 Fn，独占窗口允许 FnMut。FnOnce 必须先 `pop` 得到完整拥有元素，再消费其字段。Store 窗口不能复制或替换拥有回调字段，也没有因不同动态索引、同 ClosureId 或不同字段名而得到 apart 权限。

有限 helper 计划新增私有 Store 元素形状，使 `invoke(&mut item.callback)` 能调度已有同步 helper。是否需要这条新路径由已收费的模板类型遍历记录；没有额外扫描旧普通 Store，也没有把 Store 本身登记成 callable 参数角色。完整类型绑定仍由下一阶段承接。

捕获 Package 的资格与完整 Store 元素的非零布局分开。一个零 payload Package 仍可通过其真实 shell / Managed live flags 构成可清理环境；它不必自身成为合法 Store 元素。完整元素实际非零、捕获图与布局仍由独立 Verify 重建。独审的原直接 State 成功与 Store 失败输入保持，修复后作者对同字节输入在两种形态都执行成功。

Analyze、Verify 和 LLVM 的生产改动由各自责任人实现与提交。前端没有把自身分类当作安全证书，也没有跨写他们的检查来让源程序通过。

## 作者实际验证

| 范围 | 结果 |
|---|---|
| 最终作者 source suite | 16 / 0，包含 32 次真实 O0/O2 执行 |
| 其中明确的 202 未开边界 | 2 条；普通 StoreElement helper 丢失完整依赖类型，不能计为 Verify 安全验收 |
| 原 178 定义时预算 guard | 当前 1 / 0；没有冒称未知的历史固定 N |
| Expand / Types 库 Clippy | `--lib -D warnings` Pass |
| 四模块 Linux | 118 / 0：14 普通执行、104 探针执行，其中 84 次真实分配失败 |
| 四模块 Win64 / Wine | 118 / 0：14 普通执行、104 探针执行，其中 84 次真实分配失败 |
| 原生 Windows | NotRun |

源测试包括旧原始 02、同类型两个实际元素、同一元素两个不同环境字段、共享 Fn、多次 FnMut、pop 后两个 FnOnce、零次调用、lending、无关 Store 允许操作、同父 reserve / pop / 重借 / 重入拒绝，以及 callee 实参准备时失效冲突。新增责任测试实际覆盖满容量 push 的错误载荷清理、空 pop 错误传播、负数和溢出 reserve 保留原前缀、真实非 Copy 字段重复转交拒绝。窗口反例最终精确绑定 Verify 的 `E-BORROW-CONFLICT`，不是“任何拒绝都通过”。

## 四模块消费者和故障 oracle

`examples/mainline/callback_store201/` 由 portable、factory、state、main 四模块组成。两个工厂值各自拥有 Ticket 和 ByteBuffer，通过已有 Pair 入口进入函数，再拆进两个完整 Store 元素。每轮通过真正的元素窗口及既有同步 helper 按 `ABBA` 顺序调用，分别核对两份环境自己的缓冲长度，并修改 main 提供的真实可变切片。

输入 0、1、2、3、7 分别覆盖零轮、一轮、continue、两轮以及 break 后三轮。循环结束后，Store 从容量 2 扩为 4，再 pop B 为完整拥有值；非零轮输入还调用该拥有字段一次。Tag 的普通名义数据身份、累计切片更新、Store 长度及容量都由 ZIL 源码检查。

故障 oracle 预先独立列出 `S + ABBA… + G + B?` 的实际分配序列。S 是初次 Store 分配，G 是容量增长，A/B 是各环境自己的缓冲分配。根据源类型和冻结 x64 布局独立算出 Item stride 64；没有读取探针输出或 emitted LLVM 来生成期望。

- S 失败：Pair 仍拥有两份闭包，清理输出 `Zab`。
- 调用或 G 失败：Store 正向清理已有前缀，再清理 Pair 剩余 marker，输出尾部 `1a2bZ`。
- pop 后调用失败及正常返回：先清理已弹出的 B，再清理 Store 内 A 和 Pair marker，输出尾部 `2b1aZ`。

每个实际分配位置都失败一次，并运行最后分配之后的控制。全部故障满足 `frees == allocated`、`live == 0`；Wine 额外满足 `bad_operations == 0`。C 探针只观察或注入分配失败，没有实现任何语言操作。Wine 使用已确认的父进程 `env -u TMPDIR`，完整 loader/server/prefix/库路径记录在命令中。

## 精确预算

下表均在原默认额度内二分得到实际 N；N 成功、N−1 为对应阶段 ResourceLimit、fresh N 成功，exact / fresh / default 的完整阶段输出相同。

| 实际来源 | Expand work N | Types work N | Types locals N |
|---|---:|---:|---:|
| 原始 02 | 3095 | 4241 | 16 |
| 两个 Store 元素 | 6680 | 11851 | 39 |
| 实际字段经同步 helper | 7283 | 11792 | 40 |
| lending | 2529 | 4129 | 14 |

原 178 guard 本次找到 Expand N=2873 / N−1=2872，没有可引用的同源历史固定数值，故只报告当前 guard 通过。Analyze 作者另有原普通 Store 精确 N=561 的前后比较；那是其独立证据，不冒称由前端作者重测。

## 失败保留与未完成入口

`check01` 是 Rust usize 解引用错误，修正后限定库 check 成功。`nested01` 实际运行零个测试，不能计 Pass；`nested02` 实际暴露 helper 计划缺 Store 来源，66c 修复后才有成功执行。`source05` 在 Rust 构建阶段停于 E0463，没有运行语言测试；只清理可再生 driver 缓存后，相同源码的 `source06` 才得到 16 / 0。各次原始输出均保留。

`helpers01` 的两份原源实际 0 / 2，仍未修复：普通 `T:StoreElement` helper 实例化没有保留完整实际 T，导致内部 F 游离或生成名义族不一致。当前两个测试明确记录该未开边界，不能将其 Types 阶段停止计作安全验收。202 应保留独立普通类型 binder、完整 T 实例键、显式类型应用及参数 / 结果上下文约束；不能伪造 F 参数角色或私自内联。

当前 inline 字段调用支持命名 Store carrier 和命名元素 view；没有声明任意有副作用的复合 carrier 表达式均已开放。显式 `Store<State<F>>` 泛型参数 / 返回、直接 Store 或含 Store Package 捕获、异质 F/G State、保留窗口、dyn / async 等仍不属于本片。完整语言 NotSatisfied、产品 NotRun 保持。

## 小证据包

`evidence/impl/201/frontend/SUMMARY.json` 保存精确身份、各次运行和预算。外置 `201-author-evidence.zip` 为 **7,320,525 字节**，SHA256：

`2b32598da8b265473b92bc7eb2842b660928aa19bd29c8a11c44bcb4aa463868`

包含 2,604 个原始日志、阶段记录和复现脚本载荷，另有 MANIFEST；全部逐文件核验 SHA256。92 个执行产物 / 对象文件排除，仅在清单保留摘要。ZIP 不加入 Git，源交付不携带 target 或该证据包。正式全仓及非作者独审结果由 root 另行登记，不合并进本作者统计。
