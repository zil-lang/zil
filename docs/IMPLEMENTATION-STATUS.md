# ZIL 当前施工状态：语言组合、真实成本与研究裁决

## 最新语言阶段161–165（2026-10-27）

从校验的159包恢复后，161嵌套拥有闭包、162名义trait泛型约束、163Desc/give资源方法已按精确非作者审核合入。四模块消费者与错误准备路径均O0/O2原生通过。165仅迁移旧泛型诊断测试后，最终workspace **1384通过、0失败、0忽略**；全workspace/all-targets Clippy及fmt通过。完整证据、首轮1381通过/1失败及边界见[语言阶段报告](LANGUAGE-164.md)。

原159六项探针保持原样，嵌套拥有回调与泛型trait约束两项关闭；拥有数组元素、拥有错误结果、资源头地址接收者和捕获回调数组仍未关闭。阶段为StagePass_FullZuiNotReady；B0 Doing、完整语言NotSatisfied、产品NotRun保持。本轮专注语言，不进行编译速度研究，不继承历史性能或正式门结论。下一步见[三线实施建议](NEXT-LANGUAGE-165.md)。

---

## 最新恢复状态（2026-10-26）

已从保存版810346b9及完整历史恢复，并重新实施/独审合入120、121、122；当前实际增量、未完成边界与源身份见[恢复报告](DRAFT23-RECOVERY.md)。旧draft23聊天中的111–119进展不代表这些源对象已恢复。整个Rust workspace1195项通过，完整35正式门未在恢复版重跑；下文旧版本数字保持其原历史身份。


版本0.1.0 draft22，schema/raw wire17，2026-10-25。101–110均已绑定精确作者与非作者审核合入；101/107/108只合研究和净回退，不能读作优化准入。109普通库与精确Linux x86_64绑定组合完成固定五臂实验并限定录用，仍未证明稳健普适优势。整仓正式结果只读最终新收据，不沿用下方历史轮次计数。

最终正式 `20261025T140238Z-8db4b7c0` 在 `9a0f37ead42c700bbd29db8be67a6e058ed1297e` 通过35/35必要门、1169项工作区Rust测试和39项Roam测试。完整语言NotSatisfied、产品NotRun与B0 Doing保持。

| 切片 | 实际推进 | 当前边界 |
|---|---|---|
| 101 | 参数端口事实图、有限最小闭包证明、4940进程/19760请求、95端点独立复算 | 主完整比0.708151；89项不能排除1%回退，Inconclusive并净回退 |
| 102 | 四载体拥有字节、独立Rust+C+ZIL/C O0/O2、实际身份/失败/释放进入Roam | 自然48唯一程序、192native、0发现；控制注入独立记账 |
| 103 | 普通类型参数透明别名；修复独审暴露的名义类型和内建名捕获 | bound/const/lifetime别名未开放 |
| 104 | 同合同Rust三消费者；960时间臂、150独立RSS及12内存控制，全部原件复核 | 9编译中位有有限优势；3个4MiB运行单例Fail，完整产品未测 |
| 105 | 有限Copy枚举CT指令、VM、预算及fresh Core重化、完整名义身份 | 独立普通重检仍执行；非Copy/地址/资源/foreign CT值未开放 |
| 106 | 四模块Native run/build、清理/错误/void/完整值入口及8负控制 | 48顶层命令、148断言、34实际native；不等于T04模块增量完成 |
| 107 | 九个确定性查询输入闭包、私有构造身份、独立4940进程/19760请求 | 主完整比0.747634；32项不能排除1%回退，Inconclusive并净回退 |
| 108 | Linux外部绑定、符号/ABI/启动边界、1152时间观测、180目标RSS | buffer4MiB效率0.896642，NoGo；全部退化和IFUNC时机差异保留 |
| 109 | 普通ascii_upper值函数+显式窗口赋回；与108绑定形成五臂新实验 | 450行为、1920时间、300目标RSS全量独审；十二单元原门通过，限定录用；同bin波动、3中位/4p95不利和IFUNC边界保留 |
| 110 | 旧枚举CT回归迁移：独立Verify后实际CT=42，运行期模式变量拒绝保持 | 一条旧测试契约修复；作者16+12组、root独立双边界执行，完整门另计 |

首轮正式 `20261025T134620Z-6561d6f5` 为34/35，旧impl029期待与105批准能力冲突；失败原件和Cargo旧诊断复现保留。110未改生产语义、预算或门槛，最终验收以新完整收据为准。

103与105已在合并后的四模块项目组合：两种优化级都经独立Verify执行到完整值1099511628800。语法不支持的泛型模式及早期审查夹具失败也保留，不放宽语法或诊断。

终态差距见[DRAFT22-TERMINAL-GAPS](DRAFT22-TERMINAL-GAPS.md)，研究判据见[架构研究](research/DRAFT22-ARCHITECTURE.md)，质量发现归属见[Roam审计](DRAFT22-ROAM-AUDIT.md)。B0 Doing、完整语言NotSatisfied、产品NotRun保持；整项目语义键、一般非Copy/递归图/trait/dyn/并发/取消及六演进族维护成本仍有实际义务。

---

# draft21 历史状态：拥有动态存储组合与差分检验

版本0.1.0 draft21，schema/raw wire17，2026-10-25（Asia/Shanghai）。097–100均已按精确提交独审合入；097/100只合研究证据及生产回退，098/099为实际工具/语言增量。本轮最终正式 `20261025T100104Z-c9ffc602` 在 `e8d4a9c083d625f0d1af1f9ddd31d3417d961465` 通过35/35必需门、1134项Rust测试及36项Roam测试。前两次环境失败原件保留，不沿用下方旧轮成绩。

| 切片 | 实际推进 | 当前边界 |
|---|---|---|
| 097 | 生产Types调度与查询闭包四臂，256进程/1024请求，功能/接口/阶段产物及独立统计复算 | 三候选生产NoGo，所有crate净恢复；探索区间不是普遍退化定理 |
| 098 | 字节领域独立Rust模型+C参考+ZIL/C各O0/O2，接入原hunt/重放/缩减/关闭队列 | 有界24case/48程序/192原生执行，0发现；不冒称通用降C或自然缺陷首发现 |
| 099 | ByteBuffer直接拥有字段、拥有捕获、Package捕获、保存工厂与正确内建清理 | 保守shell来源；借入ByteBuffer capture、环境裸片返回及一般拥有聚合仍关闭 |
| 100 | 按实际收敛状态切换调度，十单元/21保留轮、440进程/1760请求 | 反链完整进程约降27.5%，三个API单元触发退化，生产NoGo并回退 |

