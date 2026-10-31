# CP25 可行性审计：稳定查询身份与事务采集依赖

审计者：`/root/toolchain_recovery`。范围：只读 `zit.ts` 与当前 ZIL 源码；未编译、未运行测试或 benchmark、未修改生产文件。这里的“可行”是源码层面的实现判断，不是通过性能门或正确性验收的结论。

## 1. 结论与可实施边界

**zit 最值得借鉴的是稳定的计算 owner、计算中记录实际读取、完成后更新依赖集合、结果改变才通知直接消费者。单独搬入反向链或 height 队列，不能把当前 ZIL 的项目级查询变成函数级增量编译。**

可以做一个保持现有行为的“小切口”：在 `CompilationSession::query` 引入有作用域的依赖收集事务，保留完整 `QueryKey` 作为缓存/调用身份，把现有 Parse → Expand → CT → Types → Analyze 的显式输入通过事务读取；稳定 owner 仅表示同一个逻辑计算槽位，不参与判断结果是否新鲜。失败和取消丢弃待提交边，成功发布之后才替换 owner 的当前依赖集合。它能把依赖遗漏和事务提交约束收拢到一个接口，为后续细分查询准备基础。

**不建议将这个基础设施小切口直接认作新的生产性能优化。** 当前 driver 的边本来就非常粗，绝大多数阶段只有一个输入，新增 owner 表和第二份边状态可能只有维护成本。若本轮要求的是立刻减少私有函数体编辑后的重算，这个小切口还不够；需要另立接口、函数体、效果等实际查询边界，并解决当前的编号、预算和负查找问题。可将 CP25 判为“设计可做；生产采用 Hold；未经性能计量”。

| 目标 | 本次判断 | 原因 |
|---|---|---|
| 事务化既有依赖提交 | 可做，范围可控 | 现有 `query` 是统一入口，`Database::publish` 已有提交时校验 |
| owner 跨源码版本稳定，同时保留现有完整缓存键 | 可做，但需明确入口命名空间 | owner 与 artifact key 分开，不靠 owner 单独命中旧值 |
| 直接让 private-body 编辑停止传播到项目 Types | 目前不构成小改动 | Types 是整个项目查询；现有摘要和 IR 编号还不足以重用单个消费者 |
| 按 height 并行执行所有编译查询 | 当前不宜 | 共享预算、诊断顺序和 ID 分配可观察；递归效果图还需要固定点 |
| 靠进程内 owner 直接改善新 CLI 进程的编辑构建 | 不成立 | 普通 CLI 每次创建新的 `CompilationSession` |

## 2. 本次读取的身份

项目根：`/workspace/scratch/4ab361722127/zil`。下面的仓库路径均相对此根。读取结束时 HEAD 为 `474e0d84a0e2a941e318533f8558441731d738fe`。主树的 db 在本次读取中仍是原完整闭包读取与扫描失效实现；CP21、CP23 的独立候选不被冒充为已经合入此树。CP22 也不在本审计中当作已合入前提。

上传文件：`/workspace/scratch/4ab361722127/upload/zit.ts`，SHA256 `719a914356a24ac1edbeb1ffe6637d22311c9299e6be9ddbcdf3f7fdddf1474f`。

| 源文件 | SHA256 |
|---|---|
| `crates/zil-db/src/lib.rs` | `2a07435a5d585581219bea2cc8aa70d9dbd62fe9cb3e0555f2a2913563e768e4` |
| `crates/zil-driver/src/session.rs` | `73697422cb96bb2bab68320c7e362b683742253785f97f144c8c2c28afe48a71` |
| `crates/zil-driver/src/project.rs` | `7e9650a7ad0c768419c567bc5c9d7b20deaf68d23934f2df8bee0b76981c7bd5` |
| `crates/zil-types/src/program.rs` | `28cb6ada4f6d3625b1cb9c2785afa8c342a971183ff84357fd07eaf394a76e0f` |
| `crates/zil-types/src/resource_effects.rs` | `5e9ea40bca14d9edff0b10dc4e0684c7da919185a775f77d88d38e17231b2e23` |
| `crates/zil-driver/src/ct_selection/context.rs` | `b5de98949cea0a9faca85170d7813118bbd8c7314c12c9903eb52115d922244a` |
| `crates/zil-driver/tests/projects.rs` | `74ed4c31d809d742cf5cfeba0d69dc96670e7fbd83605177f1c5cc09feaa86e8` |
| `docs/research/compile-performance/CP01-RESULT.md` | `c86cb9903463ea70213cf1e06da606fb874d0f2b218d7b3e7bcd15552ca6a46b` |

