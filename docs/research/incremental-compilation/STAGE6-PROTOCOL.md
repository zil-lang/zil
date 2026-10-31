# 第六阶段协议：候选筛选、增量归因与完整编译确认

本文登记第六阶段的来源和测量边界。早期协议中的 EE 分区导入方案先进入两区组初筛；后续新增原始语义候选缓存与 MergeFunctions 候选，分别冻结再测。初筛允许选择后续候选，但不能追认成六区组确认。下面除明确标注的已完成控制和初筛停止记录外，数字表示预定设计和请求数，不表示窗口已经完成；完成状态、失败和性能结果须由各窗口原始收据及后续报告给出。

## 来源与冻结身份

| 身份 | 本轮含义 | 不能代替的证据 |
|---|---|---|
| `59d8753dc` / 交付 `3d6babe17` | 上一阶段生产与已审交付，作为旧基线 | 旧报告的速度不是本轮同窗对照 |
| `ee786e7fd`（EE） | 反向依赖/等值屏障、函数初始化投影复用、显式 `--incremental-o2` 正文导入 | Session 查询效果、O2 导入效果分别测量 |
| `529bf8e20` | 显式可信本地 `--semantic-cache`，持久化原始 TypedProgram 候选；`bbd1df6b1` 另补集成控制 | 当前 Verify、完整函数类型增量、跨进程编辑命中不能据此省略或默认成立 |
| `43037eb7b` | 在上述生产上增加 `--function-merging off/after/before-and-after`，默认 `off` | 不等同 EE 的 `--incremental-o2`，也不自动开启封闭可执行文件合同 |
| `680e5b00e`、`58a753214` | 分别冻结早期协议工具及后续 MergeFunctions/真实源码工具 | 工具提交 HEAD 不等于实际链接的编译器版本 |

每个窗口同时保存生产源码/差异身份、工具及 probe SHA、实际编译器/rlib/可执行文件 SHA、完整命令、源文件身份和 CPU 亲和性。manifest 中的仓库 HEAD 只证明写收据时的工具工作树，不能单独证明独立 target 中的生产来源。复用旧 target 时必须连同其构建来源收据读取；不把同名二进制路径作为身份。编译器内部缓存身份另有其安装工具不变合同，不能以文件 SHA 充当缓存来源认证。

所有性能窗口排他运行；构建、测试、压缩和其他压力工作在窗口外完成。按窗口冻结后不得原地改生产、工具或重写失败记录。发生实现变更时另开窗口，不拼接两版样本。第六阶段归档独立保留，前五阶段证据不改写。

## 23 个生成形状：先做功能控制，再做 CLI 确认

`stage6_bench.py` 保留 23 个生成形状：单文件 128/512/2048/4096/8192 函数；总计 4096 函数的 8/64/512/1024 模块均分与偏斜分布；分支、泛型、CT、长函数体、类型实例；链和扇出依赖；总计 8192 函数的 8×1024、4×2048 和偏斜大模块；另有较小长体及类型分布。所有 helper 参与根结果，不能用一个大文件的结果替代这些模块分布。完整生成器和每次源码 SHA 是精确定义。

每个序列为 `cold → unchanged → early_edit → late_edit → restore`。前后编辑各自从基准源码生成，分别修改首/尾 helper；`restore` 恢复冷构建的逐字源码。每个区组、每个臂有独立对象缓存/Rust incremental 目录；显式启用语义缓存时再有独立语义目录，目录只在该序列内保留。cold 表示该序列的编译器缓存为空，不表示操作系统页缓存已清空。源码生成及原生执行在编译计时外，完整 CLI 的启动、身份计算、读写缓存、分析、验证、后端和链接均在计时内。

