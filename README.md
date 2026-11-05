# Source history archive

This branch preserves source ref tips and staged/working-tree snapshots as commit parents. It is not accepted source or a test result.

Restore a preserved commit with `git branch NAME SHA`; inspect manifest.json for exact source refs, worktrees, exclusions, and WIP commits. Older ref names remain in prior manifests via `git log --first-parent archive/source-history`. Push and verify this archive ref before removing any old source ref.

## 2026-11-06 分支整理

远端 200 → 3、本地 180 → 3；现只保留 main、goal 和本归档。111 个旧 backup 分支已移除。完整退役清单见 [retirement.json](retirement.json)，最初全部分支名/WIP 清单位于 `f5a15637c40b7e345980dd9b270486b9c9c94783:manifest.json`；当前 manifest 保存整理后的源码快照。5 个未提交工作区原文件及暂存内容保留。

本归档的父提交保留真实 Git 对象，不能把记录中的提交号当作源码已合入。读取旧清单后可用 `git branch 恢复名 SHA` 恢复；沿本归档第一父提交可查看历次清单。后续只更新这一个固定归档入口，不再创建按任务或日期增长的 backup 分支。
