# ZIL 语言功能全景与 Alpha 路线图（Goal）

> 更新：2026-11-25（Asia/Shanghai） · 参照业界语言路线图与特性矩阵实践（总览索引 → 分功能域「已完成 / 待完成」明细 → 真实应用综合门 → 非阻断独立专项），用于一页总览 **ZIL 支持哪些语言功能、每项已完成到什么程度、以及距离 `Alpha` 和远期目标还缺什么**。
>
> 规范权威：[`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) · [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) · [`validation/language-release-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/language-release-gates.json) · [`validation/language-alpha-profile.json`](https://github.com/zil-lang/zil/blob/main/validation/language-alpha-profile.json) · [`validation/product-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/product-gates.json)  
> 协作规则：[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md) · 历史快照：[`goal@e8faf57b2`](https://github.com/zil-lang/zil/blob/e8faf57b2/goal.md)

---

## 0. 执行摘要与当前在途

- **主线基线**：[`main@08da36ff0`](https://github.com/zil-lang/zil/commit/08da36ff0)（无 P0 阻塞；强制回归基线：**G6.2 合同语义 `27/27`**、**Fixed Projects `21/21`**（60 构建 / 152 比较）、**State/Store `16/128/56`**、**Roam `42/42 + 24/24`** 全绿）。
- **当前发布目标**：首个发布统一命名 **`Alpha`**（目标平台 **Linux x86_64**），功能完整度对标 `language-release-gates.json` 全部 **12 类基础语言功能域** + **G6.3 状态协议联合语义** + **8 类真实日常程序综合验收**。
- **非 Alpha 阻断项**：Windows x64 原生支持、ZUI 窗口/渲染/应用层、编译与运行性能突破专项（独立分支推进）均不阻断首个 Linux Alpha，但在本文末尾保持独立追踪与验收门。

### 当前在途与下一队列

| 顺序 | 事项 | 分支 / PR | 当前态 | 说明 |
| --- | --- | --- | --- | --- |
| 在途 | **Alpha 合同对齐**（12 基础能力 + G6.3 + 8 日常应用门） | `chore/alpha-profile-12-plus-8` · [PR #138](https://github.com/zil-lang/zil/pull/138) | 待合入（CI 3/3 绿） | 升级主树 `validation/language-alpha-profile.json` |
| 在途 | **G6.3-B `Replace` 状态协议联合语义** | `feat/g63-replace-state-protocol` · [PR #139](https://github.com/zil-lang/zil/pull/139) | 待 CI 与审阅 | 共享 `zil-semantics::protocol`、历史 `H` 隔离与 `old` 责任转移；本地 `30/30` 全绿 |
| 下一 | **G6.3-C `Store`** → **G6.3-D `Task/Registration`** → **G6.3-E `Cancel`** | 待切分小 PR | 待执行 | 延续 `zil-semantics` 单一事实源，严禁为 Verify 另造语义系统 |
| 并行 | **8 类日常程序标准库前置**（`std.env.args` / `std.fs` 目录与 `std.path` / `std.net` TCP / `std.string` helper） | 待切分小 PR | 待执行 | 补齐后直接驱动 8/8 日常应用端到端双后端验收 |

---

## 1. 语言功能全景总览（12 大功能域索引）

状态图例：`✅ 已闭环`（能力与验收门均已闭合）｜`🟡 已实现待联合验证`（主干已落地，待 8 类日常程序或冻结候选联验）｜`🟠 部分实现`（核心已用，仍有 Alpha 必选或 Post-Alpha 缺口）｜`⚪ 未开始`。

| 域 | 语言功能域 | Gate ID | 当前态 | 核心已支持能力（一句话） | Alpha 关键剩余项 | 历史编号 |
| --- | --- | --- | --- | --- | --- | --- |
| [F1](#f1--基础类型聚合与模式匹配) | 基础类型、聚合与模式匹配 | `values-and-patterns` | 🟡 已实现待联合验证 | 10 整数 + `f32/f64`、`Record`/`Package`/`Enum`/`Option`/`ErrorUnion`/`T \| U`、引用模式与条件部分 Drop | 随 8 类日常程序综合联验（无硬缺口） | G1 / G10.2 |
| [F2](#f2--泛型与-trait-api) | 泛型与 Trait API | `generic-libraries` | 🟠 部分实现 | 固有 `impl`、泛型/Supertrait/关联类型/默认方法、整数 `const` 泛型、blanket `where`、拥有/借用迭代 | 泛型容器/迭代在日常库场景组合闭环；高级 `Self` 常量方程属 Post-Alpha | G2 |
| [F3](#f3--所有权借用与状态协议) | 所有权、借用与状态协议 | `ownership-composition` | 🟠 部分实现 | 逐字段借用、嵌套 `Package`、512 槽共享地址数组、`FnMut` 路径补齐、64 态用户协议与投影/撤销 `H`（G6.2 `27/27`） | **G6.3 联合语义**（`Replace` 在途 PR #139；余 `Store` / `Task-Registration` / `Cancel`） | G3 / G6 |
| [F4](#f4--动态抽象与异构派发) | 动态抽象与异构派发 | `dynamic-abstraction` | 🟠 部分实现 | 借入 `dyn`、参数化 `dyn Trait<T>`、`OwnedDyn<dyn Trait, Context<'a>>` 与共同借入 `Store`、零泄漏堆箱释放 | `dyn Trait<'a>` 生命周期返回替换（`static-callback-getter-cycle-control`）与日常应用联验 | G5 |
| [F5](#f5--稳定资源与图生命周期) | 稳定资源与图生命周期 | `resource-graphs` | 🟡 已实现待联合验证 | `std.graph` runtime-sized 循环/共享边图、分配失败返还、逻辑删除与有界 ticket 复用 | 随“图依赖分析器”日常程序完成端到端 OOM 与故障清理联验 | G9.3 (Graph) |
| [F6](#f6--异步任务挂起与取消) | 异步任务、挂起与取消 | `async-ownership` | 🟠 部分实现 | `ucontext_t` 原生协程、保护页、`PendingExit` 取消回滚、OOM 清理、`std.task.*`、`task_await`、Roam `24/24` | **G6.3-D/E**（`TaskCreate` 条件成功、`Valid == false != Closed`、`TaskStart`、取消边健全性）+ 调度器应用 | G8.2 / G8.4 / G8.6 |
| [F7](#f7--线程互斥锁与原子操作) | 线程、互斥锁与原子操作 | `threads-and-synchronization` | 🟡 已实现待联合验证 | `pthread` `ThreadScope`、`Mutex<T>` / affine `Guard`、5 序原子与强 CAS、`Transfer`/`Share` 保留 trait | 与 HTTP/文件批处理/调度器日常程序并发联验；泛型自动派生属 Post-Alpha | G8.1 / G8.3 / G8.5 |
| [F8](#f8--外部接口c-abi-与已登记回调) | 外部接口、C ABI 与已登记回调 | `foreign-lifecycle` | 🟠 部分实现 | 五态 CAS 门控 `c_register`/`c_register_async`、`RISKS.json` 14 项、`CCallback`/POD C-ABI/`errno` | **G6.3-D/E**（`AsyncRegistration` 与用户协议及取消边联合） | G7 / G10.3 |
| [F9](#f9--标准库文本集合迭代与系统-io) | 标准库：文本、集合、迭代与系统 I/O | `standard-library` | 🟠 部分实现 | 26 个内嵌信任根模块：UTF-8/`String`/数值文本/POD 与 Owning 哈希集合/`iter`/`graph`/`io`/`buffered_io`/基础 `fs`/`env`/`time` | **4 项日常应用前置**：`std.env.args`、`std.fs` 目录/重命名/元数据与 `std.path`、`std.net` TCP、`std.string` 切分 helper | G9.1 / G9.3 |
| [F10](#f10--编译期编程与项目工作流) | 编译期编程与项目工作流 | `project-workflow` | 🟡 已实现待联合验证 | CT `MetaBlock`+`yield`（verified `zil-interp`）、项目清单与路径依赖锁、`zil check/build/run/test/lock` 与常驻模式 | 8 类多模块日常应用在全新 checkout 下完整走通项目工作流 | G4 / Workflow |
| [F11](#f11--linux-x86_64-原生交付与可靠性) | Linux x86_64 原生交付与可靠性 | `native-delivery` | 🟡 已实现待联合验证 | Clang + LLVM 23.1.4 C API × O0/O2、1/4/8 并行链接、Fixed Projects `21/21`、State/Store `16/128/56` | 纳入 8/8 日常程序双后端 O0/O2 与冷恢复；清理 `validation/alpha-coverage.json` 冲突残留 | G10.0 / G10.3 |
| [F12](#f12--alpha-综合验收与-8-类日常程序) | Alpha 综合验收与 8 类日常程序 | `release-acceptance` | ⚪ 未开始 | `verify-main.yml` + `alpha-acceptance.yml` 框架与既有回归门 | PR #138 合入 + 落地 8/8 日常应用 + 单一冻结提交全门复验与非作者源码独审 | G10.1 / G10.6 |

---

## 2. 按语言功能域详细目录（已完成 vs 待完成）

> 每章均将**当前已支持能力（已完成）**与**剩余差距（待完成）**分列，并以 **`Alpha 必选`** / **`Post-Alpha`** 明确区分是否阻断首个 Linux Alpha。

### F1 · 基础类型、聚合与模式匹配

- **Gate ID**：`values-and-patterns` · **规范**：`ZIL-SYNTAX`、`ZIL-MODES`、`ZIL-PACKAGE`、`ZIL-CLEANUP` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F1.1 | 标量、整数/浮点与显式转换 | ✅ 已闭环 | Alpha 必选 | `u8/u16/u32/u64/usize`、`i8/i16/i32/i64/isize`、`f32/f64`、`bool`、`char`、`unit`；同型运算、checked/wrapping 转换、无符号数组/切片越界检查 | 无（保持双后端 O0/O2 回归） |
| F1.2 | 聚合、错误处理与匿名联合 | ✅ 已闭环 | Alpha 必选 | `Record`、`Package`、`Tuple`、`Array`、`Slice`、`Enum`、`Option`、`ErrorUnion`（`E!T` 与 `?` 传播）、匿名联合 `T \| U`（含拥有载荷 Drop、规范排序与重复/非穷尽/双消费拒绝） | 无（`owned-sum-consumer` 全 32 组合已闭环） |
| F1.3 | 模式匹配与条件部分移动 | 🟡 已实现待联合验证 | Alpha 必选 | 命名/位置解构、记录模式简写/省略、`match` 守卫保留拥有 scrutinee、闭区间 `a..=b`、嵌套引用模式 `&pat`/`&mut pat`/`&&pat`、条件部分移动与 `drop_elab` 精确补齐 | 纳入 8 类日常程序（JSON/CSV/CLI Todo 等）多模块联合回归 |
| F1.4 | 深层混合模式与可选语法糖 | 🟠 部分实现 | Post-Alpha | 常用控制流与结构解构均已覆盖日常编码需求 | 更深多层地址/资源包嵌套解构；`match` 表达式直返、或模式 `A \| B`、带标签 `break` 等人体工学增强 |

### F2 · 泛型与 Trait API

- **Gate ID**：`generic-libraries` · **规范**：`ZIL-TRAIT`、`ZIL-MODULE`、`ZIL-RECEIVER` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F2.1 | 泛型参数、固有 `impl` 与 `const` 泛型 | ✅ 已闭环 | Alpha 必选 | 类型/生命周期/整数 `const` 泛型 `<#N: usize>`、固有 `impl`、具体类型与枚举静态关联函数、跨模块真实 nominal 身份保留 | 无 |
| F2.2 | Trait、Supertrait 与关联项 | 🟡 已实现待联合验证 | Alpha 必选 | Trait 定义与默认方法体、Supertrait 蕴含与循环拒绝、关联类型/常量绑定（`T: Iter<Item = U>`）与跨 trait 消歧、泛型接收者方法 | 随泛型容器/迭代在 8 类日常应用中双后端原生联验 |
| F2.3 | 泛型 Trait 方法、Blanket Impl 与定义期义务 | 🟠 部分实现 | Alpha 必选 / Post-Alpha | Trait 泛型方法、blanket impl `where` 校验、未用泛型体定义期义务与重叠/歧义诊断、`std.iter` 拥有/借用组合 | **Alpha 必选**：确认日常库所需的泛型容器与迭代组合零退化；**Post-Alpha**：更广 supertrait 泛型实参、一般 blanket 约束方程与开放泛型方法槽 |
| F2.4 | 类型/`Self` 依赖常量表达式与广义 Lending | 🟠 部分实现 | Post-Alpha | 有限 `Self` 依赖常量与 `Store<Item>` manager 普通拥有 Item lending 已合入 | 一般类型/`Self` 依赖常量求值与任意资源宿主 lending trait 方法 |

### F3 · 所有权、借用与状态协议

- **Gate ID**：`ownership-composition` · **规范**：`ZIL-ORIGIN`、`ZIL-ACCESS`、`ZIL-CAPTURE`、`ZIL-CONTRACT`、`ZIL-CLEANUP` · **总态**：🟠 部分实现

#### 3A · 所有权、借用与组合模型（历史 G3）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F3.1 | 逐字段借用与拥有资源包 | ✅ 已闭环 | Alpha 必选 | `Struct`/`Package`/`Tuple` 逐字段借用（`Carries::set_field`）、字段覆写释放旧 loan、多共享/独占借入叶、嵌套 qualified 子 `Package`、`move_out` 后补回与确定性清理 | 无 |
| F3.2 | 固定共享地址数组与快照 | ✅ 已闭环 | Alpha 必选 | 非零 `[&'a T; N]`（含 512 槽）、字段/常量/动态整数索引、普通 helper 传参返回、字段快照逐槽初始化核验（不复制 `Managed` 责任） | 空数组与独占数组复制（Post-Alpha） |
| F3.3 | 闭包捕获与调用契约 | ✅ 已闭环 | Alpha 必选 | 真实捕获 `Fn`/`FnMut`/`FnOnce`、外层上下文传递、路径敏感 `FnMut` 捕获及 `match`/`let`/`break` 补齐、逐 role 独立核验 | 无 |
| F3.4 | 可变借入存储与跨函数 Lending | 🟠 部分实现 | Post-Alpha | 有限 `Record`/`Package` 多地址叶、同型整对象从唯一实际输入返回、`Store<Item>` 普通 lending 窗口与拥有回调管线 | 多输入借用返回源、跨函数显式存储上界、任意嵌套可变载体与广义 affine lending 持有者 |

#### 3B · 状态协议与联合语义（历史 G6 — 当前最高优先专项）

> **架构红线**：全链路 `Typeck -> Body IR -> Analyze -> Verify -> Lower -> Native` 共享同一套语义事实（[`zil-semantics`](https://github.com/zil-lang/zil/tree/main/crates/zil-semantics)），**不为 `zil-check` 另造独立语义系统**；每个小 PR 须保持 **G6.2 `27/27`** 与 **Fixed Projects `21/21`** 零退化。

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G6.1 | 有限状态协议基础（内建 + 64 态用户协议） | ✅ 已闭环 | Alpha 必选 | 内建句柄与用户 64 态 `enum`/含 `state` 字段句柄的 `requires(state/apart)`、`provides(state/retired/on_pending/on_ready/on_ok/on_err)`、跨 `match` 分支精化、未标注调用方前置核验与 Raw/Wire 证明 | 超 64 态或泛型参数化状态域（Post-Alpha） |
| G6.2 | 投影失效上界、历史撤销 `H` 与嵌套资源别名 | ✅ 已闭环 | Alpha 必选 | `effects(invalidates(...))` 上界、`effects(rollback(...))` 历史撤销 `H`、完整类型图 `mut_cap` 扫描、`Managed`/`Desc` 自动描述与嵌套资源跨参数能力关联（**27/27 闭环**，PR #137） | 超 4 层嵌套字段路径、跨模块导出上界扩展（Post-Alpha） |
| **G6.3-B** | **联合语义 1：`Replace`** | 🟠 进行中（[PR #139](https://github.com/zil-lang/zil/pull/139)） | **Alpha 必选** | `zil-semantics::protocol` 单一权威模型、`Replace` 溯源分离、旧新值不共用历史 `H`、旧态与责任转入 `old`、新值清旧 `retired`、本地 `30/30` 全绿 | 等待 PR #139 远端 CI（含 LLVM 23.1.4 + Fixed Projects `21/21`）通过与独立审阅合入 |
| **G6.3-C** | **联合语义 2：`Store`** | ⚪ 未开始 | **Alpha 必选** | 既有 `StoreBorrow` 跨 `Desc` 别名 `LoanConflict` 已在 PR #111 落地 | `StorePush`/`StorePop` 区分成功与失败（OOM 返还原状态、空容器不伪造句柄）；`StoreBorrow` 借用不等于实际修改，仅实际写入/取出/替换触发状态转移与失效 |
| **G6.3-D** | **联合语义 3：`Task` / `Registration`** | ⚪ 未开始 | **Alpha 必选** | `Task`/`AsyncRegistration` 基础三态与 `FinishDestroy` 门控已落地 | `TaskCreate` 按条件成功建模（OOM 返还）；`Valid == false` **不代表** `Closed`；补齐 `TaskStart` 完整语义定义；联合 `AsyncRegistration` 五态与用户协议 |
| **G6.3-E** | **联合语义 4：`Cancel`** | ⚪ 未开始 | **Alpha 必选** | `PendingExit` 机器清理与 `TrapKind::Cancel` 控制流边已存在 | 取消边既不直接沿用调用前状态也不套用正常返回后置，保守保留取消点前所有可能已发生的副作用与所有权转移 |

### F4 · 动态抽象与异构派发

- **Gate ID**：`dynamic-abstraction` · **规范**：`ZIL-TRAIT`、`ZIL-ROUTE`、`ZIL-BYTE-STORAGE` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F4.1 | 借入 `dyn Trait` 与对象安全 | ✅ 已闭环 | Alpha 必选 | 借入 `&dyn Trait` / `&mut dyn Trait`、参数化 `dyn Trait<T>` 与 `Assoc = T`、object-safety 校验、静态 vtable 派发、共享/独占接收者与共享字段地址返回 | 无 |
| F4.2 | 显式拥有擦除存储 `OwnedDyn` | ✅ 已闭环 | Alpha 必选 | `OwnedDyn<dyn Trait, Context<'a>>`、共同借入 `Store<OwnedDyn<..., Context<'a>>>`（push/at/pop）、按值 `self: Self` 堆箱释放（零泄漏 + `double_free` 自检）、`OwnedDynCreate` 独占 Context 健全性、循环 `alloca` 提升 | 无（16 份 `dyn-context` + 40 万次循环分配已闭环） |
| F4.3 | `dyn Trait<'a>` 生命周期返回与扩展槽 | 🟠 部分实现 | Alpha 必选 / Post-Alpha | 基础借入/拥有 provider 双后端原生与 Raw/Wire 已闭环 | **Alpha 必选**：修复 `dyn Trait<'a>` trait 生命周期参数方法返回区域替换（`static-callback-getter-cycle-control.zil`）；**Post-Alpha**：有限泛型 method slot、未选 owner 错误联合捕获与非接收者来源 provider |

### F5 · 稳定资源与图生命周期

- **Gate ID**：`resource-graphs` · **规范**：`spec/checked-resource-graph.md` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F5.1 | 运行期可变图与循环/共享边 | ✅ 已闭环 | Alpha 必选 | `std.graph` runtime-sized 循环/共享边图、`ByteBuffer` 拥有载荷、分配失败精确返还与无 GC/RC 确定性回收 | 无 |
| F5.2 | 逻辑删除、槽复用与 Ticket 失效 | 🟡 已实现待联合验证 | Alpha 必选 | 逻辑删除/回收、有界 ticket 复用、旧 ticket 失效与双图同号边界（双后端 O0/O2 已过） | 结合“图依赖分析器”日常程序完成拓扑排序/环检测/影响面与 OOM 清理综合验收（`NodeKey` 跨图校验保持显式运行时合同） |

### F6 · 异步任务、挂起与取消

- **Gate ID**：`async-ownership` · **规范**：`ZIL-ASYNC`、`ZIL-CLEANUP` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F6.1 | 原生协程栈与 `Task` 运行时 | ✅ 已闭环 | Alpha 必选 | 单次 `calloc` `ucontext_t` 协程、4 KiB `PROT_NONE` 栈保护页与 `SEGV_ACCERR` 检测、`std.task.*`、`task_await`、跨模块 Direct worker、无显式类型头构造与匹配 | 无（Roam `task-result-combinations` `24/24` + `task-owner-transport` `24+9` 全绿） |
| F6.2 | 取消回滚、OOM 清理与拥有载荷 | ✅ 已闭环 | Alpha 必选 | `PendingExit` 取消后拥有结果单次 Drop、`EnumPartialDrop` 选择性枚举变体 Drop、嵌套任务级联关闭、`task_create`/`task_create_owned` OOM 返还与清理、逐路径 CFG `resume(transfer)` 格证明 | 无 |
| F6.3 | 任务协议联合与取消边健全性 | ⚪ 未开始 | **Alpha 必选** | `TaskRequestCancel`/`TaskPollClose`/`TaskFinishDestroy` 基础状态检查已落地 | 完成 **G6.3-D/E**（`TaskCreate` 条件成功、`Valid == false != Closed`、`TaskStart` 定义、取消边状态与副作用保留），并通过“异步任务调度器”日常程序验收 |
| F6.4 | 通用借入 Frame Port 与跨模块异步 ABI | 🟠 部分实现 | Post-Alpha | 多共享/独占输入与外部来源结果借用已支持 | 一般借入 frame port、`FnOnce` 挂起根精细协议区分、跨模块 `pub Suspend` 非标量 ABI、任务线程迁移 |

### F7 · 线程、互斥锁与原子操作

- **Gate ID**：`threads-and-synchronization` · **规范**：`spec/affine-mutex.md`、`ZIL-ASYNC` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F7.1 | 结构化线程与 `Mutex<T>` | ✅ 已闭环 | Alpha 必选 | 真实 `pthread_create`/`pthread_join` `ThreadScope`、参数化 `pthread_mutex_t` `Mutex<T>` 与 affine `Guard`（单次解锁与投影依赖）、拥有载荷与 OOM/init 失败清理、40 轮双线程 barrier + TLS 隔离 | 无 |
| F7.2 | 原子操作与内存序 | ✅ 已闭环 | Alpha 必选 | `i64`/`u64`/`usize` 原子与强 CAS、`MemoryOrder` 5 序及 `Sync*Ordered` 家族（Wire 43–47）、争用与错误次序原生对照 | 无 |
| F7.3 | `Transfer`/`Share` 安全边界与并发联验 | 🟡 已实现待联合验证 | Alpha 必选 / Post-Alpha | 保留 `Transfer`/`Share` trait、`thread_scope` 裸 `c_handle` 拒绝、`Mutex<Resource>` FFI Drop 拒绝 | **Alpha 必选**：与 HTTP 小服务/文件批处理/异步调度器联合并发验收；**Post-Alpha**：用户自定义泛型 `Transfer`/`Share` 自动派生与跨模块约束 |

### F8 · 外部接口、C ABI 与已登记回调

- **Gate ID**：`foreign-lifecycle` · **规范**：`ZIL-ASYNC`、Design §12.4、`RISKS.json` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F8.1 | C ABI、外部函数与回调桥接 | ✅ 已闭环 | Alpha 必选 | 10 整数 + `f32/f64` C 参数/返回、递归 POD Record C-ABI、`IntrinsicOp::CCallback`（Wire 49）、`c_null()`（Wire 48）、`ForeignHandle ==/!=`、glibc `errno` 桥接 | 非 Linux ABI 见下文 Windows 专项 |
| F8.2 | `Registration` / `AsyncRegistration` 生命周期 | 🟡 已实现待联合验证 | Alpha 必选 | `c_register`/`c_register_async` 五态 CAS 门控（`OpenIdle`..`Closed`）、`driver_lock` 串行化、`RISKS.json` 14 项 1-to-1 绑定、8 份 C-host `async-registration` 与 `raw-holder.zil` pthread 实测 | 随 **G6.3-D/E** 闭合与用户状态协议、取消边的联合语义 |
| F8.3 | 跨线程一般借入环境与外部 Waker Drain | 🟠 部分实现 | Post-Alpha | 同线程稳定拥有环境、密封跨线程环境、晚通知 gate 与 provider drain 已合入 | 一般跨线程借入捕获环境（`Address<Closure>`/`&mut`）与更广外部 waker 排空组合 |

### F9 · 标准库：文本、集合、迭代与系统 I/O

- **Gate ID**：`standard-library` · **规范**：`spec/pod-hash-collections.md`、`spec/owning-hash-collections.md`、`spec/numeric-text-and-buffered-io.md` · **总态**：🟠 部分实现

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F9.1 | 文本与数值格式化 | 🟠 部分实现 | **Alpha 必选** | `std.utf8`、`std.string`（构造/搜索/比较/编辑）、`std.text_edit`、`std.int`（10 整数十进制）、`std.float`（binary32/64 roundtrip） | **Alpha 必选**：`std.string` 常用切分/修剪/按行遍历 helper（`split`/`trim`/`lines`）；**Post-Alpha**：完整 Unicode 排版/grapheme |
| F9.2 | 容器、哈希集合与迭代 | 🟡 已实现待联合验证 | Alpha 必选 | `std.store`、`std.collections`（排序）、`std.iter`（惰性 `map`/`filter`/`collect`/`fold`/回调消费）、`pod_hash_*`/`i64_hash_*`/`string_map`、`owning_map`/`owning_hash_map`/`owning_hash_set`（26 个内嵌信任根模块主体） | 随 8 类日常程序验证 OOM/拥有值组合；高阶集合算法与跨过程 `apart` 精度属 Post-Alpha |
| F9.3 | 流、缓冲 I/O、文件系统与路径 | 🟠 部分实现 | **Alpha 必选** | `std.io`（含 `stderr`/`print*`/`EINTR` 重试）、`std.buffered_io`（固定容量 Reader/Writer、短读写与精确 suffix 重试）、`std.fs` 基础读写（`read_to_bytes`/`write_bytes`/`exists`/`remove_file`） | **Alpha 必选**：`std.fs` 目录遍历/建目录/重命名/元数据（`read_dir`/`mkdir`/`rename`/`stat`）及 `std.path` 路径拼接/规范化 |
| F9.4 | 进程环境、命令行参数与时间 | 🟠 部分实现 | **Alpha 必选** | `std.env`（`get`/`current_dir`/`exit`）、`std.time` | **Alpha 必选**：`std.env.args`（`argc`/`argv` 命令行参数安全暴露，支撑 CLI/搜索/批处理应用） |
| F9.5 | 网络基础库（POSIX TCP） | ⚪ 未开始 | **Alpha 必选** | 底层 C ABI 与 `errno` 桥接已就绪 | **Alpha 必选**：`std.net` POSIX TCP Socket（`TcpListener`/`TcpStream`、显式关闭/错误处理与资源清理），支撑 HTTP 小服务与客户端 |

### F10 · 编译期编程与项目工作流

- **Gate ID**：`project-workflow` · **规范**：`ZIL-COMPTIME`、`ZIL-MODULE` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F10.1 | 编译期计算、反射与 `MetaBlock` | 🟡 已实现待联合验证 | Alpha 必选 / Post-Alpha | 泛型 `Type` 返回、CT 反射与全序号身份、真实 Definition 上下文与物理字节计费、`MetaBlock` + `yield`（经 verified `zil-interp` 执行并拒绝非法拥有载荷 `E-RESOURCE-TYPE`） | **Alpha 必选**：保持 CT 回归零退化；**Post-Alpha**：`#if`/`#for` 元语句、rich callable、ambient `Type` 导出与一般成员/生成作用域 |
| F10.2 | 项目清单、依赖锁与 CLI 命令 | 🟡 已实现待联合验证 | Alpha 必选 | `zil check` / `build` / `run` / `test` / `lock`、项目清单与可复现路径依赖锁、每 importer 独立模块身份、Unit/Bool/I32 测试发现与失败反例 | 8 类多模块日常应用在全新 checkout 下完整走通 `lock` → `check` → `build` → `run` → `test` |
| F10.3 | 常驻模式与诊断契约 | ✅ 已闭环 | Alpha 必选 | CLI 常驻模式（复用解析并每次独立 Verify）、严格区分 `Success`/`SafeReject`/`Unsupported`/`ResourceLimit`/`InfraError`、`--json` 结构化诊断 | 无 |

### F11 · Linux x86_64 原生交付与可靠性

- **Gate ID**：`native-delivery` · **规范**：`docs/guide/build.md`、`validation/native-language-composition.json` · **总态**：🟡 已实现待联合验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F11.1 | 双原生后端与并行链接 | ✅ 已闭环 | Alpha 必选 | Clang 与 LLVM 23.1.4 C API（Stage 1 Builder 桥接）× O0/O2、1/4/8 单元并行链接、Fixed Projects `21/21`（60 构建 / 152 比较）、State/Store `16/128/56` | 无（保持双后端 O0/O2 零退化） |
| F11.2 | 大值 ABI、栈可靠与故障清理 | ✅ 已闭环 | Alpha 必选 | 原大数组 O0 崩溃与默认 2 MiB 深栈修复、`-Wl,--wrap=calloc/realloc/free` 逐分配点 OOM 与零泄漏/`double_free` 自检 | 纳入 8 类日常程序 OOM/早退资源清理实测 |
| F11.3 | 独立冷恢复与验收元数据清洁 | 🟠 部分实现 | **Alpha 必选** | `alpha-acceptance.yml` fail-closed 收据汇总框架已就绪 | 清理 `validation/alpha-coverage.json` 合并冲突残留标记；在 8/8 日常应用落地后执行独立冷恢复验证 |

### F12 · Alpha 综合验收与 8 类日常程序

- **Gate ID**：`release-acceptance` · **规范**：`validation/language-alpha-profile.json` · **总态**：⚪ 未开始

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| F12.1 | Alpha 合同升级 | 🟠 进行中（[PR #138](https://github.com/zil-lang/zil/pull/138)） | **Alpha 必选** | `validation/language-alpha-profile.json` 已升级对齐 12 基础能力 + G6.3 + 8 日常应用门，远端 CI 3/3 绿 | 合入 PR #138 |
| F12.2 | 8 类真实日常程序综合验收 | ⚪ 未开始 | **Alpha 必选** | 验收口径与 8 类目标应用已定型（见下表） | 全部 8 类多模块 ZIL 真实应用落地并通过双后端 O0/O2 + OOM/早退清理 + 最近安全负例对照 |
| F12.3 | 冻结候选独立签发 | ⚪ 未开始 | **Alpha 必选** | `verify-main.yml` / `alpha-acceptance.yml` 自动化链路就绪 | 单一冻结提交上 `12/12` + `G6.3` + `8/8` + Raw/Wire 对照 + 非作者精确源码独审 + 冷恢复共同收据 |

#### 8 类真实日常程序综合验收矩阵（F12.2 明细）

> 全部须为真实普通多模块 ZIL 项目（非单测拼接/玩具脚本），走标准 `zil check/build/run/test`，覆盖 **Linux x86_64 Clang & LLVM API × O0/O2**、非零功能断言、错误/OOM/早退清理与至少 1 组最近安全负例。

| # | 应用（Gate ID） | 主要覆盖功能域 | 阻塞前置 | 当前态 |
| --- | --- | --- | --- | --- |
| 1 | **文本统计与文件搜索** (`text-stats-and-file-search`) | F1、F9（UTF-8/`String`/`buffered_io`/`fs`/`env.args`）、F10 | `std.env.args`、`std.fs.read_dir`、`std.string` 切分 | ⚪ 未开始 |
| 2 | **CSV 处理工具** (`csv-processor`) | F1、F2、F9（`String`/`Store`/`OwningHashMap`/`int`/`float`/`buffered_io`） | `std.string` 行/分隔辅助（可先行落地） | ⚪ 未开始 |
| 3 | **JSON 解析与序列化器** (`json-processor`) | F1（递归 `Enum`/拥有聚合）、F3、F9、F11 | 递归拥有树/容器组合实测与 OOM 清理 | ⚪ 未开始 |
| 4 | **CLI Todo 应用** (`cli-todo-app`) | F3（状态协议 + `Replace`/`Store`）、F9（`fs`/`env.args`/`time`）、F10 | `std.env.args`、G6.3-B/C | ⚪ 未开始 |
| 5 | **HTTP 小服务与客户端** (`http-mini-service-and-client`) | F7、F8、F9（TCP Socket + 缓冲 I/O）、F4 | `std.net` POSIX TCP Socket | ⚪ 未开始 |
| 6 | **文件批处理工具** (`file-batch-processor`) | F9（目录/路径/重命名/错误恢复）、F7、F3 | `std.env.args`、`std.fs` 扩展、`std.path` | ⚪ 未开始 |
| 7 | **图依赖分析器** (`graph-dependency-analyzer`) | F5（`std.graph` 环检测/拓扑排序/影响面）、F2、F9 | 基于现有 `std.graph` 组装端到端应用与故障清理门 | ⚪ 未开始 |
| 8 | **异步任务调度器** (`async-task-scheduler`) | F6、F8、F3（G6.3-D/E 任务与取消协议）、F4 | G6.3-D/E 落地 | ⚪ 未开始 |

---

## 3. 非 Alpha 阻断项与独立专项

> 以下专项**不阻断首个 Linux Alpha 发布**，但保持独立追踪与验收门；任何 Alpha 验收结果不得替代以下声明。

| 历史 ID | 专项名称 | 状态 | 已完成 | 待完成 / 退出门 |
| --- | --- | --- | --- | --- |
| **G10.5** | **编译与运行性能突破专项** | 🟠 进行中 | LLVM 23.1.4 Stage 1 C API 桥接已合入；独立研究分支 `research/full-push-native`、`integrate/scalar-codegen` 推进中 | **Stage 2**（去除 `body_structured.rs` 文本桥接，直接 `VerifiedModule -> model::Module` 降级）；同语义 Rust 对照安全/运行/内存/编译/维护成本及至少一项可重复净突破 |
| **G10.4** | **Windows x64 原生支持** | ⚪ 未开始 | 无原生 Windows 执行收据 | 原生 Windows x64 CLI、ABI、Runtime、文件 I/O、线程/Task 与真实 Windows CI 验收（交叉构建或 Wine 不算通过） |
| **G9.2 / G9.4–G9.6** | **ZUI 窗口、渲染与应用层** | 🟠 部分实现 | **G9.2 已合入**：整数布局、RGBA 软件合成、IME preedit 提交/取消与失败保留普通库；**G9.4** 已有初步窗口实验 | **G9.4** 真实 OS 窗口/事件循环/OS IME；**G9.5** GPUI 式 GPU 场景首帧与字体塑形；**G9.6** 实体/组件与确定性关闭端到端应用合流 |
| **G10.6** | **终态产品门** | ⚪ 未开始 | [`validation/product-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/product-gates.json) 合同已固定 | 零 safe UB、Rust 相对性能/编译门、可重复净突破、B0 与 ZUI 终态联合冻结独立复验 |

---

## 4. 看板治理与验收红线

1. **单一真相与防退化**：子项通过不自动关闭父能力；安全边界缺陷永远 P0 阻断；所有改动必须保持 G6.2 `27/27`、Fixed Projects `21/21`、State/Store `16/128/56`、Roam `42/42 + 24/24` 零退化。
2. **反虚高计量**：全仓 `tests/**/*.zil` 的 `zil check --json` `status == "Success"` 计数**不是正例通过率或功能完成率**；每批按精确文件路径、原始输入哈希与预期结果分开统计，不以提高预算或 `Unsupported` 充当通过。
3. **独立 Verify 红线**：`zil-check` 必须独立核验 Source 与 Raw/Wire 有效证书并拒绝伪造证书，不得信任求解器结论，也不得为 Verify 另造与全链路脱节的语义系统。
4. **文档维护约定**：每次关键 PR 合入或新缺口确认时，仅同步更新「第 0 节在途表」「第 1 节总览状态」与对应功能域的「已完成 / 待完成」单元格，不在 `goal.md` 堆积流水账日志；详细施工清单可暂存任务分支 `.task/<ID>.md`（合入 `main` 前删除）。
