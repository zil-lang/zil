# 063 正式结果：先修正表示生命周期，再衡量图切割

本轮建立了可复现的阶段诊断、受控 BDD 原型、完整独立 raw oracle 核对和实际 production driver 对照。**关系研究仍为 NoGo 入生产安全核；未请求 dump 的惰性格式化可单独进入生产集成审查。** 原物理论文给出的方向不变量与责任唯一性仍值得追踪，但当前证据不支持宣称原创通用图算法突破、完整 ZIL 安全闭合或胜过 Rust。

最有价值的三项观察是：四条件失败首先暴露了累计临时节点未回收；在相同回收下，owner 顺序比 canonical 顺序显著有效，而纯 cut 并没有独特净收益；真实 driver 的重复检查原先仍花大量时间格式化并未请求的阶段输出。融合消元另外把五条件实例推入固定预算可完成的范围，但只在这一特定功能边界成立。以下分别给出证据与限制。

## 1. 冻结身份和计数

| 项目 | 精确身份 |
|---|---|
| 生产源码 | `70d3a729a6db6f8355ea99942e6f05207eb8815b`，draft12/schema9 |
| 正式研究源码 | `1c679ad8dd864f17a1a952404fd66e1b3c7e3c57` |
| 规范摘要 | `6adad9d4a24e5d51d5e871ca8871e1d0faddb6e5161395507319b938dca77a8b` |
| 原始目录 | `out/runs/research/063/frozen-003/` |
| root 独占窗口 | `063-20261024T001738Z`；00:17:47–00:19:53 UTC，2026-10-24，共126秒 |
| Rust 工具链 | rustc 1.99.1，`58cf8f9c35b355199727627d07ae34a4e2846d67`，LLVM 23.1.1 |
| 外部 clang | Ubuntu clang 23.1.5；完整版本、二进制 SHA 见 `identity.json` |
| 研究构建 | release；codegen-units=1；debug=0；incremental=0；jobs=1 |
| 正式原始样本 SHA256 | `c4a08f7beaee350053660ecc94ae6f79519a3487f87209f8371b1e5b8ffef888` |
| 正式统计 SHA256 | `aa2d7e2665dc0da4b7bce32553fb31a1b2c51ecebc693c7686fdf19a21f01b32` |

正式采样每组合2次预热、7次保留，图方法轮换、driver baseline/lazy交替，每次独立进程，不清理OS文件缓存。共 **1050 次保留进程 +300 次预热**：图966+276，driver84+24。Rust四次 `-Ztime-passes` 为附加单次诊断，准备/构建/emit/native核对也不混入1050。root协调其它代理停止构建与测试；进程命名空间清单仅作辅助记录，不能独自证明机器隔离。环境包含未完全封存的宿主OS/链接库，结论限定本次机器与配置。

准备阶段29项研究测试通过；新进程再执行1项GC预算测试，是同一测试的重复证据，不把总独立测试数写成30。Clippy all-targets `-D warnings`通过。162个图 case×method 冻结运行，含726个 `Complete` 函数实例、11个候选 `ResourceLimit`、2个 `OracleResourceLimit`；Complete 的726份候选完整canonical均与各自独立raw oracle原字节一致。生产Analyze/Verify作为另一种接受性查询单列，不被算成同精度oracle。正式图计时只选择预先声明为timing且整模块全部函数完成的方法：9个源模块、46个函数；未完成方法保留失败记录，不能给失败退出耗时报成功倍率。

相同源码绝对路径下，三个真实driver工程共10组 baseline/lazy情景核对：110对请求的阶段文本dump逐字一致，稳定报告字段一致，20次真实native进程的exit/stdout/stderr符合预期并配对相同。原始命令、参数、环境限制及SHA详见归档；机器可读精简结果见[results.json](../../../evidence/impl/063/results.json)，重放合同见[cold-replay.json](../../../evidence/impl/063/cold-replay.json)。

## 2. 四个独立条件究竟在哪里失败

使用059原 `independent.zil`，未改变输入以规避失败。新增诊断对全部完成的事件根、固定点states与已发布relations做联合可达普查；只在无未发布临时值的安全边界拍快照。原六个不回收变体均耗尽300,000个BDD槽位，位置如下。

