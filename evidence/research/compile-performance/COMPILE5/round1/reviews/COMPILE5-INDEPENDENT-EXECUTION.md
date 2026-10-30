# Compile5 独立执行核验

**结论：本次 artifact 完整性和限定 CI 实际执行核验通过；不构成全 workspace 测试或正式产品门通过。状态为 `Pass_TargetedExecutionAndArtifactIntegrity`。**

受测提交 `951471d5a6e26e15de96e0bfaf91cd3e537a3dd9`；基线 `a715f828261ff4b6ef9c8125623064360745aae4`；Actions run `41086451087`。
审查者 `/root/shape_opt` 独立从 GitHub 读取 run、artifact 和 job 元数据，确认本次 run `completed/success`、attempt 1、受测 head 与上述候选一致。Artifact 为 `12993022904`，名称 `zil-compile4-41086451087-1`，未过期；job 的准备、构建测试、归档和上传步骤均成功。API 快照及读取来源保存在同目录 `COMPILE5-GITHUB-*-INDEPENDENT.json` 和 `COMPILE5-GITHUB-FETCH-PROVENANCE.json`。

核验脚本首轮全部通过，未修改原始证据。首次输出保存在 `COMPILE5-INDEPENDENT-EXECUTION-first.log`；完整机器可读结果见同名 JSON。本审查没有运行 Cargo、编译器、原生程序或性能估计器。

## 原始文件与归档

- `files.json` 列出 **40,360 个文件**，全部分别打开并流式重算大小和 SHA-256，共 **330,305,540 字节**；没有缺失或未列文件，清单自身除外。实际存在 2,402 个不同 inode，核验没有因硬链接共享 inode 而跳过任何路径。
- 原 tar 含 **40,361 个常规文件**（包括 `files.json`）及 292 个目录。全部常规成员的原始 bytes、SHA-256、大小及解压文件 POSIX mode 一致：42 个 `0755`，40,319 个 `0644`；没有其它成员类型或缺项。
- 原 ZIP **68,939,397 字节**，独立重算 SHA-256 与独立取得的 GitHub artifact digest 一致；ZIP 内每个常规文件还与已解包内容比较。原 tar.gz **68,150,903 字节**，独立重算 SHA 与 sidecar 一致。

| 原件 | 独立重算 SHA-256 |
| --- | --- |
| ZIP | `18f2acc10acc9c9e508a92fb06beb63f5f9d331fe34a945342860c56ab894fcc` |
| tar.gz | `5ad5bffeddf7e8402b770e0009e842456ce61b1806d229c172617ca50b206010` |
| files.json | `162f6a39b23c0d7e17515276dda66ced25dcab35ca9ede49bb4ac81403d5985e` |

## 源码、构建及工具链

受测候选与本地冻结提交 `187eae4f6bb085f0d0d42765f047b7f60add6bed` 的完整 Git tree 均为 `6ab7522524122c6d21738329ab7dd885c6bbc75c`。候选 crates tree 为 `b583bc62e4aa39b206afb27f76f2f8262120fe9d`，基线 crates tree 为 `0221fa705e8dc926c54f6b85f2dc9ef3af98559a`。本审查读取指定 Git 对象，没有把后来 HEAD 上的文档提交当作受测源码。

Artifact manifest 与受测提交中的 manifest **逐字节一致**，setup commit、manifest-validation、runner SHA 和完整 `git ls-tree -r HEAD` 清单也一致。Manifest SHA 为 `215649fdd0b0b03aa724d44e614bfdb31cba53e928403bd35f4f38b1274c9ce5`。

两条实际编译器构建 argv 均为 `cargo build --release --locked -p zil-cli`，使用独立 target 目录；冻结 Cargo.toml 的默认 feature 为 `llvm-api`，通过 driver 传递到 backend，未默认开启 `timings`。基线 worktree 的创建命令、构建 cwd 和 revision 对应一致，保留 CLI 文件的 bytes、SHA 与可执行 mode 已重算。

| 编译器 | 实际字节数 | POSIX mode | SHA-256 |
| --- | ---: | --- | --- |
| baseline | 7,317,512 | `0755` | `3da310e2179a30bff2c6d520215b5e109ac346edb7baea13892ee026fe873dec` |
| candidate | 7,422,280 | `0755` | `56009a9cd67d268f41953b5e065984f050af015af6edc49a2c2188d24d2cfd17` |

