# 设计、总计划与工单协作规程

## 1. 唯一定义源

| 信息 | 维护位置 | 引用方式 |
|---|---|---|
| 语言行为、架构不变量、拒绝边界和验收族 | `_zil_design.md`及其具名附件 | 稳定规则ID；不在工单重写条件 |
| 量化预算与场景协议 | `validation/product-gates.json`及批准manifest | 只引用profile，不抄出第二套阈值 |
| 阶段和覆盖分工 | `_zil_goal.md` | 引用阶段和工单 |
| 阶段schema与生产者/消费者 | `spec/interfaces.json` | 具体schema与artifact ID |
| 工单状态、写集、交付与裁决 | 主树`.task/<ID>.md` | worktree只持派工快照 |
| 规范源样本、未来协议夹具 | `conformance/index.json`、`validation/task-test-plans.json` | case ID，不把登记当执行 |
| 实际执行证据 | `out/runs/`原始运行，`evidence/`精简收据与反例 | 绑定提交/输入/工具链/profile，大型原始产物独立归档 |
| 变更原因与历史 | `changes/`和Git | 不覆盖当前规范正文 |
| 当前工程决策 | `.agent/` | 一点一文件，只留当前规则与边界 |
| 编号实施切片和往来 | 主树`.task/impl/<号>.md` | 只引用正式父任务，不替代其Done门 |
| 冷启动摘要 | 生成的`data/handoff.json` | 快照而非权威；接续时重新运行handoff检查 |

## 2. 输入快照和工单状态

正式状态为Planned→Ready→Doing→Review→Done；返修回Doing，取消必须记录原因及能力的新承接者。本仓已有实际编译器与标量证据，完整公共schema仍由B0交接；状态只从主树工单读取，设计检查和独立准备代码均不触发整单完成。

`spec_hash`是设计正文原始字节的SHA256。`normative_hash`覆盖`spec/interfaces.json`列出的全部规范输入，算法为按路径排序后拼接`path + NUL + SHA256(file bytes) + LF`，再对UTF-8结果取SHA256；`python3 tools/check-plan.py --print-hashes`输出当前值。派工还记录任务revision、实际Git基线和案例/工具链输入哈希。

Ready必须有实际base_revision、当前两个hash、明确产物与写集、全部Done前置和所需接口；Planned允许空基线。Doing/Review不能因等待期间的规范变更自行猜测新语义。完成状态只能由验收方根据真实结果和集成裁决回写。已合入实施切片保留当时的规范摘要；handoff从精确implementation_commit读取接口声明的必需规范集合，核对注册表集合完全一致，并逐项读取全部输入字节核验，不能把旧hash追改成新规范。在途切片仍锁当前规范，变更须由集成者记录迁移与影响范围。正式任务仍以当前规范维护；其整单完成门没有因子切片合入解除。

## 3. 任务字段与交接

`required_artifacts`引用已完成生产者的交接项，`provided_artifacts`由本单唯一生产；接口字段和消费者在interfaces中登记。交接包含实际源码提交、输入摘要、schema版本和验证证据，artifact名字本身不证明存在实现。

`case_ids`指向已有`.zil`规范样本；`plan_case_ids`指向本单必须实现的测试夹具要求。后者的required_fixture路径和观察要求在task-test-plans中给出。工单的test_commands固定统一runner的调用契约，done_when细化交付判据，不能新增或放宽规范。

规范表面语法、类型/捕获、分析、核验和lowering必须对同一节点有真实消费者。stage接口不允许用同名mock冒充；测试可以使用夹具，但证据注明等级。所有缺失必需阶段返回Unsupported并停止生成可运行产物。

## 4. 并行与写权

一单一枝一树，正式任务使用`task/<ID>`，编号实施切片以编号命名分支、目录`../zil-<号>`。每树使用独立target、缓存和临时产物；共享仅限只读下载缓存。验收方独占发号、状态、集成及根共享配置，施工方只写分派范围和本单证据。

B0串行持有建立正式最小链所需的临时广写权；完成后移交各模块，不能仍与模块施工并行修改。T01/T02/T04/T05在接口冻结后互不相交，可并行；后继任务只有在前驱Done后接手相交功能目录。`tools/check-plan.py`对无祖先关系的任务保守检查写集重叠；冲突时缩小写集或显式串行，不能忽略告警。

公共schema、根Cargo/锁、工具链和共享模块注册入口由集成者单写。改变接口先提交影响生产者/消费者、最近反例和迁移步骤，再同批发布新快照；不要求整仓停工，未受影响任务继续。T08/T09虽然横跨多个crate，也只在所列功能分区接手；对父级注册文件的修改由集成者提交。

## 5. 统一runner与证据

现有`tools/run-cases.py --task ID --manifest PATH --out-root evidence/ID`支持conformance/index和task-test-plans两个manifest，创建新的RunId目录，不覆盖旧日志。只把真实执行结果用于裁决，不把登记的预期或空选择改成Pass。

