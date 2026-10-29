# ZIL 1.4 / Ante-R1：201 回调 Store 检查点

2026-10-28。201 已完成有限 callback Store 的实现、非作者审核和正式完整工作区回归。**2250 通过、0 失败、0 忽略，235 个结果组**；实际受测 clean 提交为 `893ea73eb4d86717476504c39562d32d87e1337e`，命令为 `cargo test --workspace --offline --locked -j2 --no-fail-fast`。后续接受文档不冒充新的执行。

采用的设计是用户指定的 **ZIL-LANG-1.4 / Ante-R1**，不是重新回到旧173/schema20输入。当前设计 SHA256 为 `4c57f670cae6805444e0531485deee9c3a4a9486f533fc97e49f0b5104bccfbf`，具名规范摘要为 `b72c7a0f8d62814324d7660cfc2b4655aae12bd5272b0511075f1aa7d93accb3`。201 使用 schema/Wire 25；早期运行保留当时的规范摘要，没有倒填最终身份。

完整1.4仍为 **NotSatisfied**，B0为 **Doing**，产品门、完整draft/product流水线和原生Windows均为 **NotRun**。编译速度支线没有合入，也没有重标其性能成绩。精确身份和外置证据见[集成收据](../evidence/integration/201/final/RESULT.json)。

## 已经可用的能力

现有单个 `State<F>` 或 `Pair<F>` 参数入口，可以在函数体内建立 Store，保存一个或两个直接、自含、同F的回调字段以及原有有限Copy/直接Managed字段。普通Copy T、模块和名义数据族身份保留。一个ClosureId表示同一种环境类型，不合并两个元素、两个字段或两个实际值的责任。

| 操作 | 本次实际闭合的行为 |
|---|---|
| Store拥有元素 | reserve失败保持原前缀；满容量push完整返回未插入值；pop转交真实末项并缩短前缀 |
| 借入字段调用 | Fn使用共享窗口，FnMut要求独占窗口；从实际元素地址投影字段，在实参求值前建立callee位置 |
| FnOnce | 从借入元素消费仍拒绝；pop取得完整拥有元素后可以消费实际字段 |
| 清理 | 使用真实捕获布局和live flags；前缀逐元素、元素内按字段声明顺序清理Managed和Closure，零次调用也承担清理 |
| 窗口依赖 | 字段和环境重借持续持有父Store结构依赖；callee准备、lending结果和跨helper调用仍检查相交访问 |
| 独立检查 | Types、Analyze和Verify各自核实际类型/资源；Verify独立重建布局与loan，LLVM使用已核结构生成机器清理 |

窗口仍保守按相交Store前缀检查；不同动态索引或字段名不会自动获得apart权限。没有新增隐式Box、RC、锁或运行时责任表。现有指令事实经源码、raw/Wire和机器路径验证可承载本片，因此没有为本片增加TypedIR opcode。

## 实际验证

| 范围 | 结果与解释 |
|---|---|
| 前端作者 | 最终source suite 16/0，包含32次O0/O2 native；其中两条是明确记录下一阶段未开入口的测试，不能计为该入口安全验收 |
| Analyze作者 | raw 9/0、来源派生7/0、相关旧回归35/0；原普通Store精确Analyze预算561前后相同 |
| 非作者 | 最终46项限定测试通过，包含13项独立组及作者/Analyze/历史控制重放；计数与作者有重叠，不能相加为独立覆盖 |
| Linux四模块消费者 | 作者118/0，非作者另行118/0；每组14普通执行和104探针执行，其中84次实际分配失败 |
| Win64/Wine四模块消费者 | 作者118/0，84次实际分配失败；这是兼容层执行，原生Windows仍NotRun |
| root完整工作区 | clean893ea73e：2250/0/0，235组，101.14秒；不是编译速度或Rust配对基准 |

四模块消费者覆盖0/1/多轮、continue/break、Store从容量2扩为4、pop后拥有字段调用和每个实际分配位置的故障。全部故障核对分配/释放并最终`live=0`；Wine额外核对`bad_operations=0`。期望的分配序列、清理顺序和结果由冻结源码独立列出，未从被测LLVM或探针输出生成oracle。

作者最终源码测试实际运行于d55加未提交测试差异；该差异与后来f484测试提交逐字相同，收据保留原身份。root的完整2250/0则来自包含全部最终测试的干净提交。作者、非作者、平台运行与最终回归分别登记。

## 发现并修复的问题

真实零payload Package捕获被误拒：旧Types条件把整个Store元素必须非零的要求施加给了内部捕获。实际上该捕获的shell/Managed live flags仍产生非零真实环境。非作者保存原直接State成功、Store失败的同字节输入，修复后两种形态均O0/O2输出A；Verify另有独立真实flags布局控制。

Store元素字段经过同步helper时，原依赖计划没有携带Store元素的callback形状。新增有限Store形状来自既有已计费类型遍历，修复实际嵌套helper拒绝；它不承载运行时访问权限，也不是完整T类型绑定的替代。

旧200原02从拒绝变为成功，因此仅晋升原同字节输入的期望并实际执行全部三个原探针。旧175和180仍拒绝Managed字段投影，仅迁移诊断文字，均经非作者只读核。正式全仓首轮e573为2249/1，唯一失败是180旧片段匹配；限定复跑1/0后完成最终2250/0。原失败和实际编译报告均保留。

原单State的Expand精确预算1731仍通过；原普通Store的Analyze精确预算561保持。新增原02的Expand/Types预算为3095/4241，独立来源的四阶段边界为2871/5241/16091/29902；各自验证N成功、N−1 ResourceLimit、fresh N恢复，不能把不同输入的数字相互替代。默认限额和产品阈值未提高。

## 下一阶段及剩余终态义务

**下一202实施完整的普通`T: StoreElement`模板绑定。** 两份原始helper输入仍分别在Types停止：`push`的拥有错误载荷含游离F，`empty/reserve`返回的名义族与实际元素不同。原始SHA和失败记录保留；这两项尚未进入Verify，不能称为安全通过。

选定方案是公开Core普通类型参数表和静态TypeApply，保留完整T到Types，在调用者作用域解析后独立核全部参数/结果约束。完整名义T实例键、模板效应和依赖调度都必须实现；不伪造运行时F角色，也不靠私自内联消除缺口。具体提案见[202 Core合同](../changes/202-store-template-core-proposal.md)和[Expand计划](../changes/202-expand-plan.md)。

不同F/G持久State、显式嵌套`Store<State<F>>`的callable入口、直接Store/含Store Package捕获、任意复合carrier调用、一般trait/关联项/life/HRTB/lending/dyn、卫生Syntax/Type CT、外部保留回调、挂起/取消/并发仍保留真实义务。此检查点不等于ZUI全部前置完成，完整目标没有缩小。

## Rust四目标与交付

有限Store回调组合已有真实安全近邻、责任守恒和故障清理证据；全语言安全仍未完成。普通静态调用和内联环境没有引入默认运行时机制，但行为一致与分配平衡不能代替同语义Rust的时间、峰值内存和尾延迟配对。可复用状态/回调API有所推进，框架整体维护成本尚未测量。编译成本继续由独立支线处理；本片没有相对Rust提速或产品级净突破结论。

交付为完整main标准Git bundle和旁置摘要，可作为用户现有一键脚本的本地fetch来源。新执行产物、target缓存和大证据不进入Git；证据单独打包。原历史中已跟踪的证据随祖先保留，以保持提交身份。离线导入和显式合并命令见[OFFLINE-BUNDLE](OFFLINE-BUNDLE.md)。
