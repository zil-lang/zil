# 第二轮正式CI暴露的旧断言迁移

原CI 41084362447、head 2270a16abe3c04d50ca412ba3fe997c3ed2b9ec9、实际checkout a056f6e7569d8c6cf0a67acfac45dca19950e973：默认完整2858通过/3失败/0忽略。参考阶段没有完整收尾收据，记Incomplete；后续门NotRun。本地初试又暴露214断言应使用完整名义名`@module0:Root`，该Fail与两次原生产物一起保留。

- 192的三个原source保持逐字：普通`&state`与有限Copy资源头`&state.r`现在Verify成功且验证实际执行一次；ByteBuffer头仍在Types拒绝。
- 214原`Store<Root>`源、文件身份和阶段保持；其完整Product资格晋升到Types成功，并检查OwnedStore元素的完整Package名。这条用例不声称Verify/native成功。
- capture-view原raw/Wire攻击仍双拒；只将预期needle改成当前明确诊断。immutable rebinding反例保持。文件虽位于src，模块由`#[cfg(test)]`引入，所以这是一处测试字节变化；旧冻结manifest不改写，也不声称209条仍全部相同。

限定复验40个driver测试及1个Verify测试通过，fmt、两项限定Clippy通过。workflow所有测试门、features、limits、45分钟时限和no-fail-fast保持。每阶段native-evidence先生成逐文件SHA-256清单，独立artifact上传成功后，才在重核清单一致的条件下释放该阶段产物；空阶段亦需清单。隐藏文件纳入上传，symlink拒绝。已用原文件变更、空目录、隐藏文件和symlink控制检验清理边界。此修复针对已证实容量风险，不把尚未取得原错误的runner退出原因写成磁盘耗尽。
