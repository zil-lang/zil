# CP22 静态热路径复核与后继切片建议

## 范围与冻结身份

- 复核对象：`/workspace/scratch/4ab361722127/zil-cp22`，交付提交 `7ca57fb61aa6fe4f7cac963a531a6cf2851792e4`，源码提交 `f660aeb68e4cf86eab0bc7c9e219d5bcf8bdb5ea`，crates tree `a598b9f6b04feff21fc2eed24566726a1a79ef12`。
- 冻结二进制：`/workspace/scratch/4ab361722127/build-cp22/release/zil`；已交付 SHA256 `650e8cd97c0fae262ae8c2a28e3ea6b31532978b9e419c0d2854babfea69373a`，对应源码树的 `evidence/research/compile-performance/CP22/RELEASE-FINAL.json`。本复核不重新读取/构建/移动二进制。
- 本文仅静读调用链、成本和边界；没有启动 cargo、CLI、benchmark 或性能采样，没有改冻结代码。次数是满足下述条件时由控制流推出的静态次数，不能据此宣称任何性能收益或回退幅度。
- 主要入口是文件系统 `CompilationSession::compile_path`。直接 `compile_source` / 内存 `compile_project` 只有 Session 解析复用，不启用 CP22 磁盘适配器；`Stage::Parse` 也保留原单文件路径。

下文短路径均相对 `/workspace/scratch/4ab361722127/zil-cp22/`。`PC` 指 `crates/zil-driver/src/parse_cache.rs`，`P` 指 `crates/zil-driver/src/project.rs`，`S` 指 `crates/zil-driver/src/session.rs`，`QC` 指 `crates/zil-driver/src/query_cache.rs`，`C` 指 `crates/zil-syntax/src/cache.rs`，`OC` 指 `crates/zil-backend-llvm/src/object_cache.rs`。

## 一、主要发现

1. **CP22 的 producer 是每请求一次，不是每模块一次。** 但 CP12 在后面会独立再次计算，后端 object cache 还有既有 compiler/LLVM 身份计算。installed identity 通常读取 ELF build-id 和文件元数据；只有未找到 build-id 才全文件 SHA。不能把该项写成“每请求必然多次 hash 整个 exe”。依据：PC:121、QC:193、OC:317。
2. **成功请求的同一模块 neutral Parse identity 通常被计算三次。** discovery 的 Session 查询一次、CP22 磁盘 key 一次、project pipeline 发布/读取 Parse query 一次。每次都重新 SHA 当前源码；CP22 的 `keys` 只消除了同一次请求中磁盘 load/store 之间的第四次计算。依据：P:937、PC:141、P:259、S:647。
3. **磁盘命中仍然是完整 payload 读取、SHA、AST 分配与验证。** 它消除 Lex/Parse/属性验证，保留当前源码读取、模块发现、预算计费、导入解析与后续编译。因此小模块的文件 I/O、codec 和身份成本可能接近或超过原 parser 成本；这是待测假设，不是实测结论。依据：PC:153、C:45、P:880。
4. **冷写是逐模块同步写入。** 每个可编码、未从磁盘命中的模块都有完整 encode、payload SHA、临时文件创建、5 次 `write_all`、`sync_all`、rename。成功 rename 后临时文件清理器还会对旧临时路径发起一次必然找不到文件的 `remove_file`。依据：PC:249–305。
5. **同 Session 命中当前会重复落盘。** discovery 的内存命中分支不调用 `Prepared::load`，也不填充 `loaded`；请求成功后 store 把这些模块全部重新编码、hash、同步写入并覆盖。新进程磁盘命中则填充 `loaded` 并跳过写入。这是直接控制流事实，和性能幅度无关。依据：P:951、PC:207、PC:246。

## 二、一次请求的顺序与成本落点

`compile_project_path` 在开始计时后执行只读 `parse_cache_prepare`，然后 source discovery，再进入带 ArtifactPlan 的项目流水线，最后仅在所请求阶段完整成功后执行 `parse_cache_store`。返回的 `report.seconds` 包括 prepare、所有 load 和 store。store 位于请求完成之前，`sync_all` 的等待没有从端到端耗时中排除。依据：P:103–203。

