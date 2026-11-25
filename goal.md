# ZIL 实时任务看板

> 更新：2026-11-25（Asia/Shanghai） · 按**语言功能分类**组织，仅追踪 **Alpha** 差距、未闭合能力、优先级、依赖与可执行退出门。已合入能力以主树 [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) 与 [`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) 为准；历史按阶段/施工步骤快照固定于 [`goal@e8faf57b2`](https://github.com/zil-lang/zil/blob/e8faf57b2/goal.md) 与 [`goal@e630d5f03`](https://github.com/zil-lang/zil/blob/e630d5f03/goal.md)。

[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md)

## 1. 当前目标与 Alpha 口径

- **主线基线**：[`main@08da36ff0`](https://github.com/zil-lang/zil/commit/08da36ff0)（开放 PR：0；G6.2 既定 P0 门 27/27 合同语义测试已随 [PR #137](https://github.com/zil-lang/zil/pull/137) 正式关闭；Fixed Projects 21/21、State/Store 16/128/56、Roam 42/42 + 24/24 全绿）。
- **版本命名与完整度口径**：首个发布统一命名 **`Alpha`**，目标平台为 **Linux x86_64**；功能完整度直接对标 [`validation/language-release-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/language-release-gates.json) 全部 **12/12 基础语言功能域** + **G6.3 状态协议联合语义** + **8/8 真实日常程序综合验收**（合同对齐见 [`validation/language-alpha-profile.json`](https://github.com/zil-lang/zil/blob/main/validation/language-alpha-profile.json)）。
- **非 Alpha 阻断边界**：Windows x64 原生支持、ZUI（窗口/渲染/应用层）、编译与运行性能突破专项（独立分支推进）不阻断首个 Linux Alpha，但各自的独立门禁与 [`validation/product-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/product-gates.json) 终态产品门仍持续有效。
- **总体阻塞与优先推进**：**无 P0 阻塞**。当前第一优先推进 **G6.3 状态协议联合语义**（按 `Replace -> Store -> Task/Registration -> Cancel` 顺序小 PR 分阶段落地），同步补齐 8 类日常程序所需的标准库基础设施（`std.env.args`、目录遍历/路径、`std.net` TCP Socket 等）并落地 8/8 日常应用综合验收。
- **强制回归门禁与统一计量**：全链路改动须保持 G6.2 原有 **27/27** 合同语义测试、Fixed Projects **21/21**、State/Store **16/128/56**、Roam **42/42 + 24/24** 零退化。全仓 `tests/**/*.zil` `zil check --json` `status == "Success"` 计数**不是正例通过率或功能完成率**；每批按精确文件路径、原始输入哈希与预期结果分开统计，保留 Clang/LLVM API × O0/O2、Raw/Wire 独立 `zil-check`、故障/清理与远端 CI 必需门，不以提高预算或 `Unsupported` 充当通过。
- **状态口径**：`缺失`｜`部分实现`｜`已实现待联合验证`｜`已闭环`。子项通过不自动关闭父能力；安全边界缺陷永远 P0 阻断；详细清单可暂存任务分支 `.task/<ID>.md`，合入 `main` 前移除。

## 2. 基础语言功能域（12/12 对标 `language-release-gates.json`）

| 功能域（Gate ID） | 实现态 | 当前已落地能力 | Alpha 剩余差距 | 优先级 | Alpha 验收门 |
| --- | --- | --- | --- | --- | --- |
| **标量、聚合与模式** (`values-and-patterns`) | 已实现待联合验证 | 10 种整数 + `f32/f64` checked/wrapping 转换；数组/切片无符号越界检查；`Record`/`Package`/`Enum`/`Option`/`ErrorUnion`/匿名联合 `T \| U`；`?` 传播、`match` 守卫保留拥有 scrutinee、记录模式简写/省略、闭区间 `a..=b`、嵌套引用模式 `&pat`/`&mut pat`/`&&pat` 与条件部分移动/补齐 `drop_elab` | 更深多层地址/资源包嵌套模式；可选日常语法糖（`match` 表达式、或模式、带标签 `break`）及 8 类日常程序综合覆盖 | P1 | 双后端 O0/O2 正负例 + 8 类日常程序联合回归无退化 |
| **泛型与 Trait API** (`generic-libraries`) | 部分实现 | 固有 `impl`、具体类型/枚举静态关联函数、supertrait 蕴含与循环拒绝、关联类型绑定与跨 trait 消歧、`T: Iter<Item = U>`、trait 泛型方法、blanket impl `where` 校验、整数 const 泛型 `<#N: usize>`、`std.iter` 拥有/借用迭代组合 | 依赖 `Self`/类型参数的 const 表达式、更广 supertrait 泛型实参、一般 blanket 约束方程与更广拥有 lending trait 方法、开放泛型方法槽 | P1 | 未用定义期义务与循环/重叠负例 + 泛型容器/迭代在日常程序中双后端原生全绿 |
| **拥有与借用数据模型** (`ownership-composition`) | 部分实现 | `Struct`/`Package`/`Tuple` 逐字段借用跟踪 `Carries::set_field`、字段覆写释放旧 loan、嵌套 qualified 子 `Package`、固定共享地址数组（含 512 槽/动态索引/快照初始化）、路径敏感 `FnMut` 捕获与 `match`/`let`/`break` 补齐、64 态用户协议（G6.1）与投影失效/历史撤销 `H`/嵌套资源跨参数能力关联（G6.2 27/27 闭环） | **G6.3 核心联合语义（见第 3 节：`Replace`/`Store`/`Task-Registration`/`Cancel`）**；跨函数显式存储上界/更广 lending 持有者、多输入借用返回源、超 4 层嵌套投影路径 | **P0/P1** | G6.2 27/27 零退化 + G6.3 全 4 阶段双后端原生与 Raw/Wire 独立 `zil-check` 闭环 |
| **动态抽象与异构派发** (`dynamic-abstraction`) | 部分实现 | 借入 `dyn`、参数化 `dyn Trait<T>` / `Assoc = T`、object-safety 校验、`OwnedDyn<dyn Trait, Context<'a>>`、共同借入 `Store<OwnedDyn<..., Context<'a>>>`、按值 `self: Self` 堆箱释放（`-Wl,--wrap=calloc/free` 零泄漏 + `double_free` 自检）、`OwnedDynCreate` 独占 Context 健全性、循环 `alloca` 提升 | `dyn Trait<'a>` trait 生命周期参数方法返回区域替换（`static-callback-getter-cycle-control.zil`）、有限泛型 method slot 派发、未选 owner 错误联合捕获与更广非接收者来源 provider | P1 | 16 份 `dyn-context` + 零泄漏探针 + 生命周期返回替换正负例双后端原生与 Raw/Wire 全绿 |
| **稳定资源与图生命周期** (`resource-graphs`) | 已实现待联合验证 | `std.graph` runtime-sized 循环/共享边图、`ByteBuffer` payload 与分配失败返还、逻辑删除/回收、有界 ticket 复用与旧 ticket 失效、双图同号边界（双后端 O0/O2 原生通过） | 结合“图依赖分析”日常程序完成端到端拓扑排序/环检测/影响面与 OOM 清理综合验收（`NodeKey` 跨图校验保持显式运行时合同边界） | P1 | 既有 `std.graph` 门 + 图依赖分析日常程序双后端 O0/O2 与故障清理全绿 |
| **异步任务与取消** (`async-ownership`) | 部分实现 | 单次 `calloc` `ucontext_t` 原生协程、4 KiB `PROT_NONE` 栈保护页与 `SEGV_ACCERR` 检查、`PendingExit` 取消回滚（拥有结果单次 Drop、枚举活跃变体 Drop、嵌套任务级联关闭）、`task_create`/`task_create_owned` OOM 清理、`std.task.*`、`task_await`、`EnumPartialDrop`、Roam `task-result-combinations` 24/24 与 `task-owner-transport` 24+9 | **G6.3-D/E**（`TaskCreate` 条件成功、`Valid == false` 不代表 `Closed`、`TaskStart` 定义补齐、取消边状态与副作用/所有权转移健全模型）；一般借入 frame port、`FnOnce` 挂起根精细协议区分、跨模块 `pub Suspend` 非标量 ABI、异步任务调度日常程序验收 | **P1** | G6.3-D/E 正负例 + Roam 24/24 零退化 + 异步任务调度日常程序双后端 O0/O2 全绿 |
| **线程、互斥锁与原子** (`threads-and-synchronization`) | 已实现待联合验证 | 真实 `pthread_create`/`pthread_join` `ThreadScope`、真实 `pthread_mutex_t` `Mutex<T>`/affine `Guard`、`MemoryOrder` 5 序与 `Sync*Ordered` 原子家族（Wire 43–47）、`Transfer`/`Share` 保留 trait、`thread_scope` 裸 `c_handle` 与 `Mutex<Resource>` FFI Drop 拒绝、40 轮双线程 barrier + TLS 隔离 | 用户自定义泛型对 `Transfer`/`Share` 的自动派生与跨模块约束；更广跨线程捕获运输；与 HTTP/异步调度日常程序联合并发验证 | P1 | 真实竞争/有序原子与安全负例 + 并发日常程序双后端 O0/O2 全绿 |
| **外部接口与已登记回调** (`foreign-lifecycle`) | 部分实现 | `c_register`/`c_register_async` 五态 CAS 门控（`OpenIdle`..`Closed`）、`driver_lock` 串行化、`RISKS.json` 14 项 1-to-1 绑定、`CCallback`（Wire 49）、`c_null()`（Wire 48）、`ForeignHandle ==/!=`、递归 POD C-ABI、glibc `errno` 桥接、8 份 C-host `async-registration` + `raw-holder.zil` pthread 实测 | **G6.3-D/E**（`AsyncRegistration` 与用户协议及取消边联合）；一般跨线程借入捕获环境（`Address<Closure>`/`&mut`）与外部 waker/晚通知 drain 扩展 | P1 | 14 项风险矩阵 + G6.3-D/E 正负例 + C-host 双后端原生与 Raw/Wire 全绿 |
| **标准库：文本、集合、迭代与系统 I/O** (`standard-library`) | 部分实现 | 26 个内嵌信任根模块：`utf8`/`string`/`text_edit`/`int`/`float`/`store`/`collections`/`iter`/`pod_hash_*`/`i64_hash_*`/`string_map`/`owning_*`/`graph`/`io`（含 `stderr`/`print*`/`EINTR` 重试）/`buffered_io`/`fs`（`read_to_bytes`/`write_bytes`/`exists`/`remove_file`）/`env`（`get`/`current_dir`/`exit`）/`time` | **8 类日常程序前置缺口**：① `std.env` 命令行参数（`args`/`argc`/`argv`）；② `std.fs` 目录遍历/建目录/重命名/元数据（`read_dir`/`mkdir`/`rename`/`stat`）与 `std.path`；③ `std.net` POSIX TCP Socket（`TcpListener`/`TcpStream`）；④ `std.string` 日常切分/修剪/按行 helper 与常用数据格式支撑 | **P1** | 新增标准库接入内嵌信任根，含 OOM/短读写/错误清理双后端 O0/O2 与 Raw/Wire 验收，支撑 8/8 日常程序 |
| **编译期编程与项目工作流** (`project-workflow`) | 已实现待联合验证 | `zil check`/`build`/`run`/`test`/`lock`、项目清单与路径依赖锁、CT `MetaBlock` + `yield`（经 verified `zil-interp` 执行与非法拥有载荷 `E-RESOURCE-TYPE` 拒绝）、Unit/Bool/I32 测试发现与失败反例 | 8 类多模块日常应用从全新 checkout 完整走通 `lock`/`check`/`build`/`run`/`test`；`#if`/`#for` 元语句与 ambient `Type` 导出按边界跟进 | P1 | 全新 checkout 下 8/8 日常应用工作流非零执行与失败诊断门全绿 |
| **Linux x86_64 原生交付** (`native-delivery`) | 已实现待联合验证 | Clang 与 LLVM 23.1.4 C API（Stage 1 Builder 桥接）× O0/O2、1/4/8 单元并行链接、Fixed Projects 21/21（60/60 构建、152/152 比较）、State/Store 16/128/56、`alpha-acceptance.yml` fail-closed 汇总 | 8/8 日常程序纳入双后端 O0/O2 + OOM/清理 + 独立源码冷恢复验收；清理 `validation/alpha-coverage.json` 冲突残留标记（44 行缺失 stdin 行维持合同 archival 口径） | P1 | 双后端 × O0/O2 同源码执行一致、零泄漏与冷恢复收据完备 |
| **发布综合验收** (`release-acceptance`) | 缺失 | `verify-main.yml` + `alpha-acceptance.yml` 自动化框架、既有回归门（G6.2 27/27、Fixed Projects 21/21、Roam 42+24、State/Store 16/128/56） | 升级 `validation/language-alpha-profile.json` 对齐 12/12 + 8/8；全部缺口闭合后，在单一冻结候选提交完成 12/12 + G6.3 + 8/8 + Raw/Wire 对照 + 非作者精确源码独审 + 冷恢复签发 | P1 | 单一冻结提交上 `language-alpha-profile.json` 全门非零通过并生成共同收据 |

## 3. 优先专项：状态协议联合语义（G6.3）

**架构红线**：全链路 **`Typeck -> Body IR -> Analyze -> Verify -> Lower -> Native`** 定位并共享同一套公开语义事实，**不为 `zil-check`（Verify）另造独立语义系统**；每个小 PR 必须保持 **G6.2 原有 27/27 合同语义测试**与 **Fixed Projects 21/21** 零退化，并包含双后端（Clang / LLVM API × O0/O2）原生正例与 Source / Raw / Wire 有效证书负例。

| 顺序 / ID | 范围与修订语义规则 | 状态 | 依赖 | 验收门 |
| --- | --- | --- | --- | --- |
| **1. G6.3-B (`Replace`)** | 协议句柄或含协议句柄的聚合/拥有字段 `Replace`：旧值与新值**绝不共用历史 `H`**；旧资源的状态与未履行责任完整转入 `old` 返回值，新资源仅继承自身原有状态与责任，并清除旧槽上的 retired/departed 历史；覆盖同槽替换、跨字段替换、替换后对 `old` 履行收尾与对新值继续推进状态机 | 进行中 | G6.2（已闭环） | 全链路共享语义事实 + G6.2 27/27 零退化 + Replace 跨状态/历史串扰 Source 与 Raw/Wire 负例 + 双后端 O0/O2 原生 |
| **2. G6.3-C (`Store`)** | `Store<Handle>` / 含协议字段元素的拥有容器：`Push`/`Pop` 必须**区分成功与失败路径**（`Push` OOM 时所有权与原状态返还给传入值，容器状态不变；`Pop` 空容器时不伪造协议句柄）；容器只读/可变借用（`StoreBorrow`/`at`）**不等于实际修改**，仅实际写入/取出/替换才触发协议状态转移与投影失效 | 待执行 | G6.3-B | `Push` OOM 恢复、`Pop` 空容器与取出后责任履行、借用非修改对照正负例（Source + Raw/Wire + 双后端 O0/O2） |
| **3. G6.3-D (`Task` / `Registration`)** | 异步任务与外部登记与状态协议联合：`TaskCreate` 按**条件成功**建模（OOM 失败分支返还所有权与原协议状态，不污染成功态）；`TaskValid == false` / `RegistrationValid == false` **不代表 `Closed`**（不可凭 `Valid == false` 豁免关闭/回收责任）；补齐 **`TaskStart`** 的完整语义定义与状态迁移规则；联合 `AsyncRegistration` 五态与用户协议 | 待执行 | G6.3-C | `TaskCreate` OOM 返还、`Valid == false` 伪关闭拒绝、`TaskStart` 全链路定义与联合原生 + Raw/Wire 负例 |
| **4. G6.3-E (`Cancel`)** | 异步取消边（`TrapKind::Cancel` / `PendingExit` / `TaskPollClose`）与状态协议联合：取消边**既不能直接套用调用前状态，也不能直接套用正常返回后置**；必须保守保留取消点前**所有可能已发生的副作用与所有权转移**，防止取消后把已转移/已修改/可能部分推进的协议句柄当成未变更旧态复用或漏清理 | 待执行 | G6.3-D | 取消前已修改/已转移协议句柄复用拒绝、嵌套取消清理正负例（Source + Raw/Wire + 双后端 O0/O2） |

## 4. 真实日常程序综合验收（8/8 Alpha 应用门）

全部 8 类程序须为真实普通多模块 ZIL 源码（非合成玩具/非单测拼接），通过标准项目工作流（`zil check`/`build`/`run`/`test`）在 **Linux x86_64 Clang & LLVM API × O0/O2** 执行，包含非零功能断言、错误/OOM/早退资源清理以及至少 1 组最近安全负例对照。

| # | 应用类别（Gate ID） | 核心验证功能域 | 前置缺口 | 状态 |
| --- | --- | --- | --- | --- |
| 1 | **文本统计与文件搜索** (`text-stats-and-file-search`) | `values-and-patterns`、`standard-library`（UTF-8/`String`/`io`/`buffered_io`/`fs`/`env.args`）、`project-workflow` | `std.env.args`、`std.fs.read_dir`、`std.string` 切分 helper | 待执行 |
| 2 | **CSV 处理工具** (`csv-processor`) | `values-and-patterns`、`generic-libraries`、`standard-library`（`String`/`Store`/`OwningHashMap`/`int`/`float`/`buffered_io`） | `std.string` 行/分隔辅助（可先在库/应用层落地） | 待执行 |
| 3 | **JSON 解析与序列化器** (`json-processor`) | `values-and-patterns`（递归 `Enum`/拥有聚合/模式）、`ownership-composition`、`standard-library`、`native-delivery` | 递归拥有树/容器组合实测与 OOM 清理验证 | 待执行 |
| 4 | **CLI Todo 应用** (`cli-todo-app`) | `ownership-composition`（状态协议 + `Replace`/`Store`）、`standard-library`（`fs`/`env.args`/`time`）、`project-workflow` | `std.env.args`、G6.3-B/C | 待执行 |
| 5 | **HTTP 小服务与客户端** (`http-mini-service-and-client`) | `threads-and-synchronization`、`foreign-lifecycle`、`standard-library`（TCP Socket + 缓冲 I/O）、`dynamic-abstraction` | `std.net` POSIX TCP Socket 基础库 | 待执行 |
| 6 | **文件批处理工具** (`file-batch-processor`) | `standard-library`（目录遍历/路径/重命名/错误恢复）、`threads-and-synchronization`、`ownership-composition` | `std.env.args`、`std.fs` 目录/重命名/元数据、`std.path` | 待执行 |
| 7 | **图依赖分析器** (`graph-dependency-analyzer`) | `resource-graphs`（`std.graph` 环检测/拓扑排序/影响面）、`generic-libraries`、`standard-library` | 基于现有 `std.graph` 组装端到端应用与故障清理门 | 待执行 |
| 8 | **异步任务调度器** (`async-task-scheduler`) | `async-ownership`、`foreign-lifecycle`、`ownership-composition`（G6.3-D/E 任务与取消协议）、`dynamic-abstraction` | G6.3-D/E 落地 | 待执行 |

## 5. 非 Alpha 阻断项与独立专项

以下事项**不阻断首个 Linux Alpha 发布**，但保持各自独立追踪与验收标准，不得以 Alpha 通过代替其声明：

| 专项 | 范围与现状 | 状态 | 独立推进边界 / 退出门 |
| --- | --- | --- | --- |
| **编译与性能突破专项** (`G10.5`) | 与 Rust 同语义的运行/内存/编译/维护成本及至少一项可重复净突破（含 **G10.5.1.16 Stage 2**：去除 `body_structured.rs` 文本桥接，直接 `VerifiedModule -> model::Module` 降级与同口径 O2 性能门） | 进行中 | 独立分支（`research/full-push-native`、`integrate/scalar-codegen`）持续推进，不阻塞普通语言线与 Linux Alpha |
| **Windows x64 原生支持** (`G10.4`) | 原生 CLI、ABI、Runtime、文件 I/O、线程/Task 与实际 Windows CI | 待执行 | 须真实 Windows x64 原生执行验收；交叉构建或 Wine 不算验收 |
| **ZUI 窗口、渲染与应用层** (`G9.4`–`G9.6`) | 真实 OS 窗口/事件循环/OS IME（`G9.4` 部分实现）、GPUI 式 GPU 场景与字体塑形首帧（`G9.5`）、实体/组件与确定性关闭合流（`G9.6`） | 部分实现 / 待执行 | 首个真实窗口输入关闭与 GPU 首帧分别验收，不阻断基础语言 Alpha |
| **终态产品门** (`G10.6`) | [`validation/product-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/product-gates.json)（零 safe UB、Rust 相对性能/编译门、可重复净突破、B0 与 ZUI 终态联合验收） | 待执行 | 独立于 Alpha 功能门，须全门冻结独立联合复验 |