## 3. zit 的具体机制，以及它要求什么前提

| zit 源码 | 实际机制 | 对 ZIL 的对应要求 |
|---|---|---|
| `read` 第 52 行 | 先拉取必要的低高度工作，再重算脏值，读取时 `bond` | 读入口必须知道当前 consumer，且读到的值确为当前计算使用的值 |
| `write` 第 55–65 行、`trig` 第 94 行 | 等值不推送；变化只访问 producer 的直接订阅链，脏标记和消费钟去重 | 有可相信的结果等值边界、反向边和一致的 dirty 状态；不能只把源码 hash 叫成语义 hash |
| `bond` 第 101–107 行 | consumer 对象跨重算存活；用读游标匹配旧边，匹配成功就复用；边同时进入 producer/consumer 两侧链 | 逻辑计算 owner 不随输入内容变化；边的维护要与成功结果一起提交 |
| `recalc` 第 74–88 行、`bdTrim` 第 113 行 | 每次重算重置游标，finally 剪掉没有再次消费的旧尾边 | 编译器需采用失败时回滚的事务版本；不能照抄异常后也直接剪旧已发布边的行为 |
| `aCalc` 第 122–138 行 | `vn` 与事务中止状态过滤过期异步完成 | 可借作执行尝试代次；不是语义结果版本，也不替代完整输入校验 |
| `hq/flush` 第 277–301 行 | 入队去重；按高度取出，发现高度已变则重排；read 可截断 flush | 只适用于已证明的 DAG 部分；队列层次不能代替循环/SCC 处理 |

`hUp`（第 110 行）只升不降，并在高度超过 1000 时报警。这个阈值不是编译图的循环证明。`$w/$b` 是同步全局上下文，不能原样用于并行编译；ZIL 应使用显式作用域/事务参数。zit 同高度无依赖边的性质，也不证明同高度编译工作没有共享预算、诊断顺序或编号分配方面的冲突。

## 4. 当前真正的编译调用链

### 4.1 普通 CLI 与持久 Session 的区别

`crates/zil-cli/src/main.rs` 的编译入口调用 driver 的 `compile_path`；`crates/zil-driver/src/lib.rs:267–273` 明确每次创建 `CompilationSession::default()`。因此普通 CLI 新进程里没有上次编译留下的订阅 owner。

`crates/zil-ide/src/lib.rs:9–43` 的 `EditorSession` 才持有可连续复用的 `CompilationSession`。将稳定 owner 引入 Session，最先作用于这个场景。跨进程收益必须通过已声明的持久缓存/守护进程协议实现，不能用 IDE 微测替代完整 CLI 编辑序列。

### 4.2 项目图比函数调用图粗得多

`project.rs:213–258` 按模块循环发布独立 Parse 叶；所有 Parse snapshot 汇入一个依赖数组。`project.rs:320` 的 Expand 查询依赖这个完整数组，执行 `expand_program_with_limits` 处理整个项目；`project.rs:389` 的 CT 查询依赖 Expand，包含整个项目闭包推断和各模块常量求值；`project.rs:474` 的 Types 查询依赖 CT，并调用 `zil_types::check_program`。`session.rs:403–437` 再发布整个 typed module 的 Analyze。

所以当前主要图是“多个 Parse → 一个项目 Expand → 一个项目 CT → 一个项目 Types → 一个 Analyze”。函数之间即使是多对多关系，也没有因此成为 `zil-db` 中的多个节点。把数据库扫描换成邻接表，只能优化这些阶段之间的维护，不能自动让一个函数体编辑少检查其他函数。

### 4.3 输入内容目前就是节点身份的一部分

`zil-db/src/lib.rs:63–80` 的 `QueryKey` 含 `module/phase/input/rules/target`。`session.rs:622–666` 的输入指纹绑定 FileId、ModuleId、路径和源字节；rules 绑定规则/schema、版本、全部限制和 build inputs。`project.rs:758–778` 的项目指纹绑定完整有序模块表和逻辑路径映射。

