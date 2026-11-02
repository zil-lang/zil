# 238 普通Product整值赋值事务

236原正例current=relay(current)使TypedIR先TakeLocal再Store当前局部；资源核把最新Store视为最新构造，前端退出仍按原词法次序。删defer的最小输入同样失败。原失败保存在227/kernel/236-relay-investigation及236/joint01，不放宽两核newer规则、不改236原oracle。

按§8.1.3.1，保持实际同形Product shell，经既有字段指令实现独立RHS替换和唯一直接self实参普通调用。仅在完整赋值事务内拆组，普通whole move仍让原局部失效。避免单字段wrapper引入未支持的deep Place或扩大含Store Product共享借入。old/new字段、实际参数顺序、失败、条件清理和临时均按现有实际债务处理；新增工作预费。

最近反例含self参数后Copy读取、重复target参数、闭包/Sequence隐藏自由路径、活Desc/Store窗口、move后使用、错字段来源、部分父/循环/提前退出、RHS失败和真实分配失败。不限具体函数名/模块；复杂nested self表达式仍是未完成功能，不能声称一般赋值全闭合。原消费者成功及两优化档机器行为是本片必要验收。

独审不把concrete-only限制写成完成：现有合法普通泛型静态调用和显式TypeApply同样复用普通完整实例检查；237补齐Root<T>输入的唯一T推导。新owned_supported资格walk须先付费，原失败/预算收据保留。
