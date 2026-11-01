# 236 修复后冻结Q：原样364次实测

本轮精确受测干净源码为`e92508be996a190fa188d08319d37cc8b522bae6`，实际CLI SHA为`ebc418f594c91f1c54b4f655cf6fdd0d99a9461852aec8e1f7ad09701d433395`。用独立target-236/jobs1、显式llvm-api实际构建后，重新执行原五模块ZIL和冻结oracle；不复用P、42或0df的矩阵，也不改变core源码、测试、evidence或HEAD。

O0/O2主矩阵共350次（48普通、302实际LLVM API对象分配探针，206次真实申请失败），另有14次当前控制（4普通、10探针，2次真实申请失败）。本代理共364次新机器执行全部匹配输出、退出及分配模型；每次探针最终live0、allocated=frees。无捕获defer控制为零分配。最大12个存活分配、417B请求payload仅是这些输入的实际观察，不是RSS、全部输入上界或Rust比较。

六次原生构建均使用单个经验证的LLVM API builder对象。普通执行运行原CLI产物；故障探针捕获实际链接对象，用固定realloc/free shim重链接，没有手写C/IR替代ZIL。source/阶段IR/接口/原对象/机器结果与命令的哈希定位见ARTIFACT-MANIFEST；源与oracle前后全部匹配。原有P收据及非作者审核保持原样，不能被当成本轮Q执行。Q上的source负例由root同期default/reference全门统一记账，本代理没有将旧候选负例冒充Q。

本矩阵证明有限完整Session、Store child、同步helper、整值事务与defer退出/失败清理确实共同行为；不代替237全部泛型身份、capture refill或其他238反例。Rust四目标方面已有有限安全、布局及自然API表达证据；未做运行/RSS、维护或编译成本配对，本轮不采样性能。没有产品级四目标合取或可重复净突破结论；正式全仓门由root另记，完整ZUI/1.4、产品和原生Windows不随364次限定执行升级。