099非作者额外执行10源边界、9组raw/wire、8次O0/O2原生近邻；另写指针身份/释放顺序观察器。两处手构审查夹具先被正确拒绝，原件与修正保留；不能算生产bug。四模块实际消费者可保存并增长动态批次；性能、维护成本与完整语言安全并未由行为一致自动证明。

后续具体语言缺口为精确字段backing路径、借入捕获/返回窗口合同、一般元素析构与部分初始化、拥有错误载荷、UTF-8接口及外部回调关闭。设计需裁决的问题见[反馈](DRAFT21-DESIGN-FEEDBACK.md)，本轮没有据此自行开放新语义。完整语言NotSatisfied、产品NotRun、B0 Doing保持。

---

# draft20 历史状态：动态字节存储与设计研究反馈

版本0.1.0 draft20，schema/raw wire17，2026-10-25（Asia/Shanghai）。093–096已通过非作者审核并合入；正式整仓RunId `20261025T072344Z-d4e3b9da` 在 `98c30f71ed953161ea6cda024d31ee6ed5e53e38` 上通过35/35必要门、1111项工作区Rust测试及33项Roam测试。精确身份以[最终收据](../evidence/final-summary.json)和[验收报告](../evidence/REPORT.md)为准，不重标下方draft19历史成绩。

| 切片 | 实际推进 | 边界 |
|---|---|---|
| 093 | 图输出下界、受限分解、原Lab0.6复现、18例真实生产反馈和128进程512请求 | H生产NoGo；研究schema16，生产crate恢复；没有通用编译突破 |
| 094 | 封闭ByteBuffer源面、工厂/give/参数来源返回、跨模块泛型窗口、CT边界 | 未约束T不获得Copy或数值权限；CT执行和拥有聚合仍关闭 |
| 095 | 独立raw/wire、真实来源/责任/loan、活性点apart义务及递归传播 | 不把不同参数名字当分离；不消费Types/Analyze证明 |
| 096 | 内联拥有头、Linux/Win64分配/失败/清理、真实动态输入及Rust行为对照 | 内部ZIL ABI，不是公开aggregate C FFI；Wine不是原生Windows |

[动态存储消费者](../examples/mainline/byte_buffer/README.md)用普通ZIL实现增长策略，处理运行时长度输入；泛型tail/put可在固定与动态backing上复用。源面、独立核和机器证据分列；原生SIGILL、allocation failure以及普通错误清理各有不同预期，不以任意失败代替正确拒绝。

设计与施工已通过[共同反馈机制](DESIGN-IMPLEMENTATION-LOOP.md)衔接。下一主线先完善拥有存储在Package、闭包环境和错误载荷中的组合，再推进一般元素容器、字符串编码及外部回调关闭义务。当前仍是有限原生资源语言研究编译器，B0 Doing、完整语言NotSatisfied、产品NotRun；[Rust四目标](RUST-GOALS.md)与[本轮架构裁决](research/DRAFT20-ARCHITECTURE.md)保留未证事项。

---

# draft19 历史状态：语言借入序列与完整编译成本

版本0.1.0 draft19；schema/raw wire16；2026-10-25（Asia/Shanghai）。本轮正式 `20261025T055135Z-7a014d0b` 在 `30e3b2df0f8f7676b58c87c2c24982b3ab281e6d` 上通过34/34必要门、1052次工作区Rust测试与33次Roam测试；历史精确身份见[当轮收据](../evidence/validation/20261025T055135Z-7a014d0b/result.json)，不沿用下方draft18数据。完整语言NotSatisfied、产品NotRun、B0 Doing保持。

| 切片 | 本轮范围 | 边界 |
|---|---|---|
| 089 | 真实Parse身份/请求资格与纵向发现复用，B/K/L/KL完整384进程/1536请求 | schema15冻结；三个生产NoGo，全部当前crate恢复强基线，旧080保留 |
| 090 | 切片源面、普通参数/返回、推断、Analyze、所有CT消费者与预算遍历 | 有限Copy元素；泛型转发可用，泛型正文范围/赋值仍需符号检查支持 |
| 091 | 独立raw/wire类型、来源、活性、loan、三个operand与预算核验 | 长度及动态边界隐式读取均检查当前借用；不接受生产者证明 |
| 092 | `{ ptr, i64 }`、先guard再GEP、返回/重借、闭包局部窗口和正常/错误清理 | Linux原生与Wine兼容分开；FFI胖指针、CT切片执行与动态拥有未开放 |

具体函数可接收不同长度的窗口；长度不再写入普通函数签名。共享与独占沿用现有来源/活性规则，不证明不相交下标范围可并行修改；可返回参数来源窗口，不可返回局部数组。闭包可对它自己或合法捕获的数组创建局部窗口，但不能捕获已有切片或保存切片到聚合。

本轮源到机器链由[三模块消费者](../examples/mainline/slices/README.md)驱动。对不同元素布局、范围、普通函数及闭包状态写回的独立执行和所有开发失败分开归档；语言能力与研究性能分别冻结，完整验收不由测试数量替代。

## 语言作为主体的下一阶段

切片提供借入接口，尚无拥有动态存储的分配/扩容/失败回滚协议。下一轮先建立动态容器的单一释放责任、移动/重分配后的旧视图失效和部分初始化清理，再完善字符串的字节/编码边界。泛型正文与同名实例方法/闭包组合仍有明确能力缺口。长期外部回调还需注册、注销和排空闭环；既有句柄及静态C回调不自动证明对象活性。

完整Rust同级安全、同语义运行/内存成本、维护优势和通用编译优势均未证明。当前是可运行的有限原生资源语言研究编译器，仍值得用实际库接口及消费者检验责任/描述模型；不以ZUI壳代码替代语言能力闭环。具体算法与阶段判断见[本轮架构说明](research/DRAFT19-ARCHITECTURE.md)。

---

# draft18 历史状态：纯 ZIL Win64 首窗与完整成本研究