一处编辑会给对应查询一个新 key，旧 key 对应的 snapshot 仍可保留。这是内容版本缓存，没有 zit 那种“同一个计算对象接收新输入”的 stable owner。**不能直接从 QueryKey 删除 input：它目前也承担确保缓存新鲜的职责。**

### 4.4 现有 query 没有自动采边的执行上下文

`session.rs:152–225` 的 `query` 接收手工 `dependencies` 数组，`execute` 是 `FnOnce() -> CompileResult<RawSnapshot>`。AST、Core、imports、project 等已经在闭包外取得，再被闭包捕获；执行时并不通过 `Database::read` 获取它们。只在 `Database::read` 内增加隐式采集，将漏掉这些实际输入。

发布时所有边统一为 `PrivateImplementation`，不是按签名、ABI 或效果切分。`RawSnapshot::digest`（`session.rs:34–63`）除单模块 `Typed` 特例外，用完整 key 和有序依赖身份构造 private construction identity；`ProjectTyped` 也走这个分支。它有意不是输出内容校验和，源码变了但语义输出等值时也会变。因此直接搬 `write(old == new)` 无法实现真正的语义截止。

## 5. 已有的 point-to-point 实现，可作为本地参照

`crates/zil-types/src/resource_effects.rs:89–227` 已经有贴近需求的机制：资源参数函数不少于 16 时进入 `solve_worklist`。首次按声明顺序 `scan_once::<Tracked>`，在计算效果的同时取得 callee 集合；建立 `callers[callee]`，用 `queued` 去重；只有效果集合增长才把直接 caller 入队。函数体没有为抽取依赖再做一次单独的语法遍历。小输入保留 sweep 路径。

这是真实的“计算中收依赖＋结果变化点对点触发”，而且允许递归，用单调集合固定点收敛。它表明这类思想对编译子问题有价值，但不能被严格高度调度直接替换。

这里的 `FunctionId`/template identity 只在一次完整输入中稳定，图和摘要每次重新构造。若跨编辑保留旧摘要，删除一个写操作后不能继续只做旧集合与新集合的并集：旧写效果会永久留存，可能造成新的错误拒绝。跨编辑需要受影响部分重置、SCC 重算或可靠的事实来源追踪；这已经超出单独复用 worklist 容器。

## 6. 最小实现应如何拆，哪些不能悄悄改变

### 6.1 两种身份必须分开

建议内部明确两个概念：

- **逻辑 owner/slot**：同一个 session 中同一个逻辑输入上的同一种计算。文件项目可由规范化项目根入口、规范化源路径/项目槽位及 phase/query kind 定位；它不包含源字节 hash。
- **调用/产物身份**：沿用完整 `QueryKey`，保留 FileId、ModuleId、完整输入、rules、target 等全部 freshness 轴。历史 `raw`/snapshot 仍按它保存。

当前内存 `SourceUnit` 和 `ProjectSources` API 没有显式持久 project token；路径可以只是调用者给出的逻辑字符串，不能强行 canonicalize 或偷偷读磁盘。最小版本宜只为已知文件项目分配跨请求 owner，内存入口保持现有完整 key 路径；或者先增加明确的内部 namespace 契约，并用相同路径的不同内存项目作冲突对照。不能把 `(ModuleId, Phase)` 当稳定全局 owner。

owner 只维护当前成功 invocation 的关联。不能将原多版本缓存压成“每 owner 一个值”：A → B → A 在容量允许时原实现能复用 A；单值替换会丢掉这条命中。`CompilationSession` 当前默认 capacity 32、最小 8，请求起始超限全清；新元数据的保留上限和清理也要明确，不能让清空 raw 后还有无限增长的 owner/边表。参见 `session.rs:72–95,134–135`、`project.rs:78–80`。

### 6.2 事务采边先保守迁移现有阶段

可以把统一 query 入口改为接收有作用域的 transaction/context，并把输入包装为“raw 值＋对应 QuerySnapshot”的内部句柄。执行闭包通过事务访问输入，事务记录确切 key、kind、observed 值；完成时交给现有 `Database::publish` 校验并发布。最先改 Project Expand 的 Parse 输入数组，再按同一种接口覆盖 CT、Types、Analyze 和单模块入口，不触碰各编译阶段内部算法。

第一版继续观察 `PrivateImplementation`，继续执行当前完整 project 计算。源字节、options、imports/paths 等不是 query snapshot 的输入，仍由完整 QueryKey 绑定；必须明确这是“显式查询输入收集”，还不是任意外部状态自动依赖跟踪。将来只有这些输入也变成可观察 query，才能进一步缩窄 key。

