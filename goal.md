# ZIL 实时任务看板

> 更新：2026-11-23（Asia/Shanghai） · 唯一职责：仅追踪**未完成任务、优先级、依赖与可执行退出门**，不记录历史施工流水账。已合入能力以主树 [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) 与 [`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) 为准；精简前历史快照固定于 [`goal@e630d5f03`](https://github.com/zil-lang/zil/blob/e630d5f03/goal.md)。

[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md)

## 1. 当前目标与里程碑

- **主线基线**：[`main@90312badb`](https://github.com/zil-lang/zil/commit/90312badb)（开放 PR：0；Wave 3、PR #123/#124 及 L2 G5.3 [PR #125](https://github.com/zil-lang/zil/pull/125) 均已 4/4 远端 CI 全绿合入）
- **当前阶段**：L1–L4 核心能力及 P0 安全收口（[PR #96](https://github.com/zil-lang/zil/pull/96)–[#114](https://github.com/zil-lang/zil/pull/114)）、**方向 1 + 方向 2 基础语言可用性**（[PR #115](https://github.com/zil-lang/zil/pull/115)–[#119](https://github.com/zil-lang/zil/pull/119)）、Wave 3 与审核收口（[PR #120](https://github.com/zil-lang/zil/pull/120)–[#124](https://github.com/zil-lang/zil/pull/124)）以及 **L2 G5.3**（[PR #125](https://github.com/zil-lang/zil/pull/125)：借入 provider `OwnedDyn<dyn Trait, Context<'a>>` 共同借入 `Store<OwnedDyn<..., Context<'a>>>`、`StorePop` 解包、`OwnedDynCreate` 独占/可写地址 Context 健全性、循环 `alloca` 提升、`let` 解构 3 项回归）均已 4/4 远端 CI（含 `alpha-acceptance`）全绿合入 `main`。
- **总体阻塞**：已验收的既有安全门均已闭合（不等同于对全语言所有权与控制流模型的完整形式化证明）；Fixed Projects 21/21（17 原项目 + 4 预期拒绝、60/60 原生构建、152/152 比较）全绿。
- **下一发布门**：按原 P0 优先级推进 L2 剩余项 **G6.1（用户定义状态协议前后置约束与控制流/静态验证一致性）**、**G6.2（投影失效、历史撤销与导出上界）**；编译性能专项保持独立。
- **状态与退出规则**：`进行中`｜`部分实现`｜`待执行`｜`待排期`。① 已证明完全实现即从主表移除；② 含远期扩展的父任务将已闭合部分固化到 `supported-language.md`，余项拆新任务；③ 无真实消费者或竞争收益则显式关闭；④ 安全边界缺陷永远 P0 阻断；⑤ 详细验收清单可随任务分支放在 `.task/<ID>.md`，合入 `main` 前移除，仅留正式合同与测试。

## 下一阶段施工安排（以 main@90312badb 为基线）

L2 G5.3（PR #125）已合入。后续并行语言 PR 必须在实际合并目标上执行 post-merge 编译及 Fixed Projects，不能只凭独立 PR 绿灯宣告集成成功。

**统一计量及安全门**：全仓 `tests/**/*.zil` `zil check --json` `status == "Success"` 计数**不是正例通过率或功能完成率**。每批须以精确文件路径、原始输入哈希和预期结果分开统计新增合法通过、已有合法回归、错误准入、预期拒绝和 ResourceLimit，不得仅凭文件名判负例。保留当前 Alpha 收据、Clang/LLVM API × O0/O2、Raw/Wire、清理/故障与远端 CI 必需门；失败不以提高预算或 Unsupported 作为通过。

## 2. L1 · 基础语言与类型组合

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G1.4 | 剩余：更深多层地址/资源包复杂组合；**核心闭环已合入**（PR #118：嵌套引用模式 `&pat`/`&mut pat`/`&&pat`、`Package`/`Record`/`Tuple`/`ErrorUnion` 条件部分移动与补齐 `drop_elab`、拥有 `Record` 字段 Drop 修复，6 份就绪 fixture 全过） | 部分实现 | P1 | 双后端 O0/O2 原生正例 + 部分移动/不可变 `ref mut` 负例全绿 |
| G2.3 | 剩余：类型 / `Self` 依赖常量表达式；**核心闭环已合入**（PR #115/#117：固有 `impl` 块、具体类型/枚举静态关联函数派发、supertrait 蕴含与循环拒绝、关联类型绑定与跨 trait 消歧归一化） | 部分实现 | P1 | 覆盖未用定义期义务与循环负例；Verify 与双后端原生通过 |
| G2.4 | 剩余：更广 supertrait 泛型实参；**核心闭环已合入**（PR #97/#98/#115/#117：内联方法、固有 `impl`（struct/package/enum/generic/primitive）、trait 泛型方法、关联类型约束 `T: Iter<Item = U>`、blanket impl 约束与 `where` 校验、管道/记录更新/字符字面量） | 部分实现 | P1 | 每子项含原生正例 + 冲突/越界负例；0 回归 |
| G3.4 | 剩余：跨函数显式存储上界/复杂 lending 组合；**核心闭环已合入**（PR #99/#120：`Struct`/`Package`/`Tuple` 逐字段借用跟踪 `Carries::set_field`、字段覆写释放旧 loan、Raw/Wire 有效证书精确 `LoanConflict` 证明与双后端原生全绿） | 部分实现 | P1 | 覆盖悬垂/别名负例与双后端原生 |
| G3.5 | 剩余：更一般跨线程捕获运输；**核心闭环与路径敏感 `FnMut` 补齐证明已合入**（PR #99/#120/#123/#124/#125：只读 `var` 共享捕获、非 `move` 按值 `FnOnce`、嵌套 `OuterCapture` 消费、`analyze_capture_moves` 覆盖 `give`/`match`/`let` 隐式移动与循环 `break` 后补齐，Typeck/MIR/Verify 三层交叉验证 + `let` 解构 3 项回归 + 四组合精确 `Drop` stdout 比对全绿） | 部分实现 | P1 | 仅余远期跨线程捕获运输扩展；底层 CFG 证明由 Solver/`zil-check` 担任最终权威 |
| G4.5 | 剩余：rich callable、ambient `Type` 导出与 `#if`/`#for` 元语句；**CT 表达式与 `#{ ...; yield }` 块已合入**（PR #121：`MetaBlock` + `StmtKind::Yield` 经 verified `zil-interp` 求值及非法 `yield`/拥有载荷逃逸 `E-RESOURCE-TYPE` 拒绝） | 部分实现 | P1 | 经 verified `zil-interp` 执行；覆盖非法 CT 载荷负例与原生消费正例 |

## 3. L2 · 动态抽象与状态协议

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G5.2 | 剩余：有限泛型 method slot 派发；**核心闭环与按值 `self: Self` 堆释放已合入**（PR #121/#123/#124：`check_dyn_object_safe`、参数化 `dyn Trait<T>` / `Assoc = T`、按值 thunk `load` 后 `free(%data)` 经 Clang/LLVM API × O0/O2 四组合单次 Drop + Clang `-O0`/`-O2` `-Wl,--wrap=calloc/free` 零泄漏与主动 `double_free` 探针自检 + Raw/Wire 有效证书 `UseOfUninit` 全绿） | 部分实现 | P1 | 仅余有限泛型 method slot 派发；按值/借用 `OwnedDyn` 既定门已闭环 |
| G5.3 | 剩余：trait 生命周期参数化 `dyn Trait<'a>` 方法返回区域替换（`static-callback-getter-cycle-control.zil`）；**核心闭环已合入**（PR #100/#125：借入 provider `OwnedDyn<dyn Trait, Context<'a>>`、共同借入 `Store<OwnedDyn<..., Context<'a>>>` push/at/pop、`OwnedDynCreate` 独占/可写地址 Context 健全性、entry-block 循环 `alloca` 提升、16 份 `dyn-context` + 40 万次分配循环双后端原生 + Raw/Wire 有效证书 `zil-check` 全绿） | 部分实现 | P1 | 仅余 trait 生命周期参数方法返回区域替换；共同借入 Store 与 Context 健全性既定门已闭环 |
| G6.1 | 剩余：更一般用户自定义协议状态机与别名规则（PR #102 已闭合内建协议 `requires(state)`/`provides(retired|on_pending|on_ready)` 的 IR/Wire 与独立 CFG 证明、传递 `Suspend` 跨挂起借用证明及 `resume(transfer)` 外部 Provider 角色/迁移 fail-closed） | 部分实现 | P0 | 覆盖跨分支状态合同正例 + 违反前置/后置/伪造 Raw-Wire 负例 |
| G6.2 | 剩余：真实投影、失效关系、历史撤销与导出上界组合 | 部分实现 | P0 | 依赖 G6.1、G3.4；覆盖失效后访问拒绝负例与原生正例 |
| G6.3 | 剩余：状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1 | 依赖 G6.2、G8.4、G8.6；联合原生正例 + 跨状态替换/取消负例 |

## 4. L3 · 并发与外部生命周期

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G7.2 | 剩余：一般跨线程借入环境（`Address<Closure>`/`&mut` 捕获 profile 扩展）；**核心闭环已合入**（PR #92/#105/#106/#107/#108：密封跨线程 `Registration`、作用域回调、逐路径 CFG provider 身份证明、有效证书同根别名 `LoanConflict` + 不相交对照、`RISKS.json` 14 项 1-to-1 绑定同步、`AsyncRegistrationStart` 角色门控） | 部分实现 | P1 | 仅余远期跨线程借入捕获扩展，现行规范与 14 项风险矩阵已全部双后端原生 + Raw/Wire 验收 |
| G7.3 | 剩余：跨线程一般借入环境的外部 waker / 晚通知与 drain；**同线程/C-host 核心闭环已合入**（PR #108：8 份 `async-registration` queued/inline-tuple 原生 C-host + `calloc/realloc/free` 包装全绿、`raw-holder.zil` pthread 实测） | 部分实现 | P1 | 覆盖晚通知/重复关闭原生正例 + 未 drain 释放负例 |
| G7.4 | 剩余：跨线程借入环境 `Registration` 与异步任务取消联合；**核心闭环已合入**（PR #108/#112：取消嵌套清理、OOM 回退与 `task_create_owned` 所有权恢复双后端原生全绿） | 部分实现 | P1 | 依赖 G7.2、G8.6；覆盖取消嵌套清理与分配失败原生执行 |
| G8.4 | 剩余：`FnOnce` 挂起根下的精细协议区分（`once-pending-callable/owned-result-cleanup`）；**核心闭环已合入**（PR #112：`PendingExit` 取消后拥有结果单次 Drop、枚举活跃变体 Drop、`task_create`/`task_create_owned` OOM 清理与 Direct 默认 Drop 强制拒绝） | 部分实现 | P1 | 覆盖嵌套取消清理、分配失败与非法 Direct 默认 Drop 负例 |
| G8.5 | 剩余：用户自定义泛型对 `Transfer`/`Share` 的自动派生与跨模块约束；**核心闭环已合入**（PR #104/#106/#110：`MemoryOrder` 5 序与 `Sync*Ordered` 原子家族（Wire 43–47）、保留 `Transfer`/`Share` trait、`c_null()`（Wire 48）、`ForeignHandle ==/!=`、`thread_scope` 裸 `c_handle` 拒绝、`Mutex<Resource>` FFI Drop 拒绝） | 部分实现 | P1 | 真实竞争、有序原子与安全负例双后端原生全绿 |
| G8.6 | 剩余：一般借入 frame port 与 ticket/defer 复杂组合；**核心闭环已合入**（PR #103/#106/#112：`std.task.*`、逐路径 CFG worker `resume(transfer)` 格证明、`task_await`、`EnumPartialDrop` 选择性枚举字段 Drop、`inline-task-enum/package` 双后端原生全绿） | 部分实现 | P1 | 继续帧/借入 port/取消联合原生验收与 Raw/Wire 证明 |

## 5. L4 · 标准库与 ABI

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G9.3 | 剩余：跨过程 `apart` 精度与更一般高阶集合算法；**核心闭环与拥有集合原生套件已合入**（PR #109/#111/#114/#121：canonical `lib/std/*.zil` 内嵌信任根、`E-VISIBILITY` 私有字段强制、跨 `Desc`/多定义 `StoreBorrow` fail-closed `LoanConflict`、19 份标准库消费者 + `iterator`/`string_search` + `std.owning_map`/`owning_hash_map`/`owning_hash_set`/`graph`/`buffered_io` 双后端原生全绿） | 部分实现 | P1 | 保留两后端标准库/迭代原生门与 Raw/Wire 有效证书回归门 |
| G10.3 | 剩余：非 Linux ABI；**核心闭环与匿名联合已合入**（PR #113/#116：`IntrinsicOp::CCallback`（Wire 49）、递归 POD Record C-ABI 引用、glibc `errno` 桥接、LLVM 浮点/有符号整数常量降级修复、**匿名联合 `T | U`（含拥有载荷 Drop、规范排序与重复/非穷尽/双消费拒绝）**、`main()` ErrorUnion 错误载荷清理修复与 `owned-sum-consumer` 全 32 组合 ORACLE 双后端原生全绿） | 部分实现 | P1 | 跨模块与 C ABI 原生正例 + 越界/非法 ABI 与 Raw/Wire `CCallback` 负例全绿 |

## 6. 平台与 ZUI

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G10.4 | **Windows x64 原生支持**：原生 CLI、ABI、Runtime、文件 I/O、线程/Task 与实际 Windows CI | 待执行 | P1 | 先完成独立控制台原生执行，再补线程/Task；交叉构建或 Wine 不算验收 |
| G9.4 | **ZUI 窗口层**：真实 OS 窗口、事件循环与 OS IME 服务 | 部分实现 | P2 | 真实窗口创建/输入/关闭原生验收，无资源泄漏 |
| G9.5 | **ZUI 渲染层**：GPUI 式 GPU 场景 / 首帧呈现、字体塑形与字形图集 | 待执行 | P2 | 以首个真实 GPU 场景首帧呈现为第一阶段退出门 |
| G9.6 | **ZUI 应用层**：实体 / 组件、输入调度与资源确定性关闭合流 | 待执行 | P2 | 依赖 G9.4、G9.5；真实交互应用端到端运行与确定性清理 |

## 7. 独立专项与发布验收

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G10.5 | **编译与性能专项**：与 Rust 同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破（含 **G10.5.1.16 Stage 2**：去除 `body_structured.rs` 文本桥接，直接 `VerifiedModule -> model::Module` 降级与同口径 O2 性能门） | 进行中 | P1 | 独立分支（`research/full-push-native`、`integrate/scalar-codegen`），不纳入普通语言线 |
| G10.6 | **1.0 终态产品门**：B0、完整语言与 ZUI 终态联合验收（见主树 `validation/product-gates.json`） | 待执行 | P2 | 子项完成不自动关闭父门，须全门独立联合复验 |
