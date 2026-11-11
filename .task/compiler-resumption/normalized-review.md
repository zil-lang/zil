# 规范化函数合并独立复核

复核者：本轮非作者 `review_normalized_merging`。日期：2026-11-11（Asia/Shanghai）。本记录只复核既有冻结源码和原始证据，未将任何旧成绩算作本轮新组合验收；本轮适配后的精确 SHA、CI 和实测由集成者补充。

## 裁决

独立规范化方案有继续施工和有限采用价值，建议作为现有显式 `--function-merging before-and-after` 的实现候选验收；不默认开启，不宣称一般程序都会变快。当前 PR84 的无约束组合不可接受：其包含 PR80 的资源 helper 分区，组合产生了四个新增超过 10% 的运行退化格。先建立按显式优化策略划分的兼容边界，再对精确组合复核。

后续 off 补测又发现真实运行越门，因此本文最初 off/after 都采用 helper 分区的建议已被收紧：**只有显式 after 采用新 helper 分区，默认 off 和 before-and-after 都保留旧完整调用闭包规划**。后文保留最初静审及其源码身份，末节记录新反例和修订，不把后续失败改写为原方案已通过。

这里的“递归样例慢 28.2%”指 **编译私有编辑的延迟**，不是生成程序的运行退化。两者必须分别呈现。

## 源码和证据身份

- PR84 原 head：`e19e35a9177b491b595f085fc77836a963856382`，8 个变动文件包含 PR80 的 4 文件与规范化自身 4 文件。
- 独立规范化受测源码：`5172f85373345c90d39aea8b8c51fa5e7ad2f632`；保全 head：`4f29707db7f73689f377624024b9ff991c94a164`；完整实验：<https://github.com/zil-lang/zil/actions/runs/42934564178>。
- 实际对照基线由 workflow 和 receipt 固定为 `2fbf51efff4ff21f7566aeb5e6e0e105ba8cc852`。继承测量器的 `primary/manifest.json` 仍写旧 `099d0af9` 字面量，不能将该旧标签当实际 checkout；本轮测量应把真实 SHA 作为参数记录。
- 独立源码没有 PR80，也没有静态 Drop 方案。规范化四文件：`crates/zil-llvm-api/src/lib.rs`、`crates/zil-llvm-api/src/native.rs`、`crates/zil-cli/tests/function_merging_research.rs`、`docs/guide/function-merging.md`。
- `lib.rs` blob `df19630237c945a04cff90e9bcea05ced09acaf7`；`native.rs` blob `113d27b888396f6235a2062506fa0a29f378093d`；CLI 测试 blob `0c4413fbf09b0214bf076f11b81f806a0d47b65b`；说明 blob `60bc3f11854d8fef13e5b44e0c379980ff58ace6`。PR84 的规范化四文件与此相同。
- 不合格组合实验：<https://github.com/zil-lang/zil/actions/runs/42935614289>；受测组合源码 `a15cc71988eeabc6d63b92232c1817b1ee098b92` 已与原 receipt 实读核对；组合保全 head `f9dd0bcd3313f6a04ad8883caecd7996f51cd737`。

完整数据位置：研究 head 下 `.task/normalized-merging/runs/42934564178/` 的 `primary/{rows.jsonl,cells.json,runtime.jsonl,summary.json}`、`multi/summary.json`、`quality/summary.json`、`neighbors/summary.json`、`receipt.json` 和 `raw.tar.gz`。组合数据位置：`.task/composition/runs/42935614289/`。

## 源码复核

固定前缀顺序是 SROA/early-CSE，IPSCCP/globalopt，mem2reg/有限 InstCombine/SimplifyCFG，早期 CGSCC inline，再 mergefunc，最后完整目标感知 `default<O2>` 和既有末尾函数合并。`off`、`after`、裸 CLI 默认及 Clang 对 before-and-after 的显式拒绝保持原行为。

