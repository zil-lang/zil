# ZIL

ZIL 是开发中的系统语言与编译器。当前可执行范围、限制和示例以 [语言初稿](docs/LANGUAGE-DRAFT.md) 为准；语言语义、安全边界和拒绝条件以 [_zil_design.md](_zil_design.md) 及其具名附件为准。开发检查点不代表完整语言或产品验收。实时任务、分配与阻塞只在独立 [goal 分支](https://github.com/zil-lang/zil/blob/goal/goal.md) 的 `goal.md`。

## 构建与运行

当前默认后端的宿主范围是 Linux x86_64。按 `rust-toolchain.toml` 准备 Rust 1.99.1，按 [LLVM API 工具链说明](docs/LLVM-API.md)安装并配置 `tools/llvm/version.txt` 固定的 LLVM/Clang/LLD 23.1.4，再运行 `python3 tools/llvm/check_toolchain.py`。安装命令及依赖缓存说明见 [交付说明](docs/DELIVERY.md)。已有 Win64 对象/链接验证不等于 Windows 主机原生验收。在工具链与依赖已准备好的环境中：

```sh
cargo build --offline --locked -p zil-cli
cargo run --offline --locked -p zil-cli -- check examples/modules/main.zil --json
cargo run --offline --locked -p zil-cli -- run examples/modules/main.zil --entry main
```

CLI 的 `check`、`build`、`run` 分别用于阶段检查、机器产物和执行；`--json` 用于机器报告。Success、SafeReject、Unsupported、ResourceLimit、InfraError、Cancelled 必须区分。案例登记、有限测试通过和旧轮结果都不自动证明正式发布条件。

## 检查与工程入口

```sh
cargo fmt --all -- --check
cargo test --offline --locked --workspace --no-default-features
cargo clippy --offline --locked --workspace --all-targets --no-default-features -- -D warnings
```

正式组合测试、原生回归和产品门由集成者按受影响范围与冻结提交执行，保留实际选择/执行/断言数、工具链和失败字段；见 [工作流](docs/WORKFLOW.md)。当前静态合同和验收协议位于 `spec/`、`validation/`、`conformance/`；旧主树 `.task/` 在迁移与检查器解耦期间保留，不能作为实时计划。每轮产品证据对照见 [Rust 四目标](docs/RUST-GOALS.md)。

## 源码与恢复

`crates/` 包含语法、展开、类型、分析、独立核验、lowering、LLVM 后端、查询、驱动与 CLI；`lib/` 是标准库，`tests/`、`conformance/` 与 `examples/` 提供可执行输入和反例。`spec/` 记接口与诊断，`policy/` 记依赖规则，`validation/` 记静态验收协议，`docs/decisions/` 记当前有效设计理由和[仓库布局决定](docs/decisions/repository-layout.md)。不要从文件名中的旧批次号推断功能或通过状态。

源码包的构建、独立 goal、Git 历史 bundle 和原始审计附件的恢复见 [交付说明](docs/DELIVERY.md)。代理接续见 [AGENTS.md](AGENTS.md)；用户上手见 [语言快速入口](docs/LANGUAGE-QUICKSTART.md)。历史施工报告仍可从 Git 历史追溯，不作为当前能力矩阵。
