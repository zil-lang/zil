# ZIL 任务看板

更新：2026-11-05（Asia/Shanghai）。本文件是唯一实时任务状态；只记录下表有限范围，不代表完整语言、B0 或 ZUI 已完成。规范与门槛以任务源码分支的[设计][design]、具名附件和[产品条件][gates]为准。

已合入＝所列范围已进入远端 main；实现待验＝已有实现、尚未接受；部分实现＝完整链路未闭合；进行中＝正在实现或验证、尚未验收；待执行/待排期＝尚未执行/派定。待办优先级仅 P0/P1/P2，已交付行用“—”。任务 ID 不带前导零。

[源码与恢复参考](README.md) · [分支、更新与备份规则](AGENTS.md) · [精简前完整记录](https://github.com/zil-lang/zil/blob/fc3dff269a882ef2850c4f9dba2acf6031ac4439/goal.md)

按能力分类，主表显示各项任务；子任务放在所属任务下的独立子表，默认折叠。展开后仍保留范围、状态、优先级、负责人和进展列，不使用四级标题。

父任务汇总完整范围，子任务记录具体交付，父子不重复计数；子项全完成不自动代表父项完成。需要独立派工、复核或验收的补充工作才新增任务编号，沿用父编号加一段；零散修正记在原任务进展中。编号同时用于分支与实施说明，不再另设勾稽编号。

## G1 · 基础类型与模式

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G1.1 | 有限Option、记录式枚举、usize/u64原生及CT链、拥有Tuple和混合Tuple/Enum部分移动 | 已合入 | — | — | [已合入证据][e37]；一般模式组合另列。 |
| G1.2 | 恢复Task集成误删的Option拥有载荷路径 | 已合入 | — | — | [本批验收][e41]；有限拥有 Option/Task 集成恢复已通过共同验收。 |
| **G1.3** | **拥有字段替换：Verify责任转交、真实loan/pin、机器old-drop/store及失败清理闭环** | 部分实现 | P0 | root | 局部 Package/Enum、字段/Store、退休检查及 Direct 整体 helper 的 Unit/有限 Copy/错误返回已合入，见 G1.3.4–8；剩余组合已由 G1.3.9 实现并通过限定联合测试，父项等待正式共同验收。 |
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
| G1.3.9 | 拥有值替换剩余组合：非局部 Package/Enum、capture/Store、loan/pin 与失败清理 | 实现待验 | P0 | sol · 拥有替换 | 联合源码 `be526bdf` 的 11 项已全过，含 Source/raw/Wire、Clang/API O0/O2；整体 ErrorUnion 清理函数漏生成已复现并修复。现有合法组合和最近 loan/责任反例闭合，正式共同验收待执行。实施：[task/G1.3.9](https://github.com/zil-lang/zil/tree/task/G1.3.9)；父项仍待完整验收。 |

</details>

## G2 · 泛型与 trait

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G2.1 | 有限const、supertrait、泛型trait/impl、关联类型/常量、默认体及泛型接收者方法 | 已合入 | — | — | [已合入证据][e37]；一般约束方程另列。 |
| G2.2 | 混合life/F helper、独立Invocation关系与未用声明检查，贯通两核及原生 | 已合入 | — | — | [本批验收][e41]；混合 life/F helper、独立 Invocation 与未用声明检查已通过共同验收。 |
| G2.3 | 一般关联投影、类型/Self依赖的常量表达式、约束蕴含和循环诊断 | 部分实现 | P1 | 待派 | G2.3.1 有限 Self 依赖常量已由 PR39 [合入][e39]；一般投影、泛型及 Drop 组合仍缺。 |
| G2.4 | 更一般life/lending、符号泛型方法和静态关联函数；一般/blanket/primitive/enum impl及更广supertrait实参组合 | 部分实现 | P1 | 待派 | 按需依赖 G2.3、G3.3、G3.4；保留定义期义务检查。 |

## G3 · 借用、拥有与生命周期

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.1 | 普通/嵌套Product借给可变helper、CopyT+mutable life、有限FP/HRTB和实际来源检查 | 已合入 | — | — | [已合入证据][e37]；一般借用返回见 G3.4。 |
| G3.2 | shared stored View、qualified静态Code及捕获为空的Fn，每个实际role独立核验 | 已合入 | — | — | [本批验收][e41]；真实静态 Code、空捕获 Fn 与逐 role 检查已通过共同验收。 |
| **G3.3** | **捕获的外层Fixed上下文、qualified FnMut/FnOnce的真实环境与调用关系** | 部分实现 | P0 | root | 真实 Fn/FnMut、有限 owning FnOnce free-helper 及一般无环来源流已合入，见 G3.3.4–8；剩余组合已由 G3.3.9–10 实现并通过限定测试，父项等待独审与正式共同验收。 普通闭包 Value 的 give F 按规范保持拒绝。 |
| G3.4 | 普通借入字段、存储上界、affine借用、合法lending结果及长期组合 | 部分实现 | P1 | 待派 | 依赖 G3.3、G1.3；需实际 loan/root 证明。 |
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
| **G3.3.9** | 闭包 helper 剩余组合：循环、lending、多个 F、Managed 转交和用户 Drop | 实现待验 | P0 | sol · 闭包/life | 已完成循环实际来源、逐 role 身份、非消费 Managed 与 lending/用户 Drop 组合；同 F 多个参数位置及 Fn/FnMut→Once 责任已补齐；作者冻结 `00cae775` 限定 16/0、52 次双后端原生运行、27 条预算边界与实际 raw/Wire 责任反例通过。独审及联合正式验收仍需收齐。实施：[task/G3.3.9](https://github.com/zil-lang/zil/tree/task/G3.3.9)；give F 规范反例保留。 |
| G3.3.10 | 多 callable 参数的完整实例键与逐角色来源消费者 | 实现待验 | P0 | sol · callable | [365235ae](https://github.com/zil-lang/zil/commit/365235ae94785e63daedbdd48cbcbc0fbd5c436c)：真实源码目标 20/0、三层来源等价 6/0；正在与 G3.3.9 的 lending/Drop 合并验证，不把分支通过数相加。 |

<details>
<summary>G3.3.9 · 回归收口</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G3.3.9.1 | 新闭包组合与历史验收共同收口 | 进行中 | P0 | sol · 回归 | 迁移已明确支持的零消费循环、有限 Once 适配和用户 Drop 正例，保留实际重复消费反例；精确费用差异逐项归因，默认额度不变。[实施分支](https://github.com/zil-lang/zil/tree/task/G3.3.9.1)。 |

</details>

</details>

## G4 · Type / Syntax 与编译期计算

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.1 | 普通泛型Type返回、具体纯泛型记录的CT计算/反射和已有Syntax组合 | 已合入 | — | — | [已合入证据][e37]；完整上下文和值范围另列。 |
| G4.2 | CT4混合const/life/type全序号身份及原源重实体化 | 已合入 | — | — | [本批验收][e41]；CT4 全序号及原源重实体化已通过共同验收。 |
| G4.3 | CT5真实Definition上下文、上下文键、物理字节计费和phantom Code/field查询 | 已合入 | — | — | [本批验收][e41]；CT5 实际上下文、键与物理字节计费已通过共同验收。 |
| **G4.4** | **完整实例实际参数向量、成员/生成作用域、重定位与声明归属** | 部分实现 | P0 | root | free/原始静态成员、嵌套标量及跨工厂纯标量 Phantom 返回已合入，见 G4.4.2–5.1；带life的Type结果、非标量纯字段、ambient Type 实参、泛型 host/member/generated 归属及运行时泛型体 CT 相位仍缺。 |
| G4.5 | rich callable、ambient Type导出和更一般可序列化值/匿名Sum/拥有载荷组合 | 部分实现 | P1 | 待派 | 依赖 G4.4、G3.3、G3.4；补齐实际消费者。 |

<details>
<summary>G4.4 · 子任务（6 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.2 | free CT factory 的完整 Type/Const/Life 实例向量与原声明身份 | 已合入 | — | — | [PR41 验收][e-pr41]；完整向量、作用域、复用深度计费及实际消费者通过。 |
| G4.4.3 | 原始非泛型宿主的静态混合参数 CT 工厂及成员删除后来源保留 | 已合入 | — | — | [PR42 验收][e-pr42]；实际原声明身份、物理复制/扫描费用、预算、语义缓存与原生消费者通过。 |
| G4.4.4 | 原始 CT factory 嵌套 Type/Const/Life 显式转发的标量结果 | 已合入 | — | — | [PR43 验收][e-pr43]；无占位结构体的3层转发、原成员嵌套调用及身份负控通过，真实向量/作用域和容量预付闭合。Phantom跨工厂结果及泛型宿主另缺。 |
| **G4.4.5** | **嵌套 CT 跨工厂返回 Phantom/Type 的实际 life 归属与重定位** | 部分实现 | P0 | root | 纯标量 Phantom 返回及真实 life 端点证明已由 G4.4.5.1 合入；非标量纯字段和带life的Type构造/跨工厂结果仍缺，后者见 G4.4.5.3。 |
| **G4.4.6** | ambient Type 实参、泛型宿主/成员及生成作用域的实际归属 | 进行中 | P0 | sol · CT / 生成 | G4.4.5.3 接 host/member 完整上下文；[task/G4.4.6](https://github.com/zil-lang/zil/tree/task/G4.4.6) 接实际生成树、发射位置与独立来源核验；生成片 3 组真实源码、6 次 Clang 原生运行和 9 个预算边界已限定通过。ambient Type 完整元数据与运行时相位组合继续验证，父项未验收。 |
| **G4.4.7** | 运行时泛型体中的 CT 相位与完整 Type/Const/Life 实例 | 进行中 | P0 | astra · 相位难点 | 联合矩阵 13/0 已通过真实 Source/project/Clang O0/O2 及四类预算边界；包括带 life nominal 的 Type 实参与 CT Type 值消费。[task/G4.4.7](https://github.com/zil-lang/zil/tree/task/G4.4.7) 接实际单态化入口，保留未用泛型定义检查与独立真实 Call 认证；life 保持原定义绑定。尚未验收。 |

<details>
<summary>G4.4.5 · 子任务（3 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.5.1 | 跨工厂 Phantom 返回的纯标量字段复制与真实 life 端点核验 | 已合入 | — | — | [PR45 验收][e-pr45]；双方原 schema/完整实参保留，独立核真实 Call、字段复制及 Definition/ordinal 端点。完整u64/负数/小数/Unit、两life和四余额边界闭合，并修复既存Unit回写错误。 |
| G4.4.5.3 | 带life的Type值构造与跨工厂返回的声明归属 | 进行中 | P0 | sol · CT | 实施 TypeInContext 完整图生产、真实源码 Type/Call 端点独核、CT6 编码及 ValueContext 返回；astra 复核公共认证边界。实施：[task/G4.4.5.3](https://github.com/zil-lang/zil/tree/task/G4.4.5.3)，尚未验收。 |
| G4.4.5.4 | 非标量纯字段跨工厂返回：Tuple/Array/Record/Enum 递归重构 | 进行中 | P0 | sol · CT | 接续 G4.4.5.1；原始 Tuple/Array/Record/Enum 组合已到实际 Source/机器完整 u64 结果，新增带 life 嵌套字段、最近反例与独立预算控制仍在复核。与 G4.4.5.3 同分支，保留旧标量费用快路。 |

</details>

<details>
<summary>G4.4.6 · 子任务（3 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.6.1 | qualified Type 实参的完整名义元数据及物理字段消费者 | 进行中 | P0 | sol · 独立三核 / root · 源映射 | [task/G4.4.6.1](https://github.com/zil-lang/zil/tree/task/G4.4.6.1)：Types、Analyze、Verify 分别重建 TypeArgument 名义行与完整实例；root 接原泛型形参到物理字段的实际映射。原 ambient Type Source/native/semantic-cache 完整身份已在联合提交 `612117d5` 复核通过；物理子字段复制及最近 raw/Wire 反例继续验证，不把首例通过当作子项全验收。 |
| G4.4.6.2 | CT 显式调用后泛型静态宿主的定义与后继消费 | 进行中 | P0 | sol · 生成/宿主 | [task/G4.4.6.2](https://github.com/zil-lang/zil/tree/task/G4.4.6.2)：首轮 8 项测试 3 过、5 失败；正在补单源入口导流、真实执行成功的 CT 调用回执、Const 实参域与符号上下文清理依赖。生成宿主正例及普通 runtime 双门已过；未用兄弟定义和完整原源认证由独审复核。 |
| G4.4.6.3 | 带 life 的纯 Enum 与数组字段：完整名义关系、递归消费者及原生运行 | 进行中 | P0 | sol · 源码 / 三核 / LLVM | 原递归合同缺口已明确；三组按文件并行实现，复用 Variant/Element 元数据路径，保留完整真实类型和原 life 关系。当前未通过 Source 联合验收，不将原普通枚举正例视为本项完成。 |

</details>

<details>
<summary>G4.4.7 · 子任务（2 项）</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G4.4.7.1 | runtime 泛型 Type 实参的 scoped 名义正规化与实例键 | 进行中 | P0 | sol · 实例键 | [task/G4.4.7.1](https://github.com/zil-lang/zil/tree/task/G4.4.7.1)：原 `leaf<Phantom<life,T,N>>` 与 CT Type 值两源在冻结 `c7b63633` 已 Source/project/native8 通过，并在联合 runtime 13 项重放通过；修正定义期 Copy 先行键和过早实参正规化，完整原实参仍由原 Call 独核。 |
| G4.4.7.2 | CT 相位与原源/生成来源映射的历史组合回归 | 进行中 | P0 | sol · CT 独核 | 全量发现旧 CT4、Self 常量、生成声明等真实源码回归；按实际来源树/节点与声明相位修复，保留原正例及最近伪造反例，不逐例豁免。 |

</details>

</details>

## G5 · 动态抽象

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G5.0 | 具体Fn/FnMut/FnOnce、opaque拥有工厂及有限具体环境保存 | 已合入 | — | — | [已合入证据][e37]；仅动态抽象前置。 |
| G5.1 | dyn真实Source→Core声明/provider身份 | 部分实现 | P1 | 待派 | [C39][c39] / [C40][c40] 已有前端；实际消费者待接。 |
| G5.2 | 异构调用、静态vtable、关联项与有限泛型method slot的真实派发 | 部分实现 | P1 | 待派 | 依赖 G5.1、G2.3、G2.4；两核与机器派发待闭合。 |
| G5.3 | 显式拥有/共同借入存储及管理route，移动/销毁/失败路径完整 | 部分实现 | P1 | 待派 | 依赖 G3.4、G1.3、G5.2。 |

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
| G9.3 | 完整字符串/集合与一般Iterator/lending组合 | 部分实现 | P1 | 待派 | 按具体 API 接 G3.4、G5.3；普通集合不普遍依赖 dyn。 |
| G9.4 | 真实平台窗口、事件循环及OS IME服务 | 待排期 | P2 | 待派 | 依赖 G7.3、G7.4 及平台接口；窗口和 IME 可拆分。 |
| G9.5 | 字体/文本布局、渲染呈现及输入到绘制的真实闭环 | 待排期 | P2 | 待派 | 与 G9.4 协调；需实际字体、呈现和输入链路。 |
| G9.6 | 真实ZUI应用集成、资源失败/关闭及维护场景 | 待排期 | P2 | 待派 | 依赖 G9.3、G9.4、G9.5 及必要语言能力。 |

## G10 · 可靠性、规范与产品验证

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.0 | 原大数组O0崩溃、默认2MiB API/深调用栈可靠性及普通大型按值ABI | 已合入 | — | — | [已合入证据][e37]；原 O0 崩溃已修复，更广 ABI 见 G10.3。 |
| **G10.1** | 精确源码独审、受影响组合/既有正式门、修正与合入裁决 | 进行中 | P0 | root | 远端 main 的 [PR45][e-pr45] 验收与 CurrentPass 保留；当前剩余 P0 联合批次见下方 G10.1.11，未合入 main。 |
| G10.2 | 有限f32/f64与UInt组合、匿名tagged Copy源Sum及CT Type结构Sum | 已合入 | — | — | [已合入证据][e37]；CT Sum 值与拥有载荷见 G4.5、G10.3。 |
| G10.3 | 大型closure/helper/foreign ABI及浮点/匿名联合责任的一般组合 | 部分实现 | P1 | 待派 | 补齐实际源码及最近反例；大数组修复不能覆盖全部 ABI。 |
| G10.4 | 原生Windows执行和平台差异验证 | 待执行 | P2 | 待派 | 需要原生 Windows 主机；环境可提前准备。 |
| G10.5 | 与Rust同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破 | 待执行 | P2 | root | 固定真实消费者、同语义 Rust 对照和独占计时窗口。 |
| G10.6 | B0、完整语言与ZUI终态验收 | 待执行 | P2 | root | 依赖全部必需语言任务、G9.6、G10.4、G10.5。 |

<details>
<summary>G10.1 · 当前联合验收</summary>

| 任务编号 | 任务范围 | 状态 | 优先级 | 负责人 | 进展 / 依赖 |
| --- | --- | --- | --- | --- | --- |
| G10.1.11 | 剩余 P0 的共同验收、回归修复与合入 | 进行中 | P0 | root | 冻结 `9281069d` 严格全目标 Clippy 通过；首轮完整默认测试 **4915 过 / 59 失败**。按 CT 来源、闭包/精确预算、拥有责任三类修复；参考配置及产品门尚未验收。[源码与真实失败记录](https://github.com/zil-lang/zil/tree/task/G10.1.11)。 |

</details>

[design]: https://github.com/zil-lang/zil/blob/fd261b22c8a05c5cd1e7bc9cc05d91eeb0f12c95/_zil_design.md
[gates]: https://github.com/zil-lang/zil/blob/fd261b22c8a05c5cd1e7bc9cc05d91eeb0f12c95/validation/product-gates.json
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
