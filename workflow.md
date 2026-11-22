# 开发、复核与集成工作流

> 更新：2026-11-21 · 仅保留现行常驻规则。历史阶段性施工记录固定于 [`goal@e630d5f03`](https://github.com/zil-lang/zil/blob/e630d5f03/workflow.md)。

## 1. 分支与工作树

- **主线与看板隔离**：`main` 承载语言规范、阶段合同、源码、真实测试输入与产品门；`goal` 承载实时看板与协调记录，`goal` 永不合入 `main`。不在 `main` 中存放排期、过程报告或任务号文件；需要展开细化验收项时可在任务分支暂存 `.task/<ID>.md`，合入 `main` 前移除，仅保留正式合同与测试。
- **按完整功能链建分支**：一功能链一分支一 worktree，独立 `CARGO_TARGET_DIR`；共享 IR、公共 schema、根 `Cargo.toml`/`Cargo.lock` 与正式产品门由集成负责人单写协调。
- **操作禁忌**：禁止 `git reset --hard`、`git clean`、强推或在正在构建的工作树中切换分支。

## 2. 功能链实施与安全边界

- **整链贯通**：每批改动须覆盖完整链路（Source → CT → Typeck → Solve → 独立 Check → Lower/双后端原生 → 失败清理），不把局部字段、单测或只改准入不改下游的半成品当作交付。
- **独立核查**：`zil-solve` 与 `zil-check` 必须各自独立求证同一公开语义规则；`zil-check` 绝不信任证书或跳过负例检查。
- **终态接口**：不为未发布研究版本保留历史兼容层；同根语义问题合并修复，不按测试文件逐项打补丁。
- **严格命名**：永久文件、模块、测试与工具以语义能力或不变量命名，不以临时任务号命名。

## 3. 独立验证与真实验收

- **必过门槛**：改动合并前必须通过 `cargo fmt --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`、全量 `cargo test --locked --workspace`、`tools/check-repository-layout.py --full`、`tools/update-indexes.py --check` 及远端全部必需 CI；涉及核心语言/运行时变更须跑通 `.github/workflows/alpha-acceptance.yml` 并校验非零原始 Artifact。
- **Fail-Closed 口径**：成功必须有非零选择、实际执行与完整返回值/清理顺序断言；`Unsupported`、`ResourceLimit`、超时、缺报告、零执行或基础设施失败一律判败，不得算通过。
- **正反例并重**：每项安全边界变更必须同时包含正向原生执行（Clang 与 LLVM-API × O0/O2）与最近安全负例（Source 及必要时的 Raw/Wire 伪造证书拒绝）。

## 4. PR 合入与主线边界

- 生产源码与测试冻结后，按精确提交 SHA 完成源码复核与远端 CI 验证再合入；合入后立即删除已合并的远端任务分支与本地临时 worktree。
- 涉及 `_zil_design.md` 或 `spec/` 的变更须同步运行 `tools/update-indexes.py` 与 `tools/check-conformance.py`，确保规范锚点、索引与源码实现一致。

## 5. 看板维护与分支归档

- 更新 `goal` 前先 `git fetch origin goal` 核对远端 SHA，仅修改相关任务行，推送后确认远端 SHA。
- 退役含未合并独有提交的历史分支前，须先核对无遗漏价值并确认已保全于 [`archive/source-history`](https://github.com/zil-lang/zil/tree/archive/source-history)；已合入或与 `main` 零差异的分支可直接删除。
