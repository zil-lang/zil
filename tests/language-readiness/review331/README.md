# 331 非作者独立探针

作者为`/root/impl315`，本组由`/root/language_map`独立准备。规范profile冻结于
主树`e28e08d8`。这里只新增源和预审收据，不改331生产或作者正在施工的borrow
测试文件，不创建Cargo目标。

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

当前release CLI仍对应40fb基线，8份331探针均在Expand未支持；这些结果只记录旧
边界，不是新符号检查通过。每份源可用`zil check <source> --json`重跑。精确
类别、阶段、诊断和hash见`docs/validation/review331-by334.json`；该JSON当前
`PendingCandidate`，没有受测作者候选、raw/Wire、live/restore缓存或native成绩。
作者冻结后才按实际结果更新，使用既有borrow目标及独立target334/jobs1。

`component-planned.zil`是单源普通消费者：`View<T>`共享字段借用，经具体Meter的
默认协议形成具名Message，再由具体父闭包调用thread_scope。impl Self仍为具体
Meter；worker只接收i32，借入view只被父线程body读取，未假定泛型impl或跨线程
借入资格。它依赖329、330、331、332；当前基线首先在record模式解析拒绝，未构建
或执行。计划的显式main harness输出为`0\n`、exit0；不能据源或预期升级任何任务。
