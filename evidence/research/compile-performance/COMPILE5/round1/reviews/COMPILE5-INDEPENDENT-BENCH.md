# COMPILE5 正式 CI 矩阵独立审查

## 结论与身份

**通过：8 个正式窗口的统计量、完整性和归档证据复核无阻断。** 本结论绑定 GitHub Actions run `41086451087`、artifact `12993022904`，仅适用于冻结的五语料、请求权重和本次机器环境；产品门仍为 `NotRun`。

- 受测候选：`951471d5a6e26e15de96e0bfaf91cd3e537a3dd9`。
- 受测候选 crates tree：`b583bc62e4aa39b206afb27f76f2f8262120fe9d`。
- 对照基线：`a715f828261ff4b6ef9c8125623064360745aae4`。
- 基线 crates tree：`0221fa705e8dc926c54f6b85f2dc9ef3af98559a`。
- 非作者审查者：`/root/industry_audit`；未改候选源码、原始证据或冻结计划，未重新执行 ZIL 编译器、Cargo 测试或计时负载。
- 审查程序：[COMPILE5-INDEPENDENT-BENCH.py](COMPILE5-INDEPENDENT-BENCH.py)。机器可读结果：[COMPILE5-INDEPENDENT-BENCH.json](COMPILE5-INDEPENDENT-BENCH.json)。

源码与 runner 身份均使用精确受测提交，不使用后续只增加计划/审查材料的主树 HEAD。独审程序不导入或调用任何原 runner 的 `summarize` / `aggregate`。

## 证据完整性与独立算术

逐文件复核 40,360 个清单文件、3,270 组原始 stdout/stderr 日志、2,560 项 oracle / startup / warmup / runtime 检查。正式样本覆盖 600 条编译记录及 240 条运行记录，所有原始行保持完整，无失败、超时或缺失后重新加权。

独立重算 424 项正式数值，和发布 summary 的最大绝对差为 `3.3306690738754696e-16`，属于浮点加法次序差异。脚本也验证计划、预检、完成收据、CLI 命令、源快照、oracle 输出、编译器产物身份以及当前 Verify/cache 约定。

正式估计器保持如下顺序：每个 block/case/request/arm 的两次样本先求均值，随后对三个 block 求中位数，再对绝对耗时应用请求与语料权重，最后求 B/A。运行窗口没有请求权重。没有采用平均比值或几何均值，也没有选择事后更有利的估计器。

语料权重固定为 tree4x2 / tree32x8 / tree256x8 / tree1024x8 / retained = **5% / 15% / 30% / 30% / 20%**。四请求窗口固定为 cold_process / unchanged / private_edit / public_edit = **10% / 35% / 40% / 15%**。三个区块按 ABBA / BAAB / ABBA 运行，case 顺序使用冻结 seed `20261030`。

区间严格复算冻结 runner 的整块 bootstrap：bridge 与 Rust 窗口各 2000 次，direct quality 5000 次，分位下标保持原实现。只有三个独立区块；这些区间为描述性研究结果，不是产品置信保证。

## 实际机器与工具链

正式 CI 的实际编译 CPU affinity 是 **`[0, 1]`**，运行样本固定 **CPU `0`**。CPU 型号记录为 **AMD EPYC 7763 64-Core Processor**；型号名称不表示该 runner 获得 64 核。ZIL 请求 8 CGU / 4 jobs，但可用 affinity 只有两个 CPU，不能表述为在四核环境完成本轮实测。

- OS：`Linux-7.0.0-1017-azure-x86_64-with-glibc2.39`；架构 `x86_64`。
- Rust：`rustc 1.99.1 (58cf8f9c3 2026-10-13)`；`-Vv` 记录其内置 LLVM 为 **23.1.1**。
- ZIL 使用的 LLVM / Clang / LLD：**23.1.4**。这是 Rust 原生工具链对照，不是同一 LLVM 小版本的机制对照。
- Rust 保持原生 CGU / LTO 政策；主结果没有切换为 LTO-off。双方均显式使用 O0/O2、x86-64、无调试信息及既定溢出检查配置。
- `cpu_max` 与 `memory_max` 记录为 `null`，不据此宣称无限配额；没有物理宿主独占证据。

两份 ZIL 编译器均在同机使用相同工具链与 release 设置构建。编译/限定测试先于串行测量；cold 指无相应编译缓存的新进程，不表示清空 OS 页缓存。

## 正式编译结果

以下为加权 wall time。bridge 的 A/B 是旧/新 ZIL；Rust 窗口的 A/B 是 Rust/新 ZIL。比值小于 1 表示 B 耗时更低。

