# 172 作者验证证据

实施提交：`69882483d6b29bee8d831386325da739ac915fd4`。本目录是作者限定验证，不能替代独立审核或主树正式验收。

- `SUMMARY.json`：精确身份、范围、计数、预算和未完成项。
- `commands/`：fresh target 的 172/170 定向测试、Clippy、格式与 CLI 构建原始日志/命令。
- `consumer/summary.json`：34 次最终 CLI 原生观察；每次命令、stdin、stdout、stderr、分配账目与输出摘要在其子目录。
- `consumer/main/O0/phases/`：Typed IR、候选、接口与 LLVM 机器表示。其他优化/入口保留 LLVM；重复大转储不收录。
- `tests/index.json`：19 项带确切阶段诊断的源码拒绝、20 次测试 native 构建及 22 组成对 raw/Wire 篡改索引。测试候选不在变体上重新 Analyze。
- `tests/*/verify-budget.txt` 与深/宽元数据诊断：N=7122/N−1=7121 及默认深度的独立拒绝。
- `baseline/`：旧 170 CLI 对相同最终消费者的真实 Types 拒绝。
- `failures/`：首次 Wire 栈溢出、阶段定位、同一恶意字节、前修复测试源码、Cargo 基础设施与夹具失败。
- `FILES.json`：相对路径、字节数和 SHA-256；清单本身除外。

重跑：先按仓库工具链以 `CARGO_INCREMENTAL=0`、新 target、`-j2` 构建对应提交，再执行 `cargo test -p zil-driver --test impl172_owned_store --test impl170_package_fields --offline -j2 -- --test-threads=2`。真实 allocator 消费者命令为 `python3 tools/language/172/consumer.py --repo <checkout> --cli <built-zil> --out <new-output-dir> --source-commit <exact-commit> --compiler-commit <exact-commit>`。该 Python 工具不构建容器，不替代 ZIL 后端，也不做性能测量。
