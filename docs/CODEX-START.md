# Codex 接续清单

本页只列恢复动作。当前施工规则见 [AGENTS.md](../AGENTS.md)，语言行为见 [_zil_design.md](../_zil_design.md)，实际支持范围见 [LANGUAGE-DRAFT.md](LANGUAGE-DRAFT.md)。历史 draft22 的结果由原 Git 提交和收据证明，不代表当前 HEAD。

1. 在独立工作树确认 `git status --short` 和 `git rev-parse HEAD`；不要切换正在构建的工作树。
2. `git fetch origin refs/heads/goal:refs/remotes/origin/goal`，再 `git show refs/remotes/origin/goal:goal.md`，记下 goal SHA；离线版本注明时点。
3. 读取任务分支 `.task/<ID>.md`、父合同、实际写集与规范摘要；main 的静态工程义务在 `spec/` 和 `validation/`，迁移期旧主树 `.task/` 只作对照来源。
4. 运行 `node tools/handoff.mjs --check` 检查工程关系。它不读取 goal、不遍历全部分支、不证明语言验收；失败和范围如实报告。
5. 按 [WORKFLOW.md](WORKFLOW.md) 运行限定测试、保存精确提交和证据，独审后由集成者处理正式门。源码包、Git bundle、审计附件的恢复见 [DELIVERY.md](DELIVERY.md)。

支持的工具链、依赖缓存和平台须按本次收据说明。先前的 Pass、有限模型及旧 native 运行不能自动移作新提交的结果。
