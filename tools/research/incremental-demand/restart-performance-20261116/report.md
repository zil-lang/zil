# 当前源码编译性能与落后阶段：2026-11-16

**当前没有全面超越 Rust。** 原生完整流程在1k、5.1k领先，2.5k近持平；check 5k wide整体落后，保留缓存的重启和5.1k连续私改是最值得处理的秒级损失。磁盘占用较低，但热缓存等待没有同步胜出。本轮仅测量、定位与保全，没有修改编译器源码，也不申请合入main。

被测源码 `4a23edc7d06bc18cbed1fb958fdaacd886d9607a`，tree `35116da8321766cf49106143bd699faada166dbc`，binary SHA256 `302d58db48f53cd4bc1952f2a9032ead5a6d479314d24eb2ca344f558c2c113e`；测量工具冻结在 `8bb24b85412b4b388d3c3f266786ef36ebe05e6f`，实际main `9170bb4210554392e36eb7528596612596ebef80`。Rust1.99.1/rustc `58cf8f9c35b355199727627d07ae34a4e2846d67`，Cargo `5f94df4789f005f9a352888e8355ffc645b7ed0e`。SDK18/native-link沿固定V32恢复，未重建LLVM。独立Verify、默认预算、完整O2和运行质量门保持；研究开关没有默认开启。

## 同窗总等待

全部采用CGU16/O2/编译jobs1、两个平衡顺序区块，先按每区块求请求wall之和，再取中位数。ZIL常驻发送到响应不含另记的worker启动；Cargo请求含启动。无OS缓存清空，不叫OS冷编译；没有编造统一启动后端到端总数。原生功能输入检查jobs8、持续运行质量串行，均在编译计时外。完整流程包括预期error及恢复，不能称为全部成功构建的均值。

| 独立窗口与流程 | 实际main s | 当前ZIL s | Rust s | ZIL−Rust s |
|---|---:|---:|---:|---:|
| 原生18请求，preset1000 | 8.551 | 7.166 | 8.047 | -0.881 |
| 原生18请求，preset2500 | 13.199 | 10.082 | 10.017 | +0.065 |
| 原生18请求，preset5105 | ResourceLimit | 29.477 | 31.582 | -2.105 |
| check17请求/chain，preset2500 | 3.811 | 4.175 | 4.755 | -0.580 |
| check17请求/diamond，preset2500 | 3.036 | 3.034 | 5.261 | -2.228 |
| check17请求/wide，preset5000 | 6.671 | 7.574 | 7.050 | +0.524 |
| 保留缓存6请求，preset2500 | 5.610 | 3.263 | 3.535 | -0.271 |
| 保留缓存6请求，preset5105 | ResourceLimit | 11.652 | 13.143 | -1.491 |
| 清理缓存6请求，preset2500 | 8.823 | 4.041 | 5.777 | -1.736 |
| 清理缓存6请求，preset5105 | ResourceLimit | 15.464 | 23.185 | -7.720 |

原生1000/2500/5105 preset分别为1005/2505/5105个实际函数、102文件；1k及5.1k用short，2.5k用simple，不能当同一程序的线性扩容曲线。独立check用simple+chain/diamond/wide，不与原生混池。

原生1k总等待比Rust少10.95%，5.1k少6.67%；2.5k多0.65%，应称近持平，仅两个区块不支持稳定小差额排序。check5k比Rust多7.43%，也比实际main多13.54%；check2500 chain比实际main多9.55%，这些负结果均保留。main5105原始Expand限额是失败，不能算快速成功，也不能出回归比。三个native窗口 `candidate_rust_accepted=true` 仅指功能/运行质量，`accepted=false`，不代表速度全面胜出。

## 按实际等待损失定位

| 关键场景 | ZIL s | Rust s | 多等待 s |
|---|---:|---:|---:|
| 5.1k长流程末重启 | 3.562 | 0.043 | +3.520 |
| 5.1k保留缓存重启 | 3.197 | 0.013 | +3.183 |
| check5k重启 | 1.385 | 0.027 | +1.358 |
| check5k冷用户缓存 | 1.622 | 0.807 | +0.815 |
| 5.1k重启后首改 | 1.245 | 0.995 | +0.250 |
| 2.5k位置变化 | 0.727 | 0.531 | +0.196 |
| 2.5k大范围修改 | 1.599 | 1.366 | +0.234 |
| 5.1k错误恢复 | 1.336 | 0.891 | +0.445 |

