# 当前接续：当前4a23正式测量与阶段定位

唯一施工分支research/full-push-native；当前被测源码 **4a23edc7d06bc18cbed1fb958fdaacd886d9607a**，tree35116da8321766cf49106143bd699faada166dbc，release SHA256 `302d58db48f53cd4bc1952f2a9032ead5a6d479314d24eb2ca344f558c2c113e`。测量工具冻结8bb24b854，实际main9170bb421未合。SDK18/native-link沿固定[V32](https://github.com/zil-lang/zil/blob/1eecf9b81a4929dde79e4fa98cd0ee40fb74af75/research/incremental-demand-full-chain-20261113-v32-recovery/RESTORE.json)，未重建LLVM；独立Verify、默认预算、完整O2保持，研究开关默认关。

本轮按用户要求只测试和定位，没有修改编译器。六个窗口全部完成：774原始请求、258功能格、71持续运行质量用例，候选/Rust既定质量门过。另有213条持续运行构建不混入原始请求；独立profile诊断36+36也不混入正式速度比。O2/CGU16/编译jobs1、两个平衡区块；check与原生分开。ZIL请求计时不含另记worker启动，Cargo含启动；没有OS冷缓存清空。

**当前没有全面超越Rust。** 原生18请求总等待1k/2.5k/5.1k为7.166/10.082/29.477s，对照同窗Rust8.047/10.017/31.582s；2.5k近持平，不能把完整流程优势掩盖局部损失。独立check2500 chain/diamond、5000wide为4.175/3.034/7.574s，对照Rust4.755/5.261/7.050s。check5k还比实际main6.671s慢13.54%，check2500chain比main3.811s慢9.55%。main5.1k原始Expand限额不能算快速成功；native窗口accepted仍false，main Hold。

最大损失是保留缓存重启：5.1k重启3.197/Rust0.013s，随后首改1.245/0.995s，联合4.441/1.008s，多等3.433s；长流程末重启3.562/0.043s。重启5101个恢复CFG命中，但仍有真实当前key/CFG绑定和上游/Analyze/machine补建；16个对象全命中，主要差距不在LLVM对象miss。保留窗Codegen1.547s内machine.962/partition.388/object.019s，未细分请求内.747s。父子桶不能相加，残差不等于某个API，也不保证可全删。

5.1k八次私改9.832/Rust8.688s，稳定多1.144s。每次Types/Analyze/lower均1个函数、5104复用；八次Types+Analyze仅4.33ms，Codegen8.272s占84.1%，内machine2.386/partition1.684/object3.893s；每区块124/128对象miss。实际LLVM O2 optimize2.348s、emit1.385s比Builder.041s昂贵。下一突破口为全局transform/CGU稳定与对象失效范围，不再把首改误判为Types未命中。check5k则Types/CT/Expand及请求内其他工作昂贵，没有LLVM阶段。

磁盘在本组有改善：原生1k/2.5k/5.1k请求边界物理峰值10.43/13.50/55.34MiB，Rust19.18/36.67/72.86MiB；5.1k八改增长1.58/Rust28.48MiB。含ZIL semantic/object缓存、程序/诊断IR和Rust target输出，不含SDK/宿主target/研究证据；不是瞬时峰值，不据归档恢复后的hardlink占用重算。速度与盘量必须一起报告；没有工业GB级收益或RSS结论。

完整[结果、全部落后项与分账](restart-performance-20261116/report.md)、[结构化结果](restart-performance-20261116/results.json)、[原件hash/计数](restart-performance-20261116/stages.json)及[Astra复核](restart-performance-20261116/restart-performance-results-astra-audit-01.md)。独立profile只解释读取/出版成本，Verify扰动明显；Rust Fresh会重放旧time-passes，6条已排除，不把它们当本次rustc阶段。SDK opaque native_history也不等于四槽MachineProduct。

[完整恢复入口](evidence/restart-performance-core-20261116/RESTORE.json)索引六窗完整raw/cache/artifact和单列诊断、每包空目录完整解压/hash收据、pins/脚本/日志/二进制/首败。前批正确性Types110/Driver224/Source9/publication1、strict/fmt/release通过；backend299按b197精确无差异复用，Expand124/CT76原收据仍有效，见[原验收](restart-epoch-reviews-20261116/current-acceptance.json)。旧首次失败与二进制通过固定8bb24恢复依赖，不冒称本轮重跑。

有限标量102文件单crate/O2/no-debug不代表工业workspace、泛型/宏/async/debug/test或分钟级工程。10105/25105仍受默认token/10000函数合同限制，不改预算当通过。下一按秒级绝对等待处理重启整链、全局机器变换/分区及check大图；未来逻辑改完整后统一验收。本轮不继续施工、不合main；goal/archive永不合main。旧bcf/f19跨窗退化原样保留，不能给4a23算净算法收益；98fa/59fd与021997结果各自绑定。
