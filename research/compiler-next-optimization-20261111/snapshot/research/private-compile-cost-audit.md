# Private ThinLTO：完整请求编译开销只读分解

## 结论

新 private 两 case 的运行质量已通过，但**现有研究包装器的完整编译请求显著慢于主线**。当前证据首先支持把同一请求内反复校验固定工具身份、保存与反汇编诊断的开销独立出来，保留所有真实模型、LLVM 验证及原预算证明，再重新实际测量。不能从旧请求中扣除若干耗时就声称新编译器更快。

数据来自 `out/next-optimization/private-runtime-v1`；协议 SHA256 为 `208adf6f5a3e904182afdaf1ef1c6b30a173fb559400ab14d05b2f59e4803a44`。本审只读取既有 JSON/文本及固定源码，未新增编译、LLVM/native 执行或计时。逐叶记录路径与 hash、聚合值见 `research/private-compile-cost-evidence.json`，可按其中源文件重新求和。当前是两个 base 的各一次 clean 编译观察，不是正式编译性能矩阵或多点编辑数据。

## 一、完整请求与实际工具

| 现有观察，毫秒 | N64 | N256 |
| --- | ---: | ---: |
| A 完整请求 wall / CPU | 1255.692 / 2224.391 | 4955.840 / 8773.109 |
| B 完整请求 wall / CPU | 8285.686 / 14240.383 | 14630.546 / 22469.956 |
| Rust 完整请求 wall / CPU | 826.768 / 1062.860 | 2453.920 / 3660.671 |
| B 八次 prelink：wall **总和** / CPU 总和 | 317.940 / 304.633 | 1097.983 / 1084.457 |
| B 一次 thin-link：wall / CPU | 515.540 / 929.288 | 1862.978 / 3637.678 |
| B 32 次独立 LLVM verify：wall **总和** / CPU 总和 | 300.034 / 265.457 | 522.297 / 508.618 |
| B 24 次 dump/dis：wall **总和** / CPU 总和 | 113.091 / 97.441 | 254.131 / 247.056 |
| B 65 个实际工具叶的 CPU 总和 | 1596.819 | 5477.809 |
| B 完整 CPU 减工具叶 CPU：**未分解包络** | 12643.564 | 16992.147 |

每个 unit 的 prelink wall 范围为 N64 35.725–42.638ms、N256 119.958–181.350ms。两个 worker 并行运行，八个 wall 总和不是关键路径，不能与最终链接直接相加当完整 wall。薄链接计时本身已经包含 clang driver、lld、ThinLTO 多 backend、插件检查/文件写入、postlink O2、机器码发射和 native 链接，现有记录无法继续精确拆开这些阶段。

完整请求 `wait4` CPU 已包含被编译器等待的 descendants；65 个工具叶互不包含，只有它们之间可求和。不能再把完整请求 CPU、thin-link CPU、插件或 LLVM 阶段总和叠加；同路径 `process-attempt.json` 与 `raw-metrics.json` 是同次观察的两份记录，也不能重复计数。未分解包络包含 Python、hash、文本解析、文件复制、ZIL frontend/lower/planner 和其他父进程工作，不能全标成 SHA。

RSS 是各 `wait4 ru_maxrss`，不是进程树同时峰值，且本轮明确存在 fork/launcher floor；N64 A/B/R 都为295500KiB，不足以支持它们实际内存完全相同。

## 二、研究包装器的确定重复工作

`research/thinlto-private-imports/clang_wrapper.py:main` 的每次入口都先调用 `common.validate`。一次请求共有八个 compile wrapper 和一个 link wrapper，因此重复九遍：

- `scripts()` 重新扫描/hash 全部冻结 Python/C++；所有工具重新读取全文 SHA，合计每遍 **748539944字节**，其中 clang282507120、lld213124752、opt200048624字节。
- 插件构建收据校验又读取一次其 compiler，即同一个282507120字节 clang；仅这两组每遍至少1031047064字节、九遍至少 **9279423576字节**。这只是源码可确认的逻辑读取量，不声称等于物理磁盘 I/O，更没有独立 hash 耗时样本。
- 另有基线/候选构建日志、插件源码/构建日志/收据、协议及 Rust proof 重复读取；八个插件 backend 又各自绑定 manifest/SO/Rust/prelink witness。小文件校验必要性与大工具重复读取应分别看待。

`run.py` 在启动整个研究前已经调用一次完整 `validate`；这次发生在六个编译请求计时之外。各工具版本命令、最终 native 测试和结果审查也不在每个 compile 请求计时内，不能从某个 compile wall 再扣一次。

24 个 dump/dis 是每 unit 一次 bcanalyzer summary dump、一次 prelink dis、一次实际 `.3` dis。它们的全文输出是研究见证，同时现有 Python 依赖这些文本证明 summary、Required/Allowed、actual AE、only-direct、原始成本等；**在当前实现中不能只删除命令还声称保留了证明**。将来可以由同一真实 LLVM 对象中的等价检查替代全文 dump，但那是新增实现及验收工作。

保存量也有实际边界：每请求保存75个 LTO临时文件，N64约1.763MB、N256约6.613MB；八个 input.ll 合计1.384/5.586MB、prelink.ll1.488/5.984MB，插件16份 before/after 文本2.100/8.422MB。它们可能参与父进程成本，但目前没有独立耗时，不能任意给复制/打印分配剩余数值。插件 before/after 全文现被 wrapper 用于严格逆名/linkage相等证明，不属于可无替代删掉的日志。

## 三、需要保留的真正工作

