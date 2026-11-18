# 固定项目 Rust 原生对照

清单固定 21 份原始项目、60 次原生构建和 196 次原生行为对照。项目整目录从清单指定的 Git 提交取出，核对目录 tree 和 ZIL、Rust 入口的 SHA-256。历史对照基于 `365ee0748` 的逐行收据，已登记各行的原始 stdin 哈希、预期退出码和 stdout 哈希。重跑同时逐字节核对 ZIL 和本次 Rust 对照的 stdout、stderr。[项目清单](manifest.json)

逐项核对候选提交中的同名目录 tree。两个泛型项目允许清单固定的已知规范修订版本，报告标记其实际 tree 身份，并继续用冻结原树做语义反例。

使用具备清单内提交的 Git 仓库、项目规定的 Rust/LLVM 工具链和对应候选的 CLI 构建收据。构建收据提供 `source_commit`、`cli_sha256` 和 `exit_code`。

```sh
python3 tools/language/check-fixed-projects.py \
  --cli /path/to/candidate/zil \
  --build-receipt /path/to/candidate/build-receipt.json \
  --out out/runs/fixed-projects
```

`--out` 指向一个新目录。报告保留每次进程的实际命令、退出状态、诊断和输出摘要，并保存复用的确切 stdin 和进程的输出字节。缺原始 stdin 的行仍占原分母。若另有原始输入，将原文件以完整 SHA-256 命名后经 `--stdin-directory` 提供。测试先验明字节身份。

四个原始语义反例要求在清单登记的阶段、原文件和具体原因处安全拒绝。四份合法对照从独立的固定树或逐字节核对过的局部修订取得，保留其项目及导入依赖。两个 `run_if` 对照另调用修订后的同一实际 helper，核对跳过时只清理、调用时执行一次随后清理。对照在 Clang 和 LLVM API 的 O0/O2 各执行，独立于 21 个原项目及 196 次历史对照计数。任一必需重跑或语义对照缺失、失败或不一致，验收工具退出非零。

两个此前尚未进入历史原生矩阵的原项目有各自的固定观察。`app` 使用 `result` 核对完整 i64 输出 `475000738663`，使用普通 `main` 核对自检退出及空输出。`resource_closures` 使用原协议的十二个输入观察成功和各错误出口的退出状态与准确析构顺序。两项目共选择 12 次构建和 56 次行为对照，分别核空 stderr。[app 协议](../../../examples/mainline/app/README.md) · [资源闭包协议](../../../examples/mainline/resource_closures/README.md)
