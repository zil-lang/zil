# Compile5 独立执行审查准备清单

**状态：Prepared_AwaitingArtifact。本文件不是执行通过收据。**

审查者：`/root/shape_opt`。只读检查保留证据，不运行 Cargo、编译器、原生输出或 benchmark runner，不计算速度比、置信区间、汇总得分或采纳结论；性能算术由 `industry_audit` 独立审查。

## 冻结身份

| 项目 | 预期值 |
| --- | --- |
| GitHub Actions run | `41086451087` |
| 远端受测提交 | `951471d5a6e26e15de96e0bfaf91cd3e537a3dd9` |
| 本地冻结提交 | `187eae4f6bb085f0d0d42765f047b7f60add6bed` |
| 两者完整 Git tree | `6ab7522524122c6d21738329ab7dd885c6bbc75c` |
| 候选 crates tree | `b583bc62e4aa39b206afb27f76f2f8262120fe9d` |
| 基线提交 | `a715f828261ff4b6ef9c8125623064360745aae4` |
| 基线 crates tree | `0221fa705e8dc926c54f6b85f2dc9ef3af98559a` |
| 冻结 manifest | `tools/research/compile-performance/compile4-run.json`，`run_id=compile5-round1` |

准备阶段已经用只读 Git 命令确认上述完整 tree 与 crates tree。实际 artifact 的身份尚未核验。

## Artifact 到达后的必查项目

1. **远端执行身份。** 独立获取 GitHub run 与 artifact 原始 JSON；核对 run ID、attempt、workflow、branch、head SHA、completed/success、未过期、artifact 名称与 SHA-256 digest。保存原 ZIP，独立重算其完整字节 SHA 与大小，不把下载者的记录代替当前重算。
2. **源与清单绑定。** 逐字节比较 artifact `manifest.json` 与受测 commit 中的 manifest；核对 `result.json`、setup `commit.txt`、manifest-validation、runner SHA。把 artifact 中完整 `git ls-tree -r HEAD` 输出与本地受测 commit 的 Git 对象逐字节比较，确认本地冻结 tree 与远端受测 tree 一致。
3. **所有原始文件。** 核对 `files.json` 路径集合，无缺项、额外项或逃逸路径。每个列出路径分别打开、流式重算大小和 SHA-256，不按共享 inode 跳过重复路径。随后独立流读原 tar，核对每个常规成员的原始 bytes/SHA/size 与解压文件 POSIX mode，包含 `files.json` 自身；拒绝重复、symlink、hardlink 和其它特殊成员。目录 mode 不作为文件 mode 的替代。
4. **两个编译器构建。** 核对基线 worktree 的实际 revision/cwd；两条构建命令应为 `cargo build --release --locked -p zil-cli`。从两个冻结 commit 的 Cargo.toml 核对 `zil-cli` 默认 `llvm-api` 及 driver/backend feature 传递；确认没有 `timings` 默认 feature。核对两个独立 target 目录、构建日志、实际保留 CLI 的 bytes/SHA 与可执行 mode，并将各 plan 的二进制身份绑定到这两个文件。
5. **实际测试。** 逐组核对 manifest 的 package、lib、targets、features 与实际完整 argv；不得存在名称过滤、`--no-run` 或漏掉 `--lib`。将 stderr 的 `Running` harness 与 stdout 每个 `running N tests`、逐条测试结果、完整 result 五项计数匹配。要求每个限定 harness 非空且 failed/ignored/measured/filtered 均为 0。检查 CP14、CP15、CP18、CP19 和 checkpoint/selection 预算控制的具体名字在实跑成功记录中存在且唯一。
6. **Python 控制。** 解析实际 `Ran N tests` 与完整 `OK`，与冻结源的 AST test 方法数交叉核对，记录 skipped/failure 情况。当前源定义分别为 19、26、19 项；这只是预期数量，不能预填为已执行。默认 unittest 日志只有点，不把方法名列表声称为日志逐名覆盖。
7. **执行顺序。** 核对完整 43 条外部命令的标签、连续编号、退出码、timeout 与 stdout/stderr hash。每条记录的 `started_unix + seconds` 不晚于下一条开始；最后构建/检查/测试完成后才准备计划与测量。8 个测量命令各绑定自己的 records/complete/plan，且全部串行、带 quiet-window。独立读冻结代码，确认命令与测量循环等待子进程结束；不把时间顺序当作物理宿主独占证明。
8. **工具链和计划。** 核对 LLVM 安装收据与每个安装命令日志，安装器与受测源 SHA 相同；toolchain-check、Rust 实际 `-Vv`、各 plan 中的工具身份一致。LLVM/Clang/LLD 23.1.4，Rust/Cargo 1.99.1，Rust 内置 LLVM 23.1.1。外部工具大文件未随 artifact 保留，明确只能交叉核对收据/计划，不能声称重新散列了安装后的工具二进制。

