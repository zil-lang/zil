# Layout最终退役对象独立审核

**Pass：可以先发布精确A1，再把layout任务ref从精确a60推进到W触发原guard。** 此结论绑定远端实际Git对象，不代表删除已执行；本审核没有修改ref、触发workflow、改生产或原始benchmark。

| 对象 | SHA / 独立API核验 |
|---|---|
| E证据归档 | `a48374cce53d1018fe52a1c749990db4248f7135`；直接父精确为c43cda、a60ccf、1679decf |
| Wlayout | `52284449ea652983b16069c71bc8dc1a40a8b802`；唯一父精确a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf |
| W tree | `72df7aa4b27ac5b5def37818e1c47880ac545268` |
| W唯一diff | 新增`.github/workflows/compiler-resumption-retire.yml`，无其他路径 |
| W workflow blob | `713334961b62b271610b28a1c4b951eae7f220cf`，6719字节 |
| A1 | `a018e25431dc74a5d56e7e2df9ecf75b9f179717`；直接父精确为[E,W] |
| A1 tree | `0473ff363458ed61acacc9d33c252bbf76095f7b` |

远端W反向替换名称、branch、OLD、PRIOR四字段后，与成功1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56的实际远端workflow逐字相等，替换次数[1,2,1,1]。PRIOR精确E，OLD精确a60，唯一目标为task/compiler-layout-projection。唯一新增workflow、GitHub SHA自绑定、当前archive必须直接包含[E,W]、manifest、head/base PR、排除本run后的活动run、删除前复核、精确force-with-lease与删除后404/receipt等逻辑均未改变；嵌入Python静态解析成功。

A1相对E只改manifest.json并新增归档目录下final-layout.yml，该保留recipe与W共享同一blob。manifest prior_archive=E、commit_parents=[E,W]、commit_tips含W、目标remote_refs指向计划W；retirement条目明确注明这是归档后才发布的planned recipe，不冒充已观察ref。source_evidence_head和observed_refs仍保留原a60，retirement_completed=false，没有提前声称执行完成。

E的直接父保全旧c43全历史、a60与1679两条完整源码；manifest source_trees精确d260d616与1e599b1b，missing_commit_objects为空。raw-evidence.json真实指向Library version1，包SHA256 `6058113ae324218f7e159c7f9fc35be71408e7bebf5210ce79d00554863fe330`、20657285字节、6223 tar entries/6212 raw files。已核本地包SHA/大小/entry数与该指针一致；包含完整a60 after、1679 runtime及main A/A并发证据，明确后续退役receipt另记，不伪称已在冻结raw中。

## 首次字节差异及修正

首次local final-layout.yml有6720字节、SHA256 `9d6edeb6bf9860f9ba88ba9b945c1dc1cc6aef94592c5448f58b5f841caf3f24`，比远端实际W多一个末尾LF，Git blob为ee5c87e6b19a4b90001a8ab918f930dca4f348ea。远端实际W的guard与成功模板本已一致，无需改Git对象。

root随后按远端实际source对齐本地，仅去额外LF；本审核再次读取确认6719字节、SHA256 `f5a7f1f229b2e337fddf2521a488dbd46317d1dbdf4b1a55f1fc3d58f624e5f8`、blob713334961b62b271610b28a1c4b951eae7f220cf，现已精确匹配W和A1保留文件。首差异与修正分别保存在JSON，不以新状态覆盖旧事实。

## 当前状态和允许顺序

审核时真实archive仍E、layout仍a60、bounded仍1679；main仍99035931d358add0eaed6d4535035c38413701dd，goal为acdeb7db04b796bf080ec527f1089b857e0c9431。全仓open PR为空，layout分支Actions记录为空。执行时仍须由原workflow重新查活动状态和精确lease；本次静态快照不替代它。

严格顺序：先发布A1并读回，再只推进layout ref到W，等待删除与receipt；在此之前不能发布后续A2，否则当前archive直接parents guard会拒绝。main/goal和bounded ref不在此recipe写入范围。

链接：[E](https://github.com/zil-lang/zil/commit/a48374cce53d1018fe52a1c749990db4248f7135)、[W](https://github.com/zil-lang/zil/commit/52284449ea652983b16069c71bc8dc1a40a8b802)、[A1](https://github.com/zil-lang/zil/commit/a018e25431dc74a5d56e7e2df9ecf75b9f179717)。精确身份、API核验项与字节修正见final-layout-review.json。
