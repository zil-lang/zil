# 编译7 增量编译与完整成本

状态：本轮探索交付完成，长期专项仍 Doing。父项：`research/compile-performance`，用户2026-10-31授权继续专项与增量投入。本地基线 `ee72fd6f701efa5f7a75ea3f13d4effc20d5c4b8`；恢复时可读语言主线 `42ce8081a2aa5722a14fea53f23f819f46685a60`；本轮只读核实远端main已到 `b8c93e70d44038d4ea59affa4c972c706404dac5`。本轮不宣称已吸收语言主线后续功能，不改正式验收门与公共schema。以下派工和计量安排按当时顺序保留，最终结果见末节与 COMPILE7-RESULT。

恢复：独立检出目录，读取旧对象库只作为来源；3526份非历史证据文件逐blob核对恢复，未物化84272份历史/大型证据。原handoff失败及稀疏历史限制如实保存，不改成Pass。研究候选由非作者独审；原始窗口失败保留。仅在无其他编译/测试负载时测性能。

| 切片 | 作者 | 写集 | 工作与验收 |
| --- | --- | --- | --- |
| CP29 | frontend_incremental | crates/zil-types/**、crates/zil-driver/src/session.rs、crates/zil-driver/src/project.rs、crates/zil-driver/src/lib.rs（one-shot构造器一行）、crates/zil-driver/tests/cp29* | 有限标量项目Types原始结果复用；完整有序声明/类型/模块/目标/配额/位置身份；计费回放及失败回滚；其它语言形态原路径；fresh Analyze/Verify。先做进程内实际私有编辑窗口，跨进程不冒称完成。 |
| CP30 | incremental_research | crates/zil-db/src/lib.rs、crates/zil-db/tests/cp30* | 基于发布不变量的无active读取快路；active路径不变；独立DAG/失效/取消控制，完整持久会话A/B。 |
| CP31 | root | tools/research/compile-performance/compile7*、docs/research/compile-performance/COMPILE7*、evidence/research/compile-performance/COMPILE7/** | 建立严格版本/身份的会话编辑窗口；源码备份开关成本单列，保留历史基准默认；隔离旧新ZIL和Rust新CLI窗口。 |
| 独审 | cp25_evidence | 仅自己的审核报告与控制 | 审核CP29/30身份、资源预算、输出/诊断和缓存安全；不兼任作者。 |

CP25仍是可行性研究，以上实现不回填其历史性能数字。ZIT机制证据与ZIL完整编译收益分开。默认采用以完整成本、正确性、内存与反例为依据；没有净收益的候选保留研究提交，不放入当前可合流生产树。

全语言安全、运行、表达维护、编译四目标及至少一项突破仍需同一受测实现分别证明，本轮不能将局部增量加速替代产品终态。

写集补记：根协调者已授权CP29在driver/lib.rs中将单次compile_path的构造器改为不启用Types memo；与project.rs单次入口相同，防止短命CLI无后续复用却承担epoch建立成本。此处不改变持久CompilationSession的实验行为。

## 持久会话正式计量预定口径

沿用210完整五语料，不因CP29适用性筛选有利样本：tree4x2、tree32x8、tree256x8、tree1024x8、retained，权重分别0.05/0.15/0.30/0.30/0.20。主混合请求为cold/unchanged/private/public，权重0.10/0.35/0.40/0.15。restore/private_again/restore_again另报，用于回退和连续编辑观察，不用来替换主负载。默认query容量与8192分开计分，不跨容量平均。

每容量每语料5个ABBA/BAAB区块；先在每区块内对相同arm重复求均值，再对每个语料/请求格取五区块中位数，按上述固定权重汇总绝对毫秒，最后相除；不平均各格比值。并保留整个7请求进程的wait4 wall、CPU、maxRSS以及复制/初始化/报告成本。主计时为完整compile_path到Verify，含项目发现/读取/前端；LLVM与链接未包含，不能称AOT端到端或对Rust整体领先。

正式窗口前冻结源码、二进制、同源harness、fixtures、工具、环境和host；成功fresh-session控制与正式输出互比，Verify必须每请求真实执行。停止本轮已知编译和测试，保留共享宿主不能保证硬件独占的边界。任何失败或身份改变整窗NoScore，不能拼接有利记录。CP29与CP30先各自对基线，组合候选另测，避免合并后无法归因。

## CP32：根据CP29完整负结果继续攻坚

CP29五区块完整窗口200序列/1400请求成功，默认容量主混合B/A=1.070692，8192容量B/A=0.994444，未建立净收益；不采用a0作为准备合流的生产树。根授权frontend_incremental从a0f38d2b新分支research/cp32-bounded-type-retention继续，cp25_evidence独审。写集限crates/zil-types/src/program_memo.rs、program.rs、PROGRAM-MEMO.md、限定cp32类型/driver测试，必要的公开统计访问点需仅用于诊断且不改CLI/schema。

实现目标：分开全图语言资格与存储容量；完整全局环境仍绑定；保持64MiB/16384模块上限；按稳定顺序仅保留容得下的模块，容量不足不丢弃其它模块；确定入选后才做缓存快照clone；完整Core相等后复用旧资格及源权重收据。所有元数据计入结构上限，原共享预算、首错、Span、事务提交及fresh Verify不变。先区分当前source-accounting和typed-accounting拒绝原因，不把eligible=false直接称为无用clone证据。

先做限定正确性/容量/失败控制和非作者审核，再对同ee72基线、同五语料/两容量/五区块完整计量。只优于a0而对基线无净收益仍NoGo。CP30正式窗口期间仅可编辑和静态检查，不运行编译或测试。

## CP33：Parse剩余预算身份与纯内存收据

阶段与源码分析证实，公开签名编辑增加前部文件token数量，改变后续所有文件的remaining.max_tokens/max_nodes；load_sources与project_pipeline都把它们纳入Parse查询身份，未改文件因此失效。原private仅7改11不改变token数，故仅一个模块重解析。根派incremental_research从ee72fd6新分支research/cp33-parse-budget-receipts移植旧CP22已经验证的纯内存部分，cp25_evidence独审。

写集：crates/zil-syntax/src/lib.rs、parser/mod.rs、attributes/mod.rs；crates/zil-driver/src/session.rs、project.rs；cp33限定syntax/driver测试及说明。ParseUsage记录parser nodes与attribute work的实际需求；仅把两个可回放额度从稳定Parse身份中分离，命中必须fit当前余额，不足回原完整键/原parser保留首错、Span和顺序，其它limits、规则、源/路径/FileId/ModuleId继续绑定。项目仍逐请求收费和解析当前import路径/歧义。不得携入parse_cache.rs、cache codec、磁盘目录、producer hash或fsync，也不新增一层AST保留缓存。

使用原五语料公开编辑、降低额度、属性work、错误优先级、失败/取消、文件增删/歧义、fresh Verify等限定控制；非作者审核后再跑同ee72完整固定窗口。任何原CP22性能数字不回填本切片。

## CP34：TypedModule 指纹的等价流式编码

CP32完整窗口已通过但根初算默认B/A=1.003583、8192=1.060297，仍不采用；CP33正式计量中。静态检查发现TypedModule::fingerprint先把完整模块编码到Vec再做SHA256。根授权incremental_research从ee72fd6独立新worktree实施CP34：仅将相同有序字节流直接送入既有FingerprintBuilder，降低反复身份计算的临时物化；frontend_incremental负责独立源码/等价性审核，根负责完整性能，cp25_evidence复算。

写集限crates/zil-ir/src/lib.rs及cp34限定IR/driver控制与说明；不修改zil-base的SHA256实现，不改变V27标签、任何字段顺序、端序、Span、target JSON、schema、预算、诊断、Verify调用次数或权限。保留原Vec编码作为测试oracle，覆盖全部可达编码变体及实际源产生的TypedModule，核旧新精确指纹。CP33计时结束前仅可编辑，不构建测试。必要时在单次统一sink中缓冲小字段，但须如实计入，不能另增加未经证明的结果缓存。先限定控制/非作者独审再以同基线和同冻结完整会话窗口计量；CPU/内存/冷路径与全序列并列，不能只报编码微基准。

## CP35：fresh Verify 共享不可变输入

CP33完整200序列/1400请求通过，根初算默认B/A=0.975870、8192=0.947398，待非作者复算；正式计时已经停止。静态确认finish_candidate每请求深复制TypedModule/CandidatePlan，而check_module仅借读；根授权frontend_incremental从ee72fd6独立CP35，与CP34互不叠加，cp25_evidence独审。

写集限crates/zil-verify/src/lib.rs、zil-driver/src/session.rs、project.rs及必要有限测试/说明。保留旧owned验证入口，新私有owner可持有已有Arc<TypedModule>或Arc<TypedProgram>与Arc<CandidatePlan>，项目不得先clone内嵌module再包Arc；后续module()/plan()只借读。每次调用原check_module，保持检查与预算/首错顺序、取消点和Verify计数；raw snapshot不得持有Verified权限、成功位或验证票据。共享program入口仅授予module权限，interfaces不得扩大为已验证结果。既有Owned调用不得无端增加Arc分配，wire/schema/哈希/接受规则保持。

限定控制覆盖owned/shared全结果与诊断等价、Arc别名/clone-on-write隔离、实际当前额度、单源/项目/磁盘checkpoint入口、失败/取消与每请求Verify=1；独立审核后使用同冻结完整会话协议，不引用静态工作量作为速度数字。

CP35另预先安排完整一次性CLI对照：在其正确性通过后，使用未改的bench.py、同ee72 CLI、原五语料及原四请求权重、O0/build、3个ABBA/BAAB区块、units/jobs=1、两臂source/query/object缓存一致开启，LLVM/Clang/LLD文件身份冻结，40预检+240正式CLI及各输入native输出均保留。该窗口单独判断新进程到native产物的完整成本，不与持久会话混算，不改正式product-gates；它不取决于CP34是否有利，也不以新Rust比较或O2/runtime测量冒称。其它编译/测试全部停止后才正式计时。

CP34完整会话200序列已通过，根初算默认/8192分别0.965558/0.950374；CP35会话分别1.005305/1.015609，完整CLI仍在跑，均待独立复算。为了核实CP34的前端有利点估计能否转化成真实build收益，追加独立CP34完整CLI窗口，参数、五语料、三缓存、3区块、40预检/240正式与CP35完全相同，各自与ee72配对，不能把两窗的候选彼此直接相除。当前三窗全部停止前，只能静态准备CP34 CLI构建；不叠加CP35或CP33，不改变测量权重，不挑选有利语料。

## 最终收束：2026-10-31

CP29/30/32/33/34/35 六个精确候选均完成限定作者控制、非作者审核、完整200序列/1400请求会话窗口与独立复算；两容量分别计量，原权重保持。CP34/35各自完成40编译/128 native预检及240编译/768 native正式CLI，并首次独立复算通过。CP30最初114序列失败前缀仍为Incomplete_NoScore，未拼接；CP35首次作者测试预期失败和证据打包首次失败分别保留，不重命名为成功。

结果：CP29/30/32/34/35未建立整体默认编译净提速，本轮速度采用NoGo；CP33有公开编辑与完整七请求局部收益、默认采用Hold；CP35另有CLI冷请求与编译RSS局部收益。六候选源码分别保留研究分支，没有默认合入或叠加。根生产crates树保持CP28 `a7ae8da1862a69cf673b8db50431b4991cb0d1c5`，本轮未写main、未改正式产品门。精确数值、区间、构建/源码对应与限制以[COMPILE7-RESULT](COMPILE7-RESULT.md)和独立汇总为准。

后续仅交静态Typed指纹调用图与不可变raw owner可行性，不在本轮继续实施或以调用次数估算提速；细粒度语义查询与真实依赖采集仍未完成。父专项不标Done，完整语言/运行/维护/编译四目标及产品级净突破保持未完成。
