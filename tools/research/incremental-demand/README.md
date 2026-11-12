# 增量编译：main 与 Rust 对照

> **当前施工线：完整 push 与 LLVM 优化/发射增量**，分支 `research/full-push-native`，当前编译器检查点 `2f9ada6193401865043386be00b115eb8db81490`，实际 main 基线 `42f0396f36b2a36860ecb55371d436e4327877a4`。以下 900 的性能结果属于上一冻结版本，不能转记为新版本收益。新版本已接入实际 Types/Analyze/独立 Verify/Lower 消费者、持久函数表、增量发布及 LLVM 优化阶段/MIR 回放；`--push-reuse` 和 `native-products` 默认关闭。当前 425 函数 driver 历史控制 3 项、LLVM 接口 50 项、Types 91 项、Analyze 294 项通过；不同运行覆盖有重叠，不累加。LLVM v6 的实际首域回放已通过小型对象质量控制，但新识别的基本块编号/世代绑定缺口正在修复，不能称通用正确性或完整 LLVM 增量化验收。新 425 原生剖析、同参三方矩阵和完整 workspace 分组门待完成。完整测试两次因链接磁盘耗尽失败，原始日志保留；不以基础设施失败冒充通过。接续记录见 [.task/full-push-native.md](../../../.task/full-push-native.md)。