| 无GC方法 | 失败 image point | 前一安全边界 point | 边界已分配 | 边界完整持久根可达节点 |
|---|---|---|---|---|
| mono | 98 | 87 | 290296 | 14231 |
| cut | 87 | 76 | 295845 | 14234 |
| wire | 87 | 76 | 291316 | 14234 |
| fused | 77 | 66 | 297080 | 15163 |
| fused:owner | 105 | 94 | 297350 | 19339 |
| fused:token | 104 | 93 | 293065 | 17018 |


这些不是“失败瞬间仅有14k活节点”的测量。失败发生在后续image内部，表中最后普查只描述**前一完整安全边界**；递归栈上的临时NodeId在该快照不存在，不能从前一快照推断它们在失败时的峰值。它足以提出“已完成历史临时节点占用大量arena”的可检验假设。含诊断运行的普查work单独计量，正式速度样本禁用诊断，避免把观测开销算给某种算法。

当前提取器先构建所有事件表达式和计划。失败位置表明这组样例的实际300k失败在固定点image阶段；这不证明任何更大源的事件构造都便宜。所有源点、位方程、failure phase、错误原文和前一边界身份保留在 `freeze/diagnostic/`，没有把原059/schema8历史样本重标成063/schema9。

## 3. 安全边界回收的合同

GC变体仍限300,000个**物理arena槽位**，在完整事件边界或一次完整worklist迭代后、占用达到200k时回收。其累计创建量可以超过300k；这是一项明确的表示工程变体，不能宣称保持了旧“终生累计分配300k”的限制。所有算法使用同一GC、相同限制和100M候选work。

根集合包括已完成事件的方程、fault、abort、CFG/出口guard、当前全部states及已发布relations；plans只引用这些事件因子。求解器先完成当前转移的所有变体、终端关系与successor发布，再收集。递归apply/exists/substitution/unprime内部、仍持有pending输出时不回收。存活NodeId不变；回收前清空apply缓存，unique表clear后按存活NodeId递增重建，空槽位才允许复用。

费用包括根收集、arena扫描、可达遍历与边访问、cache/unique按clear前容量扫描、live节点重插、free列表清理重建；GC并非免费。对HashMap使用 `2*capacity` 的保守逻辑费用覆盖bucket/control扫描，完整预收费含 `3*arena_len + roots_len + 2*cache_capacity + 2*unique_capacity + free_len`，另扣遍历和live重插。它是明确可重复的逻辑计量，不是假称等于CPU指令或纳秒。完整核对、census、材料化、序列化/指纹均共享候选预算；独立oracle另有100M预算。

GC缓存复用、根保持、完整关系、四guard和确定重复清理的反例均有测试。最终 `fused:owner:gc` 的完整work **N=20,647,385**，N−1拒绝，N/N+1成功；同方法同参数新进程再次给出相同N。正式代码之前曾因 `HashMap::retain` 留下随机布局相关tombstone容量而使预算边界不稳定，原失败保留，见第8节。

## 4. 回收、排序、切割的贡献分别是多少

以下是四条件函数 `independent` 本身，1440个完整联合关系行；完整候选时间计入提取、事件构造、计划、求解、精确关系核对、完整census/材料化/序列化及析构。前端、oracle和进程封装另存。arena槽位高水位不是RSS，也不是某时刻完整持久根的活节点数。

| 方法 | arena槽位高水位 | 累计创建 | GC次数 | GC work | 完整candidate work | 完整候选中位ms |
|---|---|---|---|---|---|---|
| mono:canonical:gc | 252874 | 2849036 | 15 | 40852631 | 70489914 | 1196.200 |
| cut:canonical:gc | 280874 | 3164092 | 16 | 38844095 | 72375030 | 1149.829 |
| wire:canonical:gc | 259113 | 3104229 | 16 | 41035545 | 73665297 | 1127.885 |
| fused:canonical:gc | 256678 | 2605683 | 13 | 31169781 | 61295443 | 1009.514 |
| mono:owner:gc | 207015 | 925524 | 4 | 6649572 | 19586288 | 324.106 |
| cut:owner:gc | 216560 | 1152900 | 6 | 10125890 | 26510935 | 378.717 |
| wire:owner:gc | 218717 | 1155530 | 6 | 10165452 | 26113141 | 363.140 |
| fused:owner:gc | 210118 | 766701 | 4 | 6702920 | 20647385 | 303.776 |
| fused:token:gc | 223129 | 1044861 | 5 | 8447067 | 23420912 | 330.201 |


