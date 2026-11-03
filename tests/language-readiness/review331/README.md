# 331 非作者独立探针

候选`0ecb774253de1824cf3688d852e6759b9b1bc727`（生产/测试`51ff1af8`，末提交
仅changes）已独立复核：8源为4 Success、2错来源SafeReject、2 Copy数值
Unsupported；4正例API O0/O2共8次原生成功。新增`cache.rs`只由既有borrow目标
包含，检查真实live/disk、声明上界与实际来源分离及等长正文失效；含它的borrow
29/0，另三个目标14/9/8及限定Clippy通过。精确范围与限制以审核JSON为准。

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

原release CLI对应40fb基线时，8份331探针均在Expand未支持；这些旧结果不证明
新符号检查正确，仍保留在审核JSON。候选复核新构建了target334的CLI，未冒用旧
89891e二进制。每份源可用`zil check <source> --json`重跑，精确类别、阶段、
诊断和hash见`docs/validation/review331-by334.json`。限定Rust目标使用独立
target334/jobs1，不创建新Cargo target。

`component-planned.zil`是单源普通消费者：`View<T>`共享字段借用，经具体Meter的
默认协议形成具名Message，再由具体父闭包调用thread_scope。impl Self仍为具体
Meter；worker只接收i32，借入view只被父线程body读取，未假定泛型impl或跨线程
借入资格。它依赖329、330、331、332；当前基线首先在record模式解析拒绝，未构建
或执行。计划的显式main harness输出为`0\n`、exit0；不能据源或预期升级任何任务。

B的共用原生名绑定修补后，另增两份不属于原8源/历史52源的控制：
`primitive-binder-life-positive.zil`把真实Type binder与life都命名为i64，并在正文
调用返回真实原生i64的外部声明；应保留callee词法类型且以bool实例成功。
`primitive-binder-life-numeric-negative.zil`要求未用同名Type binder仍不能从Copy
取得数值权限。精确B实测结果另在334 final报告，不追改原331审核结论。