已有 `stage6-semantic/shape-sequences-functional` 是 529 生产的单区组 O2/closed/语义缓存功能控制：23×2 臂×5 请求，共 **230 请求，220 有效，10 个原预算拒绝**。有效数为 ZIL 105、Rust 115；`body-64x16` 和 `types-64x16` 的 ZIL 各 5 次 ResourceLimit 拒绝保留，均未到 Verify。105 个成功 ZIL 请求各有一次当前 Verify；21 个接受形状共 42 次语义 hit（unchanged/restore）、63 次 missing（cold/early/late）。220 个有效产物的完整 4 字节结果、退出码和 stderr 已核对。独立只读收据在 `stage6-merge-tools/shape-functional-audit.json`；此控制没有编译速度结论，也不是 43037 新候选的性能或正确性收据。

后续正式 CLI 采用 **6 个交替 AB/BA 区组**，每臂 3 次先跑，两 CPU 亲和性、8 CGU、ZIL jobs=2；Rust 使用实际 rustc、相同 O0/O2 等级、溢出检查、panic abort 和 8 CGU。每个 O2 全 23 形状窗口预定 1380 次完整 CLI 请求。必须显式传 `--blocks 6`，不能依赖工具默认区组数。当前原生结果需要完整 4 字节相等、退出 0、空 stderr；只比较退出码低位不足以证明等价。

默认预算保持不变。预算拒绝、超时、启动错误、Verify 错误和错误原生输出都保留；任何单元含无效请求时，不从其余成功样本生成有利比值。原先受限形状仍在全分布报告中，不能提高 limits 或删掉形状来获得“全面追平”。

## 两轮初筛与其后的六区组窗口

EE 初筛以 `ee786e7fd` 为候选，使用 `--incremental-o2`，每窗 2 区组、9 形状：large-128/512/2048/4096/8192、even-64x64、large-skew-8、body-16x16、types-32x16。三个窗口分别为默认导出对 Rust、显式 closed 对 Rust，以及默认导出相对旧 3d6 生产的隔离对照。每窗 180 请求，共 540；不启用原始语义候选缓存。它们用于发现收益与退化，不作正式六区组结论，也不把 closed 的收益写成默认导出收益。

43037 的新 MergeFunctions 初筛原计划 **8 形状、4 窗口、共 640 请求**：large-128、large-2048、large-8192、branch-64x16、body-16x16、large-even-4x2048、large-skew-8、types-32x16。每窗计划 2 区组、160 请求；同一实际编译器内比较 `after` 或 `before-and-after` 相对 `off`，各做默认导出和 closed。closed 窗口两臂都传 `--closed-executable`。这轮两臂均不开语义缓存，也不隐式开启 `--incremental-o2`，只筛选合并 pass 策略。生产和工具来源分别是 43037 与 58a；完整启动命令由 `run-merge-screens.py` 及窗口 manifest 绑定。

2026-10-31 初筛停止记录：default-after、default-before-and-after、closed-after 三窗各完成 160 请求；第四窗 closed-before-and-after 完成 55 个完整有效请求后，因灾难性退化由研究负责人停止。large-8192 首区组的 cold 为 100.609 秒、early_edit 为 98.733 秒、late_edit 约 100 秒，同窗 off/closed 对照约 2 秒；这些是已完成的单次请求观察，不是六区组中位数。第四窗第二个区组的 ZIL/cold 请求标为 `AbortedByResearchDecision`，无有效耗时，另有 104 请求 `NotRun`。因此总计 **535 个完成请求、1 个中止、104 个未运行**，不能写成“已完成 640”。完整原始行保持不变，停止原因、信号和命令保存在 `stage6/merge-screen-closed-before-and-after/STOP.json`；父执行收据 `stage6/merge-screen-execution.json` 保留第四窗退出 −15。未完成单元不生成性能比值。

这是观察初筛后作出的 NoGo 决定，不追认成预先设定的数值停止阈值。现有完整请求已足以拒绝推广该灾难性组合，无需为凑齐矩阵继续重复；`before-and-after` 不再晋升后续六区组编译确认候选。失败组合和已完成证据保留，其余预定窗口及质量控制的边界不变。

