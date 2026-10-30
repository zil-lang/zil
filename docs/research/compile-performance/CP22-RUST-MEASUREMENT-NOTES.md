# CP22 与 Rust 的 O0 native incremental 对照：冻结前注意事项

## 审计结论与范围

**现有 `rust_bench.py` 可以承载完整进程、原生可执行文件的 Rust 对 CP22 增量比较，前提是明确保持 native 配置、同一四请求缓存序列、完整外部计时和原生输出核对。** 这里的 `native` 表示保留 Rust 工具链自身的 CGU、LTO、默认链接器策略；它不表示使用宿主 `target-cpu=native`，也不表示两种编译器的内部并行策略相同。

本次只读源码并整理口径，未运行 prepare、preflight、编译器或 benchmark，未修改 runner。读取主树提交为 `474e0d84a0e2a941e318533f8558441731d738fe`。源文件根为 `/workspace/scratch/4ab361722127/zil`：

| 文件 | SHA256 |
|---|---|
| `tools/research/compile-performance/rust_bench.py` | `4aafc427a4b4752a59362e17c2c5e6e6e514de335b9554a665aa313b951da116` |
| 同目录 `bench.py` | `10a7cd80819589dfceacdc2a885e71e84c723b8c8cdf66c48682934e96308ace` |
| 同目录 `runtime_bench.py` | `95b90e7929b87c4a8c5111aefaf956a40c0ac0e85466191298f4747bb7626855` |

CP22 本身的增益必须先由旧/新 ZIL 对称 query-cache 窗口隔离；Rust A / CP22 ZIL B 的窗口回答的是这组完整工具链配置谁的总成本较低。不能把“相对 Rust 的比值变化”单独归因于 CP22，也不能将与旧窗口的差值当作已消除宿主噪声的直接消融。

## 1. 冻结时必须保留的参数

对应 `rust_bench.py:100–106,149–172,205–230,513–531`。

| 参数/策略 | 本轮 O0 native incremental query-cache 应保持 | 原因 |
|---|---|---|
| 两臂身份 | A 为固定 Rust；B 为精确 CP22 源提交与其 release binary | 必须保留 source commit、crates tree、构建收据与 binary hash 的映射，不能仅凭目录名认定版本 |
| `--profile` | `O0` | 同时决定 Rust `opt-level=0` 和 ZIL `--opt O0`；O2 是另一窗口 |
| `--cache-mode` | `incremental` | 决定完整四请求序列及两臂各自缓存；默认是 cold，不能漏传 |
| `--query-cache` | 显式开启 | ZIL 这一路同时承载现有完整前端 checkpoint 与 CP22 解析复用；不能拿关闭 query 的旧结果来代表此配置 |
| `--rust-profile` | 显式 `native` | 不向 Rust 强加 ZIL 的 CGU 数；`matched` 是另外的机制控制 |
| `--rust-lto` | 显式 `native` | 不追加 `-C lto=off`；不得只展示改变 Rust 原生策略后较有利的比值 |
| `--rust-linker` | 主窗口不传，保留工具链默认 | 指定 Clang/LLD 是另一个配置；“同一个 linker”不是 native 窗口现有事实 |
| `--closed-executable` | 主窗口保持关闭，除非另立独立控制 | 开启会改变 ZIL 符号 internalization，不应混入 CP22 单因素归因 |
| `--units / --jobs` | 与本轮 ZIL 消融一致时应显式 `8 / 4` | runner 默认是 `1 / 1`，漏传就换了比较口径；native 模式下这两个数不代表 Rust 的内部 CGU/线程数 |
| affinity | 明确冻结，例如延续本轮的 `--cpus 0,1` | 两臂编译进程继承同一 CPU 集合；不能将 4 jobs 写成独占四核 |
| cases/weights | 五语料全选，5/15/30/30/20% | `--case` 可做 smoke，但会重归一化权重，不能与全矩阵总量混用 |
| blocks/seed | 冻结 3 或 5 区块及种子；不能看结果后换 | 当前默认种子为 `20261030`；CP21 用的是 `20261029`，两者不能误写为同一冻结计划 |

建议计划名称明确包含 `O0/native/incremental/query`。`prepare` 要求 runner/source 已提交且工作树干净，并在新目录生成计划；它不会覆盖旧计划。若补充 runner 的 parse-cache 收据字段，必须先提交修改，再生成新计划，不能修改已经冻结的脚本。

### Rust 实际命令约束

`command()` 目前为 Rust 固定以下参数：

```text
--edition=2024 --crate-name zil_cp08_ref -Awarnings
--target x86_64-unknown-linux-gnu
-C target-cpu=x86-64 -C opt-level=0 -C debuginfo=0
-C overflow-checks=on -C panic=abort -C embed-bitcode=no
-C incremental=<该序列的独立目录>
```