## 冻结测试选择

| Package | Library | Integration targets | 显式 features |
| --- | --- | --- | --- |
| zil-types | 是 | project_lookup_cp10、shape_resolution_cp18 | 无 |
| zil-ct | 是 | 无 | 无 |
| zil-backend-llvm | 是 | closed_executable_cp13 | llvm-api |
| zil-driver | 否 | borrowed_inference_cp15、query_cache_cp12、closures_frontend、projects、impl219_budget、impl219_selection、ct_reuse_cp19、aggregates、receiver_regression、impl105_ct_enum、impl094_ct_preflight、impl088_ct_preflight | llvm-api |

共 18 个预期 Cargo harness；实际通过数量等待 artifact。`cargo check --workspace` 与限定测试不等于 `cargo test --workspace` 或正式产品门通过。

## 核验脚本

同目录 `COMPILE5-INDEPENDENT-EXECUTION.py` 只使用 Python 标准库和只读 Git 命令；不导入仓库 runner。唯一写入是指定到 artifact 外的新审查 JSON/MD，并拒绝覆盖已有审查结果。任何缺项或异常均产生 `FailOrIncomplete_NoExecutionApproval`，保留失败，不自动降级为通过。

待实际路径给出后执行，以下仅为命令模板：

```bash
python3 recovery/reviews/COMPILE5-INDEPENDENT-EXECUTION.py \
  /ABSOLUTE/ARTIFACT/compile4-evidence \
  --repo /workspace/scratch/fec853c597a1/zil \
  --zip /ABSOLUTE/ORIGINAL.zip \
  --run-metadata /ABSOLUTE/INDEPENDENT-RUN.json \
  --artifact-metadata /ABSOLUTE/INDEPENDENT-ARTIFACT.json \
  --out /workspace/scratch/fec853c597a1/recovery/reviews/COMPILE5-INDEPENDENT-EXECUTION.json
```

准备验证仅包括脚本语法/帮助，以及将独立日志解析器用于 Compile4 已有日志：重算得到既有 10 个 Rust harness、149 项通过和三组 Python 的 8/26/18，共 52 项。没有重跑这些测试，没有由此给 Compile5 出具通过结论。

## 必须保留的结论边界

- 记录只提供顶层命令绝对开始时间与单调时钟耗时；样本行没有绝对开始/结束时间。报告必须把时间证据范围写清楚，以源码中的同步测量循环补充执行关系，不能声称观察到了不存在的逐样本绝对时间戳。
- 原 ZIP、原 tar 与 extracted 文件均应在本项核验完成前保留；如果任何原件已回收，按实际缺证据报告，不把历史 hash 说成重新核验。
- CP18 的 Owned inference → resolver Borrowed 具有现存项目功能覆盖；两个 Owned-first 组合的完整 CoreProgram 回归断言尚未增加。本轮审查不据此改动冻结源码。
- 本项不评估性能收益、Rust 对比数值或采用优化的结论，也不把限定测试通过扩大为全部语言行为已被证明。
