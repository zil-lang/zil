# ZIL 实施入口

1. 先读独立 `goal:goal.md`：`git fetch origin refs/heads/goal:refs/remotes/origin/goal`，再 `git show refs/remotes/origin/goal:goal.md`。离线版本须注明 SHA 与时点。`node tools/handoff.mjs --check` 只检查工程/交付记录和 Git 关系，不读取实时看板，也不证明语言正确性；缺 Git 的恢复见 `docs/DELIVERY.md`。
2. 语言行为、安全边界、拒绝条件只以 `_zil_design.md` 及具名附件为准；阶段接口、策略和验收协议分别见 `spec/`、`policy/`、`validation/`。`validation/engineering-contracts.json` 承接原 14 份正式工程合同的静态义务。main 不保留 `.task/`；旧合同与过程按原提交和任务分支追溯。当前任务状态只读 goal，任务分支 `.task/` 保存过程合同，当前长期决定见 `docs/decisions/`。
3. 一任务一分支一工作树，先固定规范、源码基线、依赖、写集与交付门。相交写集串行集成；公共 schema、根 Cargo/锁与正式门由集成者单写。施工方跑限定测试，正式全量和性能门由集成者协调。独立审核者不兼任作者，审核绑定精确候选提交。
4. 提交真实源码正例、最近反例、阶段接口/产物、失败清理与实际命令。模型、手填 IR、手写 C、预期案例和聊天不能冒充生产源码及原生行为。零选择、零执行、缺字段、超时、Unsupported、基础设施错误和失败字段都不能判 Pass。失败和修复使用不同 RunId，旧失败保留。
5. 生成器只产生未核代码；属性不制造权限；独立 Verify 不调用 Analyze/Types 求证。不可借宿主 Rust 借用检查器代替 ZIL 安全核，也不可因编译失败隐式引入 RC、锁或堆盒。依赖边见 `policy/dependencies.json`，设计理由见 `docs/decisions/compiler-boundaries.md`。
6. 接受性、ABI、错误/清理顺序、预算变更先列差异与最近反例，同步规范、接口、案例和验收。未采纳提案仍是提案；发现冲突只暂停受影响工作。每轮交付按 `docs/RUST-GOALS.md` 简表区分实证、未测和未完成；阈值只引用 `validation/product-gates.json`。
7. 通用派工、独审、合入与收据格式见 `docs/WORKFLOW.md`；历史恢复见 `docs/DELIVERY.md`。goal 永不合入 main，任务分支不合入 goal；禁止在编译中的工作树切换分支。推送后读回远端精确 SHA，不能把本地提交或待上传包称为已交付。
