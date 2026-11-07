# ZIL 实施入口

1. 冷启动先读独立 `goal` 分支的 `goal.md`：`git fetch origin refs/heads/goal:refs/remotes/origin/goal` 后 `git show refs/remotes/origin/goal:goal.md`；离线只读已保存版本并注明SHA。再运行 `node tools/handoff.mjs --check` 定位工程主树；该工具不读取实时看板、不检查全部开发分支。精简源码ZIP可以构建，恢复计划还须携带goal引用；缺Git按`docs/DELIVERY.md`恢复，不能把无Git检查当Pass。
2. 语言与架构只读 `_zil_design.md` 及其具名附件。当前目标、优先级、分配、工作状态只在 `goal:goal.md`；实施说明维护于各自开发分支的 `.task/<ID>.md` 或 `.task/impl/<号>.md`。main既有工单保留为工程合同/交付记录，其状态不是实时待办；重要决策、复核和验收随源码交付。详见`.agent/goal-branch.md`。
3. 每单一个分支和worktree，先核goal任务ID、实际派工、父合同、规范摘要、基线、依赖与写集。开发分支.task不复制实时看板；旧工单状态按原提交解释。切片合入不替代父任务Done；缺前置只报告具体阻塞，不猜测。
4. 施工方不合入、不改正式性能门、不自行跑正式全量；限定测试允许。独立审核绑定精确交付提交，审核者不兼任施工者。正式验收、工程合同、Cargo.lock和公共schema由集成方管理；实时进度按任务ID归并到goal。
5. 不把ZIL转Rust后调用宿主借用检查器作为安全核；不恢复通用gen/open/lease；不因编译失败隐式加入RC、锁、堆盒或全局表。
6. 生成器只产生未核代码；属性不制造权限；verify不得调用analyze/types的求证函数。依赖边见policy/dependencies.json。
7. 提交真实源码正例、最近反例、阶段IR/接口、机器表示、失败清理、实际命令与证据。手填IR、手写C和模型只能按对应等级报告；聊天和未跟踪文件不算交付。
8. 任何接受性、ABI、错误/清理顺序或预算变更先写changes提案，不在实现中暗改。发现规范冲突只暂停相应工作，引用具体ID。
9. 通用协作流程见docs/WORKFLOW.md；当前长期决策在`.agent/`，源码事实在Git，实施往来随任务分支保存。goal永不合入main，开发分支不合入goal，禁止在编译中的工作树切换到goal；只正常快进推送并核回远端SHA。模块只增加本模块接口与命令，不复制语言规则。
10. 每轮交付前按docs/RUST-GOALS.md简表反思Rust四目标及至少一项突破，区分实证、未测和未完成；不得以测试数量或更短语法代替产品结论。只引用既有product-gates条件，不增加长审批链。