| 步骤 | 实际行为 | 可复用后仍需的工作 |
|---|---|---|
| CP22 prepare | query-cache opt-in / host / interposition / cancellation 检查；root 与 parse 子目录各检查一次；取得 producer | 读取准备不 mkdir；每请求建立新的 key map 与 loaded set（PC:91–139） |
| Discovery 每模块 | 读取当前 SourceUnit；先查 Session，再尝试 CP22，再冷解析 | 当前字节计费、按次序扣 token/node、逐 import 重新查路径，含歧义和 symlink 边界（P:893–1045） |
| 项目入口 | validate_sources、ArtifactPlan、project fingerprint 与 Parse query | loader 提供的 Arc 仍通过当前 query 发布/读取；继续按原次序做项目配额检查（P:226–288） |
| CP12 lookup 前 | resolve_imports、`ct_selection::needed` 扫 token tape | CP22 命中不会跳过这一层；needed 在普通无 `#if` 源码上仍扫描 token（P:294–317；`ct_selection.rs`:95–110） |
| 后续阶段 | CP12 整包命中可走原 checkpoint 路径；其他情况走原 Expand/CT/Types/Analyze | CP22 本身没有类型级增量；完整 Build 仍调用独立 Verify（P:319–345；S:474–492） |
| CP22 store | 完整请求 Success 后，逐未标记 loaded 的模块编码并原子写 | 包括 Session 命中模块；失败请求不会在此发布语法文件（P:185–197；PC:231–300） |

一次请求可读取某些 CP22 项，随后因导入或预算失败；此时没有 CP22 store。既有 Session 可以保留更早成功阶段，CP12 的目录行为也不等于 CP22 的只读准备行为，不能将三者统称为“失败请求完全没有任何缓存状态变化”。

## 三、producer 调用次数

### 3.1 正常 eligible Build

假设 Linux 本机、无 LD_PRELOAD/LD_AUDIT、query-cache enabled、完整 filesystem Build、无 emit-dir、无 source selection，且请求成功到达相应阶段：

| 调用者 | compiler identity 次数 | 其他身份 | 是否 CP22 新增 |
|---|---:|---|---|
| CP22 `Prepared::new` | 1/request | 无 | 是（PC:121–123） |
| CP12 `Prepared::new` | 1/request | 无 | 否（QC:193–196） |
| object cache `PreparedCache::new`，若启用且实际到达 | 1/request | 另 1 次 LLVM library identity | 否（`build.rs`:269–281；OC:141–153） |

因此上述前两层合计是 2 次 compiler identity；若后端对象缓存也启用并执行 setup，则是 3 次 compiler + 1 次 LLVM library。该计数针对这几个已定位调用点，不把 LLVM 运行时配置或其他工具探测算成同一个函数调用。

Stage 不满足 CP12、使用 emit-dir、source selection 需要原流水线，或前面失败时，CP12 那次不会发生。CP22 新增调用在准备失败/早期 bypass 时也可能是 0。query-cache 关闭时 CP22 producer 与磁盘 key/load/store 都为 0。

### 3.2 身份算法的真实成本

`installed_identity` 打开文件、读取 metadata，hash 长度/mtime，以及 Unix dev/inode/ctime；尝试解析有界 ELF program headers 与 GNU build-id；找不到 build-id 才用 64KiB buffer 顺序 hash 整个文件；最后再读 metadata 检查长度和 mtime 未变。没有每模块重复 exe hash，也没有全局请求间 memo。依据：OC:314–404。

**后继方向：**在一个请求内部共享经过同样检查的 producer，减少 CP22 与 CP12 的重复 open/metadata/build-id 解析，符合现有“安装工具在运行期间不被修改”的前提。不能将此扩展成无期限跨请求常量，也不能仅绑定 Cargo 版本号。该项可能只节省小常数；无采样前不列为主要瓶颈。

## 四、Parse identity 与 source hash 次数

令 N 为最终成功发现的模块数，S 为 discovery Session 命中数，D 为 CP22 磁盘命中数，M=N−S−D 为冷解析数。以下假设配额充足、无取消、未触发 exact-budget fallback，CP22 准备成功，store 正常遍历所有模块：

| 调用位置 | neutral identity 次数 | 源码完整 SHA 次数 | 备注 |
|---|---:|---:|---|
| discovery `session.parse_key` | N | N | 每模块都会发生（P:937） |
| CP22 `Prepared::key` 首次计算 | N | N | N−S 次发生在 load；S 次发生在最终 store（PC:141–151） |
| project pipeline `self.parse_key` | N | N | 每模块重新构建 key（P:259） |
| 合计 Parse identity 层 | 3N | 3N | 与本次冷/磁盘热、S/D 的比例无关；只是第三类首次计算的位置不同 |

