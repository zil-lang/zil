# Stage 9：提前收集实际需要生成的代码，缩小 Rust O2 差距

最终生产实现 `70615ae1cfcdf20cc6f449612866f604f1b47eb8`，本轮基线 `365c8a5584dcf84b4f6842b7ee20d921489c4255`（Stage 8）。专项 [PR #13](https://github.com/zil-lang/zil/pull/13) 保持独立，未合入 main。

本轮最弱目标 **8×1024 函数、显式封闭可执行文件**的完整请求，相对本轮基线冷构建减少 **32.3%**、暖构建减少 **44.3%**、函数体编辑减少 **29.7%**；编辑耗时为同场 Rust 的 **0.807 倍**。三个独立缓存序列的每一对均改善。收益来自在当前检查完成后提前确定需要生成的代码，避免先构造、哈希、预检大量随后删除的函数。

这还不是全场景追平：保留符号模式的单次保护检查，编辑仍为 Rust 的 **1.849 倍**，冷构建较本轮基线 **回退 4.48%**。本轮没有重跑全量分布，以下历史全景与最终局部结果分别保留版本和适用范围。

## 已有全景：直接汇总，不追加测量

比值均为完整 CLI **ZIL/Rust**，小于 1 表示 ZIL 更快。历史矩阵不能冒充最终 70615ae 的全场景成绩，也不能与本轮收益相乘。

| 既有矩阵 | 冷请求 | 暖请求 | 前部编辑 | 后部编辑 | 恢复原文 |
|---|---:|---:|---:|---:|---:|
| Stage 5，21 有效生成族，check | 0.470 | 1.202 | 1.034 | 1.035 | 1.066 |
| Stage 5，21 有效生成族，O0 | 0.631 | 1.371 | 1.086 | 0.980 | 1.026 |
| Stage 6，21 有效生成族，O2 保留符号 | 1.063 | 0.986 | 1.520 | 1.396 | 0.728 |
| Stage 6，21 有效生成族，O2 封闭 | 0.696 | 0.987 | 1.193 | 1.117 | 0.698 |
| Stage 6，14 个仓库真实小项目，O2 保留符号 | 0.474 | 0.542 | 未测 | 未测 | 未测 |

Stage 5 check/O0 对应 `801933a` 历史窗口，详见 [分布报告](DISTRIBUTION-RESULTS.md) 和 [完整表](DISTRIBUTION-TABLES.md)。Stage 6 是 `888` 共同源码、既有四 TU LLVM、after-MergeFunctions、可信语义候选缓存，**incremental-o2 关闭**的六区组窗口，详见 [Stage 6 结果](STAGE6-RESULTS.md)。23 个生成族中的两个因原 ZIL 默认预算拒绝，不能当作速度胜出；真实项目均较小，且没有编辑结果。它们说明已有语料上的差距集中于 O2 编辑和部分大模块，不能证明工业大项目全面胜出。

上一轮更接近当前配置的三种大模块分布见 [Stage 8](STAGE8-RESULTS.md)：均匀分布编辑为 Rust 的 1.980 倍（保留符号）/1.174 倍（封闭）。本轮重新同时运行基线、候选和 Rust，下面的收益全部来自新窗口内配对，未跨窗口相减。

## Rust 源码依据与最终实现

固定比较工具链为 Rust 1.99.1，提交 `58cf8f9c35b355199727627d07ae34a4e2846d67`。本轮读取 [rustc_monomorphize/collector.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_monomorphize/src/collector.rs#L156)：Rust 区分真正需要生成的 used items 与仍需检查的 mentioned items。借鉴的是**分开处理生成需求与检查义务**，并非复制 Rust pass。文件来源和 SHA256 见 [来源记录](../../../evidence/research/incremental-compilation/stage9/rust-collector.json)；此前查询和稳定身份的 12 个固定源码来源继续见 Stage 8。

1. **在调用处消费当前标量事实。** 当前 Verify、精确 `bind_lir` 和有界摘要完成后，把同一私有事实表传给 emitter。只有无实际参数、无形式参数、非 foreign、非 Unit、返回类型完全匹配的已证明常量调用才直接产生 SSA 常量。先保留原被调函数/环境检查，其他指令、动态参数、效果、trap、间接调用及入口封装照原路径处理。此前同一事实要到机器表示构造后才能再次发现。
2. **批量计算对象键。** 私有 4 KiB 缓冲将大量细碎 Hash 写入合并后交给 SHA256；字节流、最终摘要、命名空间、字段和预检不变。这降低键计算本身的开销，不减少缓存依赖。
3. **封闭模式提前收集当前可达代码。** 仍限现有 Linux、LLVM API、O2 incremental、closed、Main、多 CGU 合同。在当前完整源码检查之后，以 main 和所有不属于既有严格标量支持子集的函数为根，遍历实际剩余直接调用；仅忽略第 1 项确实替换掉的常量调用边。由此可省去已无调用的受支持定义，包括带参数的叶函数。foreign 声明仍完整生成并检查；不支持的函数保留为根，即使不可达也不隐藏其原发射诊断。闭包、资源、地址/函数指针或未知剩余指令使收集回退；剩余非 foreign 函数至多 8 个；仍需遍历的非摘要函数，其体/局部/值/块工作总量至多 4096，否则走原流程。已命中摘要的保留函数直接发射常量，不遍历原函数体。可达效果、动态参数与 trap 仍存在。

最终版本还把原稳定命名准备应用于**实际保留函数的小视图**，保留当前 hints、foreign 声明、完整小模型预检、原内部化/分区及失败回退。此视图只用于名称规划，不提供 Verify 或 bind 权限。因此既避免为数千个废弃定义做稳定哈希，又保持项目中插入/删除函数、无关编辑和恢复原文时的既有对象复用约定。

实现位置为 `emit.rs`、`emit/scalar_summaries.rs`、`object_cache.rs`、`machine_stable_identity.rs`、`build.rs`。提案见 `changes/research-current-summary-calls.md`、`research-object-key-buffering.md`、`research-closed-summary-collection.md`。这些是**当前请求内**的工作削减；没有实现跨版本函数 Types 缓存，也没有复用旧 Verify 的权限。

## 最终同场测量

三个最终窗口为 `delivery-closed-target`、`delivery-closed-guards`、`delivery-default-guard`，共 **54 次构建及原生结果检查全部有效**：36 次 ZIL 均有当前 Verify，18 次 Rust 观察不跨窗口或合同复用，来自 6 条独立缓存序列；同一序列的冷、暖、编辑观察并非统计独立。均匀封闭目标有 3 个独立缓存序列；另外两个分布和均匀保留符号各 1 个序列。每序列 cold → warm → 私有函数体 edit。3 次重复不构成总体置信区间，单次保护检查不证明稳定收益，不把不同重复数混成总体几何平均。

ZIL 显式使用 `--opt O2 --incremental-o2 --function-merging after --codegen-units 8 --codegen-jobs 2`、独立对象/语义缓存；default 在表中只表示**保留符号合同**，不是 CLI 默认配置。closed 另加 `--closed-executable`，更接近 Rust 私有 helper 可删除的可见性。Rust 使用 O2、8 CGU、incremental、overflow-checks=yes、panic=abort。测量包含 CLI、当前检查、缓存、后端与链接，不包含源生成及产物执行。cold 指编译器缓存冷，不等于清空 OS 页缓存。

ZIL 使用既有 LLVM 23.1.4 四 TU 候选，Rust 为 LLVM 23.1.1；本轮没有改 LLVM。库 SHA256 为 `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98`，最终编译器 SHA256 为 `c2256c48fdc4398c59156f488ba2d45b19c76323ce51d96d1bd70f00b68141a0`。三个 manifest 源码均为最终 70615ae，crate diff 为空。

时间为毫秒中位数，比值为逐对比值的几何平均，不能用中位数之比代替。

| 分布 | 合同 | 请求 | 重复 | 原 ZIL ms | 新 ZIL ms | Rust ms | 新/原 | 新/Rust | 单对新/原范围 |
|---|---|---|---:|---:|---:|---:|---:|---:|---|
| 均匀 8×1024 | closed | cold | 3 | 623.4 | 412.3 | 1391.3 | 0.677 | 0.309 | 0.656–0.717 |
| 均匀 8×1024 | closed | warm | 3 | 418.9 | 240.6 | 466.9 | 0.557 | 0.497 | 0.510–0.593 |
| 均匀 8×1024 | closed | edit | 3 | 624.9 | 431.2 | 540.5 | 0.703 | 0.807 | 0.669–0.752 |
| 单模块 8192 | closed | cold | 1 | 682.4 | 467.2 | 2238.2 | 0.685 | 0.209 | 单次 |
| 单模块 8192 | closed | warm | 1 | 476.6 | 212.7 | 389.8 | 0.446 | 0.546 | 单次 |
| 单模块 8192 | closed | edit | 1 | 735.5 | 459.6 | 1244.7 | 0.625 | 0.369 | 单次 |
| 偏斜 8 模块 | closed | cold | 1 | 609.4 | 445.1 | 1524.3 | 0.730 | 0.292 | 单次 |
| 偏斜 8 模块 | closed | warm | 1 | 419.7 | 228.8 | 495.9 | 0.545 | 0.461 | 单次 |
| 偏斜 8 模块 | closed | edit | 1 | 619.8 | 465.1 | 979.5 | 0.750 | 0.475 | 单次 |
| 均匀 8×1024 | default | cold | 1 | 1533.4 | 1602.0 | 1255.3 | 1.045 | 1.276 | 单次 |
| 均匀 8×1024 | default | warm | 1 | 450.0 | 377.5 | 462.1 | 0.839 | 0.817 | 单次 |
| 均匀 8×1024 | default | edit | 1 | 1091.0 | 905.1 | 489.5 | 0.830 | 1.849 | 单次 |

封闭目标的 CPU 中位数 cold **0.616→0.409 s**、warm **0.418→0.240 s**、edit **0.622→0.426 s**；配对比为 0.676/0.556/0.701。最大进程 RSS 中位数分别为 **233.1→188.4 MiB**、**169.8→104.6 MiB**、**228.9→187.8 MiB**，配对比 0.808/0.615/0.820。它们来自 Linux wait4，不是整个进程树同时占用内存的总峰值。全部 12 个资源单元保留在 resources.json。

最终六对编辑后基线/候选原生可执行文件逐字一致；这支持这些受测输入的代码质量保持，不代替一般运行性能等价证明。这些分布主要是可折叠标量调用图，不能代表大型真实项目的复杂类型、泛型、CT、资源和闭包路径。

## 失败尝试、验证与归因

中间数据全部保留，名字带 final 的旧窗口也不冒充最终代码：

| 窗口 | 实际实现 | 结果与处置 |
|---|---|---|
| final-target / final-guards | `1bbc0baad` | 75 次有效；调用事实和缓冲键组合的编辑改善有限，不能独立分配两者收益 |
| collection-target / collection-guards | `0ecf97386` | 75 次有效，但只删已知零参数定义；大分布的叶函数带参数，快速收集没有触发。不是收集成功的证据 |
| final-closed-target / final-closed-guards / final-default-guard | `d2994e257` | 54 次有效，可达收集真正触发并改善；之后补齐既有稳定命名兼容，故不作为最终成绩 |
| 三个 delivery 窗口 | `70615ae1` | 最终代码和全部 12 单元，如上表 |

第一次收集条件没有匹配实际语料，却先做了计量，是本轮可以避免的工作。修正后先检查大样本机器表示，确认只留下源 main 和原生入口封装，再做候选确认。其单次机制计时没有配对，不算性能证据。`types-diagnostic` 只跑到 Types，其 511 MiB 调试树未打包，提取摘要见 typed-shapes.json。

两份私有插桩快照实际基于 `1498bfee9` 和 `365c8a558`；对应 manifest.source 只是 runner checkout，不是这两个编译器的源码身份，provenance 已明确纠正。诊断看见约 40–50 ms 的后机器标量工作和约 100 ms 的封闭稳定规划成本，但没有建立上一轮暖请求回退的精确因果归因。新版本改善了暖请求，不能反推旧回退已被解释。插桩首次匹配旧函数名、首次快照遗漏 tools/llvm/version.txt 的早期构建失败均保留日志。

最终 **20 项不同的限定控制通过**：当前调用事实及旧 LIR 拒绝 1 项、对象键差分 9 项、真实源码标量/效果/间接调用/封闭收集/trap 9 项、现有对象复用组合控制 1 项。最后一项直接运行未改断言的 `object_cache212::o2_folded_results_and_imported_bodies_preserve_current_verification`，覆盖 closed/opaque 组合、冷暖、无关编辑/恢复、前后编辑、函数插入删除及当前类型错误保持既有产物。不会把多版重复执行累加成更多独立测试。

backend 与 driver 的 all-targets Clippy（`-D warnings`）、fmt、diff 检查通过；非作者对最终源码及统计独立审核 Pass。没有本地重跑正式全仓。推送前基线 365c8a558 的官方 CI [41279340666](https://github.com/zil-lang/zil/actions/runs/41279340666) 和研究 LLVM CI [41279334464](https://github.com/zil-lang/zil/actions/runs/41279334464) 均成功；新交付 HEAD 的 CI 必须以实际新运行状态为准，不能由旧成功替代。历史 handoff 完整性 Fail 继续保留，环境配置未发布。

本轮没有新增旧图切割或 zit 内核实现；当前调用图收集不能重标为旧算法成绩。修复后 zit 适配、图实验及撤回的 DB 候选继续按 [Stage 6 算法报告](STAGE6-ALGORITHMS.md) 归因。

## 后续方向与四目标边界

下一优先项是 **保留符号合同下的后端工作与失效范围**：不能借封闭删除来回避必须保留的定义，应继续沿 Rust 的稳定生成单元、准确依赖和按需生成方式削减重复工作。先确认路径触发和成本上限，再做最弱分布的配对确认，不为汇总旧结果追加矩阵。

持久语义增量仍需稳定所有者、声明/签名/效果投影、当前 ID 绑定、真实预算重放以及 trait/CT/负依赖的完整设计。本轮只读评估的模块级普通标量函数体 Types 复用预计面对约 50 ms Types 和约 30 ms 现有解码成本，收益上限不如已确认的后端冗余，故没有实施或宣称完成。后续不能把简单反向边、去掉输入哈希或本轮当前事实表称为 Rust 式跨版本查询系统。

| Rust 目标 | 本轮实证与限制 |
|---|---|
| 安全 | 当前源码 Verify、bind、预算及机器预检保持，真实源码最近反例通过；不据此宣称完整语言安全目标完成 |
| 运行与内存 | 六对受测原生文件相同；编译进程 RSS 改善，尚无一般程序运行/内存同级结论 |
| 语法与维护 | 语法无变更；内部机制沿当前事实和已有命名规划复用，不构成新的用户表达力证明 |
| 编译 | 封闭标量大模块有明确完整请求改善；保留符号编辑和冷构建仍有上述差距与回退 |
| 至少一处突破 | 已有目标子集的实测提速并超过同场 Rust；尚不构成全语言或产品级突破，原产品门与 Doing 状态保留 |

证据：[来源与身份](../../../evidence/research/incremental-compilation/stage9/provenance.json)、[目标统计](../../../evidence/research/incremental-compilation/stage9/delivery-closed-target/analysis.json)、[分布保护](../../../evidence/research/incremental-compilation/stage9/delivery-closed-guards/analysis.json)、[保留符号保护](../../../evidence/research/incremental-compilation/stage9/delivery-default-guard/analysis.json)、[原生产物一致性](../../../evidence/research/incremental-compilation/stage9/delivery-native-equality.json)、[独审](../../../evidence/research/incremental-compilation/stage9/REVIEW.md)、[原始日志与源](../../../evidence/research/incremental-compilation/stage9/raw.tar.gz)。用 `python3 tools/research/incremental-compilation/stage7_analyze.py <窗口目录>` 与 `python3 tools/research/incremental-compilation/stage8_resources.py <窗口目录>` 可离线复算；SDK、编译器、缓存和巨大调试树不在归档中。
