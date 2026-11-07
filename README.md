# ZIL

ZIL 是开发中的系统语言与编译器。当前提供有限语言开发预览；可用能力和限制见[支持范围](docs/guide/supported-language.md)，语言语义以 [_zil_design.md](_zil_design.md) 为准。

## 构建和尝试

当前支持 Linux x86_64 宿主。先按[工具链说明](docs/development/toolchain.md)准备锁定的 Rust、LLVM/Clang/LLD 和依赖，再运行：

```sh
cargo build --offline --locked -p zil-cli
cargo run --offline --locked -p zil-cli -- check examples/modules/main.zil --json
cargo run --offline --locked -p zil-cli -- run examples/modules/main.zil --entry main
```

[快速开始](docs/guide/quickstart.md)说明 CLI、示例和结果状态；Windows 本机、完整语言与产品成本目标尚未验收。

## 继续阅读

- [文档导航](docs/README.md)：使用、开发、设计决定和研究。
- [开发工作流](docs/development/workflow.md)：并行分工、测试、独审和合入。
- [验证依据](validation/README.md)：静态合同、真实输入与精简验收。
- [恢复与交付](docs/development/delivery.md)：源码、独立看板和历史恢复。
- [goal 看板](https://github.com/zil-lang/zil/blob/goal/goal.md)：唯一实时任务分配和状态；代理先读 [AGENTS.md](AGENTS.md)。

`crates/` 与 `lib/` 是编译器和标准库；`tests/`、`conformance/`、`examples/` 提供测试和普通程序。文件按语义命名，完整目录职责见[仓库组织](docs/development/repository-layout.md)。