初筛后，在新窗口按 6 区组比较所选 O2 策略与 Rust，并明确开启原始语义候选缓存；默认导出与 closed 分窗。同编译器相对 off 的隔离对照用于判断策略本身，Rust 对照用于判断完整用户请求差距，二者不能互相代替。选择哪种策略必须在正式窗口开始前写入命令和收据；若根据初筛缩选，保留被淘汰候选及其运行质量退化反例。不能把初筛 2 区组补上 4 区组后冒称全新的 6 区组确认，也不能把多个优化窗口的比例相乘作为最终速度。

`--semantic-cache` 的命中只表示精确源码及目标、全部 limits、规则、构建输入和编译器身份匹配的原始候选复用。正常 hit 仍执行本请求 Analyze 和独立 Program Verify，不恢复 Verified 权限；私有编辑改变完整输入时仍可能 missing。该缓存不等于完整跨进程函数级增量。`--cache-dir` 保持原始源码缓存语义；与它组合、Analyze-only、emit 等不符合资格路径走原路径，不能把未命中路径宣称为语义命中。

## Session：两个独立归因窗口

同一份 `stage6_session_probe.rs` 分别链接两臂的冻结 rlib，正式运行前完成 prepare 和控制，正式窗口不再构建 probe。7 形状为 large-512、large-2048、even-64x16、even-512x8、even-1024x4、branch-64x16、body-16x16。每个新进程持有一个 Session，执行 `cold → unchanged → private → public → restore`；private 改函数体但不改签名，public 为首函数增加参数并更新调用，restore 恢复原输入。helper 在此生成器中是 pub，private 标签表示编辑未改变公共签名，不表示函数具有私有可见性。

| 窗口 | 两臂 | 可归因范围 |
|---|---|---|
| `session-db-isolation` | EE 对 EE 精确快照、仅将 `zil-db/src/lib.rs` 恢复到 3d6 的版本 | 隔离反向依赖/等值屏障的 DB 成本；两臂都保留 EE 初始化投影复用 |
| `session-final-vs-3d6` | 529 生产对 3d6 交付生产，仅完成准备及 126 控制，`SupersededPrepared`，没有正式性能行 | 不能当作完成窗口 |
| `session-scalar-vs-3d6` | ca79 生产对 3d6，已完成六区组 | 组合生产效果，不能全部归给 DB |
| `session-syntax-isolation` | ccf1 已撤回反向 DB 的生产对 ffe4 AST 保留实现 | 隔离真实 AST 复用的时间和常驻内存成本 |
| `session-syntax-vs-3d6` | 3d6 对 ffe4 | 当前组合生产的完整 Session 效果 |

每个正式窗口均为 check/O0、6 区组，各 840 请求、70 个比较单元及 420 次原生检查。probe 的 semantic_cache 默认 None，不启用 `--incremental-o2`；这些窗口都不测试跨 CLI 持久化命中，也不证明 MergeFunctions 的 O2 效果。Session 查询缓存最多 32 个原始槽位；逐请求保存完整报告、执行/命中事件、当前 Verify 和完整原生十进制输出。源码写入、快照、fresh 控制和原生执行在 `compile_project` 计时外。VmRSS 是请求后进程驻留量，不是单请求峰值，不与 CLI wait4 峰值 RSS 混用。

每臂、每形状另做 fresh 编辑/错误/恢复、取消后恢复及逻辑预算 N、N−1、fresh-N 控制，完整报告与 fresh 路径相符才进入性能窗口。函数初始化投影只缓存初始化子分析；其他 Analyze 工作与当前 Verify 的费用不据子查询命中数省略。失败、取消和恢复控制不是正常性能样本。

## O2 运行质量：单次构建与六区组运行分开

`stage6_merge_runtime.py` 保留原 opaque 生成器：512/2048/8192 算术、512/2048 分支、512 长函数体，共 6 族；getchar 提供运行时输入，循环调用全部 helper，固定 20000 轮及 seed 0/1/7/42/255。每次完整 4 字节结果与独立 Python 算术 oracle 核对，并要求退出 0、空 stderr。6 个正反交错区组测量同一产物的运行，不重新编译。

