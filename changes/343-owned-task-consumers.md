# 343 Task38实际消费者与普通枚举

实现主规范§12已冻结的Task38合同，不新增公共Type或opcode。
`Task { result }`保存完整actual Type（包括Managed），并只作为拥有头/描述符的
sealed payload。普通`std.task`声明经现有Enum、ErrorUnion和泛型专化通路进入
Core；其实际声明身份和完整T进入接口/依赖身份，同形用户Enum没有Task权限。

`task_create`按普通调用顺序求值/转移实参一次。分配失败清理已转移输入一次，
返回`TaskCreateError.Allocation`；成功值是非空拥有Task。`task_poll`返回
`TaskPollState ! Poll<T>`；只有Ready拥有T且只转移一次，其他状态无T。
`task_poll_close`返回`Poll<Unit>`，Pending保留全部稳定责任，Ready才表示Closed。

物理拥有槽、部分初始化/live标志、结果槽和取消清理从实际责任状态推导。
保存的PC必须覆盖全部实际责任；不能拿producer计划或所有locals列表代替证明。
取消先通知/驱动子帧关闭，再按实际成功初始化责任顺序清理父值。
默认Direct Drop只适用于两核实际证明完整关闭/teardown图同步完成的profile。

公开结果T不描述隐藏输入/溢出槽的借入依赖。Task<T>不能仅从T得到
ClosedEnvironment/Transfer/Share；借入Task的返回、存储和跨线程出口需要实际
frame依赖合同。尚无合同的具体出口拒绝，保留原源反例，不擦除来源根。

施工按可执行消费者逐步扩大；只有已核profile才能进入后端。当前scalar兼容ABI1
保持独立，拥有T的公共SABI另需完整参数/结果mode、layout和版本描述。

普通标准声明内部稳定名义标签选择：`@std.task:v1:TaskCreateError`、
`@std.task:v1:TaskPollState`、`@std.task:v1:Poll:<actual-T-fingerprint>`。
它们由分布式普通Enum声明及普通family专化产生；完整Ready(T)保留实际T，
两核比较完整Type，fingerprint不是资格/责任证明。此标签不授予Task权限。

标准模块qualified绑定经普通alias resolver解析：`std.task.Poll`等指向实际
分布式Enum声明，bare预导入alias仅在没有用户同名声明时可见，普通type/value
作用域遮蔽继续有效。没有全局保留Poll/TaskCreateError/TaskPollState，也不按
bare拼写强改。简单逐参数透传family alias的constructor/pattern继续由普通
泛型上下文确定完整实例；用户同名/同形Enum保持独立名义身份。
