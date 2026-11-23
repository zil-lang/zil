# ZIL 实时任务看板

> 更新：2026-11-23（Asia/Shanghai） · 唯一职责：仅追踪**未完成任务、优先级、依赖与可执行退出门**，不记录历史施工流水账。已合入能力以主树 [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) 与 [`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) 为准；精简前历史快照固定于 [`goal@e630d5f03`](https://github.com/zil-lang/zil/blob/e630d5f03/goal.md)。

[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md)

## 1. 当前目标与里程碑

- **主线基线**：[`main@8c7a0fe2b`](https://github.com/zil-lang/zil/commit/8c7a0fe2b)（开放 PR：0；L1–L4 语言与安全证明收口分支均已合入并清理）
- **当前阶段**：L1 四批（[PR #96](https://github.com/zil-lang/zil/pull/96)–[#99](https://github.com/zil-lang/zil/pull/99)）、L2 三批（[PR #100](https://github.com/zil-lang/zil/pull/100)–[#102](https://github.com/zil-lang/zil/pull/102)）、L3 全量闭环（[PR #103](https://github.com/zil-lang/zil/pull/103)–[#108](https://github.com/zil-lang/zil/pull/108)、[PR #110](https://github.com/zil-lang/zil/pull/110)、[PR #112](https://github.com/zil-lang/zil/pull/112)）与 **L4 标准库 / 借用 / C ABI 并行闭环**（[PR #109](https://github.com/zil-lang/zil/pull/109)、[PR #111](https://github.com/zil-lang/zil/pull/111)、[PR #113](https://github.com/zil-lang/zil/pull/113)）均已 4/4 远端 CI（含 `alpha-acceptance`）全绿合入 `main`
- **总体阻塞**：PR #114 已完成且经独立复核接受此前 G9.3 的两项限定 P0 修补（标准库 canonical 信任根及跨 Desc/分支多定义 StoreBorrow 的保守 LoanConflict）；当前无由这两项遗留的 P0 阻塞。一般跨过程 apart 精度仍在 G9.3 后续范围；全仓 `tests/**/*.zil` 原文件 `zil check` 计数 **`852 / 1,385`**（`477 SafeReject / 56 Unsupported`，0 回归，0 误放行，0 `ResourceLimit`；本轮同时修复了跨模块私有字段可见性、`StoreBorrow` 别名冲突、Direct 默认 `Drop` 未闭 `Task` 与 `Mutex<Resource>` FFI Drop 拒绝等 5 项安全误放行）
- **下一发布门**：G9.3 限定安全收口已验收；回到未完成的语言组合任务和产品门，按实际消费者优先级推进；Windows/ZUI 保持独立规划
- **状态与退出规则**：`进行中`｜`部分实现`｜`待执行`｜`待排期`。① 已证明完全实现即从主表移除；② 含远期扩展的父任务将已闭合部分固化到 `supported-language.md`，余项拆新任务；③ 无真实消费者或竞争收益则显式关闭；④ 安全边界缺陷永远 P0 阻断；⑤ 详细验收清单可随任务分支放在 `.task/<ID>.md`，合入 `main` 前移除，仅留正式合同与测试。

## 下一阶段施工安排（以 main@8c7a0fe2b 为基线）

本轮 L3 余项（PR #108 / #110 / #112）、L4 三批（PR #109 / #111 / #113）及限定 P0 修补（PR #114）均已合入并通过远端验收；此前 G9.3 两项 P0 阻塞已解除。编译性能专项（`research/full-push-native`、`integrate/scalar-codegen`）继续独立保留。

**统一计量及安全门**：`852/1385` 仅为 `zil check` 返回 Success 的原文件数，**不是正例通过率或功能完成率**。先以精确文件路径、原始输入哈希和预期结果把本批真实正例、预期安全负例及旧/弃用 fixture 分开；每批独立统计新增合法通过、已有合法回归、错误准入、预期拒绝和 ResourceLimit，不得仅凭文件名判负例。保留当前 Alpha 收据、Clang/LLVM API × O0/O2、Raw/Wire、清理/故障与远端 CI 必需门；失败不以提高预算或 Unsupported 作为通过。

## 2. L1 · 基础语言与类型组合

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G1.4 | 剩余：一般地址 / `Package` / `ErrorUnion` 混合解构与部分补齐组合（`ref`/`ref mut` 与多变体枚举 `drop_elab` 已合入） | 进行中 | P0 | 接 G3.4；双后端 O0/O2 原生正例 + 部分移动/不可变 `ref mut` 负例 + raw/Wire 校验 |
| G2.3 | 剩余：一般关联投影、类型 / `Self` 依赖常量表达式、约束蕴含与精确循环拒绝 | 部分实现 | P0 | 覆盖未用定义期义务与循环负例；Verify 与双后端原生通过 |
| G2.4 | 剩余：内联方法（`struct`/`package`）、更一般 life / lending、符号泛型方法、静态关联函数、blanket / primitive / enum impl 与更广 supertrait 实参（继承默认 trait 方法已合入） | 部分实现 | P0 | 每子项含原生正例 + 冲突/越界负例；0 回归 |
| G3.4 | 剩余：普通借入字段、存储上界、affine 借用、合法 lending 结果及资源包独占借用/字段转移组合 | 部分实现 | P0 | 按共用 loan/region 根因整链修复；覆盖悬垂/别名负例与双后端原生 |
| G3.5 | 剩余：一般拥有 / 借入捕获及责任组合的消费者矩阵（捕获环境存储与跨调用运输） | 进行中 | P0 | 接 G3.4；覆盖 Fn/FnMut/FnOnce 捕获正例 + 逃逸/重复消费负例 + 双后端清理 |
| G4.5 | 剩余：rich callable、ambient `Type` 导出与更一般可序列化值 / 匿名 Sum / 拥有载荷 CT 组合 | 部分实现 | P1 | 经 verified `zil-interp` 执行；覆盖非法 CT 载荷负例与原生消费正例 |

## 3. L2 · 动态抽象与状态协议

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G5.2 | 剩余：异构调用、静态 vtable、关联项与有限泛型 method slot 的真实派发 | 部分实现 | P0 | `zil-check` 独立校验 `DynImpl` 签名/身份；含异构原生正例 + 伪造槽位负例 |
| G5.3 | 剩余：借入 provider（`OwnedDyn<dyn Trait, Context<'a>>`）、共同借入存储与管理 route | 部分实现 | P0 | 覆盖移动/销毁/分配失败原生正例 + provider 悬垂/逃逸负例 |
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
| G9.3 | 剩余：更一般高阶集合算法与 GAT lending 迭代；**核心闭环与 P0 安全证明已合入**（PR #109/#111/#114：默认始终加载内嵌 canonical `lib/std/*.zil` 并拒绝本地/祖先 `lib/std/` 覆盖与 `E-STD-RESERVED` 伪造、跨模块私有字段 `E-VISIBILITY` 强制、`StoreBorrow` 跨 `Desc` 参数及分支合流多定义 fail-closed `LoanConflict` + 真独立 `StoreNew` 合法对照、19 份标准库真实消费者与 `iterator`/`string_search`/`lending` 双后端原生全绿） | 部分实现 | P1 | 保留两后端标准库/迭代原生门与 Raw/Wire 有效证书回归门 |
| G10.3 | 剩余：匿名联合 `T | U`（`TypeKind::Union`）及非 Linux ABI；**核心闭环已合入**（PR #113：`IntrinsicOp::CCallback`（Wire 49）、递归 POD Record C-ABI 引用、glibc `errno` 桥接、LLVM 浮点/有符号整数常量降级修复、`ffi.zil` / `float_roundtrip` 8,002 次 locale 零泄漏 / `float_status` / `complete-domains` / 大闭包与 16 参数辅助双后端原生全绿） | 部分实现 | P1 | 跨模块与 C ABI 原生正例 + 越界/非法 ABI 与 Raw/Wire `CCallback` 负例全绿 |

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