> 最新冻结编译器 `900293cfd8afcd7118775e8165e1ec4f5266e3d7` 已完成真实私有编辑→CFG→Analyze→独立 Verify→机器 IR→CGU/对象→最终可执行文件的增量消费者接入。源码分支 `research/incremental-demand-continuation`，研究开关仍默认关闭，未合入 main。这里是限定域的实际全流程进展，中央 ReactiveGraph 仍只调度 BodyKey，全局工作和保守回退尚未消除。
>
> **最新六轮同参结果**：[全部八场景](full-pipeline-matched-900.md) / [全部六轮、父历史和实际单元内容](full-pipeline-matched-900.json)。CGU16/O2/jobs1 私有编辑 main1612→候选903ms，Rust582ms；无修改559→74ms，Rust14ms。检查私有编辑626→276ms，Rust282ms。原生冷编译7744→7844ms、广泛修改5846→6464ms仍负；候选私有原生仍慢于 Rust，不能宣称已追平。每种模式270请求（144测量+126具名父请求），原生2808次执行及351份blob校验通过，78组成功main/候选LLVM和ELF哈希一致。成功原生请求中各方实际均16个当前单元，但内容/复用逐单元核对，未假设分区等价。Rust新Cargo与ZIL resident、LLVM23.1.1/23.1.4及检查保证差别明确保留。
>
> **实际减少的工作**：完整425函数样例中，私有编辑BodyKey1重算/420复用，机器函数1降级/424复用，15分区复用/1重建，独立Verify当前证明1检查/422复用；当前全局检查仍执行。计划持久化425行零重编码，新载荷15214B/一个15314B物理包；无修改在完整机器IR构造前复用，仍做425当前输入比较。导入位置变化BodyKey0/421，同时22模块使用当前Lower/物化，未拿旧span当新结果。见 [r7逐阶段实测](full-pipeline-profile-900.md)、[完整审计](full-pipeline-audit.md) 和 [独立剩余成本诊断](full-pipeline-residual-900.md)。
>
> **正确性与证据**：[最终验证](full-pipeline-final-validation.json) 为6253项通过、0失败/忽略（416测试程序+14文档测试套件），严格Clippy/fmt通过，精确源码非作者复核保留。最终900原始证据已保存到 [固定v4归档](https://github.com/zil-lang/zil/tree/32e1b04fb859440bb36ec3223cb35b26edf63041/research/incremental-demand-full-pipeline-20261112-v4)，从全新HTTPS克隆恢复核验24642文件/2.60GB并重新通过全部profile/matrix校验，见 [v4恢复收据](full-pipeline-recovery-v4.json) 和 [独立证据复核](full-pipeline-final-evidence-review.json)；此前v3固定archive353a6754b已从新HTTPS克隆核验906文件/298MB、r5/r6各180oracle，见 [v3收据](full-pipeline-recovery-v3.json)。v2固定7fd718bc保留旧140正式矩阵及51872文件，见 [v2收据](full-pipeline-recovery-v2.json)。旧环境最初丢失raw仍不可恢复，不能用本地tar冒充持久备份。

历史版本、撤回的跨CGU比较及原始负结果继续保留下文；最新源码结果以上述900收据为准。

> 2026-11-12 后续已实施函数Wire发布切片，并完成同窗口、同请求CGU16/O2/jobs1的main/候选/Rust检查及原生矩阵。新结果、负结果、逐阶段审计、独审和远端raw恢复见 [本轮交付](continuation-results.md)。下文历史数据与新结果分开保存。

> 更正（2026-11-12）：上一版把 Zil CGU16 与 Rust CGU256 放入同一张 O2 表，并给出 2.36/1.43 倍差距，控制变量不一致。这两个跨语言 O2 倍数撤回；相同 Zil 配置下 main→候选的结果保留。下面保留历史记录，新的同 CGU、同时间窗口对照和点对点路径审计另行记录。CGU 参数是请求值，也必须核对实际产物分区，不能从参数直接推断实际单元数。

> 恢复边界：本次环境回到初始快照，上一轮本地 raw/归档未恢复；远端源码、results.json 内的逐轮观测和独审收据已恢复。下述历史原始文件路径不是当前可访问性保证。

本研究已实现未变模块的共享表示和标量物化结果复用。局部编辑后的检查耗时约减半；普通多单元 O2 也获得有限配置下的收益，尚不能宣称整体加速或追平 Rust。研究开关仍默认关闭，未合入 main。

源码分支 `research/incremental-demand-propagation`；测量源码冻结在 `0601b9786d0fe683b4321f0d086230e5be027602`，实际 main 基线为 `99035931d358add0eaed6d4535035c38413701dd`。后续提交仅保存工具、协议与报告。main 已有 resident/function/analysis/semantic/object 缓存；这里比较整个研究分支与 main 的差异，不能把全部差值归因于本次新增的两项改动。没有用另一个施工方的分支作性能对照。

## 可重复的收益与剩余差距

固定生成项目有 22 个文件、425 个具名声明、6400 个算术分支步骤，Zil 源码 498620 字节。所有函数接收运行时输入并贡献到输出。每组六个 block；每个编辑从经过验证的具名父版本开始，保留全部准备请求。Zil 两端交替执行，使用相同缓存、默认预算、O2、单 codegen job。候选 B 仅增加 `--producer-reuse`，图开关关闭。详细值、六个观测和置信区间见 [results.json](results.json)。

下面是请求墙钟中位数（毫秒）。Rust 在独立时间窗口运行，包含 Cargo 启动；Zil 是常驻进程请求，不包含启动。Rust 类型/借用检查与 Zil 独立 Verify 的保证不同，跨语言比值仅描述这套工作流的延迟。

| 检查请求 | main | 新分支 B | Rust cargo check |
|---|---:|---:|---:|
| 冷请求 | 1412.3 | 1067.1 | 1495.8 |
| 无修改 | 141.4 | 174.9 | 17.9 |
| 单处私有编辑 | 1130.1 | 511.9 | 279.6 |
| 三处编辑 | 1173.4 | 539.3 | 282.3 |
| 广泛编辑 | 1034.0 | 1614.5 | 1366.5 |
| 恢复原版本 | 169.8 | 181.6 | 242.9 |
| 错误请求 | 654.0 | 574.9 | 246.7 |
| 错误后恢复 | 1043.7 | 647.5 | 261.0 |

私有、三处编辑均为 6/6 对改善，配对几何耗时比为 0.516、0.511；配对平均节省时间的探索性 95% bootstrap 区间分别为 [266,700]、[455,650] ms。它们相对 Rust 的中位数差距仍约 1.83、1.91 倍。冷启动、无修改、恢复原版本未证收益，广泛编辑中位数更差。六个 block 不支持稳定 p95 或语言整体性能结论。

主测原生配置为 O2 / CGU1 / jobs1。私有编辑中位数 main 10566.9 → B 8733.9 ms，但平均节省区间跨零；三处编辑 8305.0 → 10833.9 ms，平均节省区间 [-4313,-2] ms 为负。该配置不能宣称整体原生提速。78 对成功请求的 LLVM 文本及 ELF 字节哈希相同，936 次原生执行通过完整输入/输出/退出码检查。

Rust 正常增量分区参照使用 opt-level=2、incremental=true、CGU256、jobs1、lto=false；私有编辑 666.1 ms、三处编辑 1425.2 ms。这不是 Rust release 默认 incremental=false，也不是与 Zil CGU1 相同的分区。两端后端源码和优化策略不同，不能将差距全部归因于前端。

普通 CGU16 的六轮补充结果如下；两端 Zil 都使用原有的普通多单元 O2，关闭 `--incremental-o2`。实际均为 16 单元，私有编辑命中 15/16 个对象，三处编辑命中 14/16 个对象。

| 历史 O2 请求 | main CGU16 | 新分支 B CGU16 |
|---|---:|---:|
| 冷请求 | 9527.8 | 8472.3 |
| 无修改 | 785.6 | 757.2 |
| 单处私有编辑 | 2315.9 | 1574.2 |
| 三处编辑 | 2786.4 | 2040.1 |
| 广泛编辑 | 9616.2 | 7925.2 |
| 恢复原版本 | 902.5 | 913.1 |
| 错误请求 | 500.5 | 408.3 |
| 错误后恢复 | 2963.6 | 1944.7 |

同配置 main→B 的私有/三处编辑配对几何耗时比为 0.722/0.773，平均节省时间区间为 [283,1290]/[273,1095] ms。原先基于 Rust CGU256 得出的跨语言 O2 倍数已撤回。冷请求、无修改和恢复原版本未证收益。这一补充不覆盖 CGU1 主测的负结果；将 CGU1 改为 CGU16 的收益属于既有后端配置，不能算作本次新实现的收益。完整补充有 180 请求、936 次原生执行。

两个昂贵配置的未完成实验也保留：Rust CGU1 冷 O2 为 282786 ms 后停止；Zil 现有 `--incremental-o2` / CGU16 中私有编辑虽命中 14/16 个对象缓存，单个未命中单元的机器码生成仍超过 100 秒。后者仅完成 12 行、72 次原生执行；不能作为六轮性能验收。普通 CGU16 与这项额外导入策略分开报告。

## 实际消除的工作

- `ProgramModule.core`、展开结果和源码映射保留 `Arc`。生产者组装共享未变模块，修改处显式 copy-on-write，拥有式接口按需取回所有权。序列化格式及版本不变；测试检查指针身份、完整 TypedProgram、修改隔离。
- 标量物化成功收据绑定封闭的解析结果、解析器修订、深度、路径容量及初始临时内存状态。当前环境、捕获分类与可达性准入仍执行；命中后重放工作量和临时内存账本。预算或适用域不匹配时走原来的有序遍历，保持首个错误。泛型、资源和更丰富的 CT 路径继续回退。
- 修复冷版本 → 私有编辑 → 返回冷版本 → 多点编辑的解析历史身份问题。收据只从已经匹配 SourceUnit 的封闭解析输入取 AST，不依赖旧的相等 ParseResult 指针。
- 修复研究基线相对 main 引入的默认线程栈回归：私有大状态与报告改为堆上存放，拆分 checkpoint/工作帧。没有提高线程栈、递归深度或预算，没有减少原测试断言。

独立 profile 的工作门通过：单处编辑实际物化/Lower 1 个模块、复用 21 个；三处编辑执行 3 个、复用 19 个；映射条目复制为 0；Core 深比较为 0；CFG 执行仍为 5/6，没有增加。所有成功请求仍执行当前独立 Verify。

A/B/C 使用同一个候选二进制作内部归因。B 的收益没有转记给图 C；B→C 的私有和三处编辑区间跨零，广泛编辑更慢，图净收益仍未成立。小项目没有普遍收益；资源闭包用例仅证明回退正确。关闭新开关的候选 A 也单独对照 main，因为共享表示会影响关闭开关的路径。

一次 B 私有编辑 profile 共 511.7 ms，其中物化 27.1、组装 1.3、实际生产者执行 3.4；Analyze 52.5、CT 34.3、Verify 84.5、函数缓存发布 155.3、语义缓存发布 70.5 ms。下一步最明确的剩余工作是缓存发布和全局检查；不能把这一次 profile 当成稳定性能区间。当前实现没有获得 Rust 那样持久化的 query red-green / CGU 复用体系。

50 个唯一编辑、无磁盘缓存/无重置、容量 32 的 B/C 控制中，末端 RSS 分别约 383.0/384.9 MiB，后半程增长 4/0 KiB，图节点/边在源码推导界内。这是有限历史控制，不是字节上界或无泄漏证明，也不是 main/Rust 内存对比。

## 验证与限制

冻结源码 0601 的 workspace lib 1654 项、LLVM API 泛型回归 214 项均通过，严格 workspace/all-targets Clippy、fmt 和 release 构建通过，非作者独立源码复核通过。先前完整 workspace 运行 6119 通过、1 失败；唯一失败是默认线程栈回归，已在 main 证明通过，并在本分支修复后以原测试及 LLVM API 泛型全集验证。修复后没有再声称完整 workspace 全部重跑。

主矩阵独审覆盖 933 个常驻请求、1236 次原生执行及额外 5 次 fresh 检查；Rust 两个完整矩阵共 180 请求、936 次原生执行及 24 个精确 E0425 错误请求。补充矩阵计数独立列出。当时保留了原始 Source、命令、报告、输入/输出和失败；当前环境的恢复边界见开头说明。

Zil 使用 Rust 1.99.1 / LLVM 23.1.4，Rust 参照为 rustc 1.99.1 自带 LLVM 23.1.1。环境为 4 CPU / 16 GiB，只有一个重构建或测量窗口。OS 缓存未清空；cold 指本 block 的应用缓存/目标目录为空，工具链并非磁盘冷态。

原采样器在本环境缺少 `/proc/<pid>/task/<pid>/children` 时只观测根进程，旧 `tree_*` RSS 字段不是进程树内存。原数据不改名：只使用明确的 Zil resident RSS/HWM，Rust 不作内存优势比较。新的 [rust_compare.py](rust_compare.py) 明确输出 Cargo 根 PID RSS；原计时使用独立 wait4 完成边界，不受该 RSS 缺陷影响。新脚本与实际测量 v1 的哈希分开记录。

## 重现

先分别检出上述 main 与候选源码，按 `docs/guide/build.md` 构建 release CLI；不同源码使用独立 target。当前环境需先加载 `/workspace/.zil-env/env.sh`，再设置各自的 `CARGO_TARGET_DIR`。冻结二进制及构建收据位于 `out/binaries/`，SHA256 见 results.json。

使用分支内现有工具重跑检查；每次选择新的输出目录：

```bash
python3 tools/research/reactive-query-graph/native_measure.py \
  --baseline /absolute/path/to/main-zil --candidate /absolute/path/to/new-zil \
  --baseline-source 99035931d358add0eaed6d4535035c38413701dd \
  --candidate-source 0601b9786d0fe683b4321f0d086230e5be027602 \
  --out out/runs/reproduce-main-check --case scalar --mode check \
  --modules 20 --functions 20 --steps 16 --blocks 6 --cache all --capacity 256 \
  --backend llvm-api --codegen-units 1 --codegen-jobs 1 \
  --candidate-extra-arg=--producer-reuse \
  --request cold --request unchanged --request private --request multi \
  --request broad --request restore --request error --request recovery
python3 tools/research/reactive-query-graph/native_summarize.py \
  out/runs/reproduce-main-check --policy tools/research/incremental-demand/protocol.json
```

原生主测仅将 `--mode check` 换为 `--mode O2`，普通分区补充另将 `--codegen-units 1` 换为 `16`，使用新的输出目录。完整矩阵的冻结执行器、协议和独立验证脚本随原始证据归档；上面的命令生成同一主测 workload 与统计，不代替全部验收矩阵。

Rust 使用明确的真实 cargo/rustc 路径；脚本默认只生成，`--run` 才执行：

```bash
python3 tools/research/incremental-demand/rust_compare.py \
  --cargo /absolute/path/to/cargo --rustc /absolute/path/to/rustc \
  --out out/runs/reproduce-rust-check --run --mode check \
  --blocks 6 --modules 20 --functions 20 --steps 16 --codegen-units 1
python3 tools/research/incremental-demand/rust_compare.py \
  --cargo /absolute/path/to/cargo --rustc /absolute/path/to/rustc \
  --out out/runs/reproduce-rust-o2 --run --mode o2 \
  --blocks 6 --modules 20 --functions 20 --steps 16 --codegen-units 256
```

results.json 保存精简观测、哈希和证据索引；`out/` 内保留完整 raw、失败和独立复核。本报告仅作研究分支交付，不关闭通用增量编译或 Rust 对照的父目标。
