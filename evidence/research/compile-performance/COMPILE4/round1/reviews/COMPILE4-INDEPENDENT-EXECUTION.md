# Compile4 首轮独立执行核验

**结论：此次 artifact 完整性与定向 CI 回归核验通过；不构成全 workspace 测试或正式产品门通过。**

- Actions：[41068328117](https://github.com/zil-lang/zil/actions/runs/41068328117)，artifact `12982919612`，GitHub 独立读取状态为 `success`。
- 候选：`0393660f7dc8414b0ab8c77cd3d576395f4fe095`；基线：`64072793a0f41994f216a6207bf49ee1e2d224f8`。提交 manifest、setup commit、完整 Git tree 清单、两个实际 CLI 的 bytes/SHA，以及五个计划的 runner/helper/tool 身份均一致。
- `files.json` 中 40,357 个文件逐路径流式核验全部通过，共 329,813,038 字节；无缺失和未列文件（清单自身除外）。原 tar 中含清单的 40,358 个常规成员另行核验 bytes、SHA、POSIX mode 全部一致。

## 实际回归计数

| Package | Harness | 通过 |
| --- | --- | ---: |
| zil-types | `unittests src/lib.rs` | 18 |
| zil-types | `tests/project_lookup_cp10.rs` | 6 |
| zil-backend-llvm | `unittests src/lib.rs` | 45 |
| zil-backend-llvm | `tests/closed_executable_cp13.rs` | 6 |
| zil-driver | `tests/borrowed_inference_cp15.rs` | 4 |
| zil-driver | `tests/closures_frontend.rs` | 22 |
| zil-driver | `tests/impl219_budget.rs` | 3 |
| zil-driver | `tests/impl219_selection.rs` | 6 |
| zil-driver | `tests/projects.rs` | 27 |
| zil-driver | `tests/query_cache_cp12.rs` | 12 |

**10 个 Cargo harness 合计 149 tests：失败、忽略、过滤、measured 均为 0。** 逐条 `test ... ok` 数目与各 harness 的 running/result 数目一致。Python 控制为 `test_bench` 8、`test_rust_bench` 26、`test_runtime_bench` 18，合计 **52 个通过，0 skipped**。

CP14 的 2047/2048/2049 三个边界在同一个成功测试函数内逐项执行；禁用/单单元、超阈值跨单元引用和非法 codegen 参数控制也通过。CP15 的四项新回归全部实际执行；`source_selection_bypasses_checkpoint_and_replays_current_ct_budget`、精确 CT 限额/N−1、源选择共享预算与请求恢复控制均通过。

## 身份与范围

- 42 条外部命令均退出 0、无超时，stdout/stderr 哈希一致；记录时序无相邻重叠，构建和测试在测量前完成。格式检查与 workspace `cargo check` 通过。
- LLVM/Clang/LLD 为 23.1.4；Rust/Cargo 为 1.99.1，Rust 自带 LLVM 为 23.1.1。安装收据、12 个安装命令日志与各计划冻结的 LLVM/Clang/LLD 身份一致。三个 Rust 主窗口均保留 native CGU/linker/LTO。
- 八份摘要分别绑定原始记录及 complete.json，共 600 次构建、240 次原生运行，记录均通过；本核验不裁定性能收益或逐语料运行退化。

原 ZIP 历史 SHA `bda15069c64693d6e9afbb0acd944e2c499d3bbc8495afc9713db5f16afa1d7b` 与独立查询的 GitHub artifact digest 一致。ZIP 在本次审查前已截空，未声称重新计算其字节。原 gzip SHA `d56bb7d139bee4cf0fba8c11ccb5a7815827cf12031336445ae599503938fbc3` 已独立复算并核对全部成员后才回收。保留的 XZ SHA `ab3c783d01494c95f794855598e5e832f592e7b9994aa2634a545431aac6fc4a` 及 364,175,360 字节解压 tar 流 SHA `a8d98528c59e7f00ed559d3bc2458f04dd86b27c2537a8d3eb63a2b5d9f5ad0f` 另行流式验证一致。

本地硬链接去重没有改变已核验内容或权限；原 runner 的绝对路径、inode、ctime 没有伪造。工具链大二进制未保留，安装部分核验限于收据、日志与计划间的身份一致性。详细核验数据见同名 JSON。
