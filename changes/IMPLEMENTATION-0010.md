# 010 整数范围循环

## 原因与范围

当前parser有for/range形状但expand返回Unsupported。实施有限i32/i64半开范围，打通现有语法的真实native执行，不引入不完整借用或假闭包。

实现现有for半开整数范围语法的真实scalar执行链，仅for IDENT in lo..hi，支持i32/i64端点；其它iterable/pattern保持Unsupported，不能吞掉未支持语义。由集成者批准本片CoreStatementKind::ForRange { name, start, end, body }公共raw节点，所有生产消费者同步；不增加可伪造Verified能力，也不改变typed wire或产品阈值。规范不新增语言符号。

端点在外作用域左到右恰好求值一次，两个已定型不同宽度整数拒绝；无类型字面量可从另一端明确整数类型取上下文。私有counter与只读迭代绑定分离，循环变量不泄漏。控制图preheader→compare→body→step→compare；continue到step，break到after；嵌套指向最近循环；全break/return不制造无用不可达SSA依赖。半开比较保证最大整数端点不执行MAX+1，仍用已核checked add。空和逆向范围执行零次但端点仍求值。CT只遍历语法，不能把运行期迭代变量变成闭合CT输入。

## 验证

真实source→CFG→独立verify→LLVM/native：正反空范围、i32/i64/MAX边界、break/continue/嵌套、遮蔽/离域/只读反例、已定型混宽拒绝、动态CT拒绝；端点调用次数/顺序有精确阶段证据，无法以当前纯标量程序观测的副作用不得虚称native已证。新增稳定钉表，numberlab用新for遍历并保持独立筛法期望。覆盖原parser/project/receiver/CT受影响回归，施工不跑统一全量。raw在out/runs，evidence只小收据/反例。
