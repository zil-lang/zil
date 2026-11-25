# ZIL 语言功能全景与 Alpha 路线图（Goal）

> 更新：2026-11-25（Asia/Shanghai） · 沿用全仓统一的 **`G1`–`G10` 功能域编号**（直接对齐历史提交、PR、规范与 `validation/language-release-gates.json` 的 12 个基础能力门，不再引入第二套编号），采用「总览索引 → 分域已完成/待完成明细 → 8 类日常程序综合验收 → 非 Alpha 阻断专项」结构。
>
> 规范权威：[`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) · [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) · [`validation/language-release-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/language-release-gates.json) · [`validation/language-alpha-profile.json`](https://github.com/zil-lang/zil/blob/main/validation/language-alpha-profile.json) · [`validation/product-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/product-gates.json)  
> 协作规则：[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md) · 历史快照：[`goal@e8faf57b2`](https://github.com/zil-lang/zil/blob/e8faf57b2/goal.md)

---

## 0. 执行摘要与当前推进队列

- **当前发布目标**：**Linux x86_64 全基础功能日常可用 `Alpha`**，对标 `validation/language-release-gates.json` 全部 **12 类基础语言功能门** + **G6.3 状态协议联合语义** + **8 类真实日常程序综合验收**（[PR #138](https://github.com/zil-lang/zil/pull/138) 已合入对齐合同）。
- **非 Alpha 阻断项**：Windows x64（`G10.4`）、ZUI 窗口/渲染/应用层（`G9.4–G9.6`）、编译与运行性能净突破专项（`G10.5`）均独立推进，不阻断首个 Linux Alpha。
- **强制零退化基线**：G6.2 合同语义 `27/27`、Fixed Projects `21/21`（60 构建 / 152 比较）、State/Store `16/128/56`、Roam `42/42 + 24/24`。

### 当前在途与自主实施队列

| 批次 | 事项 | 关联编号 | 分支 / PR | 当前态 |
| --- | --- | --- | --- | --- |
| 已合入 | Alpha 合同 12+8 对齐 | `G10.7` | [PR #138](https://github.com/zil-lang/zil/pull/138) | ✅ 已合入 `main` |
| 合入中 | 留证瘦身 + 分层缓存 CI + `tools/dev-check.py` | `G10.1` | [PR #140](https://github.com/zil-lang/zil/pull/140) | 🟠 CI 验证中（已测 5×–22× 提速） |
| 合入中 | G6.3-B `Replace` 协议语义 | `G6.3-B` | [PR #139](https://github.com/zil-lang/zil/pull/139) | 🟠 待合入（随 PR #140 后跑新 CI） |
| 合入中 | `zil-check` 终态模块化 + 合同测试拆分 | `G6.2` / `G10.1` | [PR #141](https://github.com/zil-lang/zil/pull/141) | 🟠 叠于 PR #139，待顺序合入 |
| Wave 1 | **G6.3-C `Store`** ∥ **`std.env.args` + `std.string` 切分修剪** ∥ **`dyn Trait<'a>` 区域替换 (`G5.3`)** | `G6.3-C` / `G9.3` / `G5.3` | 即将并行启动 | ⚪ 待启动 |
| Wave 2 | **G6.3-D `Task/Registration`** ∥ **`std.fs` 目录元数据 + `std.path`** | `G6.3-D` / `G9.3` | 依赖 Wave 1 | ⚪ 待启动 |
| Wave 3 | **G6.3-E `Cancel`** ∥ **`std.net` POSIX TCP Socket** | `G6.3-E` / `G9.3` | 依赖 Wave 2 | ⚪ 待启动 |
| Wave 4 | **8 类真实日常程序综合验收**（文本搜索/CSV/JSON/Todo/HTTP/批处理/图分析/异步调度） | `G10.7` | 依赖 Wave 1–3 | ⚪ 待启动 |

---

## 1. 语言功能全景总览（统一 `G1`–`G10` 索引）

状态图例：`✅ 已闭环`｜`🟡 已实现待联合验证`｜`🟠 部分实现`｜`⚪ 未开始`。

| 域 | 语言功能域 | 对应发布门（`language-release-gates.json`） | 当前态 | 核心已支持能力（一句话） | Alpha 关键剩余项 |
| --- | --- | --- | --- | --- | --- |
| [G1](#g1--基础类型聚合与模式匹配) | 基础类型、聚合与模式匹配 | `values-and-patterns` | 🟡 已实现待联合验证 | 10 整数 + `f32/f64`、`Record`/`Package`/`Enum`/`Option`/`ErrorUnion`/`T \| U`、引用模式与条件部分 Drop | 随 8 类日常程序综合联验（无硬缺口） |
| [G2](#g2--泛型与-trait-api) | 泛型与 Trait API | `generic-libraries` | 🟠 部分实现 | 固有 `impl`、泛型/Supertrait/关联类型/默认方法、整数 `const` 泛型、blanket `where`、拥有/借用迭代 | 泛型容器/迭代在日常库场景组合闭环；高级 `Self` 常量方程属 Post-Alpha |
| [G3](#g3--借用拥有与生命周期组合) | 借用、拥有与生命周期组合 | `ownership-composition`（数据模型） | 🟡 已实现待联合验证 | 逐字段借用、嵌套 `Package`、512 槽共享地址数组、`Fn`/`FnMut`/`FnOnce` 捕获与路径敏感补齐 | 随日常程序与 G6.3 联验；广义多源 lending 属 Post-Alpha |
| [G4](#g4--编译期计算与元编程) | 编译期计算与元编程 | `project-workflow`（编译期部分） | 🟡 已实现待联合验证 | 泛型 `Type` 返回、CT 反射与全序号身份、真实 Definition 上下文、`MetaBlock` + `yield` | 保持回归零退化；`#if`/`#for` 与 ambient `Type` 属 Post-Alpha |
| [G5](#g5--动态抽象与异构派发) | 动态抽象与异构派发 | `dynamic-abstraction` | 🟠 部分实现 | 借入 `dyn`、参数化 `dyn Trait<T>`、`OwnedDyn<dyn Trait, Context<'a>>` 与共同借入 `Store`、零泄漏堆箱释放 | **`G5.3`**：`dyn Trait<'a>` 生命周期返回区域替换（`static-callback-getter-cycle-control`） |
| [G6](#g6--状态协议与效应合同) | 状态协议与效应合同 | `ownership-composition`（协议核心） | 🟠 部分实现 | 64 态用户协议（G6.1）、投影失效上界/历史撤销 `H`/嵌套资源跨参数关联（G6.2 `27/27` 闭环） | **`G6.3`**：`Replace`（PR #139）→ `Store` → `Task/Registration` → `Cancel` |
| [G7](#g7--外部接口c-abi-与已登记回调) | 外部接口、C ABI 与已登记回调 | `foreign-lifecycle` | 🟠 部分实现 | 完整 POD C-ABI/`CCallback`/`errno`、五态 CAS `c_register`/`c_register_async`、`RISKS.json` 14 项 | **`G6.3-D/E`** 联合语义（`AsyncRegistration` 与用户协议及取消边） |
| [G8](#g8--异步任务与多线程并发) | 异步任务与多线程并发 | `async-ownership` + `threads-and-synchronization` | 🟠 部分实现 | `ucontext_t` 协程与 `PendingExit` 取消回滚、Roam `24/24`、`pthread` `ThreadScope`/`Mutex<T>`/5 序原子与强 CAS | **`G6.3-D/E`**（`TaskCreate`/`Valid`/`TaskStart`/取消边）+ 异步调度器日常应用 |
| [G9](#g9--标准库资源图与-zui) | 标准库、资源图与 ZUI | `standard-library` + `resource-graphs` | 🟠 部分实现 | 26 个内嵌信任根模块（UTF-8/`String`/数值/POD&Owning 哈希集合/`iter`/`graph`/`io`/`buffered_io`/基础 `fs`/`env`/`time`）+ `G9.2` 软件 UI 核心 | **Alpha 4 件套**：`std.env.args`、`std.string` 切分修剪、`std.fs` 目录元数据 + `std.path`、`std.net` TCP（`G9.4–G9.6` ZUI 不阻断 Alpha） |
| [G10](#g10--工程工作流原生交付与发布验收) | 工程工作流、原生交付与发布验收 | `project-workflow` + `native-delivery` + `release-acceptance` | 🟠 部分实现 | `zil check/build/run/test/lock`、双后端 O0/O2、Fixed Projects `21/21`、分层缓存 CI（PR #140） | **`G10.7`** 8 类真实日常程序综合验收 + 冻结候选全门独立签发（`G10.4` Windows / `G10.5` 性能 / `G10.6` 终态产品门不阻断 Alpha） |

---

## 2. 分功能域明细（`G1`–`G10`：已完成 vs 待完成）

### G1 · 基础类型、聚合与模式匹配

- **发布门**：`values-and-patterns` · **规范**：`ZIL-SYNTAX`、`ZIL-MODES`、`ZIL-PACKAGE`、`ZIL-CLEANUP` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G1.1 | 标量、整数/浮点与显式转换 | ✅ 已闭环 | Alpha 必选 | 10 种整数（`u8..usize`/`i8..isize`）+ `f32/f64`、`bool`/`char`/`unit`、checked/wrapping 转换、无符号数组/切片越界检查 | 无（保持双后端 O0/O2 回归） |
| G1.2 | 聚合、错误处理与匿名联合 | ✅ 已闭环 | Alpha 必选 | `Record`/`Package`/`Tuple`/`Array`/`Slice`/`Enum`/`Option`/`ErrorUnion`（`?` 传播）、匿名联合 `T \| U`（拥有载荷 Drop、规范排序与重复/非穷尽/双消费拒绝） | 无（`owned-sum-consumer` 全 32 组合闭环） |
| G1.3 | 拥有字段替换与借用/Drop 协同 | ✅ 已闭环 | Alpha 必选 | 整值/字段 `Replace` 责任转交、loan/pin 与 `old` 释放、失败清理闭环（状态协议扩展见 `G6.3-B`） | 随 `G6.3-B` 闭环协议状态与历史隔离 |
| G1.4 | 模式匹配、引用模式与条件部分移动 | 🟡 已实现待联合验证 | Alpha 必选 / Post-Alpha | 命名/位置解构、记录模式简写/省略、`match` 守卫保留拥有 scrutinee、`a..=b`、嵌套引用模式 `&pat`/`&mut pat`/`&&pat` 与 `drop_elab` 补齐 | **Alpha**：随 8 类日常程序联验；**Post-Alpha**：更深多层地址/资源包嵌套模式、`match` 表达式/或模式/带标签 `break` |

### G2 · 泛型与 Trait API

- **发布门**：`generic-libraries` · **规范**：`ZIL-TRAIT`、`ZIL-MODULE`、`ZIL-RECEIVER` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G2.1 | 泛型参数、固有 `impl` 与整数 `const` 泛型 | ✅ 已闭环 | Alpha 必选 | 类型/生命周期/`<#N: usize>`、固有 `impl`、具体类型/枚举静态关联函数、跨模块 nominal 身份 | 无 |
| G2.2 | Trait、Supertrait、关联项与默认方法 | 🟡 已实现待联合验证 | Alpha 必选 | 默认方法体、Supertrait 蕴含与循环拒绝、`T: Iter<Item = U>` 关联类型/常量绑定与跨 trait 消歧、泛型接收者方法 | 随泛型容器/迭代在 8 类日常应用中双后端原生联验 |
| G2.3 | 关联投影与 `Self`/类型依赖常量表达式 | 🟠 部分实现 | Post-Alpha | 有限 `Self` 依赖常量已合入 | 一般关联投影、任意类型/`Self` 依赖常量求值与约束方程 |
| G2.4 | 泛型 Trait 方法、Blanket Impl 与广义 Lending | 🟠 部分实现 | Alpha 必选 / Post-Alpha | Trait 泛型方法、blanket impl `where` 校验、未用定义期义务诊断、`Store<Item>` 普通拥有 lending | **Alpha**：日常库泛型容器/迭代组合闭环；**Post-Alpha**：更广 supertrait 泛型实参、一般 blanket 约束方程与开放泛型方法槽 |

### G3 · 借用、拥有与生命周期组合

- **发布门**：`ownership-composition`（数据模型） · **规范**：`ZIL-ORIGIN`、`ZIL-ACCESS`、`ZIL-CAPTURE`、`ZIL-PACKAGE` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G3.1 | 逐字段借用与嵌套拥有资源包 | ✅ 已闭环 | Alpha 必选 | `Struct`/`Package`/`Tuple` 逐字段借用（`Carries::set_field`）、字段覆写释放旧 loan、多共享/独占借入叶、嵌套 qualified 子 `Package`、`move_out` 后补回 | 无 |
| G3.2 | 固定共享地址数组与快照 | ✅ 已闭环 | Alpha 必选 | 非零 `[&'a T; N]`（含 512 槽）、常量/动态整数索引、普通 helper 传参返回、字段快照逐槽初始化核验 | 空数组与独占数组复制（Post-Alpha） |
| G3.3 | 闭包捕获与路径敏感补齐 | ✅ 已闭环 | Alpha 必选 | 真实捕获 `Fn`/`FnMut`/`FnOnce`、外层上下文、路径敏感 `FnMut` 捕获与 `match`/`let`/`break` 补齐、逐 role 独立核验 | 无 |
| G3.4 | 可变借入存储与跨函数 Lending | 🟠 部分实现 | Post-Alpha | 同型整对象从唯一实际输入返回、`Store<Item>` lending 窗口与拥有回调管线 | 多输入借用返回源、跨函数显式存储上界、任意嵌套可变载体与广义 affine lending |
| G3.5 | 拥有/借入捕获与责任综合矩阵 | 🟡 已实现待联合验证 | Alpha 必选 | PR #123/#124 闭合 `FnMut` 捕获与 `OwnedDyn` 释放矩阵 | 随 8 类日常应用与 G6.3 联合验证 |

### G4 · 编译期计算与元编程

- **发布门**：`project-workflow`（编译期部分） · **规范**：`ZIL-COMPTIME`、`ZIL-MODULE` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G4.1–G4.3 | CT 泛型 `Type`、全序号身份与 Definition 上下文 | ✅ 已闭环 | Alpha 必选 | 普通泛型 `Type` 返回、混合 const/life/type 全序号身份与原源重实体化、真实 Definition 上下文与物理字节计费 | 无 |
| G4.4 | `MetaBlock` + `yield` 与完整实例上下文 | 🟡 已实现待联合验证 | Alpha 必选 / Post-Alpha | `MetaBlock` + `yield`（经 verified `zil-interp` 执行、拒绝非法拥有载荷 `E-RESOURCE-TYPE`） | **Post-Alpha**：一般成员/生成作用域、重定位与 `#if`/`#for` 元语句 |
| G4.5 | 高级 CT 值导出与 Rich Callable | 🟠 部分实现 | Post-Alpha | 基础纯值 CT 反射已可用 | rich callable、ambient `Type` 导出、更一般可序列化值与拥有载荷组合 |

### G5 · 动态抽象与异构派发

- **发布门**：`dynamic-abstraction` · **规范**：`ZIL-TRAIT`、`ZIL-ROUTE`、`ZIL-BYTE-STORAGE` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G5.1–G5.2 | 借入 `dyn Trait`、静态 vtable 与对象安全 | ✅ 已闭环 | Alpha 必选 | `&dyn Trait` / `&mut dyn Trait`、参数化 `dyn Trait<T>` / `Assoc = T`、object-safety 校验、`FnRole::DynSlotImpl` 授权核验、共享/独占接收者 | 有限泛型 method slot（Post-Alpha） |
| **G5.3** | **显式拥有擦除存储 `OwnedDyn` 与生命周期区域替换** | 🟠 部分实现 | **Alpha 必选** | `OwnedDyn<dyn Trait, Context<'a>>`、共同借入 `Store<OwnedDyn<..., Context<'a>>>`、按值 `self: Self` 堆箱零泄漏释放、`OwnedDynCreate` Context 健全性、循环 `alloca` 提升 | **Alpha 必选**：修复 `dyn Trait<'a>` trait 生命周期参数方法返回区域替换（`static-callback-getter-cycle-control.zil`）；**Post-Alpha**：未选 owner 错误联合捕获与非接收者 provider |

### G6 · 状态协议与效应合同

- **发布门**：`ownership-composition`（协议核心） · **规范**：`ZIL-CONTRACT`、`ZIL-ORIGIN`、`ZIL-CLEANUP` · **总态**：🟠 部分实现

> **架构红线**：全链路 `Typeck -> Body IR -> Analyze -> Verify -> Lower -> Native` 共享 [`zil-semantics`](https://github.com/zil-lang/zil/tree/main/crates/zil-semantics) 单一事实源，不为 `zil-check` 另造语义系统；保持 **G6.2 `27/27`** 与 **Fixed Projects `21/21`** 零退化。

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G6.1 | 有限状态协议基础（内建 + 64 态用户协议） | ✅ 已闭环 | Alpha 必选 | `requires(state/apart)`、`provides(state/retired/on_pending/on_ready/on_ok/on_err)`、跨 `match` 分支精化、未标注调用方前置核验、Raw/Wire 独立证明 | 超 64 态或泛型参数化状态域（Post-Alpha） |
| G6.2 | 投影失效上界、历史撤销 `H` 与嵌套资源别名 | ✅ 已闭环 | Alpha 必选 | `effects(invalidates(...))`、`effects(rollback(...))`、完整类型图 `mut_cap` 扫描、`Managed`/`Desc` 自动描述与嵌套资源跨参数关联（**27/27 闭环**；PR #141 完成 `zil-check` 终态模块化） | 超 4 层嵌套字段路径、跨模块导出上界扩展（Post-Alpha） |
| **G6.3-B** | **联合语义 1：`Replace`** | 🟠 合入中（[PR #139](https://github.com/zil-lang/zil/pull/139)） | **Alpha 必选** | `zil-semantics::protocol` 单一模型、`Replace` 溯源分离、旧新值不共用历史 `H`、旧态与责任转入 `old`、新值清旧 `retired`；本地 `30/30` 全绿 | 随 PR #140 合入后跑新 CI 并合入 `main` |
| **G6.3-C** | **联合语义 2：`Store`** | ⚪ 待启动（Wave 1） | **Alpha 必选** | `StoreBorrow` 跨 `Desc` 别名 `LoanConflict` 已落地 | `StorePush`/`StorePop` 区分成功与失败（OOM 返还原状态、空容器不伪造句柄）；`StoreBorrow` 借用不等于实际修改，仅实际写入/取出/替换触发状态转移与失效 |
| **G6.3-D** | **联合语义 3：`Task` / `Registration`** | ⚪ 待启动（Wave 2） | **Alpha 必选** | `Task`/`AsyncRegistration` 基础三态与 `FinishDestroy` 门控已落地 | `TaskCreate` 条件成功建模（OOM 返还）；`Valid == false` 不代表 `Closed`；补齐 `TaskStart` 完整语义定义；联合 `AsyncRegistration` 五态与用户协议 |
| **G6.3-E** | **联合语义 4：`Cancel`** | ⚪ 待启动（Wave 3） | **Alpha 必选** | `PendingExit` 机器清理与 `Terminator::Call { cancel }` 控制流边已存在 | 取消边既不直接沿用调用前状态也不套用正常返回后置，保守保留取消点前所有可能已发生的副作用与所有权转移 |

### G7 · 外部接口、C ABI 与已登记回调

- **发布门**：`foreign-lifecycle` · **规范**：`ZIL-ASYNC`、Design §12.4、`RISKS.json` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G7.1 | 同步 `Registration`、C ABI 与 `CCallback` | ✅ 已闭环 | Alpha 必选 | 10 整数 + `f32/f64` C-ABI、递归 POD Record、`CCallback`（Wire 49）、`c_null()`（Wire 48）、`ForeignHandle ==/!=`、glibc `errno`、同步稳定闭包登记与显式强关闭 | 非 Linux ABI 见 `G10.4` |
| G7.2–G7.3 | `AsyncRegistration` 五态门控与晚通知 Drain | 🟡 已实现待联合验证 | Alpha 必选 / Post-Alpha | 五态 CAS 门控（`OpenIdle`..`Closed`）、`driver_lock`、`RISKS.json` 14 项 1-to-1 绑定、8 份 C-host 与 `raw-holder.zil` pthread 实测 | **Alpha**：随 `G6.3-D/E` 闭环协议联合；**Post-Alpha**：一般跨线程借入捕获环境（`Address<Closure>`/`&mut`） |
| G7.4 | `Registration` 与异步任务取消/失败组合 | 🟠 部分实现 | **Alpha 必选** | 有限 Task 失败/取消/返回排空已合入 | 随 **`G6.3-D/E`** 闭环取消边与状态协议联合 |

### G8 · 异步任务与多线程并发

- **发布门**：`async-ownership` + `threads-and-synchronization` · **规范**：`ZIL-ASYNC`、`spec/affine-mutex.md` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G8.1–G8.2 | 结构化 `ThreadScope`、协程栈与静态拥有 `Task` | ✅ 已闭环 | Alpha 必选 | `pthread_create`/`pthread_join`、`ucontext_t` 协程、4 KiB `PROT_NONE` 保护页、`std.task.*`、`task_await`、Roam `24/24` + `24+9` | 无 |
| G8.3 & G8.5 | `Mutex<T>` / `Guard`、有序原子与 `Transfer`/`Share` | 🟡 已实现待联合验证 | Alpha 必选 / Post-Alpha | `pthread_mutex_t` `Mutex<T>`/affine `Guard`、`MemoryOrder` 5 序与 `Sync*Ordered`（Wire 43–47）、强 CAS、保留 `Transfer`/`Share`、`thread_scope` 裸 `c_handle` 与 `Mutex<Resource>` FFI Drop 拒绝 | **Alpha**：与 HTTP/文件批处理/调度器联合并发验证；**Post-Alpha**：用户泛型自动派生 `Transfer`/`Share` |
| G8.4 & G8.6 | `PendingExit` 取消回滚、任务协议与调度组合 | 🟠 部分实现 | **Alpha 必选** | `PendingExit` 单次 Drop、`EnumPartialDrop`、`task_create`/`task_create_owned` OOM 清理、逐路径 CFG `resume(transfer)` 证明 | **Alpha 必选**：完成 **`G6.3-D/E`** 并通过“异步任务调度器”日常程序验收；**Post-Alpha**：一般借入 frame port、`FnOnce` 挂起根精细区分、跨模块 `pub Suspend` 非标量 ABI |

### G9 · 标准库、资源图与 ZUI

- **发布门**：`standard-library` + `resource-graphs` · **规范**：`spec/pod-hash-collections.md`、`spec/owning-hash-collections.md`、`spec/numeric-text-and-buffered-io.md`、`spec/checked-resource-graph.md` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G9.1 | 基础文本、数值格式与资源图 | 🟠 部分实现 | **Alpha 必选** | `std.utf8`、`std.string`（构造/搜索/比较/编辑）、`std.text_edit`、`std.int`、`std.float`、`std.graph`（循环/共享边/逻辑删除/有界 ticket 复用） | **Alpha 必选**：`std.string` 高频切分/修剪/按行遍历（`split`/`trim`/`lines`/`starts_with`/`ends_with`/`contains`）+ `std.graph` 纳入图依赖分析器联验 |
| G9.3a | 容器、哈希集合与迭代器 | 🟡 已实现待联合验证 | Alpha 必选 | `std.store`、`std.collections`、`std.iter`、`pod_hash_*`/`i64_hash_*`/`string_map`、`owning_map`/`owning_hash_map`/`owning_hash_set`（26 个内嵌信任根模块） | 随 8 类日常程序验证 OOM/拥有值组合；高阶集合算法与跨过程 `apart` 精度属 Post-Alpha |
| **G9.3b** | **系统 I/O、路径、CLI 参数与网络基础库** | 🟠 部分实现 | **Alpha 必选** | `std.io`（含 `stderr`/`print*`/`EINTR` 重试）、`std.buffered_io`、`std.fs` 基础读写（`read_to_bytes`/`write_bytes`/`exists`/`remove_file`）、`std.env`（`get`/`current_dir`/`exit`）、`std.time` | **Alpha 必选**：① `std.env.args`（`argc`/`argv`）；② `std.fs` 目录/重命名/元数据（`read_dir`/`mkdir`/`mkdir_all`/`rename`/`stat`）+ `std.path`；③ `std.net` POSIX TCP Socket（`TcpListener`/`TcpStream`） |
| G9.2 | ZUI 软件核心库 | ✅ 已合入 | 非 Alpha 阻断 | 整数布局、RGBA 软件合成、IME preedit 提交/取消与失败保留普通库 | 见 `G9.4–G9.6` |
| G9.4–G9.6 | ZUI 窗口、GPU 渲染与应用层 | 🟠 部分实现 / ⚪ 未开始 | **非 Alpha 阻断** | `G9.4` 已有初步窗口实验 | `G9.4` 真实 OS 窗口/事件循环/OS IME；`G9.5` GPU 首帧与字体塑形；`G9.6` 实体/组件与确定性关闭合流 |

### G10 · 工程工作流、原生交付与发布验收

- **发布门**：`project-workflow` + `native-delivery` + `release-acceptance` · **规范**：`docs/guide/build.md`、`validation/language-alpha-profile.json`、`validation/product-gates.json` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G10.0 & G10.2–G10.3 | Linux x86_64 双后端、大值 ABI 与栈可靠 | ✅ 已闭环 | Alpha 必选 | Clang + LLVM 23.1.4 C API × O0/O2、1/4/8 并行链接、Fixed Projects `21/21`、State/Store `16/128/56`、大数组 O0/2 MiB 深栈修复、逐分配点 OOM 零泄漏自检 | 随 8 类日常程序验证 OOM/早退清理与冷恢复 |
| G10.1 | 项目工作流、分层 CI 与留证治理 | 🟠 合入中（[PR #140](https://github.com/zil-lang/zil/pull/140)） | Alpha 必选 | `zil check/build/run/test/lock`、常驻模式；PR #140 清理 20 份历史 JSON（含 `alpha-coverage.json` 冲突残留）、CI 提速至 ~4m / `documentation` 13s、新增 `tools/dev-check.py` | 合入 PR #140 与 PR #141 |
| **G10.7** | **8 类真实日常程序综合验收与 Alpha 冻结签发** | ⚪ 待启动（Wave 4） | **Alpha 必选** | `validation/language-alpha-profile.json` 已随 PR #138 对齐 12+8 合同 | 落地全部 8 类多模块 ZIL 应用（见下表）并在单一冻结提交完成全门复验与非作者独立源码复核 |
| G10.4 | Windows x64 原生支持 | ⚪ 未开始 | **非 Alpha 阻断** | 无原生 Windows 执行收据 | 原生 Windows x64 CLI、ABI、Runtime、I/O、线程/Task 与真实 Windows CI（交叉构建/Wine 不算通过） |
| G10.5 | 编译与运行性能突破专项 | 🟠 进行中 | **非 Alpha 阻断** | LLVM 23.1.4 Stage 1 桥接已合入；独立研究分支 `research/full-push-native`、`integrate/scalar-codegen` 推进中 | Stage 2（去除 `body_structured.rs` 文本桥接直接降级）及同语义 Rust 相对性能/编译净突破 |
| G10.6 | 终态产品门（`product-gates.json`） | ⚪ 未开始 | **非 Alpha 阻断** | `validation/product-gates.json` 合同已固定 | 零 safe UB、Rust 相对性能/编译门、可重复净突破、B0 与 ZUI 终态冻结独立复验 |

#### G10.7 明细：8 类真实日常程序综合验收矩阵

| # | 应用（Gate ID） | 覆盖功能域 | 阻塞前置 | 当前态 |
| --- | --- | --- | --- | --- |
| 1 | **文本统计与文件搜索** (`text-stats-and-file-search`) | G1、G9、G10 | `std.env.args`、`std.fs.read_dir`、`std.string` 切分 | ⚪ 未开始 |
| 2 | **CSV 处理工具** (`csv-processor`) | G1、G2、G9 | `std.string` 行/分隔辅助 | ⚪ 未开始 |
| 3 | **JSON 解析与序列化器** (`json-processor`) | G1、G3、G9、G10 | 递归拥有树/容器组合与 OOM 清理验证 | ⚪ 未开始 |
| 4 | **CLI Todo 应用** (`cli-todo-app`) | G6（`Replace`/`Store`）、G9、G10 | `std.env.args`、G6.3-B/C | ⚪ 未开始 |
| 5 | **HTTP 小服务与客户端** (`http-mini-service-and-client`) | G7、G8、G9（TCP）、G5 | `std.net` POSIX TCP Socket | ⚪ 未开始 |
| 6 | **文件批处理工具** (`file-batch-processor`) | G9（目录/路径/重命名）、G8、G3 | `std.env.args`、`std.fs` 扩展、`std.path` | ⚪ 未开始 |
| 7 | **图依赖分析器** (`graph-dependency-analyzer`) | G9（`std.graph`）、G2 | 基于现有 `std.graph` 组装端到端应用与故障清理 | ⚪ 未开始 |
| 8 | **异步任务调度器** (`async-task-scheduler`) | G8、G7、G6（`Task`/`Cancel`）、G5 | G6.3-D/E | ⚪ 未开始 |

---

## 3. 看板治理与验收红线

1. **单一真相与防退化**：子项通过不自动关闭父能力；安全边界缺陷永远 P0 阻断；所有改动保持 G6.2 `27/27`、Fixed Projects `21/21`、State/Store `16/128/56`、Roam `42/42 + 24/24` 零退化。
2. **活证据优先**：日常 PR 以 Git SHA + 可执行测试 + 绿灯 CI 证明正确性，不向 `validation/` 追加过程 JSON 留证；静态验收收据仅在冻结发布候选或里程碑签发时生成一份。
3. **反虚高计量与独立 Verify**：全仓 `tests/**/*.zil` `zil check --json` `status == "Success"` 计数不是通过率；`zil-check` 必须独立核验 Source 与 Raw/Wire 有效证书并拒绝伪造证书。
4. **原地维护**：PR 合入或状态变化时仅原地更新对应单元格，不在 `goal.md` 堆积流水账。
