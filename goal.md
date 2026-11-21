# ZIL 实时任务看板

> 更新：2026-11-21（Asia/Shanghai） · 当前主线基线：[`main@a3e2f5227`](https://github.com/zil-lang/zil/commit/a3e2f5227)（Alpha Core、并发 Store 安全修补、A1 外部回调原生运行时、A2 验收自动化、A3 LLVM Builder Stage 1、PR #95 `_zil_design.md` 终态瘦身、PR #96 L1 Batch 1、PR #97 L1 Batch 2 均已合入；开放 PR：0；远端分支：5）
>
> **唯一职责**：本文件仅追踪**未完成任务、优先级、依赖与精确退出条件**，不记录历史施工流水账。已合入能力以主树 [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) 与规范 [`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) 为准；精简前完整历史快照固定于 [`goal@e630d5f03`](https://github.com/zil-lang/zil/blob/e630d5f03/goal.md)。

[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md)

## 任务状态与退出规则

- **状态定义**：`进行中`（当前轮次施工或验证）｜`部分实现`（已有有限子集进入 `main`，主退出条件未闭）｜`待执行`（已排定轮次、未启动）｜`待排期`（依赖前置轮次）。
- **退出规则（不以强行清零牺牲安全）**：
  1. **已证明完全实现**：经独立 Verify、正反例与原生验收合入后直接从本表移除（不再保留折叠历史表）。
  2. **含远期扩展的父任务**：已闭合部分随 `supported-language.md` 固化，剩余扩展拆为边界清晰的新任务。
  3. **无真实消费者或无竞争收益**：评审后显式关闭为不采用，不为了清零堆砌无用特性。
  4. **安全边界缺陷**：永远作为最高优先级 P0 阻断，不得用测试通过率抵消。

---

## 第一轮：语言核心收口（L1 + L2）

以全仓 `tests/**/*.zil`（1,385 份）按共用类型/所有权/来源根因小批整链收口（当前全仓通过 **716/1,385**，较基线 `573/1,385` 净增 **+143**，0 回归，0 误放行）：
- **L1 Batch 1（[`PR #96`](https://github.com/zil-lang/zil/pull/96)，已合入 `main@2504c9a86`）**：记录/包显式 `life` 实参、元组枚举 `manages` 字段、`ref`/`ref mut` 模式、多变体枚举 `drop_elab`（+120）。
- **L1 Batch 2（[`PR #97`](https://github.com/zil-lang/zil/pull/97)，已合入 `main@a3e2f5227`）**：泛型单态化 `FnSig.region_params` 计数、继承默认 trait 方法合成（+23）。

### L1 · 普通语言组合（P0）

| 任务 | 范围与精确退出条件 | 状态 | 优先级 | 依赖 / 备注 |
| --- | --- | --- | --- | --- |
| G1.4 | 更一般的地址 / Package / ErrorUnion 解构、混合模式及部分补齐组合 | 进行中 | P0 | 接 G3.4；覆盖正反例、raw/Wire 与双后端清理 |
| G2.3 | 一般关联投影、类型 / `Self` 依赖常量表达式、约束蕴含与循环诊断 | 部分实现 | P0 | 含未用定义期义务与精确循环拒绝 |
| G2.4 | 更一般 life / lending、符号泛型方法、静态关联函数、blanket / primitive / enum impl 与更广 supertrait 实参 | 部分实现 | P0 | 共享 lending 与有限拥有宿主已在 `main` |
| G3.4 | 普通借入字段、存储上界、affine 借用、合法 lending 结果及长期借用组合 | 部分实现 | P0 | 合并同根 loan/region 规则，不逐例打补丁 |
| G3.5 | 一般拥有 / 借入捕获及责任组合的消费者矩阵 | 进行中 | P0 | 接 G3.4；覆盖捕获环境存储与运输 |
| G4.5 | rich callable、ambient `Type` 导出与更一般可序列化值 / 匿名 Sum / 拥有载荷 CT 组合 | 部分实现 | P1 | 经 verified `zil-interp` 执行 |

### L2 · 动态抽象与状态协议（P0）

| 任务 | 范围与精确退出条件 | 状态 | 优先级 | 依赖 / 备注 |
| --- | --- | --- | --- | --- |
| G5.2 | 异构调用、静态 vtable、关联项与有限泛型 method slot 的真实派发 | 部分实现 | P0 | 含 `zil-check` 独立 `DynImpl` 签名校验 |
| G5.3 | 显式拥有 / 共同借入存储及管理 route，`OwnedDyn` 移动 / 销毁 / 分配失败路径完整 | 部分实现 | P0 | 扩展至借入 provider 与更广捕获 |
| G6.1 | 逐结果的一般状态转换、角色与别名规则 | 部分实现 | P0 | 覆盖跨分支状态合同与反例 |
| G6.2 | 真实投影、失效关系、历史撤销与导出上界组合 | 部分实现 | P0 | 接 G6.1、G3.4 |
| G6.3 | 状态协议与拥有存储、替换及异步责任共同运行 | 待排期 | P1 | 依赖 G6.2、G8.4、G8.6 |

---

## 第二轮：并发生命周期与标准库（L3 + L4）

以 `main@af63106d6`（已含 A1 原生 `Registration`/`AsyncRegistration` 与 P0 并发 Store 安全修补）为基础，以真实应用作联合消费者。

### L3 · 外部生命周期与并发（P0）

| 任务 | 范围与精确退出条件 | 状态 | 优先级 | 依赖 / 备注 |
| --- | --- | --- | --- | --- |
| G7.2 | 更一般借入 / 跨线程环境的登记对象与关闭合同 | 部分实现 | P0 | 密封 `ClosedEnvironment + Transfer + SyncDrop + ThreadCallable` 已合入 `main`；余一般借入环境与任意入口图 |
| G7.3 | 外部 waker / 晚通知、no-future 与 drain，关闭后所有 raw 入口静止 | 部分实现 | P0 | 余一般借入环境与通用 waker 图 |
| G7.4 | `Registration` 与异步任务责任的失败 / 取消 / 完成组合 | 部分实现 | P0 | 依赖 G7.2、G8.6 |
| G8.4 | 通用 `PendingExit`、挂起 / 取消排空、外部 waker 与晚通知 | 部分实现 | P0 | 覆盖取消嵌套清理与分配失败路径 |
| G8.5 | 一般 `Mutex<T>` / `Guard`、`Transfer`/`Share`、原子位宽与拥有同步构造 | 进行中 | P0 | Guard 跨挂起 / 跨线程权限需显式能力证明 |
| G8.6 | 一般拥有 Sum / result、借入 frame port、部分责任、ticket / defer 的任务组合 | 部分实现 | P0 | 余独占 / 嵌套借入结果与 owner/capture 运输 |

### L4 · 标准库与 ABI（P0）

| 任务 | 范围与精确退出条件 | 状态 | 优先级 | 依赖 / 备注 |
| --- | --- | --- | --- | --- |
| G9.3 | 完整字符串 / 集合与一般 `Iterator` / lending 组合 | 部分实现 | P0 | 依赖 L1 泛型 / lending 收口 |
| G10.3 | 大型 closure / helper / foreign ABI 及浮点 / 匿名联合责任的一般组合 | 部分实现 | P0 | 覆盖跨模块与 C ABI 边界 |

---

## 第三轮：平台扩展与图形界面（P1 + P2）

独立分阶段推进，不阻塞 Linux 语言版本。

| 任务 | 范围与精确退出条件 | 状态 | 优先级 | 依赖 / 备注 |
| --- | --- | --- | --- | --- |
| G10.4 | **Windows x64 原生支持**：原生 CLI、ABI、Runtime、文件 I/O、线程/Task 与实际 Windows CI | 待执行 | P1 | 先形成独立控制台执行，再补线程/Task；交叉构建或 Wine 不算验收 |
| G9.4 | **ZUI 窗口层**：真实 OS 窗口、事件循环与 OS IME 服务 | 部分实现 | P2 | 独立图形线 |
| G9.5 | **ZUI 渲染层**：GPUI 式 GPU 场景 / 首帧呈现、字体塑形与字形图集 | 待执行 | P2 | 以首个真实 GPU 场景为第一阶段交付 |
| G9.6 | **ZUI 应用层**：实体 / 组件、输入调度与资源确定性关闭合流 | 待执行 | P2 | 依赖 G9.4、G9.5 |

---

## 独立专项与长期验收门（非普通施工项）

| 任务 | 范围与精确退出条件 | 状态 | 优先级 | 负责人 / 备注 |
| --- | --- | --- | --- | --- |
| G10.5 | **编译与性能专项**：与 Rust 同语义的安全、运行/内存、编译、维护成本及至少一项可重复净突破（含 **G10.5.1.16 Stage 2**：去除 `body_structured.rs` 文本桥接，直接 `VerifiedModule -> model::Module` 降级与同口径 O2 性能门） | 进行中 | P1 | 编译专项独立分支（`research/full-push-native`、`integrate/scalar-codegen`），不纳入普通语言线 |
| G10.6 | **1.0 终态产品门**：B0、完整语言与 ZUI 终态联合验收（见主树 `validation/product-gates.json`） | 待执行 | P2 | 子项完成不自动关闭父门 |
