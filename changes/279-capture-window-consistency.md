# 279：普通捕获窗口的 Analyze 一致性

本片修复既有借用规则在 Analyze 的遗漏，不开放新生命周期、调用模式或
资源表示。普通具体闭包的共享或独占 Address 捕获在其最后实际使用之前
仍要求原存储保持有效；caller 的相交写入或存储更换应报告
`Analyze/E-BORROW-CONFLICT`。Verify 已独立拒绝这些输入，最终安全边界不变。

窗口按真实来源和 CFG 最后使用核验。捕获类型相同不等于实例同根；写入
不同来源、写入发生在捕获建立之前或最后使用之后均保持合法。捕获的
child 闭包继续传递实际 Address 依赖，不以闭包的词法整个存活期代替借期。

同一窗口检查覆盖直接写与存储更换、经地址写入、独占借入/重借与投影，
以及普通/外部/间接/闭包调用的实际可变地址实参。调用本身使用的捕获
载体也参与检查，不只看调用后的存活集合。指向闭包环境的实际地址保留
其捕获的依赖，但这些依赖不是该地址的额外 pointee；内部来源区分两者，
公共 Hold 仍按原格式投影。经真实同一地址的父权限委托继续允许，不能用
相同 ClosureId 或相同根替代真实重借链。

实现复用 Analyze 私有 Hold 来源与独立 liveness；不调用 Verify，不消费
候选公开 Hold 行授予权限。public schema、Wire、ABI、清理顺序及默认预算
不变。没有 Address 捕获的模块保持旧逻辑费用；具有 Address 捕获的模块
新增实际捕获依赖、闭包载体 liveness、caller 写入与真实根比较工作，所有
集合、遍历和历史行复制均使用本次请求余额并在增长前预付。此项必要补核
允许这些既有借入闭包输入的 Analyze 精确 N 增加，交付时记录原/新 N 及
N−1/fresh，不以免计费维持旧数值。

验收包含三份原始 caller-write 源、独立 raw/Wire、最近合法前后窗口与
不相交对照、无 Address 捕获旧费用对照，以及新增路径的预算恢复。

集成时逐原源核对旧借用测试的诊断迁移：borrowed_closures的四个caller写、
shared_addresses的两个捕获副本及一个默认捕获写、impl182的一个mutable捕获写，
保留源字节和E-BORROW-CONFLICT，仅将实际阶段Verify改为Analyze。纯Address、
词法存储退出和其余Desc冲突仍按各自原阶段。原StorageDead/重激活raw变异现在先
断言Analyze拒绝；另保留原合法边界计划并更新实际module指纹，继续独立要求
Verify raw及Wire同样拒绝，不再依赖Analyze放过非法IR。

整合279与282时，scoped-local-borrow的实际Analyze成本由282单片2352增加到2798，
来自真实捕获窗口补核；两片的单独测量不覆写为组合测量。整合预算仍逐项核N/N−1/fresh。

后续整合另核实五个既有源码负例：construction11的跨模块捕获、
cross_function_borrows的闭包复制后写，以及impl077的三个Package捕获冲突。
保持原源码、SafeReject与E-BORROW-CONFLICT，仅精确迁移到Analyze；
纯Address负例仍留在原阶段。三个完整限定目标为2+11+12项通过。

## 原始源码及最近控制

三例均由真实 source→Types 产生公共候选，再分别直接检查 TypedIR 及
Wire 解码输入。修复前 Analyze 放过、Verify 均以 `E-BORROW-CONFLICT`
拒绝；修复后两阶段均拒绝。

```zil
fn main()->i32{var count=0;var f=()->i32=>{count+=1;return count};let r=f();count=10;return f()-r}
```

```zil
fn main()->i32{var count=0;let f=()->i32=>count;let r=f();count=10;return f()-r}
```

```zil
fn main()->i32{var count=0;let alias=&count;let f=()->i32=>*alias;let r=f();count=10;return f()-r}
```

`count=10` 移至第二次调用之后，或改写无关 `other`，均保持合法。
独立对照另含首次调用前写、同根第二个闭包尚未结束、经 `*p=10` 的
间接写，以及捕获建立之前写、间接写在最后使用后或位于不同根。

## 实测费用

同一公共 TypedIR 在修复前 `9c6a64ee` 与本片分别核查，不复用跨请求
授权。下表是完整 Analyze 的最小 `max_analysis_steps`，默认值未改。

| 输入 | 原 N | 新 N | 归因 |
| --- | ---: | ---: | --- |
| 无捕获 `(n:i32)=>n+1`，调用并返回 | 547 | 547 | 无 Address 捕获，不进入新工作 |
| 无捕获、分支返回拥有型 Reply 的具体闭包 | 11540 | 11540 | 无 Address 捕获，不进入新工作 |
| 可变标量捕获，两次调用后写原标量 | 1339 | 1997 | 捕获依赖清单、实际闭包载体 liveness、写/独占借入与调用实参根检查 |
| 共享标量捕获，两次调用后写原标量 | 1206 | 1859 | 同上，按实际值/局部与操作数量计费 |

带 Address 捕获路径的增加为必要补核；原实现缺失的检查不享受免计费。
每个 N 都检查 N−1 为 ResourceLimit、N 及再次独立 N 返回同一候选。
非法直接/间接写的最早拒绝工作也独立检查 N/N−1/fresh，低额度不得
复用此前的 SafeReject 或成功结论。

## 限定验证与保留边界

本片 `cargo test -p zil-analyze --lib` 为 105/0；
`cargo clippy -p zil-analyze --all-targets -- -D warnings` 通过。独立 raw
测试覆盖共享/独占、显式地址局部、直接/间接/helper 写入、捕获前/首次
调用前/最后调用后与不同根；单独核查通过环境自身的独占重借及多次 helper
调用保持合法。三原源重新经当前 Syntax/Expand/Types 产生相同 TypedIR，
Analyze raw/Wire 精确拒绝，独立 Verify 的原始负例入口继续保留。

原有非法 source 的诊断阶段可能从 Verify 提前到 Analyze，仍为
`E-BORROW-CONFLICT`；仅按实际受影响输入迁移阶段断言，不放宽判据或删除
独立 Verify 负例。整仓、两后端及正式产品门由集成方运行，本片未宣告。

Rust 四目标：独立 Analyze 补上已确认的窗口遗漏，最终 Verify 安全边界
原已正确；没有运行 ABI、隐式堆分配、RC 或锁变化，未做 Rust 运行/RSS
配对；保留普通最后使用与不相交组合；新增编译工作明确预付并记录精确
费用，未宣称编译提速。突破限于修复一个可复核的独立核缺口，不晋升完整
语言、ZUI 或四目标产品门。
