# 稳定类与重启缓存整链：当前正式结果

被测源码 `305175a125240f5dd7ae4d935352047d43992723`；发布二进制 SHA256 `b051fa85d441394d7e698fd932fa638a09c707e5b5b05a780314c41ce873c7ce`。实际 main `9170bb4210554392e36eb7528596612596ebef80`；工具冻结 `305175a125240f5dd7ae4d935352047d43992723`。本轮三窗均使用同一冻结compiler与tools；实际layout key贯穿所有生产/复用和消费路径。旧4a23、da06结果不转记。

整链实现：稳定纯DAG类实现经完整CGU导入、原复制预算、逐unit内化/裁剪、实际对象键；完整Function/真实callee类依赖规划缓存；checked-add已完成规划的有界缓存；短标量CFG及原六费用的磁盘恢复；单次wire解码绑定真实字节段、raw编码复用及跨Store传递。研究开关默认关闭；独立Verify、原默认预算、完整O2保持。

统一验收使用同O2/CGU16/compiler jobs1、两平衡区块；check/native分别比较。ZIL resident请求不含进程启动，Cargo请求含启动，启动另存；没有清空OS页缓存。native输入核验jobs8，持续运行质量串行。

本轮 498 次原始请求、166 个功能格、61 个持续运行质量用例；实际状态见下表。

| 窗口 | 原始请求 | 功能格 | 运行用例 | candidate/Rust质量 | 全局采纳 |
|---|---:|---:|---:|---|---|
| retain | 72 | 24 | 10 | True | False |
| native | 324 | 108 | 51 | True | False |
| check-5000-wide | 102 | 34 | 0 | True | True |

498次完成不等于498次成功：48条是main5105默认Expand ResourceLimit，正确error拒绝也不生成程序；166格是当前协议candidate/Rust功能核验，check的通过不含原生运行门。main5105在默认Expand限额失败，不是快速成功，也不产生有效main编译比值。全局采纳须区分该缺口和candidate/Rust质量。

## 完整请求等待（秒）

| 窗口/规模 | 请求组合 | main | candidate | Rust |
|---|---|---:|---:|---:|
| retain/2500 | 完整流程 | 6.213 | 4.608 | 4.395 |
| retain/2500 | cold | 2.815 | 1.639 | 1.618 |
| retain/2500 | unchanged | 0.234 | 0.030 | 0.034 |
| retain/2500 | warm-restart | 0.264 | 1.060 | 0.027 |
| retain/2500 | private-001 | 0.594 | 0.412 | 0.487 |
| retain/2500 | 真实连续重启+首改 | 0.858 | 1.472 | 0.514 |
| retain/5105 | 完整流程 | ResourceLimit | 16.303 | 16.453 |
| retain/5105 | cold | ResourceLimit | 6.045 | 7.472 |
| retain/5105 | unchanged | ResourceLimit | 0.039 | 0.039 |
| retain/5105 | warm-restart | ResourceLimit | 4.413 | 0.026 |
| retain/5105 | private-001 | ResourceLimit | 1.394 | 1.413 |
| retain/5105 | 真实连续重启+首改 | ResourceLimit | 5.807 | 1.438 |
| native/1000 | 完整流程 | 9.906 | 7.970 | 9.521 |
| native/1000 | cold | 1.955 | 0.893 | 1.119 |
| native/1000 | unchanged | 0.308 | 0.080 | 0.036 |
| native/1000 | warm-restart | 0.199 | 0.900 | 0.037 |
| native/1000 | private-001 | 0.981 | 0.423 | 0.435 |
| native/1000 | positions | 0.364 | 0.184 | 0.452 |
| native/1000 | interface | 0.519 | 0.554 | 0.667 |
| native/1000 | broad | 1.866 | 0.738 | 0.971 |
| native/1000 | restore | 0.247 | 0.675 | 1.070 |
| native/1000 | 八次私改合计 | 3.789 | 3.538 | 4.340 |
| native/2500 | 完整流程 | 13.616 | 10.496 | 10.655 |
| native/2500 | cold | 3.233 | 1.522 | 1.587 |
| native/2500 | unchanged | 0.239 | 0.051 | 0.034 |
| native/2500 | warm-restart | 0.267 | 1.089 | 0.017 |
| native/2500 | private-001 | 0.596 | 0.594 | 0.536 |
| native/2500 | positions | 0.481 | 0.496 | 0.575 |
| native/2500 | interface | 0.828 | 0.968 | 0.963 |
| native/2500 | broad | 2.151 | 1.358 | 1.189 |
| native/2500 | restore | 0.185 | 0.947 | 1.223 |
| native/2500 | 八次私改合计 | 5.449 | 3.553 | 4.309 |
| native/5105 | 完整流程 | ResourceLimit | 35.348 | 37.789 |
| native/5105 | cold | ResourceLimit | 5.452 | 6.874 |
| native/5105 | unchanged | ResourceLimit | 0.043 | 0.023 |
| native/5105 | warm-restart | ResourceLimit | 4.194 | 0.037 |
| native/5105 | private-001 | ResourceLimit | 1.634 | 1.656 |
| native/5105 | positions | ResourceLimit | 1.343 | 1.842 |
| native/5105 | interface | ResourceLimit | 2.868 | 2.845 |
| native/5105 | broad | ResourceLimit | 4.153 | 6.086 |
| native/5105 | restore | ResourceLimit | 3.603 | 6.083 |
| native/5105 | 八次私改合计 | ResourceLimit | 12.129 | 11.808 |
| check-5000-wide/5000 | 完整流程 | 7.922 | 6.762 | 7.916 |
| check-5000-wide/5000 | cold | 1.235 | 1.191 | 1.558 |
| check-5000-wide/5000 | unchanged | 0.066 | 0.017 | 0.020 |
| check-5000-wide/5000 | warm-restart | 0.071 | 1.185 | 0.052 |
| check-5000-wide/5000 | private-001 | 0.417 | 0.110 | 0.248 |
| check-5000-wide/5000 | positions | 0.727 | 0.947 | 0.881 |
| check-5000-wide/5000 | broad | 0.659 | 1.127 | 0.962 |
| check-5000-wide/5000 | restore | 0.078 | 0.987 | 0.778 |
| check-5000-wide/5000 | 八次私改合计 | 4.303 | 1.120 | 3.032 |

