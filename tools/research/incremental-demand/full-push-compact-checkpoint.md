# Full-push 精简恢复入口

唯一施工分支research/full-push-native；最新正式被测源码 `175987698fd27ccd26a99e7bf52c7634a6409d44`。

[直接重启首改整链结果](direct-restart-edit-chain-20261116/summary.md)：正式132请求/44格/4运行质量通过，另三请求profile/257原生输入过。原生冷3.772/R6.991s，但直接首改3.077/R1.424s仍落后；check冷1.468/R1.165s，连续八私改.995/R3.076s。原生用户边界峰42.416/R65.934MiB，check13.637/R58.355MiB。

已完本批构建/测量，没有待完成进程。剩余首改Codegen（O2对象/机器/分区）、重启bootstrap与checkcold；全面/main/工业门仍开放。按[compiler-performance skill](../../../.agents/skills/compiler-performance/SKILL.md)，直接修改先于新进程首个请求，不能先无改预热。只保精简结果，不默认大归档；原Verify/预算/完整O2/研究默认保持。旧8f/480e/bee7/98fa/021997不转记，goal/archive不合main。SDK18/native-link固定e85/V32，不重建LLVM。