5.1k保留缓存的**重启+随后首改**为4.441/1.008s，联合多3.433s；两个区块多3.484/3.382s。5.1k长流程的**八次私改**为9.832/8.688s，联合多1.144s，两个区块均落后（+1.222/+1.065s）。2.5k位置变化差额在两个区块为+413/−21ms，方向不稳；2.5k大范围修改为+211/+256ms，方向一致。无修改的若干2–60ms损失也列在后附全量表，优先级不能只按倍数排序。

**重启：对象缓存已命中，仍需补建上游和机器产品。** 5.1k保留缓存重启正式中位耗时：Expand295ms、Types270ms、Analyze152ms、独立Verify105ms、CT68ms、Codegen1547ms；Codegen内machine962ms、partition388ms、object仅19ms，另有请求内未细分747ms。长流程末重启Expand892ms、Codegen1295ms（内machine708/partition299/object25ms），未细分891ms。父项与子项不能相加。两窗都5101个磁盘CFG恢复命中、仅4个Types body执行，但仍执行5101个当前body key、遍历191558个Core节点并重新绑定5101个CFG；Analyze与machine各处理5105个函数，transform/partition不复用。16/16对象命中，主要差距不是LLVM对象miss。

**私改：前端稀疏命中已成立，成本留在全局后端与O2对象工作。** 5.1k八次编辑每次Types/Analyze/lower均只处理1个函数，其余5104复用。八次Types+Analyze合计约4.33ms，Codegen合计8.272s，占9.832s的84.1%；内machine2.386s、partition1.684s、object3.893s。每区块对象miss序列为16/15/16/16/16/15/15/15，即124/128；transform和partition均未复用。原始对象ProfileStats中O2 optimize合计2.348s、emit1.385s，远大于Builder0.041s。它们是object的子项，不能再累加。重启后首改也只检查1体，Codegen1058ms内machine268/partition197/object546ms，16个对象miss。下一突破口是实际依赖下的全局transform/CGU稳定与对象失效粒度，不能再按“Types首改未命中”施工。

**check大图：落后在前端生产与请求内其他工作，不能归到LLVM。** check5k冷编译Types528ms、CT306ms、Expand210ms、Analyze94ms、Verify56ms、未细分425ms；重启Types389ms、Analyze163ms、Expand163ms、Verify139ms、未细分482ms；大范围修改Expand372ms、Types246ms、Analyze110ms、未细分395ms。check没有原生Codegen。2.5k位置变化主要Expand223ms与未细分322ms，Codegen仅56ms且对象3ms；大范围修改则Codegen821ms（machine376/partition144/object184ms）与未细分510ms。错误恢复仍为backend主要占时：5.1k Codegen1043ms内machine502/partition211/object289ms。完整计数与原始响应索引见stages.json和Astra复核。

**Rust参考与诊断边界。** 原正式保留缓存重启的Cargo stderr均为Fresh，无Running rustc；Cargo源码指纹Fresh路径直接保留现有产物。ZIL仍补建真实当前producer，不能用对象hit冒称等价Fresh。Rust普通修改走query green/磁盘缓存或provider与CGU work-product资格检查，参考源码均为本次固定版本。diag02另有36+36请求，只解释读取/序列化等机制，不用于正式比率：5.1k重启FunctionStore读143ms、semantic读95ms、source ingress85ms、semantic publication136ms、function publication24ms。semantic实际写0个新pack，但仍序列化5105个函数/计划，说明零物理写不等于零出版成本。profile Verify724ms与正式75/105ms差异很大，不能把单次profile比例填进正式残差。6条Rust Fresh诊断stderr重放旧time-passes输出，已排除；其中重启17ms却含上一restore的total5.982s，不是本次rustc执行。原正式残差不能无证据直接命名为某API，也不是都可安全删掉。

check5k重启仅101个实际磁盘CFG命中，4904个函数重新Builder检查，producer/RawInfer/PlainCT/Analyze仍各5005项；tiny-function worthwhile门排除了大量小函数，不能套用native short的5101命中。冷编译Types两块175/881ms，同工作计数仍波动，故不把总差全部归给Types。2.5k位置变化则0 body/0 lower，但仍2505次Core和syntax深拷贝、41071 source map项搬迁、2505 CFG位置重绑；完成最终产物在对象编译前复用，object report为空，不是16次对象hit。大范围修改16个partition重建却仅1个对象miss；错误恢复仅1函数lower却15个对象miss。必须区分分区重建、完成产物复用与对象编译，不能混称。详见补充Astra逐计数/源码审核。

## 用户编译磁盘占用

