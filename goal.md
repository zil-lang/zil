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

优先级：P0为当前集成、安全及恢复关键路径；P1为后续语言组合；P2为依赖稳定接口的平台和产品工作。批次表示依赖顺序，不是承诺完成日期。表中的340–345是已有施工责任区和源码来源，不表示旧代理仍在运行。实际新代理接手时只更新所负责行。

## 2. 当前源码与恢复入口

| 标识 | 实际源码/远端引用 | 已核范围 |
| --- | --- | --- |
| M37 | [`main@4bc1d77b5ec3bccb5b789fd813e85386f03ae13f`][m37] | PR33已合入；历史共同门默认4372、reference4261及相关检查。只适用于该接受树，完整语言/B0/ZUI未完成。见[E37][e37]及[远端合入收据][remote37] |
| C39 | [`task/language-terminal-20261104@c15b77847502d02acc83474bf7f9a8789186a3ac`][c39] | 可运行39/CT4候选；含后继修复和限定复核，未成为远端main。Option回归修复在该线另复核50项 |
| C40 | [`task/terminal-public40@7dfc6846194643e5295a7704a0b8ce99fa5176e4`][c40] | 作者组合检查：aggregates149、cross84、generic_regressions83、generics33，共349通过。不是全仓或终态验收；见[E40][e40] |
| C41 | [`task/terminal-cas41@7c7950dfc820d1b06a295814c662105842e03587`][c41] | CAS41公共运输候选；源码、两核、机器完整链未闭合 |
| W340 | [`backup/wip-20261104-340@5075da0c1f82ea80313b6f6a546fc4b027664063`][w340] | 拥有替换等实际未提交源码已保存为远端快照；不等于已验收 |
| W343 | [`backup/wip-20261104-343@9764f5119c2ccdf49d6e5c81683c64c11a9cd017`][w343] | CAS等实际未提交源码快照；不是仅保存一个提交编号 |

以上是建板时实查的基线，后续只按实际 `git ls-remote`、源码及证据更新。旧301–312缺失的对象/未提交内容没有因本看板建立而被宣布恢复。原大数组O0/默认2MiB栈问题已经修复并进入M37，不再作为待修缺口重复派工。

当前施工源码仍分布于39/40/41。优先在共同40接入已有消费者，保留39可运行检查点；不要把不同分支的通过数累加成一棵树的全量通过。

## 3. 下一批任务安排

1. **批次A：收敛已有成果。** G10.01对C40做精确独审和适用共同门；并行接续G01.03拥有替换、G03.03外层life上下文、G04.04完整CT上下文、G08.03源码CAS。先固定各自基线和不相交写集，公共表示由root协调。
2. **批次B：闭合跨能力组合。** 按前置推进G03.04 lending、G06.01/02一般状态、G05.02/03 dyn消费者，以及G07.03/G08.04取消排空与外部通知。不能用同步join替代外部入口排空。
3. **批次C：库、平台及产品。** G09系列依赖所需语言接口，原生Windows环境准备G10.04可以提前；正式成本测量和G10.06终验使用实际完整消费者。

G10.01只验收交付的候选范围，不是所有后续施工的前置审批门。环境、CI和独审资源不足时记录真实阻塞，未受影响的工作继续。

## 4. 分解任务与状态

### G01 基础类型与模式

规范引用：ZIL-SYNTAX、ZIL-MODES、ZIL-PACKAGE、ZIL-CLEANUP。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G01.01 | 有限Option、记录式枚举、usize/u64原生及CT链、拥有Tuple和混合Tuple/Enum部分移动 | 已合入 | — | 340；M37/E37。不是一般模式组合全部完成 |
| G01.02 | 恢复Task集成误删的Option拥有载荷路径 | 实现待验 | P0/A | root；C39 `022ad313e`、C40 `81a0667b7`；两树各50项，随G10.01集成核验 |
| G01.03 | 拥有字段替换：Verify责任转交、真实loan/pin、机器old-drop/store及失败清理闭环 | 部分实现 | P0/A | 续340；[生产者限定复核][replacement]、W340；不得把121/122/123表示接线记为完成 |
| G01.04 | 更一般的地址/Package/ErrorUnion解构、混合模式及部分补齐组合 | 部分实现 | P1/B | 续340/342；已有`.ok/.err`等局部消费者；按具体模式取G01.03/G03.04所需前置，不等待全部lending完成 |

### G02 泛型与 trait

