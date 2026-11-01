# Stage 8：借鉴 Rust 所有者组织，减少重复前端构造

实现 `8c5dcc072318f9c496bdc19e14cbbbaadccccc2e`，基线 `1498bfee9`（Stage 7）。代码保留在专项 [PR #13](https://github.com/zil-lang/zil/pull/13)，未合入 main。

本轮取得了有边界的改善：目标 **8×1024 函数**的编辑请求，保留符号模式耗时下降 **5.5%**，显式封闭模式下降 **10.2%**；编译进程最大 RSS 约下降 **15%**。当前编辑耗时仍为 Rust 的 **1.98 倍 / 1.17 倍**。同时，保留符号模式暖构建 **回退 8.4%，三对全部回退**，偏斜分布默认模式的单次保护检查也退步。不能表述为全面净收益或已追平 Rust。

## 从 Rust 源码落实了什么

参照实际比较工具链 Rust 1.99.1 的固定提交 `58cf8f9c35b355199727627d07ae34a4e2846d67`，而非只读概念介绍：

- [queries.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/queries.rs#L186)：resolver、AST 索引、HIR lowering 是 `eval_always`；Rust 并非每次编辑都跳过整个前端。
- [ast_lowering/lib.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_ast_lowering/src/lib.rs#L495)：共享当前所有者数据，避免不必要的完整结构复制。本轮移植的是这一组织原则，不是 Rust pass 代码。
- [hir/mod.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/hir/mod.rs#L395)：owner 的节点、父关系、trait map、属性分别投影；合并投影曾导致性能问题。[fn_sig](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_hir_analysis/src/collect.rs#L1007) 只返回签名结果，让函数体变化有机会停在等值屏障前。这是后续跨版本增量应借鉴的方向。
- [on_disk_cache.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/query/on_disk_cache.rs#L646)：稳定 DefPathHash 映射回当前 ID。持久缓存需要稳定身份和当前绑定，不能简单去掉输入 hash。

12 个实际读取文件的 URL、SHA256 和大小保存在 [Rust 来源清单](../../../evidence/research/incremental-compilation/stage8/rust-source-manifest.json)。

## 最终实现

1. 项目语法归一化/泛型特化完成后，把已经拥有的条目移动到各模块，避免再次复制整棵 AST；原来的预算访问、声明检查与顺序保留。
2. 闭包推导可返回借用结果。只有它是原输入上的恒等操作、CT 后完整 Core 与原输入相同且 Limits 完全相同，Types 才复用这一已完成的推导。真实类型检查、聚合类型解析、Analyze、独立 Verify 继续运行；发生转换的闭包、CT 或限制变化走原路径。
3. 对有界白名单内的普通标量 Core，CT 使用只读遍历执行原声明、提示、诊断和费用检查，避免重建完全相同的 Core 树。验证成功后共享当前 `Arc<CoreProgram>`。含 CT、闭包、复杂类型或超出资格扫描界限时回退原求值器；普通前缀后遇到复杂模块会恢复原项目剩余额度，再完整求值。

这是**当前输入内**的工作复用，没有增加持久化模块/函数语义缓存，也没有绕过 CT 或 Verify。准备信息字段私有、不可修改、不序列化，不携带安全权限。原公共 raw API 和 Wire 保留；不改变接受性、错误顺序、ABI、优化级别或默认额度。

中间提交 `f289037fe` 只实现了前两项。其 135 请求确认中，大多数收益只有几个百分点，均匀模块 closed 编辑几乎不变，因此继续实现第三项。中间窗口保留并单独标识，不能归为最终版本成绩。

## 最终测量

最弱的均匀分布测 3 个独立缓存序列；单大模块和偏斜模块各做 1 个保护序列。每序列 cold → warm → 私有函数体 edit。总计 **75 次构建和原生 oracle**：60 次 ZIL 均执行当前 Verify，15 次 Rust。两种 ZIL 可见性合同引用相同的 Rust 观察，不重复计为 Rust 样本。

所有 ZIL 臂显式使用 `--opt O2 --incremental-o2 --function-merging after --codegen-units 8 --codegen-jobs 2` 及独立对象/语义缓存。default 只表示保留符号合同，不是 CLI 默认配置；closed 加 `--closed-executable`，更接近 Rust 私有 helper 可删除的合同。Rust 为 O2、8 CGU、增量、overflow-checks=yes、panic=abort，LLVM 23.1.1；ZIL 为 LLVM 23.1.4 加既有四 TU 修复，库 SHA256 `7b5c5ef8…fa9a98`。本轮未改后端或工具链。

以下均为同窗口基线与候选比较。时间是毫秒中位数，比值是逐对比值的几何平均；不要用中位数之比代替配对比值。3 次重复不构成总体置信区间，单次保护检查不证明稳定收益。没有将不同重复数的窗口合为一个总体收益。

| 分布 | 合同 | 请求 | 原 ZIL ms | 新 ZIL ms | Rust ms | 新/原 | 新/Rust | 单对新/原范围 |
|---|---|---|---:|---:|---:|---:|---:|---|
| large-even-8x1024 | default | cold | 1342.5 | 1277.0 | 1279.3 | 0.955 | 1.022 | 0.943–0.973 |
| large-even-8x1024 | default | warm | 387.9 | 425.6 | 427.6 | 1.084 | 0.974 | 1.040–1.114 |
| large-even-8x1024 | default | edit | 991.2 | 949.9 | 473.5 | 0.945 | 1.980 | 0.917–0.971 |
| large-even-8x1024 | closed | cold | 642.5 | 583.9 | 1279.3 | 0.896 | 0.459 | 0.878–0.927 |
| large-even-8x1024 | closed | warm | 397.1 | 395.9 | 427.6 | 1.001 | 0.898 | 0.966–1.043 |
| large-even-8x1024 | closed | edit | 627.1 | 565.6 | 473.5 | 0.898 | 1.174 | 0.888–0.906 |

以下两种分布各只有一次观察：

| 分布 | 合同 | 请求 | 原 ZIL ms | 新 ZIL ms | Rust ms | 新/原 | 新/Rust |
|---|---|---|---:|---:|---:|---:|---:|
| large-8192 | default | cold | 1466.8 | 1359.0 | 2090.3 | 0.927 | 0.650 |
| large-8192 | default | warm | 405.9 | 416.8 | 374.4 | 1.027 | 1.113 |
| large-8192 | default | edit | 834.9 | 760.3 | 1225.2 | 0.911 | 0.621 |
| large-8192 | closed | cold | 712.3 | 621.5 | 2090.3 | 0.873 | 0.297 |
| large-8192 | closed | warm | 411.7 | 422.1 | 374.4 | 1.025 | 1.127 |
| large-8192 | closed | edit | 682.9 | 630.4 | 1225.2 | 0.923 | 0.515 |
| large-skew-8 | default | cold | 1331.7 | 1396.2 | 1417.2 | 1.048 | 0.985 |
| large-skew-8 | default | warm | 391.3 | 453.1 | 491.3 | 1.158 | 0.922 |
| large-skew-8 | default | edit | 817.2 | 862.6 | 771.5 | 1.056 | 1.118 |
| large-skew-8 | closed | cold | 620.6 | 585.4 | 1417.2 | 0.943 | 0.413 |
| large-skew-8 | closed | warm | 392.1 | 390.0 | 491.3 | 0.995 | 0.794 |
| large-skew-8 | closed | edit | 642.5 | 587.6 | 771.5 | 0.915 | 0.762 |

目标 edit 的 CPU 配对比为 default **0.969**、closed **0.895**；最大进程 RSS 中位数为 **286.2→244.5 MiB / 269.6→228.6 MiB**。资源来自 Linux wait4，RSS 不是同时运行的整个进程树内存总峰值。暖构建 RSS 基本不变。全部资源单元保存在 resources.json，没有仅挑改善单元。

暖构建的实际报告均显示 semantic hit 和全部对象命中（default 8/8，closed 1/1）；项目代码在完成 Typed 缓存命中后返回，绕过本轮的 Expand/CT/Types 准备路径。default 暖请求 CPU 同时回退约 7.2%，不能简单称为等待噪声。**回退原因尚未定位**，也不能据未执行新路径就声称与候选无关；保留为下一次诊断的明确问题。此次没有为消除不利结果而重跑窗口。

## 验证、证据与后续方向

24 项限定测试通过：CT 新旧遍历的真实源码差分及 N−1/N/N+1 节点/工作/深度边界，原 first error/usage 相同；闭包/CT/限制/声明变化回退；跨模块 trait；项目缓存恢复、取消及产物路径。特别覆盖普通模块前缀后遇到 CT 模块的预算恢复，`max_functions=2` 下 cold/warm/edit 原生结果为 5/5/9。另对现有 2048 分支函数动态输入样本执行基线/候选×两个源码版本的 4 次 check，各有当前 Verify；这不是该样本的新原生构建或性能结论。最终 10 对编辑后的 ZIL 原生可执行文件逐字一致，支持这些受测输入的机器代码保持，不代替一般运行质量证明。

四个相关 crate 的 all-targets Clippy（`-D warnings`）、fmt 及非作者源码/统计审查通过。没有在本地重跑正式全仓。推送前 Stage 7 HEAD 的官方 CI `41276295788` 和研究 LLVM CI `41276289155` 均成功；新 HEAD 的 CI 以 GitHub 后续实际状态为准，不用旧结果代替。

第一次 screen 与 release 构建重叠，计时明确排除；尝试 GNU time 因工具不存在而在构建前失败，也保留日志。其后的独占初筛、中间确认、最终两窗各自保留身份。最终使用 wait4 测量，两臂同一计时器；不把不同计时器/历史窗口的绝对时间相减或收益叠乘。环境恢复限制及历史 handoff Fail 继续保留，未发布环境配置。

当前最大的结构问题仍是整项目输入键与整项目语义结果。ZIL 的项目 snapshot digest 目前依据构造键/依赖身份，并不是 Rust 式结果内容等值屏障；增加反向边、图切割或删除 input hash 都不能单独解决。后续应借 Rust 的**稳定所有者 + 声明/签名/效果投影 + 当前绑定**，先把私有函数体变化与不变声明环境分开，再落地函数体依赖及等值复用。trait/default/generic body、CT、负查询和预算重放必须在同一设计里。现有单模块 Expand API 缺少项目完整上下文，不能直接充当项目缓存 miss 的计算器。

本轮未新增图切割或 zit 内核集成，不能把收益归给它们。修复后的 zit 机制与图实验继续按 [Stage 6 记录](STAGE6-ALGORITHMS.md) 归因。下一轮先定位暖请求回退，随后实施稳定声明投影；不再围绕已经证明效果有限的整树复制做大规模重复试验。

证据入口：[来源](../../../evidence/research/incremental-compilation/stage8/provenance.json)、[目标完整统计](../../../evidence/research/incremental-compilation/stage8/final-target/analysis.json)、[保护分布](../../../evidence/research/incremental-compilation/stage8/final-guards/analysis.json)、[独审](../../../evidence/research/incremental-compilation/stage8/REVIEW.md)、[原始日志与生成源](../../../evidence/research/incremental-compilation/stage8/raw.tar.gz)。使用 `stage7_analyze.py <窗口目录>` 和 `stage8_resources.py <窗口目录>` 复算，不执行新构建；SDK、二进制和缓存未打包。

Rust 四目标：当前安全检查和预算未放宽；10 对受测机器代码相同，但未建立完整语言运行/内存同级证明；语法不变；编译成本有上述改善与回退。持久函数级语义增量及产品级可重复突破仍未完成，工单保持 Doing，产品门未改。