完整管线进入 `pipeline_identity`，原对象不能误命中新策略；原生收据与对象缓存使用同一个身份函数。当前独立 ZIL Verify、结构化机器预检、LLVM 优化前后验证都保留。没有新 `noinline`、`cold`、`noalias`、可观察地址别名化或缓存授权豁免。现有外部 C 地址观察测试覆盖具名导出与不同地址；本轮仍须在适配后的实际候选执行必要控制。

独立实验的常规 native consumer 脚本部分使用默认 O2，只能证明默认路径回归。需要额外使用 `.task/normalized-consumers/state_store.py` 这类明确传入 before-and-after 且核对实际收据的脚本，证明新路径上的 State/Store 清理；原后续消费者实验和组合实验已经做过这类控制，但不能替代本轮精确组合核验。

## 原协议量化

原 8 样本：图 45%、trait 25%、资源 30%；各组内均分，模式与请求频率在实验前固定，频率是假设而非遥测。全部 Check/O0/O2、冷/无修改/私有编辑/编辑后无修改/公共编辑/恢复历史保持；1,800 次编译请求、1,500 次正式请求、3,672 次机器输出核对、216 次计时运行。

| 原 8 样本频率 | 基线平均 ms | 候选平均 ms | 节省 ms/请求 | 节省比例 |
| --- | ---: | ---: | ---: | ---: |
| interactive/edit | 192.5305 | 183.6718 | 8.8587 | 4.6012% |
| interactive/clean | 286.0996 | 281.4395 | 4.6601 | 1.6288% |
| release/edit | 338.9322 | 308.9463 | 29.9859 | 8.8472% |
| release/clean | 562.0903 | 547.2210 | 14.8693 | 2.6454% |

按计划事前定义的扩展 12 项描述性频率：仍为图 45%、trait 25%、资源 30%，将资源组均分给原 2 项和新增 4 多模块项，得到如下结果；不得用它覆盖原 8 项结果。

| 扩展 12 项频率 | 基线平均 ms | 候选平均 ms | 节省 ms/请求 | 节省比例 |
| --- | ---: | ---: | ---: | ---: |
| interactive/edit | 196.6557 | 186.6277 | 10.0280 | 5.0993% |
| interactive/clean | 290.8588 | 284.4805 | 6.3783 | 2.1929% |
| release/edit | 342.4368 | 308.3308 | 34.1060 | 9.9598% |
| release/clean | 565.6289 | 544.6086 | 21.0203 | 3.7163% |

原 8 项按场景频率加权的 O2 冷构建 CPU：1396.938 → 1375.253 ms，约减少 1.55%。所有正式请求最大 RSS 观测为基线 577128 KiB、候选 577020 KiB；这只是该有限矩阵观测，不能说明任意程序内存有硬上限。

### 有意义的绝对收益与退化

- `shared4096` 私有编辑 O2 中位数 2580.613 → 1853.869 ms，减少约 727 ms；均值减少 28.29%。
- `owned32` 私有编辑 O2 中位数 303.666 → 257.336 ms，减少约 46 ms。
- 多模块 `dispatch8`、`dispatch16` 私有编辑 O2 均值分别减少 28.36%、37.37%；`pipeline8`、`pipeline16` 分别减少 27.76%、38.77%。
- `ring768` 冷 O2 均值 1033.589 → 1370.561 ms，增加 336.972 ms/32.60%；私有编辑 529.673 → 678.952 ms，增加 149.279 ms/28.18%。
- ring 的生成程序运行中位数反而由 378.548 → 365.017 ms，快约 3.57%。原 primary 运行最坏新退化是 owned32/base 约 7.27%，未超过原 10% 相对门。

### 运行质量门

