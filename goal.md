# ZIL 实时任务看板

> 更新：2026-11-21（Asia/Shanghai） · 唯一职责：仅追踪**未完成任务、优先级、依赖与可执行退出门**，不记录历史施工流水账。已合入能力以主树 [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) 与 [`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) 为准；精简前历史快照固定于 [`goal@e630d5f03`](https://github.com/zil-lang/zil/blob/e630d5f03/goal.md)。

[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md)

## 1. 当前目标与里程碑

- **主线基线**：[`main@5ce64983e`](https://github.com/zil-lang/zil/commit/5ce64983e)（开放 PR：0；所有 L1/L2 任务分支已合入并清理）
- **当前阶段**：本轮排期内的 **L1 四批**（[PR #96](https://github.com/zil-lang/zil/pull/96)–[#99](https://github.com/zil-lang/zil/pull/99)）与 **L2 两批**（[PR #100](https://github.com/zil-lang/zil/pull/100) 借入 `OwnedDyn<dyn Trait, Context<'a>>` provider + 区域合同不变性；[PR #101](https://github.com/zil-lang/zil/pull/101) 函数 `resume`/`effects`/协议状态合同 + 外部回调 provider 角色验证）均已 4/4 远端 CI 全绿合入 `main`
- **总体阻塞**：无 P0 安全阻断；全仓 `tests/**/*.zil` Success 数 **`764 / 1,385`**（0 回归，0 误放行，0 `ResourceLimit`；未证明的 `effects(spawn)` 与 `scoped_callback_*` 保持显式 `Unsupported`）
- **下一发布门**：待新一轮 L3/L4 派工（如 `std.task` 命名空间、`TaskThreadScopeResult` 跨线程任务迁移、`c_with_callback` 作用域回调运行时与真实应用联合验收）
- **状态与退出规则**：`进行中`｜`部分实现`｜`待执行`｜`待排期`。① 已证明完全实现即从主表移除；② 含远期扩展的父任务将已闭合部分固化到 `supported-language.md`，余项拆新任务；③ 无真实消费者或竞争收益则显式关闭；④ 安全边界缺陷永远 P0 阻断；⑤ 详细验收清单可随任务分支放在 `.task/<ID>.md`，合入 `main` 前移除，仅留正式合同与测试。

## 下一阶段施工安排（以 main@5ce64983e 为基线）

本轮排定的 L1 Batch 1–4 与 L2 Batch 1–2 已全部验收合入，当前无待施工的 L1/L2 批次；等待下一轮 L3/L4 派工。编译性能专项独立保留，不在普通语言任务分支修改。

**统一计量及安全门**：`716/1385` 仅为 `zil check` 返回 Success 的原文件数，**不是正例通过率或功能完成率**。先以精确文件路径、原始输入哈希和预期结果把本批真实正例、预期安全负例及旧/弃用 fixture 分开；每批独立统计新增合法通过、已有合法回归、错误准入、预期拒绝和 ResourceLimit，不得仅凭文件名判负例。保留当前 Alpha 收据、Clang/LLVM API × O0/O2、Raw/Wire、清理/故障与远端 CI 必需门；失败不以提高预算或 Unsupported 作为通过。

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
| G6.1 | 剩余：函数/外部契约子句（`function contracts` / `extern contract clause`）、逐结果状态转换、角色与别名规则 | 部分实现 | P0 | 覆盖跨分支状态合同正例 + 违反前置/后置/别名负例 |
| G6.2 | 剩余：真实投影、失效关系、历史撤销与导出上界组合 | 部分实现 | P0 | 依赖 G6.1、G3.4；覆盖失效后访问拒绝负例与原生正例 |
| G6.3 | 剩余：状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1 | 依赖 G6.2、G8.4、G8.6；联合原生正例 + 跨状态替换/取消负例 |

## 4. L3 · 并发与外部生命周期

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G7.2 | 剩余：一般借入 / 跨线程环境的 `Registration` 与关闭合同（密封 `ClosedEnvironment + Transfer + SyncDrop + ThreadCallable` 已合入） | 部分实现 | P0 | 覆盖借入环境登记正例 + 关闭后回调/悬垂捕获负例 + 原生双后端 |
| G7.3 | 剩余：一般借入环境的外部 waker / 晚通知、no-future 与 drain，关闭后所有 raw 入口静止 | 部分实现 | P0 | 覆盖晚通知/重复关闭原生正例 + 未 drain 释放负例 |
| G7.4 | 剩余：`Registration` 与异步任务责任的失败 / 取消 / 完成组合 | 部分实现 | P0 | 依赖 G7.2、G8.6；覆盖取消嵌套清理与分配失败原生执行 |
| G8.4 | 剩余：通用 `PendingExit`、挂起 / 取消排空、外部 waker 与晚通知 | 部分实现 | P0 | 覆盖嵌套取消清理、分配失败与非法 `task_yield` 负例 |
| G8.5 | 剩余：一般 `Mutex<T>` / `Guard`、`Transfer`/`Share`、原子位宽与拥有同步构造 | 进行中 | P0 | 覆盖真实多线程竞争原生正例 + Guard 跨挂起/跨线程未授权负例 |
| G8.6 | 剩余：一般拥有 Sum / result、借入 frame port、部分责任、ticket / defer 的任务组合（独占/嵌套借入结果与 owner/capture 运输） | 部分实现 | P0 | 覆盖原生多轮挂起/取消清理正例 + 帧逃逸/清理顺序负例 |

## 5. L4 · 标准库与 ABI

| ID | 任务与剩余边界 | 状态 | 优先级 | 依赖 / 验收门 |
| --- | --- | --- | --- | --- |
| G9.3 | 剩余：完整字符串 / 集合与一般 `Iterator` / lending 组合 | 部分实现 | P0 | 依赖 L1 泛型 / lending 收口；标准库真实消费原生测试全绿 |
| G10.3 | 剩余：大型 closure / helper / foreign ABI 及浮点 / 匿名联合责任的一般组合 | 部分实现 | P0 | 覆盖跨模块与 C ABI 原生正例 + 越界/非法 ABI 负例 |

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