| 窗口 | A 毫秒 | B 毫秒 | B/A | B 耗时变化 | 描述性区间 |
|---|---:|---:|---:|---:|---|
| 旧/新 ZIL O0 bridge | 596.154 | 586.761 | 0.984244 | -1.576% | [0.979499, 0.988789] |
| Rust / ZIL O0 cold | 817.748 | 669.229 | 0.818380 | -18.162% | [0.809501, 0.826212] |
| Rust / ZIL O0 incremental | 424.156 | 556.831 | 1.312799 | +31.280% | [1.304954, 1.320060] |
| Rust / ZIL O2 closed | 1711.839 | 1793.171 | 1.047511 | +4.751% | [1.039244, 1.047511] |

本轮可直接归因于候选组合的完整同机窗口是 O0 bridge：source + object cache、无 query cache、两臂同为 8 CGU / 4 jobs，完整 CLI 编译加权耗时降低 **1.576%**，约 **9.393 ms**。对应 CPU 耗时比为 **0.988762**。本矩阵未把 CP18 和 CP19 分别消融，因此不能独立宣称某一个子优化贡献了多少提速。

新 ZIL 的 O0 cold 编译在这五语料的固定权重下比 Rust 少用 **18.162%** wall time；O0 incremental 仍多用 **31.280%**，O2 closed 仍多用 **4.751%**。这不支持“全面达到/超过 Rust 编译效率”的结论。不得把它与旧轮另一台机器的 Rust 比值差异拼接成 CP18/CP19 的收益。

### 各语料的编译比值

| 语料 | 新/旧 ZIL O0 | ZIL/Rust O0 cold | ZIL/Rust O0 incremental | ZIL/Rust O2 closed |
|---|---:|---:|---:|---:|
| tree4x2 | 0.993606 | 0.674676 | 0.598404 | 0.668869 |
| tree32x8 | 0.989539 | 0.745221 | 0.798894 | 0.790980 |
| tree256x8 | 0.991949 | 0.802531 | 1.100533 | 1.052588 |
| tree1024x8 | 0.982097 | 0.829556 | 1.451649 | 1.060377 |
| retained | 0.988019 | 0.610572 | 0.536489 | 0.576300 |

### O0 incremental 各请求的 Rust 对照

| 请求 | 加权 Rust 毫秒 | 加权 ZIL 毫秒 | ZIL/Rust |
|---|---:|---:|---:|
| cold_process | 1166.029 | 908.206 | 0.778888 |
| unchanged | 326.870 | 405.633 | 1.240962 |
| private_edit | 350.832 | 583.798 | 1.664037 |
| public_edit | 352.104 | 603.467 | 1.713892 |

private/public 编辑仍是显著差距，尤其 tree1024x8。其数值来自本轮完整四请求序列，不能用 O0 cold 优势替代这些请求的结果。

## 运行质量结果与机器产物边界

以下运行样本包含完整 native process 启动和文件 I/O，固定 CPU 0；编译、正确性检查、startup 诊断与 warmup 都不计入运行样本。

| 窗口 | A 毫秒 | B 毫秒 | B/A | 描述性区间 |
|---|---:|---:|---:|---|
| Rust/ZIL O0 cold base | 132.500 | 75.633 | 0.570814 | [0.570814, 0.574316] |
| Rust/ZIL O0 incremental base | 138.713 | 76.472 | 0.551300 | [0.551300, 0.555726] |
| Rust/ZIL O2 closed base | 39.568 | 38.999 | 0.985633 | [0.985176, 0.988130] |
| 旧 closed 1/1 → 新 closed 8/4 | 39.443 | 39.103 | 0.991395 | [0.990742, 0.992233] |

这些是固定权重的加权结果。O0 cold runtime 的 tree4x2 与 tree32x8 分别为 Rust 的 **1.345720** 与 **1.288164** 倍；O2 closed runtime 的 tree4x2 为 **1.028762** 倍，不能把总加权结果描述为每个程序均更快。Rust runtime 仅来自 base 预检产物，不覆盖编辑后产物的运行性能。

直接 O2 quality 的两臂 codegen 配置分别为旧 closed 1/1 与新 closed 8/4。加权比值 **0.991395** 没有显示本窗口的总体运行耗时退化，但不构成 CP18/CP19 单独影响运行性能的同配置归因。

**ELF 证据分开表述：** O0 bridge 的 **15 个 case/snapshot 保留的 ELF SHA-256/size 记录一致**；冻结 runner 在每个请求序列后删除了 bridge ELF 原件，因此本审查没有声称重新逐字节复核这些 ELF。runtime 窗口实际保留的 ELF 原件则已独立重新读取、校验 SHA-256 和大小，并和预检、startup/warmup、正式样本中的可执行文件身份绑定。

