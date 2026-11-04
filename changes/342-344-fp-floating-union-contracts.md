# 高阶借用函数指针、浮点和匿名Sum合同

按本轮完整语言实施授权细化现行终态要求。当前规范此前未固定浮点转换与匿名Sum成员选择，本次明确IEEE binary32/64、rounded float-target/exact-integer-target转换，以及带tag的安全结构Sum；不把其与unsafe raw union混同。见_zil_design §§2.4、15.2、18.1。

新增公共表示为Type28/29浮点、31借用薄FP，Constant11/12原始浮点位、CtScalar8/9；ConvertNumber opcode98区别于旧整数ConvertInteger。所有原始字段仍未核，版本同在待验schema35/CT2，不在生产核完成前宣称可执行通过。各施工分支可持其字段子集，最终合并版本必须同时具备所有生产/消费者及独立核。

最近反例覆盖NaN比较、signedzero、最大整数舍入边界、NaN/Inf/fraction转整数、CT与目标差异、不同名义但同布局Union成员、重复成员、非活动载荷及清理；FP伪造返回关系、共享升级可变、不同调用life混同、实参相交、逃逸局部、环境/外部函数冒充薄Direct指针。适用预算/错误清理与机器实证不省略，原用户终态目标和产品门未降。
