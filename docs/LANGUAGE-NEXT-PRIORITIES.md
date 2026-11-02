# 当前语言主线优先级

已合入：拥有Event→具体闭包/公共dispatch→拥有Reply、F原值转交、Type/Syntax、同线程结构化外部回调。最新已验收main为a7f0783a，见LANGUAGE-SCOPED-CALLBACK.md；历史阶段文档的旧“下一目标”不覆盖这里。

1. 273–275：泛型函数内具体资源准备已实现/独审，9c6a64ee正式验收中。
2. 277、278、280：并行补generic helper内具体Event→Reply handler与工厂取得的拥有body，统一源码/机器失败清理消费者。
3. 279：Analyze捕获窗口caller写漏判的层间一致性；Verify已有准确拒绝，未发现安全绕过。
4. 下一真实候选：参数来源共享Product捕获、更多一般泛型组合；先做源程序与正控，不凭旧Unsupported文本扩大施工。

276只清理CI原始归档等待，不占用语言实现写集。保持主线集中，不重启CP研究、测试目录大重组或Skill重写。跨线程/异步外部生命周期、完整1.4/ZUI与产品指标仍未完成。
