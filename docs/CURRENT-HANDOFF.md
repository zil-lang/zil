# 2026-11-03 恢复后313–317、320–321本地有限验收

本轮权威主树 `/workspace/zil`、本地分支 `work`。真实恢复基线为 `7f039d16`；用户指定的301–312及集成9c3deb92、未提交源码均未恢复。补充旧对话说明记载当时未推送，不能归因于历史清理；可用来源审计已关闭，仅新源码附件或平台导出能提供新恢复入口。见[恢复边界](RECOVERY-20261103.md)。

313/321有限Copy tuple具体及泛型绑定、314/320 Type/Syntax普通Copy泛型与Struct Copy消费者、315普通完整Product同步可变helper已完成本地有限集成；316只核查life/trait/并发缺口，317以四模块实际组件组合核验。受测提交`a57a668b68044e37e870d058684f9364307e04b1`：默认4055/0、reference3947/0，零忽略/过滤；text64/0、migration9/0、fmt、Clippy、依赖和工具链检查通过。非作者318/319绑定各自精确实现；原失败、223期望迁移与317两后端12次实际机器结果见[本轮报告](LANGUAGE-RECOVERY-20261103.md)。

远端候选备份为`task/language-recovery-20261103`，各任务及review分支同时保存；完整bundle与最终校验清单在`/workspace/backups/zil-20261103/`。远端main仍为恢复基线，未宣称远端CI通过，工单保留待验而非父任务Done。后续报告/交接修改不改变全量受测的生产及注册测试输入。

当前大数组原规模在LLVM23.1.4的O0两后端仍崩溃，源和[精简复现](validation/recovery-large-array.json)已保存，生产修复尚未实现。life/Self/FP、一般trait、异步/线程/shared Mutex及完整基础模式仍待接续；完整1.4/ZUI和产品目标未完成。下一步只读[现行剩余清单](LANGUAGE-REMAINING.md)及[优先级](LANGUAGE-NEXT-PRIORITIES.md)，下方历史“下一步”不覆盖本节。旧handoff最终显式检查1976项错误：历史完整性债务及旧工具对main分支/evidence路径的要求仍不满足；check-plan规范hash债务保留Fail。现行规范注册摘要已核回实际字节，精简报告不迁回原始证据目录。

## 恢复时的已验收主线记录

# 当前接续：普通可变借入与Type/Syntax泛型组合

最新语言集成main `8bbb38f2d9e0c34a8bc28a44d12b309d6ff48392`，PR27受测候选`33542bff29621d271c3d82208ebb4770aeacf20e`：[错误联合上下文](LANGUAGE-GENERIC-INJECTION.md)已验收，前序拥有Event→dispatch→Reply、F转交、Type/Syntax、结构化外部回调和普通闭包/资源组合保持。schema/Wire32，Rust1.99.1、LLVM/Clang/LLD23.1.4。本地完整4005/0；两套最终CI均默认4005/参考3897/文本64、零失败/忽略/过滤。精确源、各次结果和原失败见[精简验收](validation/generic-injection.json)，不把重放相加。

294–299派定切片已验收，原两个合法泛型注入源已晋升。下一批集中普通完整Product局部值借给可变helper、Type/Syntax与普通泛型组合，独立写集并行；具体待实现项及成功对照见[剩余清单](LANGUAGE-REMAINING.md)，顺序见[主线优先级](LANGUAGE-NEXT-PRIORITIES.md)。同线程scope可覆盖长时间同步事件环，返回后保留捕获环境的登记与异步并发另按真实需求推进。

首轮新增全域符号不相等规则误拒两项旧合法程序；回修撤回过严要求，原178/194测试与清理预期不改。后续接入限制须先核现行有限特化合同和已有消费者，不能把潜在未来同型一律判成非法定义。实际同型实例与确定T!T仍拒绝。

主树工单由boss维护，集中提交，不逐条push；有用测试输入与精简结果入库，生成产物可清理。evidence历史清理和CP清理已完成，唯一成果CP25保留；263机制重写和264测试重组未合入。环境沿用原托管线程，不重绑或发布。历史handoff元数据及14份正式旧工单的规范hash仍有债务：check-plan报告28项旧hash错误，相关输入与本轮基线一致，不标Pass。完整1.4/ZUI NotSatisfied、B0 Doing，产品与原生Windows NotRun。

构建target固定到同一worktree；已完成目录也不跨树改名复用。曾因此误用旧Verify产物，原失败保留；当前本地固定target297，远端CI使用独立临时目录。元数据收尾提交不改变本轮受测的代码、测试、构建和规范输入。