初筛前原计划含 before_and_after 两臂；发现其灾难性退化后撤回，运行质量确认改为显式 7 臂：43037 baseline、baseline_closed，ffe4 after、after_closed、incremental_after、incremental_after_closed，以及 Rust。新导入臂用于检查缩小优化单元是否牺牲运行质量。脚本仍保留历史默认臂，最终命令必须显式列出七臂；baseline 独立绑定 43037 二进制。每族每臂只构建一次：若全跑为 42 次构建和 1260 次运行观察。一次构建耗时只作诊断，不能称为六区组编译性能。

ZIL 构建需当前 Verify=1，保留源码、工具和编译器身份、产物 SHA/大小及 `nm --extern-only --defined-only` 的完整全局定义符号记录。该 nm 表不是动态导出表，也不能单独证明所有 ABI/地址合同。默认导出与显式 closed 的优化权限不同，分别报告；不能将减少可见函数的收益归到默认合同。编译、运行、超时、错误输出和因失败构建而跳过的观察都保留。候选即使编译更快，若运行退化也必须同时展示，不能直接晋升默认。

## 14 套真实配对源码：冷/暖完整请求

`stage6_real_pair_bench.py` 使用既有 14 套配对：app、stream、slices、byte_io、byte_buffer、byte_buffer_owned、resources、resource_closures、error_cleanup、owned_component、package_ui、retained_ui、update_ui、helper_ui。逐字复制实际源码并绑定 SHA。每个产物检查固定 15 输入：空串、`! D S U O M E T L 0 1 2 3` 各单字节，以及 `12as+!-q\0\x80` 字节串；与独立 Rust O0 oracle 比较完整退出码、stdout、stderr。保留 app 的 Rust `--check` 调用约定。程序合法返回非零可以等价，信号/超时/启动失败不能作正确结果；部分程序忽略输入，因此 15 输入不等于 15 条独立执行路径。

每个 O2/合并策略/导出合同独立冻结窗口，6 个交替 AB/BA 区组，每臂 `cold → unchanged`，按序列隔离对象、语义和 Rust incremental 目录。正式命令显式 `--mode O2 --semantic-cache`，用重复 `--zil-arg` 传合并策略。每窗预定 336 次完整编译请求；全部成功时对应 5040 次产物输入观察，另有 14 次 O0 oracle 构建及 210 次 oracle 输入观察，后者不计入正式编译时间。构建与原生失败完整保留，不能以 runner 返回成功代替逐行有效性检查。

这些是实际项目内的小程序，补充生成分布；不是大型工业应用抽样。早期工具的 `/bin/false` 故障控制和旧 529 的小功能控制仅证明收据保留及基本验证路径，不证明新 MergeFunctions 候选已经有效或变快。工具说明与控制来源见 [stage6_merge_tools.md](../../../tools/research/incremental-compilation/stage6_merge_tools.md)。

## Verify、图方法与统计解释

正常 ZIL 成功样本每请求必须有一次当前 Verify；不把 Rust 的编译流程记成同一个 Verify 指标。语义缓存损坏重建或候选在 Analyze/Verify 被拒绝后的恢复控制按实际事件记账：若已做一次失败 Verify，再 fresh Verify，则真实计数为 2，不能重置报告伪装成 1。取消不重试；Verify 成功后下游失败不重跑链接。此类恢复是故障控制，不纳入普通 hit 性能样本。

旧 076 完整资源联合关系、局部域/BDD/事件支持分解，zit JS 内核适配，以及生产 CGU/语义查询，是不同实验。图研究需完整独立 oracle 和输出等价；不支持类型和预算失败明确保留。zit 适配的精确环检测、换边、等值屏障和事务控制必须与实际源码调用图回放及生产 Session 证据分开。修复适配后的负收益也保留，不把 JS 原使用场景的高度策略泛称原内核 bug，不以少量重算节点比例冒称完整编译加速。来源、已完成研究结果及边界见 [STAGE6-ALGORITHMS.md](STAGE6-ALGORITHMS.md)，不移植其影子内核时间作生产 CLI 时间。

