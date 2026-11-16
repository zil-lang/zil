Formal04 / 305 结果独立只读审计

结论：未发现阻止采用本轮有限范围结果的版本、配对或数值汇总 blocker；正式结果不能表述为全面超过 Rust。最大的稳定负项仍是重新启动后的当前状态恢复。native5105 完整流程两区块领先，八次私改却两区块均落后；retain5105 真正连续 restart＋firstedit 仍多等约 4.368s。全局 accepted=false，main5105 默认 Expand ResourceLimit 保留，不能转成有效 main 速度比。

审计范围仅为指定 report/results/stages/current-acceptance，以及有明确路径的小型 manifest、summary、completion、gate receipt、原 rows、选定 response 和 Rust restart stderr。没有编译、测试、重放程序、额外计时、源码修改或读取大归档。本报告只写外部审计文件。

版本、来源和计数已核对：

- compiler/tools 都是 `305175a125240f5dd7ae4d935352047d43992723`，tree `4adb25dc86d0f418f883f8ad72d46c303d1d4dc7`；三窗 manifest/stages 与 pins 一致，工具 checkout clean，记录的 diff 为空。
- release receipt 与三窗记录一致：`b051fa85d441394d7e698fd932fa638a09c707e5b5b05a780314c41ce873c7ce`。实际 main 为 `9170bb4210554392e36eb7528596612596ebef80`。本审计核对记录与小型 receipt hash，没有重新读取大二进制做 hash。
- 三窗同一 Rust 1.99.1 工具：rustc 实体 SHA256 `b53b780f77f952062f8ca8a3b70aef2390b1e45ad91939be94afdc9543026853`，Cargo `a670e02cfa39ac2e53f77a2d911ca1fc9e2c479b0603ec956625be1daf24e3af`。candidate 与 main 的 LLVM DSO 记录不同，不能当成只有前端算法不同的严格消融。
- 原 rows/manifest/summary/completion 的记录 SHA256 全部核对成功。原请求为 72＋324＋102＝498，三臂源身份逐格一致，共 24＋108＋34＝166 格；逐项原 wall 值与 stages 配对一致，重算所有 request/flow/八私改中位数与 results 相符。原 rows 的 valid=true 为 60＋288＋102＝450；另48条正是 main5105 ResourceLimit（12＋36）。因此“498请求完成”不等于“498编译成功”；预期 error 请求的正确失败也不代表生成 executable。
- runtime summary 分别10和51例，candidate/Rust门通过；check没有 runtime case，其 results 中 quality/accepted=true 是 check 功能有效性汇总，不是原 summary 存在同名 native runtime 门。未把 runtime效率换算成编译速度。
- current-acceptance 六份 fresh gate receipt 的 hash/内容相符；八项 exact reuse 源 tree/blob 与305及所列旧提交一致。backend314/publication2/tool22/strict/fmt/release 是305收据，其余明确继承；不是全部新跑。404a/da06/4a23的数字不转记。

主要负结果与区块漂移（单位秒；各格 C/R，区块0、1按同窗同请求配对）：

| 范围 | block 0 | block 1 | 两区块中位数 C/R |
|---|---:|---:|---:|
| retain5105 restart | 4.193 / 0.0324 | 4.633 / 0.0190 | 4.413 / 0.0257 |
| retain5105 首改 | 1.582 / 1.426 | 1.206 / 1.400 | 1.394 / 1.413 |
| retain5105 连续 restart＋首改 | 5.774 / 1.458 | 5.839 / 1.419 | 5.807 / 1.438 |
| native5105 完整18请求流程 | 34.886 / 36.468 | 35.810 / 39.110 | 35.348 / 37.789 |
| native5105 八私改 | 12.077 / 11.906 | 12.181 / 11.710 | 12.129 / 11.808 |
| native2500 完整流程 | 10.653 / 10.585 | 10.339 / 10.725 | 10.496 / 10.655 |
| check5k 完整流程 | 6.386 / 7.977 | 7.137 / 7.855 | 6.762 / 7.916 |
| check5k cold | 1.418 / 1.514 | 0.965 / 1.602 | 1.191 / 1.558 |
| check5k restart | 0.705 / 0.0810 | 1.665 / 0.0220 | 1.185 / 0.0515 |

retain5105 首改中位数仅领先18.9ms，且一输一赢，不能称稳定领先；restart＋首改两个区块分别落后4.316s、4.420s。native5105八私改分别落后171.2ms、471.0ms，完整流程领先并未覆盖掉这个重复等待短板。native2500完整流程一输一赢，仅可称此两样本近持平。retain5105完整流程同样一输一赢（C−R为+2.258/−2.558s），不能用其中位数约领先150ms作稳定收益结论。

check5k cold 两区块确实都领先；Rust 从1.514变1.602s（约+5.9%），candidate从1.418变0.965s（约−32.0%），中位数366.7ms优势包含双方漂移。仅两次平衡区块、未清OS缓存，不能判明漂移原因或作跨4a/404a算法收益归因。check positions 差额+277.0/−146.4ms、broad −46.8/+377.8ms均变号；restore +121.3/+297.2ms与restart则两区块都落后。retain5105 cold也由+48.0ms变为−2.901s，进一步说明不能选择有利cold中位数泛化。