下表为每区块请求边界物理峰值再取中位（MiB，lstat/st_blocks、跨显式根按inode去重）。ZIL含semantic/object缓存、程序与诊断IR，Rust含整个target及输出；不含SDK、宿主编译target、源码或研究证据，不是瞬时峰值，也未强加同缓存配额。八改增长为cold结束到unchanged-after-edits结束的同区块增量。

| 场景 | ZIL峰值 MiB | Rust峰值 MiB | ZIL八改增长 MiB | Rust八改增长 MiB |
|---|---:|---:|---:|---:|
| 原生18请求/1000 | 10.43 | 19.18 | 1.17 | 5.12 |
| 原生18请求/2500 | 13.50 | 36.67 | 1.08 | 11.53 |
| 原生18请求/5105 | 55.34 | 72.86 | 1.58 | 28.48 |
| check17请求/chain/2500 | 9.33 | 29.67 | 0.71 | 7.66 |
| check17请求/diamond/2500 | 9.33 | 29.67 | 0.80 | 7.65 |
| check17请求/wide/5000 | 17.96 | 58.35 | 0.91 | 15.14 |

5.1k原生峰值少24.04%，八改增长少约94.44%。它确实缓解了本组热缓存盘量痛点；但绝对规模仍只有数十MiB，不能推广为工业多crate的GB级节省。保留缓存短流程5.1k峰值34.77/58.69MiB；清理后重启5.028/6.979s，清理确实丢失热编译收益。两窗独立控制，不能把窗口差直接当净因果；cleanup和worker启动原始账目独立保留。

## 质量、覆盖与优先级

六窗全部结束：**774原始请求/258功能检查格/71持续运行质量用例**，候选与Rust均通过既定功能和运行门；额外213条持续运行构建行不进入774。预期错误正确拒绝，main5105共有60条ResourceLimit观测，未抹去。Astra只读复算258配对、258候选响应、142份backend时间账；阶段分析六窗warnings为空。此前正确性/release收据仍按原源码绑定保留，本轮没有重跑无关组件门。

优先级按本组真实等待：先处理3.4s的重启+首改损失及八次私改1.14s损失，再处理check5k的冷编译/重启/大改，再处理2.5k全局编辑；无修改几毫秒差额不抢占这些链路。仅给成本位置和实现突破口，不把全部bucket当作保证可收回的收益。本轮不继续改编译器。

覆盖包括冷用户缓存、进程内无修改、八次真实私改、位置变化、接口变化（native）、大改、错误恢复、恢复原源码、真实进程重启、保留与清理缓存及chain/diamond/wide图。仍是有限标量、102文件、单crate、O2/no-debug；不覆盖工业workspace、跨crate增量、泛型/宏/async/debug/test、依赖版本变化、链接规模及分钟级工程。默认token/整项目10000函数合同仍在，10105/25105不能靠提高预算当作成功。RSS未作新净收益结论；四槽历史与更长序列淘汰语义保留。全面超Rust目标开放、main Hold。

本次是当前4a23对同窗main/Rust的比较，不能用旧bcf/f19跨窗结果给当前代码算净算法收益；98fa/59fd与021997历史仍各自绑定。SDK opaque native_history不是四槽MachineProduct；本组below-minimum-average-lir-work，SDK product selection关闭，不能据它的0计数否定或宣称四槽命中。

## 全部正差额及阶段位置

正数是ZIL−Rust请求wall。所有中位正差额均列出，含方向不稳的小差额；“主要候选耗时”是本编译器的成本位置，**不是对Rust同名阶段相减的因果净损失**。报告外时间与未细分请求内工作分开。父codegen不和其backend子项相加。