主统计单位是固定形状/模式/请求/导出合同的同窗两臂中位时间之比；CLI 记 ZIL/Rust，隔离与 Session 记候选/基线，均越小越快。它不是逐对比例中位数，也不是所有请求混合的一个中位数。若另给配对 bootstrap 区间，须绑定完整区组、方法与重采样参数，并标为探索性；2 区组筛选无正式置信或产品门结论。保留 RSS、全部失效单元和不利点；失败单元不静默进入几何均值或消失。不同窗口、不同合同和不同优化策略各自报告，不能跨窗拼接速度。

本阶段只对冻结源码、输入分布、选项和这台机器的编译及运行效果负责。六区组和扩大分布提高可复核性，仍不能推出全工业样本的普遍优势；Rust 对照也不能替代安全、语法或全部运行性能的产品验证。四目标未测部分与产品门保持原定义。最终报告应引用本协议、精确冻结身份和完整原始表，并清楚区分完成的结果、筛选观察与仍待测项。

## 早期 ffe4 确认计划（由下节的新整合身份替代，未运行）

在 Session 和 LLVM 插件功能审查完成后，先运行上述七臂运行质量窗口。CLI 确认选用 ffe4（scalar promotion、当前项目 AST 复用、原始语义候选缓存；反向 DB 已在 ccf1 撤回），显式 `--function-merging after --semantic-cache`。默认导出与 closed 各运行全 23 形状、6 个 AB/BA 区组；另对默认导出加 `--incremental-o2` 运行全分布，以检验优化分区对编辑的影响。每个窗口独立 1380 请求，不能跨窗拼接比例。若运行质量否决导入候选，记录理由并保留其功能/初筛观察，正式窗口不会默默换成别的选项。14 真实配对在默认导出、after、语义缓存下另做 6 区组 O2 冷暖请求。

这些策略仍为显式选项；after/语义缓存的最终结果不得宣传为无参数默认行为。生产 CLI 与隔离 LLVM 修正插件是不同身份；插件只比较同一完整研究路径的原版 clone 与修正版，不能将 API→opt 迁移或工具检查差异归于算法收益。最终报告同时保留代码运行质量、预算拒绝及全部不利单元。

## 主分支整合与四项 LLVM 修正后的确认计划

本节在正式窗口开始前替代前述 ffe4/七臂计划。主分支 f257 已并入专项；当前规范摘要为 `b4caffbf123b1915079c9275d8a93e9534dc19a5dcf59ef098cf80a51d9474f4`，ZIL 两臂共同生产源码为 `888af53a0a7389d10a508250059db6b631c61e4a`。历史窗口保持原源码、规范与二进制身份。新确认不是将旧窗口补成六区组。

四项 LLVM 修正分别为：保持入口块的 JumpThreading 合并；在 CFG 已稳定的路径调用 SSAUpdaterBulk，并按完整定义块/使用块集合复用其精确结构计算；LVI 的 Value→缓存块反向索引；O2 在 IPSCCP 后首次昂贵 InstCombine 前调用原有 SimplifyCFG。组合包含四个上游翻译单元，原版臂也以相同设置重编译这四个单元。两个 ZIL CLI 仅另外带各自的研究管线/缓存身份串；仍通过原有 LLVM API 执行。固定 LLVM 23.1.4 源码、SDK、公共头与类布局，完整重建入口为 `toolchain_combination/rebuild.py`。

| 冻结项 | 原版 | 候选 |
|---|---|---|
| LLVM 实验臂 | `four-tu-original` | `entry-ssa-v4-lvi-cfg` |
| libLLVM SHA256 | `7046ae9083e553ddcccaa5b8148ff090f9582d6b16f205dfb50cb4fe345f7dc6` | `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98` |
| CLI ELF SHA256 | `fb97c19b751136a28a647fdc50df67a9612c740c5b7d97a1fe6bbd0c6da95e25` | `35eece4722e193dd8a286236ab6f0c483108753ae8ffd2bc65bdd8a2540ae81e` |