各组先按区块合计再取两区块中位数。完整流程只含原编译请求，不含校验运行、启动或清理；native末尾warm-restart不与之前的private-001拼接为连续重启首改。

## 用户构建盘量（MiB）

| 窗口/规模 | main区块边界峰值中位数 | candidate区块边界峰值中位数 | Rust区块边界峰值中位数 | candidate八私改增长 | Rust八私改增长 |
|---|---:|---:|---:|---:|---:|
| retain/2500 | 10.195 | 9.047 | 30.992 | — | — |
| retain/5105 | — | 34.867 | 58.691 | — | — |
| native/1000 | 23.418 | 7.566 | 16.564 | 0.959 | 5.125 |
| native/2500 | 28.832 | 10.451 | 36.670 | 0.984 | 11.527 |
| native/5105 | — | 38.547 | 72.855 | 3.209 | 28.480 |
| check-5000-wide/5000 | 45.312 | 15.078 | 58.348 | 0.873 | 15.141 |

盘量为显式用户semantic/object caches、程序及诊断IR与Rust target含输出，按lstat/st_blocks做请求边界快照并按inode去重；不含SDK、宿主构建target、源码与研究证据。不是瞬时峰值或等额缓存配额。归档副本/宿主debug清理不计产品节省。

## 仍落后Rust的最大请求差额

正数是candidate等待更长；按绝对秒数排序，不用倍率掩盖痛点权重。以下保留每个规模/窗口的最大四项，另以八私改/真实重启首改合计评价反复等待。

| 窗口/规模 | 请求 | candidate − Rust（秒） |
|---|---|---:|
| retain/2500 | warm-restart | 1.033 |
| retain/2500 | cold | 0.020 |
| retain/5105 | warm-restart | 4.387 |
| retain/5105 | error | 0.027 |
| retain/5105 | unchanged | 0.000 |
| native/1000 | warm-restart | 0.864 |
| native/1000 | private-003 | 0.186 |
| native/1000 | unchanged | 0.044 |
| native/2500 | warm-restart | 1.072 |
| native/2500 | broad | 0.169 |
| native/2500 | private-003 | 0.061 |
| native/2500 | private-001 | 0.058 |
| native/5105 | warm-restart | 4.157 |
| native/5105 | private-003 | 0.205 |
| native/5105 | private-004 | 0.200 |
| native/5105 | private-005 | 0.183 |
| check-5000-wide/5000 | warm-restart | 1.134 |
| check-5000-wide/5000 | restore | 0.209 |
| check-5000-wide/5000 | broad | 0.165 |
| check-5000-wide/5000 | positions | 0.065 |

