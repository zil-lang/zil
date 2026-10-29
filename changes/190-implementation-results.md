# 190 作者候选：真实循环 helper 与范围前置

状态：作者候选的限定语义、原生与故障验证已完成；正式独立审查、主线合流与全仓门由root负责。没有声明全部1.4语言或ZUI产品完成。

## 源码身份

- 原ForRange端点宽度修正冻结：`d92c8edfc7920dfdc0635c0d8485d1965429ed33`。
- Flow首个三阶段纵切：`8d4dc557`；最终私有生产为`edd858b6`（仅实际helper模块才分配并支付新增独立depth表）。
- 完整受测源/测试冻结：`7491c212d392b12bb90f735b90f535066fa00a47`，schema24。
- 依赖统一根候选`a00472da`已经包含188真实DAG调度/来源、189显式Copy地址和191显式Finite/Flow运输。最终主线193等后续变更不属于这些历史执行的源身份。
- 190生产作者`/root/resume190`；独立源面/四模块消费者测试作者`/root/resume192`。三阶段安全实现没有相互调用对方的求证算法。

## 实际行为

ForRange先从两端只读元数据取得真实整数上下文，再左→右各展开/求值一次。支持当前整数半开区间、实际参数/字段/数组投影/泛型identity端点，保持索引词法作用域以及外层非Copy/Store责任回边检查。空与逆向区间不跳过正文的符号检查，generic break/continue保持既有明确边界。

Types以有界block片段构造调用控制图；每个静态nested helper调用点有自己的子图副本和continuation。Analyze以独立来源标记和单模式精确计数行重建；Verify直接从实际指令续点做BFS，并用自己的显式DFS判环。仅真实组合图有环时导出Flow；无环保留原Finite排序去重轨迹。入口0、有序边BFS编号、全部节点可达，Return续接、Trap终止。FnOnce有环（含零调用）明确Unsupported，有限0/1次保持。

嵌套仍只接受本次真实环境来源；相同ClosureId的另一factory实例不能冒充该参数。188静态helper DAG的深度由Types既有计划、Analyze与Verify各自按原max_call_depth核验；runtime次数不是编译depth。图的节点、事件和边在复制前checked累计并按原额度计费，随后扫描/复制/重定位/队列/比对都支付实际工作。没有调大默认预算、修改ABI/后端或引入runtime堆盒/RC。

资源消费者对Flow的全部可达节点核是否存在调用，包含无正常返回的有限前缀。现有初始化、真正capture与本次显式argument权限、借出窗口、资源Live/Drop/失败保持义务仍独立核验；图或Fn标签都不授予重复消费权限。

## 实际验证与范围

| 限定执行 | 实际结果 | 源码/收据 |
|---|---|---|
| ForRange新10组 | 10/0；含O0/O2端点求值/作用域/宽度 | `evidence/impl/190/for-range` |
| ForRange邻接7套 | 55/0；Expand Clippy Pass | 同上 |
| 最终Flow源面与raw/Wire联合 | 31/0（源17、raw14），ignored0 | clean7491，`run-logs/190-author/flow-joint01` |
| 新源面实际机器 | 112次O0/O2执行；stdin决定0/1/2/7及7000轮控制 | `evidence/impl/190/source-and-consumer` |
| 四模块消费者 | 84次O0/O2执行，含62次真正allocator故障；精确输出、退出、alloc/frees/live0 | 实际clean864 CLI/输入，与7491生产和消费者文件逐字节相同 |
| 有限邻接 | 62个不同测试最终均通过；最初59/3后原三个测试所属3套21/0 | `raw-and-finite/REPORT.json`；不把重复运行当83项 |
| 最终Clippy | 三个语义crate及driver lib、新源/raw tests，`-D warnings` Pass | clean7491，`flow-clippy02` |

raw14覆盖实际CFG/指令与claim不符、事件删加改、空/孤立/非BFS/错索引/重复边/少回边/伪Return/Trap、Finite冒充cycle、FnOnce零事件cycle、真实不同factory来源、子图continuation篡改、协同伪claim、无返回前缀、Hold/Call/writes/Cleanup伪造、重复标签消费拥有capture、静态depth N/N-1、图复制上限与raw/Wire工作前沿、失败后fresh请求。

源面包括Fn/FnMut的while/for、拥有/借入环境、零调用、有限与Flow双向嵌套、两次Flow子调用点、正常/Copy错误返回、真实Drop序列、189显式&mut Copy、184环境借出和旧窗口负例。Trap只声明防御合同出口保留，未声称原生执行防御Trap。无返回的raw正例只做有界分析/核验，没有运行无限机器程序。消费者作者机器证据为Linux x64，没有冒称Win64。

## 保留的失败与精确修复

原ForRange9/1的真实i64泛型identity误拒及更早夹具问题保留。Flow有限邻接最初59/3的原126/188循环输入已按root ae3ac244批准，逐字迁为真实O0/O2 exit3正例；191原伪图仍被Analyze/raw/Wire精确SafeReject，未改其输入。

raw02和raw03各13/1来自新构造的无返回正例仍含孤立返回block的Load、随后scope markers；原初始化/cleanup门正确拒绝。仅修正raw测试构造，保留SSA定义并移除不可达块的旧scope操作，最终14/0；生产未因此放宽。源面测试首次遇到E0463编译缓存问题，没有执行语言程序，定向重建driver后17/0；原故障和恢复记录保留。

原源包与执行产物保持分离。证据目录只保留命令、输入、诊断、精选文本/Raw Wire和SHA清单，排除执行文件、target、缓存及重复巨大phase产物；完整历史失败仍在独立run-logs。独审/合流不得把作者限定Pass当成正式语言全门完成。