native 模式不追加 `-C codegen-units=...`，`rust_lto=native` 不追加 `-C lto=...`，没有显式 rust-linker 时也不追加 linker 参数。应保留全部这些现有参数，不能临时关闭 overflow checks、改变 panic/bitcode/debug 配置，或把 `native` 理解为添加 `target-cpu=native`。

### ZIL 实际命令约束

ZIL 调用是 `build` 到完整可执行文件，固定 `--json --backend llvm-api --clang <固定工具> --opt O0`，并带冻结的 `--codegen-units/--codegen-jobs`。incremental 会同时添加 `--cache-dir` 和 `--object-cache`；本窗口再添加 `--query-cache`。不允许由命令默认值悄悄改后端或绕过 Verify。

## 2. 缓存序列与“冷”的含义

对应 `rust_bench.py:275–340`，以及 `bench.py:28–29`。

每一个 ABBA/BAAB 位置创建一个新工作目录，两种语言、不同语料和不同区块之间不共享编译器缓存。同一位置依次执行：

| 请求 | 源快照 | 允许沿用的缓存 |
|---|---|---|
| cold_process，权重 10% | base | 该序列为空；OS 页缓存未清 |
| unchanged，35% | base | 上一请求的同一臂缓存 |
| private_edit，40% | private | 同一序列中已建立的缓存 |
| public_edit，15% | public | 前面 base/private 请求留下的缓存 |

Rust 每次都启动新的 rustc，复用同一路径下 `-C incremental=`；ZIL 每次都启动新的 CLI，复用 source/object/query 缓存。不是 Cargo 判断无需运行编译器的 no-op，不是持久 Session，也不是清空系统页缓存后的物理冷启动。四个请求的冷/热成本都应保留，不能只展示 unchanged 的最佳点。

两种缓存的算法、粒度和落盘成本可以不同；比较的是冻结后各自完整工作流的总成本，不要求内部数据结构相同。每序列结束才删除工作目录，不能中途额外清一方的缓存或预热其中一方。

## 3. 计时范围是完整编译进程

`one_build()` 在外部计时前完成源快照同步、移除上次输出和源文件 hash 记录，再调用 `common.legacy.timed()`。因此计量包含新编译器进程、源码发现、缓存查读/解码/费用检查、前端、当前 Verify、lowering/codegen、链接和进程完成；语料复制、原生正确性运行与计划核验是对称的计时外准备/核对。

这对 CP22 特别重要：解析缓存作用于源码发现，不能用 ZIL 内部 `report.seconds` 或单阶段 duration 代替完整 wall。全请求是否获益由外部 wall/CPU 决定；命中本身不说明命中成本小于重解析。

CPU 是 wait4 的 user+system，包含被父进程回收的子进程。RSS 口径是单个进程的最大值，不是同一时刻整个进程树内存峰值。formal `report_ok()` 要求 `timings is None`，避免把另一个阶段计时配置混入本窗口。

`--quiet-window` 是执行者明确承诺暂停本轮其他构建/测试的门，不是自动获得物理宿主独占。需要保持真实安静窗口；共享宿主和 OS 缓存因素仍须在结果中说明。

## 4. 当前 runner 已有的公平性与完整性约束

`prepare/load` 会冻结并复核 runner/helper、语料/输入/oracle、编译器、Clang、Rust sysroot 库和工具、LLVM shared library 等额外工具、可能的链接器及其选择路径、环境、affinity 和配额。它保留 symlink 的调用路径及解析后的文件身份，不仅记录一个版本字符串。参考 `rust_bench.py:49–97,177–201`。

Rust sysroot 中预编译的 std/alloc/core、rustc_driver、内置 LLVM 与 bundled linker 被计入身份清单，但它们的构建时间不在本次应用编译里。系统 libc/crt 和动态装载器属于共享宿主依赖。ZIL 报告保留实际外部链接 argv；Rust 默认链接器候选被保留，但脚本没有逐条追踪 rustc 内部的实际 linker argv，不能仅从体积猜测它用了哪一个 linker，更不能声称两臂必然使用同一 LLVM/LLD。

逐请求须确认：成功退出、无超时、产生完整输出、源 hash 前后与冻结快照一致、所有原生输入的输出和独立 oracle 一致。ZIL 另须 `verification_runs=1`、llvm-api 后端正确、cold 对象缓存零 hit、unchanged 对象缓存全实际单元 hit，以及冻结的 closed-executable 模式一致。原始 stdout/stderr 有 hash，summary 会重新读取并核回，不靠一条 pass 布尔值盲目计分。

任何失败、缺行、超时、工具/源码/log 变化都会使整窗不能计分。不得删除慢样本、以成功重试替代失败条目或在原目录覆盖补跑。若磁盘问题导致缓存写失败，即使编译返回成功，也必须保留报告中的失败状态，不能把该请求描述为成功使用 CP22 缓存的证据。

