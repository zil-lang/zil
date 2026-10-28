# 201 非作者独审

**裁决：Pass，限于已登记的 callback Store 有限纵切。** 实际受测完整提交为 `7d690f67d76d665bf2e383ffab2c071d05685ecd`，schema25，设计 `4c57f670…`，最终规范摘要 `b72c7a0f…`。每条最终命令开始时源码均 clean。后补收据、作者新增测试和 root 全仓结果各自绑定实际提交，不倒标本次执行。身份和逐文件摘要见 SOURCE.json，机器结果见 REVIEW.json，独立外置证据的大小和哈希见 MANIFEST.json。

## 实际完成的核验

最终限定测试 **46 通过、0 失败、0 忽略**：13 组独立源码/raw/Wire/预算，13 组作者源码重放，7 组实际来源派生 Analyze/raw/Wire，9 组 Analyze 原始结构与预算，以及 4 组原输入和旧边界。源码 O0/O2 实际运行共 54 次，其中独立输入 24 次、作者输入重放 24 次、三份旧原输入 6 次。测试计数不替代安全范围说明。

四模块消费者在同一实际 CLI 上完成 **Linux 118 次、全部通过**：14 次普通执行、104 次分配探针，包含 **84 次真实分配失败**。所有探针最终 live=0，allocated=frees；最大同时存活 3 份分配、269 字节。CLI SHA256 为 `e49b7fb470835f6a759978191c061cc33b61ee268ac8d53fcd6368bce218eb7c`。本 reviewer 未执行 Wine 或原生 Windows；作者的跨平台结果单列。

独立源码覆盖两个相同 ClosureId 的实际元素、不同运行时状态、零次调用、满容量 push 的错误载荷清理、增长后 pop、两个 FnOnce 字段仅在完整拥有元素 pop 后消耗，以及空环境零大小与非零真实捕获对照。清理顺序逐字断言：元素顺序、字段声明顺序、未选字段和剩余父对象均不能按类型身份合并。

借用反例实际到达 Verify：callee 字段在准备当前调用参数时保持活跃；lending 结果在最后一次读之前阻止 reserve（包括容量不变和将失败的请求）、pop、重新调用和父字段写入。两个父 Store 即使完整类型和 ClosureId 相同，也不共享窗口权限。

独立 raw/Wire 有 6 份真实源码控制、1 份来源派生的合法未调用定义扩展及 6 份攻击。两份攻击要求重新 Analyze 成功，再由两个 Verify 入口拒绝：在活跃 lending 后插入真实父 Store reserve，以及把另一同类型 Store 的 reserve 操作数改为活跃父 Store。重复推送已转交 SSA 值先被 Analyze 拒绝，raw/Wire 仍给 E-VERIFY-RESOURCE；这不写成 fresh Analyze 成功。直接/间接 Store 捕获环与未调用三 callback 字段定义按实际资格门 Unsupported 记录，不冒充借用安全验收。

## 独审发现并关闭的生产误拒

一个闭包捕获零载荷 `Token { manages empty: Empty }`。旧直接 State 控制 O0/O2 均输出 A；把同一个闭包存入 Store，实际在 Types 被 Unsupported 拒绝、Verify0。完整失败留在 `raw/zero_capture01`，源提交 `86e710c9…`。

原因是新 Types 检查把整个 Store 元素必须非零的条件套到了被捕获 Package 本身。该 Package 的数据载荷虽为零，真实闭包环境仍有 shell 与字段存活标记。`66c1d1de…` 将捕获字段资格与整个元素非零条件分开；Verify 的真实布局核没有放宽。独立最终执行使用逐字相同的直接 State 和 Store 原输入，两者均 O0/O2 输出 A、退出 0。SOURCE/REVIEW 记录输入哈希，首轮失败没有被覆盖。