规范引用：ZIL-TRAIT、ZIL-MODULE、ZIL-RECEIVER。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G02.01 | 有限const、supertrait、泛型trait/impl、关联类型/常量、默认体及泛型接收者方法 | 已合入 | — | 341；M37/E37；保留真实声明和模块身份，不表示全部符号方程已支持 |
| G02.02 | 混合life/F helper、独立Invocation关系与未用声明检查，贯通两核及原生 | 实现待验 | P0/A | 341/342/root；C40 `4d9d1b9ae`，两源8次native，N21782邻界；随G10.01验收 |
| G02.03 | 一般关联投影、类型/Self依赖的常量表达式、约束蕴含和循环诊断 | 部分实现 | P1/B | 续341；已有有限方程实现，后续按原源码逐式扩展 |
| G02.04 | 更一般life/lending、符号泛型方法和静态关联函数；一般/blanket/primitive/enum impl及更广supertrait实参组合 | 部分实现 | P1/B | 续341；按所需前置接G02.03、G03.03/04；定义期义务必须与实例一致，不能靠具体实例证明未用泛型体 |

### G03 借用与拥有组合

规范引用：ZIL-ORIGIN、ZIL-ACCESS、ZIL-CAPTURE、ZIL-PACKAGE。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G03.01 | 普通/嵌套Product借给可变helper、CopyT+mutable life、有限FP/HRTB和实际来源检查 | 已合入 | — | 342；M37/E37；广义借用返回另见G03.04 |
| G03.02 | shared stored View、qualified静态Code及捕获为空的Fn，每个实际role独立核验 | 实现待验 | P0/A | 342/root；C39/C40；raw/Wire及角色/别名近邻见E40 |
| G03.03 | 捕获的外层Fixed上下文、qualified FnMut/FnOnce的真实环境与调用关系 | 部分实现 | P0/A | 续342；C40当前仍拒绝缺实际外层上下文的情形；基于G02.02接续 |
| G03.04 | 普通借入字段、存储上界、affine借用、合法lending结果及长期组合 | 部分实现 | P1/B | 续342；依赖G03.03、G01.03；保留实际loan/root证明 |
| G03.05 | 一般拥有/借入捕获及责任组合的消费者矩阵 | 待排期 | P1/B | 342协调340/343；依赖G03.04、G06.01；按真实API补齐，不新增永久类型白名单 |

### G04 Type/Syntax 组合

规范引用：ZIL-COMPTIME、ZIL-MODULE、ZIL-TRAIT。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G04.01 | 普通泛型Type返回、具体纯泛型记录的CT计算/反射和已有Syntax组合 | 已合入 | — | 341/344；M37/E37；完整上下文和值范围仍见下列任务 |
| G04.02 | CT4混合const/life/type全序号身份及原源重实体化 | 实现待验 | P0/A | 344/root；C39 `e50317ad5`，5源控制、16次native；限定独审已记录，待共同合入 |
| G04.03 | CT5真实Definition上下文、上下文键、物理字节计费和phantom Code/field查询 | 实现待验 | P0/A | 344/root；C40，6项上下文控制、两个源8次native；G10.01 |
| G04.04 | 完整实例实际参数向量、成员/生成作用域、重定位与声明归属 | 部分实现 | P0/A | 续344；C40已有Definition路径，其他作用域不能套用该结论 |
| G04.05 | rich callable、ambient Type导出和更一般可序列化值/匿名Sum/拥有载荷组合 | 部分实现 | P1/B | 续344/341；依赖G04.04、G03.03/04；没有消费者的分项保持待实现 |

### G05 动态抽象

规范引用：ZIL-CAPTURE、ZIL-BYTE-STORAGE、ZIL-TRAIT、ZIL-ROUTE。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G05.00 | 具体Fn/FnMut/FnOnce、opaque拥有工厂及有限具体环境保存 | 已合入 | — | M37/E37；是动态抽象前置，不能称完整dyn |
| G05.01 | dyn真实Source→Core声明/provider身份 | 部分实现 | P1/B | 341/342；C39/C40有前端成果；需要与实际消费者合交，不单独标运行完成 |
| G05.02 | 异构调用、静态vtable、关联项与有限泛型method slot的真实派发 | 部分实现 | P1/B | 续341/342；依赖G05.01、G02.03/04；以两核和机器行为闭环 |
| G05.03 | 显式拥有/共同借入存储及管理route，移动/销毁/失败路径完整 | 部分实现 | P1/B | 续342；依赖G03.04、G01.03、G05.02 |

### G06 状态协议

规范引用：ZIL-CONTRACT、ZIL-ORIGIN、ZIL-ROUTE、ZIL-CLEANUP。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G06.00 | 有限资源U/K/D、初始化/部分移动、Store及callable State合同 | 已合入 | — | M37/E37；一般§5.2协议仍未完成 |
| G06.01 | 逐结果的一般状态转换、角色与别名规则 | 部分实现 | P1/B | 续342；C40逐role检查只是一个局部消费者 |
| G06.02 | 真实投影、失效关系、历史撤销和导出上界组合 | 部分实现 | P1/B | 续342；依赖G06.01、G03.04 |
| G06.03 | 状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1/B | 342协调340/343；依赖G01.03、G06.02、G08.04/06 |

