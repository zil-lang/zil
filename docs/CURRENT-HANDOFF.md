# 当前接续：泛型上下文与长期生命周期

已验收main `c3a5d3f5371b525de4729d62a7211c446055b7d4`：[Type/Syntax](LANGUAGE-TYPE-SYNTAX.md)、[结构化外部回调](LANGUAGE-SCOPED-CALLBACK.md)、[资源准备](LANGUAGE-LOCAL-PREPARATION.md)、[具体闭包组合](LANGUAGE-CALLABLE-COMPOSITION.md)、[共享参数捕获](LANGUAGE-SHARED-PARAMETER.md)与[泛型错误返回](LANGUAGE-GENERIC-ERROR-RETURN.md)均已合入。schema/Wire32，Rust1.99.1、LLVM23.1.4。最新默认3993/参考3885/文本64与两CI通过。

本轮277–280、282–293派定切片已验收；两个合法泛型初始化/实参注入缺口保留原源，随后继续长期外部生命周期。具体派工只在主树.task/impl，下一目标见[主线优先级](LANGUAGE-NEXT-PRIORITIES.md)。

主树工单由boss维护，集中提交，不逐条push；有用测试输入与精简结果入库，生成产物可清理。evidence历史清理和CP清理已完成，唯一成果CP25保留；263机制重写和264测试重组未合入。环境沿用原托管线程，不重绑或发布。历史handoff元数据仍有债务，不标Pass。完整1.4/ZUI NotSatisfied、B0 Doing，产品与原生Windows NotRun。

构建target固定到同一worktree；已完成目录也不跨树改名复用。曾因此误用旧Verify产物，原失败保留，清理后完整本地参考3882/0及剩余门通过；当前两远端CI使用独立临时目录。
