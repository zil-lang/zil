# 196全仓旧夹具迁移

首五轮因公开Rust结构旧构造器未迁移而编译失败，尚未执行测试。保留日志。第六轮实际2122通过/4失败/0忽略，四项定位如下，未修改生产以迎合断言。

- 182 helper地址签名关闭、188未知嵌套环境关闭仍在原phase/category/code拒绝；仅把旧单角色诊断文字改为实际多角色文字。原source不动。
- 190的同ClosureId工厂实参替换攻击，Analyze现先从实际来源识别不支持的borrowed factory forwarding，返回Unsupported；独立Verify raw/Wire仍精确SafeReject/E-VERIFY-CALLABLE-CONTRACT。只为该原攻击分别声明两个阶段的既定预期，其余攻击保留共同category。原IR篡改、candidate重绑定及Wire均不改，不把Analyze的Unsupported宣传为Safety proof。
- 194的两个未实例化多角色定义如今属于196的明确准入；逐字保留并改为定义检查正控制，再补真正调用同F双实例与F/G的O0/O2。原无bound、混合mode和递归关闭项另保留。

原始2122/4及早先编译失败进入最终收据；新的全量结果另绑定提交。