连续配对与成本范围：retain 请求顺序明确为 cold→unchanged→warm-restart→private-001→error→recovery，以上 pair 按每个区块真实相邻两请求先求和。native 的 warm-restart 位于最后017，private-001在前部，不能相加冒充真实重启首改。ZIL resident wall 不含进程启动、Cargo请求含启动；retain5105 candidate另存 startup-ready统计约6.5–9.9ms，不能称5.807s已含全部启动。完整flow只含原请求，包含错误/恢复等待，不含独立质量运行和清理；这些口径不能与runtime成绩混用。已核所有12份Rust restart stderr的记录hash与Fresh行，未见 rustc 启动；这里Rust实际做的是Cargo Fresh路径，不是内部编译阶段对照。

最主要剩余阶段与可证范围：

1. retain5105 restart 中位 codegen父项1940.8ms，其 backend total1940.7ms；子项 machine1009.1、partition765.0、diagnostic89.8、object14.9ms。另 Expand391.3、Types169.0、Analyze115.2、CT85.9、Verify78.0ms；报告内未分账1623.3ms，wall外于report仅2.7ms。原response两区块均4个Builder/5101个真实restored CFG命中、Hydrated、独立Verify1、16objects hit/0miss；但CT body与Analyze product queries各5105次、machine lowering/representation/source projection当前全域工作，machine/transform/partition全体复用标志为false。故不是LLVM对象未命中，也不是首改缓存假命中；重启仍在当前前端及 backend构建中付费。未分账1.623s没有细分计时，不能全扣给wire decode、canonical encoding或I/O。
2. retain首改当前Types/CT/Analyze/lower均1函数执行、其余5104保留；CFG统计缺失，不能拿前次restored计数补填。真实15objects hit/1miss，backend机器/分区仍有全局工作。native5105八私改也每次1 lower/5104 reuse、2个class key重建/5098复用、checked-add 1检查/5103复用、15hit/1miss。八次backend总计中位9.552s，其中object4.494、machine2.928、partition1.608、diagnostic0.215s；这说明剩余candidate成本所在，不是把相对Rust落后321ms直接归因于任一桶，也不等于object桶全是LLVM优化。
3. native末尾restart中位4.194/R0.0369s；两个区块machine2055.6/779.9ms，Verify74.5/1038.8ms，波动不能归结为固定算法步数增长。两次仍16hit及相同4/5101 CFG恢复。native restore则当前Types5000体/100位置重绑、CT5100执行、Analyze与lower5000执行/105复用，machine/transform/partition均未整链命中；16对象hit只说明对象层成功，不证明冷历史仍在四槽内。SDK native_history字段不是MachineProduct四槽，不混读。
4. check5k restart主要观测为Expand161.4/1096.7ms、未分账282.4/300.8ms，都是4Builder/5001restored CFG，CT和Analyze product仍5005当前执行。broad中位Expand275.0、Types107.3、Analyze79.5、Verify69.6、CT61.4ms，未分账531.8ms；cold未分账910.1/508.4ms解释了candidate区块变化的很大部分，但其内部原因未测。check没有codegen/backend，不把缺字段当零成本命中。
5. 另一个两区块均落后的项目 native2500 broad：差额186.7/151.4ms；candidate中位codegen559.4ms（machine283.3、partition124.7、object101.0为子项），Expand160.9ms及未分账473.7ms。仍只能定位candidate已记录工作，不能推出Rust同名阶段更快。

阶段核算边界：codegen包含backend total，backend total包含各子桶；driver桶为空，正式cost_profiled=false的零细分计时不表示无该工作。report residual、wall−report、phase/backend是不同层次，不能父子累计；CT等phase及backend工作计数也不能混成同一函数总数。results 的 backend_ms 内部字段名仍带 `_ns` 后缀，但值已转换为毫秒；本报告按ms读取。原选定response hash/phase值已核对。没有使用旧diag02、Rust time-passes或旧profile比率填充本轮缺字段。

盘量复算：原快照 total 横跨各arm，必须按 categories 的arm前缀取值。按同样边界、inode去重和physical_bytes口径，全部reported峰值/八私改增长可复算。native5105 candidate两个区块边界峰值为41.0977/35.9961MiB，中位38.5469；Rust各72.8555MiB。candidate八私改净增长5.1836/1.2344MiB，中位3.2090；Rust各28.4805MiB。报告38.55是“区块峰值的中位数”，不是全窗最大值，更不是瞬时峰值或等额cache限额比较。只含显式用户缓存、程序/IR与Rust target；SDK、host target及证据归档不计产品节省。

保留结论：此正式窗口支持完整流程、对象局部失效与用户盘量的有限实测结果；同时明确保留重启、5.1k八私改及check广改/还原等负项。仅两区块，不足以证明整体稳定冷优、跨版本算法净收益或工业多crate普遍优势。main采纳门与全面超Rust目标仍未完成。
