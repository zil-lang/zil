# 324：有限 Copy 大聚合的局部内存快照

## 冻结合同

保留 recovery-large-array.json 的原失败及原 a18 源/hash。本片修复该262144字节
本地聚合源，两后端O0/O2须真实成功执行；不缩输入、不改优化级别、不切换后端。

共享LLVM Emitter对有限Copy Array/Tuple/Record、物理大小至少4096字节的SSA值
使用独立固定entry栈槽。Load/ReadAddress在原指令位置取得完整副本，构造、字段及
动态索引投影也产生独立快照；Store/Update不改变旧值。快照不借用可变local，
其地址不能成为源Address或逃逸。求值及范围检查沿已有LIR顺序。

使用现有machine Call/GEP/Load/Store与LLVM memcpy/memset intrinsic，不增加公共
IR/opcode/schema、隐式堆分配或noalias事实。大小按实际机器类型计算，包括Unit
占位、对齐及尾补齐。计划在当前Verified limits内模块级预付，供并行函数只读使用。

现有LIR的分支/循环合流走local Store/Load，不引入指针phi或共享快照。每个静态
ValueId独立；每次动态执行在其真实定义点写入。未覆盖消费者显式实体化旧SSA值，
函数签名及调用ABI不改。一般大聚合ABI边界仍可能触发LLVM问题，不算本片已修复，
也不新增拒绝。原a18及旧失败保持，不用较小输入代替。

写集为共享emit.rs、新emit/copy_aggregates.rs及已有aggregates测试。限测原a18四
组合、Copy快照/别名/动态投影/分支循环/求值顺序/近阈值ABI及既有反例。大源编译
CPU45s/VM2GiB/wall55s/core0；独立target324、jobs1。实际结果待执行。