候选源码 `thinlto-complete-visibility/crates/zil-backend-llvm/src/build.rs` 保留当前 Verified 输入、完整 lower、checked/scalar 原策略、fold前完整 model preflight、fold后完整 partition-input preflight、原 Complete planner预算与owner proof、Disabled canonical分区、八个当前 unit preflight。两组整图 preflight分别为 N64 8.774/7.196ms、N25645.527/45.427ms；unit preflight总和9.023/41.557ms。这些数字只是明确埋点，完整 planner/clone/front-end未单独计时。

每 unit 外部 `opt -passes=verify` 检查原始IR及真实 prelink，link后检查八份真实 `.3.import` 和八份 `.4.opt`，共32次；插件 hook自身变更前后再调用 verifyModule。原默认 LTO 验证也未用这些记录替代。下一 screen若承诺“全部检查保持”，应继续实际执行这些检查；进程启动和重复parse可以今后在同一LLVM进程合并，验证检查点和失败行为仍需保留，不能仅留producer receipt。

八次真实 Clang ThinLTO prelink、同一 ThinLTO postlink O2及 after merging、原 target/PIC/code model、完整 import可见性、原copy预算、唯一canonical owner和最终 native link，都是机制本身。当前质量Pass是 N64 B/A0.738734、N2560.910565，全部54个native输出正确；它只覆盖固定纯函数base，不能扩大到地址/global状态，也不证明八case或many-edit质量已过。

主线 A 的既有八个API object report显示 builder总和44.974/208.166ms、O2 optimize731.860/3423.087ms、emit1091.711/3912.865ms。这支持主线LLVM优化/发射是实质成本，也说明无需把本次工作包装成又一次 emitter微优化；这些阶段时间与B外部进程计时边界不同，不能直接相减得收益。

## 四、下一最小实现边界

**先做研究执行器的请求级身份去重，单独screen。** 最小写集限新独立 runner/common/wrapper，不改冻结旧v1、生产Rust、插件变更或预算；完整工具/构建/source/script身份在受控请求启动处统一校验一次，八个子wrapper只核当前请求绑定与unit输入，末端再次检查身份未变。工具需处于明确冻结/不可变的快照或具有稳定前后stat/hash的拒绝合同；不能只凭任意环境字符串跳过工具身份。保留同32次verify、完整Rust proof、prelink/.3实际覆盖、插件exact8收据及before/after全文差异检查。这样首先移除确定存在的九遍约9.28GB逻辑大文件读取，而不把验证算法删掉。诊断输出是否精简要与此分开，以免一口气失去归因。

这仍是研究pipeline screen，不能仅因wrapper更薄就当成已完成生产编译器优化。真正产品化需要把checked exporter、真实prelink及postlink/私有导入接入后端明确入口，复用LLVM进程/内存对象并保留全部验证点及失败顺序；当前pure研究准入不能直接成为通用默认路径。现有 `build.rs::compile_units`、`UnitInput` 和链接阶段是明确接入点；默认/off路径与原API object cache应保持不变。完整LTO bindings/API设计比几行wrapper去重更大，应等独立screen证明有余量再施工。

**many-edit最小后续机制是精确 canonical-unit prelink cache，而非先启用普通 lld object cache。** 主线 `object_cache.rs` 缓存完整native object；新路线可独立缓存每个 canonical unit 的已验证 prelink bitcode/summary/export，键必须包括实际机器内容、声明/类型、target/CPU/features/PIC/code-model、LLVM/Clang实现、完整prelink policy、merge策略和cache格式。命中仍先对当前完整请求和每unit机器预检，校验缓存bytes，再执行本请求Required/Allowed/owner与实际导入检查及LLVM验证；新miss按原O2 prelink生成，成功发布后原子入cache。复用的是已预优化body，不复用旧Verified能力。

这里有两个已查到的真实限制：`model.rs::render_module` 在每unit打印全局 verified-fingerprint，直接hash完整文本会使一个编辑改变所有unit键；应复用现有机器内容key原则，只排除明确的授权注释，不能删真实 operands或header字段。canonical函数编号和unit分配也可能因插入新定义而变，不能承诺修改k文件就仅kunit miss。当前仅有clean base，无法报出真实hit率。

即使prelink命中，root导入的callee body变了，它的postlink优化/native object仍需失效。真正LTO对象cache还需要plugin SO/完整policy/预算owner域与导入依赖身份隔离；官方cache key不绑定plugin环境，当前每次fresh cache与exact8 hook门不可顺手取消。请求级manifest含随机id/路径，如何与稳定对象身份分离也要单独设计；这一步不是本次最小screen。

Rust/Blaze可借鉴的是细粒度artifact、明确依赖键和同一已预优化body复用。已pin Blaze `ae1a7a7a1771c1658173ee4829392c40b4f1d4cc` 的 `release.rs` 将own/export保存为 `Arc<[u8]>`，无debug时共享buffer；compile直接parse/link这些既有bytes，再run postlink。它仍做module.verify与O2，且导入实际为AE，不能将其描述成当前private方案相同。照搬逐函数分区会改变prelink/inliner顺序与旧布局，不是本次最小低风险改动。

## 退出条件

任何请求级身份去重若不能证明当前tool/source/policy绑定，或保不住完整Verify/preflight/LLVM验证/原预算、actualvisibility及exact8 hook门，应停止。完成新screen后只报告实际完整wall/CPU和原始失败格，不从本表推算“净编译时间”；质量门、原整体加权门与多文件/多点/并发覆盖须分别保留名称和范围。
