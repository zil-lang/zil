# 326：handoff当前记录与历史可用性分开核验

现有工具把历史清理后无法恢复的旧提交/证据与当前交付混在一个Pass/Fail中，同时只接受`evidence/`、只识别`main`分支，导致当前`work`主树和已提交精简JSON被误报。保存的摘要不是当前权威。本片先冻结`fe4cb19ae9bf82cedc1c80dd7677808aa46b4072`的旧任务原字节，再修工具；不删除或改写原历史，不恢复大型raw证据，不替旧Pass背书。

迁移清单逐路径记录原SHA、原状态、对象引用及存在性。原已合入记录、root明确归档的25个旧在途记录及153/158历史笔记作为HistoricalRecord；无法复核者显示Unavailable，有对象者也只显示Unreviewed，不自动复审为Pass。181与14正式任务保持current，由root迁真实基线/hash及历史evidence字段；未列任务默认current，不按编号或日期跳过。

现代输出分离`current_gate`与`historical_status`，正常入口只给CurrentPass/CurrentFail；历史Unavailable保留且`claims_revalidated=false`。`--legacy-audit`另外执行旧记录的严格规范/对象/证据/精确review检查，历史缺失仍失败。无迁移清单的旧调用保持原严格行为，原13个self-test不削弱。

真实权威可以是唯一main工作树，或显式同仓work主checkout；无main时只允许Git common-dir对应的真实work主checkout。错误不能再同时声称current_is_main。输出真实branch、HEAD与本地origin/main跟踪值，不把本地work等同远端已合入。

精简证据根仅增加`docs/validation/`，仍核已提交非空JSON、路径及符号链接不逃逸。显式JSON selector选择具体记录；PassLimited/LocalLimitedAcceptance保持有限范围，不满足正式任务Done。review从指定review_commit读取，核非作者和精确交付绑定；317的doc-only交付只在显式白名单与真实Git diff证明等价时关联原受审代码。源码/测试变化不得借metadata-only绕过。

限定测试沿用`tests/handoff.test.mjs`，保留原13个断言族，追加历史冻结/篡改、current失败不被归档遮蔽、docs未提交/错selector/自审/错commit、work权威和metadata-only源码变化拒绝、legacy-audit与只读检查。root持任务状态、正式规范摘要、最终主树和远端合入；本片不改其它任务文件，不跑正式全量。

Rust四目标：本片是交付记录工具，不改变语言安全、运行或内存表示；目标是诚实区分可复核当前交付与不可复核历史。未做Rust成本/维护配对或性能测量，未建立产品级突破。实际限定命令及结果在交付时追加。