LLVM/Clang/LLD 为 **23.1.4**；Rust/Cargo 为 **1.99.1**，实际 `rustc -Vv` 记录的内置 LLVM 为 **23.1.1**。安装器与受测源 SHA 一致，安装命令日志全部匹配收据，工具身份在检查结果、安装收据和冻结计划之间一致。外部工具大二进制未保留，因此不声称对安装后的 LLVM/Rust 工具重新散列。

## 实际 Cargo 测试

| Package | Harness | 通过 | 失败 | 忽略 | measured | 过滤 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| zil-types | `unittests src/lib.rs` | 18 | 0 | 0 | 0 | 0 |
| zil-types | `tests/project_lookup_cp10.rs` | 6 | 0 | 0 | 0 | 0 |
| zil-types | `tests/shape_resolution_cp18.rs` | 6 | 0 | 0 | 0 | 0 |
| zil-ct | `unittests src/lib.rs` | 26 | 0 | 0 | 0 | 0 |
| zil-backend-llvm | `unittests src/lib.rs` | 45 | 0 | 0 | 0 | 0 |
| zil-backend-llvm | `tests/closed_executable_cp13.rs` | 6 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/aggregates.rs` | 10 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/borrowed_inference_cp15.rs` | 4 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/closures_frontend.rs` | 22 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/ct_reuse_cp19.rs` | 4 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/impl088_ct_preflight.rs` | 12 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/impl094_ct_preflight.rs` | 8 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/impl105_ct_enum.rs` | 12 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/impl219_budget.rs` | 3 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/impl219_selection.rs` | 6 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/projects.rs` | 27 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/query_cache_cp12.rs` | 12 | 0 | 0 | 0 | 0 |
| zil-driver | `tests/receiver_regression.rs` | 4 | 0 | 0 | 0 | 0 |

**共 18 个限定 harness、231 个通过；failed、ignored、measured、filtered 全部为 0。** 每条 `test … ok` 均与所属 harness 的 Running/running/result 计数匹配。完整 argv 与冻结 manifest 的 lib、targets、features 一致，没有名称过滤；backend 和 driver 两组显式使用 `llvm-api`，所有测试组均为 release。

CP18 六项、CP19 source 四项及 type-replay 三项、materialization 精确预算/诊断顺序控制，以及 CP14、CP15、checkpoint/selection 控制的具体测试名均在成功记录中实际出现。名称与受测 commit 中的测试源码绑定，不用文件存在或编译成功代替执行。

Python 控制实际为 `test_bench.py` **19**、`test_rust_bench.py` **26**、`test_runtime_bench.py` **19**，合计 **64 项通过、0 skipped**。完整 unittest 总数与冻结源 AST 的测试方法数一致；默认日志只有点和总数，不把源方法列表声称为逐名日志。

## 执行顺序

**43 条外部命令**全部退出 0、无超时，stdout/stderr hash 与收据一致，连续编号和预期顺序完整。`cargo fmt --all -- --check` 与 `cargo check --locked --workspace` 均成功。

记录的命令区间无相邻重叠；最后编译器构建/检查/测试结束早于第一次计划准备，全部测量命令在此前置工作完成后串行执行。八个窗口的 records/complete/plan/summary 身份相互绑定，分别保留 **600 行构建记录及 240 行原生运行记录**，行状态均通过。本项只核验执行与记录完整性，没有计算性能比或采纳结论。

## 范围限制

- 这是运行结束后的保留证据审查，没有重新运行编译器或测试。限定 harness、workspace check 和格式检查通过，不扩大为 `cargo test --workspace` 或正式产品门通过。
- 命令区间末端由收据中的 UNIX 开始时间加单调时钟耗时推得；样本行没有绝对开始/结束时间戳。串行关系同时依据绑定到受测源码的同步 runner 控制流，不声称宿主机器完全独占或没有其它进程。
- 工具链大文件与 LLVM 原始下载未在本地保留。工具链部分限于安装/检查日志和冻结计划间身份交叉核对，CLI 及原始 artifact 文件则已实际重新散列。
- Feature 选择由冻结源默认值和完整 argv 核对；本审查没有另行执行 Cargo feature 解析。
- 性能收益、逐 case 退化、Rust 比较数值和是否采用优化，由另外的独立性能审查负责。
