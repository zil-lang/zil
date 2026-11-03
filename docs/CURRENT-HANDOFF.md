# 当前接续：普通可变借入与Type/Syntax泛型组合

最新语言集成main `8bbb38f2d9e0c34a8bc28a44d12b309d6ff48392`，PR27受测候选`33542bff29621d271c3d82208ebb4770aeacf20e`：[错误联合上下文](LANGUAGE-GENERIC-INJECTION.md)已验收，前序拥有Event→dispatch→Reply、F转交、Type/Syntax、结构化外部回调和普通闭包/资源组合保持。schema/Wire32，Rust1.99.1、LLVM/Clang/LLD23.1.4。本地完整4005/0；两套最终CI均默认4005/参考3897/文本64、零失败/忽略/过滤。精确源、各次结果和原失败见[精简验收](validation/generic-injection.json)，不把重放相加。

294–299派定切片已验收，原两个合法泛型注入源已晋升。下一批集中普通完整Product局部值借给可变helper、Type/Syntax与普通泛型组合，独立写集并行；具体待实现项及成功对照见[剩余清单](LANGUAGE-REMAINING.md)，顺序见[主线优先级](LANGUAGE-NEXT-PRIORITIES.md)。同线程scope可覆盖长时间同步事件环，返回后保留捕获环境的登记与异步并发另按真实需求推进。

首轮新增全域符号不相等规则误拒两项旧合法程序；回修撤回过严要求，原178/194测试与清理预期不改。后续接入限制须先核现行有限特化合同和已有消费者，不能把潜在未来同型一律判成非法定义。实际同型实例与确定T!T仍拒绝。

主树工单由boss维护，集中提交，不逐条push；有用测试输入与精简结果入库，生成产物可清理。evidence历史清理和CP清理已完成，唯一成果CP25保留；263机制重写和264测试重组未合入。环境沿用原托管线程，不重绑或发布。历史handoff元数据及14份正式旧工单的规范hash仍有债务：check-plan报告28项旧hash错误，相关输入与本轮基线一致，不标Pass。完整1.4/ZUI NotSatisfied、B0 Doing，产品与原生Windows NotRun。

构建target固定到同一worktree；已完成目录也不跨树改名复用。曾因此误用旧Verify产物，原失败保留；当前本地固定target297，远端CI使用独立临时目录。元数据收尾提交不改变本轮受测的代码、测试、构建和规范输入。
