# ZIL 任务看板

更新：2026-11-09（Asia/Shanghai）。本文件是唯一实时任务状态；只记录下表有限范围，不代表完整语言、B0 或 ZUI 已完成。规范与门槛以任务源码分支的[设计][design]、具名附件和[产品条件][gates]为准。

已合入＝所列范围已进入远端 main；实现待验＝已有实现、尚未接受；部分实现＝完整链路未闭合；进行中＝正在实现或验证、尚未验收；待执行/待排期＝尚未执行/派定。待办优先级仅 P0/P1/P2，已交付行用“—”。任务 ID 不带前导零。

[源码与恢复参考](README.md) · [分支、更新与备份规则](AGENTS.md) · [精简前完整记录](https://github.com/zil-lang/zil/blob/fc3dff269a882ef2850c4f9dba2acf6031ac4439/goal.md)

按能力分类，主表显示各项任务；子任务放在所属任务下的独立子表，默认折叠。展开后仍保留范围、状态、优先级、负责人和进展列，不使用四级标题。

父任务汇总完整范围，子任务记录具体交付，父子不重复计数；子项全完成不自动代表父项完成。需要独立派工、复核或验收的补充工作才新增任务编号，沿用父编号加一段；零散修正记在原任务进展中。编号同时用于分支与实施说明，不再另设勾稽编号。

**当前主线：以通用编程语言 1.0 的功能闭环为下一阶段目标。** 当前交付 `main@b07b85dd4` 已接受本批有限语言范围，精确运行口径见下文。仍是开发预览，不能据此称1.0完备。

本阶段先补普通数据模型、泛型库、模式、同步与标准库，再收动态抽象、图/外部生命周期和完整程序。必需能力全部进入P0；有普通合法程序仍被临时白名单或未实现组合阻挡，就保留缺口，不能靠缩小子项关闭父项。运行/编译成本和安全产品门保持原要求，功能就绪与正式发布分别验收。

Sol high负责实现，疑难交Astra。前一交付main@53e3e092d：借入Package窗口的ByteBuffer长度/容量查询、Direct actual-read效应及按程序点保留/撤销的弱委派。默认整树单轮5659/0、参考相关四目标682/0、strict/格式/结构通过，GitHub独立冷构建16次执行及3项真实loan拒绝通过。[精确验收快照](https://github.com/zil-lang/zil/blob/53e3e092d45a19dcdcd01e88d2cc25e4f7670793/validation/native-language-composition.json)。新增初始化修复已合入main@b07b85dd4：相关690/0与全目标strict通过，见[限定验收](https://github.com/zil-lang/zil/blob/b07b85dd49b3df6f382734d509919a2a6013c97d/validation/shared-array-snapshot-initialization.json)。组合候选已备份至task/G10.6.3@ed8e3a5de：拥有trait返回、泛型lending、回调、字段替换、动态数组及资源工作量修复已汇总，两核单测207/0、459/0。完整原消费者仍阻于拥有字段Byte视图，正在G3.4.22修复；未共同验收或宣称1.0。

## G1 · 基础类型与模式

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G1.1 | 有限Option、记录式枚举、usize/u64原生及CT链、拥有Tuple和混合Tuple/Enum部分移动 | 已合入 | — | — | [已合入证据][e37]；一般模式组合另列。 |
| G1.2 | 恢复Task集成误删的Option拥有载荷路径 | 已合入 | — | — | [本批验收][e41]；有限拥有 Option/Task 集成恢复已通过共同验收。 |
| **G1.3** | **拥有字段替换：Verify责任转交、真实loan/pin、机器old-drop/store及失败清理闭环** | 已合入 | — | — | [PR46 验收][e-pr46] 补齐本表拥有替换组合：真实来源、loan/pin、责任、退休和失败清理；更一般借用/模式仍见 G1.4、G3.4。 |
| G1.4 | 更一般的地址/Package/ErrorUnion解构、混合模式及部分补齐组合 | 进行中 | P0 | sol high · patterns | 已有 `.ok/.err` 路径；按具体模式接 G1.3、G3.4。 |
| G1.5 | 补齐u32、i8/i16/isize整数矩阵 | 待排期 | P1 | 待派 | 优先u32；须单次协调IR/Wire/CT/转换/布局/FFI/双后端，当前六种整数已实现，不能只加语法。 |

<details>
<summary>G1.3 · 子任务（6 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G1.3.4 | 局部完整 Package 替换：唯一根、单个直接 Managed 字段、来源转交与失败清理 | 已合入 | — | — | [PR41 验收][e-pr41]；真实来源、旧值/新临时值清理及多根反例已闭合。 |
| G1.3.5 | 完整局部 Enum 替换：跨分支来源、实际票、失败清理与唯一根检查 | 已合入 | — | — | [PR42 验收][e-pr42]；有限载荷、真实来源与失败清理已闭合，既有单已知变体精度边界保留。 |
| G1.3.6 | 跨 helper 资源退休使旧 Desc 失效；补齐本体与调用摘要的独立检查 | 已合入 | — | — | [PR42 验收][e-pr42]；两核按实际根/字段/捕获独立检查，拒绝旧描述后用并保留合法分离调用。原误拒、旧断言失败及修正记录保留。 |
| G1.3.7 | 通过可变 helper 整体替换 Package：借入端点、责任转交与准确的返回后来源 | 已合入 | — | — | [PR43 验收][e-pr43]；两核及独审 Source/raw/Wire/机器消费者通过。首片 Direct Unit，Fresh 限单块单 Commit 无调用，保留实际来源、旧描述失效与 caller 清理年龄。 |
| G1.3.8 | 整体 Package helper 的非 Unit/错误返回、失败时旧值保留与清理 | 已合入 | — | — | [PR44 验收][e-pr44]；Direct helper 有限普通 Copy/错误返回已到 Source/raw/Wire/双后端；实际 Commit/Abort、旧值及临时清理成立，非 Unit 不授 Fresh。 |
| G1.3.9 | 拥有值替换剩余组合：非局部 Package/Enum、capture/Store、loan/pin 与失败清理 | 已合入 | — | — | [PR46 验收][e-pr46] 非局部/capture/Store 与 Package/Enum/ErrorUnion 组合已闭合，含旧值/临时值清理及最近责任反例。 |

</details>

<details>
<summary>G1.4 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G1.4.1 | 具名 Record/Enum 的普通绑定与条件模式、名义身份/可见性及真实字段借用 | 已合入 | — | — | `dc7be483d`已进入联合候选；具名Copy Record/Enum和身份反例通过专项， 已由main@222e550af本批有限范围验收。 |
| G1.4.2 | 拥有Package具名解构、选择性移动、ref/ref mut混合及拥有错误载荷消费 | 已合入 | — | — | `557753f32`：递归拥有Package模式、ref/ref mut与兄弟移动、错误key、真实清理正反例已专项通过，进入联合候选；不代表父项全部完成。 已由main@222e550af本批有限范围验收。 |
| G1.4.4 | 显式ErrorUnion成功/失败构造及泛型同型实例的已选tag保真 | 已合入 | — | — | `762213171`精确独审并合：AST/TypeRef站点、trait default/impl实体化前保留已选分支，E=T及真实owning清理正反例通过； 已由main@222e550af本批有限范围验收。 |
| G1.4.5 | 已知ErrorUnion标签的真实CFG可达性与局部拥有模式 | 已合入 | — | — | `a40bc6134`独审并合：由Verify实际完整tag裁边，资源liveness/effects/transfer同图，旧观察/可达债6类raw/Wire拒；真实Clang/API通过。 已由main@222e550af本批有限范围验收。 |
| G1.4.6 | 显式错误构造与既有Store/资源give-create路径兼容 | 已合入 | — | — | `538f81656`精确独审接受、18目标190/0、EDCBA四配置及真实来源反例；Ref资源type alias当前在Expand即拒，未冒充构造器支持。 已由main@222e550af本批有限范围验收。 |
| G1.4.7 | 泛型调用实参的定义期错误tag，以及显式泛型Store构造 | 已合入 | — | — | `536/b81/206/266`四增量精确独审并合：完整定义约束/真实AST锚保tag，owner/fresh及method实参保真；新native和raw/Wire、原104深调用/预算控制通过， 已由main@222e550af本批有限范围验收。 |
</details>

## G2 · 泛型与 trait

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.1 | 有限const、supertrait、泛型trait/impl、关联类型/常量、默认体及泛型接收者方法 | 已合入 | — | — | [已合入证据][e37]；一般约束方程另列。 |
| G2.2 | 混合life/F helper、独立Invocation关系与未用声明检查，贯通两核及原生 | 已合入 | — | — | [本批验收][e41]；混合 life/F helper、独立 Invocation 与未用声明检查已通过共同验收。 |
| G2.3 | 一般关联投影、类型/Self依赖的常量表达式、约束蕴含和循环诊断 | 部分实现 | P0 | 待派 | G2.3.1 有限 Self 依赖常量已由 PR39 [合入][e39]；一般投影、泛型及 Drop 组合仍缺。 |
| G2.4 | 更一般life/lending、符号泛型方法和静态关联函数；一般/blanket/primitive/enum impl及更广supertrait实参组合 | 部分实现 | P0 | sol high · traits | 共享lending、静态关联函数、具体primitive/Enum及有限拥有Package宿主已合入main；更广blanket、life/lending和约束组合仍保留。 |

<details>
<summary>G2.3 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.3.3 | 关联投影贯通普通函数、Struct/Enum字段、泛型/具体别名及多约束库 | 已合入 | — | — | `deb9b77a4`：关联投影、拥有Package Item、K/V/E/H约束、真实跨模块回调Map与私有consumer provider已合候选；一般owning泛型与impl族仍由父项保留。 已由main@222e550af本批有限范围验收。 |
| G2.3.4 | Tuple/Array完整POD元素、generic Store与实际类型身份 | 已合入 | — | — | `2ca7bce64`/`9995b9fc7`/`dbf09adcf`已进入候选；Source、raw/Wire同布局错型、双后端及故障清理专项通过， 已由main@222e550af本批有限范围验收。 |
| G2.3.5 | 通用完整Value责任、多binder Entry及owning泛型容器 | 已合入 | — | — | `6645db3a4`已合候选：有序多binder、完整zero-debt Package仍affine、Managed/Value字段消费、普通Store helper全调用写hold；额外bound漏消费及库存费已修并独审，Clang/API各47专项， 已由main@222e550af本批有限范围验收。 |
</details>

<details>
<summary>G2.4 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.1 | 普通 trait 逐调用 lending iterator：关联 Item、方法 life 与真实接收者来源 | 已合入 | — | — | [PR50 验收][e-pr50]：跨模块关联 Item、方法 life、真实 std.iter、两后端 O0/O2；默认5146/0、参考5034/0。原 SliceIter/Option 转发回归已修复；后续G2.4.10已补有限`Store<Item>`拥有宿主；更一般owning lending仍缺。 |
| G2.4.2 | 普通 trait 静态关联函数：类型限定调用、符号泛型宿主、既有 type-only 参数与返回组合 | 已合入 | — | — | [PR51 验收][e-pr51]：具体/符号类型宿主、type-only 方法、where 与完整返回、跨模块 provider 身份通过；旧 receiver 调用费用保持。 |
| G2.4.3 | impl 关联类型显式约束：定义期证明、独立 binder 与跨模块泛型消费者 | 已合入 | — | — | `main@e800b39d5`；定义期显式 impl 关联约束、独立 binder、跨模块及原生已接受。[共同验收][e-native-foundations]；一般投影/lending 留在父项。 |
| G2.4.5 | 标量/Enum的普通trait impl、实际owner及完整provider调用 | 已合入 | — | — | 已合候选并修默认Self真实receiver；相关184项及两后端/旧privacy/精确预算联合通过。 已由main@222e550af本批有限范围验收。 |
| G2.4.6 | 泛型定义与shared Store/defer既有义务兼容 | 已合入 | — | — | `027b510a8`/`40c355258`/`671c7ffa5`已入候选；真实shared引用仍只读、拥有let可管理Store、未用Copy/nominal义务及私有host边界恢复。旧Types418等精确边界与183项联合回归通过， 已由main@222e550af本批有限范围验收。 |
| G2.4.7 | 普通拥有Package共享trait宿主与跨module provider | 已合入 | — | — | `f343334d2`已合；恢复实际Package owner import边，shared固定签名及私有/同名provider；154trait专项和Source/raw/Wire/native， 已由main@222e550af本批有限范围验收。 |
| G2.4.8 | 普通拥有Package的独占trait接收者及跨module泛型调用 | 已合入 | — | — | `e9f530337`独审并合；Unit/scalar可变方法、真实拥有负载、158回归/四配置失败清理与raw实际receiver/provider反例通过。 已由main@222e550af本批有限范围验收。 |
| G2.4.9 | 导出life泛型get实例化诊断 | 无需改动 | — | — | 已证为include_str嵌入旧stdlib的CLI与接口改名不一致；最终源重建后正确。零生产修改，空分支已保全并删除，编号不复用。 |
| G2.4.10 | 普通拥有Item的trait lending iterator | 已合入 | — | — | main@0e770a26e；普通`Store<Item>`拥有Item的trait lending与跨模块消费者已交付；165/0专项、四配置及allocfail、精确预算通过，非GAT/dyn或广泛ByteOption。 |
| G2.4.11 | 拥有Package实例trait方法返回普通拥有值及错误结果 | 待集成 | P0 | sol high · owning_iterator | 基线main@53e3e092d；跨模块Duplicate/可失败拥有返回、定义期义务及一次清理；冻结fc20fce58：专项172/0、Expand30/0及40原生/分配失败通过；Sol已精审接受，临时method match推导边界保留。 |
| G2.4.12 | lending helper的额外泛参与多个无歧义bound | 待集成 | P0 | sol high · lending_generic_composition | 87e0d028f已获Sol精审接受；291/0、4配置原生及新旧精确费用通过；root候选fbf69d299待共同门，不扩GAT/blanket。 |

<details>
<summary>G2.4.11 · 普通方法与生命周期组合</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.11.1 | 无关life trait不改变普通具体impl正文检查路径 | 待集成 | P0 | sol high · owning_iterator；Astra判据 | 6c5162cd9已获Sol/Astra限定接受；320相关及40新原生通过，保持完整conformance、unused/body及符号定义义务；root已合，待共同门。 |

</details>

<details>
<summary>G2.4.10 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.10.1 | 拥有Item lending与旧定义期义务/精确费用控制兼容 | 已合入 | — | — | main@0e770a26e；旧定义期义务/实际Copy反例与逐项新增费用兼容完成；旧失败保留，未修改生产规则或限额。 |

</details>

<details>
<summary>G2.4.2 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.2.1 | 符号类型宿主、定义期义务与词法值优先 | 已合入 | — | — | [PR51][e-pr51]：词法值优先、符号宿主边界和未用定义期义务均保留；非法声明与最近反例仍拒绝。 |

</details>

<details>
<summary>G2.4.1 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.1.1 | trait 方法声明、impl binder 对齐与关联 Item 实例化 | 已合入 | — | — | [PR50][e-pr50]：完整 provider 身份、alpha 对齐、Copy 与未用定义义务通过；没有按生成名称授予来源。 |
| G2.4.1.2 | life/Types 关系与 Option 结果合同 | 已合入 | — | — | [PR50][e-pr50]：实际 receiver 来源、认证跨模块 Option 与构造器完成；固定宿主 life 和调用 life 分别处理，原用例保留。 |
| G2.4.1.3 | Analyze 实际接收者、调用窗口与返回来源 | 已合入 | — | — | [PR50][e-pr50]：真实 Source 正例与局部载荷伪造反例通过；Analyze 生产核未放宽。 |
| G2.4.1.4 | Verify 独立来源/父loan与 raw/Wire 复核 | 已合入 | — | — | [PR50][e-pr50]：独立 raw/Wire 活结果相交调用拒绝和合法不同 receiver 通过；Verify 生产核未放宽。 |

</details>

</details>

<details>
<summary>G2.4.12 · 泛型lending精确费用</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.12.1 | lending候选资格扫描保持旧Expand精确预算 | 待集成 | P0 | sol high · lending_generic_composition | 44e1540eb已精审接受并合root；先排除既有不支持返回形状，再扫无关约束；旧827/1106费用原断言保持，共同相关378/0。 |

</details>

## G3 · 借用、拥有与生命周期

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.1 | 普通/嵌套Product借给可变helper、CopyT+mutable life、有限FP/HRTB和实际来源检查 | 已合入 | — | — | [已合入证据][e37]；一般借用返回见 G3.4。 |
| G3.2 | shared stored View、qualified静态Code及捕获为空的Fn，每个实际role独立核验 | 已合入 | — | — | [本批验收][e41]；真实静态 Code、空捕获 Fn 与逐 role 检查已通过共同验收。 |
| **G3.3** | **捕获的外层Fixed上下文、qualified FnMut/FnOnce的真实环境与调用关系** | 已合入 | — | — | [PR46 验收][e-pr46] 本表 Fixed/Invocation、闭包 helper、循环及多 role 组合已闭合；普通闭包 Value 的 give 按规范拒绝，一般 lending 继续见 G3.4。 |
| G3.4 | 普通借入字段、存储上界、affine借用、合法lending结果及长期组合 | 部分实现 | P0 | 待派 | 有限Record/Package/地址数组、拥有Package直接数组、唯一输入whole共享返回及字节query已合；动态下标、更多来源/投影/lending/捕获仍未完。当前优先G3.4.17，有限子项不关闭父项。 |
| G3.5 | 一般拥有/借入捕获及责任组合的消费者矩阵 | 进行中 | P0 | sol high · callbacks | G3.5.1先补借入输入到封闭拥有结果；捕获环境的一般存储/运输仍留父范围。 |

<details>
<summary>G3.3 · 子任务（8 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.3.2 | 真实捕获共享 Fn、有状态 FnMut 与有限拥有 FnOnce | 已合入 | — | — | [PR40 验收][e40]；不含一般 helper/lending 与用户 Drop。 |
| G3.3.4 | 共享 Fn free-helper：单 F、实际地址字段、标量结果与来源桥 | 已合入 | — | — | [PR41 验收][e-pr41]；phantom 错误来源已修复，源码、两核与机器消费者通过。 |
| G3.3.5 | 真实有状态 FnMut free-helper 的捕获来源与可变调用模式 | 已合入 | — | — | [PR42 验收][e-pr42]；同一真实环境两次状态变化、物理共享捕获与独立来源核验已到机器，旧 None 费用回归已修复。 |
| G3.3.6 | 真实拥有 FnOnce 的按值 free-helper、捕获来源与一次消费 | 已合入 | — | — | [PR42 验收][e-pr42]；两核独立恢复实际参数暂存来源，消费真实环境一次。限单 F、有限 scalar resource 与共享 Tag，give/lending/用户 Drop 另列。 |
| G3.3.7 | 有限拥有 FnOnce helper 在消费完成后的动态分支 | 已合入 | — | — | [PR43 验收][e-pr43]；两核全图身份、一次消费、raw/Wire、预算与机器完整值通过。限 entry 消费后两个退出块；独立双环境结果198。 |
| G3.3.8 | 拥有 helper 消费前分支及更一般无环 CFG 的实际来源 | 已合入 | — | — | [PR44 验收][e-pr44]；消费前分支、一般无环合流、合法 Drop 分支的实际参数/Local/SSA 来源由两核独立证明；旧快路预算保持。循环、lending、多 F/用户 Drop 仍另缺。 |
| **G3.3.9** | 闭包 helper 剩余组合：循环、lending、多个 F、Managed 转交和用户 Drop | 已合入 | — | — | [PR46 验收][e-pr46] 循环实际来源、同 F 多位置、Fn/FnMut→Once 环境责任、Managed/lending/用户 Drop 组合已到独立两核和原生。 |
| G3.3.10 | 多 callable 参数的完整实例键与逐角色来源消费者 | 已合入 | — | — | [PR46 验收][e-pr46] 完整实例键与逐角色来源已接通；独审发现的查找/事件替换/具体角色过滤欠计均关闭，旧一/二角色费用保持。 |

<details>
<summary>G3.3.9 · 回归收口</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.3.9.1 | 新闭包组合与历史验收共同收口 | 已合入 | — | — | [PR46 验收][e-pr46] 新组合与历史正反例、精确预算共同通过；真实重复消费和错误身份仍拒绝，默认额度未上调。 |

</details>

</details>

<details>
<summary>G3.4 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.4.1 | 原 SliceIter 与 Option<&T> 的 life/返回来源消费者 | 已合入 | — | — | [PR48 验收][e-pr48]：原始源码、两核 raw/Wire、连续 next/失效及四配置原生完整输出42；合法life遮蔽和原位置拒绝回退已修复。机器首片限thin shared Address，一般lending另缺。 |
| G3.4.2 | 单直接可变叶 Record holder：真实来源、父 loan、helper 重借与失效 | 已合入 | — | — | `main@e800b39d5`；实际目标/父 loan、helper 重借、raw/Wire 和双后端 O0/O2 接受；通过标量 helper 与 Task/登记联编。[验收][e-native-foundations]；混合拥有 Package、多叶和槽操作仍缺。 |
| G3.4.3 | 混合拥有/借入 Package 与多叶可变存储，同函数移动、借出及准确清理 | 已合入 | — | — | 最终`ce5a8e337`已独审并合：真实constructor child、普通Store地址转交、全部spent使用、may-empty复制及合法循环；28专项/24新native。先前406静态接受已撤回，以ce5为准；更广嵌套Record等父范围仍未关闭。 已由main@222e550af本批有限范围验收。 |
| G3.4.4 | 存储流新能力保持旧精确请求预算 | 已合入 | — | — | `8a01aa338`已整合，原Analyze精确N=14196保持；重复资格扫描和无关投影已移除，Analyze全203/0。 已由main@222e550af本批有限范围验收。 |
| G3.4.5 | 独立Verify新增存储/Task证明保持旧请求费用 | 已合入 | — | — | `9f75c5538`与root74900adc3已整合；旧captured helper24513、old-tail raw/Wire52141及Package13954原断言恢复；新写槽十项仍通过。 已由main@222e550af本批有限范围验收。 |
| G3.4.6 | 普通借入Enum与ErrorUnion的分支条件、来源和完整消费者 | 已合入 | — | — | `0f711d389`：真实variant/class、泛型完整实例、跨helper/ErrorUnion及short-to-long raw/Wire控制专项通过；原Option17项已复核， 已由main@222e550af本批有限范围验收。 |
| G3.4.7 | 标准Poll的真实借入载荷与sealed身份 | 已合入 | — | — | `a1e737c09`已合；真实role/fullR、19布局身份反例、跨module与12新native通过，不等于Task结果已验收。 已由main@222e550af本批有限范围验收。 |
| G3.4.8 | Store实际manager借入叶/字段返回、一般life及Option借出组合 | 已合入 | — | — | `f3b600238`精确独审接受并合：真实Map get/inspect/identity/field helper、Some投影前强pin、两核真实heap来源，24机器及长heap N/N−1通过；已保留旧ce5构造/消费规则， 已由main@222e550af本批有限范围验收。 |
| G3.4.9 | 独占constructor实际loan site与现有写槽组合 | 已合入 | — | — | `a19ee3339`已独审并合；constructor只激活自己的site排程，不扩大slot updates权限；原三误拒恢复、cross112/stored33/mixed28专项通过。 已由main@222e550af本批有限范围验收。 |
| G3.4.10 | 借入结果Task与拥有Map/Option在同一普通CFG中的真实组合 | 已合入 | — | — | `c5/c8/e042/e6`及独立`a030/10dd`已由main@222e550af本批验收：标准let/临时match/双来源if真实default、16native与失败cleanup/rawWire通过；先前默认耗尽与Copy Poll兼容首次失败保留。 |
| G3.4.11 | 拥有Package嵌入含真实借入叶的qualified Record | 已合入 | — | — | main@0e770a26e；qualified Record含真实借入叶进入有限拥有Package；构造、getter及真实来源映射通过。窗口传播漏洞由子项修复；父Package地址经identity helper返回仍由G3.4.13接续。 |
| G3.4.12 | 固定共享地址数组的普通函数与存储消费者 | 已合入 | — | — | main@62e9c85dd；有限非零薄共享地址数组、qualified Record、常量ordinal和helper来源已合。真实Source/raw/Wire、精确预算及四机器配置通过；拥有Package直接含数组见G3.4.16，动态/独占/空数组范围未扩大。 |
| G3.4.13 | qualified拥有Package的整对象共享地址经普通helper返回 | 已合入 | — | — | main@62e9c85dd；实际唯一输入的whole共享地址返回及pick_second/多根单实参已交付；不从声明造根，资源与loan独立检查。多个callee来源和投影实参仍另缺；字节query后续已由G3.4.15补有限消费者。 |
| G3.4.14 | 地址数组、Record窗口及整Package借入返回的共同来源与预算验收 | 已合入 | — | — | main@62e9c85dd；保留三域规则，修复binary默认栈帧，原104请求恢复；默认修复完整覆盖5633/0、参考相关656/0、冷执行16次及strict过。当批Package含Array未开放，后续G3.4.16已补有限组合；不关闭父能力。 |
| G3.4.15 | 借入Package窗口的ByteBuffer长度/容量查询与实际read权限 | 已合入 | — | — | main@53e3e092d；规范DescByte输入零class、真实Direct读效应、逐alternative来源/当前委派及独立资源生存期已验收。循环site重执行撤销旧grant，原错误Success已拒；不把weak变active，不扩输出/capture/间接路径。 |
| G3.4.16 | 拥有Package含共享地址数组及普通helper返回的组合 | 已合入 | — | — | main@c1504aac7；实际Package字段常量ordinal、两life非identity映射、whole/selected loan和Managed cleanup已验收。70专项/旧23/104默认栈/20新预算/API20与无API10native通过；大Package ABI未验，512默认Source仅Analyze ResourceLimit。 |
| G3.4.17 | 固定共享地址数组的运行时下标与真实借入来源 | 待集成 | P0 | sol high · borrowed_record；Astra审 | 1fdd1b746已获Astra精确接受；相关111/0、动态19/0、旧预算23/0、104默认请求及strict通过；四配置12成功/8bounds trap，root已合。无通用life meet，旧无dynamic flatCall邻接问题另列。 |
| G3.4.18 | Byte查询与拥有Package地址数组的共同来源/资源验收 | 已合入 | — | — | main@53e3e092d；保主线bounded Array resolver和全部16准入；同一程序Byte len/cap与Array叶读取四native及唯一target写冲突控制通过，97条旧N、3capture阈值及104默认栈保持。 |
| G3.4.19 | 整共享地址数组读取逐实际根核初始化 | 已合入 | — | — | main@b07b85dd4；精确源码057308f16相关API四目标690/0、snapshot/Byte组92/0、全目标strict/fmt及结构3979通过；含4配置snapshot原生。新增全量/冷恢复未跑，不等于动态下标完成。 |
| G3.4.20 | 借入泛型Package的Managed Store字段替换 | 待集成 | P0 | sol high · array_analysis_cost | f0ee4f821已获Sol精审接受；189/0、12原生、真实旧描述失效和新旧精确预算通过；root已合。只接实际可变Package的完整Store字段，不开放whole/capture。 |
| G3.4.21 | 普通拥有字段Copy观察、共享错误臂及标量入口组合 | 待集成 | P0 | sol high · integration_audit；Astra审 | 7dd7bb0eb已获精确接受并合root；真实字段/tag/init/loan及Byte入口callee证明保留，相关10过/1后端失败；后端首阻断ReadPackageField由G10.3.5补接。 |
| G3.4.22 | 存储借入程序中局部拥有ByteBuffer字段的共享字节视图 | 进行中 | P0 | sol high · byte_view_composition；Astra复核 | 已合联合基础并实测原四模块：79bba68c8仍在Sum标签分类前后耗尽默认Analyze额度（1失败，原日志保留）。正按费用分布复用不变CFG liveness；不提高Limits。独审发现raw重定位负控先触发SSA支配，已修测试待执行；生产未发现明确放行缺陷，未验收。 |

</details>

<details>
<summary>G3.4.11 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.4.11.1 | qualified Record及父Package窗口保留真实借入叶，独立两核拒冲突写入 | 已合入 | — | — | main@0e770a26e；修复&refs、&job.refs、&job及Record identity窗口实际叶/parent loan丢失；Source与独立raw/Wire真实冲突拒、分离目标合法。main旧裸Record漏洞已修；冷CLI四原样反例拒绝、四合法控制16native通过。 |

</details>

<details>
<summary>G3.5 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.5.1 | 普通闭包借入输入到封闭拥有结果 | 待集成 | P0 | sol high · borrowed_owned_callbacks | 4b9f90472已获Sol精审接受；65/0、20新原生及新旧预算/限定strict通过；root已整合，联合门待跑。普通lending与无关closure误拒另见子项。 |

<details>
<summary>G3.5.1 · 普通Direct签名与闭包推断组合</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.5.1.1 | 无关closure不阻挡普通lending调用和借入Option模式 | 待集成 | P0 | sol high · borrowed_owned_callbacks | f46254b88已精审并合root；Direct完整签名与真实函数指针ABI分流，原lending/无关closure组合及4原生通过。与泛型fee修复共同相关378/0。 |
| G3.5.1.2 | 存储借入CFG中的借入输入闭包与封闭拥有结果 | 待集成 | P0 | sol high · borrowed_owned_callbacks | 6a95603ed已获Sol精审接受并合root，生产5deaa1fc6；117所选、精确A/V预算及strict通过，59项证据hash/原四Source已独立核。完整原生仍待G3.4.22。 |

</details>

</details>

## G4 · Type / Syntax 与编译期计算

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.1 | 普通泛型Type返回、具体纯泛型记录的CT计算/反射和已有Syntax组合 | 已合入 | — | — | [已合入证据][e37]；完整上下文和值范围另列。 |
| G4.2 | CT4混合const/life/type全序号身份及原源重实体化 | 已合入 | — | — | [本批验收][e41]；CT4 全序号及原源重实体化已通过共同验收。 |
| G4.3 | CT5真实Definition上下文、上下文键、物理字节计费和phantom Code/field查询 | 已合入 | — | — | [本批验收][e41]；CT5 实际上下文、键与物理字节计费已通过共同验收。 |
| **G4.4** | **完整实例实际参数向量、成员/生成作用域、重定位与声明归属** | 已合入 | — | — | [PR46 验收][e-pr46] 本表完整 Type/Const/Life 向量、作用域/归属、跨工厂纯值及 runtime CT 相位组合已闭合；更一般载荷见 G4.5。 |
| G4.5 | rich callable、ambient Type导出和更一般可序列化值/匿名Sum/拥有载荷组合 | 部分实现 | P1 | 待派 | 依赖 G4.4、G3.3、G3.4；补齐实际消费者。 |

<details>
<summary>G4.4 · 子任务（6 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.2 | free CT factory 的完整 Type/Const/Life 实例向量与原声明身份 | 已合入 | — | — | [PR41 验收][e-pr41]；完整向量、作用域、复用深度计费及实际消费者通过。 |
| G4.4.3 | 原始非泛型宿主的静态混合参数 CT 工厂及成员删除后来源保留 | 已合入 | — | — | [PR42 验收][e-pr42]；实际原声明身份、物理复制/扫描费用、预算、语义缓存与原生消费者通过。 |
| G4.4.4 | 原始 CT factory 嵌套 Type/Const/Life 显式转发的标量结果 | 已合入 | — | — | [PR43 验收][e-pr43]；无占位结构体的3层转发、原成员嵌套调用及身份负控通过，真实向量/作用域和容量预付闭合。Phantom跨工厂结果及泛型宿主另缺。 |
| **G4.4.5** | **嵌套 CT 跨工厂返回 Phantom/Type 的实际 life 归属与重定位** | 已合入 | — | — | [PR46 验收][e-pr46] 带 life 的 Type 结果、Phantom 与递归纯字段跨工厂重构/重定位完成；保留完整原声明和实际端点。 |
| **G4.4.6** | ambient Type 实参、泛型宿主/成员及生成作用域的实际归属 | 已合入 | — | — | [PR46 验收][e-pr46] ambient Type 完整名义元数据、泛型静态宿主/成员/生成来源及递归字段消费者完成。 |
| **G4.4.7** | 运行时泛型体中的 CT 相位与完整 Type/Const/Life 实例 | 已合入 | — | — | [PR46 验收][e-pr46] runtime 泛型正规化与 CT 调度完成，真实 Call/原源/完整上下文独核和未用定义检查保留。 |

<details>
<summary>G4.4.5 · 子任务（3 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.5.1 | 跨工厂 Phantom 返回的纯标量字段复制与真实 life 端点核验 | 已合入 | — | — | [PR45 验收][e-pr45]；双方原 schema/完整实参保留，独立核真实 Call、字段复制及 Definition/ordinal 端点。完整u64/负数/小数/Unit、两life和四余额边界闭合，并修复既存Unit回写错误。 |
| G4.4.5.3 | 带life的Type值构造与跨工厂返回的声明归属 | 已合入 | — | — | [PR46 验收][e-pr46] TypeInContext 完整图、CT6 编码和 ValueContext 贯通真实 Type/Call 消费；原 life 归属独立核验。 |
| G4.4.5.4 | 非标量纯字段跨工厂返回：Tuple/Array/Record/Enum 递归重构 | 已合入 | — | — | [PR46 验收][e-pr46] Tuple/Array/Record/Enum 递归纯值和带 life 嵌套字段已到完整 u64 原生结果；旧标量费用与反例保留。 |

</details>

<details>
<summary>G4.4.6 · 子任务（3 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.6.1 | qualified Type 实参的完整名义元数据及物理字段消费者 | 已合入 | — | — | [PR46 验收][e-pr46] TypeArgument 名义行、完整实例与实际物理字段映射由 Types/Analyze/Verify 分别消费，缺关系/目录仍拒绝。 |
| G4.4.6.2 | CT 显式调用后泛型静态宿主的定义与后继消费 | 已合入 | — | — | [PR46 验收][e-pr46] 真实完成 CT 调用回执、完整原源和泛型静态宿主后继消费已闭合；定义检查、清理和预算边界通过。 |
| G4.4.6.3 | 带 life 的纯 Enum 与数组字段：完整名义关系、递归消费者及原生运行 | 已合入 | — | — | [PR46 验收][e-pr46] qualified Enum/Array 的 class 来源、递归值、large-copy 快照/参数/返回和完整 u64 原生通过；默认栈修复未增栈。Source 地址数组参数边界见 G3.4。 |

</details>

<details>
<summary>G4.4.7 · 子任务（2 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.7.1 | runtime 泛型 Type 实参的 scoped 名义正规化与实例键 | 已合入 | — | — | [PR46 验收][e-pr46] scoped 名义正规化、完整实例键与 CT Type 值消费已贯通 Source/project/native；原 Call 独核保留。 |
| G4.4.7.2 | CT 相位与原源/生成来源映射的历史组合回归 | 已合入 | — | — | [PR46 验收][e-pr46] Self/生成来源、CT 调度、原源映射及 buffer 释放次序修复通过独审和最终默认/参考全量。 |

</details>

</details>

## G5 · 动态抽象

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G5.0 | 具体Fn/FnMut/FnOnce、opaque拥有工厂及有限具体环境保存 | 已合入 | — | — | [已合入证据][e37]；仅动态抽象前置。 |
| G5.1 | dyn真实Source→Core声明/provider身份 | 已合入 | — | — | [PR52][e-pr52]及main@222e550af：实际Source/Core/provider、Types、独立两核及双后端机器派发已贯通；一般关联项/泛型slot和拥有擦除存储继续见G5.2/3。 |
| G5.2 | 异构调用、静态vtable、关联项与有限泛型method slot的真实派发 | 部分实现 | P0 | 待派 | 首批 G5.2.1 先接单共享接收者、固定方法槽和两个 provider；关联项、泛型槽与拥有擦除存储仍按各自合同后续扩展。 |
| G5.3 | 显式拥有/共同借入存储及管理route，移动/销毁/失败路径完整 | 部分实现 | P0 | sol high · dyn | `45d9c9327`限定ByteBuffer cleanup独审并合；修同frame失败栈、interface-only债、旧view及闭包捕获硬拒。真实同frame400000故障已补。userDrop/更广管理与捕获仍有缺口，不关闭父项。 |

<details>
<summary>G5.2 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G5.2.1 | 两个 provider 的单共享 dyn 接收者与固定标量方法槽 | 已合入 | — | — | [PR52 验收][e-pr52]：实际运行时双 provider、跨模块默认包装、完整 u64 实参与返回、raw/Wire/预算/缓存及三 CGU 冷暖缓存通过；双后端 O0/O2 与共同门已验收。Unit Source、拥有/可变 dyn 和一般擦除存储仍不开放。 |
| G5.2.2 | Unit/标量槽、独占接收者与跨模块默认包装 | 已合入 | — | — | `794e74a7a`已合候选；真实双provider可变写入/读回、合同反例、四机器配置和缓存专项通过，完整trait111/0； 已由main@222e550af本批有限范围验收。 |

<details>
<summary>G5.2.1 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G5.2.1.1 | Source/coercion、Types 和真实 provider 合同 | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |
| G5.2.1.2 | Analyze 实际 dyn 根与目标合同核验 | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |
| G5.2.1.3 | Verify 独立 dyn 合同与 raw/Wire | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |
| G5.2.1.4 | CT 保留真实 dyn inventory 与运行期表达式 | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |

</details>

</details>

## G6 · 状态协议

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G6.0 | 有限资源U/K/D、初始化/部分移动、Store及callable State合同 | 已合入 | — | — | [已合入证据][e37]；一般状态协议另列。 |
| G6.1 | 逐结果的一般状态转换、角色与别名规则 | 部分实现 | P0 | 待派 | [C40][c40] 已有逐 role 检查；G6.1.1 的登记三态及逐 Poll 结果已合入，一般用户协议/转换仍待补。 |
| G6.2 | 真实投影、失效关系、历史撤销和导出上界组合 | 部分实现 | P0 | sol high · graph | G6.2.1实现显式checked graph-relative ticket及真实拥有payload；ticket不是Desc/跨Graph来源证明，不因此关闭一般投影与描述图。 |
| G6.3 | 状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1 | 待派 | 依赖 G1.3、G6.2、G8.4、G8.6。 |

<details>
<summary>G6.1 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G6.1.1 | 已有登记协议的逐结果静态核验：Closing/Closed、真实 Poll 结果及受检关闭 | 已合入 | — | — | `main@e800b39d5`；逐 Poll 结果三态合同、跨 helper、旧结果反例和真实排空已接受。[共同验收][e-native-foundations]；通用用户协议、Guard 和跨线程关闭仍缺。 |

</details>

<details>
<summary>G6.2 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G6.2.1 | 运行时图、真实拥有payload、逻辑删除/显式回收/旧ticket不复活 | 已合入 | — | — | main@222e550af：普通std.graph、拥有ByteBuffer载荷、运行时环/共享边、删除/collect/旧ticket不复活与失败返还已验收；G6.2.3已补原rich消费者默认10m两核/机器，旧需50m描述停用。有限graph-relative checked ticket不代表一般Desc图。 |
| G6.2.3 | 同函数真实动态图在默认预算下完成两核与机器执行 | 已合入 | — | — | `711968341`/`5d464be44`已合：rich原Source默认10m两核及Clang/API O0/O2、fault通过，50m覆盖已删除；实际不可变type memo及稀疏活loan扫描消冗余，N−1/fresh仍验，非抬限额。 已由main@222e550af本批有限范围验收。 |
</details>

## G7 · 外部回调生命周期

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G7.1 | 安装函数返回后稳定拥有真实闭包环境的Registration、显式强关闭及失败清理 | 已合入 | — | — | [已合入证据][e37]；限同步关闭，不覆盖跨线程排空。 |
| G7.2 | 更一般借入/跨线程环境的登记对象与关闭合同 | 部分实现 | P0 | 待派 | G7.2.1已合入closed FnMut环境的跨线程准入、raw holder、no-future/drain和一次cleanup；一般借入环境及任意入口图未闭合，继续依赖G3.4、G8.5。 |
| G7.2.1 | 跨线程raw持有、FnMut准入、完整drain及一次关闭 | 已合入 | — | — | `861f18086/e4380130`独审并合：具体与generic四前提、原子发布/准入/driver、真实raw holder、完整drain与一次cleanup。closed环境/Direct入口有限，不授任意FFI/线程能力。 已由main@222e550af本批有限范围验收。 |
| G7.3 | 外部waker/晚通知、no-future及drain，关闭后所有raw入口静止 | 部分实现 | P0 | root | 同线程晚通知gate/provider排空已[合入][e42]；G7.2.1的closed跨线程raw holder/no-future/drain已由main@222e550af验收。一般借入环境和通用waker图仍待补。 |
| G7.4 | Registration与异步任务责任的失败/取消/完成组合 | 部分实现 | P0 | root | 有限 Task 失败/取消/返回时排空已[合入][e42]；更一般组合依赖 G7.2、G8.6。 |

## G8 · 异步与并发

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G8.1 | 结构化pthread、密封MutexI32/AtomicI32、稳定TaskI32帧和有限静态Suspend child | 已合入 | — | — | [已合入证据][e37]；限密封 I32 及有限静态任务。 |
| G8.2 | 静态拥有Task/child、保存SSA、Unit/有限Copy错误结果及清理 | 已合入 | — | — | [本批验收][e41]；有限静态拥有 Task/child 已通过共同验收；通用 frame port/拥有 Sum 仍另列。 |
| G8.3 | CAS41真实源码→Types→两核/Wire→机器及争用/错误次序控制 | 已合入 | — | — | [本批验收][e41]；源码 CAS、两核/Wire、原生争用与错误次序控制已通过共同验收。 |
| G8.4 | 通用PendingExit、挂起/取消排空、外部waker及晚通知 | 部分实现 | P0 | root | 有限同线程Pending清理/Task责任及closed跨线程登记已合入；有限真实Task ports见G8.6.2/3。通用PendingExit、外部waker及一般拥有/借入责任组合仍缺。 |
| G8.5 | 一般Mutex<T>/Guard、Transfer/Share、原子位宽和拥有同步构造 | 进行中 | P0 | sol high · sync | G8.5.1已合入拥有Package/Tuple/Enum/Array的Mutex<T>/affine Guard、结构化线程与更宽atomics；一般T及Guard跨挂起/线程迁移不因有限实现获得权限，继续补真实能力证明。 |
| G8.6 | 一般拥有Sum/result、借入frame port、部分责任、ticket/defer的任务组合 | 部分实现 | P0 | CT / 异步 | 已合入完整拥有Enum及内部只读借用跨挂起、多个共享/独占输入普通结果、单/多共享输入中实际来源的共享借入结果；更一般独占/嵌套结果及owner/capture运输仍未完。 |

<details>
<summary>G8.5 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G8.5.1 | 通用 Mutex<T>/affine Guard、结构化线程与必要原子位宽 | 已合入 | — | — | `e48fb3643`已合候选：多输入Guard从真实返回根选源，拥有Array/Package/Tuple/Enum、pthread共享、i64/u64/usize atomics四配置与失败清理专项通过；Guard不获跨挂起/线程权限。 已由main@222e550af本批有限范围验收。 |
| G8.5.2 | Mutex线程内部Direct helper的普通返回值 | 待集成 | P0 | sol high · integration_audit | 2f8af552f已获Sol精审接受；API/reference各4/0、旧21/0及strict通过，root已整合待共同门；根ABI/Transfer/同步cleanup保持。 |
</details>

<details>
<summary>G8.6 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G8.6.2 | 单共享 &i32 输入 Task：真实 port、工厂返回及 frame 全寿命保留 | 已合入 | — | — | `main@e800b39d5`；实际共享 &i32 port 保留至 Destroy/同步 Drop，跨模块返回、PC0/取消及独立 raw/Wire/预算通过。[验收][e-native-foundations]；其它结果、Task 输入和线程迁移未开放。 |
| G8.6.3 | 多共享/独占Task端口、普通结果族及跨helper完整上下文转交 | 已合入 | — | — | `dccc5764c`已由main@222e550af有限范围验收：多共享/独占端口、标量及有限Product referent、Task<T,Context>泛型helper，真实来源/完整上下文正反例闭合。单输入借入结果见G8.6.4；嵌套owner运输仍缺。 |
| G8.6.4 | Task借入结果：外部输入来源、真实Poll载荷及Destroy后父loan | 已合入 | — | — | `ee8f13055/b2ecb006e`已独审并合：真实local父链/普通SSA消费、无dummyheader/合法elision/非首参数/cross-module Direct，61专项及原预算通过；pub Suspend仍SABI1，后续G8.6.5已补多共享输入的实际共享来源结果；exclusive/nested结果仍未关闭。 已由main@222e550af本批有限范围验收。 |
| G8.6.5 | 至少双共享Task输入、只返回其中一条真实外部来源 | 已合入 | — | — | main@0e770a26e；多个共享Task输入中返回声明结果类的真实外部来源；frame保留全部port、Ready只取实际eligible来源，真实calloc/取消/fullcontext/Slice费用控制通过。 |

</details>

## G9 · 标准库、平台与 ZUI

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.1 | 普通字符串/Unicode与事务编辑、Store插入移除、有限回调迭代/Option返回 | 已合入 | — | — | [已合入证据][e37]；完整库见 G9.3。 |
| G9.2 | 整数布局、RGBA软件合成、IME preedit提交/取消及失败保留的普通库 | 已合入 | — | — | [实现证据][ui] / [独审][ui-review]；不覆盖 OS IME、字体及 GPU。 |
| G9.3 | 完整字符串/集合与一般Iterator/lending组合 | 部分实现 | P0 | 待派 | G9.3.1 与 G3.4.1 已交付普通 UTF8 入口与共享借入迭代首片。G9.3.2/G9.3.3/G9.3.4 已交付普通 POD Record/九种标量动态集合与元素可变借用；G9.3.5 已交付新 Store 族的有限普通拥有组合；grapheme/格式化等仍待补，现有 StoreElement 不等于一般值容器。 |
| G9.4 | 真实平台窗口、事件循环及OS IME服务 | 部分实现 | P2 | 待派 | 已有旧 Win32 首窗/消息实验，本轮 Win64 Verify 通过；统一平台、OS IME 未接。窗口/事件层供 G9.5.1 复用，旧 GDI 不进入 ZUI 渲染路线；原生 Windows 仍待实际执行。 |
| G9.5 | GPUI 式 GPU 场景/呈现、字体塑形与字形图集 | 待执行 | P2 | 待派 | 后续 G9.5.1 接 GPU 首帧；D3D11/DXGI/可选 DirectComposition 与 DirectWrite 为已核 GPUI Windows 参考。GPU ABI、设备完成、字体/atlas 与真实输入链路仍待实现。 |
| G9.6 | GPUI 式 ZUI 应用、实体/组件、输入及资源关闭合流 | 待执行 | P2 | 待派 | G9.6.1 核查现状；G9.6.2 接首个 GPU 应用。第一帧不代表开放组件、文字/IME、可访问性、异步与完整 ZUI 已完成。 |

<details>
<summary>G9.3 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.1 | 普通 UTF8 文本字面量到只读文本与 String 的入口 | 已合入 | — | — | [PR48 验收][e-pr48]：静态UTF8、显式来源、Unicode/空串/NUL、跨模块返回、显式String分配失败清理及预算闭环；完整Unicode/格式化库另缺。 |
| G9.3.2 | 普通 POD 动态集合与真实借入视图 | 已合入 | — | — | [PR49 验收][e-pr49]：普通 Record 的 std.store、reserve/push/pop/shared view 和大 Copy 协议；默认5124/0、参考5012/0、文本70/0及工程门通过。原262144字节源码默认栈 O0/O2通过；一般元素/可变片/拥有组合仍另缺。 |
| G9.3.3 | 普通 POD 集合的可变元素借用：实际写入、helper 重借用与结构修改互斥 | 已合入 | — | — | [PR51 验收][e-pr51]：真实 std.store 可变元素与 helper 重借用、写后读回、失败保留、两核 raw/Wire 和双后端 O0/O2通过；保留整 Store 排他，不推断不同下标不交。 |
| G9.3.4 | 标量动态序列：直接 Store<i32/u64/usize/bool/浮点等既有标量> 与真实 std.store | 已合入 | — | — | [PR51 验收][e-pr51]：bool/u8/u16/i32/i64/u64/usize/f32/f64 直接序列与真实 std.store、完整 u64 和双后端通过；Unit、一般拥有/捕获及 standalone Array/Tuple/Enum 仍排除。 |
| G9.3.5 | POD/标量 Store 的普通拥有数据模型：字段、整值流、替换与自含捕获 | 已合入 | — | — | [PR52 验收][e-pr52]：精确源码 `498f0f5a1` 默认5248/0、参考5136/0、文本70/0；17 Source/68 原生、独审、release/10工程门和远端重建运行通过，远端 main `c2ae6edb9` 已核。仅本表有限拥有组合，首轮5242/6与历史缺件状态保留。 |
| G9.3.6 | 基础String查询、排序、文件/流I/O与真正可封装的Map/Set | 进行中 | P0 | sol high · stdlib | `c116bd65f`：UTF8查询/整数转换/排序/文件流、拥有StringMap、固定容量POD回调HashMap、名义i64 Map/Set已合候选；真实私有Key、失败返key/删除故障在四配置通过。完整owning K/V已由G9.3.9交付；自动增长泛型哈希/通用Set与迭代适配仍缺。 |
| G9.3.9 | 真正按key的owning K/V Map、替换/取出及失败责任 | 已合入 | — | — | 原`b3f3888f2`实际KeyEq/替换/取出/分配失败责任保持，`f3b600238`加入已准入V的Option借入get及helper，actual manager源/强pin/错误key来源已独审； 已由main@222e550af本批有限范围验收。 |
| G9.3.10 | 拥有/借入迭代消费者及map/filter/collect适配 | 待排期 | P1 | 待派 | 依赖G2.4.11/G2.4.12/G3.5.1；先用真实普通库区分语言门与库实现，不重做已有LendingIterator。 |
| G9.3.11 | 自动增长泛型HashMap及通用Set | 部分实现 | P1 | sol high · collections | POD自动增长/Copy Set源码已备份；G3.4.20修复后暴露非local callable普通参数Commit效应缺口，子项继续阻塞。owning K/V哈希未排，已有线性OwningMap可用。 |
| G9.3.12 | 无符号/浮点文本转换与缓冲I/O | 待排期 | P1 | 待派 | 复用已有String、文件流、i64解析/格式化；覆盖范围/舍入/短读写及失败清理。 |

<details>
<summary>G9.3.11 · 自动增长集合首批</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.11.1 | POD泛型HashMap自动增长/rehash和通用Copy Set | 进行中 | P1 | sol high · growing_collections | 接续c42c06111真实Map/Set并合当前联合基础；正以完整普通Input may-replace/retire与callback capture分开映射修调用效果，保失败留旧值且不授must/Fresh。不改库绕过、不扩schema；Source/raw/Wire和rehash分配失败待验证。 |

</details>

<details>
<summary>G9.3.5 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.5.1 | Source/Types 完整拥有组合及 Store 机器清理 | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |
| G9.3.5.2 | Analyze 实际根、责任和捕获独立核验 | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |
| G9.3.5.3 | Verify 独立责任、窗口与 raw/Wire | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |
| G9.3.5.4 | 原样数据模型、捕获、失败分配及最近反例消费者 | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |
| G9.3.5.5 | 拥有 Tuple 模式的实际消费分类与 FnOnce 捕获 | 已合入 | — | — | [PR52 验收][e-pr52] 随父有限合同通过；源码、独立核、原生及共同门均已核回远端 main。 |

</details>

<details>
<summary>G9.3.3 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.3.1 | Types 完整元素准入及 LLVM 实际写入 | 已合入 | — | — | [PR51][e-pr51]：完整 Record 类型/布局、可变 Address 与大 Copy 读写协议通过。 |
| G9.3.3.2 | Analyze 实际根、父loan和结构修改互斥 | 已合入 | — | — | [PR51][e-pr51]：真实根/父loan、结构修改互斥及旧费用通过。 |
| G9.3.3.3 | Verify 独立准入、实际窗口和 raw/Wire | 已合入 | — | — | [PR51][e-pr51]：独立实际窗口、stale Hold 与真实重叠 raw/Wire 反例通过。 |

</details>

<details>
<summary>G9.3.4 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.4.1 | Source/Types 完整标量准入与 LLVM 计划 | 已合入 | — | — | [PR51][e-pr51]：九种标量完整准入、不同 canonical 身份与 LLVM 精确计划通过。 |
| G9.3.4.2 | Analyze 标量载体与实际窗口 | 已合入 | — | — | [PR51][e-pr51]：独立 canonical 载体、完整标量/实际窗口及精确预算通过。 |
| G9.3.4.3 | Verify 标量协议与 raw/Wire | 已合入 | — | — | [PR51][e-pr51]：独立标量协议、raw/Wire、owning/capture 排除与旧费用通过。 |

</details>

<details>
<summary>G9.3.2 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.2.1 | 普通 POD 与 StoreElement 模板、Source/Types 接入 | 已合入 | — | — | [PR49 验收][e-pr49]：Source/Types 与实际 std.store 入口、完整元素资格和未用定义反例通过。 |
| G9.3.2.2 | Analyze 独立准入及实际 Store 借用窗口 | 已合入 | — | — | [PR49 验收][e-pr49]：Analyze 独立准入、实际共享窗口及预算内收敛通过；旧 Package 费用保持。 |
| G9.3.2.3 | Verify 独立准入及候选复核 | 已合入 | — | — | [PR49 验收][e-pr49]：Verify 独立准入、raw/Wire/真实失效与预付索引通过。 |
| G9.3.2.4 | LLVM 完整 Copy 协议与大元素内存路径 | 已合入 | — | — | [PR49 验收][e-pr49]：完整 Copy 协议、一次释放及原大值默认栈 O0/O2通过；保守栈槽复用已独审。 |

</details>

</details>

<details>
<summary>G9.5 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.5.1 | 窗口/事件、固定 POD scene、GPU draw/present 与 resize/关闭 | 待排期 | P2 | 待派 | 通用语言批次之后；依 G10.3.1 冻结平台接口，必须真实 GPU 首帧及输入后的帧变化。现有 ui_raster 仅普通库/参考，不作 ZUI 渲染；文字/atlas 后续接续。 |

</details>

<details>
<summary>G9.6 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.6.1 | ZUI 当前语法/语言缺口核对与 GPUI 首批应用合同 | 已合入 | — | — | [PR47 核查报告][zui-audit]；26 项 Source 观察及 3 项既有 UI 内核原生复跑，GPU ABI 仅形状 Verify，未执行 GPU。已明确不采用 GDI/CPU raster 替代路线；语言补片尚待实施。 |
| G9.6.2 | GPUI 式 GPU 首片应用的输入、状态、场景与关闭合流 | 待排期 | P2 | root · 集成 | GUI 批次后置，接 G9.5.1；真实设备/呈现和失败关闭须实跑，静态首帧可先于开放组件；后续文字/IME/实体删除与异步责任仍须补齐。 |

</details>

## G10 · 可靠性、规范与产品验证

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.0 | 原大数组O0崩溃、默认2MiB API/深调用栈可靠性及普通大型按值ABI | 已合入 | — | — | [已合入证据][e37]；原 O0 崩溃已修复，更广 ABI 见 G10.3。 |
| **G10.1** | 精确源码独审、受影响组合/既有正式门、修正与合入裁决 | 已合入 | — | — | [PR46 验收][e-pr46] 当前 P0 批次已合入远端 main；只验本表有限范围，不代表全部语言、B0、ZUI 或未测产品条件完成。 |
| G10.2 | 有限f32/f64与UInt组合、匿名tagged Copy源Sum及CT Type结构Sum | 已合入 | — | — | [已合入证据][e37]；CT Sum 值与拥有载荷见 G4.5、G10.3。 |
| G10.3 | 大型closure/helper/foreign ABI及浮点/匿名联合责任的一般组合 | 部分实现 | P0 | 待派 | 补齐实际源码及最近反例；大数组修复不能覆盖全部 ABI。 |
| G10.4 | 原生Windows执行和平台差异验证 | 待执行 | P2 | 待派 | 需要原生 Windows 主机；环境可提前准备。 |
| G10.5 | 与Rust同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破 | 待执行 | P1 | root | 固定真实消费者、同语义 Rust 对照和独占计时窗口。 |
| G10.6 | B0、完整语言与ZUI终态验收 | 待执行 | P2 | root | 依赖全部必需语言任务、G9.6、G10.4、G10.5。 |
| G10.7 | 全仓语义命名、主树资产归位与独立 goal 施工协调中心 | 已合入 | — | — | [本轮仓库组织收据](records/repository-organization.json)绑定远端 main `2b603607dacf07273ccc3b741f309a29ab7e33b9`、完整消费者/路径门、适用独审和远端读回；[PR54][e-pr54] 仅是已被取代的旧布局历史，不作为本轮验收。语言及产品状态不因目录整理改变。 |

<details>
<summary>G10.7 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.7.1 | 文档入口、工作流与长期决定的语义归档 | 已合入 | — | — | [PR53 验收][e-pr53] 随首批完成静态义务、来源/字节映射、独审与真实消费者；源过程保留在 Git 历史。 |
| G10.7.2 | 正式工程合同迁出与治理工具解耦 | 已合入 | — | — | [PR53 验收][e-pr53] 随首批完成静态义务、来源/字节映射、独审与真实消费者；源过程保留在 Git 历史。 |
| G10.7.3 | 工单中的真实 Source/Rust 输入与历史收据追溯 | 已合入 | — | — | [PR53 验收][e-pr53] 随首批完成静态义务、来源/字节映射、独审与真实消费者；源过程保留在 Git 历史。 |
| G10.7.4 | 源码、测试、示例、工具及规范提案的语义身份迁移 | 已合入 | — | — | 按语义能力迁移真实消费者闭包；Source/正反例、选择和断言、静态义务、清理及预算条件保留。精确远端 main `2b603607dacf07273ccc3b741f309a29ab7e33b9` 与[仓库组织收据](records/repository-organization.json)绑定，旧文件由固定 archive 历史恢复。 |
| G10.7.5 | 上一轮文档重组方案 | 已被取代 | — | — | [PR54][e-pr54] 记录上一轮实际合入与当时检查；用户已否定其主树过程内容分界。本轮由 .6 重做，旧收据只说明旧源码范围，不作为当前方案验收。 |
| G10.7.6 | 全仓语义命名与 main/goal 协调边界：主树仅留语言权威、阶段合同、真实输入及必要构建指引，goal 统管任务和施工流程 | 已合入 | — | — | main 七页文档与具名规范/验证资产、goal 唯一看板/工作流/精简记录完成分界；完整路径、链接、工具、CI、独审及远端 SHA/blob 核验见[仓库组织收据](records/repository-organization.json)。不合并 goal 到 main，不产生新语言功能验收。 |

</details>

<details>
<summary>G10.6 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.6.1 | 语言初稿开发预览：通用必备组合、构建文档与同批验收；不替代 B0/完整语言门 | 已合入 | — | — | [PR51 验收][e-pr51]：精确源码 `a8c7953e7` 默认5190/0、参考5078/0、文本70/0；双严格检查、release、10项工程门及远端源码恢复构建运行通过。有限预览不替代 B0/完整语言。 |
| G10.6.2 | 通用 Native 库与异步资源联合验收：精确源码、真实消费者、独审和远端恢复 | 已合入 | — | — | `main@e800b39d5`：默认5269/0、参考5157/0、文本70/0；真实联合双后端 O0/O2、3735文件远端恢复/空缓存构建、16次 API 构建/128次运行/56次故障注入均通过。[本批记录][e-native-foundations]。 |
| G10.6.3 | 通用语言1.0功能门、真实普通程序、工程使用与精确源码交付 | 进行中 | P0 | root · 集成 | 北京时间最后施工 2026-11-09 09:16；远端续工2a50a8bb8保留原四模块和48次原生矩阵。工具链Rust1.99.1/LLVM23.1.4恢复；原程序Analyze默认预算已实测耗尽，正修真实重复工作。待共同验收，未宣称功能1.0。 |
| G10.6.4 | 全量回归中的旧Store/实际callback Package准入 | 已合入 | — | — | `6c1ac6fdf`已合候选；恢复旧flat callback/Store真实root快路，保留新borrowed Product分类，84项与原CLI/window正反例专项通过。 已由main@222e550af本批有限范围验收。 |
| G10.6.5 | 新共享构造检查的两核实际独占loan发现与旧fee保持 | 已合入 | — | — | `e5c52c936/396c183c`已合：复用付费建表，无额外扫描，原两预算控制恢复原值、28独占构造/slot控制通过。 已由main@222e550af本批有限范围验收。 |
| G10.6.6 | 默认worker深调用与life组合栈溢出 | 已合入 | — | — | 既有递归帧修复由main@222e550af验收；source_life合流回归的子项也已由main@0e770a26e交付。原默认栈/深调用/费用控制保留。 |

</details>



<details>
<summary>G10.6.6 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.6.6.1 | 普通trait/lending合流后source life深调用保持默认线程栈 | 已合入 | — | — | main@0e770a26e；source_life递归帧13880→1832B；原104默认worker请求和回归通过，未增栈、Limits或弱化断言。 |
| G10.6.7 | 拥有lending、trait拥有返回与借入回调的同CFG联合消费者 | 进行中 | P0 | root · 联合验收 | 原4095e7431四模块及48原生矩阵保留，接续当前联合候选与G3.4.22最新远端；先复验Byte视图和实际分析成本，再核双后端/失败清理，不以近控替代完整程序。 |

</details>


<details>
<summary>G10.6.3 · 项目使用闭环</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.6.3.1 | 项目清单、可复现路径依赖/锁定及真实 zil test | 已合入 | — | — | `55a773aac`：清单/依赖锁、per-importer同名helper及CT/热缓存、真实Unit/Bool/I32测试已9/9；原始SourceUnit与有界扫描保留， 已由main@222e550af本批有限范围验收。 |

</details>

<details>
<summary>G10.3 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.3.1 | GPU 平台 ABI 路线、实际消费者与 CPU/GPU 完成合同 | 待排期 | P2 | 待派 | 通用语言批次之后；已有 out-handle/固定 POD 签名通过 Verify。先具名 changes/独审并接真实 native adapter；若选纯 ZIL COM 再补外部方法/指针能力，不全面放宽 c_handle。 |
| G10.3.2 | 正常规模共享地址数组的独立两核分析成本 | 待集成 | P0 | sol high · array_analysis_cost | 19d929e03已精审接受；A205/V459、35专项与4个512原生通过，精确N4170159；root已整合。大型地址Array Package仍分析受限；一般大型拥有值物理ABI见G10.3.3。 |
| G10.3.3 | 大型拥有Package物理snapshot和内部参数/返回ABI | 待集成 | P0 | sol high · large_owned_value_abi | 424791b53已精审接受并合root；239/0与16原生验证16KiB字段、移动/refill/固定栈循环，不授Copy。大型Closure三种签名仍Codegen Unsupported；地址Array Package分析边界保留。 |
| G10.3.4 | 拥有借入与ErrorUnion组合的独立资源分析成本 | 待集成 | P0 | sol high · resource_sum_cost | 5cf970ba5已获Sol精审并合root；完整unknown/缺失alternative、循环固定点与最终检查保持，旧核预算不变。A实际CFG调度及V独立工作量修复，原Source默认额度过相关阶段。 |
| G10.3.5 | 后端普通Package字段及Byte/Error消费者完整类型预付 | 待集成 | P0 | sol high · large_owned_value_abi | 9f91ccc5e已精审并合root；完整actual type预付预算7/0及strict过，原四模块和ErrorArm两后端O0/O2共8原生成功。更大pipeline仍前端阻断，不冒称共同门完成。 |
| G10.3.6 | 普通Closure的大型拥有参数/结果内部ABI | 进行中 | P0 | sol high · large_closure_abi；Astra复核 | 远端checkpoint57df31aa7已保全env/out/copy-in；后续本地680af78d1普通local Closure三签名、借入循环与失败清理28原生通过，旧大Package回归通过。factory/泛型helper另有Types边界，首失败保留，边界与小ABI测试待收尾；未共同验收。 |

</details>

<details>
<summary>G10.1 · 当前联合验收</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.1.11 | 剩余 P0 的共同验收、回归修复与合入 | 已合入 | — | — | [PR46 验收][e-pr46] 精确源码 `953913782`：默认 **5007/0**、参考 **4895/0**、文本 **70/0**，两配置严格 Clippy、release 消费者与 10 项工程检查全过；远端 main `adbead51` 已核。Actions 因付款/额度未启动，原失败保留。 |
| G10.1.12 | 通用文本、泛型方法与借入迭代共同验收 | 已合入 | — | — | [PR48 验收][e-pr48]：默认5071/0、参考4959/0、文本70/0、双严格检查、release原生及10项工程门通过；首轮5项旧回退已修复且保留失败记录。 |

</details>

[design]: https://github.com/zil-lang/zil/blob/28b3e37bb4d8a1fe8b146c72cf891fc99de32440/_zil_design.md
[gates]: https://github.com/zil-lang/zil/blob/adbead51fc8916ac1cb1019702d906a19ebd3e98/validation/product-gates.json
[c39]: https://github.com/zil-lang/zil/tree/c15b77847502d02acc83474bf7f9a8789186a3ac
[c40]: https://github.com/zil-lang/zil/tree/7dfc6846194643e5295a7704a0b8ce99fa5176e4
[e37]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/terminal-root37-integration-20261104.json
[task40]: https://github.com/zil-lang/zil/blob/c15b77847502d02acc83474bf7f9a8789186a3ac/docs/validation/review343-task40-classifier-carrier-fee-correction.json
[ui]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/terminal-345-ui.json
[ui-review]: https://github.com/zil-lang/zil/blob/4bc1d77b5ec3bccb5b789fd813e85386f03ae13f/docs/validation/review345-ui-terminal-architecture.json
[e41]: https://github.com/zil-lang/zil/blob/ace4e70eb10fc86490da7e4c51fd1441b4a3d384/docs/validation/G10.1-common41-acceptance.json
[e42]: https://github.com/zil-lang/zil/blob/b5685e2adb7462cdbaf16d6e4aabc4041e58e2d3/docs/validation/G8.4.2-common42-acceptance.json
[e39]: https://github.com/zil-lang/zil/blob/6d4d3e1182001290f87cc0105f9c5a64d769b153/docs/validation/G2.3.2-common-acceptance.json

[e40]: https://github.com/zil-lang/zil/blob/e7247e781d49ab569de263d142f72546f53470d3/docs/validation/G3.3.2-common-acceptance.json

[p41]: https://github.com/zil-lang/zil/pull/41

[e-pr41]: https://github.com/zil-lang/zil/blob/88cb9277932e910b26b97084e9c42dd7a0d1e234/docs/validation/G4.4.2.1-common-acceptance.json

[p42]: https://github.com/zil-lang/zil/pull/42

[e-pr42]: https://github.com/zil-lang/zil/blob/fcdea036500945b51991b044a00bbbcc8578ade7/docs/validation/G1.3.5.2-common-acceptance.json

[e-pr43]: https://github.com/zil-lang/zil/blob/917971a13a1590ee912db73be7474d0cceef2b2c/docs/validation/G1.3.7.3-common-acceptance.json

[e-pr44]: https://github.com/zil-lang/zil/blob/a9c5075fed796560cfa30e74872ae57eb0ca8fec/docs/validation/G1.3.8.3-common-acceptance.json

[e-pr45]: https://github.com/zil-lang/zil/blob/fd261b22c8a05c5cd1e7bc9cc05d91eeb0f12c95/docs/validation/G4.4.5.1-common-acceptance.json

[e-pr46]: https://github.com/zil-lang/zil/blob/adbead51fc8916ac1cb1019702d906a19ebd3e98/docs/validation/G10.1.11-common-acceptance.json

[zui-audit]: https://github.com/zil-lang/zil/blob/c63001c8ec43909d2c41ca314c32e465f531711f/docs/ZUI-READINESS-20261106.md

[e-pr48]: https://github.com/zil-lang/zil/blob/4550ec4083706f67771229a5213f19d885402e57/docs/validation/G10.1.12-common-acceptance.json

[e-pr49]: https://github.com/zil-lang/zil/blob/80c197b724c6cab538fbec7bf7fa526aaa877c5f/docs/validation/G9.3.2-common-acceptance.json
[e-pr50]: https://github.com/zil-lang/zil/blob/bb9d113455d3f940cfa2fe237347039f1e241007/docs/validation/G2.4.1-common-acceptance.json

[e-pr51]: https://github.com/zil-lang/zil/blob/15b9e09d83aeea9be1cc9ea4b4bdd9f5a5f9fa71/docs/validation/G10.6.1-common-acceptance.json

[task-g935]: https://github.com/zil-lang/zil/blob/f07b9a22c348ab6a9709d0be805cca114134582b/.task/G9.3.5.md

[task-g521]: https://github.com/zil-lang/zil/blob/b8048c021b6bd45ffb152d77812c547c96b54529/.task/G5.2.1.md
[review-chat9]: https://github.com/zil-lang/zil/blob/f07b9a22c348ab6a9709d0be805cca114134582b/docs/reviews/CHAT9-REVIEW.md

[e-pr52]: https://github.com/zil-lang/zil/blob/c2ae6edb94f56ac710b7a66a23db03c451193cc8/docs/validation/ordinary-ownership-and-shared-dispatch-acceptance.json

[e-pr53]: https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/repository-organization-acceptance.json

[e-pr54]: https://github.com/zil-lang/zil/blob/28b3e37bb4d8a1fe8b146c72cf891fc99de32440/validation/results/documentation-layout/acceptance.json

[e-native-foundations]: https://github.com/zil-lang/zil/blob/goal/records/native-language-composition.json