canonical mono加GC已能完成，故“四guard恢复成功”不能归为cut的独特收益。同为mono+GC，只改变内部rank到owner分组，完整候选中位1196.200→324.106ms，约 **3.69×**；规范坐标、因子顺序和完整输出没变。这给下一阶段建立了更强的研究基线。

同owner顺序、同GC、同语义下，cut为378.717ms，比mono慢约16.8%；wire为363.140ms，慢约12.0%；fused为303.776ms，观察到约6.3%的降低，但模块配对比例0.937、探索95%区间[0.837,1.046]跨1，不能宣布稳定的四guard净加速。fused创建节点少于mono，却因GC、因子访问和量化等工作组成不同，完整逻辑work反而20.647M大于19.586M；单一node计数不是总成本替代品。token顺序虽使最大逐点可达节点进一步减少（2452 vs owner的2724），总成本仍高于owner fused，同样说明需看完整路径。

下面保留所有9个计时源模块。每格是完整模块候选时间中位比与1000次配对bootstrap探索区间；样本仅7，使用seed63，未校正多重比较，不是跨机器或产品级置信声明。第一列只比较排序，其余列都以owner mono为控制，**全部开启同一GC**。小于1表示该变体较快。

| 完整源模块 | owner排序 / canonical mono | cut / owner mono | wire / owner mono | fused / owner mono |
|---|---|---|---|---|
| tiny | 1.025 [0.746, 1.188] | 0.934 [0.728, 1.088] | 0.943 [0.801, 1.109] | 0.938 [0.742, 1.535] |
| branch | 0.823 [0.738, 0.888] | 1.046 [0.938, 1.236] | 1.017 [0.956, 1.493] | 0.936 [0.900, 1.213] |
| correlated | 0.771 [0.712, 0.842] | 1.026 [0.955, 1.176] | 0.926 [0.847, 1.015] | 0.891 [0.854, 0.915] |
| independent | 0.271 [0.241, 0.299] | 1.168 [1.087, 1.322] | 1.120 [1.048, 1.241] | 0.937 [0.837, 1.046] |
| closure | 0.857 [0.817, 0.888] | 0.987 [0.940, 1.026] | 0.963 [0.914, 1.022] | 0.943 [0.911, 0.996] |
| error | 1.171 [1.053, 1.535] | 1.097 [0.837, 1.172] | 1.019 [0.763, 1.114] | 0.969 [0.718, 1.054] |
| loop | 1.036 [0.948, 1.092] | 1.030 [0.967, 1.090] | 1.010 [0.957, 1.090] | 0.941 [0.889, 1.077] |
| guarded_desc | 1.096 [0.895, 1.206] | 0.879 [0.775, 1.129] | 0.951 [0.807, 1.164] | 0.857 [0.790, 1.003] |
| real_error_cleanup | 0.851 [0.792, 0.914] | 1.007 [0.960, 1.091] | 1.041 [0.949, 1.072] | 0.971 [0.935, 1.077] |


不能把不同源的局部最佳方法挑出来合成为未经实现的“自动最优”结果。correlated等局部fused收益值得追踪，但tiny、error、loop和真实19函数error_cleanup模块没有形成一致净优势；不同宽度、稠密度与输出规模都必须保留。

### 一个新成立的可行域边界

1、2、3条件的canonical mono+GC、owner mono+GC、owner fused+GC均完成。5条件的完整oracle仍可计算3552行，oracle work45,142,000：canonical mono在image point117耗尽300k槽位；owner mono在point128以 `work 100000003 > 100000000` 拒绝；owner fused完成，完整candidate work **99,246,291**，arena258,424，累计创建3,541,339，GC21次，完整关系与oracle原字节相同。它距100M上限约0.75%，因此这是**固定输入、固定费用模型下的功能边界差异**，不是统计速度结论；这些预声明的scaling输入不进入正式计时。

6条件三者都在某次image内部耗尽300k槽位，独立oracle也耗尽自己的work，不能算正确成功或推断关系精度。原state_limit仍为OracleResourceLimit。融合消元确实在5条件上比同GC/排序mono扩大了这一预算下的可完成范围；纯cut、wire的5条件未测，不能把fused效果拆成已经证明的单独因素。早期frozen-001采用较弱GC费用时owner mono曾完成5条件，那是旧源码与旧费用的观察，不能覆盖最终003结论。