本轮正式 RunId `20261025T032710Z-1fa688c9`：受测 `00d27e63ef443f5f7a81983a6d48a045d31db78c`，33/33 必需门、992 工作区 Rust 测试、33 Roam 测试通过；Win64 10 PE / 66 断言通过。完整语言 NotSatisfied、产品 NotRun，首轮 32/33 失败及旧 u8 拒绝夹具迁移保留于 evidence。

版本：0.1.0 draft18；schema/raw wire15；日期：2026-10-25 UTC。当前正式身份只读[最终收据](../evidence/final-summary.json)，各作者/独审/冷恢复分开记录；下方旧轮次保留历史身份。

## 本轮实际推进

| 切片 | 已实现或观察的范围 | 证据边界 |
|---|---|---|
| 084 | 新键失效 shortcut 的完整功能/预算与 192 进程、768 请求研究；生产恢复强基线 | 研究交付接受，生产 NoGo；schema14 历史测量，不重标为15 |
| 085 | u16、handle、静态 UTF16、静态 C callback、外部合同及跨模块严格 record | 源面/Analyze；未使用 generic/alias 形式漏检经独审返修，前端不是安全许可 |
| 086 | 独立 raw/wire 静态来源、回调定义、noescape/reentry、目标/布局及预算核验 | 不调用 Types/Analyze 求证函数；外部 unsafe 声明与 OS liveness 仍是可信边界 |
| 087 | Linux/Win64 ABI、LLVM/COFF/PE、CLI、纯 ZIL 实际窗口和 O0/O2 失败/退出清理 | 作者 10 PE/66断言；独立机器近邻另列；Wine 不代表原生 Windows |
| 088 | Source CT/collector 在复制前对声明、类型、UTF16、合同和局部形状计费；materialize 以惰性游标限制宽队列 | 原七失败基线及独审宽队列反例保留，返修1a66经独审通过，成功/拒绝语义保持；无额度提高 |

源级窗口位于[examples/mainline/win32](../examples/mainline/win32/README.md)。状态更新通过 Win32 的八字节整数槽；正常关闭后恰好一次 DestroyWindow 与 UnregisterClassW，每次绘制释放字体与两个画刷。创建失败/重复注册失败有实际退出码和清理日志。独立 C 文件仅作 ABI oracle，不进入 UI runtime。

## ZUI 启动条件与下一前沿

首窗编译—OS 事件—状态—绘制—退出链已经有可重放执行证据。可以开始抽出 ZUI 的平台启动层和纯计算布局 API。长期可变对象、动态事件队列、闭包环境与 OS 注册/注销/排空的关系尚未完成，不能把 Copy handle 或静态 callback 当作这些对象的生命周期证明。

下一主线按[架构安排](research/DRAFT18-ARCHITECTURE.md)并行推进必要字符串/动态容器与分配器、长期状态和事件订阅关闭合同、平台 API 与布局；DirectWrite/DirectX、IME、可访问性及原生 Windows/驱动测试保留独立缺口。当前没有现成独立 ZUI 仓库或 GPUI 同等框架的完成证据。

当前仍值得检验责任/描述模型在真实 UI 中的表达与维护价值，但没有证明 Rust 同级完整安全、同语义运行/内存成本、维护优势或通用编译优势。完整语言 NotSatisfied、产品 NotRun、B0 Doing 保持，原量化条件不变。

## 本轮研究结论

084 的原样请求可复用517个raw查询，但发现器仍真实解析源码；查询命中率不能当成免解析次数。下一实验拆分预算身份K与发现syntax复用L，另测相同字节流式摘要及明确投影身份。完整强基线、拒绝/取消/诊断和独立Verify保持。物理/图直觉继续生成结构假设，任何推广都需当前功能相同的完整成本证据；本轮没有新的图切割复杂性定理。

---

# draft17 历史状态：生产调度优化与字节缓冲区闭环

版本：0.1.0 draft17；内部 schema/raw wire14；日期：2026-10-25（Asia/Shanghai）。正式运行身份以[最终验收收据](../evidence/final-summary.json)为准；下方draft16章节保留历史身份，不把旧成绩重标为本轮。

本轮正式受测`07e540f2c2855ab54858bcbc1e8fac1dd3c817a1`，RunId`20261025T012212Z-7624e60c`：32/32必要检查、936个workspace Rust测试和33个独立Roam测试通过。新工作树/空target；4个完整语言检查仍未通过，完整语言NotSatisfied、产品NotRun、B0 Doing。最终交付冷恢复按旁置报告另行记录，不混算重复执行。

## 当前阶段与施工价值

当前已有Rust实现的真实ZIL编译器，独立Verify后直接生成LLVM原生程序；不会把ZIL翻译成Rust再借用rustc作为安全核。已实现跨模块、有限泛型/聚合/错误传播、有限共享/独占借用、Managed/Desc/Package责任、静态具体拥有回调工厂和正常/同步错误清理。正式schema.v1与完整语言要求尚未交接，B0仍Doing；部分能力横跨计划M1–M3，不等于这些阶段整体完成。

值得继续的目标是检验责任/描述及清理模型能否在真实UI中减少表达与维护负担，并检验可重复的编译成本机会。目前没有相对Rust的通用安全、运行/内存成本、维护或编译优势证明。如果唯一目标是最快交付GUI产品，Rust/GPUI仍有明显的现成能力优势；ZIL应以有限窗口闭环和真实对照证明继续投入的价值。不能用功能子集更小、语法短或测试数量替代比较。

## 080：已合入的实际生产优化

两阶段各自从真实callee效果消费建立反向依赖，效果变化仅通知依赖方；最终仍按原函数顺序完整验证。经典工作队列不是新图切割定理。128反向链完整source→Verify冷请求中位61.851→17.594ms；原样会话复用21.078→1.804ms，严格绑定schema13冻结测量。272进程/1088请求、七个保留配对样本及非作者完整统计复算见[080报告](research/080/RESULTS.md)。

真实组件与小例多数区间跨1；未用资源也有构图成本。Build的父进程VmHWM不含clang子进程总峰值，不声称普遍加速或内存下降。076的88.8倍仍属于研究dense-direct对BDD，不是图切割或生产收益；联合关系候选继续NoGo。

## 081–083：字节和同步借入C缓冲区

新增u8及`u8(x)/i32(x)/i64(x)`保值转换，覆盖源码、推断、raw/wire、CT与LLVM。u8保持unsigned算术/比较/移位与边界检查；转换不把目标类型反推到操作数，也不截断越界值。纯标量CT新增一次有累计预算的后序类型提示，修复右侧u8无法约束左侧无后缀常量的问题。

