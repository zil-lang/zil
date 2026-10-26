# 048 独立有限资源责任核验

## 为什么

Managed/Desc/give 进入真实源链后，普通初始化集合不能证明责任只转交一次，也不能防止内联资源头移动、清理或重新激活后旧描述被重新解释。源码生成器和候选证书都不授予这些权限。

## 实现

使用 049 统一的 schema 6，独立检查 `ResourceDefinition`、完整 Copy payload 和隐藏 teardown 的精确 Desc 参数及 unit 返回。普通 Call、FunctionRef 不能进入 teardown；仅 DropManaged 的已核清理激活可以调用。顶层 Managed/Desc 才获准进入有限资源路径，嵌套资源、资源地址、描述返回与资源闭包仍拒绝。

`resources.rs` 从已检查的 CFG 建立压缩资源槽表，无资源函数不分配本分析状态。Managed 局部和 SSA 值各自记录 Empty/Live/Moved/Done 可能状态。Store、Take、Call、Return 按实际操作转交，正常出口和 StorageDead 不得残留 Live；静态 SSA 定义点在循环中重用前也必须结清上一轮责任。入口的回边照常合流，不能把参数当成每轮重新获得的责任。

描述通过 DescribeLocal、Load 和 Store 独立传播 Local/Parameter 来源。它不具有管理权、不制造 Address loan 或 noalias；顺序使用两个可能同源的 Desc 参数不因为别名而被拒绝。独立的反向活跃分析禁止移动、清理或重新激活后仍将使用的旧头描述，覆盖同次调用 give+alias 和原槽重新构造的复活问题。

构造来源区分真正 fresh 与输入责任。MakeManaged 和经独立正文核验的 Create callee 建立 fresh；Store/Take 保留，Give callee 的返回首片保守视为非 fresh。Create 返回不得洗白输入责任。候选计划中的大来源/责任证书保持未支持，不据其字段授信。

自动清理的先后关系从真实 Store/参数接收顺序重建，不信 LocalId 排序。DropManaged 条件结清 Live，允许移出/未构造的槽保持空；重复已完成清理与逆序违例拒绝。各 worklist 为有限单调状态，分配、复制、合并与遍历均计既有 work budget，不引入固定迭代次数或无界路径枚举。

每次资源类型/构造准入和 Call/FunctionRef 的 teardown 判定也独立扣除资源表查找费用；重复 payload 检查及比对按实际类型形状扣费。定义初次校验的预算不能代付其后所有引用，资源数乘使用次数的工作超限会返回 ResourceLimit。

## 边界

正常作用域、return、break、continue 的实际源 lowering 由 047/049 提供，本模块检查最终 CFG，不能仅凭源关键字推定清理。既有 Trap 是进程 abort，不声称 panic unwind、取消、一般恢复性清理已实现。清理顺序的 may 合流可能保守误拒关联复杂的路径；首片不恢复已丢失的路径关联，也不枚举组合。

完整表达式临时依据设计 §2.2 在自己的边界结清，不能为了等待后来绑定的局部退出而延长生命。当前 IR 尚无独立清理作用域身份；顺序检查对实际 live 局部的 Store/参数接收采取保守逆序，未宣称覆盖所有嵌套清理边界。前端可先求出绑定 RHS 的 Managed SSA，结清该表达式临时，再把 SSA 转入新局部。该 SSA 自 MakeManaged/Call 产生时即携带 Live 责任，后续 Store 只是转交；它从未被视为尚未构造或免于正常出口余额检查。

## 验证

资源 raw-IR 正反例覆盖正常余额、缺清理、SSA 泄漏、重复转交/清理、完整构造、旧描述、Create 输入洗白、精确 teardown、实际构造顺序、条件给出、循环激活、入口回边、同源顺序 Desc 及工作预算。它们只是独立边界证据；真实源到 native 由集成时另行检验，不能把这些夹具算为语言贯通。