## 5. 实际编译器：未请求输出也在付格式化成本

阶段观察发现原 `Artifacts::dump` 接受已构造的字符串；调用者先 `format!("{ir:#?}")`，之后才判断用户是否请求输出。query即使复用，外围仍重复格式化大IR，因而unchanged检查仍慢。唯一补丁 `tools/research/063/lazy-dump.patch` 在三个driver文件把格式化放入闭包，仅有输出路径时执行；取消、冲突、原子发布仍经既有dump路径。必要的 `RawSnapshot::digest` 与缓存身份没有变化。

测量直接调用实际公开 `CompilationSession::compile_project_with_observer`，无请求dump，分别到Verify或native Build；不是mock阶段计时。cold指本进程首次session编译，并非OS冷缓存；unchanged复用同一session；comment_edit追加注释；两模块incremental的private_body_edit把私有函数返回7改9，真实native输出相应改变。每个baseline与lazy读取同一份冻结源的相同绝对路径，避免把path/fingerprint不同误认为代码差异。

下表是一次实际driver调用的完整wall中位，包含query结束后的发布、必要指纹、安全核验，以及Build的LLVM/clang链接；源文件初次读取和进程启动/JSON包装在该计时外。回调间隔结束于阶段事件，包含前一query发布等工作，不能当作独占pass profiler。

| 工程 | 阶段/情景 | baseline ms | lazy ms | lazy/baseline [95%探索区间] |
|---|---|---|---|---|
| package_ui | verify/cold | 54.095 | 8.163 | 0.151 [0.133, 0.169] |
| package_ui | verify/unchanged | 45.048 | 1.134 | 0.025 [0.024, 0.028] |
| package_ui | verify/comment_edit | 49.599 | 6.791 | 0.137 [0.131, 0.142] |
| package_ui | build/cold | 115.619 | 69.382 | 0.600 [0.562, 0.637] |
| package_ui | build/unchanged | 112.205 | 61.758 | 0.550 [0.497, 0.561] |
| package_ui | build/comment_edit | 120.252 | 67.399 | 0.560 [0.535, 0.606] |
| error_cleanup | verify/cold | 53.331 | 7.577 | 0.142 [0.135, 0.164] |
| error_cleanup | verify/unchanged | 46.488 | 1.003 | 0.022 [0.021, 0.023] |
| error_cleanup | verify/comment_edit | 49.936 | 5.683 | 0.114 [0.108, 0.123] |
| error_cleanup | build/cold | 118.940 | 71.905 | 0.605 [0.567, 0.646] |
| error_cleanup | build/unchanged | 112.512 | 64.173 | 0.570 [0.538, 0.613] |
| error_cleanup | build/comment_edit | 120.039 | 68.804 | 0.573 [0.549, 0.602] |
| incremental | verify/cold | 1.990 | 0.812 | 0.408 [0.278, 0.452] |
| incremental | verify/unchanged | 1.420 | 0.191 | 0.135 [0.126, 0.146] |
| incremental | verify/comment_edit | 1.747 | 0.413 | 0.236 [0.186, 0.261] |
| incremental | verify/private_body_edit | 1.586 | 0.402 | 0.254 [0.161, 0.338] |
| incremental | build/cold | 56.488 | 51.541 | 0.912 [0.849, 1.015] |
| incremental | build/unchanged | 53.006 | 50.947 | 0.961 [0.803, 1.141] |
| incremental | build/comment_edit | 53.462 | 49.083 | 0.918 [0.780, 1.013] |
| incremental | build/private_body_edit | 53.116 | 49.130 | 0.925 [0.899, 1.008] |


两个四模块真实应用的Verify明显改善，Build也观察到约40%–45%的完整调用时间减少。最小两模块native构建主要受外部发码/链接成本支配，各Build探索区间跨1；不据此宣称所有native构建都更快。warm Verify的大倍率属于这三份源、这个公开session与无dump选项，不冒充完整CLI、Cargo依赖构建、Rust对比或一般增量失效能力的倍率。