地址C调用仅限一层共享/独占地址，目标为u8/i32/i64或递归非空固定数组，显式noescape表精确覆盖全部地址参数。Verify独立检查词法unsafe、类型、初始化、来源、活性、重借与同期实参冲突；外部实现遵守同步、不保留、不重入及ABI/extent仍是unsafe责任。没有擅加noalias等LLVM属性。固定64字节数组不是Vec或String；UTF-8字节通行不是字符串库。

[byte_io](../examples/mainline/byte_io/README.md)提供真实流式ASCII转换、NUL/高位/UTF-8字节保存、pipe/EOF/句柄关闭、错误返回，以及65536对字节算术的独立模型/Rust对照。119次作者原生运行中，76次是ZIL/Rust四变体行为对照（其中16次用显式短写C夹具），其余为转换/陷阱/捕获回写控制，不混称全部Rust性能测量。

## 独审发现的真实遗漏

最初ffce候选对move Copy捕获的地址更新只改激活局部，没有WriteCapture。连续自增应129、130，实际129、129；普通函数、C函数、数组投影、别名、提前/错误/Unit退出均存在近邻。该候选被退回，原生错误值、IR与原审核Fail保留。

修复由root单独编写并交原非作者复核：完整捕获扫描记录按值Copy根的可变地址写，在各返回路径的StorageDead前发布当前局部值；未借出可变地址的直接赋值路径及借入捕获保留旧行为。默认配额不变；新增扫描与指令都有N/N−1。不能把原raw临时副本可读的Fn改写理解成Verify权限漏洞，真正环境WriteCapture仍必须符合FnMut权限。

## 仍缺的产品基础

一般动态容器/字符串、slice与allocator协议、完整trait/高阶/递归拥有、一般跨外部回调逃逸、异步取消和并发、平台窗口/GPU/文本/输入法及开发工具仍未完成。当前研究环境能继续承接，研究与语言各自工作树/目标目录，性能独占窗口已单独协调。下一步以真实窗口消费者给平台ABI和语言功能排序，不先把整个Rust功能表全部补齐。

---

# draft16 继承状态：组件拥有环境与强显式基线研究

版本：0.1.0 draft16 集成稿；规范：ZIL-LANG-1.3/A；本轮 schema / raw wire 发布版本：13；日期：2026-10-25（Asia/Shanghai）。

正式受测`3c486ec89d76f41df3a0cba06b7eb48af43bd7a8`，RunId`20261024T183734Z-b2bb6f1f`：32/32必要检查、880个workspace Rust测试、33个Roam测试通过。首次27/32为恢复环境缺少Roam依赖；按原锁补齐11个crate后，同源码在新工作树/空target重新通过，原失败及环境恢复收据保留。另4个完整语言检查仍未通过，完整语言NotSatisfied、产品NotRun、B0 Doing不变；[精确收据](../evidence/final-summary.json)与[报告](../evidence/REPORT.md)区分当前有限实现及全语言范围。

## 本轮主线：077–079 组件拥有环境

完整有限Package通过显式move进入Fn/FnMut/FnOnce环境，可由普通工厂返回并跨同步事件保存。普通工厂输入允许完整Package，callback显式输入/结果仍有限Copy。每个Package捕获有唯一入口词法view，真实data、shell及直接Managed字段flag同址；Bind/End只改变词法事实，不移动数据、清理或重置责任。完整Package普通值ABI保持，环境ABI和wire更新至13。

源码模式推断用有预算的字段状态流区分只读、所有返回均恢复与消费；Analyze从实际IR重建，Verify另从原始TypedIR/wire独立重建。Fn禁止直接/经callee写入，FnMut普通及Error返回完整，FnOnce可部分或整体消费后由caller清余项；Trap仍abort。伪造重复view、错索引、普通Storage重置、ReadCapture/TakeCapture绕过、整包借用、失效Desc和Shared写均有反例。

三个owned_component入口提供20固定输入×ZIL/Rust各O0/O2共80次行为对照。非作者另用不链接Types/Analyze的16次raw/wire控制及异形双Package混合捕获O0/O2测试核验独立性、偏移和清理顺序。真实fd入口检查环境跨工厂返回仍持有句柄、替换旧字段立即关闭、最终余项关闭。它不是平台GUI或相对Rust运行性能测试。

审核修正了End→Trap误加恢复义务及scope view查询计费遗漏；原始失败和旧预算保留，最终Types918/917、Analyze26221/26220、Verify2793/2792分别核对N成功/N−1限额。源模式推断不是安全许可，独立核验仍必须执行。完整语言NotSatisfied、产品NotRun、B0 Doing不变。

## 本轮研究：076 强显式控制与整态共享

在冻结schema10上新增dense-direct、dense-intern、local-intern，与原五方法保持完整观察及独立oracle。53项测试、544查询中的526 Complete全量canonical相等；10 ResourceLimit、8 OracleResourceLimit如实保留。504采样进程包含112预热、392保留。

四guard完整候选dense-direct4.268ms、local-direct4.523ms；后者配对比1.060，探索性95%区间[0.935,1.144]。整态intern未建立相对强基线的普遍净收益，小例回归保留。dense-direct自身相对dense-BDD约88.8倍候选/40.6倍整进程，意味着071大幅改善主要来自直接执行器；不是图切割、生产或Rust整体提速。完整行输出仍可能指数增长，当前生产NoGo。下一结构方向和三种源码身份见[076结果](research/076/RESULTS.md)及[架构研究](research/DRAFT16-ARCHITECTURE.md)。

## 继承的draft15主线：072–074 直接字段取走、补入与替换

普通`give p.field`转交直接Managed字段，`var p`可通过普通赋值补入或替换，有限Copy字段可直接更新。新资源RHS先由真实临时责任守护，准备成功后显式条件Drop旧字段再Store新责任；失败不提前清旧字段。LIVE/MOVED分支汇合与循环恢复均有真实源码/独立raw核验。部分包保留活动外壳及Copy字段，整包跨SSA/参数/返回边界必须完整。