30 格原活跃资源 runtime 的相对退化门全部通过，最坏约 8.71%；但 64 规模六格原 Rust90 门仍失败，原 `runtime_gate_pass:false` 必须保留。空/稀疏环境邻近矩阵最坏相对增加约 0.20%，其中多个 Rust90 失败同样原样保留；不能把“没有新增相对退化”表述为所有场景达到 Rust90。独立多模块 16 格全过原相对门和 Rust90。

### 权重敏感性

固定其他场景之间的相对频率，仅提高 ring 的总请求份额，原矩阵 clean 加权收益约在 ring 占比 18.19%（release）/18.95%（interactive）归零；原占比是 11.25%。edit 的归零占比约 37.74%/38.67%。这是从原格均值计算的敏感性分析，不是改变原验收频率，也不是实际用户分布。因此值得提供为显式选择，但没有证据将其作为默认的普遍优选。

## 组合退化的成因与兼容边界

组合实验的 `pipeline8/16` 低负载四格运行分别慢约 15.6%、16.3%、21.1%、15.8%，是真实新增越门。基线 pipeline8/16 因完整调用闭包要求保持 1 个单元；PR80 组合变成 8 个单元，大源函数只留外部声明、未被复制。pipeline8 的总对象字节约 9944 → 29328，pipeline16 约 16384 → 36024。资源 helper 的复制与串联源实现的跨单元边界改变了优化可见范围；这与 ring 的重复早期优化成本是两个不同问题。

同参尾分派亲和已经使 dispatch 保持 1 单元，故该样例组合没有相同退化；不能据此认为任意串联调用也能保质量。把不可见的源函数恢复给 LLVM 需要改变分区/导入边界，单纯局部 pass 顺序未必能弥补。

建议本轮最小修复：新 helper 分区仅在 off/after 策略采用，before-and-after 保留已有完整调用闭包规划；这依据用户显式选择的优化合同，既不按函数名/样例/规模特判，也不修改预算、身份和运行门。这样两个策略分别使用已经有有限证据的优化邻域。应测试真实资源模块的不同分区结果、实际 cache 身份/命中和切换后重新复用，并核对两种策略上的外部地址及清理控制。未建立此边界前不能合入原 PR84 组合。

## 下一项有价值的优化

ring 的私有编辑缓存命中均为 3/4，增加的时间不是失效范围扩大；其冷构建对象总量 401672 → 496288 字节，LLVM 优化合计约 452 → 731 ms；私有编辑的重编单元优化约 122 → 196 ms。证据指向额外早期内联/简化造成代码增长及重复优化，具体 pass 占比仍需测量。

后续可筛选“前缀只暴露便于等价比较的小实现，完整 O2 保留默认内联政策”，但不能把它当作本轮已经完成的小修复。LLVM23 库有 `LLVMPassBuilderOptionsSetInlinerThreshold` 导出；孤立 `cgscc(inline)` 的 advisor 可能直接使用默认 InlineParams，因此简单设置该选项未必影响早期 sweep，必须用实际 IR/remarks 证明参数生效。如果拆成两次 LLVMRunPasses，需要计入分析缓存丢失的成本，并将两阶段、阈值和末尾默认策略全部纳入对象缓存身份。

初筛可保留原 ring、shared4096、owned32、相同/不同 handler 与串联负载，固定原低负载和深负载运行门。不要直接新增“递归就换算法”的未经证明策略，不加永久 noinline，不降低完整 O2 的运行质量。本轮先完成可验收的两种显式策略；下一项以实际 pass 计时和代码增长原因决定，而不是继续盲扫管线。

## 本轮兼容边界与承接复核

资源分区作者冻结源码 `29f47a3a17024d2929e850c9fc003c3dd1c779a5`，tree `b604914c289753200a7e20c0899ca4907217c6db`；远端发布 `136d77b7df304349240413a8e155b0da95e68e10` 已 fetch 并核对 tree 逐字相同。非作者静审未见阻断：唯一生产调用以 `CallImportPolicy::for_incremental_o2` 选择 off/after 的 `ResourceHelpers` 与 before-and-after 的 `Complete`，O0/非 incremental O2 为 `Disabled`。Complete 跳过 helper recipe，旧完整闭包 recorder、普通分区回退和复制预算均未改变；策略枚举直接进入布局 action key，未改变任何当前验证要求。

