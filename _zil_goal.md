# ZIL 当前目标入口

当前任务分解、分配、优先级、工作状态和阻塞只维护在独立 **[goal分支的goal.md](https://github.com/zil-lang/zil/blob/goal/goal.md)**。

```bash
git fetch origin refs/heads/goal:refs/remotes/origin/goal
git show refs/remotes/origin/goal:goal.md
```

离线时读取已保存的goal版本并注明SHA；不要把旧缓存或下述历史报告称为最新状态。编辑使用独立goal工作区，goal永不合入main，源码分支也不合入goal。暂不使用GitHub Projects。

语言行为与验收条件继续由`_zil_design.md`、具名附件及`validation/product-gates.json`定义。任务分支的`.task`保存实施合同和交付要求；main保留有用决策、源码及复核/验收记录。main旧工单的status是工程记录字段，停止作为日常派工和进度的来源，详见[分支决策](.agent/goal-branch.md)与[工作流](docs/WORKFLOW.md)。

`node tools/handoff.mjs --check`检查本地主树工程记录完整性；它不读取实时goal内容，也不验证全部开发分支或语言终态。`check-plan`继续检查工程合同、规范与既有验收约束，不承担当前排期。

历史资料保留：[迁移前完整总计划（c15b77847）](https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/_zil_goal.md)、[2026-11-04进度复核（c15b77847）](https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/LANGUAGE-PROGRESS-20261104.md)及相应源码/验收收据。它们是固定时点记录，不再追加实时状态。

源码和计划分别备份。离线恢复包必须显式包含goal引用；仅保存main不足以恢复当前任务分配。新流程不降低任何语言、安全、独审或产品门槛。
