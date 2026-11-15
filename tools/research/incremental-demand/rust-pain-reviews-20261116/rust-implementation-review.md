# Rust 实现对照：查询依赖→CGU→跨进程缓存

2026-11-16（Asia/Shanghai），Astra只读复核。实际读取官方 `rust-lang/rust` 固定提交 **58cf8f9c35b355199727627d07ae34a4e2846d67**，本地 `/workspace/zil-recovery/rust-pain-rust-source-20261116`，`git rev-parse HEAD`核对一致。无编译、native执行、Git写入或ZIL修改；仅写此报告。ZIL成本仍绑定0276953/window05，未进行新性能测量。下列Rust源码行号全部属于该固定提交，不能据静态代码直接分配本窗Rust耗时。

## 结论及对上一份建议的修正

Rust已经实现了当前ZIL最需要的顺序：**跟踪真实查询依赖；重算但结果未变时停止失效传播；在构造LLVM IR之前判断CGU工作产物能否复用；仅按需读取查询值，并把可继续复用的结果带到下一session。** ZIL应借鉴这一条完整顺序，不先把庞大MachineOutput/整个BackendSession序列化，再希望磁盘恢复解决成本。

root已核对本窗全部6次Rust warm-restart都是Cargo Fresh、没有rustc执行。故2.232s/.0217s是端到端产品差距，**不能用rustc red-green算法解释这21ms**。本报告中的rustc路径对应Cargo判定需要重新调用编译器后的增量构建；它适用于ZIL私改/接口变化与实际编译器重启恢复路径。

## 1. 失效传播：Rust不是“任一输入变了，清空整个环境”

