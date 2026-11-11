# 当前资源候选：独审与有限采用意见

## 独立采用意见

**支持有限采用：仅显式 `--incremental-o2 --function-merging after` 的资源辅助代码分区；当前审计未发现需要继续修改生产实现的新语义或生成程序回退阻断。完整性能验收仍未全过，不能宣称原八项整体提速、Rust90 全部达成或编译专项父任务完成。**

该结论针对候选 `f38ed14b2c18c587f01a117f3c8a6d9cc67c4e7b`（本地 `d0b2eeeb466f8e7d5d4edca5c59265727da78254`，tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`），基线 `4ec5e78cfe1f2d0188ddc2502e6b15358f062b71`。原始记录、失败、频率、输入与门槛均未改动。本意见不把计时 false 改成 true，也不代替 root 对精确交付 SHA 的源码回归/CI 和已经冻结的一次 off 补充确认的最终裁决。

采用理由不是只选有利格子：新增四类 multi 是既有冻结协议，完整四类、双快照、双 depth 都保留；原八项及其 quality/neighbors 仍全部计入报告。它提供的是多模块编辑的独立收益，而非原八项的新加权总体收益。

## 未激活路径为何可以认定保持

精确 diff 中，唯一生产调用先计算 `CallImportPolicy::for_incremental_o2`：非增量 O2 为 Disabled，只有 AfterOptimization 才为 ResourceHelpers，其余为 Complete；只有 ResourceHelpers 才进入新 recipe，Complete 继续以原布尔 true 调用既有完整闭包规划。布局缓存键绑定策略，不能跨策略误用。新 recipe 对单源文件提前返回；这是策略/结构条件，不读取样例名称或运行输入。

因此可确认本次改动保持默认 off/before 路径的原分区算法，并结合有限实测确认这些保存程序的行为与生成代码保持；不能外推为无限程序的形式等价证明，也不能把机器等价当作编译耗时不退的证明。当前 off 的 8 对完整 ELF 一致；after 原八项保存的 16 对、quality 10 对、neighbors 10 对也逐字相同。primary 的 576 个原比较代码段全相同。

quality 的 owned64/base/depth0 从 base 98.888201 ms、candidate 105.968120 ms、Rust 90.820460 ms 得到一次新 Rust90 数值穿越，原 false 必须保留。但 base/candidate 完整 ELF 同为 `386904fd44b8259ba82ae991653127f46263d45cca07ebf96f8334d7d2146716`；相同输入执行同一程序的这次差异，不是本次候选生成了更慢代码的证据。将它归为“机器等价、测量门观察失败”是归因判断，不是修改门值。旧 Rust90 差距仍是未解决产品问题。

## 原始数据审计

`resource-current-audit.json` 从全部原始行重算：complete=true、errors=0；完整 ELF 逐字比较另存 `resource-current-code-identity.json`。quality/neighbors 各 30 个原始构建、各 20 个 ZIL 收据均独核 Success/Verify=1。实际二进制与独立 receipt/primary/multi manifest 哈希一致；旧脚本硬编码的 baseline 标签保留，实际比较身份由独立 receipt 明示。

| lane | 构建 | 运行/观察 | 新 >10% 回退 | 新 Rust90 穿越 | 既存 Rust90 失败 | 当前数值状态 |
|---|---:|---:|---:|---:|---:|---|
| primary | 1800 | 216 runtime；3672 短输出 | 0 | 0 | 0 | 运行门通过 |
| multi | 720 | 384 runtime；12960 短输出 | 0 | 0 | 0 | 固定 16 格通过 |
| quality | 30 | 900 runtime | 0 | 1 | 4 | **false 保留** |
| neighbors | 30 | 480 runtime | 0 | 0 | 10 | **false 保留** |
| controls | 48 | 864 观察 / 432 对 | — | — | — | 完整通过 |
| multi-controls | 32 | 640 观察 / 320 对 | — | — | — | 完整通过 |

State/Store 另从业务 oracle、原 stdout/stderr、命令及分配追踪核对：16 构建、128 观察、56 次实际分配失败，live=0，allocated=frees，完整输出/退出码一致。8 个 O2 构建实际 pipeline 均 `default<O2>|merge-functions=after|aliases=false`，当前 Verify=1；probe 链接真实 API 对象，ordinary/probe 输入 IR 相同，未以 Clang 重编 IR 代替。独审记录为 `resource-state-current-audit.json`，errors=[]。

## 收益与代价

以下仍使用原固定频率；正数表示耗时减少。multi 是单独四类、O2-only 分布，不加入原八项分母。

| 固定分布 | base wall ms/请求 | candidate wall ms/请求 | 等待减少 | CPU 变化 |
|---|---:|---:|---:|---:|
| primary/interactive/edit | 308.894030 | 312.558525 | -1.1863% | +1.3079% |
| primary/interactive/clean | 437.338329 | 435.442135 | +0.4336% | -0.2550% |
| primary/release/edit | 559.575162 | 569.220553 | -1.7237% | +1.8881% |
| primary/release/clean | 827.466929 | 838.123799 | -1.2879% | +1.5310% |
| multi/O2-only/edit | 242.786492 | 180.181832 | +25.7859% | -20.2289% |
| multi/O2-only/clean | 306.921670 | 282.793118 | +7.8615% | +11.6299% |

原八项三负一正，损失分别约 3.66、9.65、10.66 ms/请求，唯一正收益约 1.90 ms；不能写成全局突破，也不能因生成程序相同而抹掉编译器自身的耗时变化。单格最坏 trait128/O0/public 增加 62.97% / 59.15 ms，这些原始编译观察也保留。

新增 multi/edit 等待减少 62.60 ms/请求（25.79%），CPU 同时降低；主要来自 pipeline8/private 255.20→95.29 ms、pipeline16/private 383.18→157.27 ms。dispatch 仍保守合组，其保存的 4 对 ELF 相同；pipeline 的 4 对 ELF 不同，对应 8 个实际新代码运行格。完整固定 16 格均通过原门，不能只报告 pipeline 的编译收益而漏掉 dispatch。

四类等权纯冷请求：wall 329.328612→329.070638 ms，几乎不变；总 CPU 324.411050→400.948850 ms，增加 23.5928%。具体如下：

| 冷 O2 场景 | base CPU ms | candidate CPU ms | CPU 增加 |
|---|---:|---:|---:|
| dispatch8 | 224.229200 | 233.251800 | +4.0238% |
| pipeline8 | 210.870200 | 293.058000 | +38.9755% |
| dispatch16 | 459.082000 | 480.685200 | +4.7057% |
| pipeline16 | 403.462800 | 596.800400 | +47.9196% |

这说明部分等待收益来自并行及后续对象复用，而冷总工作量增加。multi/clean 加权等待虽减 7.86%，CPU 却增约 11.63%；pipeline16/edit-warm 还从 60.43→84.98 ms（+40.63%），不能承诺无修改请求都更快。有限采用适合明确选择 after、重视多模块真实编辑等待的用途；缺少证据把它作为冷构建吞吐、低核数机器或整仓通用最优策略。

## 运行边界与最终裁决应如何写

multi 最坏 pipeline8/base/depth0：7.073043→7.775862 ms，精确 +9.9365859928%；该对 ELF 不同。距 10% 门仅 0.0634140072 个百分点、0.0044853 ms。按原始数值它通过，不能临时把门降成 9% 后判失败；但裕量很薄，不能声称具有稳健的零退化余量，也不能把本机结果推广到其他目标 CPU/LLVM。

建议采用措辞：“接受显式增量 O2 + after 的有限资源辅助分区；四类既定 multi 协议显示编辑净收益且全部运行格在原门内，State/Store 及身份/清理控制成立。默认/未激活路径继续原规划，在已测保存程序上机器等价。原八项未证明整体编译收益；quality 新 Rust90 数值穿越 1、既存质量/邻居失败、原 off false 与冷 CPU 代价均保留。整体 Rust90 与编译专项父门未关闭。”

由精确 guard 与完整机器等价消解“该新 Rust90 穿越是本候选新增代码退化”的归因，有足够依据；据此另作有限采用，也有独立的目标收益依据。若把本次合入描述成“所有原验收布尔门都已通过”，则与事实冲突，当前证据不支持。off 的一次 A/A 补充已另行预注册，仍等主重型会话释放；本审计未执行它或任何新测试。