## 5. CP22 需要额外保留的机制证据

**当前 `rust_bench.py` 的 `row.reported` 没有包含 `parse_cache`。** `one_build()` 第 292 行只摘取 status、verification_runs、native_codegen、commands、disk_cache、query_cache、timings；但完整 stdout JSON 原文及 SHA256 已保留，`validate_build_rows()` 也会重新读取原文。因此完整性能计量不因此失效，CP22 的命中归因仍需额外核对。

可以在冻结前明确附加一个只读原始报告审计：逐条从 B 臂 stdout 提取 `parse_cache`，保留以下字段及按语料/请求的分布：

```text
requested, hits, misses, corrupt, quota_misses, stored,
session_hits, parsed, reason, write_reason
```

字段来自 CP22 的 `crates/zil-driver/src/parse_cache.rs`。需区分磁盘 hits、Session hits、实际 parsed 数和因为预算不适用而重新解析的 quota misses；不能把所有 skipped/parsed 状态都记作增量命中，也不能用 `query_cache.requested=true` 代替实际 hit。

whole-input checkpoint 与 per-module parse cache 是两个层次。现有 `report_ok()` 第 259–268 行要求完整 query cache：unchanged 为 hit 且不重新 stored；cold/private/public 为 miss 且 stored，ct_steps/nodes/functions 为非负整数。**private/public 整体 checkpoint miss 与未改变模块的 parse hit 可以同时成立**，不得为了让解析缓存“看起来生效”而放松旧 checkpoint 的首错、费用或状态检查。

当前计划文字及 CLI help 仍将 `--query-cache` 描述为 CP12 的完整输入 checkpoint。这是 runner 文案的旧范围；CP22 结果报告应明确该旗标现还启用了模块解析复用，不能沿用“只有整个输入相同才有任何复用”的描述。若不改 runner，可在冻结计划配套说明与额外 raw-report 审计中写明这点；若修改文案/字段，则应在冻结前提交并使用新 hash。

旧/新 ZIL 的对称 CP24 桥接已经在 `bench.py` 收录并核对 `reported_parse_cache`。应先使用该桥接隔离 CP22 的贡献，再把 Rust 对照视为另一独立窗口，保留全部结果。

## 6. 原生等价与统计口径不能扩大

五语料总权重中 80% 是合成调用树，20% 是 retained 消费者。retained 在 Rust 使用 Vec、ZIL 使用 ByteBuffer；Rust 预编译标准库与 ZIL/宿主 libc 的角色不同。当前有效输入的 native oracle 可以证明这些路径输出符合独立模型，不证明两种语言在 panic、分配失败、所有错误输入或全语言特性上等价。保持现有 overflow/panic 参数只能固定比较，不足以抹平全部语言行为差异。

完整五语料、三配对区块、四请求时，构建矩阵为 240 条正式请求、40 条预检请求。正式每行还做原生正确性核对；这些运行不计入编译 wall，也不作为运行性能分数。Rust runner 的预检另外保存 base 可执行文件，执行 empty-input startup 和 long-input warmup；它们同样不进入构建分数。

聚合沿用既定规则：区块内同臂均值 → 跨区块中位数 → 按语料与请求权重累加绝对成本 → ZIL/Rust 比率。A=Rust、B=ZIL，B/A 小于 1 才表示该指标下 ZIL 成本较低。保留 wall、CPU、五语料和四请求全部正负方向；不得平均百分比，也不得把比率与 CP21/CP23 或历史不同缓存模式的窗口相乘。

当前 bootstrap 只对完整配对区块做描述性的墙钟区间，三/五区块且共享宿主，不能据此升级产品门。CPU 没有同样区间时须明确，不借用墙钟区间。`measure-runtime` 是另一类样本，使用预检保存的 base binaries、单 CPU、完整原生进程及文件 I/O；仅有构建窗口不能声称运行性能也改善。

## 7. 本轮工具链与结果表述

目前 Rust 1.99.1 自带 LLVM 23.1.1；ZIL 实际使用 LLVM/Clang/LLD 23.1.4，且本地 libLLVM 是官方静态组件派生的共享库。应冻结实际文件及其 hash 并直接说明版本差异。native Rust 对照应保留本来的工具链策略，不为追求字面“同 LLVM”而静默换 Rust 后端、CGU 或 linker。

最终结果应写成“在冻结的 O0、native Rust、incremental、ZIL query-cache、指定 affinity/CGU/jobs 和这五类语料下，完整请求成本的观察结果”，并附候选 source/crates tree、二进制/toolchain hash、完整 argv、原始记录和非作者复核。CP22 是否采用、ZIL 对 Rust 的观察比值、运行性能及产品门属于不同结论，应分别给出证据状态。