### G07 外部回调生命周期

规范引用：ZIL-ASYNC、ZIL-CAPTURE、ZIL-CLEANUP；Design §12.4。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G07.01 | 安装函数返回后稳定拥有真实闭包环境的Registration、显式强关闭及失败清理 | 已合入 | — | 343；M37/E37；同步限定范围，不代表跨线程/异步入口全部静止 |
| G07.02 | 更一般借入/跨线程环境的登记对象与关闭合同 | 待排期 | P1/B | 343协调342；依赖G03.04、G08.05 |
| G07.03 | 外部waker/晚通知、no-future及drain，关闭后所有raw入口静止 | 待排期 | P1/B | 续343；与G08.04合派实际消费者；Busy/Closing不替代该证明 |
| G07.04 | Registration与异步任务责任的失败/取消/完成组合 | 待排期 | P1/B | 续343；依赖G07.02/03、G08.04/06 |

### G08 异步与并发

规范引用：ZIL-CONCURRENCY、ZIL-ASYNC、ZIL-CLEANUP、ZIL-LOWERING。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G08.01 | 结构化pthread、密封MutexI32/AtomicI32、稳定TaskI32帧和有限静态Suspend child | 已合入 | — | 343；M37/E37；一般线程/同步/任务并未全部完成 |
| G08.02 | 静态拥有Task/child、保存SSA、Unit/有限Copy错误结果及清理 | 实现待验 | P0/A | 343；C39/C40；[Task40限定复核][task40]，不覆盖通用frame port/拥有Sum |
| G08.03 | CAS41真实源码→Types→两核/Wire→机器及争用/错误次序控制 | 部分实现 | P0/A | 续343；C41、W343；[机器模型证据][cas]不等于源码完成；集成依赖共同40检查点 |
| G08.04 | 通用PendingExit、挂起/取消排空、外部waker及晚通知 | 待排期 | P1/B | 续343；与G07.03共用同一责任合同和真实事件源 |
| G08.05 | 一般Mutex<T>/Guard、Transfer/Share、原子位宽和拥有同步构造 | 待排期 | P1/B | 343协调342；依赖G03.04、G06.01；不能把密封I32接口当完整泛型实现 |
| G08.06 | 一般拥有Sum/result、借入frame port、部分责任、ticket/defer的任务组合 | 待排期 | P1/B | 续343；依赖G01.03、G06.01/02、G08.02/04 |

### G09 标准库与平台

规范引用：ZIL-MODULE、ZIL-COMPTIME、ZIL-ASYNC、ZIL-TARGET；产品场景引用原profile。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G09.01 | 普通字符串/Unicode与事务编辑、Store插入移除、有限回调迭代/Option返回 | 已合入 | — | 345及语言施工；M37/E37；不是完整字符串/集合库 |
| G09.02 | 整数布局、RGBA软件合成、IME preedit提交/取消及失败保留的普通库 | 已合入 | — | 345；[UI作者证据][ui]及[限定独审][ui-review]；不覆盖OS IME/字体/GPU呈现 |
| G09.03 | 完整字符串/集合与一般Iterator/lending组合 | 部分实现 | P1/B | 续345协调342；按具体API取G03.04/G05.03所需前置，普通集合不普遍依赖dyn；逐API列输入/清理 |
| G09.04 | 真实平台窗口、事件循环及OS IME服务 | 待排期 | P2/C | 续345；依赖G07.03/04及所需FFI/平台接口；窗口与IME可分工 |
| G09.05 | 字体/文本布局、渲染呈现及输入到绘制的真实闭环 | 待排期 | P2/C | 续345；普通软件光栅不能替代平台呈现；与G09.04协调 |
| G09.06 | 真实ZUI应用集成、资源失败/关闭及维护场景 | 待排期 | P2/C | root/345；依赖G09.03/04/05及必要语言缺口 |

### G10 规范与产品验证

规范引用：ZIL-TARGET、ZIL-ARCH、ZIL-ACCEPT；唯一量化条件为product-gates。