| 窗口/preset/请求 | 多等待 ms | 两区块差额 ms | 主要候选阶段 ms | 请求内未细分 ms |
|---|---:|---|---|---:|
| retain/2500/warm-restart | +600.73 | +540.9/+660.6 | codegen 303.4, expand 76.8, types 66.9; 内machine 169.5, partition 58.0, object 6.2 | 143.3 |
| retain/2500/private-001 | +9.00 | +28.1/-10.1 | codegen 307.3, expand 15.2, verify 13.3; 内machine 85.7, partition 70.1, object 118.5 | 63.0 |
| retain/5105/unchanged | +4.42 | +5.2/+3.7 | codegen 1.4, analyze 0.0, verify 0.0; 内machine 0.5, partition 0.0, object 0.6 | 15.5 |
| retain/5105/warm-restart | +3183.24 | +3045.9/+3320.5 | codegen 1547.0, expand 295.3, types 270.1; 内machine 961.6, partition 388.2, object 18.7 | 747.3 |
| retain/5105/private-001 | +249.78 | +438.0/+61.6 | codegen 1057.9, verify 28.9, expand 17.1; 内machine 267.7, partition 197.3, object 546.2 | 125.8 |
| native/1000/unchanged | +4.44 | -11.6/+20.5 | codegen 5.1, verify 0.1, analyze 0.0; 内machine 0.3, partition 0.1, object 3.7 | 34.2 |
| native/1000/private-001 | +166.16 | +237.3/+95.0 | codegen 345.8, expand 43.2, verify 16.1; 内machine 54.9, partition 42.7, object 214.3 | 95.8 |
| native/1000/private-004 | +42.66 | -63.4/+148.7 | codegen 309.1, expand 27.1, verify 14.3; 内machine 49.2, partition 45.7, object 186.5 | 73.1 |
| native/1000/warm-restart | +712.14 | +428.8/+995.4 | codegen 488.5, expand 60.0, types 21.1; 内machine 321.6, partition 74.4, object 12.0 | 125.6 |
| native/2500/unchanged | +24.85 | +22.8/+26.9 | codegen 5.3, verify 0.2, analyze 0.0; 内machine 0.5, partition 0.1, object 3.4 | 40.3 |
| native/2500/private-007 | +58.48 | +264.5/-147.5 | codegen 364.9, expand 55.9, verify 20.9; 内machine 85.4, partition 53.7, object 174.2 | 149.1 |
| native/2500/unchanged-after-edits | +5.92 | +19.0/-7.1 | codegen 4.9, verify 0.2, analyze 0.1; 内machine 0.4, partition 0.0, object 3.8 | 29.5 |
| native/2500/positions | +196.13 | +412.8/-20.6 | expand 222.9, codegen 56.3, types 39.4; 内machine 44.8, partition 0.0, object 3.1 | 322.4 |
| native/2500/broad | +233.61 | +211.3/+256.0 | codegen 821.5, expand 116.3, types 48.9; 内machine 375.9, partition 143.6, object 183.7 | 509.6 |
| native/2500/recovery | +149.46 | +164.4/+134.5 | codegen 293.4, expand 21.2, verify 14.6; 内machine 91.4, partition 52.5, object 123.1 | 132.9 |
| native/2500/warm-restart | +835.29 | +584.2/+1086.4 | codegen 331.6, expand 191.9, types 82.1; 内machine 189.9, partition 53.9, object 9.1 | 217.1 |
| native/5105/unchanged | +59.60 | +49.0/+70.2 | codegen 8.5, verify 0.1, analyze 0.0; 内machine 0.8, partition 0.0, object 6.2 | 64.6 |
| native/5105/private-001 | +76.42 | +78.9/+73.9 | codegen 1104.7, verify 41.3, expand 18.7; 内machine 295.0, partition 227.3, object 533.4 | 102.7 |
| native/5105/private-002 | +114.15 | +256.1/-27.8 | codegen 1049.7, verify 42.8, expand 29.0; 内machine 294.8, partition 225.3, object 486.1 | 94.6 |
| native/5105/private-003 | +128.17 | +94.1/+162.2 | codegen 1107.5, verify 43.1, expand 27.8; 内machine 327.1, partition 234.3, object 511.9 | 92.6 |
| native/5105/private-004 | +226.86 | +199.3/+254.4 | codegen 1124.6, verify 43.5, expand 17.5; 内machine 329.7, partition 238.4, object 519.9 | 113.0 |
| native/5105/private-005 | +127.58 | +156.1/+99.0 | codegen 985.6, verify 46.0, expand 39.6; 内machine 305.9, partition 188.8, object 454.5 | 111.6 |
| native/5105/private-006 | +333.77 | +338.4/+329.1 | codegen 1008.5, verify 49.1, expand 48.4; 内machine 278.9, partition 184.1, object 505.8 | 129.3 |
| native/5105/private-007 | +17.46 | -36.8/+71.7 | codegen 922.9, verify 68.4, expand 23.5; 内machine 278.3, partition 184.7, object 424.1 | 94.8 |
| native/5105/private-008 | +119.13 | +135.5/+102.7 | codegen 969.2, verify 37.6, expand 14.4; 内machine 275.9, partition 201.1, object 457.0 | 86.9 |
| native/5105/unchanged-after-edits | +32.96 | +10.7/+55.2 | codegen 6.4, analyze 0.1, verify 0.0; 内machine 0.8, partition 0.0, object 4.4 | 42.1 |
| native/5105/recovery | +445.01 | +198.9/+691.2 | codegen 1042.7, verify 58.1, ct 22.6; 内machine 501.9, partition 210.5, object 288.8 | 179.3 |
| native/5105/warm-restart | +3519.61 | +3428.7/+3610.5 | codegen 1295.1, expand 891.7, types 208.6; 内machine 708.4, partition 299.1, object 24.7 | 891.0 |
| check-2500-chain/2500/private-001 | +152.09 | -39.8/+344.0 | expand 176.0, verify 23.2, ct 12.7 | 83.6 |
| check-2500-chain/2500/private-005 | +82.28 | -97.9/+262.5 | verify 36.3, expand 9.6, ct 7.9 | 180.1 |
| check-2500-chain/2500/broad | +120.68 | +341.4/-100.1 | expand 207.2, ct 91.8, types 74.8 | 246.7 |
| check-2500-chain/2500/restore | +226.17 | -206.0/+658.3 | types 335.9, expand 132.6, analyze 38.2 | 204.6 |
| check-2500-chain/2500/warm-restart | +768.80 | +250.4/+1287.2 | expand 490.4, types 69.7, analyze 27.9 | 158.6 |
| check-2500-diamond/2500/private-005 | +11.37 | -64.2/+86.9 | expand 33.8, verify 20.9, ct 8.9 | 77.1 |
| check-2500-diamond/2500/private-006 | +2.32 | +85.5/-80.8 | expand 73.6, verify 21.5, ct 8.4 | 76.1 |
| check-2500-diamond/2500/warm-restart | +283.36 | +300.4/+266.3 | expand 73.1, types 56.3, analyze 25.1 | 115.5 |
| check-5000-wide/5000/cold | +814.58 | +579.9/+1049.3 | types 527.7, ct 306.0, expand 210.3 | 425.0 |
| check-5000-wide/5000/private-005 | +13.56 | -260.9/+288.0 | verify 222.1, ct 14.7, expand 11.8 | 70.2 |
| check-5000-wide/5000/broad | +415.93 | +323.4/+508.4 | expand 372.0, types 246.1, analyze 110.4 | 394.8 |
| check-5000-wide/5000/restore | +29.77 | +511.0/-451.4 | expand 195.4, analyze 152.2, types 103.2 | 303.5 |
| check-5000-wide/5000/warm-restart | +1357.59 | +1312.1/+1403.1 | types 388.9, analyze 163.5, expand 163.3 | 481.6 |
| clear/2500/private-001 | +115.28 | +385.6/-155.1 | codegen 510.6, verify 14.1, expand 12.4; 内machine 89.1, partition 173.4, object 156.5 | 96.1 |
| clear/5105/unchanged | +9.65 | -7.1/+26.4 | codegen 3.9, verify 0.1, analyze 0.1; 内machine 0.6, partition 0.0, object 2.6 | 40.2 |
| clear/5105/private-001 | +130.41 | +116.2/+144.6 | codegen 1103.8, ct 100.7, verify 57.9; 内machine 312.0, partition 219.1, object 501.7 | 99.1 |

