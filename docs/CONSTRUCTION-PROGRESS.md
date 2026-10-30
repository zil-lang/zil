# 当前语言施工进度

2026-10-30 12:35 中国标准时间：固定工具链已恢复，基线release CLI已实际构建；218重建与219源级#if已冻结输入，218/220/221独立前端/Analyze/Verify并行施工。还没有新的语言验收。

|任务|内容|分支|
|---|---|---|
|218|公共拥有 callable 工厂前端|[task/218](https://github.com/zil-lang/zil/tree/task/218)|
|219|已封闭纯标量条件的源级 #if|[task/219](https://github.com/zil-lang/zil/tree/task/219)|
|220|拥有工厂独立 Analyze 与组合集成|[task/220](https://github.com/zil-lang/zil/tree/task/220)|
|221|拥有工厂独立 Verify 与机器合同|[task/221](https://github.com/zil-lang/zil/tree/task/221)|

机器记录见 [construction-progress.json](../data/construction-progress.json)，冻结证据见 [FREEZE.json](../evidence/planning/218-221/FREEZE.json)。更新时间只证明记录被更新；有效推进以新的源码检查点、已完成检查或实际运行证据为准。超过20分钟没有新记录时状态为“待检查”，不能仅凭静止记录断言仍在运行或已经停止。

内置独立定时通知最低每小时一次，当前未创建后台通知任务；施工中的对话更新继续。阶段代码先推远端并读回精确SHA，再进入长测试/审核，大证据归档不阻塞源码检查点。

旧218仍为NotRecovered；本轮以新源码、新输入、新提交重新实施。当前完整语言、全部ZUI前置与产品门均未完成。
