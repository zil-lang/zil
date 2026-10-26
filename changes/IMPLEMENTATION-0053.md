# 0053：拥有资源的有限闭包环境与机器清理

状态：施工；规范来源 `_zil_design.md` §7、§9。公共 schema / wire 由集成方迁至 7；旧候选明确拒绝，不改变语言版本及问号候选。

## 合同先行

闭包环境仍由 `ClosureDefinition.captures` 列出实际字段。Managed 字段承担唯一清理责任；Desc 字段仅携带描述且调用时验证目标。拥有字段并不自动要求 FnOnce：只读为 Fn，修改拥有字段为 FnMut，移出字段为 FnOnce。资源闭包本片的显式参数及返回保持有限 Copy；资源可转交给普通 give 参数函数。泛型高阶、逃逸闭包、嵌套拥有聚合与错误展开不由本片声称完成。

新增原始操作：

- `DescribeCapture { index }`：取得活动环境 Managed 字段当前描述，不转移责任。
- `TakeCapture { index }`：只允许 Consume/FnOnce 入口，从活动环境移出仍 Live 的 Managed 字段并清字段责任标志。
- `CallClosureShared { callee: LocalId, arguments }`：共享调用实际初始化的 Fn 环境，不通过 Load 复制拥有环境。
- `CallClosureOnce { callee: ValueId, arguments }`：消耗一个 FnOnce 环境 SSA 值；正常调用返回后清理仍 Live 的 Managed 字段，之后调用方恢复执行。调用中移出的字段由其接收方持责。
- `DropClosure { local }`：有条件清理拥有环境局部；未调用的环境同样清理；已转交的局部不重复清理。

LLVM 环境为 inline 字段及 Managed 字段完成标志，不引入隐式堆分配/RC/锁。拥有环境的共享入口及 FnMut/FnOnce 入口通过实际环境地址调用。FnOnce 的调用点使用固定入口块 scratch，避免循环反复 alloca。局部仍按逆成功构造顺序清理；环境字段按捕获首出现的固定顺序清理，清标志先于用户 Drop。正常返回合同不等于 panic/unwind 已完成。

Analyze 和 Verify 分别重建来源、存活和责任；原始候选/计划不制造权限。ReadCapture 不得读取 Managed 字段形成复制，CallClosure 不得复制拥有环境。跨分支重复 consume、悬空描述、重复责任与遗失责任必须拒绝。

## 验证计划

新增 wire 完整往返、截断、旧版本拒绝及指纹覆盖；源码资源闭包的未调用/重复只读/描述重读/拥有可写/消耗后剩余字段清理，比较独立 Rust 程序在 O0/O2 的输出和退出码。独立负例由 052 提供；最终完整门只由主树执行。
