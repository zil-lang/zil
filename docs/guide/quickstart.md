# ZIL 语言快速入口

本页用仓库中的真实程序展示当前可尝试的 CLI 路径。当前支持矩阵和限制见 [语言初稿](supported-language.md)；语义、拒绝条件与安全边界以 [_zil_design.md](../../_zil_design.md) 及具名附件为准。示例可运行不等于完整语言或产品门通过。

先按 [交付说明](build.md)准备 `rust-toolchain.toml`、受支持 LLVM/clang 和 `Cargo.lock` 对应依赖，再构建 CLI：

```sh
cargo build --offline --locked -p zil-cli
./target/debug/zil check examples/modules/main.zil --json
./target/debug/zil run examples/modules/main.zil --entry main
```

`check` 可用 `--stop-after parse|core|types|analyze|verify` 选择阶段；`build` 以 `--output` 和 `--emit-dir` 保留机器产物；`run` 执行普通程序。`--json` 报告需要检查状态、诊断、退出码和失败字段。SafeReject 是正确拒绝时的结果，Unsupported、超时、InfraError 或零执行不能写成通过。

更多源码例子在 [`examples/mainline/`](../../examples/mainline) 与 [`conformance/`](../../conformance)；后者含登记的正反例和预期，登记本身不等于真实执行。使用某例前先读其 README 和实际编译结果。工作流和收据格式见 独立 goal 分支的 `workflow.md`。