- [`compiler/rustc_middle/src/dep_graph/graph.rs:368`](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/dep_graph/graph.rs#L368)：`with_task`执行provider期间用TaskDeps记录实际读取的节点；397–399以读取边和当前结果分配节点。`edges.rs:11–60`保存去重后的首次读取顺序。
- `graph.rs:909–936`：`try_mark_green`先找上次相同DepNode；已有green直接返回、red拒绝、unknown递归检查旧依赖。`943–1034`按旧边顺序处理：遇red停止；unknown先递归，不成则force那个查询；force后可能green，也可能red。仅全部依赖green才迁入当前图。错误导致未知状态时不把错误session持久化。
- **关键结果截断**：`graph.rs:791–825`中重算结果的稳定指纹与旧结果相同就标green；不同才red，`no_hash`保守red。由此某body变化而某个type/signature查询结果相同，可以不继续使仅读取该结果的消费者失效。green不等于“从未执行”。
- [`rustc_query_impl/src/execution.rs:418`](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_query_impl/src/execution.rs#L418)跳过`eval_always`的绿色尝试；其余先try_mark_green，失败才在with_task内重新provider。`533–562`的ensure在green时可不取结果，仍记录当前读取边。

**正确性边界**：这是依赖完整性、稳定key/结果hash和无遗漏隐式读取共同成立下的复用，不是文件mtime或上次“检查过”凭据。动态控制依赖先变红时，停止沿旧路径强制后来可能已不再有效的查询；重新执行本查询建立当前读取集合。不能把旧依赖无序并发跑完，也不能只记录正向callee而忽略名称查找失败/遮蔽/全局选项。Rust实际使用稳定指纹；ZIL既有“hash仅选择、完整等价/当前输入授权”合同更强，借鉴传播方式即可，不因此弱化ZIL合同。

**直接对应ZIL**：接口修改只改变2语义结果、初始化查询执行6，却Expand/CT/backend全部5105重做。清空点是`zil-expand/src/incremental.rs:930–949`、`zil-ct/src/plain_session.rs:173–176,312–369`、`zil-backend-llvm/src/emit/incremental.rs:301–367`。应该把消费者实际读到的header/ABI/绑定投影作为依赖，并在结果相等时截断传播，而不是套一个全项目Context比较。沿用现有函数产品和query结果结构，不引入另一套通用查询框架。Analyze与独立Verify各自保留读取/证明边界；Analyze绿色不能替代Verify。

## 2. CGU：Rust先判断复用，再构造LLVM IR

源码链：
1. [`rustc_codegen_ssa/src/base.rs:735–747`](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_ssa/src/base.rs#L735)仍收集/分区MonoItem，并先ensure所有当前`codegen_unit`查询。Rust**并非跳过所有分区工作**。这里显式ensure，避免无法从opaque DepNode重新执行codegen_unit导致不必要重编。
2. `base.rs:785–787,1238–1279`的`determine_cgu_reuse`先要求上次work product存在，再对`CompileCodegenUnit` dep-node `try_mark_green`。无LTO可PostLto复用；有LTO先PreLto，不凭CGU名字就复用最终对象。
3. `base.rs:842–877`：只有`CguReuse::No`调用`backend.compile_codegen_unit`并提交LLVM；green分支直接提交CachedModuleCodegen及previous_work_product。
4. [`rustc_codegen_llvm/src/base.rs:64–91`](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_llvm/src/base.rs#L64)：实际module_codegen被with_task包住；**ModuleLlvm::new在91行，位于以上reuse判定之后**。112–124才按CGU内实际items创建声明/定义。这把CGU所消费查询的依赖与最终工作产物连接起来。
5. `rustc_middle/src/mono.rs:513–520,595–606`：work product按稳定CGU名字定位，CompileCodegenUnit是对应dep-node。名字只是索引；“产物存在+当前依赖green”才是复用判定。

**ZIL当前反序与代价**：window05重启语义disk hit、16/16object hit，但先lower5105、整图变换、分区/内部化/裁剪，再计算对象键。`build.rs:379–487,515–752`在object路径之前，BackendSession内的transformed/partition等退出即失。对应machine1.155s+partition.256s；object只有.015s。私改只lower1函数，但TransformProduct完整ScalarIdentity不等仍整图运行；八次machine+partition合计4.777s。

**可直接落地的整链**：在既有函数产品、partition recipe和磁盘对象索引上加窄范围的“当前消费输入→unit→已有对象”依赖关系；把完整当前语义/独立Verify授权完成后的unit判定移到MachineOutput展开之前。当前unit membership、callee ABI、完整导入闭包、入口/地址根/全局初始化、选项与工具链均须绑定；已知domain内完整比较依赖结果，相等才用旧对象，缺项/未知/损坏走原完整生产。对红unit再构造机器表示、原完整体检查、内部化、裁剪、full O2及LLVM Verify。已有语言/机器授权合同若仍要求对象hit前得到完整模型，则该提前路径尚未闭合，不能仅凭dep摘要跳过；先把当前必要绑定和检查沿已有ClosedBody/WorldBound链贯通，不发明“green即Verify”的证书。

这比“把整个MachineOutput落盘”更接近Rust真正省掉的工作。机器产品落盘可作为需要实际值的cache miss恢复辅助，而不是第一选择。保持当前完整O2，不能拿此前closed-only运行质量失败的路线换编译快。

## 3. 磁盘查询缓存：green与“需要读取值”分开，但维护仍有成本

- [`rustc_query_impl/src/execution.rs:459–519`](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_query_impl/src/execution.rs#L459)：green query只有需要值时才按`will_cache_on_disk_for_key`试load；没缓存则provider重算，并验证结果与旧fingerprint一致。并非所有query都存盘。
- `rustc_middle/src/query/on_disk_cache.rs:346–393`：SerializedDepNodeIndex→文件offset的索引，在mmap上按需解码；`648–662`借DefPathHash把旧DefId映射当前DefId，不能持久化进程内ID/指针后直接复用。
- `rustc_query_impl/src/incremental.rs:162–174`反序列化置于`with_query_deserialization`，禁止暗中创建新DepNode。`95–103,146–159`对一部分loaded values检查hash一致性（本提交按session/key分组轮换或显式全验）；这是incremental consistency check，**不是ZIL独立Verify的等价替代**。
- [`rustc_incremental/src/persist/save.rs:60–84`](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/save.rs#L60)：保存前`exec_cache_promotions`把green且旧盘上有值、当前内存还没有的query加载进来，随后序列化下一session缓存；源码72–75明确TODO其在小改/无改时仍做不少工作。
- `graph.rs:1070–1089`只晋升已green节点，不把所有未知旧节点都强制计算；`incremental.rs:105–159`eligible、尚未memory cached才加载和complete。这个维护动作保住下一次热复用，同时有IO/解码/重编码代价，不能将Rust描述为零成本留存。
- `persist/load.rs:58–85`先检查work product所有保存文件存在，缺失删除不完整产物；`98–116`检查dep-tracking选项hash；`file_format.rs:134–167`检查magic/format/compiler version。**这几处不是逐对象内容hash验证**，不要把ZIL现有更强的损坏校验降为存在性检查。
- `persist/save.rs:92–129`保存本次new_work_products并删除上次索引里不再需要的产物。session目录GC由root另外审核，本报告不作“Rust保留所有历史”的错误推断。

**ZIL取舍**：原始查询/函数产品按当前真实消费者选择持久化，稳定身份映射到当前；内容共享与当前可达索引分离，保留当前热路径和必要恢复基线，有界清理无用历史。避免把绿色且本次未读取的完整大值全解码/重编码作为必经步骤；若采用原块延续，需要自己的schema/身份/完整性验证，不能凭Rust TODO声称已经安全可用。新增缓存必须同时测“保留当前热集合、清理历史、再restart/edit”的时间与空间，不能只看少写多少MB。

## 4. 主动优化：与Rust的MonoItem去重/分区精确比较

`rustc_monomorphize/src/collector.rs:354–372,578–600`用MonoItem身份的visited集合避免重复遍历，按used/mentioned模式分别处理；`274–303`构造used_map/user_map并选择LocalCopy依赖。它表明“按实际生成项与引用关系准备后端”的实现方式，**不等于把不同函数因body相等合并**。本次阅读范围不能证明Rust整个编译链有没有ZIL同类body-equality pass，LLVM MergeFunctions政策由root独立审核。

`partitioning.rs:220–281`区分GloballyShared根和LocalCopy；给CGU复制的是可达的inline items，不是无条件所有transitive callee body。`230–239`增量时区分generic volatile组；`318–375`按inline项重叠合并CGU以减少复制，并顾及规模；`423–445`合并后名字按所包含CGU排序稳定。稳定性与优化机会是明确权衡，不能假定Rust在任意编辑下CGU从不变化。

ZIL当前5100eligible→156classes的归并收益大，根unit完整导入保持了运行质量；私改object成本主要是根unit，八次root object3.828s、其他15unit仅.121s。下一优化应先用Rust式消费依赖和早期CGU复用减少无效重构；要探索缩小导入，必须把“选择性导入/分组→fullO2→实际运行质量”作为整条实验，保留此前完整导入所恢复的性能。不能照搬Rust inline启发式就预设ZIL不会运行退化，也不能先放弃原16CGU条件。

## 交付方案：一条实施链，一次集中验收

在现有表示里串起：**实际读取的header/body结果及名称绑定 → 变更后结果比较截断 → class代表/反向调用与导入依赖更新 → 当前unit输入判定 → 先选缓存对象、红unit才构造机器表示 → 发布当前索引并有界清理历史。** 先打通私改、接口改、实际编译器重启；Cargo Fresh与ZIL项目fresh属于另外一层，不能借此绕过用户要求的独立Verify。

边界测试集中覆盖：调用结果相同但输入改动；ABI改变/删除/新增遮蔽与查找失败；类代表分裂合并；地址逃逸/全局初始化；分区变动与O2选项变化；缓存缺件/坏件/版本变化；跨进程ID重排；错误与取消不得发布；清理历史后仍保持当前热集合。正向仍实际main/候选/Rust、默认预算、CGU16/O2/jobs1、独立Verify、完整运行门，报告绝对等待和联合空间成本。

按本窗优先级，私改machine+partition4.777s是首先可触及的累计成本桶；接口Expand+.machine+partition约1.388s、重启machine+partition约1.412s是后续同链收益对象。它们是待削减成本，不是保证节省。1万/2.5万项目受既有token及函数总量硬合同拒绝，Rust查询算法不能消除此合同；容量仍独立标失败，不提额度冒充性能收益。

## 补充：本窗 `lto=false` 不是显式关闭 rustc 的所有 LTO

root核对当前Cargo manifest为`lto=false`，实际cold rustc argv没有`-Clto`，O2/CGU16。上述通用NoLTO→PostLto、LTO→PreLto分支说明仍成立，但不能把本窗直接归入NoLTO。

已继续只读核对同一58cf8f9源码：`rustc_session/src/session.rs:853–860`区分Unspecified与显式No；无flag继续默认选择。`877–913`在支持ThinLTO、无强制关闭、无-Z覆盖、CGU>1且优化级别>O0时返回**ThinLocal**。`rustc_codegen_ssa/src/back/write.rs:799–822`在非linker-plugin-LTO时把ThinLocal映射到ComputedLtoType::Thin，故`base.rs:1266–1277`绿色CGU走**PreLto**复用路径，而非一律直接拷最终对象。按已核参数和正常LLVM后端条件，本窗应按该默认路径解释；本报告未新增运行trace，不把静态推导冒认新的执行收据。

`write.rs:415–424`明确增量且Fat/Thin/ThinLocal需要pre-LTO bitcode；`203–205`令emit_pre_lto_bc开启；`846–862`负责保存，`2275–2286`复用时提交保存的bitcode和work product。因而热缓存所占空间包含跨CGU优化恢复需要的前LTO表示，有实际复用用途，不能都称为多余文件。后续ThinLTO还可能复用已优化产物；不能由PreLto入口推出每次全部CGU重跑LLVM。本窗各类别具体字节由root磁盘审计负责，不能把全部Rust缓存体积都归为bitcode。

此差异也约束ZIL优化方案：对完整导入或跨unit优化后的产物，复用必须绑定导入依赖及优化阶段，不能只因源CGU本体green就复用旧最终对象。今后每项主动优化与落后点均附实际Rust对应路径、适用flags及正确性边界；不以“Rust没有同类优化”作为未经源码证实的前提。
