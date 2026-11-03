# 已确定闭包错误参数的注入

基线4bea9c88。独审非泛型闭包正控在Types因aggregate形状不同误拒。
`call_closure`先把range_hint直接送进inference.argument，再做argument_expr；
已确定E!T形参因此在正常注入前被当作必须与payload类型完全相同。

按既有§15.2规则，已确定的E!T上下文使用正常argument_expr核查实际值及
唯一注入；预收约束不能先把payload与完整union强行等同。完整union原值
保持；两侧同时/均不匹配、默认i32与错误名义类型照旧拒绝。实参按原顺序
求值一次，Fn/FnMut/FnOnce资格、callee暂存及错误清理均保持。

范围限已知签名的普通具体闭包调用；不扩普通closure签名、不补尚无明确
类型上下文的泛型enum推断，不改schema/ABI/默认限额。复用已有功能测试
目标，不新建工单命名Cargo目标。294负责Expand，其符号歧义/Some(f())
修复独立；298与其按真实组合消费者集成，299非作者核精确提交。