顺序约束不能省略：`RawSnapshot::digest` 会编码有序依赖身份，数据库闭包遍历也按 dependencies 的顺序决定先遇到什么。迁移时保持当前输入顺序、重复边与 fingerprint kind；不要顺手按集合排序/去重，再宣称完全等价。

### 6.3 bond 的可复用部分在成功提交时生效

每个执行尝试有自己的待提交读集合。运行途中可匹配旧 owner 的依赖槽以准备复用，但不得修改仍被旧成功结果持有的边。先完成原有错误/取消检查和 `Database::publish` 输入校验，再提交 owner 当前 key 与双向边变更；失败只丢弃 pending 集合。旧 `Arc<QuerySnapshot>` 的 immutable dependencies 不能原地改，否则旧版本缓存也被改写。

`Database::publish` 已在 `zil-db/src/lib.rs:202–231` 对输入做 `read` 和 observed fingerprint 检查，失败时移除 active 记录。新 transaction 应复用这条提交规则。执行代次只用于拒绝过期尝试，不得把“revision 变了”直接当成依赖改变；原来一个被观察 fingerprint kind 等值的替换可以继续有效。active/cycle 检查仍基于现有 QueryKey 语义，不能仅因两个不同输入归到同一 owner 就额外报循环。

这也解释了第一版的成本风险：历史 snapshot 不可变，如果另外保留可变 owner 边图，就会出现额外表和元数据。若此时还不改变实际查询粒度，它未必节约任何编译重算。必须隔离测量其完整请求成本，再决定是否保留。

### 6.4 明确不纳入这个小切口的内容

不增加 height 调度，不改变 phase/observer 顺序，不改预算，不把 private construction digest 换成接口 hash，不缓存 Verified 权限，不改变磁盘格式和默认缓存行为。`session.rs:441–472` 的每请求独立 Verify 保持；RawSnapshot 没有 Verified variant。

最小可能写集是 `zil-db` 的事务接口及行为测试、driver 的 `session.rs/project.rs` 及对应 session/project 测试。只有准备真正细分函数查询时才需要触及 `zil-types`/IR；不应把它们混成同一个可归因的小补丁。

## 7. 必须面对的可验证反例

以下是源码推出的行为要求及已有测试入口；本次没有重新执行它们。

| 反例/编辑序列 | 朴素移植会错在哪里 | 当前证据或未来验收断言 |
|---|---|---|
| 同一个 `main.zil` 从 `return 0` 改成 `return 1` | owner 不变便直接回旧 raw | `driver/src/tests.rs:103` 的 `cache_changed_source_limits_build_inputs_and_file_identity_cannot_reuse_stale_snapshots` 已要求重新执行 |
| 依赖库私有 `hidden()` 从 41 改成 42，公开签名不变 | 整个项目 Types 只观察签名就漏掉需要的新实现 | `driver/tests/projects.rs:151` 的 `m003_dependency_edit_changes_runtime_without_changing_public_signature` 要求运行结果变 42、签名不变、实现摘要变、Types Executed |
| 模块数组反序，公开导出类型不变 | 签名 hash 相等，但旧 Typed IR 中的具体 FunctionId 不能直接沿用 | `projects.rs:216` 的 `m004_signature_and_implementation_hashes_track_different_inputs` 已断言签名 hash 相等但 export.function 不同；`types/src/lib.rs:148` 用 `offset + i` 分配 |
| 文件发现时插入更早的 import | 只按路径复用带旧 ModuleId/FileId 的 AST/IR，出现错指或错位置 | `project.rs:964–990` 按发现次序给模块和文件编号；完整输入键目前防止了这类复用 |
| 初次仅 `foo.zil` 存在，第二次新增 `foo/mod.zil` | 只订阅成功读过的 `foo.zil`，漏掉新歧义 | `project.rs:781–848` 同时检查两个候选；`project.rs:955` 特意重复解析已注册路径；`cached_syntax_cannot_hide_lower_quotas_syntax_errors_or_ambiguous_entries` 已覆盖缓存隐藏歧义的风险 |
| `#if(config.enabled())` 中 enabled 的签名不变、函数体 true 改 false | 签名边无法代表 CT 使用的实现 | `ct_selection/context.rs:138–197` 已递归记录被引用函数，但 source 仍为模块指纹；`driver/tests/impl219_selection.rs` 有跨模块条件与函数体规则控制 |
| 动态条件从依赖 A 切到依赖 B，重算中途失败/取消 | 运行中直接 bdTrim 旧 A 边，旧结果却还留着 | 应断言失败不提交任何新 owner/边，恢复后读图与最后成功结果一致；不得把 stale 旧结果暴露给新 invocation |
| 资源函数从会写改成只读，公开签名相同 | 跨编辑继续旧单调并集，无法删除旧效果 | `resource_effects.rs:211–225` 的并集只适用于本轮由空状态开始的固定点；跨轮要另有失效/重置边界 |
| 同 height 两个模块都接近 token/CT 预算上限 | 改执行次序可能改变第一个 ResourceLimit 和 span | `project.rs:213–256,417–431,625–637` 按当前顺序逐项收费；`types/src/program.rs:132–135` 明确维持全图词法诊断优先级 |
| observer 在 Types Executed 时取消，或 Parse Reused 后取消 | 过早发布边/结果或合并事件会改变取消结果 | `driver/src/tests.rs:266–317` 已要求 active 归零、没有发布未完成 Types、下次正确恢复；Verify 次数也有明确断言 |
| A → B → A，随后清缓存，或执行中祖先 active | 单 owner 单值、悬挂反向边、提前 fast return 可能改变命中或循环错误 | 保留多版本与 clear 语义；db 现有 cycle/active、stale-publication 和 9,600 DAG oracle 必须继续适用 |