Verify独立维护字段债务、精确Desc根、SSA守恒、壳清理次序与配额；字段槽不进入外层局部初始化次序矩阵。LLVM仅对发生字段操作的局部包分配入口固定责任flag，完整ABI保持原inline struct，不引入堆盒、RC或全局世代。`update_ui`的20输入在四个ZIL/Rust O0/O2变体完成80次行为对照，并验证真实fd关闭；其余源码近邻、独立bool责任模型和预算控制见072–074审核收据。

边界仍是无整体自定义Drop、直接Managed与有限Copy字段；嵌套拥有包、包内拥有闭包、动态容器和平台GUI尚未完成。本轮实现没有改变全语言NotSatisfied、产品NotRun或B0 Doing。

## 继承的draft15研究：071 局部域与直接同步转移

冻结schema10上的五方法完成328个函数全关系逐字对照，保留7候选限额与5oracle限额。7族245正式样本、70预热显示完整候选成本的明确改善；四guard322.422→3.834毫秒，整进程约41.4倍改善。RSS口径是包含前端/oracle的整进程。优化显式dense/MDD、生产与同能力Rust未对照；输出完整关系的有限接口不能外推整体编译提速。研究交付Pass，生产替换NoGo。

详细因果、收费、census措辞限制和负结果见071/RESULTS与DRAFT15-ARCHITECTURE；正式计时与语言构建错开，同一对话可隔离推进。

## 继承的draft14：069–070 自包含工厂环境返回

源码 `-> impl Fn/FnMut/FnOnce` 由一个实际隐藏环境类型满足，按普通函数直接调用返回，不在调用点强制内联工厂正文。工厂输入和捕获只允许有限Copy及拥有Managed；独立Verify检查raw边界、唯一返回身份、精确kind和完整cleanup，不借前端检查授予权限。公开接口的callable_returns只在Verify后发布，Typed/raw wire同步升级11。

调用者可以保存、移动、借入或消费返回环境，FnMut跨同步事件循环更新同一个状态，FnOnce消费或未调用销毁均结账。原生环境沿已有内联struct携带责任位，不隐式分配堆盒或RC。五模块retained_ui的20输入在ZIL/Rust O0/O2完成80次行为对照，含工厂临时与返回环境分开存活、give实参进入后返回、未调用、两字段消费、错误回调及较晚实参失败。

本片只允许一个隐藏身份的创建位置；不同闭包分支、借用逃逸、工厂直接递归/转发、工厂内generic helper、一般trait opaque、嵌套拥有环境、OS注册/取消排空未完成。声明kind与实际kind必须一致。完整语言与产品门状态不变。

## 继承的draft14研究：067 精确位置编码，068 纵向共享

067在冻结schema10上实现责任token位置编码，258个Complete函数查询与独立raw oracle完整canonical相等，8 ResourceLimit与2 OracleResourceLimit均非完成。关联编码缩短后因子支持变宽，四guard fused中位比1.802，主要样例更慢，当前候选NoGo。精确工作预算原始日志N=545002；作者摘要误抄345002已由独审发现并在更正收据保留全过程。

068在生产请求通道复用不可变syntax、Typed借用和输入身份。52项测试及独立私有编辑/恢复等控制通过；正式schema10的288请求、144对语义、576阶段hash、192次native对照一致。三类重复确实减少，普遍稳定的时间收益未建立，峰值内存未测。详见各研究目录及DRAFT14-ARCHITECTURE。

## 继承的 draft13：064–066 具体环境公共 helper

实际源码以 `F: Fn/FnMut/FnOnce` 声明一个具体环境参数，在调用者环境类型已知后生成独立实例，保持定义模块绑定。`&F` 借用拥有环境，`&mut F` 修改原环境；按值 F 可条件消费或零次调用后清理。Fn 可以适配 FnMut/FnOnce，FnMut 可以适配 FnOnce；实际入口ABI和剩余字段清理随适配执行，不只更换标签。

公开合同包含具体参数/环境/请求模式、captured Address Hold、Address/Desc Call依赖、保守写上界、Owned/Borrowed Cleanup及最多两次调用的Return/Trap轨迹。Verify独立重建并核对，caller不读取helper私有写效果摘要；BorrowLocal保留环境位置和其隐藏捕获借用。Return包含Copy错误联合任一标签，本片不导出按错误标签区分的关系。

[helper_ui](../examples/mainline/helper_ui/README.md)的10个UI输入与15个生命周期输入在ZIL/Rust O0/O2完成100次行为对照。源面、Analyze与raw/wire Verify的最近反例另按065/066收据计数。首次接口hash panic、已知资源字段误拒、Address<Closure>机器表示遗漏及缓存/静态检查异常均保留。测试数量不等于完整安全证明或性能优势。

该片只有一个有限同步非逃逸环境参数；callback参数/结果为有限Copy，额外helper参数为Copy/Managed give/完整Package。无helper循环、递归或转发，无环境返回/保存、dyn、拥有错误、异步取消或平台GUI；FnMut helper仅通过&mut F。详情以schema登记和064–066精确提交为准。

## 继承的 draft13 研究：063 联合关系表示与实际编译成本

063从059四条件BDD限额定位临时节点累积，分别研究完整root集回收、owner相关变量顺序、融合关系积与提前消元。控制组含同一改进后的单体算法；关系、预算、确定反例和完整总成本分别核对。原文倒序图切割的结构性机会继续研究，当前工程改进不冒称UP定理的编译迁移。实际driver另比较cold/reuse/edit/Build成本；未请求Debug阶段产物的格式化已改为lazy，原Verify和必要身份计算保留。精确结论见[063报告](research/063/README.md)，旧059数字不改写。

## 继承的 draft12：060–062 完整拥有 package

`Type::Package` 是具有名义形状的有限内联非 Copy Value。至少一个 `manages` 字段为已有 Managed 资源，其他字段为有限 Copy。包整体沿普通 Value 的 Take/Store/参数/返回移动，不用 `give`；`give package`、Copy Load、包进入闭包捕获/错误载荷/嵌套聚合等未开放形状明确拒绝。

构造先按源码求值顺序准备真实 owning locals，全部成功后无失败地提交 MakePackage。准备阶段 `?` 清理已成功临时，按实际成功初始化逆序；完整包 DropPackage 按声明字段序清理。一个整体责任状态足以描述当前禁止成品部分移动的载体；字段 Desc 仍保留精确 `(local, index)` 身份，别名、分支和局部闭包捕获不能绕过 whole move/cleanup 检查。LLVM 在调用任何字段 teardown 前清除本地整体 live flag，字段存放内联，无隐式堆盒、RC、锁或 Desc noalias 承诺。

