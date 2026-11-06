# ZIL 任务看板

更新：2026-11-07（Asia/Shanghai）。本文件是唯一实时任务状态；只记录下表有限范围，不代表完整语言、B0 或 ZUI 已完成。规范与门槛以任务源码分支的[设计][design]、具名附件和[产品条件][gates]为准。

已合入＝所列范围已进入远端 main；实现待验＝已有实现、尚未接受；部分实现＝完整链路未闭合；进行中＝正在实现或验证、尚未验收；待执行/待排期＝尚未执行/派定。待办优先级仅 P0/P1/P2，已交付行用“—”。任务 ID 不带前导零。

[源码与恢复参考](README.md) · [分支、更新与备份规则](AGENTS.md) · [精简前完整记录](https://github.com/zil-lang/zil/blob/fc3dff269a882ef2850c4f9dba2acf6031ac4439/goal.md)

按能力分类，主表显示各项任务；子任务放在所属任务下的独立子表，默认折叠。展开后仍保留范围、状态、优先级、负责人和进展列，不使用四级标题。

父任务汇总完整范围，子任务记录具体交付，父子不重复计数；子项全完成不自动代表父项完成。需要独立派工、复核或验收的补充工作才新增任务编号，沿用父编号加一段；零散修正记在原任务进展中。编号同时用于分支与实施说明，不再另设勾稽编号。

**当前主线：通用语言完整性与 Rust 竞争力优先。** G9.3.2 普通 POD 动态集合与原大值 O0 修复已由 [PR49 验收][e-pr49] 合入；G3.4.1 共享借入迭代、G9.3.1 普通 UTF8 入口已交付。G2.4.1 的普通 trait 逐调用共享 lending 已由 [PR50 验收][e-pr50] 合入；下一批 P0 是 G9.3.3 普通集合可变元素与 G2.4.2 trait 静态关联函数，再推进 dyn/显式存储及通用异步责任。GPU/GUI 专用 G10.3.1、G9.5.1、G9.6.2 后置；ZUI 保持 GPUI/GPU 方向。实现与常规独审采用 sol high，疑难问题才交 Astra。

## G1 · 基础类型与模式

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G1.1 | 有限Option、记录式枚举、usize/u64原生及CT链、拥有Tuple和混合Tuple/Enum部分移动 | 已合入 | — | — | [已合入证据][e37]；一般模式组合另列。 |
| G1.2 | 恢复Task集成误删的Option拥有载荷路径 | 已合入 | — | — | [本批验收][e41]；有限拥有 Option/Task 集成恢复已通过共同验收。 |
| **G1.3** | **拥有字段替换：Verify责任转交、真实loan/pin、机器old-drop/store及失败清理闭环** | 已合入 | — | — | [PR46 验收][e-pr46] 补齐本表拥有替换组合：真实来源、loan/pin、责任、退休和失败清理；更一般借用/模式仍见 G1.4、G3.4。 |
| G1.4 | 更一般的地址/Package/ErrorUnion解构、混合模式及部分补齐组合 | 部分实现 | P1 | 待派 | 已有 `.ok/.err` 路径；按具体模式接 G1.3、G3.4。 |

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

## G2 · 泛型与 trait

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.1 | 有限const、supertrait、泛型trait/impl、关联类型/常量、默认体及泛型接收者方法 | 已合入 | — | — | [已合入证据][e37]；一般约束方程另列。 |
| G2.2 | 混合life/F helper、独立Invocation关系与未用声明检查，贯通两核及原生 | 已合入 | — | — | [本批验收][e41]；混合 life/F helper、独立 Invocation 与未用声明检查已通过共同验收。 |
| G2.3 | 一般关联投影、类型/Self依赖的常量表达式、约束蕴含和循环诊断 | 部分实现 | P1 | 待派 | G2.3.1 有限 Self 依赖常量已由 PR39 [合入][e39]；一般投影、泛型及 Drop 组合仍缺。 |
| G2.4 | 更一般life/lending、符号泛型方法和静态关联函数；一般/blanket/primitive/enum impl及更广supertrait实参组合 | 部分实现 | P1 | 待派 | G2.4.1 的有限普通 trait 共享 lending 已合入；G2.4.2 静态关联函数为下一批 P0，其余依赖 G2.3、G3.4，保留定义期义务。 |

<details>
<summary>G2.4 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.1 | 普通 trait 逐调用 lending iterator：关联 Item、方法 life 与真实接收者来源 | 已合入 | — | — | [PR50 验收][e-pr50]：跨模块关联 Item、方法 life、真实 std.iter、两后端 O0/O2；默认5146/0、参考5034/0。原 SliceIter/Option 转发回归已修复；资源宿主/一般 owning lending 仍缺。 |
| G2.4.2 | 普通 trait 静态关联函数：类型限定调用、符号泛型宿主、既有 type-only 参数与返回组合 | 实现待验 | P0 | static_traits · sol high | 实际源码已复现具体宿主“generic method without receiver”和符号宿主“unknown value F”；沿用现有泛型义务与静态调用，不限特定工厂名称。 |

<details>
<summary>G2.4.2 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.4.2.1 | 符号类型宿主、定义期义务与词法值优先 | 实现待验 | P0 | static_check · sol high | 与静态 provider 选择共享明确私有接口。 |

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

## G3 · 借用、拥有与生命周期

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.1 | 普通/嵌套Product借给可变helper、CopyT+mutable life、有限FP/HRTB和实际来源检查 | 已合入 | — | — | [已合入证据][e37]；一般借用返回见 G3.4。 |
| G3.2 | shared stored View、qualified静态Code及捕获为空的Fn，每个实际role独立核验 | 已合入 | — | — | [本批验收][e41]；真实静态 Code、空捕获 Fn 与逐 role 检查已通过共同验收。 |
| **G3.3** | **捕获的外层Fixed上下文、qualified FnMut/FnOnce的真实环境与调用关系** | 已合入 | — | — | [PR46 验收][e-pr46] 本表 Fixed/Invocation、闭包 helper、循环及多 role 组合已闭合；普通闭包 Value 的 give 按规范拒绝，一般 lending 继续见 G3.4。 |
| G3.4 | 普通借入字段、存储上界、affine借用、合法lending结果及长期组合 | 部分实现 | P1 | 待派 | shared View/片与短借入 getter 已通；尚缺 mutable holder、借入＋拥有 Package、resource 宿主关联 Item/lending、一般 lending 和 Source 地址数组参数；原 SliceIter 的共享借入迭代首片已由 G3.4.1 交付。 |
| G3.5 | 一般拥有/借入捕获及责任组合的消费者矩阵 | 待排期 | P1 | 待派 | 依赖 G3.4、G6.1；按真实 API 补齐。 |

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
| G5.1 | dyn真实Source→Core声明/provider身份 | 部分实现 | P1 | 待派 | [C39][c39] / [C40][c40] 已有源码/Core 声明与 provider 目录；当前 Types 仍拒绝未接通的动态消费者，不能当作已有机器派发。 |
| G5.2 | 异构调用、静态vtable、关联项与有限泛型method slot的真实派发 | 部分实现 | P1 | 待派 | 首批 G5.2.1 先接单共享接收者、固定方法槽和两个 provider；关联项、泛型槽与拥有擦除存储仍按各自合同后续扩展。 |
| G5.3 | 显式拥有/共同借入存储及管理route，移动/销毁/失败路径完整 | 部分实现 | P1 | 待派 | 依赖 G3.4、G1.3、G5.2。 |

<details>
<summary>G5.2 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G5.2.1 | 两个 provider 的单共享 dyn 接收者与固定标量方法槽 | 待执行 | P1 | 待派 | 通用抽象后续批次；贯通真实接收者/入口身份、两核和机器结果，保留错 provider/slot 反例；不先开放拥有擦除存储。 |

</details>

## G6 · 状态协议

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G6.0 | 有限资源U/K/D、初始化/部分移动、Store及callable State合同 | 已合入 | — | — | [已合入证据][e37]；一般状态协议另列。 |
| G6.1 | 逐结果的一般状态转换、角色与别名规则 | 部分实现 | P1 | 待派 | [C40][c40] 已有逐 role 检查；一般转换待补。 |
| G6.2 | 真实投影、失效关系、历史撤销和导出上界组合 | 部分实现 | P1 | 待派 | 依赖 G6.1、G3.4。 |
| G6.3 | 状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1 | 待派 | 依赖 G1.3、G6.2、G8.4、G8.6。 |

## G7 · 外部回调生命周期

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G7.1 | 安装函数返回后稳定拥有真实闭包环境的Registration、显式强关闭及失败清理 | 已合入 | — | — | [已合入证据][e37]；限同步关闭，不覆盖跨线程排空。 |
| G7.2 | 更一般借入/跨线程环境的登记对象与关闭合同 | 待排期 | P1 | 待派 | 依赖 G3.4、G8.5。 |
| G7.3 | 外部waker/晚通知、no-future及drain，关闭后所有raw入口静止 | 部分实现 | P1 | root | 同线程稳定拥有环境、晚通知 gate 与 provider 排空已[合入][e42]；一般跨线程环境待补。 |
| G7.4 | Registration与异步任务责任的失败/取消/完成组合 | 部分实现 | P1 | root | 有限 Task 失败/取消/返回时排空已[合入][e42]；更一般组合依赖 G7.2、G8.6。 |

## G8 · 异步与并发

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G8.1 | 结构化pthread、密封MutexI32/AtomicI32、稳定TaskI32帧和有限静态Suspend child | 已合入 | — | — | [已合入证据][e37]；限密封 I32 及有限静态任务。 |
| G8.2 | 静态拥有Task/child、保存SSA、Unit/有限Copy错误结果及清理 | 已合入 | — | — | [本批验收][e41]；有限静态拥有 Task/child 已通过共同验收；通用 frame port/拥有 Sum 仍另列。 |
| G8.3 | CAS41真实源码→Types→两核/Wire→机器及争用/错误次序控制 | 已合入 | — | — | [本批验收][e41]；源码 CAS、两核/Wire、原生争用与错误次序控制已通过共同验收。 |
| G8.4 | 通用PendingExit、挂起/取消排空、外部waker及晚通知 | 部分实现 | P1 | root | 有限同线程登记、Pending 清理及 Task 责任已由 PR37 [合入][e42]；跨线程、借入 frame port、一般拥有组合仍缺。 |
| G8.5 | 一般Mutex<T>/Guard、Transfer/Share、原子位宽和拥有同步构造 | 待排期 | P1 | 待派 | 依赖 G3.4、G6.1；从密封 I32 扩展到一般拥有类型。 |
| G8.6 | 一般拥有Sum/result、借入frame port、部分责任、ticket/defer的任务组合 | 部分实现 | P1 | CT / 异步 | G8.6.1 完整拥有 Enum 与内部只读借用跨挂起已由[PR41合入][e-pr41]，包含实际返回值、稳定帧存储与完成/取消清理。一般拥有结果、借入 port 与部分责任另待补。 |

## G9 · 标准库、平台与 ZUI

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.1 | 普通字符串/Unicode与事务编辑、Store插入移除、有限回调迭代/Option返回 | 已合入 | — | — | [已合入证据][e37]；完整库见 G9.3。 |
| G9.2 | 整数布局、RGBA软件合成、IME preedit提交/取消及失败保留的普通库 | 已合入 | — | — | [实现证据][ui] / [独审][ui-review]；不覆盖 OS IME、字体及 GPU。 |
| G9.3 | 完整字符串/集合与一般Iterator/lending组合 | 部分实现 | P1 | 待派 | G9.3.1 与 G3.4.1 已交付普通 UTF8 入口与共享借入迭代首片。G9.3.2 已交付普通 POD Record 动态集合；grapheme/格式化等仍待补，现有 StoreElement 不等于一般值容器。 |
| G9.4 | 真实平台窗口、事件循环及OS IME服务 | 部分实现 | P2 | 待派 | 已有旧 Win32 首窗/消息实验，本轮 Win64 Verify 通过；统一平台、OS IME 未接。窗口/事件层供 G9.5.1 复用，旧 GDI 不进入 ZUI 渲染路线；原生 Windows 仍待实际执行。 |
| G9.5 | GPUI 式 GPU 场景/呈现、字体塑形与字形图集 | 待执行 | P2 | 待派 | 后续 G9.5.1 接 GPU 首帧；D3D11/DXGI/可选 DirectComposition 与 DirectWrite 为已核 GPUI Windows 参考。GPU ABI、设备完成、字体/atlas 与真实输入链路仍待实现。 |
| G9.6 | GPUI 式 ZUI 应用、实体/组件、输入及资源关闭合流 | 待执行 | P2 | 待派 | G9.6.1 核查现状；G9.6.2 接首个 GPU 应用。第一帧不代表开放组件、文字/IME、可访问性、异步与完整 ZUI 已完成。 |

<details>
<summary>G9.3 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.1 | 普通 UTF8 文本字面量到只读文本与 String 的入口 | 已合入 | — | — | [PR48 验收][e-pr48]：静态UTF8、显式来源、Unicode/空串/NUL、跨模块返回、显式String分配失败清理及预算闭环；完整Unicode/格式化库另缺。 |
| G9.3.2 | 普通 POD 动态集合与真实借入视图 | 已合入 | — | — | [PR49 验收][e-pr49]：普通 Record 的 std.store、reserve/push/pop/shared view 和大 Copy 协议；默认5124/0、参考5012/0、文本70/0及工程门通过。原262144字节源码默认栈 O0/O2通过；一般元素/可变片/拥有组合仍另缺。 |
| G9.3.3 | 普通 POD 集合的可变元素借用：实际写入、helper 重借用与结构修改互斥 | 实现待验 | P0 | sol high 并行 / root 集成 | 原 store_at_mut 合法需求仍在 Types 显式拒绝；先保留整 Store 排他窗口、失败清理和两核独验，不推断不同下标互不重叠。 |
| G9.3.4 | 标量动态序列：直接 Store<i32/u64/usize/bool/浮点等既有标量> 与真实 std.store | 实现待验 | P0 | sol high 三核并行 | 初稿审计确认此前必须包装 Record；同批补直接标量、完整值协议和所有实际窗口，保留旧失败证据。 |

<details>
<summary>G9.3.3 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.3.1 | Types 完整元素准入及 LLVM 实际写入 | 实现待验 | P0 | store_types · sol high | 保留实际类型/范围与大 Copy 协议。 |
| G9.3.3.2 | Analyze 实际根、父loan和结构修改互斥 | 实现待验 | P0 | store_analyze · sol high | 保留 whole-Store 排他与旧费用。 |
| G9.3.3.3 | Verify 独立准入、实际窗口和 raw/Wire | 实现待验 | P0 | store_verify · sol high | 不以 Analyze 证明作为权限。 |

</details>

<details>
<summary>G9.3.4 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G9.3.4.1 | Source/Types 完整标量准入与 LLVM 计划 | 实现待验 | P0 | store_types · sol high | 复用既有协议，保留 owning 排除。 |
| G9.3.4.2 | Analyze 标量载体与实际窗口 | 实现待验 | P0 | store_analyze · sol high | 独立完整类型与实际根。 |
| G9.3.4.3 | Verify 标量协议与 raw/Wire | 实现待验 | P0 | store_verify · sol high | 独立完整类型、原父loan与计费。 |

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
| G10.3 | 大型closure/helper/foreign ABI及浮点/匿名联合责任的一般组合 | 部分实现 | P1 | 待派 | 补齐实际源码及最近反例；大数组修复不能覆盖全部 ABI。 |
| G10.4 | 原生Windows执行和平台差异验证 | 待执行 | P2 | 待派 | 需要原生 Windows 主机；环境可提前准备。 |
| G10.5 | 与Rust同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破 | 待执行 | P2 | root | 固定真实消费者、同语义 Rust 对照和独占计时窗口。 |

| G10.6 | B0、完整语言与ZUI终态验收 | 待执行 | P2 | root | 依赖全部必需语言任务、G9.6、G10.4、G10.5。 |

<details>
<summary>G10.6 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.6.1 | 语言初稿候选：通用必备组合、构建文档与同批验收；不替代 B0/完整语言门 | 实现待验 | P0 | root / sol high 并行 | 联合初稿必备 G2.4.2/G9.3.3/G9.3.4 与真实多模块程序已实现；冻结源码a8c7953e7已推送；首轮5188/2旧标量见证断言已修正，相关24/0，正在重跑完整共同验收与远端冷恢复。 |

</details>

<details>
<summary>G10.3 · 子任务</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.3.1 | GPU 平台 ABI 路线、实际消费者与 CPU/GPU 完成合同 | 待排期 | P2 | 待派 | 通用语言批次之后；已有 out-handle/固定 POD 签名通过 Verify。先具名 changes/独审并接真实 native adapter；若选纯 ZIL COM 再补外部方法/指针能力，不全面放宽 c_handle。 |

</details>

<details>
<summary>G10.1 · 当前联合验收</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.1.11 | 剩余 P0 的共同验收、回归修复与合入 | 已合入 | — | — | [PR46 验收][e-pr46] 精确源码 `953913782`：默认 **5007/0**、参考 **4895/0**、文本 **70/0**，两配置严格 Clippy、release 消费者与 10 项工程检查全过；远端 main `adbead51` 已核。Actions 因付款/额度未启动，原失败保留。 |
| G10.1.12 | 通用文本、泛型方法与借入迭代共同验收 | 已合入 | — | — | [PR48 验收][e-pr48]：默认5071/0、参考4959/0、文本70/0、双严格检查、release原生及10项工程门通过；首轮5项旧回退已修复且保留失败记录。 |

</details>

[design]: https://github.com/zil-lang/zil/blob/80c197b724c6cab538fbec7bf7fa526aaa877c5f/_zil_design.md
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