阶段分账与真实命中/重建计数在 `stages.json`、`results.json` 以及原response中。阶段父/子视图不能相加；ZIL阶段成本不是Rust内部阶段证据，也不是全部可消除时间。缺失字段不按0推断。

## 验收和失败保全

实际新候选：backend314、native publication2、strict/fmt/release通过；Python质量核验22通过。IR87/Types111/driver225（224+定点1）/Source9及未改Expand/CT收据按精确源码差异复用，详见current-acceptance.json及raw gate-reuse ledger，不冒称全部重新在404a执行。Astra与Peirce对非本人部分交叉独审，后续布局/质量检查整链接线由Astra按404a精确冻结复核。

保留62d Types首败、7b08机械首败、后续旧调度断言及strict夹具首败。da06原生首cold compiler Success，但旧quality仅接受zil.fn.*，两窗口各3原始行后拒绝；原结果不算性能通过。随后整链补齐完整源函数体相等/纯DAG/实际ABI的helper校验、完整imports/DCE集合、真实layout成功事务发布。404a的02原生首cold又由末尾aggregate旧假设拒绝，后继ea5349仅修工具精确集合分账；两个真实三arm冷编译完整quality离线重放通过，compiler仍404a。未重建编译器、未重测已完成check02。da06独立check102请求也保留，数字仅属于da06。404a native03在私改005遇到两份历史layout均匹配物理对象，仍拒绝；之后一次贯通actual layout key→分区/四槽/终端→报告→inventory/retained/ELF/汇总，并补真实Source十epoch回归。85,083file完整初始raw已fresh验证并随305175归档，不能转记为本候选性能。

## 覆盖边界

native含1000/2500/5105函数单crate等价工程、连续八私改、位置/接口/广改、错误/恢复/还原、末尾重启；retain含2500/5105真实缓存重启和首改。check含5000wide。旧2500chain/diamond及clear窗口仍只属于4a23，本版未重测。尚不代表工业多crate依赖图、proc-macro/build.rs或分钟级/GB级项目，全面超Rust和main采纳门保持开放；没有main合入。完整raw/构建依赖恢复入口见RESTORE.json。

## 当前结论与残余成本

native1k、native5.1k完整18请求流程两个区块均领先Rust；native2.5k只在中位数领先0.159s，两个区块一输一赢，应视为近持平。check5k完整流程与cold两个区块均领先，但仅两区块且有明显漂移；不把旧4a/404a跨窗差额当作算法净收益。

主要重复等待仍未解决：native5.1k八次私改12.129/Rust11.808s，两个区块均落后；retain5.1k真实连续重启＋首改5.807/Rust1.438s，额外等待4.368s。首改单格中位仅领先18.9ms且符号跨区块变化，不能替代真实连续过程。两区块的16次native私改均15objects hit/1miss、1个lower/1个class input重建；因此局部对象失效已修复，但不意味着整条请求免于全局工作。

retain5.1k restart：codegen父项约1.941s（内含machine1.009s、partition0.765s、diagnostic0.090s、object0.015s）；Expand约0.391s、Types0.169s、Analyze0.115s、CT0.086s、Verify0.078s，另有报告内未细分1.623s。两次均4个Builder/5101 CFG真实恢复、16objects全命中；CT/Analyze仍各5105当前查询，机器/变换/分区全体复用为false。未细分部分不能直接归因于wire、编码或I/O。native八私改backend合计9.552s，其中object4.494s、machine2.928s、partition1.608s；这些是candidate自身成本，不能把落后Rust的0.321s直接全归给某个阶段。

盘量38.5469/Rust72.8555MiB是两区块“边界峰值”的中位数；candidate两区块各41.0977/35.9961MiB，全窗观测到的边界最大为41.0977MiB。八私改增长candidate3.2090/Rust28.4805MiB。保持显式用户构建产物范围，既非瞬时峰值也非等额cache限额，不能外推工业GB级workspace。完整逐块独审见stable-class-305-results-audit-04.md。
