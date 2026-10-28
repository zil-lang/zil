# 197 实现记录：Copy数据与唯一拥有callback State

生产最后修改为`da3a663abd3730418f8ad1dd0941a9b312ec933e`，最终限定受测源码`27d8e92269b1618006503cc6ae7139a6c9c50aa2`。生产只改Expand的generic_callable、generics、generic_check及新增私有generic_state；Types、Analyze、Verify、LLVM、schema24和默认额度均未改。

State语法分类接收一或两个实参中的唯一直接F。当前函数及结果签名的真实词法binder明确传入家族构造；原始aggregate和完整规范化非F实参决定数据族，生成名字保留该族，Types既有nominal_base只去掉最后F展开序号。phantom T、同布局A/B、i32/i64不会合并；同一T的透明别名、不同词法F/G、嵌套Copy与跨模块名义类型保持正确身份。实际Package字段仍由Types检查，生成名称不授予所有权或可调用能力。每份新增族键、扫描与复制在原预算中先收费。

普通T可以从State外壳推导，也可由明确结果上下文约束。先部分特化普通T，真正F仍由实际ClosureId实例化。状态字段直接调用经真实Package place和F bound作符号检查，Copy仅许可复制和普通传递，不授予字段/数值/callable能力。整体State仍拥有而不可Copy；多callback字段、一般aggregate bound、多个拥有环境和dyn保留关闭。

## 实际结果

最终6个限定targets合计82/0，其中197新源码14项+raw/Wire6项，旧176/192/194/195邻接62项。新增197源码实际O0/O2共18次；其数量以native.json枚举，早期进度消息推算的14次不是最终统计。四模块stdin消费者在87f09211源码/CLI上真实Linux执行86次，14正常、72独立分配探针、52次实际分配失败，两个ByteBuffer的alloc/free/live/peak、输出及Drop顺序全部按独立oracle核对。消费者CLI构建与最终受测源码的生产字节相同；作者新原生合计104次。Wine/原生Windows不在作者这份执行范围。

raw攻击使用同一factory产生的实际ClosureId和不同真实State实例：宽度不同的T、相同布局名义T、完全相同运行字段的phantom T、错factory返回。重定向真实Call而非只改显示名，raw与Wire均被独立Verify E-VERIFY-CALL拒绝；Analyze是否接受不受信候选单列记录，未冒充类型证明。Expand nodes/work/depth/functions和Types/Analyze/Verify work均测N/N−1及新请求；未改额度。限定Driver lib/197两个tests及Expand lib的clippy通过。

## 原始失败与范围校准

- 第一版原12预检9成功、3Unsupported；其中04/05/09暴露直接callback字段被当作trait method。符号检查补齐后保留原输入。
- 原09的`pub struct Data{n:i32}`并未公开字段，准入后正确E-FIELD-PRIVATE。原字节仍为负例；只在独立正控制中明确`pub n`，O0/O2通过。
- 原12仅定义但不调用run；其Success不能证明借用安全拒绝。新增真正make/run的 held字段冲突到Verify拒绝，最后使用后复借正例通过。
- 首次新FnMut负例只修改显式参数，实际闭包为Fn，正确被既有opaque kind检查拦截；自有夹具补真实可变捕获后才触达目标冲突。首轮输出保留。
- 第一次raw测试按显示名查找helper导致5项定位断言失败；改为实际Typed参数/返回形状定位后6/0，攻击输入未弱化。
- 最后限定组合首轮81/82；直接current.callback(...)循环会take字段。保留真实拒绝，并用既有显式字段借址验证循环正例。此剩余可用性项单列changes/199-proposal.md，未伪称已完成。
- 宽选全部Driver tests的clippy触发原126单元素for警告。原文件不在本片写集且未改；失败保留，最终lint只选实际修改模块与新测试。

## Rust四目标

本片新增可复用拥有State的类型身份、真实当前字段权限与清理证据；未证明完整语言安全。没有隐藏GC、RC、锁或新运行时表。实际分配释放与失败清理已核，但未作同语义Rust运行/RSS/编译时间比较。普通Copy数据和拥有callback的组合表达缺口缩小；产品维护优势、Rust净突破及完整1.4均未成立。产品门NotRun、父任务B0 Doing不变。