[package_ui](../examples/mainline/package_ui/README.md) 的三个真实源码入口覆盖四模块有界事件环、准备失败/晚实参错误及两个真实 Linux fd：18 个输入 × ZIL/Rust O0/O2 = 72 次原生行为对照。UI 组合状态更新、dirty 重绘、字段描述回调与整包往返；它没有平台窗口/GPU/字体或跨帧保存的捕获回调。首跑发现跨模块接口哈希漏分支并 panic，修复后原例通过；首次失败保留在060/061收据，不将崩溃算作语义拒绝。

061负责源码、类型及独立 Analyze；062从原始 TypedIR/wire 另行验证全函数责任与字段来源；060单写公共表示、传输及机器路径。正式全量身份与计数只读本轮 final-summary，不累加作者/独审/历史重复执行。

## 继承的 draft12 支线：059 有限联合关系与切割

059冻结draft11/schema8依赖，在真实TypedIR每个原始点保留有限完整联合关系，以独立直接raw解释的枚举oracle核对mono/cut/wire。完整核对包含全部行和基数，不能用相同摘要代替关系等价。当前切割是单事件位方程的提前存在消元，wire增加受前置约束的恒等/取反等式代换；全函数仍是联合固定点，没有实现跨CFG区域摘要，也没有替换生产核。

10个入口中8个完整完成、覆盖43个可执行非foreign函数；2次warm加7次正式循环、4种方法产生64次warm和224次正式进程运行，其中903次为shadow函数完整关系核对。相关分支实例的精确联合关系无资源故障，而现有生产合流返回E-UNINITIALIZED，显示精度机会。当前cut/wire尚无稳定整体成本优势，候选NoGo；四个独立条件例触及300000 BDD节点上限，17个Bool输入例触及oracle选择上限，均非Pass。完整抽象、成本、失败及固定样本见[059结果](research/059/RESULTS.md)。

研究固定算法/驱动为c0c1dc7746bfb4f5f819b10bc68aff579e8fbf46，依赖为6acda08c72dbd3e387893b4260770f0834dcdbe0。它不能直接套用当前schema11依赖重放旧数字。调用按有限合同抽象，数值含Top；未来Desc查询为不区分guard的保守may，位置不代表无界动态代次，同址重叠明确UnsupportedMultiplicity。此有限联合子集已经实现，一般联合语言语义和生产可替换性仍未完成。

## 继承的 draft11：同步错误主线

056/057/058在同一schema8上完成有限Copy错误联合`E!T`、后缀`?`和显式词法存储作用域。错误与成功载荷均有实际类型；唯一一侧匹配时注入，同型歧义拒绝，错误类型不隐式转换。`?`只计算一次操作数，成功投影T；失败先保留E，再清理本侧实际持有的资源/拥有闭包并返回对应错误联合。普通Return可以返回两种tag，不能将Return标签视为成功证明。

所有有可执行源码函数体的非foreign函数生成root及词法ScopeEnter/ScopeExit；foreign声明没有源码CFG。Analyze与Verify分别重建scope栈、存储归属和CFG汇合；独立Verify仍核全函数拥有SSA/局部责任守恒。scope约束存储，块结果的拥有SSA可以跨scope转交。临时StorageLive在词法入口保留空槽，实际Store才建立责任；先求值的give参数和FnOnce callee等待后续实参期间有真实临时载体，后续`?`失败可准确清理。unsafe退出在storage退出之前完成，不把已结束scope内的区域指令留在错误出口。

LLVM使用内联`{i1,E,T}`，两侧载荷存储均保留，非活动侧置零；投影先核tag，错误tag投影即使结果为未使用的Unit也真实trap。源main的E!i32成功返回该i32，E!void成功退出0，错误退出1且不自动打印；清理在main返回前完成。此表示没有新增隐式堆分配/RC，但不是紧凑联合ABI，也没有Rust内存同级结论。

[四模块应用](../examples/mainline/error_cleanup/README.md)的15输入在ZIL/Rust O0/O2上作60次行为对照，另有5条原生入口/载荷路径及6条精确Types拒绝。非作者另作12次原生近邻检查，含宽整数嵌套布局、Unit错误投影、待提交callee和部分移出捕获；身份与计数见[056作者收据](../evidence/impl/056/acceptance.json)及[独立审核](../evidence/reviews/056/review.json)。056测试数与原生执行次数不相加。057源至Verify的33案例与058原始IR/wire反例各保留自己的证据等级。

纯CT的scope指令仍按真实VM访问计步；新作用域不改变值，但查询入口和callee增加实际指令。累积预算夹具检查12步单查询、12/23步双查询拒绝及24步双查询通过；没有提高生产默认配额或放宽产品门。六个错误源反例进一步锁定阶段/理由，发现旧`3i64`拼写会先在Lex失败，现用合法i64绑定抵达目标类型拒绝。最初失败与修后通过均保留。

055把图研究推进到真实TypedIR的只读抽取、独立raw图审计、完整may-liveness对照及透明节点消去/逆序恢复。055本身没有联合状态求解；分支标签交换反例说明may事实相同不足以证明关系等价。冻结实验保留draft10/schema7身份，新schema8只做功能影子核对；详见[055入口](research/055/README.md)与[完整结果](research/055/RESULTS.md)。19源3192样本的配对cut/block总成本中位比均大于1；当前恒等点候选暂不入核，研究未改变生产接受集合或替换Verify。

## 继承的 draft10 主线

051/052/053把本地资源闭包接入源码至LLVM/native全链。普通资源捕获为Desc，调用时读取当前目标；`move` Managed或正文`give`在创建闭包时转交实际责任。拥有后只读为Fn，共享调用可重复；修改拥有目标为FnMut；移出捕获责任为FnOnce。拥有捕获不自动成为FnOnce。普通Desc捕获写外部目标仍为Fn，直接/递归/跨模块helper的描述写效果参与推导；读取Copy字段后修改副本不算修改原资源。

拥有环境可以转交，不能复制。创建但未调用的环境仍自动销毁；Consume调用清理尚未移出的字段。正常return、块结束、break、continue和完整表达式临时进入真实CFG清理。局部按逆成功初始化顺序清理；环境字段按捕获首出现的固定顺序清理。Analyze与Verify分别重建来源、责任和写效果，Verify不调用前者求证；LLVM使用内联环境与真实live flag，没有为本片引入隐式Box/RC。