为避免只报内部时间，下面给每个进程的全部场景组wall，包含启动、源码IO、全部情景、JSON封装与退出。四模块组包含cold/unchanged/comment三个调用；incremental组再含private编辑，组间不可当单次编译直接比较。

| 工程 | 完整进程场景组 | baseline ms | lazy ms | lazy/baseline [95%探索区间] |
|---|---|---|---|---|
| package_ui | verify | 154.992 | 18.486 | 0.119 [0.117, 0.128] |
| package_ui | build | 349.382 | 198.967 | 0.569 [0.554, 0.593] |
| error_cleanup | verify | 151.612 | 17.376 | 0.115 [0.104, 0.117] |
| error_cleanup | build | 356.461 | 210.539 | 0.591 [0.567, 0.605] |
| incremental | verify | 8.816 | 3.488 | 0.396 [0.292, 0.431] |
| incremental | build | 221.289 | 205.841 | 0.930 [0.826, 1.010] |


准备阶段另在 **请求全部emit** 时配对核对110对原字节dump和20次native行为，证明本补丁不会靠跳过实际产物来获益；正式计时本身不做dump写出。冻结只证明本研究三工程合同，生产吸收仍由064及其独立审查覆盖取消、输出冲突等近邻不变量。本研究没有改变任何实际生产源文件。

## 6. Rust四次诊断支持什么判断

使用同仓两份手写Rust应用，edition2024、debuginfo0、codegen-units1、O0/O2，显式 `RUSTC_BOOTSTRAP=1 rustc -Ztime-passes`。标准库已构建，仅测根crate；不是Cargo干净依赖构建、generic-heavy大型工程或严格语义/选项配齐的Rust/ZIL竞赛。每配置只运行一次，阶段原文保留三位小数精度；这些数字用于定位方向，不支持显著性或生态总体占比。

| Rust程序/opt | 完整进程ms | rustc total ms | typecheck ms | MIR borrow ms | mono graph ms | LLVM passes ms | run linker ms | link crate ms |
|---|---|---|---|---|---|---|---|---|
| error_cleanup-O0 | 62.481 | 56 | 4 | 4 | 3 | 7 | 25 | 26 |
| error_cleanup-O2 | 100.689 | 92 | 5 | 4 | 4 | 33 | 30 | 32 |
| package_ui-O0 | 73.410 | 67 | 4 | 3 | 5 | 7 | 33 | 34 |
| package_ui-O2 | 86.037 | 77 | 4 | 3 | 6 | 25 | 24 | 25 |


这四次中MIR borrow为3–4ms、typecheck为4–5ms，LLVM passes7–33ms、link_crate25–34ms。只优化借用检查不能覆盖这些负载的大部分总等待；应同时关注类型/trait、实例化、发码、链接与增量边界。`time-passes`含嵌套阶段，如mono/LLVM和codegen内部计时，不能把所有行相加；0.000也只是显示精度，不代表真实零成本。Rust dev guide的“大部分时间在LLVM”原文指编译rustc自身，不能外推所有应用。[一手来源与适用域](SOURCES.md)

截至本轮检索，官方2026-08-21文章报告next solver在nightly默认启用、并讨论性能与稳定化工作；2026-08-04文章报告Polonius alpha的nightly节点。这里引用**这些带日期的落地事实**，不由8月计划推断10月stable版的完整状态。coherence较早的稳定化与全局替换也分开。新solver的canonicalization、约束响应、递归深度/循环上下文敏感缓存值得用于ZIL已核摘要设计；Polonius从研究Datalog到可用架构的经验说明精度、表示与落地成本必须共同处理。

Zig0.16官方发布说明中的按需类型/字段分析、避免over-analysis、增量依赖管理、x86机器码及链接方向提供互补路线；其中LLVM bitcode增量并不等于LLVM Emit Object已加速，incremental已知问题/默认关闭的状态必须保留，也不能把最新dev实验算进0.16稳定发布能力。详细链接、成熟度与有限推论见[SOURCES](SOURCES.md)，具体ZIL架构建议见[ARCHITECTURE](ARCHITECTURE.md)。

## 7. 原物理图思想的当前价值与下一实验

Deng/Hani/Ma的free/fixed切口算子复合与UP的全局/局部相反方向，依赖明确端点单调性。它启发我们先证明哪个区域切后不新增反向依赖，再在区域内部消除已无后续关联的状态；不能把任意CFG反向遍历就叫实现原UP。本轮是已有关系积/早量化机制的受控实验，未实现物理论文UP，也未证明其物理结论可移植到程序语义。

