# ZIL 任务看板

更新：2026-11-05（Asia/Shanghai）。本文件是唯一实时任务状态；只记录下表有限范围，不代表完整语言、B0 或 ZUI 已完成。规范与门槛以任务源码分支的[设计][design]、具名附件和[产品条件][gates]为准。

已合入＝所列范围已进入远端 main；实现待验＝已有实现、尚未接受；部分实现＝完整链路未闭合；进行中＝检查正在运行、尚未验收；待执行/待排期＝尚未执行/派定。待办优先级仅 P0/P1/P2，已交付行用“—”。任务 ID 不带前导零。

[源码与恢复参考](README.md) · [分支、更新与备份规则](AGENTS.md) · [精简前完整记录](https://github.com/zil-lang/zil/blob/fc3dff269a882ef2850c4f9dba2acf6031ac4439/goal.md)

| ID | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G1.1 | 有限Option、记录式枚举、usize/u64原生及CT链、拥有Tuple和混合Tuple/Enum部分移动 | 已合入 | — | — | [已合入证据][e37]；一般模式组合另列。 |
| G1.2 | 恢复Task集成误删的Option拥有载荷路径 | 实现待验 | P0 | root / g10_1_review | [C39][c39] / [C40][c40]修复已实现；由 root / g10_1_review 在 G10.1 复核。 |
| G1.3 | 拥有字段替换：Verify责任转交、真实loan/pin、机器old-drop/store及失败清理闭环 | 部分实现 | P0 | root / g1_3 | [G1.3.1](https://github.com/zil-lang/zil/tree/8d80f87ee320bc4837a7d57623460194c41b9254) 已闭合局部 Store 元素字段替换并独审，已组合 PR36；跨 helper 的实例失效关系由 G1.3.2 继续。 |
| G1.4 | 更一般的地址/Package/ErrorUnion解构、混合模式及部分补齐组合 | 部分实现 | P1 | 待派 | 已有 `.ok/.err` 路径；按具体模式接 G1.3、G3.4。 |
| G2.1 | 有限const、supertrait、泛型trait/impl、关联类型/常量、默认体及泛型接收者方法 | 已合入 | — | — | [已合入证据][e37]；一般约束方程另列。 |
| G2.2 | 混合life/F helper、独立Invocation关系与未用声明检查，贯通两核及原生 | 实现待验 | P0 | root / g10_1_review | [C40][c40] 已有两核及原生检查；由 root / g10_1_review 在 G10.1 复核。 |
| G2.3 | 一般关联投影、类型/Self依赖的常量表达式、约束蕴含和循环诊断 | 部分实现 | P1 | g8_4_front | G2.3.1 已复现 Self 依赖常量缺口并确定逐 impl 的核验/求值方案，正在接既有 CT 链；一般投影、蕴含和泛型组合仍缺。 |
| G2.4 | 更一般life/lending、符号泛型方法和静态关联函数；一般/blanket/primitive/enum impl及更广supertrait实参组合 | 部分实现 | P1 | 待派 | 按需依赖 G2.3、G3.3、G3.4；保留定义期义务检查。 |
| G3.1 | 普通/嵌套Product借给可变helper、CopyT+mutable life、有限FP/HRTB和实际来源检查 | 已合入 | — | — | [已合入证据][e37]；一般借用返回见 G3.4。 |
| G3.2 | shared stored View、qualified静态Code及捕获为空的Fn，每个实际role独立核验 | 实现待验 | P0 | root / g10_1_review | [C39][c39] / [C40][c40] 已有 role 检查；由 root / g10_1_review 在 G10.1 复核。 |
| G3.3 | 捕获的外层Fixed上下文、qualified FnMut/FnOnce的真实环境与调用关系 | 部分实现 | P0 | root / g3_3 | 标量捕获及[G3.3.1实际life类修复](https://github.com/zil-lang/zil/tree/99b03ae59768aedd4d5f79e045275c6e74d08252)已独审并组合；G3.3.2正在复核外层Fixed接口，尚无实现。 |
| G3.4 | 普通借入字段、存储上界、affine借用、合法lending结果及长期组合 | 部分实现 | P1 | 待派 | 依赖 G3.3、G1.3；需实际 loan/root 证明。 |
| G3.5 | 一般拥有/借入捕获及责任组合的消费者矩阵 | 待排期 | P1 | 待派 | 依赖 G3.4、G6.1；按真实 API 补齐。 |
| G4.1 | 普通泛型Type返回、具体纯泛型记录的CT计算/反射和已有Syntax组合 | 已合入 | — | — | [已合入证据][e37]；完整上下文和值范围另列。 |
| G4.2 | CT4混合const/life/type全序号身份及原源重实体化 | 实现待验 | P0 | root / g10_1_review | [C39][c39] 已有限定独审和原生检查；由 root / g10_1_review 在 G10.1 复核。 |
| G4.3 | CT5真实Definition上下文、上下文键、物理字节计费和phantom Code/field查询 | 实现待验 | P0 | root / g10_1_review | [C40][c40] 已有上下文及原生检查；由 root / g10_1_review 在 G10.1 复核。 |
| G4.4 | 完整实例实际参数向量、成员/生成作用域、重定位与声明归属 | 部分实现 | P0 | root | 成员CT及[G4.4.1子值上下文修复](https://github.com/zil-lang/zil/tree/d1b7867ba21b38784d076eaa8337c3b0163e9be4)已独审并组合；完整实参向量/生成life仍缺。 |
| G4.5 | rich callable、ambient Type导出和更一般可序列化值/匿名Sum/拥有载荷组合 | 部分实现 | P1 | 待派 | 依赖 G4.4、G3.3、G3.4；补齐实际消费者。 |
| G5.0 | 具体Fn/FnMut/FnOnce、opaque拥有工厂及有限具体环境保存 | 已合入 | — | — | [已合入证据][e37]；仅动态抽象前置。 |
| G5.1 | dyn真实Source→Core声明/provider身份 | 部分实现 | P1 | 待派 | [C39][c39] / [C40][c40] 已有前端；实际消费者待接。 |
| G5.2 | 异构调用、静态vtable、关联项与有限泛型method slot的真实派发 | 部分实现 | P1 | 待派 | 依赖 G5.1、G2.3、G2.4；两核与机器派发待闭合。 |
| G5.3 | 显式拥有/共同借入存储及管理route，移动/销毁/失败路径完整 | 部分实现 | P1 | 待派 | 依赖 G3.4、G1.3、G5.2。 |
| G6.0 | 有限资源U/K/D、初始化/部分移动、Store及callable State合同 | 已合入 | — | — | [已合入证据][e37]；一般状态协议另列。 |
| G6.1 | 逐结果的一般状态转换、角色与别名规则 | 部分实现 | P1 | 待派 | [C40][c40] 已有逐 role 检查；一般转换待补。 |
| G6.2 | 真实投影、失效关系、历史撤销和导出上界组合 | 部分实现 | P1 | 待派 | 依赖 G6.1、G3.4。 |
| G6.3 | 状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1 | 待派 | 依赖 G1.3、G6.2、G8.4、G8.6。 |
| G7.1 | 安装函数返回后稳定拥有真实闭包环境的Registration、显式强关闭及失败清理 | 已合入 | — | — | [已合入证据][e37]；限同步关闭，不覆盖跨线程排空。 |
| G7.2 | 更一般借入/跨线程环境的登记对象与关闭合同 | 待排期 | P1 | 待派 | 依赖 G3.4、G8.5。 |
| G7.3 | 外部waker/晚通知、no-future及drain，关闭后所有raw入口静止 | 待排期 | P1 | 待派 | 与 G8.4 联合实施；需证明关闭后 raw 入口静止。 |
| G7.4 | Registration与异步任务责任的失败/取消/完成组合 | 待排期 | P1 | 待派 | 依赖 G7.2、G7.3、G8.4、G8.6。 |
| G8.1 | 结构化pthread、密封MutexI32/AtomicI32、稳定TaskI32帧和有限静态Suspend child | 已合入 | — | — | [已合入证据][e37]；限密封 I32 及有限静态任务。 |
| G8.2 | 静态拥有Task/child、保存SSA、Unit/有限Copy错误结果及清理 | 实现待验 | P0 | root / g10_1_review | [Task 复核][task40]；由 root / g10_1_review 在 G10.1 复核，不覆盖通用 frame port/拥有 Sum。 |
| G8.3 | CAS41真实源码→Types→两核/Wire→机器及争用/错误次序控制 | 实现待验 | P0 | root | [源码交付及作者收据](https://github.com/zil-lang/zil/blob/e797559b51cef6ccc75377caf55270ea8fc7a927/docs/validation/G8.3-cas-source.json)；非作者限定复核11项通过，待共同回归合入。 |
| G8.4 | 通用PendingExit、挂起/取消排空、外部waker及晚通知 | 实现待验 | P1 | root / g3_3 | [A源码及两核](https://github.com/zil-lang/zil/tree/3a3b7649d40b5f12eca6882d94fe0d5654732e94)已限定独审；[B机器消费者](https://github.com/zil-lang/zil/tree/43881a0965c490a7086bf12199b9e9247c3df99f)已冻结，真实Pending排空/晚通知/父子await通过作者检查，独审及共同全量待完成。 |
| G8.5 | 一般Mutex<T>/Guard、Transfer/Share、原子位宽和拥有同步构造 | 待排期 | P1 | 待派 | 依赖 G3.4、G6.1；从密封 I32 扩展到一般拥有类型。 |
| G8.6 | 一般拥有Sum/result、借入frame port、部分责任、ticket/defer的任务组合 | 待排期 | P1 | 待派 | 依赖 G1.3、G6.1、G6.2、G8.2、G8.4。 |
| G9.1 | 普通字符串/Unicode与事务编辑、Store插入移除、有限回调迭代/Option返回 | 已合入 | — | — | [已合入证据][e37]；完整库见 G9.3。 |
| G9.2 | 整数布局、RGBA软件合成、IME preedit提交/取消及失败保留的普通库 | 已合入 | — | — | [实现证据][ui] / [独审][ui-review]；不覆盖 OS IME、字体及 GPU。 |
| G9.3 | 完整字符串/集合与一般Iterator/lending组合 | 部分实现 | P1 | 待派 | 按具体 API 接 G3.4、G5.3；普通集合不普遍依赖 dyn。 |
| G9.4 | 真实平台窗口、事件循环及OS IME服务 | 待排期 | P2 | 待派 | 依赖 G7.3、G7.4 及平台接口；窗口和 IME 可拆分。 |
| G9.5 | 字体/文本布局、渲染呈现及输入到绘制的真实闭环 | 待排期 | P2 | 待派 | 与 G9.4 协调；需实际字体、呈现和输入链路。 |
| G9.6 | 真实ZUI应用集成、资源失败/关闭及维护场景 | 待排期 | P2 | 待派 | 依赖 G9.3、G9.4、G9.5 及必要语言能力。 |
| G10.0 | 原大数组O0崩溃、默认2MiB API/深调用栈可靠性及普通大型按值ABI | 已合入 | — | — | [已合入证据][e37]；原 O0 崩溃已修复，更广 ABI 见 G10.3。 |
| G10.1 | C40精确源码独审、受影响组合/既有正式门、修正与合入裁决 | 进行中 | P0 | root | [PR36](https://github.com/zil-lang/zil/pull/36)候选318875：组合栈修复已独审，完整默认4617通过/0失败；reference全量正在运行，release消费者待复验。Clippy及工程检查通过；Actions因账单/额度未启动。 |
| G10.2 | 有限f32/f64与UInt组合、匿名tagged Copy源Sum及CT Type结构Sum | 已合入 | — | — | [已合入证据][e37]；CT Sum 值与拥有载荷见 G4.5、G10.3。 |
| G10.3 | 大型closure/helper/foreign ABI及浮点/匿名联合责任的一般组合 | 部分实现 | P1 | 待派 | 补齐实际源码及最近反例；大数组修复不能覆盖全部 ABI。 |
| G10.4 | 原生Windows执行和平台差异验证 | 待执行 | P2 | 待派 | 需要原生 Windows 主机；环境可提前准备。 |
| G10.5 | 与Rust同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破 | 待执行 | P2 | root | 固定真实消费者、同语义 Rust 对照和独占计时窗口。 |
| G10.6 | B0、完整语言与ZUI终态验收 | 待执行 | P2 | root | 依赖全部必需语言任务、G9.6、G10.4、G10.5。 |

[design]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/_zil_design.md
[gates]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/validation/product-gates.json
[c39]: https://github.com/zil-lang/zil/tree/c15b77847502d02acc83474bf7f9a8789186a3ac
[c40]: https://github.com/zil-lang/zil/tree/7dfc6846194643e5295a7704a0b8ce99fa5176e4
[e37]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/terminal-root37-integration-20261104.json
[task40]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/review343-task40-classifier-carrier-fee-correction.json
[ui]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/terminal-345-ui.json
[ui-review]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/review345-ui-terminal-architecture.json
