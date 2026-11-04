# ZIL Goal：任务分配与进度看板

更新日期：2026-11-05（Asia/Shanghai）。本文件位于独立 `goal` 分支，是当前目标拆分、优先级、任务分配和工作状态的唯一记录。暂不使用 GitHub Projects。

## 1. 目标、依据与读法

目标仍是完成既定语言、编译器与 ZUI 所需能力，并满足原有产品条件。语言语义和验收条件只读源码分支的 [`_zil_design.md`][design]、具名附件及 [`validation/product-gates.json`][gates]；这里安排工作，不另造语义、通过门槛或完成百分比。

上述规范链接固定到建板时快照；实施须读取任务选定源码分支的当前规范及具名附件，规范变更后同步更新链接和受影响任务。本表是非规范性分工，省略某条款不免除该条款义务。

**“已合入”只表示该行写明的有限范围已经进入实际远端 main。** 一个基础切片完成，不代表该类能力全部完成。“实现待验”表示有真实源码及所列检查，尚未完成独审、共同验收或正式合入。没有源码消费者的公共字段、提案和测试数量都不能代替功能完成。

| 工作状态 | 含义 |
| --- | --- |
| 已合入 | 所列范围已在远端 main，附工程证据；后继缺口另列任务 |
| 实现待验 | 所列范围已实现并有检查，尚未正式接受 |
| 部分实现 | 有实际源码或局部消费者，完整链路未闭合；当前等待接续派工 |
| 待执行 | 具体检查或集成工作尚未执行 |
| 待排期 | 缺口已确认，尚未派定实际源码任务 |

优先级：P0为当前集成、安全及恢复关键路径；P1为后续语言组合；P2为依赖稳定接口的平台和产品工作。优先级不代表依赖顺序或完成日期。负责人只记实际接单者；“待派”不改变已有实现进度。旧施工编号仅用于查找源码。

## 2. 当前源码与恢复入口

| 标识 | 实际源码/远端引用 | 已核范围 |
| --- | --- | --- |
| M37 | [`main@4bc1d77b5ec3bccb5b789fd813e85386f03ae13f`][m37] | PR33已合入；历史共同门默认4372、reference4261及相关检查。只适用于该接受树，完整语言/B0/ZUI未完成。见[E37][e37]及[远端合入收据][remote37] |
| C39 | [`task/language-terminal-20261104@c15b77847502d02acc83474bf7f9a8789186a3ac`][c39] | 可运行39/CT4候选；含后继修复和限定复核，未成为远端main。Option回归修复在该线另复核50项 |
| C40 | [`task/terminal-public40@7dfc6846194643e5295a7704a0b8ce99fa5176e4`][c40] | 作者组合检查：aggregates149、cross84、generic_regressions83、generics33，共349通过。不是全仓或终态验收；见[E40][e40] |
| C41 | [`task/terminal-cas41@7c7950dfc820d1b06a295814c662105842e03587`][c41] | CAS41公共运输候选；源码、两核、机器完整链未闭合 |
| W340 | [`backup/wip-20261104-340@5075da0c1f82ea80313b6f6a546fc4b027664063`][w340] | 拥有替换等实际未提交源码已保存为远端快照；不等于已验收 |
| W343 | [`backup/wip-20261104-343@9764f5119c2ccdf49d6e5c81683c64c11a9cd017`][w343] | CAS等实际未提交源码快照；不是仅保存一个提交编号 |

以上SHA是建板时实查的功能核验/恢复快照，不代表迁移后分支的最新HEAD。后续管理文档提交不改变这些功能证据的绑定；新源码进展只按实际 `git ls-remote`、源码及证据更新。旧301–312缺失的对象/未提交内容没有因本看板建立而被宣布恢复。原大数组O0/默认2MiB栈问题已经修复并进入M37，不再作为待修缺口重复派工。

