# ZIL 代理入口

1. 先核 checkout、分支/worktree、未提交和未跟踪源码；再 fetch/read 独立 `goal:goal.md`，记录两者 SHA。禁止 reset/clean，不在构建中的树切分支。接续命令见 [工作流](docs/development/workflow.md)。
2. 按当前任务读取相关规范、模块和短合同。语言权威是 `_zil_design.md` 及具名附件；接口/规则/静态验收分别在 `spec/`、`policy/`、`validation/`。文档导航见 [docs](docs/README.md)，目录和命名见 [仓库组织](docs/development/repository-layout.md)。
3. 一任务一分支一 worktree，先约定依赖、写集、成功例、最近反例和退出条件。实现/独审按用户要求使用 sol high；疑难语义才交 Astra。无冲突写集并行；公共 schema、根 Cargo/锁、正式门与本地重型构建窗口由集成者单写/协调。
4. 永久文件、模块、测试以能力/不变量命名；任务号只留看板、分支、临时合同和来源字段。`.task/<ID>.md` 仅任务分支保留，合入前将长期内容归位并移出交付树。不得增加 CURRENT/HANDOFF/批次报告或主树实时状态副本。
5. Verify 独立，不调用 Analyze/Types 求证。生成代码与属性不制造权限；宿主 Rust 检查不代替 ZIL 安全核；不得隐式引入 RC、锁或堆盒。接受性/ABI/清理/预算变化同步规范、接口和最近反例。
6. 真实 Source、独立 raw/Wire、机器行为和模型分级报告；零选择、零执行、缺字段、Unsupported、超时或失败不得 Pass。失败与修复保留不同运行身份。按风险选门；独审不得兼任作者，必须绑定精确候选 SHA。完整规则见工作流与 [证据决定](docs/decisions/evidence-and-review.md)。
7. 推送后核远端 SHA，独立更新 goal；父任务只能按完整退出条件收口。goal 永不合入 main。源码备份统一用固定 `archive/source-history`；分支确认可恢复且无唯一修改后退役。恢复与工具环境见 [交付](docs/development/delivery.md)和[工具链](docs/development/toolchain.md)。
