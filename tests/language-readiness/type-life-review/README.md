# 类型、life 与来源探针

本目录的 Source 检查符号类型与 life 参数的独立名字空间、实参替换、Copy 数值权限和未用定义。它们是有限正反例，不因原目录编号或旧编译器的 Unsupported 结果改变规范。逐源预期如下：

| 源 | 实现后必须验证的条件 |
| --- | --- |
| `order-and-explicit-type.zil` | 任意type/life次序；显式实参只计普通T；i32/i64推导与显式调用一致。 |
| `life-does-not-shadow-alias.zil` | life Context不隐藏外部类型别名Context。 |
| `renamed-and-permuted-callee.zil` | callee生命名换位后按真实参数替换；返回第一对象不应阻止修改第二对象。 |
| `unused-wrong-return-root.zil` | 未实例化的错返回根仍拒绝。 |
| `unused-copy-is-not-numeric.zil` | 未用模板不能从Copy取得数值运算权限。 |
| `used-copy-is-not-numeric.zil` | 即使i32实际调用可做数值运算，也不能替符号模板补证明。 |
| `callee-same-spelling-wrong-root.zil` | 两函数相同life拼写不能遮蔽实参换位造成的错来源。 |
| `project/main.zil`及`project/api.zil` | 跨模块CT类型查询重建、显式T、真实返回来源及后续cache检查。 |


`component-planned.zil` 是具体 Meter 默认协议、具名 Message、共享字段借用与父闭包 `thread_scope` 的组合：worker 只接收 i32，借入 view 只由父线程读取，不授予泛型 impl 或跨线程借入资格。显式 main 的成功预期是 `0\n`、退出 0。

`primitive-binder-life-positive.zil` 同时将真实 Type binder 与 life 拼写为 `i64`，要求 callee 词法类型不被 life 遮蔽，bool 实例可运行。`primitive-binder-life-numeric-negative.zil` 要求未用同名 Type binder 不能仅凭 Copy 取得数值权限。所有来源错误与未用定义都应在定义/调用实际入口受检。

每份源可用 `zil check <source> --json` 在当前编译器重跑，正例仍须按范围执行机器测试。候选 `0ecb774253de1824cf3688d852e6759b9b1bc727` 的精确类别、阶段、hash、独审和有限机器结果见 [冻结审核](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review331-by334.json)。该审核绑定自己的源码，不是当前 checkout 的自动通过。