| ID | 任务范围与完成边界 | 工作状态 | 优先级/批次 | 责任及证据/前置 |
| --- | --- | --- | --- | --- |
| G10.00 | 原大数组O0崩溃、默认2MiB API/深调用栈可靠性及普通大型按值ABI | 已合入 | — | M37/E37；旧失败保留，更广ABI组合见G10.03 |
| G10.01 | C40精确源码独审、受影响组合/既有正式门、修正与合入裁决 | 待执行 | P0/A | root；独审由非作者承担。349项不是正式全量，CI未运行不能记通过 |
| G10.02 | 有限f32/f64与UInt组合、匿名tagged Copy源Sum及CT Type结构Sum | 已合入 | — | 344；M37/E37；CT Sum值、拥有载荷及全部责任组合仍归G04.05/G10.03 |
| G10.03 | 大型closure/helper/foreign ABI及浮点/匿名联合责任的一般组合 | 部分实现 | P1/B | 344/root；按真实源码及最小反例补齐；不能复用旧大数组修复当全部证明 |
| G10.04 | 原生Windows执行和平台差异验证 | 待执行 | P2/C，可提前准备环境 | root/345；需要实际Windows主机，Linux交叉构建/Wine不代替原生执行 |
| G10.05 | 与Rust同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破 | 待执行 | P2/C | root；固定实际产品消费者，独占计时窗口，沿用原profile |
| G10.06 | B0、完整语言与ZUI终态验收 | 待执行 | 最终 | root；依赖全部必需语言任务、G09.06及G10.04/05；没有因建板而完成 |

## 5. 分支派工与文档归属

| 责任区 | 当前源码分支 | 分支内实施说明 | 接续任务 |
| --- | --- | --- | --- |
| 基础/模式340 | `task/340`及W340 | `.task/impl/340.md` | G01.03/04 |
| 泛型341 | `task/341`，近期成果已组合至C40 | `.task/impl/341.md` | G02.03/04、G05.01/02 |
| life/协议342 | `task/342`、`task/342-rich40`及C40 | `.task/impl/342.md` | G03.03/04、G06.01/02 |
| 运行时343 | `task/343`、W343、C41 | `.task/impl/343.md` | G08.03，后续G07/G08责任组合 |
| CT/可靠性344 | `task/344`，近期成果已组合至C40 | `.task/impl/344.md` | G04.04/05、G10.03 |
| 库/平台345 | `task/345`保留旧交付；下一任务从选定共同基线建立 | `.task/impl/345.md` | G09.03起；不得直接把旧分支当最新共同基线 |
| 集成root | C39/C40/C41与新派任务分支 | 各分支对应`.task`说明 | G10.01、规范/公共接口协调及阶段备份 |

这里的`task/…`是GitHub运输引用；本地旧分支可能名为`340`、`341`等，先核真实SHA再操作。不要因名字相似合并旧副本。开始接续时明确选择基线，将已有源码增量和WIP恢复到可复核树，再限定写集；不再新建很多测试目标。

`goal.md`只维护当前任务行；分支`.task`保存目标ID、父合同、基线、规范身份、写集、实现方案、交付和验证要求，不再另记一份“当前排期/进度”。分支交付时把有用的设计决策、源码、精简复核/验收记录带入main；旧工单历史保留。main既有正式工程合同仍约束规范及验收，不能以迁移看板为由删除Done条件。

## 6. 更新、冲突和备份规则

1. **独立工作区。** `goal`是轻量独立历史分支，不合入main；开发分支也不merge进goal。不得在正在编译的源码worktree切换到goal。
2. **一份当前状态。** main的`_zil_goal.md`只作入口；旧`.task`、`data/handoff.json`和进度报告是注明来源的工程记录/历史快照。`handoff`检查记录完整性，不充当实时待办，更不等于语言验收通过。
3. **按ID更新。** 代理提交“任务ID、读取的goal SHA、原状态→新状态、源码SHA、证据、阻塞”的增量。初期由root集中写回goal.md，各代理不覆盖整份文档。历史提交本身不证明源码、测试或合入存在。
4. **普通快进推送。** 更新前fetch并核对goal最新版本；只改所负责行及必要链接。推送被拒后重新读最新内容；不同任务可重新应用补丁，同一任务/字段并发修改先记录冲突，不强推、不自动覆盖。当前阶段不用Projects、机器人或双向同步系统。
5. **验收不能由拖状态产生。** “实现待验→已合入”必须有实际接受main、精确源码、适用独审和验收记录。上游切片/公开表示通过，不替代下游消费者；失败/未运行记录保留。
6. **每阶段备份。** 普通push后用`git ls-remote origin refs/heads/goal`核完整SHA。源码/WIP继续单独批量备份。离线bundle明确包含`refs/heads/goal`；只备份main不足以恢复当前计划。网络失败只能标本地已保存，不能标远端已同步。
7. **不凭旧故障停工。** 上轮代理曾因额度停止，CI曾因账户限制未启动；本次已能运行新审计代理。接续时重查可用性，不把历史故障写成永久阻塞；原生Windows和正式CI未有成功证据时仍按未执行处理。

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
