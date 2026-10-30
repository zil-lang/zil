# zil编译4：实际优化、同机 Rust 对照与运行质量

日期：2026-10-30。研究分支：`research/compile-performance`。本轮被测候选固定为 **`0393660f7dc8414b0ab8c77cd3d576395f4fe095`**，后续报告提交不改变这次计量的源码身份。

## 本轮结论

**新旧 ZIL 的同配置 O0 桥接窗口，加权完整构建耗时从 450.530 ms 降至 446.530 ms，下降 0.89%。** 收益较小，且没有覆盖所有语料或请求：冷请求略慢，最小树及 retained 的编译点估计也更慢。O0 下 CP14 不生效，这个窗口隔离的是 CP15 借用推断结果的改动。所有语料及输入快照的旧新 O0 ELF 哈希相同。

**CP14 保留了本轮小程序单单元基线的运行质量。** 在直接质量窗口中，最小树与 retained 从请求 8 CGU/4 jobs 回退到实际 1/1，新旧单单元 ELF 逐字相同；其余三棵树仍为 8/4。本窗口没有观察到稳定的逐语料运行退化，但只有三个配对区块，不能据此宣称普遍运行加速或正式产品验收。

**整体编译效率超过 Rust 的目标仍未达到。** 本轮 O0 冷编译基本持平，增量混合比 Rust 慢 20.01%，O2 closed 编译慢 9.68%。本次主机与编译3不同，已合入的新主线也不同；不能迁移历史“快 22.93%”的结论，也不能把跨窗口比值变化全部归因为 CP15。

main 未合流。CP12 query cache 仍默认关闭，closed executable 仍默认关闭。完整工作区测试与正式产品门保持 **NotRun**；历史 CP13 NoGo 及其原始证据保持原状。

## 精确来源与执行

