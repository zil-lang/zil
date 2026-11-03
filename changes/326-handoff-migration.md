# 326：handoff当前记录与历史可用性分开核验

现有工具把历史清理后无法恢复的旧提交/证据与当前交付混在一个Pass/Fail中，同时只接受`evidence/`、只识别`main`分支，导致当前`work`主树和已提交精简JSON被误报。保存的摘要不是当前权威。本片先冻结`fe4cb19ae9bf82cedc1c80dd7677808aa46b4072`的旧任务原字节，再修工具；不删除或改写原历史，不恢复大型raw证据，不替旧Pass背书。

迁移清单逐路径记录原SHA、原状态、对象引用及存在性。原已合入记录、root明确归档的25个旧在途记录及153/158历史笔记作为HistoricalRecord；无法复核者显示Unavailable，有对象者也只显示Unreviewed，不自动复审为Pass。181与14正式任务保持current，由root迁真实基线/hash及历史evidence字段；未列任务默认current，不按编号或日期跳过。

现代输出分离`current_gate`与`historical_status`，正常入口只给CurrentPass/CurrentFail；历史Unavailable保留且`claims_revalidated=false`。`--legacy-audit`另外执行旧记录的严格规范/对象/证据/精确review检查，历史缺失仍失败。无迁移清单的旧调用保持原严格行为，原13个self-test不削弱。

真实权威可以是唯一main工作树，或显式同仓work主checkout；无main时只允许Git common-dir对应的真实work主checkout。错误不能再同时声称current_is_main。输出真实branch、HEAD与本地origin/main跟踪值，不把本地work等同远端已合入。

精简证据根仅增加`docs/validation/`，仍核已提交非空JSON、路径及符号链接不逃逸。显式JSON selector选择具体记录；PassLimited/LocalLimitedAcceptance保持有限范围，不满足正式任务Done。review从指定review_commit读取，核非作者和精确交付绑定；317的doc-only交付只在显式白名单与真实Git diff证明等价时关联原受审代码。源码/测试变化不得借metadata-only绕过。

限定测试沿用`tests/handoff.test.mjs`，保留原13个断言族，追加历史冻结/篡改、current失败不被归档遮蔽、docs未提交/错selector/自审/错commit、work权威和metadata-only源码变化拒绝、legacy-audit与只读检查。root持任务状态、正式规范摘要、最终主树和远端合入；本片不改其它任务文件，不跑正式全量。

Rust四目标：本片是交付记录工具，不改变语言安全、运行或内存表示；目标是诚实区分可复核当前交付与不可复核历史。未做Rust成本/维护配对或性能测量，未建立产品级突破。实际限定命令及结果在交付时追加。

## 实际限定结果

工具及清单checkpoint为47d8ec2f3，吸收root当前元数据迁移0c60f1d15后的共同头为d315af23。最终源增量ef2e14994035afec02e05021ec4bc49d56638c73仅补当前规范路径canonical检查及最近反例；最终文档/validation记录不改变工具与测试字节。

`ZIL_HANDOFF_REPORT=/tmp/handoff326-closeout-check.json node --test tests/handoff.test.mjs`实际25/0、129断言、0跳过。原13族及其69断言原样保留；新场景显式防当前错误被归档遮蔽、清单/对象/字节篡改、文档伪Pass、自审/错selector、metadata-only夹带源码、非canonical当前规范及零门验收。结果绑定工具/测试SHA，与ef2e14994提交字节一致，不叠加早期重放。

真实当前主树只读命令为`node /workspace/zil-326/tools/handoff.mjs --check --legacy-audit`，输入主树头c1a61ccb36067af253fbd6ff4cbdd52d2c70ee0c。实际`CurrentPass`/0当前错误，同时`LegacyAuditFail`、exit1、1924条严格历史错误；274条历史中220Unavailable、54Unreviewed，claims_revalidated=false。实际权威为main，origin/main本地跟踪值0d7d74f7；这些观察不等于326执行了远端操作。

早期可丢弃本地共享clone亦得到同样的分离结果，快照原字节SHA08ba15ebb24629e7786fef357529aa5d776d211a80a2ee73ed1eae1350b3f550不变。首次从原树显式指定另一仓clone时正确触发authority拒绝；改从clone真实cwd检查后通过，没有修改源码或root权威。两次环境中断不计Pass；未落地的canonical尾补丁在恢复后重新核实、实施和限测。

精简机器结果位于迁移清单的`/validation`，历史原记录仍在`/records`，二者的证据等级和状态分开。未改其它任务文件，不生成权威snapshot，不跑正式全量或语言/原生/产品验收，不push或自审。root及非作者仍负责最终集成和独审。
