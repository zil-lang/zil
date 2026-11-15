# ZIL 当前落后 Rust 的源码与证据审核

2026-11-16（Asia/Shanghai）；Astra 只读审核。checkout `research/full-push-native` / `53c077e723fbd3aa43e96962260d4834d33ce96b`，工作树干净；性能仅绑定编译源码 `027695318436fcb1f8994d0dc9802a0b2349c362`、binary `5b3a309a7860dca3a1d3f15e7c930ec7d3f3a3fa59286e6d91c67e0067c0743f`。未执行编译、原生程序、测试或 Git 写操作。只新增本报告。不重复行业资料研究或磁盘分类审计。

## 判断

当前优化确实降低了限定合成项目的完整等待，但**没有解决“保留磁盘缓存就能获得接近 Rust 的跨进程热编译”**。磁盘语义缓存和对象缓存均命中后，ZIL 仍支付大量机器表示重建、整图变换、分区及当前 Verify 成本。低磁盘占用与热编译能力必须同时看；不能用 resident no-op 代替正常退出后再编译，也不能由边界峰较小推出清理后仍能保持热编译。

三族使用高度重复的纯整数直接调用图。5105 输入中 5100 eligible functions 归为156 classes，4944 calls rewritten；冷请求又删除7104个各unit中的不可达本地定义。这是合法、有运行质量支撑的收益，但重复率非常高，不能外推到大规模独特函数、泛型/宏/async/多包工作区。既有运行门成立不等于行业覆盖充分。

## 已证明的成本，不重复相加

来源：`out/runs/closed-project-window-05/rows.jsonl` 指向的 `raw.archive`；只取 `arm=candidate,preset=5105,sustained=false`。两block的中位数；私改为每block八次之和再取中位数。以下 codegen 子桶嵌套在总时间中，不能另加到总时间；object 内的 LLVM 时间亦不能再次加到 object。

| 场景 | ZIL/Rust完整等待 s | ZIL内部定位 s |
|---|---:|---|
| 冷编译 | 3.835 / 6.719 | codegen 1.971，其中 machine .917、partition .365、object .547；Expand .355、Types .218、Analyze .490、Verify .092 |
| 八次私改 | 10.754 / 9.859 | codegen合计9.257，其中 machine 2.498、partition 2.279、object 4.069、diagnostic .155、link执行 .224 |
| 接口变化 | 2.688 / 1.852 | codegen1.478，其中 machine .680、partition .250、object .516；Expand .458、CT .077、Types .070、Analyze .044、Verify .077 |
| 跨进程热缓存重启 | 2.232 / .0217 | codegen1.531，其中 machine1.155、partition .256、object .015；Verify .265、driver Lower .0085 |

请求计时还含未在以上 phase 中完全展开的加载/缓存等工作；`exclusive_costs`、`source_ingress`在本窗为null，不能把余数武断归为IO。machine 包括 backend lowering/变换以及很小的scoped-thread扫描，未对每个pass独立计时，不能声称1.155s全属于等价类算法。

**关键纠偏：私改15–16个object miss并非15–16份昂贵优化。** 八次私改 unit0 的object报告总计3.828s，其中完整O2 optimize2.333s、emit1.402s；其他15unit合计仅.121s。根unit含完整导入，实际优化工作集中于此。单纯把空unit命中数做好，最多触及很小部分，不能解决主要等待。

## 具体逻辑与应改的完整链

### 1. 优先做“实际依赖变更→变换→导入分区→当前检查→对象/链接”的增量链

证据：私改只lower 1函数、复用5104函数；当前Verify主要检查1函数，前端Expand仅执行1module/1item。但 `reused_machine_transforms=false`、重建16partition、全部16未裁unit重新检查。每次仍约.28s machine+.25s partition。

源码入口：
- `crates/zil-backend-llvm/src/build.rs:421–476`：TransformProduct要求整个ScalarIdentity一致；任一函数变化则 `machine.to_module()`，全图checked-add/scalar/class passes重新执行。
- `crates/zil-backend-llvm/src/incremental.rs:255–271`：完整产品列表决定变换命中。
- `build.rs:515–575`：PartitionInput整值命中；现有recipe路径明确 `!incremental_o2`，当前完整导入配置用不上。
- `build.rs:669–730`：完整导入、内部化和逐unit原始体检查/裁剪重新运行。
- `machine_call_classes.rs:366–406`：等价类每次重新建，按源顺序选代表。一个代表变化会牵动等价成员的调用目标；不能只dirty原修改函数而漏掉原class成员和反向调用闭包。

落地方向：沿已有函数产品/当前输入所有权做依赖级更新；维护反向调用依赖、class成员与代表变更、unit导入依赖和当前可达根。只重算变动函数及受影响调用闭包；等价类分裂/合并必须更新所有受影响成员。沿用现有partition recipe思想扩展到完整导入策略，最终对实际变动unit重做原完整输入检查、内部化、裁剪与当前preflight。不要增加新IR或通用证书体系；完整未知域、地址逃逸、递归、全局初始化依旧保守回退。

收益尺度：本窗八私改 machine+partition **4.777s是可触及桶总量，不是可承诺节省**；只消除其中约20%已约.955s，足以覆盖私改落后Rust的.895s，但依赖维护开销与有效范围需实测。先做这一链，比继续压no-op几毫秒更值得。完整导入导致根unit重做O2属另一个真实成本；当前资料不支持减少完整O2或撤销导入。既往native140捕获无净益已回退，不建议重新包装同样方向。

对象键源码 `object_cache.rs:349–362` 已排除仅诊断用的module fingerprint，按实际items散列。不能把miss笼统归因于“全局source hash污染”。代表变化/声明与导入集合变化会影响items，是需精确diff验证的具体假说；本报告没有取得每次所有unit模型的前后diff，不将其列为已证明的全部miss原因。

