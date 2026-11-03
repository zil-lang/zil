# handoff：当前核验与历史可用性

`docs/GIT-HISTORY-CLEANUP.md`记载旧Git对象及raw evidence的删除。原工单的“已合入”是历史记载，不等于现在还能复核。326不改删这些原文件，也不把可读的旧提交自动重审为Pass。

## 冻结迁移

`docs/validation/handoff-migration-326.json`只列root明确归档的274条记录，来源固定为`fe4cb19ae9bf82cedc1c80dd7677808aa46b4072`：247条原已合入记录、25条旧在途记录和153/158两份原始笔记。每项固定原路径、原字节SHA、原状态、实际Git引用及对象存在性、原证据在该快照中的存在性。清单自身也须提交。改变路径、原字节、引用或存在性不能获得豁免；修改/删除原归档文件会使当前门失败。

181、14份正式任务、313–321当前交付以及后续任务不在归档清单。未列任务默认按当前规则严格检查；不会按编号、日期或“产生很多错误”自动跳过。root保留当前任务的真实责任，迁移实际base/hash并把旧引用留在historical字段，不把Planned或B0 Doing改成Done。

## 输出和命令

```text
node tools/handoff.mjs --check
node tools/handoff.mjs --check --legacy-audit
node tools/handoff.mjs --check --main-tree PATH
```

现代清单模式的`status`是`CurrentPass`或`CurrentFail`，并明确给出`current_gate`。`historical_status`单独显示`Unavailable`或`Unreviewed`、各自数量及`claims_revalidated=false`。原有工单首行状态完整保留在历史记录中；退出当前工作队列不等于任务Done或语言义务取消。

`--legacy-audit`强制只读，另外逐条执行清单内原历史记录的规范快照、Git对象、证据、审核及祖先关系核验；当前任务继续按当前字段核验，保留的historical字段不反向充当旧验收。`LegacyAuditFail`保留这些真实失败，不能用当前门通过覆盖。缺清单的旧API保持原严格`Pass`/`Fail`行为和检查条件，不提供历史豁免。缺Git或基础设施故障仍失败。

生成快照仍须在已验证权威checkout根运行不带`--check`的命令；分树仅检查。快照省略运行时绝对根路径，接续须重新检查。历史清理丢失的对象/证据不会由旧bundle提示自动恢复。

## 真实权威

优先找唯一`main`工作树；没有main时，只可识别Git common-dir对应、实际检出的`work`主checkout。显式路径必须同仓并为main或该work主checkout。普通施工树、未知分支或别仓不能冒充权威；失败时`authority.verified`和`current_is_main`均不能为真。

`authority.branch`、当前HEAD和本地`origin/main`跟踪值分别输出。工具不联网、不从work名称或本地祖先关系宣称远端合入。root按实际PR/远端结果更新状态。

## 精简证据与审核

证据仅允许仓库内`evidence/`或`docs/validation/`的已提交、非空JSON，路径与symlink均不得逃逸。未跟踪文件、错误selector、失败/零执行、错交付和自审保持失败。JSON Pointer必须显式选择一个结构化记录，不递归搜任意Pass。

review优先读取精确`review_commit`中的JSON，再按明确selector/commit字段匹配reviewer、verdict和实际交付。318的分项记录由清单显式adapter定位；319可直接绑定顶层精确交付。317受审代码与最终交付之间只增加changes文档，单独列出两SHA及逐路径白名单，工具实际核Git祖先和diff；出现源码或测试变化就失败。

`PassLimited`只可用于明确有限切片的审核；`LocalLimitedAcceptance`还须绑定该切片交付、实际受测提交和非零通过门。它们不变成全任务或产品Pass，不能满足正式任务Done。316的源码盘点没有独审Pass，不自动补造。

## 验证和边界

沿用`tests/handoff.test.mjs`的真实临时Git仓库与CLI。原13族保持；新增场景检查当前/历史隔离、冻结与未提交清单、未列当前错误、历史笔记、work权威、docs和symlink、精确review selector、有限范围、metadata-only与当前规范必需集合。检查只验证记录关系，不认证人员身份、评审质量、ZIL安全或产品胜出。

最终源ef2e14994的限定自测25/0、129断言；真实主树CurrentPass/0与历史audit的1924项失败同时保留。清单`/validation`只保存工具验证结果，可用显式selector引用；`/records`原历史仍Unavailable/Unreviewed，不能从验证工具的Pass推导历史Pass。权威snapshot留给root最终生成。
