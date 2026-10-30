# 2026-10-30：历史拆分回归的 feature 覆盖说明

原始 e8 `RESULT.json` 和全部日志保持不变：2,744 项测试执行通过是真的，但拆分后的独立 `zil-driver` 分组没有继承 workspace 中由 `zil-cli` 默认 feature 统一启用的 `llvm-api`。

因此，原记录不能被表述为与一次 `cargo test --workspace` 的 feature 覆盖完全等价。当时 driver 的 `llvm_api208` 与 `object_cache212` 条件目标未运行，另运行了 API-off 控制。各候选独立定向 LLVM API 控制和实际原生 benchmark 证据另行成立，不能据此补写原全仓测试记录。

最终组合源码 `6e8e63a5ceae861447d0447c325bed78b59c3f42` 追加了完整 driver `--features llvm-api` 分组，包括 216 个 integration target 及 lib/doc/examples；保留原默认轮，未重写任何失败或历史结果。其工作区统一 feature 等效唯一覆盖为 2,805 项，加入 API-off 控制后唯一并集为 2,806 项，零失败/忽略/过滤。

完整新日志见 `evidence/research/compile-performance/LLVM23-ROUND2/integration/`；非作者逐日志、测试名和目标核对提交为 `ac2bfe979f70ee6c96808102607fb27fa35df44c`。这项新验证只绑定最终组合源码，不倒填为旧 e8 的同配置验证。