## 本次限定验证执行情况

本次 CI 实际通过 **231 项 Rust 限定测试**、**64 项 Python 基准工具控制**，并通过 cargo fmt 检查及 cargo check --workspace。以下计数来自本次原始日志，并根据 Cargo stderr 的实际 Running target 顺序映射到 stdout 结果，不使用旧轮或作者分树计数。

| crate / target | 通过 | 失败 |
|---|---:|---:|
| zil-types / lib | 18 | 0 |
| zil-types / project_lookup_cp10 | 6 | 0 |
| zil-types / shape_resolution_cp18 | 6 | 0 |
| zil-ct / lib | 26 | 0 |
| zil-backend-llvm / lib | 45 | 0 |
| zil-backend-llvm / closed_executable_cp13 | 6 | 0 |
| zil-driver / aggregates | 10 | 0 |
| zil-driver / borrowed_inference_cp15 | 4 | 0 |
| zil-driver / closures_frontend | 22 | 0 |
| zil-driver / ct_reuse_cp19 | 4 | 0 |
| zil-driver / impl088_ct_preflight | 12 | 0 |
| zil-driver / impl094_ct_preflight | 8 | 0 |
| zil-driver / impl105_ct_enum | 12 | 0 |
| zil-driver / impl219_budget | 3 | 0 |
| zil-driver / impl219_selection | 6 | 0 |
| zil-driver / projects | 27 | 0 |
| zil-driver / query_cache_cp12 | 12 | 0 |
| zil-driver / receiver_regression | 4 | 0 |

Python 控制为 test_bench 19、test_rust_bench 26、test_runtime_bench 19。限定测试提供本轮相关路径和边界的执行证据，不能替代全仓正式产品门。安全与 Verify/cache 检查保持；运行/内存目标中的全进程树峰值内存没有在本轮得到证明，wait4 单进程最大 RSS 不等价于同时峰值。语言表达能力与完整接受集也不能由这五语料或这些测试计数推出。

## 归档来源与重放

主要原始证据定位：编译/运行样本在 `windows/<window>/build/records.jsonl` 和 `windows/<window>/runtime/records.jsonl`；对应配置和实际环境在 `windows/<window>/plan/plan.json`；候选测试的命令、stdout/stderr 绑定在 `logs/013-candidate-tests-0.json` 至 `logs/016-candidate-tests-3.json`。本报告的全部正式数值也保留于同目录的独审 JSON，便于逐项追溯。

完整 ZIP 的 SHA 与 GitHub artifact metadata 一致；内层 tar 与 CI 自带 SHA 收据一致；tar 内的 files.json 与当前解包后的 files.json 一致，再由该清单核验每个证据文件。解包时对相同内容/模式的文件去重为 hardlink，不重建原始 ctime；本独审比较保留字节和记录身份，不把解包后的时间戳当成原机器时间戳。

- `zip_sha256`：`18f2acc10acc9c9e508a92fb06beb63f5f9d331fe34a945342860c56ab894fcc`。
- `tar_sha256`：`5ad5bffeddf7e8402b770e0009e842456ce61b1806d229c172617ca50b206010`。
- `archived_file_manifest_sha256`：`162f6a39b23c0d7e17515276dda66ced25dcab35ca9ede49bb4ac81403d5985e`。
- 独审脚本 SHA-256：`82f4992ec41e3e078cb1bce6b4bea7902e6f6d5b31617a75bc85bba379215c75`。

[GitHub Actions 本轮运行](https://github.com/zil-lang/zil/actions/runs/41086451087)

```bash
python3 recovery/reviews/COMPILE5-INDEPENDENT-BENCH.py "$ZIL_COMPILE5_EVIDENCE_ROOT" \
  --candidate 951471d5a6e26e15de96e0bfaf91cd3e537a3dd9 \
  --artifact-zip recovery/compile5-run1.zip \
  --artifact-metadata recovery/compile5-artifact-metadata.json \
  --out recovery/reviews/COMPILE5-INDEPENDENT-BENCH.json
```

本环境的普通 sandbox 与 escalated 进程使用不同 IPC/tmpfs 视图；首次普通进程看不到已解包路径，产生的路径缺失尝试已保留为 `COMPILE5-INDEPENDENT-BENCH-attempt01-path-unavailable.json`。随后通过只读访问实际证据目录完成全部审查。没有重试或替换任何性能样本，未向 /dev/shm 或原始证据写入文件。
