# 施工、独立验收与集成

## 权威与边界

| 信息 | 唯一位置 |
|---|---|
| 语言语义、架构和拒绝边界 | `_zil_design.md` 及具名附件 |
| 阶段/产物接口与依赖 | `spec/interfaces.json`、`policy/dependencies.json` |
| 静态工程义务、案例和量化门 | `validation/engineering-contracts.json`、`validation/task-test-plans.json`、`conformance/index.json`、`validation/product-gates.json` |
| 实时目标、负责人、状态和阻塞 | 独立 `goal:goal.md` |
| 任务写集、过程与交付 | 各任务分支 `.task/<ID>.md` |
| 当前长期决策 | `docs/decisions/` |
| 源码、独审和验收事实 | Git 提交及随源码交付的精简收据 |

旧主树 `.task/`、`_zil_goal.md` 与重复任务索引已经退役；生效义务由静态合同承接，原过程按 Git 提交追溯。`goal` 不复制规范阈值；任务过程不复制实时看板。历史状态按原提交解释，不能标 Done 来通过本轮静态检查。

## 派工与交付

一任务一分支一 worktree。派工先记 Goal ID、父合同、实际规范/源码 SHA、依赖、写集、正例、最近反例、退出判据及证据级别。作者只改分配写集并跑限定测试；公共 schema、Cargo.lock、正式门由集成者单写。相交写集先串行集成再冻结后一个候选；无关任务可继续。源码检查点及时提交、推送、核远端 SHA；推送不等于验收。

语言或接口变更须列具体行为差异与反例，同步规范、接口、诊断和案例。生成、Analyze、Verify、lowering、机器行为各有自己的责任；手填中间值与模型只能按其等级报告。未实现能力不能通过改计划或将其标成永久 Unsupported 消失。重要选择提炼到 `docs/decisions/`，历史讨论留 Git。

## 执行与失败关闭

`tools/run-cases.py` 使用 `validation/task-test-plans.json` 和 `conformance/index.json` 的稳定 case ID。静态合同 ID 是迁移索引，永久文件以功能命名。case 登记与执行分开；执行报告须有精确候选提交、规范输入/fixture 哈希、工具链/profile、命令、预期与实际、退出码、选择/执行/断言数、失败字段及原始结果位置。零选择、缺 fixture、零执行/断言、Unsupported、超时、构建失败和基础设施异常均不得 Pass。拒绝例必须匹配阶段与诊断，且不得产生可运行产物。

每次运行写不可覆盖的 RunId；原始失败、缩减和未变异控制不得被后续成功覆盖。大型机器产物放 `out/runs/` 并按交付范围归档；主树晋升必要的精简收据、独立审核和最小失败输入。证据复用须证明源码、输入、目标、feature、工具链及依赖身份相同或不受影响，旧结果不能充当新提交的实际通过。

## 独审、集成与恢复

审核者不得兼任作者。冻结候选 SHA 后，核写集、规范、静态合同、案例和实际结果，再做适用的正式组合/原生门；审核绑定原交付提交。待验缺交付、缺证据或审核对象漂移必须退回。集成后更新 goal 的状态；main 保留可复现的静态义务与必要裁决，过程只在任务分支和 Git 历史。合入不能抹掉被审核对象的可追溯身份；若 squash/rebase，另证明审查差异对应最终树。

独立准备实现、模型通过、一阶段测试或实施切片合入，只证明各自冻结范围；它们不解除父任务的前置依赖、完整接口及 Done/发布验收条件。父任务状态须由集成者依据全部规定案例和正式裁决更新。

冷启动先 fetch/read `goal:goal.md`，再跑 `node tools/handoff.mjs --check` 检查工程树。该命令不读取 goal 内容、不遍历全部开发分支，也不证明语言/产品通过。 不带 `--check` 生成的 `data/handoff.json` 只是本地工程快照，不提交主树；历史快照按原提交追溯。源码 ZIP 可在支持的工具链和锁定依赖已准备的环境中构建；恢复原历史和原始审计须按 `docs/DELIVERY.md` 的 manifest 与 bundle。干净源码克隆测试不等于空机器无缓存安装。远端状态须在 push 后读回精确 SHA；离线历史必须显式包含 goal 引用。
