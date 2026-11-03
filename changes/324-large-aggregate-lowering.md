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

## 迭代结果

首生产9d068de62通过后端cargo check；原a18的Clang/API O0分别0.516s/0.385s，
原生退出0，但直接intrinsic的两O2在2GiB下仍SROA OOM，保留为未完成候选。
root发布公共Attr::NoInline（c0e8068c0，本树独立接入71c9169a3）。本片仅在私有
copy/zero helper使用该属性，helper保留真正LLVM memcpy/memset及动态字节数，
不使用optnone，不修改请求优化级别。边界使O2不在调用者把大内存复制展成海量SSA。

随后默认aggregates目标通过，但最初错误地把测试代码的四组循环当作四组实证；
恢复后逐组日志确认driver默认未启用llvm-api，实际只执行Clang O0/O2。撤回此前
API已通过的断言；显式开启llvm-api后才能形成四组成绩，不把cfg跳过记为Pass。

## 最终限定结果

恢复后重新执行（独立target324、jobs1，Rust1.99.1/LLVM23.1.4，Linux x64）：

- `cargo test -p zil-driver --test aggregates --offline -- --nocapture --test-threads=1`
  24/0，59.48s；其中a18实际只执行Clang O0/O2。
- 同命令显式增加 `--features llvm-api`：24/0，66.46s。逐组日志实际确认原a18
  Clang O0、Clang O2、API O0、API O2均原生退出0、backend匹配、fallback=null；
  Clang argv核-O0/-O2，API报告核default<O0>/default<O2>，不是只读请求选项。
  每组CPU45s/VM2GiB/wall55s/core0，原源SHA256仍为
  `d1ee96ab93bf289321231369c79f9c54a329d2690c394785633e2bb333899ea3`。
- 五个新增test覆盖原a18、可变local旧值、ReadAddress快照、嵌套动态索引及越界trap、
  分支循环合流、RHS副作用和4096B按值参数桥接。Record用真实Unit空struct、bool
  与i64尾字段核7字节padding；布局从llvm_type结果计算，不假定Unit占1字节。
- 原a18当前Codegen预算N=323040：N及fresh成功，N−1=323039在Codegen返回
  ResourceLimit。新计划有模块累计预付；小aggregate机器路径虽不变，计划预检会
  新增Codegen费用，不声称全部旧输入费用不变。既有tuple Types51/50控制保持。
- `cargo test -p zil-backend-llvm --lib key_binds_all_machine_changes --offline`
  1/0。machine_key哈希namespace、triple和完整module.items；helper及NoInline
  均进入键，诊断source fingerprint不作为对象许可。新增NoInline键变化反控通过。
- 后端lib Clippy（默认及llvm-api，`-- -D warnings`）、格式及diff检查通过。

恢复后的日志在/tmp/324-aggregates-final.log、324-aggregates-api.log、
324-cache-key.log、324-clippy.log、324-clippy-api.log；旧离线前日志已丢失，
不把聊天记录冒充保存的原件。测试辅助字段/argv类型初次书写导致过编译失败，
修为正确CancellationToken及argv逐元素检查，不改生产规则。

`large-array-a18-by-value.zil`只保存ABI边界后续输入，本轮未执行；不把它标为
已复现的新崩溃或通过。一般大按值参数/返回、闭包及enum/error特殊消费者仍会
实体化旧SSA桥接，未由本片修复；没有新增拒绝入口。未跑正式全量、Windows或
Rust性能比较。

Rust四目标：Copy快照/别名/范围沿原独立核且真实源正反控通过，完整安全未完成；
原a18四组合及无隐式heap有证，额外栈槽/helper成本存在，未测同语义Rust运行/RSS；
合法大数组从后端崩溃进入正常机器执行，框架维护收益未测；新增编译工作有共享
预算边界，无编译速度优势结论。该修复是实际突破，不升级B0、完整语言/ZUI或产品门。
