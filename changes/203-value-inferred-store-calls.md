# 203 增量：从真实值唯一推导普通 StoreElement 的完整 T

状态：集成方在真实五模块消费者失败后冻结的同片合同补充，尚未验收。原失败来自 `bc00db91` 的 main：两个实际factory构造State，随后 `identity(first)`/`seed(first)`。main没有F/G声明，Expand却把producer结果残余F/G当作完整具体普通实参而提前特化。仅强行写TypeApply<State<Tag,F,G>>也不合法，因为这些名字不是main的binder。失败输入及早期1355规范记录保留。

## 选定行为

保持公开Core既有Call和TypeApply，不增加假类型占位或新opcode。在203中，正常静态解析到一个retained普通T:StoreElement模板的裸Call，可以从真实值参数按声明参数模式唯一推得完整T。源显式TypeApply仍是精确完整约束，走原检查；不得因为存在值参数而丢弃它。裸模板函数仍不能成为值、函数指针或动态callee。CT执行该模板仍明确禁止，wrapper不能遮蔽。

从T、Store<T>及既有错误联合包装中的对应位置提取完整实际类型，全部参数位置中的约束必须完全一致。没有唯一值约束的empty等调用继续拒绝，不能从某个已见实例、最终机器布局、一个ClosureId或全局同名F/G猜T。调用者词法binder规则不变，不给main补一个伪F/G权限。每个实际T的canonical OwnedStore资格定义及Analyze/Verify检查保持。

类型规划是有界静态工作，不重复执行参数；实际值仍按原callee/实参次序求值和转交一次。必要时按既有参数暂存机制保存准备好的值，后续实例调用消费同一值。新计划形状需保留完整Type、名义族、普通/phantom参数和两个有序callback环境，不能用现有仅存(field,ClosureId)的简化形状替代。真正未完成的实例、递归和增长循环仍不能借占位逃过拒绝。

当前函数的声明模式来源也随实际T及原实参形状传递；ordinary helper只有opaque T能力，不能借此观察字段或提升G:FnOnce为Fn。新推导、快照、比较、替换及实例队列先计费；无新裸模板调用的旧路径不新增遍历/收费。

## 各层工作与验证

Expand保留实际普通T定义及静态裸Call，保留理由来自真实producer/表达式形状事实，不能从全局字符串猜。Types在lib/store_templates/helper_plan/inference/state_origins等已登记路径实现值推导和一次暂存；描述符效果仍指向真实模板键。CT按模板头继续拒绝执行；已有公共消费者必须认识新的合法裸Call分类。

这明确扩展202“裸Call不命中普通模板”的有限规则。旧raw拒绝夹具应逐项判断：完全、唯一、合法值约束的原输入可以具名晋升；不完整、冲突、错误target、未用非法定义、作用域或权限反例继续拒绝。必须先保存原断言实际失败再迁移，不能批量降低期望。

正控制必须包括main构造两份同family完整State、普通identity/seed/push/pop及错误两侧、不同名义/phantom类型和两个实际closure。最近反例包括互相冲突的参数T、无约束empty、显式TypeApply与真实值不符、非静态或错误类别目标、跨定义binder、重复值转交以及未用模板能力越权。真实多元素故障清理、非作者独立核和新N边界闭合后才计接受。