`Prepared::key` 的方法调用不等于实际计算：load 调用 N−S 次，store 再调用 N−D 次，共 2N−S−D 次；其中首次计算总共 N 次，其余是 BTreeMap 命中。`loaded` 查找 N 次、key map 查询至多约 2N 次，都是有序容器查询成本。依据：PC:141、160、245–249。

每次 `parse_identity` 包含源码 SHA、源码身份 buffer SHA、规则 buffer SHA、CP22 namespace 规则 SHA；磁盘 key 在此基础上再 hash QueryKey 编码与 producer envelope。它没有缓存已有的 `SourceUnit::fingerprint`。后者直接调用当前完整 `text.as_bytes()` 的 SHA。依据：S:647–701；`zil-base/src/lib.rs`:74–75、371–372；`zil-db/src/lib.rs`:71–79。

还有既有的 driver 直接 source fingerprint 调用，不应误归入 CP22 新增：

- `project_fingerprint` 为每模块再 hash 源码一次：N 次（P:787–807）。
- `project_key` 先调用 root `query_key` 再覆盖 input，额外 hash root 一次（P:776–785）。
- `CompileReport::new` 为 root 再 hash 一次，后面该 input fingerprint 会被 project fingerprint 覆盖（`driver/src/lib.rs`:177–195；P:88、229–230）。
- 若旧 source cache `options.cache_dir` 开启，逐模块构造其完整 query key，再额外 hash 源码 N 次（P:243–250）。

因此，限制在以上明确列出的 driver 调用点，普通成功文件系统请求是 **4N+2 次 SourceUnit 文本 SHA 调用**，若 source cache 开启则为 **5N+2 次**。这是调用次数；root 文本被额外扫描两次，所以总扫描字节量分别为 `4×所有模块源码字节+2×root字节` 和 `5×所有模块源码字节+2×root字节`。下游其他结构 fingerprint、payload SHA、query snapshot digest 不在这个计数中。`query_key` / namespace 中的小 buffer hash 也不等同于源码字节扫描。

query-cache 关闭时，没有 CP22 的 N 次磁盘 key source hash；neutral Session key 在 discovery/pipeline 的 2N 次仍发生。低预算时 `parse_key` 可能发现 neutral receipt 不足，并额外计算一次 exact-budget `query_key`；上述数字不适用于该 fallback，也不适用于中途失败、取消、Stage::Parse 或内存直入口。

**低风险后继方向：**以请求内不可变 SourceUnit 为边界保存 text fingerprint/neutral Parse key，供 discovery、磁盘 envelope、project fingerprint 和发布复用。至少先去掉 CP22 key 与 discovery 的重复 neutral identity 计算。传递的是源事实身份，当前额度不足时仍必须由 `parse_key` 选择原 exact-budget fallback，不能把一个成功键当作当前额度已经足够的证明。不要让中和 token/node 的 key 渗入 Types/CT 等完整配额身份。

## 五、磁盘命中成本与内存

每次 `load`：生成 key/path → symlink_metadata（文件种类、权限、长度）→ 按文件长度 reserve transport Vec → open/read bounded payload → 核对外层 magic/key/长度 → 完整 payload SHA → syntax decode。只有成功解码后才 `hits += 1`、插入 loaded。预算不够或损坏都回原 parser。依据：PC:153–229。

codec decode 包含：

- source bytes、payload 长度、magic、token/node receipt 的前置检查；额度不足在 AST 分配之前返回。不过此前文件读取和外层 payload SHA 已经完成（C:45–66）。
- 所有 Module/Item/Expr/Type/Token/属性元数据的重建，分配 Vec、String、Box；String 还验证 UTF-8 并复制；Vec 检查剩余字节和分配预算（C:67–74、216–280）。
- 每个 Span 的当前 FileId 绑定、源范围与 UTF-8 边界检查（C:283–304）。
- decode 完成后校验完整消费 payload、token 数量一致、ModuleId/root span，并再遍历完整 token tape 检查连续覆盖、无 Invalid/中途 EOF、最后唯一 EOF。Token Span 在此还有一次检查（C:75–107）。