### 2. 同批收口真实接口依赖，避免不相关函数失效

已证明：接口修改只改变2个语义结果，初始化函数查询仅执行6/复用5099；但Expand执行102module/5105item，CT检查全部5105body，backend重lower全部5105函数、投影访问201776条指令。最终15object命中，仅1重编，说明大量重建最终产物相同。

源码：`zil-expand/src/incremental.rs:930–949` 环境不兼容清除所有entry的receipt/normalized/restored/lower；`zil-ct/src/plain_session.rs:173–176,312–369` 用整个hint环境所有header决定旧body可复用；`zil-backend-llvm/src/emit/incremental.rs:301–367` 把所有函数header装入Context，任何差异清空products/output_bodies。当前Verify局部函数proof已复用5101，但region/lexical仍检查5103，不应以“Verify已完全局部化”描述。

落地：在以上真实消费者分别保留所读取的callee header/类型/ABI/名称绑定依赖；changed header传播到实际读取者，未知读取仍全失效。必须保留删除/新增同名/遮蔽/重排/外部ABI/负依赖，不可以只存已有callee正向边。A/V各自建立和验证其依赖，不互借结论。与变换/partition更新同一链一次验收，避免前端缩小了失效而backend仍整清。

本窗可触及主要量为接口Expand .458 + machine .680 + partition .250s，三者不重叠；尚不能保证全部消除，也不能把差额 .836s直接宣称已解决。

### 3. 跨进程缓存恢复必须覆盖机器产品恢复成本

已证明：两block重启均semantic disk hit、16/16 object hit，却重新lower5105、重建16partition、重渲染5109diagnostic items。`BackendSession` 的lower/transformed/partition/keys/preflights/link/final_product是内存owned（`incremental.rs:9–26`），进程退出后丢失；磁盘object只在上述重建后被lookup。

落地方向：在现有磁盘缓存边界增加**有界、可丢弃的原始机器产品/分区输入索引恢复**，读取时必须精确绑定当前源/工具链/目标/配置/当前语义结果，缓存数据不承担Verify权限；当前独立Verify及必要机器输入检查仍运行。设计前先确定恢复原始模型和绑定它的成本，不能只把大内存Session全部序列化。按函数/内容共享，限定历史保留量，损坏/缺失走正确重建。实现必须一起交付删除过期历史后再restart/edit的验证，否则可能以额外磁盘换取速度又重演Rust痛点。

成本上限：machine+partition约1.412s是首要对象；object仅.015s，link执行.022s，不值得把重启主要问题归为链接器。仍有Verify .265s和未展开加载开销。Rust .0217s很可能是Cargo fresh检查而未运行rustc；应由root核对Rust进程证据。**要求每次ZIL独立当前Verify，同时承诺跨进程达到Cargo不启动编译器的21ms，当前证据不支持。** 应分别列构建系统fresh判断和实际编译器热恢复，产品端到端差距仍要承认，不能换口径制造胜利。

## 容量合同：不能把硬上限说成重复工作

`zil-base/src/lib.rs:270–278` 默认 max_tokens=1,000,000、max_nodes=1,000,000、max_functions=10,000。`zil-syntax/src/lexer.rs:34–45` 比较实际已产生token数；空白/换行/注释也token化（107–124）。`zil-driver/src/project.rs:2284–2332` 每模块使用剩余额度，按唯一发现的文件消费；`charge_frontend:1774–1786` token数分别扣token和node两个不同ledger。后续pipeline自己的remaining从原limits重建（778），不是把discovery已扣余额再扣第二遍。`project_discovery_cache.rs:397–408`也对缓存记录按全项目总token重新核额度。

实际10105冷失败落在file89，25105在file36，均lex token limit，尚未Verify。没有证据显示这次失败是重复扣同一ledger造成。只读数当前原源共10105/25105个fn（102文件，1,676,812/4,205,521bytes）；即使token环节通过，`project.rs:2534–2538`和`zil-ct/src/plain_session.rs:204–208`还会逐module累计扣functions，超过10000的整项目必然仍受硬合同限制。

因此在“数值与整项目预算语义均不改”约束下，不能承诺10105/25105单项目成功；减少工作不减少真实函数个数。未来独立编译单元/包边界或预算合同变更是产品设计议题，须显式界定隔离与每次资源上界，不能暗中改成每文件补满预算，更不能用提高额度冒充编译性能优化。先把硬容量失败与可优化工作开销分清；原失败保留。当前后端优化并未解决这个工业大项目准入缺口。

## 完整链完成后一次性验收

使用actual main/新候选/Rust、同CGU16/O2/jobs1，check与native分开。保留现有18请求及原失败；增加低重复/大量独特body、代表分裂合并/删除、接口新增删除/负依赖、地址与全局initializer、跨进程恢复后私改、错误恢复。独立Verify、默认预算、完整O2、实际16unit、运行质量门均保持；缓存缺失/损坏/取消不得发布旧成功产物。

“热缓存+体积”须联合列三种起点：保留全部、仅保留当前可达缓存并清理历史、删除缓存；每种做restart→私改→接口改，报告绝对等待、用户缓存保留量及增长、原生运行质量。不能只展示清理后空间降低而不测丢失多少热收益。另加较长编辑历史观察缓存是否趋稳；八编辑只能说明短历史增长。不要把这些新增实验结果预写为已实现。

排序：容量硬合同明确列未解决；施工主线优先“私改/接口实际依赖→机器变换→导入分区”以争取重复编辑累计秒数；跨进程恢复与有界缓存作为紧接的完整产品链。上述建议是定位和可落地边界，未执行实现或声称可量化收益已达成。
