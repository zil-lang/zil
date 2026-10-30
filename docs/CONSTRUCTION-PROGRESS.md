# 当前语言施工进度

2026-10-30 13:06 中国标准时间：218、220、221已有分别可编译的源码检查点，均已推送并读回同一SHA；219源级#if继续实施。新语言功能尚未完成贯通测试与独立验收。

|任务|实际状态|远端源码检查点|
|---|---|---|
|218|Expand编译检查通过；Types和限定测试继续|[62a529a2](https://github.com/zil-lang/zil/commit/62a529a21883dbbf246ba9f99de67228d03c0e9b)|
|219|已保存原217拒绝探针；源选择模块实施中|[task/219](https://github.com/zil-lang/zil/tree/task/219)|
|220|Analyze独立拥有工厂图与伴随参数入口编译通过；raw/Wire测试继续|[0fba9fed](https://github.com/zil-lang/zil/commit/0fba9fed65c1c460bc51bd851d7d9e7317e0570f)|
|221|Verify独立入口编译通过；raw/Wire测试继续|[5c4588e5](https://github.com/zil-lang/zil/commit/5c4588e50cf796aa4cc70fd3b0ed48999bb632e9)|

机器记录见 [construction-progress.json](../data/construction-progress.json)。更新时间只证明记录更新；有效推进以源码检查点、已完成检查和实际运行证据为准。超过20分钟没有新记录时为“待检查”，不能据此断言仍在运行或已经停止。

内置独立定时通知最低每小时一次，目前未创建后台通知任务；施工中的对话更新继续。阶段源码先推远端，再进入长测试/审核。

旧218仍为NotRecovered，本轮以新源码、新输入、新提交重新实施。完整语言、全部ZUI前置和产品门仍未完成。