令 P 为 payload 字节、A 为 AST/元数据结构量、T 为 token 数，则磁盘命中的主体为 `O(P+A+T)` 加当前源码身份扫描及文件系统操作，并非 `O(1)`。它不重新执行 parser/attribute validator，但还要做至少一个 payload hash pass、一次 decode、一次完整 token tape 验证；后续 source-selection needed 还会扫描 token。逐模块串行 load，没有并行预取。

单 payload 上限 32MiB、显式 decode 分配预算 64MiB、结构嵌套 128 层；transport Vec 在 decode 期间仍存在，且所有成功模块的 Arc<ParseResult> 会保留至整个请求结束/Session 持有。上述预算不能解释成整个请求 RSS 上限，也未包含 allocator 管理开销。依据：C:13–15、52–57、154–168；P:1043。

新进程 no-op 即便最终命中 CP12，仍然需要先完成上述 discovery/Parse query/导入解析，因此 CP22 的命中成本确实落在 no-op 端到端路径上。若冷热结果显示回退，应优先拆清“读取+解码+身份”与“冷写”两类，而不能根据 `parsed=0` 直接认定更快。

## 六、冷写、少量修改与同 Session 行为

假设所有 AST 均可编码、磁盘可写、请求成功且没有后发取消：

| 请求形态 | load 尝试 | parser | encode/write/sync_all | 预期诊断含义 |
|---|---:|---:|---:|---|
| 新进程全冷 | N | N | N | misses=N，parsed=N，stored=N |
| 新进程全磁盘热 | N | 0 | 0 | hits=N；store 仍建立 protected set、循环并跳过 N 模块 |
| 新进程只有 K 个必要身份改变 | N | K | K | 未改模块可命中 N−K；身份重排会扩大 K |
| 新进程当前项全部损坏 | N | N | N | corrupt=N；成功后修复 |
| 同 Session 全内存命中 | 0 | 0 | N | session_hits=N，hits=0，stored=N；已有磁盘项仍被覆盖 |
| 混合 S/D/M | N−S | M | N−D=S+M | 内存命中和本次冷解析都进入 store |

唯一被 store 跳过的成功模块来源标志是本次 `Prepared::load` 填入的 `loaded`。Session 命中没有该标志，且 Prepared 在每次请求新建；它不记得上次请求刚成功写入同一个文件。正常同 Session 也可能被 capacity guard 清空查询所有权，但 discovery 先取得的 Arc 仍可借给本次流水线；这不改变上述“内存命中还会落盘”的判定。依据：PC:124–130、207–208、246；P:117–120、951–953、78–79。

写入内部的固定成本：

1. 所有项目源路径和 artifacts 一次性构建 protected BTreeSet，约 `O((N+输出数) log(N+输出数))`，无每模块全表扫描（PC:239–244）。
2. 每个待存模块先 `valid_root` 遍历 token，再完整 encode；Writer 每次 put 都检查尺寸并 try_reserve，内存上限在扩容前检查（C:26–42、81–123）。
3. 首次成功的写准备 mkdir/check root、mkdir/check parse 子目录、canonicalize 子目录，随后 write_ready 复用。若准备每次都失败，write_ready 仍为 false，之后模块会重复尝试；“一次/request”仅指首次准备成功的正常路径（PC:253–258）。
4. 每模块 create_new(mode 0600)，4 次 header `write_all` + 1 次 payload `write_all`，其中有一次完整 payload SHA，随后 `sync_all`、close、取消检查、rename（PC:268–292）。这些是 Rust 方法调用次数，短写/中断可使底层 syscall 数更大。
5. Temporary Drop 始终 remove_file 临时路径；成功 rename 后这次删除只得到 NotFound（PC:302–305）。

CP22 当前 `sync_all` 比既有 CP12 和 object cache 的发布强：后二者是 `flush`/close/原子 rename，没有这里的逐模块 sync_all。三者均没有在这些发布函数中 sync 目录；因此不要把 CP22 描述成已经提供完整掉电后目录项持久性保证。依据：QC:389–415；OC:280–311；PC:281–292。

## 七、后继切片的优先级与边界

这些建议不改变已冻结 CP22 结果。任何实施都应独立提交、独立候选身份、沿原五案例/四请求权重对照，不覆盖本轮回退证据。