工作流迁移已由[PR35](https://github.com/zil-lang/zil/pull/35)合入远端[`main@4c775ec09d00818deffe06c4dd30ab4c540af524`](https://github.com/zil-lang/zil/tree/4c775ec09d00818deffe06c4dd30ab4c540af524)，只更新入口、工作流和工具输出，没有合入C39/C40/C41的候选语言实现；语言接受范围仍按M37及各行证据解释。对应独立复核与限定检查见该提交的`docs/validation/review-goal-workflow-20261105.json`。

当前施工源码仍分布于39/40/41。优先在共同40接入已有消费者，保留39可运行检查点；不要把不同分支的通过数累加成一棵树的全量通过。

## 3. 下一批任务安排

1. **P0：收敛已有成果。** G10.1对C40做精确独审和适用共同门；并行接续G1.3拥有替换、G3.3外层life上下文、G4.4完整CT上下文、G8.3源码CAS。先固定各自基线和不相交写集，公共表示由root协调。
2. **P1：闭合跨能力组合。** 按前置推进G3.4 lending、G6.1/2一般状态、G5.2/3 dyn消费者，以及G7.3/G8.4取消排空与外部通知。不能用同步join替代外部入口排空。
3. **P2：库、平台及产品。** G9系列依赖所需语言接口，原生Windows环境准备G10.4可以提前；正式成本测量和G10.6终验使用实际完整消费者。

G10.1只验收交付的候选范围，不是所有后续施工的前置审批门。环境、CI和独审资源不足时记录真实阻塞，未受影响的工作继续。

## 4. 分解任务与状态

负责人“—”表示该范围已交付；“待派”表示等待实际接单。仅保留 P0/P1/P2 优先级，先后关系见依赖。历史实现细节和旧编号可查[精简前记录](https://github.com/zil-lang/zil/blob/4615715d76afee307cbfbe3d818fb45020701362/goal.md)。

### G1 基础类型与模式

规范引用：ZIL-SYNTAX、ZIL-MODES、ZIL-PACKAGE、ZIL-CLEANUP。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G1.1 | 有限Option、记录式枚举、usize/u64原生及CT链、拥有Tuple和混合Tuple/Enum部分移动 | 已合入 | — | — | [已合入证据][e37]；一般模式组合另列。 |
| G1.2 | 恢复Task集成误删的Option拥有载荷路径 | 实现待验 | P0 | root | [C39][c39] / [C40][c40]修复已实现；待 G10.1 集成核验。 |
| G1.3 | 拥有字段替换：Verify责任转交、真实loan/pin、机器old-drop/store及失败清理闭环 | 部分实现 | P0 | 待派 | [替换复核][replacement] / [WIP][w340]；责任转交及机器消费者未闭合。 |
| G1.4 | 更一般的地址/Package/ErrorUnion解构、混合模式及部分补齐组合 | 部分实现 | P1 | 待派 | 已有 `.ok/.err` 路径；按具体模式接 G1.3、G3.4。 |

### G2 泛型与 trait

规范引用：ZIL-TRAIT、ZIL-MODULE、ZIL-RECEIVER。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.1 | 有限const、supertrait、泛型trait/impl、关联类型/常量、默认体及泛型接收者方法 | 已合入 | — | — | [已合入证据][e37]；一般约束方程另列。 |
| G2.2 | 混合life/F helper、独立Invocation关系与未用声明检查，贯通两核及原生 | 实现待验 | P0 | 待派 | [C40][c40] 已有两核及原生检查；待 G10.1。 |
| G2.3 | 一般关联投影、类型/Self依赖的常量表达式、约束蕴含和循环诊断 | 部分实现 | P1 | 待派 | 已有有限方程实现；补齐一般投影、蕴含与循环诊断。 |
| G2.4 | 更一般life/lending、符号泛型方法和静态关联函数；一般/blanket/primitive/enum impl及更广supertrait实参组合 | 部分实现 | P1 | 待派 | 按需依赖 G2.3、G3.3、G3.4；保留定义期义务检查。 |

### G3 借用与拥有组合

规范引用：ZIL-ORIGIN、ZIL-ACCESS、ZIL-CAPTURE、ZIL-PACKAGE。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.1 | 普通/嵌套Product借给可变helper、CopyT+mutable life、有限FP/HRTB和实际来源检查 | 已合入 | — | — | [已合入证据][e37]；一般借用返回见 G3.4。 |
| G3.2 | shared stored View、qualified静态Code及捕获为空的Fn，每个实际role独立核验 | 实现待验 | P0 | 待派 | [C39][c39] / [C40][c40] 已有 role 检查；待 G10.1。 |
| G3.3 | 捕获的外层Fixed上下文、qualified FnMut/FnOnce的真实环境与调用关系 | 部分实现 | P0 | 待派 | 依赖 G2.2；外层捕获上下文及实际环境关系未闭合。 |
| G3.4 | 普通借入字段、存储上界、affine借用、合法lending结果及长期组合 | 部分实现 | P1 | 待派 | 依赖 G3.3、G1.3；需实际 loan/root 证明。 |
| G3.5 | 一般拥有/借入捕获及责任组合的消费者矩阵 | 待排期 | P1 | 待派 | 依赖 G3.4、G6.1；按真实 API 补齐。 |

### G4 Type/Syntax 组合

规范引用：ZIL-COMPTIME、ZIL-MODULE、ZIL-TRAIT。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.1 | 普通泛型Type返回、具体纯泛型记录的CT计算/反射和已有Syntax组合 | 已合入 | — | — | [已合入证据][e37]；完整上下文和值范围另列。 |
| G4.2 | CT4混合const/life/type全序号身份及原源重实体化 | 实现待验 | P0 | 待派 | [C39][c39] 已有限定独审和原生检查；待 G10.1。 |
| G4.3 | CT5真实Definition上下文、上下文键、物理字节计费和phantom Code/field查询 | 实现待验 | P0 | 待派 | [C40][c40] 已有上下文及原生检查；待 G10.1。 |
| G4.4 | 完整实例实际参数向量、成员/生成作用域、重定位与声明归属 | 部分实现 | P0 | 待派 | 已有 Definition 路径；成员、生成等作用域待补。 |
| G4.5 | rich callable、ambient Type导出和更一般可序列化值/匿名Sum/拥有载荷组合 | 部分实现 | P1 | 待派 | 依赖 G4.4、G3.3、G3.4；补齐实际消费者。 |

### G5 动态抽象

规范引用：ZIL-CAPTURE、ZIL-BYTE-STORAGE、ZIL-TRAIT、ZIL-ROUTE。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G5.0 | 具体Fn/FnMut/FnOnce、opaque拥有工厂及有限具体环境保存 | 已合入 | — | — | [已合入证据][e37]；仅动态抽象前置。 |
| G5.1 | dyn真实Source→Core声明/provider身份 | 部分实现 | P1 | 待派 | [C39][c39] / [C40][c40] 已有前端；实际消费者待接。 |
| G5.2 | 异构调用、静态vtable、关联项与有限泛型method slot的真实派发 | 部分实现 | P1 | 待派 | 依赖 G5.1、G2.3、G2.4；两核与机器派发待闭合。 |
| G5.3 | 显式拥有/共同借入存储及管理route，移动/销毁/失败路径完整 | 部分实现 | P1 | 待派 | 依赖 G3.4、G1.3、G5.2。 |

### G6 状态协议

规范引用：ZIL-CONTRACT、ZIL-ORIGIN、ZIL-ROUTE、ZIL-CLEANUP。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G6.0 | 有限资源U/K/D、初始化/部分移动、Store及callable State合同 | 已合入 | — | — | [已合入证据][e37]；一般状态协议另列。 |
| G6.1 | 逐结果的一般状态转换、角色与别名规则 | 部分实现 | P1 | 待派 | [C40][c40] 已有逐 role 检查；一般转换待补。 |
| G6.2 | 真实投影、失效关系、历史撤销和导出上界组合 | 部分实现 | P1 | 待派 | 依赖 G6.1、G3.4。 |
| G6.3 | 状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1 | 待派 | 依赖 G1.3、G6.2、G8.4、G8.6。 |

### G7 外部回调生命周期

规范引用：ZIL-ASYNC、ZIL-CAPTURE、ZIL-CLEANUP；Design §12.4。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G7.1 | 安装函数返回后稳定拥有真实闭包环境的Registration、显式强关闭及失败清理 | 已合入 | — | — | [已合入证据][e37]；限同步关闭，不覆盖跨线程排空。 |
| G7.2 | 更一般借入/跨线程环境的登记对象与关闭合同 | 待排期 | P1 | 待派 | 依赖 G3.4、G8.5。 |
| G7.3 | 外部waker/晚通知、no-future及drain，关闭后所有raw入口静止 | 待排期 | P1 | 待派 | 与 G8.4 联合实施；需证明关闭后 raw 入口静止。 |
| G7.4 | Registration与异步任务责任的失败/取消/完成组合 | 待排期 | P1 | 待派 | 依赖 G7.2、G7.3、G8.4、G8.6。 |

### G8 异步与并发

规范引用：ZIL-CONCURRENCY、ZIL-ASYNC、ZIL-CLEANUP、ZIL-LOWERING。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G8.1 | 结构化pthread、密封MutexI32/AtomicI32、稳定TaskI32帧和有限静态Suspend child | 已合入 | — | — | [已合入证据][e37]；限密封 I32 及有限静态任务。 |
| G8.2 | 静态拥有Task/child、保存SSA、Unit/有限Copy错误结果及清理 | 实现待验 | P0 | 待派 | [Task 复核][task40]；待 G10.1，不覆盖通用 frame port/拥有 Sum。 |
| G8.3 | CAS41真实源码→Types→两核/Wire→机器及争用/错误次序控制 | 部分实现 | P0 | 待派 | [C41][c41] / [WIP][w343]；[机器模型][cas]已有，源码全链未闭合。 |
| G8.4 | 通用PendingExit、挂起/取消排空、外部waker及晚通知 | 待排期 | P1 | 待派 | 与 G7.3 共用责任合同和真实事件源。 |
| G8.5 | 一般Mutex<T>/Guard、Transfer/Share、原子位宽和拥有同步构造 | 待排期 | P1 | 待派 | 依赖 G3.4、G6.1；从密封 I32 扩展到一般拥有类型。 |
| G8.6 | 一般拥有Sum/result、借入frame port、部分责任、ticket/defer的任务组合 | 待排期 | P1 | 待派 | 依赖 G1.3、G6.1、G6.2、G8.2、G8.4。 |

### G9 标准库与平台

规范引用：ZIL-MODULE、ZIL-COMPTIME、ZIL-ASYNC、ZIL-TARGET；产品场景引用原profile。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.1 | 普通字符串/Unicode与事务编辑、Store插入移除、有限回调迭代/Option返回 | 已合入 | — | — | [已合入证据][e37]；完整库见 G9.3。 |
| G9.2 | 整数布局、RGBA软件合成、IME preedit提交/取消及失败保留的普通库 | 已合入 | — | — | [实现证据][ui] / [独审][ui-review]；不覆盖 OS IME、字体及 GPU。 |
| G9.3 | 完整字符串/集合与一般Iterator/lending组合 | 部分实现 | P1 | 待派 | 按具体 API 接 G3.4、G5.3；普通集合不普遍依赖 dyn。 |
| G9.4 | 真实平台窗口、事件循环及OS IME服务 | 待排期 | P2 | 待派 | 依赖 G7.3、G7.4 及平台接口；窗口和 IME 可拆分。 |
| G9.5 | 字体/文本布局、渲染呈现及输入到绘制的真实闭环 | 待排期 | P2 | 待派 | 与 G9.4 协调；需实际字体、呈现和输入链路。 |
| G9.6 | 真实ZUI应用集成、资源失败/关闭及维护场景 | 待排期 | P2 | 待派 | 依赖 G9.3、G9.4、G9.5 及必要语言能力。 |

### G10 规范与产品验证

规范引用：ZIL-TARGET、ZIL-ARCH、ZIL-ACCEPT；唯一量化条件为product-gates。

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.0 | 原大数组O0崩溃、默认2MiB API/深调用栈可靠性及普通大型按值ABI | 已合入 | — | — | [已合入证据][e37]；原 O0 崩溃已修复，更广 ABI 见 G10.3。 |
| G10.1 | C40精确源码独审、受影响组合/既有正式门、修正与合入裁决 | 待执行 | P0 | root | 需非作者独审及适用共同门；[349 项作者检查][e40]不等于全量。 |
| G10.2 | 有限f32/f64与UInt组合、匿名tagged Copy源Sum及CT Type结构Sum | 已合入 | — | — | [已合入证据][e37]；CT Sum 值与拥有载荷见 G4.5、G10.3。 |
| G10.3 | 大型closure/helper/foreign ABI及浮点/匿名联合责任的一般组合 | 部分实现 | P1 | 待派 | 补齐实际源码及最近反例；大数组修复不能覆盖全部 ABI。 |
| G10.4 | 原生Windows执行和平台差异验证 | 待执行 | P2 | 待派 | 需要原生 Windows 主机；环境可提前准备。 |
| G10.5 | 与Rust同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破 | 待执行 | P2 | root | 固定真实消费者、同语义 Rust 对照和独占计时窗口。 |
| G10.6 | B0、完整语言与ZUI终态验收 | 待执行 | P2 | root | 依赖全部必需语言任务、G9.6、G10.4、G10.5。 |


## 5. 既有源码位置与文档归属

新派任务统一使用看板 ID：例如 `task/G8.3` 与 `.task/G8.3.md`；独立子任务使用 `G8.3.1`，分支与说明同名。旧带前导零的格式按去掉前导零识别，不另建映射表。ID 不随排序变化、不复用；子任务完成不自动表示父任务验收通过。跨任务分支以主要任务命名，实施说明列出其余任务。以下仅供查找旧源码，不表示当前派工；已有分支暂不改名。

| 旧施工记录 | 既有源码分支 | 分支内实施说明 | 关联任务 |
| --- | --- | --- | --- |
| 基础/模式340 | `task/340`及W340 | `.task/impl/340.md` | G1.3/4 |
| 泛型341 | `task/341`，近期成果已组合至C40 | `.task/impl/341.md` | G2.3/4、G5.1/2 |
| life/协议342 | `task/342`、`task/342-rich40`及C40 | `.task/impl/342.md` | G3.3/4、G6.1/2 |
| 运行时343 | `task/343`、W343、C41 | `.task/impl/343.md` | G8.3，后续G7/G8责任组合 |
| CT/可靠性344 | `task/344`，近期成果已组合至C40 | `.task/impl/344.md` | G4.4/5、G10.3 |
| 库/平台345 | `task/345`保留旧交付；下一任务从选定共同基线建立 | `.task/impl/345.md` | G9.3起；不得直接把旧分支当最新共同基线 |
| 集成root | C39/C40/C41与新派任务分支 | 各分支对应`.task`说明 | G10.1、规范/公共接口协调及阶段备份 |

这里的`task/…`是GitHub运输引用；本地旧分支可能名为`340`、`341`等，先核真实SHA再操作。不要因名字相似合并旧副本。开始接续时明确选择基线，将已有源码增量和WIP恢复到可复核树，再限定写集；不再新建很多测试目标。

`goal.md`只维护当前任务行；分支`.task`保存目标ID、父合同、基线、规范身份、写集、实现方案、交付和验证要求，不再另记一份“当前排期/进度”。分支交付时把有用的设计决策、源码、精简复核/验收记录带入main；旧工单历史保留。main既有正式工程合同仍约束规范及验收，不能以迁移看板为由删除Done条件。

## 6. 更新、冲突和备份规则

1. **独立工作区。** `goal`是轻量独立历史分支，不合入main；开发分支也不merge进goal。不得在正在编译的源码worktree切换到goal。
2. **一份当前状态。** main的`_zil_goal.md`只作入口；旧`.task`、`data/handoff.json`和进度报告是注明来源的工程记录/历史快照。`handoff`检查记录完整性，不充当实时待办，更不等于语言验收通过。
3. **按ID更新。** 代理提交“任务ID、读取的goal SHA、原状态→新状态、源码SHA、证据、阻塞”的增量。初期由root集中写回goal.md，各代理不覆盖整份文档。历史提交本身不证明源码、测试或合入存在。
4. **普通快进推送。** 更新前fetch并核对goal最新版本；只改所负责行及必要链接。推送被拒后重新读最新内容；不同任务可重新应用补丁，同一任务/字段并发修改先记录冲突，不强推、不自动覆盖。当前阶段不用Projects、机器人或双向同步系统。
5. **验收不能由拖状态产生。** “实现待验→已合入”必须有实际接受main、精确源码、适用独审和验收记录。上游切片/公开表示通过，不替代下游消费者；失败/未运行记录保留。
6. **每阶段备份。** 普通push后用`git ls-remote origin refs/heads/goal`核完整SHA。源码/WIP继续单独批量备份。离线bundle明确包含`refs/heads/goal`；只备份main不足以恢复当前计划。网络失败只能标本地已保存，不能标远端已同步。
7. **不凭旧故障停工。** 上轮代理曾因额度停止；本次已能运行新审计代理。PR35的Actions运行41868129740/41868129771再次核实因账户付款或额度限制而未启动、无steps，记录为未执行。接续时重查可用性，不把历史故障写成永久阻塞；原生Windows和正式CI未有成功证据时仍按未执行处理。

恢复阅读：先从GitHub读本文件，或在任意源码checkout运行：

```bash
git fetch origin refs/heads/goal:refs/remotes/origin/goal
git show refs/remotes/origin/goal:goal.md
```

首次建立独立编辑区：`git worktree add -b goal ../zil-goal refs/remotes/origin/goal`（本地goal已存在时使用`git worktree add ../zil-goal goal`，先检查是否已被其他worktree占用）。更新只在该工作区内进行，正常pull/commit/push；冲突按上述规则处理。

## 7. 本次建板说明

本次按实际远端引用、源码和既有验收收据整理，另由只读审计代理核对十类缺口；没有重新运行编译器全量测试，也没有增加已完成能力。现阶段先把单文档、稳定ID、分支归属与备份可靠运行。若以后同一文件更新冲突频繁，再在goal分支内拆任务记录并生成看板；现在不引入两份状态数据或同步服务。

[design]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/_zil_design.md
[gates]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/validation/product-gates.json
[m37]: https://github.com/zil-lang/zil/tree/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f
[c39]: https://github.com/zil-lang/zil/tree/c15b77847502d02acc83474bf7f9a8789186a3ac
[c40]: https://github.com/zil-lang/zil/tree/7dfc6846194643e5295a7704a0b8ce99fa5176e4
[c41]: https://github.com/zil-lang/zil/tree/7c7950dfc820d1b06a295814c662105842e03587
[w340]: https://github.com/zil-lang/zil/tree/5075da0c1f82ea80313b6f6a546fc4b027664063
[w343]: https://github.com/zil-lang/zil/tree/9764f5119c2ccdf49d6e5c81683c64c11a9cd017
[e37]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/terminal-root37-integration-20261104.json
[remote37]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/terminal-root37-remote.json
[e40]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/terminal-progress-20261104.json
[replacement]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/review340-replacement-producer-construction.json
[task40]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/review343-task40-classifier-carrier-fee-correction.json
[cas]: https://github.com/zil-lang/zil/blob/7dfc6846194643e5295a7704a0b8ce99fa5176e4/docs/validation/terminal-343-cas-machine.json
[ui]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/terminal-345-ui.json
[ui-review]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/review345-ui-terminal-architecture.json