当前最值得攻坚的候选是token位置表示与完整边界摘要：对合法唯一责任态，把carrier×token稠密关联列改为token当前位置；保留显式非法态与每步fault见证，避免编码先把待拒绝状态删除。按token数T、carrier数C，关联位可能从T*C降至T*ceil(log2(C+1))，但这不包括生命周期/Desc/初始化顺序，也不保证BDD节点同比减少。还需解码D保持初态、迁移、fault/abort/exit、并集与最小固定点的证明/双向核对，再与本轮强基线owner mono+GC比较**完整构造、核对、解码、输出**费用。

更大的编译架构机会是已核语义摘要驱动失效，而非文本改动传遍全模块。公开类型、布局、效果、责任摘要字节不变时，某些下游query可能复用；必须证明依赖足够、预算/循环暂态不污染缓存。063 lazy只减少无用序列化，没有扩张复用权限。按义务投影/证书检查或不同知识编译表示可以继续探索，但若改变完整逐点输出query，就应建立新合同，而不是把减少输出算成同精度免费加速。

## 8. 失败与合同差异保留

`development/`保存缺工具链路径、重排序初始cube错误、conditional重复清理预期错误、Clippy错误和后续修正的原命令/退出码。早期dev binary只绑定其实际哈希，不伪造精确Git源码身份。`frozen-001`绑定d272a8d，准备通过但没有正式采样；其GC按表len计费不满足最终费用合同。`frozen-002`绑定71ba28a，正式准备在预算测试失败：同程序不同hash种子下，N=19,082,138的运行遇到 `work 19082806 > 19082138`。原因是retain后的tombstone/容量受随机布局影响。最终1c679ad改为clear后按NodeId递增重建，并保守按capacity扫描收费，通过独立新进程N边界，才形成frozen-003正式数据。所有目录及失败记录一起归档，未覆盖失败或重标旧样本。

还保留一个重要语义差异：在条件合流后重复 `DropManaged`，研究逐状态事件oracle报duplicate-cleanup关系，当前生产对MOVED|DONE合流可以接受；机器条件live flag可能让该清理成为no-op。**这不是机器double-free漏洞证明。** 059文档已区分研究query与生产接受性，§2的生命周期/违规语义还不能直接等同生产条件cleanup合同。063不修改历史，而在 `freeze/counterexamples/joined-conditional-duplicate/` 同存最小源、修改后的TypedIR、双方完整canonical与production输出。另一个 `definite-duplicate/` 反例在四guard之后保留确定live资源并重复清理，生产明确拒绝、oracle记录fault，GC仍执行且完整关系一致。不能静默删掉前一个差异，仅换成“符合预期”的反例。

## 9. 交付裁决与Rust四目标

| 目标 | 本轮实证 | 尚未建立 |
|---|---|---|
| Rust同类安全 | schema9有限关系、完整独立raw oracle；反例与预算边界真实执行 | 完整ZIL语义、一般escape/异步/拥有结构、生产接受集合等价 |
| 运行与内存同级 | 本轮仅为lazy补丁验证真实native行为保持 | Rust/ZIL运行时间、峰值内存、尾延迟的产品比较 |
| 表达与维护 | 研究把算法、排序、回收、输出工作与语义接口分开，可定位后续工作 | 本轮研究自身没有新增用户语言构造或证明维护成本优势 |
| 编译成本 | 实际driver的重复无用格式化已定位且配对改善；有限BDD可行域扩大 | 完整同能力Rust比较、跨机器稳定优势、生产关系核收益 |
| 至少一项净突破 | 有可重复的工程改进与5条件特定预算边界；原文结构有下一实验合同 | 原创通用算法定理、一般内存管理突破或产品级净突破 |

原 `validation/product-gates.json` 未改，完整语言/产品门不能由本研究标为完成。关系核继续NoGo；lazy补丁独立吸收；token位置与语义分隔集保持下一研究方向。完整Git可恢复代码，便携原始归档可审计所有正式数字与失败；[冷重放指南](COLD-REPLAY.md)给出在新目录从70d+1c重建并逐字核对四条件的命令。