| 优先级 | 可独立切片 | 可安全保持的契约 | 风险/需要证明的部分 |
|---|---|---|---|
| A | 合并 80 字节外层 header，减少 4 次小 write 为一次；成功 rename 后 disarm Temporary | 同协议字节、同 create_new/权限/取消/rename/错误回退 | 边界失败仍清理临时文件；不把写完成前当成 stored |
| A | 请求内复用 source fingerprint/neutral key | 当前 bytes/路径/FileId/ModuleId/规则/非中和限制都绑定；预算不足仍冷回退 | 防止把 neutral key 当成足额结果、避免 mutable SourceUnit 后复用旧指纹 |
| A/B | 省去同 Session 重复持久化 | 仅改变可丢弃缓存的写策略，不影响本次 AST 使用或 Verify | 需定义 cache 首次启用、目录删除、外部损坏、producer/目录切换的补写/修复策略；不能将未检查的磁盘项报告为 hit |
| B | 每请求共享 producer 给 CP22/CP12 | 同一 installed-tool identity 协议与信任前提 | 不得跨请求无限 memo，不能删 interposition/private directory 检查；收益很可能小于逐模块成本，但须测 |
| B | 去掉部分重复 token span 检查或融合 token 验证 | 保持范围/UTF-8/连续覆盖/EOF/数量完整性 | 只有编译器内部已知 `Span::decode` 成功的路径能消除重复；encode/公共 decode 入口须仍安全 |
| B/C | 减少 CP22 的逐模块 `sync_all`，与既有 cache 的“可丢失缓存”契约对齐 | 完整临时文件、hash/key/length 校验、原子 rename、损坏 miss、Success/ArtifactPlan 后写不变时，丢缓存不应改变编译结果 | 这是 crash durability 策略变化，应明确文档和失败控制，不能作为无说明的小清理；不能通过直接截断正式路径省 I/O |
| C | 多模块打包、异步/批量持久化、codec 紧凑布局或借用源码 | 有机会减少 syscall/payload/allocation | 改动面大；必须重新证明 bounded read/alloc/depth、单项损坏隔离、取消/退出、key 身份和只写成功结果，不宜与低风险去重混成一个切片 |

对同 Session 写策略，一个保守选项是携带“已知本次只从 Session 取得”的 provenance 并允许跳过可选落盘。它不授予任何磁盘内容使用权，编译语义安全，但新启用磁盘缓存的长驻 Session 可能一直没有暖盘。较完整方案是按 producer + 目标缓存目录 + 完整持久 key 记录已成功发布的条目，并在明确需要修复时补写；额外状态需有容量边界。若要保证外部删/损坏后立即修复，必须付出当前路径检查或校验成本，而不能宣称“零检查仍保证盘上存在有效项”。

对于减少 sync，正确性基础是缓存可随时 miss 并回原路径，且读取方拒绝半写/损坏项；这与对最终可执行产物的持久性要求不同。应单独核对项目对私有缓存的 durability 约定，禁止为了速度移除读取完整性检查或 Success 门槛。

## 八、不可用作优化捷径的改法

- 不再读取当前源码、仅凭 mtime/size 判定旧 AST 可用：削弱 source identity。
- 不再逐 import 做当前文件发现：会漏掉新歧义、缺失或 symlink 越界。
- 去掉 FileId/ModuleId/路径身份而不显式重绑定整个 AST/所有 span：导入顺序变化会使用旧身份。
- 仅看 tokens.len()，丢 parser.nodes / attribute.work receipt：会放宽原 max_nodes 检查。
- neutral Parse key 不做 current limits 足额检查，或把相同中和延伸到下游 CT/Types：会改变限额和首错行为。
- 用持久缓存中的 checksum 代替可信 producer、私有目录、结构/边界校验，或用它导入 Verified：checksum 不是权限，独立 Verify 必须保留。
- 将写入提到 ArtifactPlan/当前请求成功之前，或把失败半结果放入成功缓存：改变当前隔离与发布边界。
- 根据 Parse hit 数宣称语义增量或类型阶段跳过：CP22 没有这种依赖分割；最终 Types 是否复用仍由既有整体键/CP12 决定。

本复核的直接落点是：先由根的冻结窗口给出端到端加权结果；如果存在回退，分别检查新进程热态的 decode/hash/I/O 与 cold 的逐模块 sync，不把同 Session 已证实的重复写误当作本轮新进程 CLI 回退的充分解释。它是另一条独立且清晰的后继优化机会。
