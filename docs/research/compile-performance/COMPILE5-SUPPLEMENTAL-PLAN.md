# 编译5：正式语料覆盖与固定辅助探针

冻结时间：2026-10-30 16:09:13 UTC（北京时间 2026-10-31 00:09:13）。只读依据为源码 `187eae4f6bb085f0d0d42765f047b7f60add6bed`；尚未取得本轮正式性能结果，也未运行以下探针。

## 正式五语料的覆盖边界

依据：`tools/research/210/corpus.py`、`tools/product/108/programs104/retained/zil/*`、`tools/research/compile-performance/compile4_ci.py`；函数数含 extern 声明。

| 语料 | 固定权重 | 实际模块 / fn 数 | 复合类型与改写 |
| --- | ---: | ---: | --- |
| tree4x2 | 5% | 7 / 18 | 全标量，无聚合、枚举、闭包；形状解析可保持原 Core |
| tree32x8 | 15% | 35 / 294 | 同上，仅扩大模块/函数数量 |
| tree256x8 | 30% | 259 / 2310 | 同上 |
| tree1024x8 | 30% | 1027 / 9222 | 同上 |
| retained | 20% | 4 / 12 | 1 个两字段 State package、1 个闭包；ByteBuffer、切片与 opaque callable 等浅层形状 |

retained 可见的归一化 Type 包括 `Package(State, Managed(ByteBuffer), i64)`（4 节点）、`Opaque(FunctionPointer(u8)->i64)`（4 节点）、`Address(Slice(u8))`（3 节点）。闭包的参数/结果为标量；不会把整个捕获环境嵌入其 Core 返回 Type。该来源没有达到 CP19 的 8 节点入表阈值的类型树。这里的节点数指递归 Type 节点，不是运行数组长度。

全部五语料均没有 `#(...)`。CT Walker 仍遍历声明和运行时代码，但不存在这些源码发起的纯 CT VM 查询。四棵树的标量参数直接绕过 CP19 的 map；retained 仅能暴露浅复合类型尝试查询但不入表的固定成本，以及 borrowed hints 对浅类型的影响。私有/公开编辑仍只改标量函数体或增加一个标量参数，复合形状密度不随编辑增加。

因此主矩阵能检测 CP18 大量未改写标量 Core 的净成本、稀疏声明视图、retained 的真实资源/闭包行为和总体回退；**不能据此证明大型 Type Eq 的回退边界，也不能证明 CP19 大复合 shape memo 的收益**。O0 bridge 关闭 query cache，前端仍运行；Rust incremental 窗口启用 query cache，实际重走哪些阶段须读取收据，不能把热查询复用归因于本轮节点优化。O2 窗口沿用同一类型分布，没有补足此覆盖缺口。

## 辅助探针：3 个 case，固定规模

仅增加独立机制观察，不改变五语料、四请求权重或正式统计。不与主矩阵合并，不生成 ZIL/Rust 总分。保留三个 case，避免重复提示的收益抵消多处改写的成本。

共同类型记号：`P=(i64,i64)`，`Q=(P,P,P,P)`，`T=(Q,Q,Q,Q)`。T 为 **53 个 Type 节点、32 个 i64 叶、最大元组 arity=4**。P/Q/T 仅是生成器记号，源码须完全展开，不能添加语言尚未提供的类型别名。每个标量值用 `i64(1)`，不用不开放的 `1i64` 后缀。

全部 case 固定为 `main.zil` 和 `work.zil` 两模块。main 只调用 `work.run()`，通过 `extern "C" fn putchar(x:i32)->i32` 在 unsafe 中输出 `i32(result % 251)`，返回 0；stdin 为空。