新增资源 fixture 测试检查真实计划：off/after 为 3 单元，before-and-after 与旧完整闭包的 2 单元计划完全相同，并验证各机器模块。另一个测试实际发布布局缓存，检查策略不同不能误命中、切回原 key 仍命中。这些是源码及测试设计复核，不冒充本轮测试执行或生成程序的运行验收。

已在独立 normalized 工作树承接远端资源候选，形成本地合并提交 `51abb96f5bbed9f89824d5f914aa49d4c0bc6dea`，tree `d1273ceea8427e21c315002eb68c20ce3c2080d9`，父提交为原 PR84 `e19e35a9177b491b595f085fc77836a963856382` 与资源发布 `136d77b7df304349240413a8e155b0da95e68e10`。相对资源发布仅保留原规范化 4 文件；其中两个实现文件和 CLI 测试与原 PR84 blob 完全相同，仅补充 function-merging 指南的策略选择、独立衡量收益、递归编译代价。指南将没有额外低阈值保证的“轻量内联”改称“早期内联”，没有修改实际 pipeline。

本地 `cargo fmt --all -- --check`、合并索引与工作树 `git diff --check` 通过，工作树清洁。没有启动 Cargo 构建、测试或 benchmark；本轮构建、明确 before-and-after 消费者及原运行质量门由根集成者对实际发布 tree 继续执行。此静审接受不替代上述实际验收。

## Off 新反例与 OnlyAfter 修订

根集成者对资源发布 `136d77b7df304349240413a8e155b0da95e68e10` 补跑关闭函数合并的有限质量矩阵，数据保留于 `/workspace/scratch/beabdcf5ebf4/evidence/resource-off/`。实读测量脚本、命令与对象收据，确认实际使用 `--function-merging off` 和 `default<O2>`，没有将 after 成绩误标。24 次构建、432 次短输出检查、384 次计时运行（336 正式）完成；16 格中 pipeline16/base/depth0 的候选中位 15.507934 ms、基线 13.295978 ms，增加 16.6363%，违反原 10% 门。Rust90 全部通过不能抵消该失败；原失败保持 false。

资源作者据此将候选收紧为 `d0b2eeeb466f8e7d5d4edca5c59265727da78254`、tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`。精确 diff 仅 4 文件各 10 行增删：启用 incremental O2 时仅 `AfterOptimization` 选择 `ResourceHelpers`；函数合并 off 和 before-and-after 都选择 `Complete`，未启用 incremental O2 时仍是 `Disabled`。真实分区测试相应要求 off/before 与原 2 单元计划完全相同，after 为 3 单元；缓存身份、预算、回退与验证未改。非作者静审未见阻断，但不代替修订候选的实际验收。

资源远端发布为 `f38ed14b2c18c587f01a117f3c8a6d9cc67c4e7b`，已 fetch 核对上述 tree 相同。normalized 已合并该远端提交并同步指南，最终本地提交 `4cc1226bd7e652201cfb37d992f4bb13988db42a`，tree `4f60d438f30724ed979f62fe3d08904faee07b8b`，父为上一轮 `51abb96f5bbed9f89824d5f914aa49d4c0bc6dea` 与资源远端 `f38ed14b2c18c587f01a117f3c8a6d9cc67c4e7b`。相对新资源发布仍仅原规范化四文件；规范化的两个实现及 CLI 测试与原 `e19e35a9` 完全无差异，指南明确 OnlyAfter 边界。再次执行 `cargo fmt --all -- --check` 与 `git diff --check` 通过，工作树清洁，未运行任何构建、测试或 benchmark。原 d127 tree 不作为最终候选；原 off 失败证据不删改。
