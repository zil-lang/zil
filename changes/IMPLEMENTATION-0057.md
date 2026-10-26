# 057 同步错误源码、显式CFG与Analyze

施工源基线为 draft10 `7e4c4728c09fd07a55680844514e0f38bb29e708`，公共表示依赖056 `25d2a574cecda2fd8af32527f3a73f1d0c9c14e9`。本片消费056冻结的schema8，在syntax已有的 `E!T` 与后缀 `?` 上接通真实源路径，不新增构造拼写，不使用用户自定义Outcome普通返回代替错误传播。

## 类型与源面

Expand保留ErrorUnion与Try，名义有限Copy载荷逐层解析，泛型类型身份、替换、调用推断与`?`的结果形状保持联合结构。明确期望E!T时，完全相同联合直接传递；其他值只按唯一精确匹配载荷注入。两边同型且都匹配时拒绝歧义，已绑定值不做隐式转换。普通无上下文整数字面量仍沿用i32默认；需要不同类型时使用已有类型注解。

`?`先求值一个实际错误联合，检查所在函数同一错误类型，然后产生ErrorTag分支。失败边读取错误载荷并构造所在返回类型的错误分支；成功边读取成功载荷。两套闭包推断和源码效果遍历都识别此结构。仍要求错误联合双方为有限Copy；未知推断变量不能靠猜测选择错误或成功，需要唯一类型约束或注解。

最近拒例保留已有 `E-TRY-NOT-ERROR`，新增精确拒例为 `E-TRY-RETURN-TYPE`、`E-TRY-ERROR-TYPE`、`E-ERROR-INJECTION`、`E-ERROR-INJECTION-AMBIGUOUS`、`E-ERROR-INFERENCE`。这些诊断由集成方登记到规范诊断表。

## 清理与求值承诺

每个实际源函数、块、模式绑定和展开Sequence都有明确scope激活。参数隐式属于根，普通local在其定义位置StorageLive。所有退出沿逆scope、逆初始化LocalId输出真实Drop和StorageDead后ScopeExit；Return/Error先关闭已离开的unsafe区域，根ScopeExit后不再产生运行指令。

编译器临时的StorageLive放在所属ScopeEnter之后，以便条件分支和`?`在合流时有一致存储集合。没有Store的槽位不拥有资源。完整表达式仍及时执行真实Drop，清空资源责任；StorageDead在词法scope退出。后续发现的临时也会补齐已生成早退边上的空槽StorageDead，并按输出指令预算计费。成功路径的待清理列表不会被`?`失败边消耗。

调用参数从左到右求值。先求出的拥有参数和Consume callee暂存到有scope的临时；全部参数成功后才TakeLocal提交给Call。这样后一个参数失败时，尚未进入callee的资源和拥有环境可以真实清理。此暂存不增加Box、RC或堆分配。资源字段仍仅为Copy，全部字段成功后才MakeManaged；本片不声称拥有字段部分构造已经实现。

Analyze独立重建scope栈及local存储归属，拒绝活跃scope重入、无存储使用、未关闭存储退出和不一致合流。它不读取Verify代码，也不输出可信scope证明。原有独立资源分析继续重建真实初始化顺序和责任转移。Verify由058独立实现，057仅调用公开Verify作源路径联测。

## 限定验证与未完成边界

`evidence/impl/057/run-source-cases.sh` 从33组真实ZIL源码进入Parse、Expand、Types、Analyze、Verify，包含现有PP13/PP14、有限聚合/泛型联合、不同成功类型同错误传播、Unit与嵌套联合、创建前后失败、未调用拥有闭包、callee错误、给出参数后失败、Consume callee后失败、环境内`?`、临时、模式、循环及unsafe返回出口。另保留源生成IR删掉错误边StorageDead的拒绝测试。

三个独占crate的49项现有限定测试通过；Analyze一项精确工作预算随新增逐指令scope扫描更新。clippy `-D warnings`、限定fmt与diff检查通过。这里的33例证明到独立Verify；多模块实际机器行为和Rust O0/O2配对由056整合，不把前端成功写成机器成功。正式门禁、精确提交独立review和主树任务结算由集成方执行。
