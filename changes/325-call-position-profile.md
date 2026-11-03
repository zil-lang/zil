# 325 调用位置与保守资格扫描澄清

有限结构化线程调用仅在普通 Direct 函数内实现。父闭包入口或带 callable 参数/helper入口合同的函数内调用暂报 Unsupported；这些位置仍是终态待实现义务，不把父调用位置混为 worker 的安全反例。worker 静态调用图逐函数扫描全部 CFG 块，不以源常量分支隐藏不支持的效果。

最近对照：普通函数中 body 合法捕获父线程借用仍可执行；闭包内部调用返回 Unsupported；普通 worker 调用图内存在 FFI 即使位于常量未走分支也不获得线程资格。Types、Analyze、Verify 分别实现入口与资格检查，公共 schema/Wire34 不变。

独立审核325时，原措辞“无callable合同”对返回`impl Fn`的普通Direct factory产生
歧义。实际源在factory内部完成`thread_scope`与join后才返回捕获标量结果的闭包；
该函数没有入口环境和callable参数/helper入口合同，仅有`callable_return_contract`。
root裁决该返回合同不改变当前Direct执行资格，§11.1将明确其与入口合同的区别，
返回载荷仍按既有闭包/借用规则检查。这是措辞澄清，不是UB发现，也不修改生产准入。

既有`generic_regressions`目标保留审核原源，并将factory作为Clang/LLVM API各O0/O2
普通机器正控；最近反例固定为同一有资源捕获的Once body在两次调用间重用，应在
Analyze以`E-UNINITIALIZED`拒绝。另保留worker常量未走分支内地址操作的拒绝控制，
防止把源级不可达当作纯标量资格证明。实测与精确共同提交绑定在独立审核JSON中。