`expected.exit_code`只比较真实main进程退出状态；`expected.entry`加`expected.return_value`由原生harness调用具名函数并读取完整类型值，包括超过255的值。二者不得互换，也不能以常量解析代替执行。

每次至少记录任务/提交/规范和输入hash、精确命令、工具链与profile、实际选择/执行/断言数量、逐case预期与实际、stdout/stderr、退出码及阶段/机器产物hash。预期拒绝需要匹配注册的阶段和理由；任何编译器崩溃或工具失败都不能代替正确拒绝。零选择、缺fixture、零执行/断言、Unsupported、ResourceLimit、超时和基础设施失败均非Pass。

Done需要`acceptance_record`及真实result.json定位；所有规定case都有实际执行。不同证据等级分别记录：规范、模型、合成工具输入、手工lowering、生产源贯通、目标机器行为、产品比较；不得相互冒充。模型/压力调度不替代弱内存和真实OS/驱动证据。

## 6. 复核、合入与复用

施工方执行限定测试并提交请验报告，不自行合入或跑正式全量。验收方固定候选提交，先核输入/写集/依赖，再按风险跑受影响闭包；规范、schema、安全核、清理、构建规则改变属于高风险。普通局部cargo check不因全量锁而全部串行。

复用证据必须说明源码、测试输入、feature、target、规则、工具链和产物依赖未变或已证明不受影响；“刚测过”不是依据。相交分支依次集成，后合分支吸收已合变化后才固定候选。正式计时独占资源，不与构建/压力测试竞争。

验收方更新主树工单状态；worktree里的工单副本不读不改，派工信息提供主树权威位置。未派工任务保持Planned，施工方不得自己把缺实现状态标Done。已有旧证据归档到对应旧版本；新规范下不沿用旧截图/检查计数冒充本次执行。

## 7. 变更和阻塞

不改变接受性/行为的文字澄清可以轻量处理；影响模式、错误、求值/清理顺序、来源、ABI或支持范围的变更必须在changes列出具体差异和最近反例，同批更新规范、接口和案例。性能数字不得在排期或工单另改。

发现冲突只暂停受影响任务，引用具体规则/案例与最小复现；集成者在既有授权范围内修正规范或实现，明确发布revision。未完成的终态能力仍有责任单，不能通过取消任务或标Unsupported把它从发布条件中删除。

## 8. Codex冷启动与编号切片

先运行`node tools/handoff.mjs --check`。脚本从Git工作树中查找main，输出当前主树位置和相对工单路径；不依赖工单内保存的旧机器绝对路径。无法唯一定位时显式传`--main-tree PATH`，并验证它属于同一仓库的main分支。只有源码ZIP也可直接构建；要核验历史和继续原分支，另取配套历史bundle，使用其实际路径执行`git clone --branch main <history.bundle路径> zil-checkout`，再从恢复的checkout接续。主源码包只含按 Git 收录规则导出的源码、测试、工具和文档，不放证据、执行产物或 Git 目录；精简审计与可恢复历史另置附件，详见交付 manifest 和 docs/DELIVERY.md。

切片首行状态为施工、待验、退回、已合入或卡住，与JSON一致；JSON保存编号、正式父任务、完整基线、规范摘要、写集、实施者、交付提交、独立审核和合入提交。正文只由集成者调整。末节只追加开工、交付、退回/验收、合入的必要往来，不复制完整对话。

待验必须有`implementation_author`、`implementation_commit`和结构化evidence引用。已合入还要求`review = {reviewer, verdict: "Pass", implementation_commit, evidence}`，审核者与实施者不同、审核绑定精确交付提交，且`implementation_commit`和`integration_commit`都已进入main历史。本轮使用`git merge --no-ff`保留已审核原提交，保证仅携带main/HEAD的bundle也能恢复审核对象。至少一份审核JSON须包含与工单一致的reviewer、verdict和implementation_commit。证据必须是主树已提交的JSON结果，或交付提交中可读取的JSON结果；聊天、未跟踪日志和文件名本身不构成证据。合入后的工单记录也须提交。

handoff检查记录完整性，不验证审核者身份真实性、审查质量或语言安全，不替代check-plan和正式测试。Git子进程使用临时文件描述符捕获输出以兼容受限环境；任何进程基础设施错误都明确失败，即使同时报告退出状态零。施工只运行本单限定测试；独立审核由另一代理承担；全量由集成者统一执行。当前会话不搬入Cursor专用常驻监听机制，等待和重派由当前可用协作工具管理。

主树集成完成后执行`node tools/handoff.mjs`生成`data/handoff.json`；分树只允许`--check`。摘要不保存绝对工作目录，记录受测提交与实际当前HEAD而不要求它们相等，避免文档/证据提交造成自身哈希循环。接续时重新检查实时记录，不能用旧摘要直接判断完成。之后仍按`python3 tools/check-plan.py --self-test`与工单命令执行相应门。
