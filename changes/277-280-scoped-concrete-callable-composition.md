# 277–280 结构化回调内的具体闭包组合

基线9c6a64ee；前片273–275冻结验收，本片隔离并行，合入依赖前片。

两个真实原源阻碍普通UI处理：泛型scoped wrapper中建立具体Event→Reply handler在Expand被拒；普通非泛型工厂返回拥有body后传给With在Verify被误作必须是wrapper形参角色。相同代码的非泛型最近控制通过。依据设计§9.3.1、§9.4及§12.4.1补组合；新profile不开放新的签名、捕获类型、opaque拥有签名、F逃逸、异步或跨线程。

具体本地闭包按其真实参数/结果/捕获逐项检查；不把With存在性、具体实例的更强kind或同ClosureId视为符号F的新权限。允许封闭Event→Reply handler在普通同步callable helper中定义、合法转交及调用，拥有参数/返回/失败清理沿原核。无With对照必须适用同一原则。符号F被捕获、声明FnOnce复用、借入延寿及其它原拒绝仍独立成立。

With的本地body也可来自已有合法具体工厂调用及局部转交。只消费已核公共返回合同下的本次实际值，不能用同型值、私有callee正文或任意无来源Call替代实际事实。角色Flow只描述wrapper真实callable形参；局部具体body沿实际资源/借用/清理核验，不捏造形参角色。错误返回合同、同型不同值、已移走body、重复非Copy消费、实际F别名或捕获仍拒绝。独立raw与Wire先保留准确结构/Flow正前缀，再变异最近反例。

复用现有IR/schema/Wire32；新增遍历及分配预付共享预算，不提高默认额度。记录新路径N/N−1/fresh及旧精确费用控制。机器验收复用既有功能target和按功能命名输入：源码→Verify→生产Lower→LLVM API及参考后端O0/O2，登记成功/null、准备分配失败、剩余环境清理和F返回后仍可用；独立oracle，非法源不native，不永久保留原始产物。

277负责Expand及必要Types通用组合，278负责独立Verify工厂body来源，280负责消费者与独立预期；相交文件先交root协调。常规完整门由root，作者不合入、不跑全量。
