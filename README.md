# ZIL 当前计划

打开 **[goal.md](goal.md)** 查看任务范围、优先级、负责人和实际进展。本分支只保存计划，不合入 main；语言规范、源码和验收记录位于相应源码分支。分支命名、文档归属、更新与备份规则集中在 [AGENTS.md](AGENTS.md)。

以下是固定源码/恢复参考，不是第二份实时任务状态。[精简前完整记录](https://github.com/zil-lang/zil/blob/fc3dff269a882ef2850c4f9dba2acf6031ac4439/goal.md)保留原规范索引、安排及历史说明。

## 源码与恢复快照

| 标识 | 实际源码/远端引用 | 已核范围 |
| --- | --- | --- |
| M37 | [`main@4bc1d77b5ec3bccb5b789fd813e85386f03ae13f`][m37] | PR33已合入；历史共同门默认4372、reference4261及相关检查。只适用于该接受树，完整语言/B0/ZUI未完成。见[E37][e37]及[远端合入收据][remote37] |
| C39 | [`task/language-terminal-20261104@c15b77847502d02acc83474bf7f9a8789186a3ac`][c39] | 可运行39/CT4候选；含后继修复和限定复核，未成为远端main。Option回归修复在该线另复核50项 |
| C40 | [`task/terminal-public40@7dfc6846194643e5295a7704a0b8ce99fa5176e4`][c40] | 作者组合检查：aggregates149、cross84、generic_regressions83、generics33，共349通过。不是全仓或终态验收；见[E40][e40] |
| C41 | [`task/terminal-cas41@7c7950dfc820d1b06a295814c662105842e03587`][c41] | CAS41公共运输候选；源码、两核、机器完整链未闭合 |
| W340 | [`backup/wip-20261104-340@5075da0c1f82ea80313b6f6a546fc4b027664063`][w340] | 拥有替换等实际未提交源码已保存为远端快照；不等于已验收 |
| W343 | [`backup/wip-20261104-343@9764f5119c2ccdf49d6e5c81683c64c11a9cd017`][w343] | CAS等实际未提交源码快照；不是仅保存一个提交编号 |

以上SHA是建板时实查的功能核验/恢复快照，不代表迁移后分支的最新HEAD。后续管理文档提交不改变这些功能证据的绑定；新源码进展只按实际 `git ls-remote`、源码及证据更新。旧301–312缺失的对象/未提交内容没有因本看板建立而被宣布恢复。原大数组O0/默认2MiB栈问题已经修复并进入M37，不再作为待修缺口重复派工。

工作流迁移已由[PR35](https://github.com/zil-lang/zil/pull/35)合入远端[`main@4c775ec09d00818deffe06c4dd30ab4c540af524`](https://github.com/zil-lang/zil/tree/4c775ec09d00818deffe06c4dd30ab4c540af524)，只更新入口、工作流和工具输出，没有合入C39/C40/C41的候选语言实现；语言接受范围仍按M37及各行证据解释。对应独立复核与限定检查见该提交的`docs/validation/review-goal-workflow-20261105.json`。

上述 39/40/41 是建板时的历史快照；后续共同验收与当前派工以 [goal.md](goal.md) 的逐项记录为准，不再以本节安排施工。不同分支的通过数不能累加成一棵树的全量通过。


## 旧源码查找参考

以下仅供查找旧源码，不表示当前派工；旧分支名称按本页末尾的统一归档恢复。`task/…` 是 GitHub 运输引用，本地旧分支可能名为 `340`、`341` 等，使用前核真实 SHA，不因名字相似合并旧副本。

| 旧施工记录 | 既有源码分支 | 分支内实施说明 | 关联任务 |
| --- | --- | --- | --- |
| 基础/模式340 | `task/340`及W340 | `.task/impl/340.md` | G1.3/4 |
| 泛型341 | `task/341`，近期成果已组合至C40 | `.task/impl/341.md` | G2.3/4、G5.1/2 |
| life/协议342 | `task/342`、`task/342-rich40`及C40 | `.task/impl/342.md` | G3.3/4、G6.1/2 |
| 运行时343 | `task/343`、W343、C41 | `.task/impl/343.md` | G8.3，后续G7/G8责任组合 |
| CT/可靠性344 | `task/344`，近期成果已组合至C40 | `.task/impl/344.md` | G4.4/5、G10.3 |
| 库/平台345 | 归档中的 `task/345` 旧交付；下一任务从已接受 main 建立 | `.task/impl/345.md` | G9.3起；不得直接把旧分支当最新共同基线 |
| 集成root | C39/C40/C41与新派任务分支 | 各分支对应`.task`说明 | G10.1、规范/公共接口协调及阶段备份 |

[m37]: https://github.com/zil-lang/zil/tree/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f
[c39]: https://github.com/zil-lang/zil/tree/c15b77847502d02acc83474bf7f9a8789186a3ac
[c40]: https://github.com/zil-lang/zil/tree/7dfc6846194643e5295a7704a0b8ce99fa5176e4
[c41]: https://github.com/zil-lang/zil/tree/7c7950dfc820d1b06a295814c662105842e03587
[w340]: https://github.com/zil-lang/zil/tree/5075da0c1f82ea80313b6f6a546fc4b027664063
[w343]: https://github.com/zil-lang/zil/tree/9764f5119c2ccdf49d6e5c81683c64c11a9cd017
[e37]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/terminal-root37-integration-20261104.json
[remote37]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/terminal-root37-remote.json
[e40]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/terminal-progress-20261104.json

## 2026-11-06 分支整理

旧 backup、已结束的 task/review 分支已集中到 [source-history 归档](https://github.com/zil-lang/zil/tree/archive/source-history)。归档的 `manifest.json` 保存原分支名、实际提交和未提交源码快照；源码通过 Git 父提交保全，不能据此视为已验收。按清单中的 SHA 执行 `git branch 恢复名称 SHA` 可恢复；旧清单可沿归档第一父提交查看。历史工作区保留文件，已解除过期分支绑定。后续只维护活跃任务分支与这个固定归档入口，不再批量创建 backup 分支。
