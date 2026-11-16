# Full-push 精简恢复入口

唯一施工分支research/full-push-native；最新正式被测源码 `8f06a9ebb635e64b49aef5affdcd7ab7e0c0e639`。

[本轮精简结果](restart-bootstrap-chain-20261116/summary.md)：138请求/46格/5运行质量；原生冷3.973/R6.901s、首改1.033/R1.266s，重启构建+首改3.568/R1.287s仍输。直接重启首改profile3.715s/257原生输入过，Analyze1.169s与磁盘layout miss待修。check5000冷1.484/R.804s仍输，全流程6.832/6.832s近持平。

当前继续实现直接重启首改Analyze与磁盘类布局链，完成后集中验收。按[compiler-performance skill](../../../.agents/skills/compiler-performance/SKILL.md)，默认仅精简结果，不打包/复制SDK。独立Verify、原预算、完整O2、研究默认关闭；旧480e/bee7/98fa/021997数字不转记，goal/archive不合main。SDK18/native-link沿固定e85/V32恢复，不重建LLVM。
