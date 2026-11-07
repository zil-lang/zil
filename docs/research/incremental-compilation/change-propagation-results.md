# Stage 10：缩小传播结果引起的重编范围，精简带参叶函数发射

生产实现 `8cf48baf2b5fd6fa944e4c65a6116907f1cf53d7`，基线 `6dd46bcf59f74c0def15b3db8c3bf2ac49d0bf87`（生产同 Stage 9 的 70615ae）。专项 [PR #13](https://github.com/zil-lang/zil/pull/13) 保持独立。

本轮重点是此前最弱的**保留符号模式、均匀 8×1024 函数**：完整冷/暖/编辑请求分别减少 **7.7% / 22.8% / 30.4%**，三个缓存序列每一对均改善。最新耗时为同场 Rust 的 **0.930 / 0.692 / 1.334 倍**。冷暖已低于 Rust 的点估计；编辑仍慢 **33.4%**，全场景超过 Rust 的目标尚未完成。

单大模块和偏斜模块的保留符号保护序列也改善，但各仅一次。封闭均匀模式的单次保护检查冷/暖回退 **1.32% / 1.45%**，编辑改善 1.47%，完整保留这些不利结果。没有以旧窗口的 Rust 比值相乘或相减来计算本轮收益。

## 找到的问题与 Rust 依据

Stage 9 最终保留符号编辑只使 3/8 对象失效，并非全部失效。但变更的叶函数、模块汇总结果和根汇总结果分处桶 1、5、0；两个很小的常量结果各自连带重编约 1024 个未变化 helper。每个失效对象约需 81 ms 优化和 84 ms 发射，两个工作线程要分两波处理三个大对象。

固定 Rust 1.99.1 提交 `58cf8f9c35b355199727627d07ae34a4e2846d67` 的 [partitioning.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_monomorphize/src/partitioning.rs#L38) 将较稳定正文和易变实例分组，避免新增实例牵连无关正文；同一文件明确说明分区粒度与内联质量的取舍。本轮借鉴“隔离不同失效原因”的原则，分类依据是 ZIL 当前传播后的小调用者，不是照搬 Rust 的泛型分类。读取文件 SHA256 为 `e7c5bbba84fc3531c7a9cb861778a028544a56fb211318d925c05b24d1ad30e4`，原文与来源记录随证据保存。

另一个明确成本是仍须导出的 8192 个参数化 `x + 常量` 函数：旧 emitter 每个先构造空入口跳转及 `add 0, 常量`，交给 LLVM 再删除。删除这些函数会改变保留符号合同，因此本轮精简其生成过程，保留全部定义。

## 实现与边界

1. **小调用者集中到原入口对象。** 从当前 LIR 的直接源函数调用识别可能随传播结果变化的调用者；先确定必须共享局部状态的完整组件，再完整计算原文件/256 函数块的分桶。仅将合格的小调用者组件移至原入口桶，其余每个 helper 的原归属不变。扫描限额 100 万、至多 256 个选中定义、机器成本至多 4096 且不超过整体的 1/16；若仍有直接调用需要组外普通叶函数，回退原策略。原完整调用闭包导入、canonical 地址、foreign 声明依赖、当前对象键和预检继续生效。
2. **紧凑生成简单带参叶函数。** 仅在既有显式 incremental O2 下，识别单参数、单局部、单源块、至多 32 个值/指令、参数与同类型常量进行唯一一次加法并直接返回的函数。支持 i32/i64 的 sadd 和 u8/u16 的 uadd，保留原符号、签名、溢出 intrinsic、检查分支与 trap，直接使用常量操作数和三个必要机器块。Store、调用、地址、额外运算、环境或未知形状均走原 emitter。完整当前 Verify/bind 在前，资格确认完成前不修改 intrinsic 集合。

源码为 `machine_cgu.rs`、`emit.rs` 与新增 `emit/scalar_leaf.rs`；提案见 `changes/research-propagated-caller-cgu.md`、`changes/research-compact-scalar-leaf-emission.md`。没有新增选项或持久语义缓存，也没有 alias、noinline thunk、降低 O2 或重新启用此前退化的 early MergeFunctions。

最初考虑预留一个独立小桶，但静态分析发现“7 个大桶 + 1 个小桶”在两个线程下可能增加约 12%–14% 冷编译尾部时间，因而在计量前改为原入口桶集中。该放弃方案没有性能样本，不计作提速。本轮两项改动组合计量，不单独分配各自的完整请求收益。

## 机制检查与最终结果

先对真实均匀大源码执行 3 次 cold/warm/edit：当前 Verify 和原生结果均通过；全部 **8203 个定义保留**，**8192 个 checked leaf** 使用紧凑形式，机器文本由 3162987 字节减至 2769771 字节，编辑实际为 **6 hit / 2 miss**。这是机制检查，单候选时间不作为配对提速证据。

最终独占计量沿用 Stage 8 runner：均匀 default 3 个缓存序列，单模块/偏斜 default 各 1 个，均匀 closed 1 个，共 **54 次构建与原生结果检查全部通过**。36 次 ZIL 都执行当前 Verify；18 次 Rust 观察来自 6 条独立缓存序列，不跨窗口/合同复用。同一序列的冷、暖、编辑并非统计独立；3 次重复不构成总体置信区间，单次保护检查不证明稳定收益。

ZIL 显式使用 O2、incremental-o2、after-MergeFunctions、8 CGU、2 jobs、独立对象/语义缓存。default 指保留符号合同，不是 CLI 默认配置；closed 加显式封闭选项。Rust 使用 O2、8 CGU、incremental、overflow-checks=yes、panic=abort。完整 CLI、当前检查、缓存、后端和链接都计时，生成源码和原生产物执行不计时；cold 指编译器缓存冷。

ZIL 继续使用 LLVM 23.1.4 的既有四 TU 候选，Rust 为 LLVM 23.1.1；本轮未改 LLVM。DSO SHA256 `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98`，最终 ZIL SHA256 `15a988bcb1b42c6f01be3659728fdb074c2369955ef48b2395c8a408863e66dd`。三个最终 manifest 均绑定 8cf48baf2，crate diff 为空。

时间为毫秒中位数，比值为逐对比值的几何平均；不能以中位数之比代替。没有混合不同重复数计算一个总体提速。

| 分布 | 合同 | 请求 | 次数 | 原 ZIL ms | 新 ZIL ms | Rust ms | 新/原 | 新/Rust |
|---|---|---|---:|---:|---:|---:|---:|---:|
| 均匀 8×1024 | default | cold | 3 | 1343.1 | 1229.6 | 1323.5 | 0.923 | 0.930 |
| 均匀 8×1024 | default | warm | 3 | 392.9 | 301.5 | 434.6 | 0.772 | 0.692 |
| 均匀 8×1024 | default | edit | 3 | 960.3 | 672.5 | 499.8 | 0.696 | 1.334 |
| 单模块 8192 | default | cold | 1 | 1412.2 | 1274.1 | 2301.8 | 0.902 | 0.554 |
| 单模块 8192 | default | warm | 1 | 400.7 | 344.0 | 466.1 | 0.858 | 0.738 |
| 单模块 8192 | default | edit | 1 | 782.5 | 744.1 | 1308.8 | 0.951 | 0.569 |
| 偏斜 8 模块 | default | cold | 1 | 1392.1 | 1249.8 | 1575.0 | 0.898 | 0.794 |
| 偏斜 8 模块 | default | warm | 1 | 373.8 | 331.9 | 451.7 | 0.888 | 0.735 |
| 偏斜 8 模块 | default | edit | 1 | 778.8 | 645.5 | 850.8 | 0.829 | 0.759 |
| 均匀 8×1024 | closed | cold | 1 | 413.7 | 419.2 | 1261.8 | 1.013 | 0.332 |
| 均匀 8×1024 | closed | warm | 1 | 232.3 | 235.7 | 408.9 | 1.015 | 0.577 |
| 均匀 8×1024 | closed | edit | 1 | 431.4 | 425.1 | 464.5 | 0.985 | 0.915 |

均匀 default 的单对新/原范围分别为 cold **0.888–0.982**、warm **0.767–0.778**、edit **0.683–0.714**。最大进程 RSS 中位数 cold **245.4→212.5 MiB**、warm **168.9→135.4 MiB**、edit **244.8→212.2 MiB**；配对 CPU 比 **0.940 / 0.754 / 0.724**。RSS 来自 wait4，不是整个进程树同时占用内存的总峰值；全部资源单元都保存。

## 正确性与原生质量

**31 项不同限定控制通过**：16 个分区控制、11 个真实源码标量控制、3 个对象缓存/资源组合控制、1 个当前绑定控制。新增源码控制覆盖四种整数的安全上边界与溢出 trap、带效果的最近回退形状，以及四模块编辑只失效两个对象、恢复原文全部命中。既有无关编辑、函数插入/删除、动态输入、资源清理、闭包、函数地址和旧 LIR 拒绝断言未弱化。backend/driver all-targets Clippy `-D warnings`、fmt 和独立源码/统计审核通过；未本地重复正式全仓。

另对现有六对最终编辑产物做离线静态比较，没有增加编译或运行计时。GNU nm 给出函数实际大小，排除函数间填充；objdump 保留操作码、立即数及符号分支目标，只归一化布局地址和明确标注的 RIP 相对重定位。各对生成符号集合相同，全部共同生成函数及入口的指令序列、函数大小一致；其中两对完整可执行文件逐字相同，另四对布局不同。这支持这些固定输入的机器代码质量保持，不证明一般运行性能、地址值或缓存局部性完全等价。工具为 `stage10_quality.py`，原反汇编、符号和汇总均保存。

推送前 Stage 9 HEAD 的官方 CI [41283921039](https://github.com/zil-lang/zil/actions/runs/41283921039) 已成功；研究 LLVM CI [41283915061](https://github.com/zil-lang/zil/actions/runs/41283915061) 最近观察仍在运行。新交付 HEAD 以实际新工作流状态为准。历史 handoff 完整性 Fail 保留，未发布环境或合入 main。

## 全场景目标与下一优先项

已有 check/O0、21 有效生成族 O2、14 个真实小项目全景继续见 [Stage 9 汇总](STAGE9-RESULTS.md)，保持其历史版本。此次没有重新测量最新版本的全部模式和真实大项目。当前大模块语料以可折叠标量调用图为主，不能替代泛型、CT、复杂类型、资源、闭包及工业项目的广泛性能结论。

均匀保留符号编辑仍有约 33.4% Rust 差距。下一项应继续减少**两个失效大对象中未变化函数的重复后端工作**，在保持导出身份和内联质量的条件下评估更细的工作产品；同时以当前阶段成本判断是否值得推进声明/签名/效果投影和函数体语义复用。不能靠省略真实依赖或把正文 hash 去掉制造命中。持久 Types 增量仍未完成。

本轮未新增旧图切割或 zit 内核；调用者分组不是旧算法收益的重新归因，原结论继续见 [Stage 6 算法报告](STAGE6-ALGORITHMS.md)。

| Rust 目标 | 本轮实证及限制 |
|---|---|
| 安全 | 当前 Verify/bind、原预算和机器检查保留，最近正反例通过；完整语言安全目标未升级 |
| 运行与内存 | 六对产物的上述静态代码检查一致，编译 RSS 降低；没有一般程序运行/内存同级证明 |
| 语法与维护 | 源语法不变；增加两个有界内部策略，没有新的用户配置负担 |
| 编译 | 目标冷/暖/编辑改善，编辑仍慢于 Rust；封闭保护冷暖有小幅不利点 |
| 至少一处突破 | 已有目标子集的实测提速，尚不能认定全场景或产品级突破；Doing 与原产品门保持 |

证据：[来源与身份](../../../evidence/research/incremental-compilation/stage10/provenance.json)、[目标](../../../evidence/research/incremental-compilation/stage10/final-target/analysis.json)、[大模块保护](../../../evidence/research/incremental-compilation/stage10/final-guards/analysis.json)、[封闭保护](../../../evidence/research/incremental-compilation/stage10/final-closed-guard/analysis.json)、[原生质量](../../../evidence/research/incremental-compilation/stage10/native-quality.json)、[独审](../../../evidence/research/incremental-compilation/stage10/REVIEW.md)、[原始记录](../../../evidence/research/incremental-compilation/stage10/raw.tar.gz)。复算沿用 `stage7_analyze.py <窗口目录>` 和 `stage8_resources.py <窗口目录>`；SDK、二进制及缓存未归档。
