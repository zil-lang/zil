# ZIL 实施入口

1. 冷启动先运行 `node tools/handoff.mjs --check`，从输出的 `authority.main_tree_now` 定位主树，再读主树 `_zil_goal.md` 和派定工单。精简源码ZIP可以直接构建；缺Git时按`docs/DELIVERY.md`使用`PACKAGE.json`命名的旁置恢复附件内历史bundle恢复checkout，不能把无Git的检查当Pass。历史绝对路径只作来源记录，不作为当前主树地址。
2. 语言与架构只读 `_zil_design.md` 及其具名附件；历史文件、对话和changes不能覆盖当前规范。正式工单为主树 `.task/<ID>.md`，编号实施切片为主树 `.task/impl/<号>.md`；分树副本不读不改，必要往来只追加主树工单。
3. 每单一个分支和worktree，核对规范摘要、基线、依赖与写集。正式任务须为Ready/Doing；实施切片须有明确父任务、基线与派工，处于施工或退回。切片合入不替代父任务Done；缺前置只报告具体阻塞，不猜测。
4. 施工方不合入、不改正式性能门、不自行跑正式全量；限定测试允许。独立审核绑定精确交付提交，审核者不兼任施工者。正式验收、主树工单状态、Cargo.lock和公共schema由集成方管理。
5. 不把ZIL转Rust后调用宿主借用检查器作为安全核；不恢复通用gen/open/lease；不因编译失败隐式加入RC、锁、堆盒或全局表。
6. 生成器只产生未核代码；属性不制造权限；verify不得调用analyze/types的求证函数。依赖边见policy/dependencies.json。
7. 提交真实源码正例、最近反例、阶段IR/接口、机器表示、失败清理、实际命令与证据。手填IR、手写C和模型只能按对应等级报告；聊天和未跟踪文件不算交付。
8. 任何接受性、ABI、错误/清理顺序或预算变更先写changes提案，不在实现中暗改。发现规范冲突只暂停相应工作，引用具体ID。
9. 通用协作流程见docs/WORKFLOW.md；当前长期决策在`.agent/`，源码事实在Git，往来在主树工单。模块只增加本模块接口与命令，不复制语言规则。
10. 每轮交付前按docs/RUST-GOALS.md简表反思Rust四目标及至少一项突破，区分实证、未测和未完成；不得以测试数量或更短语法代替产品结论。只引用既有product-gates条件，不增加长审批链。
