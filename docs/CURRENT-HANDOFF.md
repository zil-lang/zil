# 当前接续：泛型错误返回

已验收main `16ff72cb9caecc8176420b19658937e7b4c6bf4a`：[Type/Syntax](LANGUAGE-TYPE-SYNTAX.md)、[结构化外部回调](LANGUAGE-SCOPED-CALLBACK.md)、[资源准备](LANGUAGE-LOCAL-PREPARATION.md)、[具体闭包组合](LANGUAGE-CALLABLE-COMPOSITION.md)与[共享参数捕获](LANGUAGE-SHARED-PARAMETER.md)均已合入。schema/Wire32，Rust1.99.1、LLVM23.1.4。

292/293泛型直接enum错误返回修复已完成独审，冻结ad61e769经PR26验收。原唯一Choice正例误拒已修，Binding/call-arg两个合法旧缺口如实留后继。具体派工只在主树.task/impl，下一目标见[主线优先级](LANGUAGE-NEXT-PRIORITIES.md)。

主树工单由boss维护，集中提交，不逐条push；有用测试输入与精简结果入库，生成产物可清理。evidence历史清理和CP清理已完成，唯一成果CP25保留；263机制重写和264测试重组未合入。环境沿用原托管线程，不重绑或发布。历史handoff元数据仍有债务，不标Pass。完整1.4/ZUI NotSatisfied、B0 Doing，产品与原生Windows NotRun。

构建target固定到同一worktree；已完成目录也不跨树改名复用。曾因此误用旧Verify产物，原失败保留，清理后按实际源码复跑；当前两远端CI使用独立临时目录。