三模块程序`examples/mainline/resource_closures/main.zil`用12个输入覆盖描述重读/写、拥有后重复读/写、消费、未调用、环境转交、循环退出及显式错误值。ZIL与独立Rust程序分别在O0/O2运行，共48次原生执行，核对stdout、退出码和空stderr。精确受测身份见[evidence/impl/053/acceptance.json](../evidence/impl/053/acceptance.json)，完整输入表见[CODEX-START](CODEX-START.md)。其中`Outcome`是普通用户枚举，构造边界上的错误分支通过正常Return清理已完成资源；这不代表`E!T`、后缀`?`或一般错误展开已实现。

054将Rust `resource-closure-native` profile接入已有`zil qc` hunt/issue发现、重放、缩减和独立修复通道。限定4项测试通过，另一次无注入hunt完成8个case、32个动作，未发现真实生产缺陷；`closure-omit-call`是验证通路的受控注入。051的Copy快照误拒来自手工边界回归，不归功于hunt。模型及精确身份见[evidence/impl/054/acceptance.json](../evidence/impl/054/acceptance.json)与[工具说明](../tools/roam-rs/README.md)。

050交付独立图关系研究原型、慢oracle和计时前冻结输入，未接入生产TypedIR。窄链16相对同一研究程序的单体关系基线约217.7倍加速；dense_12候选耗时约为基线5.34倍，小例、闭包与owned模型也有退化。宽观察边界的约1.19倍差异来自执行路径，不能归因于切割收益。所有形状、离散度与阶段成本见[050结果](research/050/RESULTS.md)；这些不是ZIL完整编译或Rust对比结果。

047–049既有有限Managed/Desc、give、create/give结果模式与正常退出资源清理继续保留。它们的三模块20次ZIL/Rust轨迹及4次Linux fd/OwnedFd对照属于原受测提交，见[evidence/impl/049/acceptance.json](../evidence/impl/049/acceptance.json)。046返回来源调度把96函数反向链重求解从9312降到191，256链完整check约1.615倍改善；其固定小例仍慢9–22%。050的新研究数字不抵消046的退化，见[PROOF-CUT-PERFORMANCE](PROOF-CUT-PERFORMANCE.md)。

## 本轮仍未完成

资源嵌套/替换、一般拥有聚合及部分移动、逃逸闭包、嵌套拥有环境捕获、资源型闭包显式参数/返回、Desc返回或聚合保存/逃逸、Address<Resource>尚未完成。已有闭包环境作为另一闭包捕获项的一般组合仍未开放。本地Desc捕获、拥有Fn/FnMut/FnOnce及同函数资源与Copy闭包共存已经实现，不应再列为缺口。

有限词法存储scope、Copy错误联合及同步传播已完成；一般拥有聚合逐字段初始化、拥有错误载荷、panic unwind、cancel、异步frame、并发与SABI仍未完成。Copy字段求值失败不会构成资源对象，不应外推为拥有聚合的完整部分初始化。当前checked trap仍为进程abort。一般trait/求解器、动态容器、字符串和完整标准库保持原缺口。

正式B0仍Doing，完整语言NotSatisfied，产品门NotRun；schema10不等于全形状schema.v1完成。正式全门的受测提交和计数只读[evidence/final-summary.json](../evidence/final-summary.json)及[REPORT](../evidence/REPORT.md)，不累加历史切片成绩。下一阶段分工以[RESOURCE-HANDOFF](RESOURCE-HANDOFF.md)和主树派单为入口。

## 已得到的实际程序

`examples/mainline/stream/main.zil` 把 042–044 放进同一个五模块程序：getchar 读取真实 stdin，逐字节解析整数 token，经公共 `Option<Result<i32,i32>>` 传递，用函数指针选择转换，默认可变捕获直接更新外部 Stats，再由普通 ZIL 模块格式化并调用 putchar 写 stdout。源码完整经过解析、展开、类型、分析、独立核验、LIR、LLVM 与本机链接。