| 身份 | 值 |
| --- | --- |
| 纳入的最新主线 | `a8e8831a9e28ac2ec83be6eac740a8f04a00af7d` |
| 本轮基线 | `64072793a0f41994f216a6207bf49ee1e2d224f8` |
| 基线 crates tree | `dca1c046f0e03cfa037ac49e7275c5b114c3d3cc` |
| 被测候选 | `0393660f7dc8414b0ab8c77cd3d576395f4fe095` |
| 候选 crates tree | `0221fa705e8dc926c54f6b85f2dc9ef3af98559a` |
| 基线 CLI SHA-256 | `896aecfe398b4f4babd84052f96a3f0bf0d01ff0ab504bab469a1ff1210effec` |
| 候选 CLI SHA-256 | `3da310e2179a30bff2c6d520215b5e109ac346edb7baea13892ee026fe873dec` |
| Actions | [run 41068328117，success](https://github.com/zil-lang/zil/actions/runs/41068328117) |
| 原始 Actions artifact | [12982919612](https://github.com/zil-lang/zil/actions/runs/41068328117/artifacts/12982919612) |

本轮先恢复准确 Git 历史，将最新 main 合入原研究分支，再冻结基线与候选。通过 Git Data API 发布时，提交元数据会变化；各个 tree/blob 的 Git SHA 均逐个核对一致，父提交顺序保持，远端 commit 原始对象也已恢复并验证。对应关系见 [source-publication.json](../../../evidence/research/compile-performance/COMPILE4/source-publication.json) 和 [发布对象收据](../../../evidence/research/compile-performance/COMPILE4/round1/publication-commits.json)。

## 实际改动

| 项目 | 实现 | 保留的边界 |
| --- | --- | --- |
| CP14 | 显式 closed O2 且请求多单元时，机器指令总数不超过预先冻结的 2048 阈值则保留单单元。超过阈值仍走原分区。 | 阈值不依赖语料名称或跑分；O0、非 closed、原单单元路径保持。非法 units/jobs 仍先拒绝。保留 `default<O2>`、机器预检、Verify、ABI 和发布保护。 |
| CP15 | closure inference 不需要改写时返回 `Cow::Borrowed`；需要改写时才构造拥有结果，延迟复制未改变的函数前缀。driver CT 与类型入口采用借用 API。 | 原公开拥有返回 API 保留。所有元数据、名称、依赖、类型与预算检查照常执行；借用结果仍是不可信 Core，独立 Verify 继续执行。 |
| CP16 | 明确区分 Rust `native` 与 `off` LTO 控制，冻结到计划、命令及摘要。 | 主要对照仍用 native；旧 schema `/2` 不静默迁移到 `/3`。本轮未测 LTO-off 速度，不能择优替换主结果。 |
| CP17 | 提交驱动的同机串行 CI 入口；分别构建两份 release 编译器，完成定向回归后才开始现有基准。 | 固定完整五语料和权重，记录失败、超时、源身份与原始输出；不自动授予采用或产品验收。 |

将最新 main 合入研究分支时，还修复了 source `#if` 与 CP12 缓存的交叉：旧 checkpoint 不记录源码选择阶段的 CT usage，因此遇到 source `#if` 时保守绕过 checkpoint，继续原管线；`expanded.selection.remaining` 的扣费必须保留。这个修复已包含在本轮**两臂基线共同部分**，不计为 CP15 的性能贡献。

独立集成审查发现了 CP15 新测试中 `Option<CoreExpr>` 解包遗漏；已修复后再执行远端测试。初审漏报和后续更正一起留存，没有把早期静审当作编译通过。

## 比较口径：相同的部分与仍有的差异

本次机器报告为 AMD EPYC 9V45，Linux x86_64；实际可用 CPU 只有 **[0, 1]**。正式编译计时两臂均约束到这两个 CPU，运行窗口固定 CPU 0；直接质量窗口的非评分产物构建也固定 CPU 0。前轮使用 CPU 0–3，故绝对时间和相对 Rust 比值都不能直接拼接。GitHub 物理宿主不由本任务独占。

ZIL 使用 LLVM/Clang/LLD **23.1.4**；Rust **1.99.1** 内置 LLVM **23.1.1**。二者不是同一份 LLVM 二进制。Rust 使用原生 CGU、链接器与 LTO 选择，并显式设置 `overflow-checks=on`、`panic=abort`、`debuginfo=0`、`embed-bitcode=no`、`target-cpu=x86-64`。ZIL 8/4 是显式研究请求，CLI 默认仍为 1/1；请求 4 jobs 不代表主机有四个 CPU。

Rust 未显式指定 LTO 时，在适用配置下会尝试单 crate 内的 ThinLTO；本轮主对照保留这种原生能力。ZIL 仍用其当前每单元优化实现，不声称后端优化范围完全相同。参见 [rustc codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html#lto)。

五语料依次为 tree4x2、tree32x8、tree256x8、tree1024x8、retained，权重保持 **5% / 15% / 30% / 30% / 20%**；四请求保持 cold/unchanged/private/public 的 **10% / 35% / 40% / 15%**。合成树占 80% 权重，不代表行业项目或全部 ZIL 语言特性。

完整 CLI 进程计时包含前端、当前 Verify、LLVM、链接与产物发布；输入准备、编译器本身构建和 oracle 执行在计时外。cold 表示新进程、无编译缓存，OS 页缓存保持暖。unchanged 仍是完整编译请求及产物发布，不是 Cargo 的无工作检查。

每个窗口固定三个 ABBA/BAAB 配对区块。先求区块内每臂两次观测的均值，再对每个语料/请求/臂取三个区块中位数，按原权重加权**绝对成本**，最后求 B/A；没有平均局部比值。区间重采样完整区块，仅作本窗口的探索性描述。

## 新旧 ZIL：0.89% 的小幅收益

两臂均为 O0、请求 8/4，source/object cache 开启，query 关闭。正式构建 240 次。

| 请求 | 基线 ms | 候选 ms | 候选/基线 |
| --- | ---: | ---: | ---: |
| cold_process | 663.884 | 665.145 | 1.001900 |
| unchanged | 411.800 | 406.843 | 0.987962 |
| private_edit | 431.679 | 426.566 | 0.988157 |
| public_edit | 448.935 | 446.622 | 0.994849 |
| 原权重混合 | **450.530** | **446.530** | **0.991120** |

加权 CPU 比值为 **0.993421**，墙钟探索区间 **[0.978760, 0.992873]**。最大树的混合耗时从 1163.755 ms 降至 1151.309 ms；tree256x8 比值 0.994794。但 tree4x2 为 1.003665、tree32x8 为 1.000583、retained 为 1.029168。最小树三个区块编译均略慢，不能称作全面改善。

全部 15 组旧新 O0“语料 × 快照”的记录 ELF 哈希一致。正式计时 ELF 已按原协议在 oracle 后删除，这里比较保存的记录哈希，并未声称重新读取所有临时 ELF；预检保留的 ELF 字节另有核验。它支持这些实际程序代码产物未变，不外推到所有输入，也不把编译等待时间的变化说成程序运行速度变化。

## 对照 Rust：冷编译持平，编辑重编仍有差距

下表每行来自独立冻结窗口，B/A 为 ZIL/Rust。两种语言对应源码、输入与 oracle 在计划中固定。

| 完整构建 | Rust 加权 ms | ZIL 加权 ms | ZIL/Rust | 探索性区间 | 解读 |
| --- | ---: | ---: | ---: | --- | --- |
| O0 cold，无编译缓存 | 464.036 | 463.711 | **0.999300** | [0.999300, 1.019041] | 基本持平；两区块比值大于 1，不能称领先 |
| O0 incremental，ZIL query 开启 | 343.299 | 412.005 | **1.200135** | [1.193725, 1.213970] | ZIL 慢 20.01% |
| O2 closed，ZIL 请求 8/4 | 958.343 | 1051.129 | **1.096820** | [1.017591, 1.112508] | ZIL 慢 9.68% |

三行的加权 CPU 比值分别为 **0.978758 / 1.433699 / 1.186609**。桥接窗口 query 关闭，这里的增量窗口 query 开启；两个结果不能拼成一个加速分数。

O0 cold 的四个较小语料仍快于 Rust，最大树却为 1.032582；它在绝对成本中占比高，使加权结果接近 1。增量差距更集中于较大项目：tree256x8 为 1.156027，tree1024x8 为 1.260360。

| 增量请求 | Rust ms | ZIL ms | ZIL/Rust |
| --- | ---: | ---: | ---: |
| cold_process，含缓存建立 | 688.521 | 667.289 | 0.969163 |
| unchanged | 289.926 | 299.785 | 1.034003 |
| private_edit | 313.132 | 432.652 | 1.381692 |
| public_edit | 318.129 | 448.602 | 1.410123 |

unchanged/private/public 的三个配对区块均更慢。CP12 只缓存完整相同输入，私有编辑仍会使完整前端检查点失效；CP15 降低复制成本，没有实现细粒度编辑失效。所有正式 ZIL 构建仍核验当前 Verify 恰好一次。

## O2 运行质量：小程序保留单单元产物

直接比较本轮基线 **closed O2 1/1** 与本轮候选 **closed O2 请求 8/4**。此表是原生运行时间，不是编译时间；B/A 大于 1 表示候选程序更慢。

| 语料 | 候选实际 CGU/jobs | 墙钟 B/A 点估计 | 三个配对区块墙钟 B/A | 新旧 ELF |
| --- | ---: | ---: | --- | --- |
| tree4x2 | 1/1 | 0.992689 | 0.992689 / 1.006379 / 0.991564 | 相同，6504 bytes |
| tree32x8 | 8/4 | 0.975919 | 0.995583 / 0.975919 / 0.991513 | 不同 |
| tree256x8 | 8/4 | 0.984625 | 0.986408 / 0.983473 / 0.984625 | 不同 |
| tree1024x8 | 8/4 | 0.978086 | 0.970428 / 0.992272 / 0.973871 | 不同 |
| retained | 1/1 | 0.994576 | 0.969248 / 0.991989 / 1.001315 | 相同，6592 bytes |

tree4x2 和 retained 记录专属 fallback `small optimized closed executable`，同时实际 units=1/jobs=1。只看到实际单单元不能单独证明 CP14 触发；这里也核对了专属原因及边界测试。

最小树的 CPU 点估计为 **1.001108**，逐区块墙钟与 CPU 方向混合，且代码字节相同，因此不能把小幅时间差称为运行加速。其余三棵树在本次三个区块中墙钟和 CPU 都低于 1。加权墙钟为 **0.981259**，CPU 为 **0.982071**，探索性墙钟区间 **[0.981259, 0.986711]**。

裁决为 **`NoStablePerCaseRegressionObserved_ThreeBlockExploratoryWindow`**。不把此结果升级为普遍非退化或生产采用。当前矩阵缺少旧 closed O2 8/4 的同配置产物，不能直接证明超过阈值语料相对旧 8/4 的分区和机器码逐字不变。报告也没有机器指令总数；冻结阈值由源码和实际执行的 2047/2048/2049 控制佐证。

独立 Rust/ZIL 原生窗口也保留：O0 cold、O0 incremental、O2 的加权运行比值分别为 **0.522542 / 0.529750 / 0.979855**。O0 最大树对总体影响很大，最小树仍略慢于 Rust；这些是语言产物对照，不是 CP15 造成的运行加速。

## 正确性和独立核验

真实 CI 通过 `cargo fmt --all -- --check`、`cargo check --locked --workspace`、以下定向 release 测试，以及三套 Python 基准工具控制。LLVM 相关测试显式开启 `llvm-api`，没有遗漏条件测试。

| 测试范围 | 实际通过 |
| --- | ---: |
| zil-types lib + project_lookup_cp10 | 18 + 6 |
| zil-backend-llvm lib + closed_executable_cp13 | 45 + 6 |
| driver：borrowed inference、closures、selection budget/selection、projects、query cache | 4 + 22 + 3 + 6 + 27 + 12 |
| Cargo 合计 | **149** |
| Python bench / rust_bench / runtime_bench 控制 | **8 + 26 + 18 = 52** |

10 个 Cargo harness 的失败、忽略、过滤和 measured 都为 0，Python 为 0 skipped。CP14 三个阈值分别在一个模型测试中循环执行；CP15 四项回归覆盖借用地址、实际改写、跨模块归一化、元数据及零预算错误；source `#if` 缓存旁路和当前 CT 限额也真实执行。

42 条外部命令均退出 0、无超时；编译器构建和测试先于测量，窗口串行。正式样本为 **600 次完整构建 + 240 次原生运行**，另有 100 次预检构建和 10 次直接质量产物构建，不进入计时分数。

三份独立报告见 [round1/reviews](../../../evidence/research/compile-performance/COMPILE4/round1/reviews)：

- 执行与完整性：核验清单中的 40,357 个文件及原 tar 的 40,358 个常规文件（含清单自身），字节、SHA 和模式全部吻合；交叉绑定提交、源码树、工具链和二进制。
- 编译/Rust 对照：未调用原聚合函数，独立核验 780 条记录、缓存/Verify/原始 JSON/oracle；七份摘要最大数值差 4.44e-16。
- 直接 O2 质量：独立核验 60 次原生运行、10 个构建和全部对象报告；58 个聚合/诊断值最大差 1.11e-16。该复算作者也曾实现 CP14，报告明确披露这一点；非作者源码审查由另一位审查者完成。

仓库交接检查返回 1，仍有 1,138 项历史问题、0 警告，与最新 main 的已有记录逐项相同，没有新增。本轮不将该检查写成通过。也没有运行完整 `cargo test --workspace` 或正式产品门。

## 证据保存与复现

本轮全部原始证据文件、计划、完整记录、工具版本、两份编译器、协议保留的预检/运行 ELF、输入、oracle、日志和收据保存在 [raw-evidence.tar.xz](../../../evidence/research/compile-performance/COMPILE4/round1/raw-evidence.tar.xz)。[ARCHIVE.json](../../../evidence/research/compile-performance/COMPILE4/round1/ARCHIVE.json) 保留归档身份和映射。

- XZ 大小：**9,930,828 bytes**；SHA-256：`ab3c783d01494c95f794855598e5e832f592e7b9994aa2634a545431aac6fc4a`。
- 解压后的原 tar 字节流：**364,175,360 bytes**；SHA-256：`a8d98528c59e7f00ed559d3bc2458f04dd86b27c2537a8d3eb63a2b5d9f5ad0f`。
- 原 Actions ZIP SHA-256：`bda15069c64693d6e9afbb0acd944e2c499d3bbc8495afc9713db5f16afa1d7b`。

为了降低临时磁盘占用，只把 gzip 包装换成 XZ；原 tar 字节流、头部和所有成员均保持，三方独立核验已完成。协调者回收重复 gzip 时，一位审查者的读取被中断；已切换到保留 XZ 并重新完成核验，运输层事件如实记入审计，未改写任何计时记录。解包工作副本用相同内容/模式的硬链接去重，原始归档不受影响。

重现本次源码应 checkout 被测提交 `0393660f7dc8414b0ab8c77cd3d576395f4fe095`，安装固定工具链，再按 [CP17-CI.md](CP17-CI.md) 执行新输出目录中的 prepare/preflight/measure 流程。不能将旧计划指向新编译器后复用旧成绩。归档解包后的绝对路径和 ctime 不同，审核通过保存的字节哈希与 `original_evidence_root` 映射完成，没有伪造原主机路径或时间。

## 下一项研究应解决什么

编辑重编是优先缺口：本轮私有/公开编辑仍分别慢于 Rust 38.17% / 41.01%。后续应研究受依赖约束的前端失效范围；当前完整输入缓存不能解决这一点，当前 Verify 仍必须保留。

已有只读分析还确认，当前前端会反复遍历全项目的空 enum/aggregate 声明表，形状解析仍无条件复制 CoreModule。这些是下一步可独立实施的候选，尚无 CP15 之后的专属阶段计时，不能把旧 Types/CT 阶段全部当作可节省成本。

O2 需要继续同时约束编译成本与运行质量。本轮只有两个 CPU，8/4 请求并不保证最优；自动工作线程预算、跨 CGU 优化和 Rust 原生 ThinLTO 的机制差异都需要新的预先冻结对照。不得事后改阈值、改权重，或只保留最快配置来覆盖这里的结果。