还只读核查了 helper_plan 的新增 Store shape：它沿已计费类型遍历取得具体类型依赖，新增 clone/容器工作先计费；它不是 runtime 来源或权限证明。实际 Store 字段仍由普通 Builder、Analyze 和 Verify 分别检查。Verify 的字段 loan 具有真实 parent，保守保留父 Store roots；不把相同 ClosureId 或不同动态下标当作互不相交。

## 布局、清理与故障消费者

核查了 Verify 在资源定义资格期独立重建真实捕获元数据、深度/宽度/有界布局及 live flags；LLVM 从实际 ClosureDefinition 取对齐和每字段 drop。零字节完整元素继续在 Verify Unsupported，捕获 Unit 与零载荷拥有 Package 的真实标记按实际环境计入。没有新增 schema 位或生产者权限承诺。

消费者由 Pair 的两个实际环境移入两个完整元素，经既有同步 helper 交替 ABBA 调用，在最后使用窗口后增长 Store，再 pop 并调用拥有值。返回 Tag 及可变切片内容都实际核对，不能仅凭打印序列通过。期望分配序列从源程序和固定 64 位布局推导，元素步长为 64；没有用观测到的 allocator 值或 LLVM 输出回填 oracle。每个分配序号单独失败，最后一个之后的控制必须正常成功。探针 C 仅观察/注入 allocator，不实现 ZIL 程序。

## 预算与历史输入

独立新 lending 源的最小工作预算为 Expand **2871**、Types **5241**、Analyze **16091**、Verify **29902**，各自 N 成功、N−1 ResourceLimit、fresh N 再成功；默认额度未提高。早期新 201 实现同源 Types=5245 的记录保持其原提交与旧摘要，不倒填到最终修复。

旧单字段 State 的 Expand 实测仍为 **1731**，与先前独立旧基底相同。旧普通 Store 的 Analyze 实测仍为 **561**，与 Analyze 作者旧基底相同。原 178 动态边界测试通过，但没有在本轮建立所有普通 Store 源码的历史 Expand/Types 固定 N 对照，不能扩大为全阶段旧成本均已证明。

三份 200 原探针逐字保留，最终全部 Types 成功且 O0/O2 实际执行。迁移 b2、175 和 180 的只读收据独立列出。175/180 只更新一处消息片段，拒绝的 Managed 字段、窗口捕获、逃逸和整包反例没有改变。原 180 测试自身未单独断言错误码或 Verify0；这些事实仅归 root 的真实首轮报告。

作者后补 `f484589d…` 的三个新测试只读复核无问题：同元素双字段状态与 FnOnce pop、满 push/空 pop/overflow 责任、四源精确预算。旧负例只收紧到实际 Verify 消息，没有删除或改成成功。其新三组未计入本 reviewer 的 46 组执行。

## 真实失败和剩余边界

原 source01 的 6/4 是三处独立夹具整数语法错误和一个错误的 FnOnce 分类预期；source02 的 8/2 是算术和错误码 oracle 失误。修正只改测试，原输入/输出保留。source03 的 10/0、capture01、unused01 与后来的真实生产 zero_capture01 误拒分别留档，不将多轮数量拼成一次完整通过。

普通 `T: StoreElement` helper 在实际 T 仍依赖 F 时的完整类型绑定，以及显式 `Store<State<F>>` 泛型输入/结果仍是 202 缺口。其前门拒绝不是安全核通过。不同动态下标或 sibling 字段没有 apartness；借入 FnOnce 仍在 Expand 拒绝，完整 pop 拥有值是已成功路径。Store 捕获、含 Store 的 Package 捕获、不支持的嵌套 callable Package 捕获、异构持久 F/G、递归拥有元素、dyn/async 等边界仍关闭。

当前没有未决的 201 安全问题。该有限增量不建立完整语言安全、同语义 Rust 运行/RSS 同级、完整维护成本优势或编译速度优势；产品四目标合取及可重复净突破未建立。full_language=NotSatisfied，product=NotRun，产品门未改。正式全工作区由 root 的独立集成记录负责，本 reviewer 不自行验收主线。
