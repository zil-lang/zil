# 当前接续：Type/Syntax 验收与外部回调

已验收主线包含拥有 Event → 具体闭包/公共 dispatch → 拥有 Reply，以及回调原值转交。历史清理后远端 main 为 `f8f750e6c2d1065b9343cbbd272bbc4a28093d42`；迁移后的已验收源码为 `f8116c84374c1bcd2c6239d0e7b6f023ffe8de9e`。旧证据目录已按用户要求连同Git历史移除，七个必要输入保留在tests/fixtures，见[清理记录](GIT-HISTORY-CLEANUP.md)。

257–265 Type/Syntax 精确源码 `bea1e95b0826dd3e695396cac1665d6a74ad38ec` 已通过本地默认3857/0、参考3749/0、text64/0、格式、Clippy和迁移9项；独立审查及原失败见[阶段验收](LANGUAGE-TYPE-SYNTAX.md)。远端CI与正式main合入仍待完成。运行产物可清理，不再创建永久原始证据归档。

266–272已在隔离分支并行施工：公共schema/Wire32、前端与实际所有权、独立Verify、LLVM后端、真实三模块+C消费者、CT/driver适配及独审。目标为同线程结构化 `c_with_callback`，公共wrapper内反复调用F、一次消费body、关闭确认无未来入口后才释放环境。已交兼容接口可编译，尚未形成源到原生全链验收，不能标完成。

主树工单是任务状态入口，分树副本不维护；源码和小型结果集中提交，不逐条状态push。CP清理已完成，唯一成果的CP25保留，263机制重写和264测试重组未合入。环境沿用原托管线程，不重绑或发布配置。handoff历史元数据仍有债务，不把它写成Pass。完整1.4/ZUI NotSatisfied、B0 Doing，产品和原生Windows NotRun。