## 原件与恢复

- [结构化结果与每区块差额](results.json)、[全部阶段/计数/原件hash](stages.json)、[阶段简表](stages.md)。
- [次要落后项逐计数与源码审阅](restart-performance-secondary-losses-astra-01.md)。
- [Astra原始数字与因果复核](restart-performance-results-astra-audit-01.md)、[Rust/Cargo/current-source路径审核](restart-performance-causal-review-01.md)。
- [完整原始证据恢复索引](../evidence/restart-performance-core-20261116/RESTORE.json)：六窗完整cache/artifact/raw、单列诊断02、首次诊断setup失败、pins/commands/scripts/logs及归档完整fresh校验。证据归档后恢复出的hardlink展开占用不能重新计算产品盘量。
- 当前二进制与正确性首次失败见固定[8bb24恢复入口](https://github.com/zil-lang/zil/blob/8bb24b85412b4b388d3c3f266786ef36ebe05e6f/tools/research/incremental-demand/evidence/restart-function-chain-20261116/RESTORE.json)；本轮core也复制了实际使用的两份DSO及完整native-link profile/root；完整重构建SDK仍依赖固定[V32](https://github.com/zil-lang/zil/blob/1eecf9b81a4929dde79e4fa98cd0ee40fb74af75/research/incremental-demand-full-chain-20261113-v32-recovery/RESTORE.json)。

准备期旧bcf全raw pack的ENOENT，以及diag01缺模块alias的setup异常均保留。后者发生在0个编译请求之前；修正仅在外部诊断脚本，diag02独立成功，未改变正式收据或编译器。归档前仅清除可重建的宿主debug/incremental以腾出完整fresh校验空间，不计作产品磁盘优化；源码/SDK/release/原始用户cache与证据保留。