| case | 固定源码结构 | 要观察的路径 | 独立 stdout oracle |
| --- | --- | --- | --- |
| unchanged-wide | work 定义 64 个不同的无参 `f0..f63()->T`，各返回全 1 元组；run 对每个 f 调用一次并将 `.0.0.0 + .1.3.1 + .3.3.1` 累加 | 53 节点相等比较，类型/表达式无需形状改写。返回类型各有独立地址；声明访问会建立 memo 候选，普通返回值 hint 仍走原 hint_type，预期没有同键 memo 重用。可暴露 map 未复用及 Eq 成本 | `bytes([192])` |
| multiple-rewrites | 与上一 case 同为 64 个 f；T 的 `.1.3.1` 与 `.3.3.1` 分别替换为本模块的 `CellA` / `CellB`；两 struct 均只有 `value:i64`，对应值为记录字面量。run 对这两处读取 `.value` | 每个 f 的返回类型两处 Named→Record，两个记录表达式也必须改写；原始类型仍53节点，解析后55节点。64个函数同时有多处真实改写，触发 Owned 子树合并。没有闭包提前预解析 | `bytes([192])` |
| repeated-hints | work 只有 `consume(p:T)->i64` 与 run；consume 返回上述三个标量字段之和。run 定义一个显式 T 的全1元组，直线展开 **128 条** `total += consume(p)`，无循环、无分支 | 同一原始参数 Type 地址、相同深度的重复 ≥8 节点提示：第一处调用建摘要，其余127处有命中机会；参数声明/入作用域和显式 binding 另有既有重复检查。静态预期，不当作已实测的命中计数 | `bytes([133])` |

前两 case 各67个 fn 声明（含 main 的 extern），第三个4个。所有 f 和 consume 都被实际调用；改写后的两个记录值进入 stdout。三个 case 都没有 # 查询，直接针对 CT 前端的重复 shape/hint 路径，不混入 VM 执行成本。

语言入口状态：递归元组、完整类型标注、有限 Copy record、字段访问、跨模块普通调用、i64 转换和 putchar 均已有生产路径/既有测试；基础依据包括 `ct_reuse_cp19::real_source_repeated_tuple_shapes_keep_complete_core_and_exact_node_boundary` 与 `aggregates` 测试。**本规模与组合尚未执行，不能预填 Success。** 先让 A/B 各执行三个源码的完整 CLI/native preflight；若拒绝，原样保存诊断并停止该版探针，不放宽预算、不静默缩小规模。修订需新计划及新目录。

## 冻结执行口径

- A=`a715f828261ff4b6ef9c8125623064360745aae4`，B=`187eae4f6bb085f0d0d42765f047b7f60add6bed` 对应的冻结二进制/构建收据；若发布提交只有元数据变化，另记提交及相同 crates tree 的关系。
- 只测独立 cold_process，每次新 CLI、全新工作/源码缓存/对象缓存目录；不宣称清空 OS 页缓存。本探针不增加私有/公开编辑场景。
- 与正式 O0 bridge 相同：`build ... --json --opt O0 --codegen-units 8 --codegen-jobs 4 --cache-dir ... --object-cache ... --clang ... -o ...`；同 LLVM API、工具链、target、CPU affinity、默认 Limits、溢出检查；query cache 与阶段计时关闭。A/B 使用相同配置。
- 固定3区块。区块1 case 顺序 unchanged/rewrite/repeated，各 ABBA；区块2 rewrite/repeated/unchanged，各 BAAB；区块3 repeated/unchanged/rewrite，各 ABBA。共 **36 条正式编译样本**；6次 A/B preflight 与全部 native oracle 在计时外。
- 保存全部源/生成参数/工具/二进制哈希、CLI stdout/stderr、退出码、完整 wall/CPU/RSS、机器产物与逐次 native 输出；身份漂移、失败、超时、缺行禁止该版评分，不删除慢值或挑成功重跑。窗口不并发构建、测试或其他探针。
- 每 case 每区块先平均同臂两次完整成本，跨区块取中位数后报告 B/A，并列全部区块与原始样本；无跨 case 加权总分。组合 A/B 只能说明 CP18+CP19 的净变化，不能单凭结果拆分两项的独立因果贡献。

这份文件只固定观察设计；未改代码、未生成基准源码、未运行 Cargo/编译/性能负载。