输入按 ASCII 空白切分；有效整数绝对值至多 1000000，依次交替作 `x * 2` 和 `x + 11`，输出 `accepted rejected total`。非法 token 计入 rejected，空输入输出 `0 0 0`；超过 1000 个 token 退出 2，不输出部分统计。putchar 当场返回失败时退出 3。应用限额不作为语言限制。

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/stream/main.zil --output out/stream --emit-dir out/stream-stages
printf '10 20 -3\n' | ./out/stream
printf '1 broken 2 -4 +5\n' | ./out/stream
```

结果分别为 `3 0 45` 和 `4 1 23`，退出 0。主程序不使用 `--entry` 打印 wrapper。Rust 对照用标准字节切分和整数解析，与 ZIL 状态机独立。

stream历史行为对照涵盖10组输入的ZIL/Rust O0/O2，以及CLI普通/JSON pipe。本轮是否再次执行、受测身份和完整测试计数只读[REPORT](../evidence/REPORT.md)与[final-summary](../evidence/final-summary.json)；不将draft08的538项测试移记为本轮成绩。

完整语言仍 **NotSatisfied**，产品门 **NotRun**，正式 B0 **Doing**。各切片通过不替代完整接口移交或正式 T01–T09 / W1–W4 的 Done 条件。

## 继承的042–045增量与实际边界

| 切片 | 已贯通能力 | 关键限制与拒绝 |
|---|---|---|
| [042 一层独占地址](../evidence/reviews/042/review.json) | `&mut`、独占/共享重借、间接写入、投影、直接调用/返回；独占地址与环境转交；默认闭包写外部 Value | 拒绝同源冲突、活子窗口期间父访问、共享升级、转交后使用和局部逃逸；目标限有限 Copy，整对象来源，无嵌套地址/两阶段借用 |
| [043 公共模块与有限模板](../evidence/reviews/043/review.json) | 公共普通记录/枚举/函数、泛型函数/记录/枚举、非泛型透明别名跨模块组合；定义模块 helper、名义身份与字段隐私 | 私有声明/字段读写借入和源构造拒绝；CT opaque 工厂结果可复制；跨模块省略字段默认值、泛型别名、一般 bounds/泛型方法未完成 |
| [044 标量 C ABI](../evidence/reviews/044/review.json) | 私有无正文 extern C，i32/i64 参数，i32/i64/Unit 返回；显式 unsafe；真实 getchar/putchar | 普通 Call/FunctionRef/CT 不可绕过；闭包需自己正文的 unsafe；指针、varargs、bool/聚合、未知 ABI 等拒绝；C 实现遵约仍可信 |
| 045 根集成应用 | 五模块动态输入、公开泛型、私有解析状态、独占更新、默认捕获与 native 输出组合；旧固定数据应用显式公开应开放的数据字段 | 有界行为对照、CLI 与本轮统一必要门通过；完整 IO、字符串、一般资源与标准库仍未完成 |

042 的实际机器操作保留独占来源、父子窗口和转交，不把非 Copy 地址变成普通 Load 可复制值。含独占地址环境不能复制，调用需可写位置；**只读 `move &mut` 捕获目前也保守按 FnMut 处理**。默认捕获已有地址会按读写用途重借，不再一律归为嵌套地址 Unsupported；嵌套地址本身、重绑定被捕获地址变量、部分形成静态自依赖的重借循环、一般环境逃逸仍未开放。

043 的模板实例留在定义模块；透明别名与同模块两个导入名不制造名义类型，跨模块同名同布局也不合并。源私有构造在展开处拒绝，实际字段索引访问在类型阶段检查；合法 CT 结果重化属于复制，后续仍重新核验。新增 Core 可见性元数据不是可执行权限证明。

044 的独立核验检查完整声明、参数/结果和 CFG 的 unsafe 区域栈，要求汇合一致。外层 unsafe 创建闭包不授权它将来的调用，普通函数指针也不携带 foreign 调用权限。链接到的 C 必须遵守正确签名、无跨栈展开等约定；这是可信边界，不是编译器静态证明的 C 正文。getchar 的 EOF/底层读错与进程退出时的最终缓冲刷新错误仍不能完整区分或观察，libc 的缓冲、锁和可能分配仍算外部成本。

## 继承的可运行基础

| 范围 | 已有能力 | 尚未完成 |
|---|---|---|
| 值与控制流 | i32/i64/bool/Unit，有限 Copy tuple/record/array/enum，构造/更新/投影、checked/wrapping 算术、范围循环、模式条件 | 字符串、动态容器、一般嵌套/部分移动的拥有聚合；普通不可达源码类型形状检查仍有 Unsupported 缺口 |
| 共享来源 | 一层共享参数/返回、字段/索引投影、重借、共享接收者、独立返回来源合同 | 不交字段/区间权限、一般嵌套地址、escaping callable 合同 |
| 闭包与函数值 | 默认共享、owned Copy FnMut、副本独立、函数指针与明确 fn 上下文的无捕获 lambda；本地Desc/Managed捕获、拥有Fn/FnMut/FnOnce、未调用清理与环境转交 | 资源型闭包显式参数/返回、一般环境存储/逃逸、嵌套拥有环境或已有闭包捕获、dyn/opaque |
| 编译期 | 纯直接调用、递归/控制流、有限 tuple/record/array；独立核验和有界 VM | enum、地址、函数指针/闭包、foreign、Type/Syntax、反射与代码生成 |
| 模块与驱动 | 点路径、唯一文件或 mod.zil 入口、显式内存模块图、保守依赖失效、实际 native | 完整包管理和细粒度公开合同增量 |

### draft07 历史身份

025–041 把共享引用、Copy 聚合、有限泛型、模式条件、CT 与函数值组合进 `examples/mainline/app`。它使用编译期固定八行数据，历史完整结果为 **475000738663**，普通 main 自检退出 0；当轮 ZIL/Rust O0/O2 行为证据仍只属于当轮受测提交。draft08 按字段隐私规则把应用确实对外提供的声明和字段改成显式 pub，未放宽隐私。

历史实现、审核及合入见主树 `.task/impl/<号>.md` 和对应 evidence。首次全量预跑的旧失败仍保存在 `evidence/integration/round07-preflight.json`；修复不改写旧失败，新一轮成绩也不通过相加旧测试得出。

## 可信边界与版本

TypedCFG、CandidatePlan、CT 重化值、缓存及 raw wire 都只是候选。Verify 独立重建来源、初始化、活跃性、窗口与转交；已核包装私有构造，后端还校对 LIR 与同一已核模块。安全核不调用宿主 Rust 借用检查器，不以属性、哈希或未核标签授予事实。

本轮由集成者统一发布 schema/wire **10**，保留原Package等tags，新增CallClosureBorrowed（53）及完整CallableParameterContract元数据、Core模板参数与已核公开接口。raw版本1–9拒绝，Typed fingerprint为V10；签名指纹绑定实际环境布局，接口合同指纹绑定全部公开关系。历史收据仍按原版本核对，当前规范摘要见`data/normative-inputs.json`；版本10不等于完整正式`schema.v1`移交。

语言保持 **1.3/A**，`&T? = (&T)?`。有限`E!T`和后缀错误传播已接通；`T?`、`?.`、`??`以及问号前缀实验仍不在该实现范围。逐项move、`@`与逐项捕获写法仍是候选。

## 下一阶段与目标判断

下一包优先打通有限非逃逸高阶 helper 的具体环境参数/调用合同，再推进更一般的拥有组合、替换/部分移动与保存。完整包和准备失败清理已贯通，不重新派发；保持源至独立核验至机器路径同批闭合。可复用库先消费已核能力；escape、一般高阶合同、unwind/取消按真实前置拆片，见[RESOURCE-HANDOFF](RESOURCE-HANDOFF.md)。

图研究继续以完整事件与出口为输入，推进路径相关的联合关系而非仅对may集合压缩；先找出生产Analyze/Verify的可替换热点，计入抽图、构造、恢复、独立核对和回退。055研究中的宽度、收益及退化不与046/050跨合同相加，性能身份只读各自报告。

Rust四目标及至少一项突破的本轮裁决见[RUST-GOALS](RUST-GOALS.md)：有限错误/清理有行为实证，完整安全、运行内存、维护和同能力Rust编译成本尚未达到产品结论。

现有`zil qc`已有session、scalar-native及resource-closure-native领域，共用独立oracle与问题通道；本轮没有新建质量平台。真实发现归属和覆盖边界见[ROBUSTNESS](ROBUSTNESS.md)与各切片收据。实测平台仍为Linux x86_64、Rust 1.99.1、clang 23.1.4。