还有一个精确的安全边界：`ScalarInterface` 已有 `signature_fingerprint`，算法在 `types/src/program.rs:409–446` 绑定 target、导出名、参数/返回 mode 和 type identity，故不能说当前完全缺少接口摘要。但 `ir/src/program.rs:35–46` 的 callable returns、result origins、contract fingerprint 等在独立 Verify 前仍不可用；签名摘要不等于所有函数体、效果、布局、实例化和返回契约依赖都已齐备。

## 8. height 的最小适用范围

现在的阶段链本来已经有顺序，添加高度不会减少一个全项目 Types 的工作。导入图有现成的 Kahn 检查（`project.rs:695–745`），并明确拒绝当前未实现的模块导入环；这里可以记录/验证拓扑层次，但不应因此改变其他查询的循环语义。

函数调用和资源效果图有递归，现有 worklist 使用固定点。未来可先把 SCC 压缩成 DAG 再研究高度；这需要独立算法、费用和错误顺序审计，不能用 zit 的高度超过 1000 报警来替代。

因此 CP25 小切口不包含调度改变。若以后做调度实验，先在既定 query 图上证明结果、首错、费用、取消观察和 deterministic ID 不受顺序影响，再比较队列维护收益。

## 9. 建议的后续验证，不在本次执行

先做一个“事务化与旧接口完全同结果”的独立提交：对同一完整输入核对 query event 序列、snapshot dependency 向量、raw 命中、diagnostic/span、active 清理、Verify 次数及输出。复用已有 db DAG oracle 和 driver cache/取消/project 测试，补缺失的 owner 命名空间冲突、A → B → A、多版本 Arc 不被改写、成功换边与失败回滚行为断言。

若想证明 bond 带来收益，应单独对“依赖稳定”“依赖次序改变”“分支换边”“失败回滚”“高 fanout”计维护开销，并把图创建、提交、清理与内存全部计入；不能仅计触发循环。正式验收仍是完整请求。跨进程 CLI 与持久 Session 两种场景要分开，不相互代替。

历史 CP01 已在完整 CLI 窗口测过单独反向索引：check 加权 wall B/A `1.005874818`、build `1.022059634`，区间都跨 1，结论 NoGo，而非确定退化。见 `docs/research/compile-performance/CP01-RESULT.md`。这个历史结果不否定 point-to-point，但说明新候选必须明确减少了哪一段实际工作，不能只凭图操作复杂度就预报整体收益。

本次建议的优先级是：先保留当前正在计量的小候选和真正的 parse 复用证据；CP25 作为后续独立的事务接口/稳定身份研究。只有当细化查询的消费者确实存在、输入面可完整观察、等值摘要充分、费用语义可保持时，再让稳定 owner 的点对点通知成为生产增量路径。
