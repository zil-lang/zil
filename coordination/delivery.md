# 交付与恢复

一次交付须区分三种身份：候选源码提交、独审所审的精确提交、远端 main 实际接受提交。源码、静态规范附件、真实测试输入和必要精简验收位于 main；[goal 看板](../goal.md)只记录分配、状态与证据定位。发布前核完整执行命令、工具链/profile/features、成功和失败计数、原始日志位置及未测边界。Actions 成功、附件上传和 Git 对象可恢复分别验证。

每次运行用独立 RunId；原始日志写唯一 `out/runs/`。失败、缩减、故障注入和修复后运行分别保留身份，不以新结果覆盖原失败。goal 的 `records/` 只存帮助协调的短索引。语言验收结果随精确源码交付，不能由静态登记或任务状态推断。

活跃分支普通 push。阶段 WIP 与退役分支只写固定 [`archive/source-history`](https://github.com/zil-lang/zil/tree/archive/source-history)；manifest 保存原 ref、实际源码/暂存/工作树提交，父提交关系让对象可达。退役旧 ref 前核远端完整 SHA、manifest 与唯一修改已保全。固定 Git 提交可恢复旧研究、施工和验收记录，但历史路径与结论只按原源码解释。禁止对工作区 reset/clean。
