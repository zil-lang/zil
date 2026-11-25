# 开发、复核与集成工作流

> 更新：2026-11-25 · 仅保留现行常驻规则。历史阶段性施工记录固定于 [`goal@e630d5f03`](https://github.com/zil-lang/zil/blob/e630d5f03/workflow.md)。

## 1. 分支与工作树

- **主线与看板隔离**：`main` 承载语言规范、活合同、源码、真实测试输入与产品门；`goal` 承载实时看板与协调记录，`goal` 永不合入 `main`。不在 `main` 中存放排期、过程报告或任务号文件；需要展开细化验收项时可在任务分支暂存 `.task/<ID>.md`，合入 `main` 前移除，仅保留正式合同与测试。
- **按完整功能链建分支**：一功能链一分支一 worktree，独立 `CARGO_TARGET_DIR`；共享 IR、公共 schema、根 `Cargo.toml`/`Cargo.lock` 与正式产品门由集成负责人单写协调。
- **操作禁忌**：禁止 `git reset --hard`、`git clean`、强推或在正在构建的工作树中切换分支。

## 2. 功能链实施与安全边界

- **整链贯通**：每批改动须覆盖完整链路（Source → CT → Typeck → Solve → 独立 Check → Lower/双后端原生 → 失败清理），不把局部字段、单测或只改准入不改下游的半成品当作交付。
- **单一语义源与独立核查**：`zil-solve` 与 `zil-check` 共享 `zil-semantics` 公开语义定义并各自独立求证；**不为 `zil-check` 另造独立语义系统**，且 `zil-check` 绝不信任求解器证书或跳过负例检查。
- **终态接口与模块化边界**：不为未发布研究版本保留历史兼容层；同根语义问题合并修复，不按测试文件逐项打补丁；避免继续膨胀超大单文件，按语义职责拆分模块与集成测试套件。
- **严格命名**：永久文件、模块、测试与工具以语义能力或不变量命名，不以临时任务号命名。

## 3. 留证规范（活证据优先、禁止过程留证堆积）

- **日常 PR 以可执行证据为准**：日常开发 PR 通过精确 Git 提交 SHA、自动化正反例测试与绿灯 CI 证明正确性；**禁止**向主树 `validation/` 追加一次性阶段 JSON 快照或手动复制的运行收据。
- **仅在冻结里程碑生成静态收据**：静态验收 JSON/Artifact 仅在冻结发布候选（Release Candidate）、正式里程碑签发或可重复性能基线对照时由自动化工具生成一份；`validation/` 主树仅保留活合同、清单与真实测试输入（见主树 `validation/README.md`）。
- **历史只读归档**：旧验收收据分别固定于 `28b3e37bb4d8a1fe8b146c72cf891fc99de32440:validation/results/` 与 `08da36ff0384144d795d6ada072a3a51a3eb9a68:validation/`，只读溯源，不再留在当前主树制造维护噪音。

## 4. 分层验证、提速与真实验收

- **本地快速开发环**：日常迭代优先使用 `python3 tools/dev-check.py --fast -p <crate>` 做秒级反馈（格式、增量布局、索引、一致性、crate 架构与目标测试）；发 PR 前执行 `python3 tools/dev-check.py --full` 跑通完整本地门（含 `clippy --all-targets -- -D warnings`）。
- **远端分层 CI（不降质量前提下提速）**：
  1. `repository-docs.yml`：所有 PR 必过（~30s，布局、计划、一致性、索引）；
  2. `verify-main.yml`：代码 PR 必过，拆分为并行 `workspace`（`fmt` + `clippy` + 全量测试 + `text-reference` + CI 自测，复用同一构建与 LLVM/Cargo 缓存）与 `reference-and-consumer`（Clang 参考构建 + State/Store 16/128/56）；
  3. `research-llvm.yml`：按路径触发（`crates/zil-backend-llvm/**`、`crates/zil-llvm-api/**`、`tools/llvm/**`、`tools/research/**`）+ 每日定时（nightly）+ 手动触发，不拖慢无关语言/标准库 PR；
  4. `alpha-acceptance.yml`：核心运行时/语言里程碑与发版候选按标签或手动触发，校验 Fixed Projects 21/21 与 Roam 42+24 原始非零 Artifact。
- **Fail-Closed 口径与正反例并重**：成功必须有非零选择、实际执行与完整返回值/清理顺序断言；`Unsupported`、`ResourceLimit`、超时、缺报告、零执行或基础设施失败一律判败。每项安全边界变更必须同时包含正向原生执行（Clang 与 LLVM-API × O0/O2）与最近安全负例（Source 及必要时的 Raw/Wire 伪造证书拒绝）。

## 5. PR 合入与看板维护

- 生产源码与测试冻结后，按精确提交 SHA 完成源码复核与远端 CI 验证再合入；合入后立即删除已合并的远端任务分支与本地临时 worktree。
- 涉及 `_zil_design.md` 或 `spec/` 的变更须同步运行 `tools/update-indexes.py` 与 `tools/check-conformance.py`，确保规范锚点、索引与源码实现一致。
- 更新 `goal` 前先 `git fetch origin goal` 核对远端 SHA，仅修改相关状态单元格，推送后确认远端 SHA。退役含未合并独有提交的历史分支前须确认已保全于 [`archive/source-history`](https://github.com/zil-lang/zil/tree/archive/source-history)。
