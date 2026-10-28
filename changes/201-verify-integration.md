# 201 Verify/LLVM 集成记录

生产 `63a90ad9fed8ee526cb47a6317a25f95a26d7f56` 保持schema25：独立读取实际Closure捕获与live flags布局、检查非零元素和有界DAG，早期资源定义在Copy谓词及布局之前按调用者限制界定捕获类型；Store字段AddressField建立带父loan的环境位置，继承保守Store来源；LLVM使用实际Closure表求对齐，并按元素/字段顺序调用已有环境drop。

该精确clean提交的限定库测试为Verify246/0、LLVM18/0，共264/0，包含6个新增raw布局边界单元。它们不冒充源或机器证据。非作者后来发现Types误把Store元素非零条件施加给合法零payload Package捕获；root补`5e147ee3`的第7个单位近邻，实际1/0，明确零Package payload加shell/Managed两个live flags得到2字节真实环境，最终源和机器修复另由作者/非作者验证。

首次Clippy `-D warnings`在`54161766`报两项既有警告：`borrows/captures.rs`的复杂返回类型和`helpers.rs`的嵌套条件。修复只命名RoleAccesses类型别名并合并同一条件，不改变判断式、权限、预算或默认lint门；原输出保留于外置201-root/clippy01。Clippy复跑、全仓和最终身份分别记录，不能把旧失败覆盖为成功。

原200边界迁移见201-regression-migration.md。root限定复跑于clean`bf3f0da67c2356db5f4b9bb3356f6699ebb87f05`得到2/0（原3源各O0/O2共6次真实执行），旧172无上下文/捕获/零Store门另1/0。非作者MIGRATION仅是只读复核，不重计为执行。