完整来源、实际加载器记录、包装器、管线标签与工具 SHA 以 `combined-v1-pair/api-pair-receipt.json` 及每个窗口执行收据冻结。Rust 固定 1.99.1（提交 `58cf8f9c35b355199727627d07ae34a4e2846d67`，自带 LLVM 23.1.1）。不能仅凭 `--version` 等同这两种 LLVM 工具链。独立限定正确性通过后方可进入正式窗口；远端官方和候选 LLVM 各执行完整测试，远端结果不当成本机性能样本。

重新检查了新工具链下的 early MergeFunctions：算术大例 after/early 生成程序相同，诊断未见新优势；scalar/closed/early 仍有约 17.86 秒的 IPSCCP 单阶段成本。因此正式策略继续显式 `--function-merging after`，旧 early 失败和新交互控制都保留。这个选择没有把默认 `off` 改成 `after`。

23 形状正式计划增加一个同源 LLVM 隔离窗：两臂均开启自己的对象缓存、语义候选缓存和 after，不能只给候选语义缓存而把差值归因于 LLVM。另有候选对 Rust 的默认导出、closed、默认导出加 incremental-o2 三窗；共四窗，各 1380 请求。`stage6_confirmation.py` 校验实际 pair 身份并逐窗执行。14 真实配对另有 336 请求。所有数字在此仍是计划，完成/失败/停止另由收据判断。

23 形状中大量 helper 调用使用编译期已知输入，所以另补与运行质量样本一致的六族 opaque 完整编译窗口：512/2048/8192 算术、512/2048 分支及 512 长函数体；输入来自 getchar，固定 20000 轮。仍为六个 AB/BA 区组和五请求序列，前后编辑真正改变首/尾 helper 正文常数，恢复逐字原源码；每个产物检查五种输入的完整结果。每个两臂窗口计划 360 请求，分别测同源原版/候选隔离，以及候选对 Rust 的默认、closed、incremental-o2。构造源码和 oracle 在计时外；编译、身份检查、缓存 I/O、当前分析/Verify、后端、链接都在完整请求内。原版慢例也保留，不能用候选阶段时间替换原版完整时间。

运行质量改为九臂：430/off 默认与 closed；888/原版 LLVM/after 默认与 closed；888/候选 LLVM/after 默认与 closed；候选加 incremental-o2 默认与 closed；Rust。`stage6_toolchain_runtime.py --prepare-only` 先构建 54 个产物，核实际编译器进程加载的库和原生完整 oracle；准备耗时只作诊断。之后 `--run-prepared` 在排他窗口执行 1620 个原生观察（六族×九臂×五输入×六区组），不重复编译。旧七臂脚本和旧 v1 身份保持历史用途，新 config 明确原版/候选变体和标签命名空间。

必须同时报告代码大小、运行性能、编译峰值 RSS、对象/语义缓存命中和全部不利单元；原版与组合的程序不保证逐字一致，等价输出及运行质量需独立实测。Pass 热点诊断和单次准备构建不能代替上述完整请求结论；当前还不能据此宣称普遍追平 Rust 或完成函数级语义增量。

### 正式运行质量后的增量策略停止记录

上述 1620 次运行观察已完整完成，全部输出有效。独立复算确认原有有界导入分区的运行质量显著退化：incremental/候选的默认导出几何均值约 5.3933，closed 约 9.5819；8192 算术族约 326 倍。因此该未修改策略不晋升“同等 O2 质量”的完整编译确认；原计划 23 形状的 1380 请求和六 opaque 族的 360 请求均在启动前标记 `NotRun`，合计 1740，不计成执行失败或完成窗口。决定及精确证据身份见 `confirmation-incremental-original-nogo.json`。这是观察质量结果后的决定，不能追认为预先冻结的阈值。

已完成的准备、运行、旧编译初筛和不利结果完整保留，其他隔离、默认、closed 和真实源码窗口继续。增量研究继续追查完整调用闭包与分区间优化损失，修正候选必须另冻结源码、构建和运行质量证据，不能替换原策略或拼接本窗结果。组合 LLVM 本身与该显式分区选项分开判断。
