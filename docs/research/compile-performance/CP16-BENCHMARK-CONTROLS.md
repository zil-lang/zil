# CP16 显式 Rust LTO 对照

## 为什么增加这个控制

CP08 的 Rust 主配置保留工具链原生 CGU、链接器和 LTO 策略。未指定 `-C lto` 时，Rust 在适用的优化配置下可以对同一 crate 的多个 CGU 执行 thin local LTO；ZIL 当前按自己的 CGU 划分独立运行 LLVM 优化管线。两种流程的完整编译成本比较有意义，但不能把它当作完全相同的后端工作量。

CP16 在既有 `rust_bench.py` 增加一个可选的 LTO 控制，用于定位这项差异。默认主配置保持原有行为，语料、权重、采样、正确性检查、失败拒绝和产品门均不变。

| 参数 | Rust 实际命令 | 结果用途 |
| --- | --- | --- |
| `--rust-lto native`，默认 | 不增加 `-C lto` | 保留工具链原生 LTO 策略；接续既有主比较 |
| `--rust-lto off` | 增加 `-C lto=off` | 独立的后端机制诊断；不能替代 native 主结果 |

该参数只改变 Rust 命令，ZIL 命令、Rust CGU 配置和链接器选择均保持各自显式计划。`native` 只描述 LTO 策略；若另选 `--rust-profile matched` 或 `--rust-linker`，仍须披露这些非默认设置。即使两端都不做 LTO，LLVM 版本、前端 IR 和单元划分仍可能不同，不能标成全部设置完全相同。

关闭 LTO 也可能改变运行代码质量，因此每个模式都需要自己的 preflight、完整构建和原生运行窗口。不能只比较编译耗时，不能从两种模式中逐语料挑选有利数字混分。

## 冻结和旧计划

新计划 schema 为 `zil-rust-compile-performance/3`。`prepare` 将 `rust_lto` 和解释范围写入计划；`load` 要求明确的 `native` 或 `off`，缺失、非法值或旧 schema 均拒绝，不对旧文件补默认值。实际每条 Rust argv 仍与冻结计划重新生成的命令严格一致；缺少或多出 LTO 参数会使该行失败，不能评分。构建摘要和运行摘要都保留模式及解释范围。

旧 `/2` 计划没有失效为历史证据，应从其 `runner_source_commit` 恢复原 runner、helper、工具与输入后复算。不得把旧计划 schema 或哈希手改为 `/3` 后冒充新口径的实测；要测新控制必须另建新计划和新输出目录，保留原始窗口。

## 执行

沿用 [CP08 完整流程](CP08-BENCHMARK.md)，仅在 `prepare` 明确选择模式。下面路径和提交必须替换成实际已冻结值；两个模式使用不同目录。

```sh
python3 tools/research/compile-performance/rust_bench.py prepare \
  --out /tmp/cp16-o2-native-plan \
  --zil /path/to/zil --zil-source EXACT_ZIL_COMMIT \
  --rustc /path/to/rustc --clang /path/to/clang \
  --tool /path/to/libLLVM.so \
  --profile O2 --cache-mode cold --rust-profile native --rust-lto native \
  --units 8 --jobs 4 --blocks 5

python3 tools/research/compile-performance/rust_bench.py prepare \
  --out /tmp/cp16-o2-lto-off-plan \
  --zil /path/to/zil --zil-source EXACT_ZIL_COMMIT \
  --rustc /path/to/rustc --clang /path/to/clang \
  --tool /path/to/libLLVM.so \
  --profile O2 --cache-mode cold --rust-profile native --rust-lto off \
  --units 8 --jobs 4 --blocks 5
```

两个模式均继续执行 CP08 的 `preflight`、`measure`、`summarize --kind build`、`measure-runtime` 和 `summarize --kind runtime`。是否启用 `--closed-executable` 等 ZIL 研究模式由事前测量计划明确指定，不在两种 LTO 模式之间顺带改变。

## 验证与边界

本片运行 `python3 tools/research/compile-performance/test_rust_bench.py`：原有 21 项控制保持，并增加 5 项控制，合计 26 项通过。新增覆盖 CLI 默认、仅改变 Rust 的显式 LTO 参数、缺失或非法冻结模式拒绝、旧 schema 拒绝、记录中的 LTO 参数丢失拒绝；既有构建和运行摘要控制同时核验模式保留。

这些使用合成编译器和原生记录，证明运行器的受测行为，不证明真实 Rust LTO 已关闭后的速度或运行性能。实际新窗口、独立审核和产品门均未在本片执行。没有增加精确链接器控制：现有 `--rust-linker` 仍是 driver 选择，不能单凭相同 driver 路径声称实际 LLD 完全相同。

| Rust 目标 | 本片结果 |
| --- | --- |
| 安全 | 未改生产编译器或当前 Verify；不是语言安全的新证据 |
| 运行与内存 | 模式分别冻结并要求独立运行质量比较；新性能未测 |
| 表达与维护 | 只增加显式研究参数；未改语言语法或维护目标 |
| 编译成本与实质突破 | 补充可复现的机制对照能力；本片不宣称编译提速或产品突破 |

一手依据：[Rust codegen options 的 LTO 说明](https://doc.rust-lang.org/rustc/codegen-options/index.html#lto)。实际工具链能力与版本仍以新计划保存的 `rustc -Vv` 及真实执行结果为准。
