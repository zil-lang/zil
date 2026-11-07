# ZIL

ZIL 是开发中的系统语言与编译器。当前提供有限语言开发预览；可用能力和限制见[支持范围](docs/guide/supported-language.md)，语言语义以 [_zil_design.md](_zil_design.md) 为准。

## 构建和尝试

当前支持 Linux x86_64 宿主。先按[工具链说明](docs/guide/build.md)准备锁定的 Rust、LLVM/Clang/LLD 和依赖，再运行：

```sh
cargo build --offline --locked -p zil-cli
cargo run --offline --locked -p zil-cli -- check examples/modules/main.zil --json
cargo run --offline --locked -p zil-cli -- run examples/modules/main.zil --entry main
```

[快速开始](docs/guide/quickstart.md)说明 CLI、示例和结果状态；Windows 本机、完整语言与产品成本目标尚未验收。

## 继续阅读

- [文档导航](docs/README.md)：使用、构建和语言设计。
- [构建说明](docs/guide/build.md)：工具链与实际 CLI 构建。
- [验证依据](validation/README.md)：产品门、静态合同与真实输入。
- [goal 看板](https://github.com/zil-lang/zil/blob/goal/goal.md)：唯一实时任务分配和状态；代理先读 [AGENTS.md](AGENTS.md)。

`crates/` 与 `lib/` 是编译器和标准库；`tests/`、`conformance/`、`examples/` 提供测试和普通程序。文件按语义命名，任务施工和交付约定见独立 goal 分支的 `workflow.md`。
