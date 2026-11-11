# 三个研究ref最终独立回读

**独立回读确认：本轮layout与bounded精确退役均成功，composition/layout/bounded三个任务ref均不存在；主线和goal在本快照中未移动，archive仍A2。** 本轮只GET远端状态与实际job日志，并对照本地两份root收据，没有改任何ref、源码或benchmark，也未重查旧raw。

| 项目 | 实际读取结果 |
|---|---|
| layout run | [42988640504](https://github.com/zil-lang/zil/actions/runs/42988640504)，completed/success，head `52284449ea652983b16069c71bc8dc1a40a8b802` |
| bounded run | [42988998893](https://github.com/zil-lang/zil/actions/runs/42988998893)，completed/success，head `69a0aeb3cbd1f7cf7f992315f9bb1b33b9a73247` |
| composition ref | Git ref API 404 / NOT_FOUND |
| layout ref | Git ref API 404 / NOT_FOUND |
| bounded ref | Git ref API 404 / NOT_FOUND |
| main | `99035931d358add0eaed6d4535035c38413701dd` |
| goal | `acdeb7db04b796bf080ec527f1089b857e0c9431` |
| archive/source-history | A2 `5c97386cef5a337c432fc7f1351dfcc22a0c02e6` |
| 完整branches列表 | 只有archive/source-history、goal、main；第一页3条<100，列表完整 |

从实际job130415780442和130416969841日志逐字提取运行receipt，两者均returncode0、stdout含精确目标[deleted]、stderr空、verified_absent=true。layout记录PRIOR E及archive A1；bounded记录PRIOR A1及archive A2，与先归档再推进的链相符。提取后的完整workflow_receipt与root本地layout-retirement-receipt.json、bounded-retirement-receipt.json逐字段精确相等，没有只相信本地副本。

此为root提交最终F归档与更新goal之前的独立快照；后续root为了记录已完成退役而移动archive/goal，不属于本报告声称已观察的状态。源码与否定结果仍经归档祖先保全，两候选均未采用，未合入main。

精确API URL、实际日志receipt及对照结果见retirement-final-readback.json。
