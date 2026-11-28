# ZIL 编译器架构与实现合同

> 适用规范版本：**`ZIL-LANG-1.5`** · 语言语义总权威：[`_zil_design.md`](../../_zil_design.md) · 中段内核合同：[`spec/semantic-kernel.md`](../../spec/semantic-kernel.md) · 降低合同：[`spec/source-lowering.md`](../../spec/source-lowering.md) · 依赖边约束：[`policy/dependencies.json`](../../policy/dependencies.json)

本文规定 ZIL 主编译器的内部架构、阶段职责、分析/核验算法、增量编译与缓存架构、以及 LLVM/目标后端降低合同。

**分层判断标准**：若未来使用 ZIL 自举重写一套全新编译器，任何实现都必须遵循的语言行为归属 [`_zil_design.md`](../../_zil_design.md)（含其形式语义附录）；可替换的内部数据结构、分析算法、调度策略、缓存布局与后端流水线归属本文。

---

<a id="ZIL-ARCH"></a>

## 1. 编译器架构与依赖约束

### 1.1 实现基础与数据组织

编译器主实现为 Rust，稳定工具链与依赖通过 `Cargo.lock` 精确锁定；不使用 `rustc_private`，不将 ZIL 转译为 Rust 后借用宿主借用检查器替代本语言安全核。Cargo workspace 仅管理编译器自身构建；ZIL 的模块、包清单（`Zil.toml`/`Zil.lock`）、缓存与目标配置是独立产品接口。

核心内部数据采用类型化 ID、紧凑表与不可变阶段快照。每张主表由唯一阶段写入，禁止跨模块共享任意可写全局 `CompilerContext`。内部常规调用不因模块解耦而引入 RPC、JSON 或逐操作虚派发；crate 仅按可信边界、独立状态、替换点与重依赖划分。

### 1.2 模块职责与禁止项

| 模块（Crate 族） | 主要职责与产出 | 禁止项 |
| --- | --- | --- |
| `base` / `ir` | 稳定 ID、span、诊断；Core / Body IR / 合同 / 证书 / `SourceMap` / LIR schema；`OpSpec`（操作数角色、规范 Place、结构约束、静态效果）的唯一穷尽定义 | 不含求解、driver 或平台状态 |
| `semantics` | 唯一语义规则内核：状态域与合流、转移（多后继/分支条件）、安全前提、边语义、出口义务、调用合同与区域约束的合法性 | 不含不动点调度、诊断流程；不构造 `Verified` |
| `syntax` / `expand` | 无损词法/语法树、固定语法、卫生作用域、未核 CoreAST | 不构造 `Verified`；不调用安全求解 |
| `ct` (`interp`) | 已核 CTIR 的受限虚拟机执行 | 不递归进入未完成类型/impl 查询 |
| `types` (`typeck`) | 名称解析、类别、trait、有限投影与类型证据；降为 Body IR，唯一产出候选 `SourceMap` | 不按安全失败切换目标 |
| `analyze` (`solve`) | 基于语义内核求解来源、`One`/`Many`、绑定、前缀合同、窗口与责任；生成候选归纳证书与完整诊断 | 不展开运行时堆，不读私有 callee 正文，不授予安全权限 |
| `verify` (`check`) | 不信任任何上游候选：基于语义内核核验证书的入口、转移、全部控制边、出口、调用与区域；核验 Source→IR 保真与 CT 定义；**唯一安全授权入口** | 不依赖 `types`/`analyze`，不把候选结论当事实 |
| `lower` / `backend` | 消费已核 Body/LIR、实际布局、ABI、初始化、清理、帧布局与 LLVM/原生对象生成 | 不按 parser 关键字补安全语义，不擅添 `noalias` 或降低错误合同 |
| `db` / `driver` | 查询调度、结果指纹、会话与可信本地缓存协调 | 叶模块不得反向依赖 `driver` |
| `runtime` | 内存、同步、完成根、平台适配 | 不依赖编译器、ZUI 或业务对象 |
| `cli` / `ide` | 统一驱动与语义查询入口 | 不维护第二个安全检查器 |

允许的 crate 依赖边由 [`policy/dependencies.json`](../../policy/dependencies.json) 固定并由 `tools/check-architecture.mjs` 强制检查。`zil-check`（Verify）及核心前端强制 `#![forbid(unsafe_code)]`；其余 `unsafe` 仅限于已声明并审计的后端 FFI 边界（如 `zil-llvm-api`）。

求解（Analyze）与核验（Verify）共享 `OpSpec` 与 `zil-semantics` 单一规则定义，但职责严格分离：Analyze 搜索候选不变量与证书，Verify 不信任生产者结论，依据原始 IR、规范语义与候选证书独立核验全部证明义务后才构造 `VerifiedModule`。语义内核自身由不调用内核的独立测试资产约束（见 [`validation/README.md`](../../validation/README.md)）。

阶段逻辑数据流为：

```text
Syntax → CoreAST → TypedCFG / Body IR → 候选证书（Solve） → VerifiedModule（Check） → LIR / LLVM IR → 原生对象与链接产物
```

`driver` 按拓扑与已完成形状快照协调各阶段，避免 `expand`/`types`/`ct` 互相递归导入。

---

## 2. 中段分析算法与独立证书核验

本节记录当前编译器为实现 [`_zil_design.md`](../../_zil_design.md) 形式语义所采用的可替换分析与核验算法；完整接口合同见 [`spec/semantic-kernel.md`](../../spec/semantic-kernel.md) 与 [`spec/analysis-inventory-sharing.md`](../../spec/analysis-inventory-sharing.md)。

### 2.1 抽象位置基数（`One` / `Many`）与有限位置图

- **基数格**：每个抽象根除来源外还携带 `One` / `Many` 基数。`One` 表示在当前证明激活下指代唯一确定位置（不同参数仍可能同源，不自动不交）；`Many` 表示容器家族、循环历史集合或未知索引汇总。**候选集合仅含一个元素不是强更新的充分条件**：强更新要求候选是单一稳定精确位置且基数为 `One`；`Many` 或多候选必须弱更新。
- **预建有限位置图**：SSA 定义点、phi、可寻址字段、调用投影与分配成功点在分析初始化时预分配节点。循环回边仅在既有有限节点间增加边与事实，不得在每次不动点迭代构造新的 `Union(old, new)` 语法节点以防“无限长的有限 DAG”。递归名义结构的未知路径归入既定存储族；返回包内部存在根使用签名模板。
- **TypedCFG 与控制流合流**：所有求值、临时值、清理、panic 与取消点显式进入 CFG。分支采用 may-并 / must-交，保留单层同源 SSA 条件关联，不展开任意布尔树；互斥路径交接单一权限，不在合流处叠加资格。递归组先确立形状、投影与状态合同，may-prefix 与返回可达性求最小不动点。

### 2.2 有序资源快路径（U/K/D）与 `defer` 中段降低

- **固定资源快路径**：对序列片段维护摘要 `U`（入口使用，含 `close` 自身）、`K`（可能关闭）与 `D`（先关后用所需分离关系），顺序组合满足：
  `U(A; B) = U_A ∪ U_B`，`K(A; B) = K_A ∪ K_B`，`D(A; B) = D_A ∪ D_B ∪ (K_A × U_B)`。
  仅正常继续出口进入 `B`，异常/错误路径分别保留；所有函数均具 `prefix` 与逐 tag `exits`，无正常返回的递归仍核验 `prefix`。
- **同步 `defer` 降低合同**：Core 阶段的 `Defer` 登记词法零参 `FnOnce` 闭包；TypedIR 以 `RegisterDefer { scope, local }` 绑定当前 scope 与已完整初始化的局部槽；`TakeLocal(local)` 紧邻零参 `CallClosureOnce` 为唯一履行序列，且必须位于所属 `ScopeExit` 的可核清理后缀中。Analyze 与 Verify 独立重建每次激活的注册、逆序履行与混合清理顺序，不信任前端履行标记。

### 2.3 不可变共享快照与归纳证书核验

- **无深复制独立核验**：`TypedCFG`、名义声明与候选计划可由不可变共享快照承载；Verify 独立核对安全事实，无需为独立性深复制不可变原始输入，但快照不得含核验后可被其他阶段改写的内部可变状态。
- **归纳证书路线**：Analyze 通过 RPO 工作表迭代求取块不变量并生成候选证书；Verify 不重复搜索不动点，而是依据 [`spec/semantic-kernel.md`](../../spec/semantic-kernel.md) 独立逐项核验入口覆盖、逐指令安全前提、全部控制边闭合（含隐式清理/错误/取消/挂起/循环出口）、出口义务、调用/递归 SCC 归纳合同、区域约束及 Source→IR 保真（`check_fidelity`）。任一缺项或 `Unknown` 均报 `SafeReject`。
- **确定性并行与计费**：模块读取/解析、Solve、Check、契约/保真核验与多 CGU IR 生成均支持多线程并行，并以确定性顺序合并诊断与回放工作预算。必须判定与可选优化分开计费，不得预建 `CFG × 来源 × 对象` 的笛卡尔积。

---

## 3. 查询粒度、稳定编译身份与增量缓存

### 3.1 查询粒度与失效范围

查询演进目标为：文件 `Parse` → 声明形状与导出 → 单项类型/合同 → 递归 SCC → 单函数候选/核验 → 实例/LIR/CGU。当前已实现：

- **常驻 AST 解析缓存**：`CompilationSession` 以完整 `ParseCacheKey { text_fp, file_id, module_id, max_source_bytes, max_tokens, max_nodes, max_call_depth }` 为键，避免同文本不同文件/模块身份串用或放宽限额误命中；一次性 CLI 使用 `CompilationSession::ephemeral()`（`capacity = 0`）避免多余 AST/IR 克隆。
- **可信本地语义候选缓存（`--semantic-cache DIR`）与常驻会话（`--resident`）**：
  - 缓存键（`zil-sem-cache-v2`）绑定显式 `SEMANTIC_CACHE_ABI_VERSION`、`CARGO_PKG_VERSION`、`WIRE_VERSION`、`TargetProfile`、`Limits` 与完整模块源图指纹，以 `ZSEM` + SHA-256 封装保存整程序 `(Module, SourceMap, Certificate)` 候选（滚动上限 8 份快照）；常驻会话额外维护内存逐函数 `CertificateCache` 复用未变函数的 Solve 证书。
  - **严格信任边界**：`--semantic-cache DIR` 与 `--object-cache DIR`、`rustc -C incremental` 属于同一**可信本地构建缓存**模型。每次命中仍强制执行新鲜 `verify_ct_original_definitions`、`check_fidelity` 与 `check_module`（`verification_runs == 1`），绝不绕过内存/借用/协议安全验证；损坏或无效证书候选自动清除并回退完整重建。因 `check_fidelity` 不重跑全部纯常量表达式类型检查，不得将该目录暴露给不可信远端写入者。
- **下一阶段目标**：在现有整程序语义快照与逐函数 Solve 证书复用基础上，推进按模块导出签名指纹驱动的**模块级增量 `typeck`**。

### 3.2 稳定编译身份与结构化 CGU

编译器严格区分五类身份：局部紧凑 ID、稳定声明身份、泛型/闭包单态化身份、运行时资源实例身份与当前请求验证授权。

- **符号与单态化命名**：跨请求函数符号基于规范模块/声明路径而非全局遍历序号；`impl Trait for Concrete` 以目标类型规范标签限定（非 ADT 使用无 `Ty(N)` 序号的规范类型标签）；泛型单态化标签对**完整未截断类型结构**计算 64-bit SHA-256 后缀（仅可读前缀截断显示）；字符串静态符号使用完整 256-bit SHA-256。
- **固定桶空间 CGU 划分**：多单元发射以 `actual_units = requested_units.max(1)` 固定桶空间，完全脱离总函数数、模块数与不可达死函数；普通模块按稳定哈希（FNV-1a + `fmix64`）分桶，泛型单态化函数按完整规范符号跨 worker CGU 分片，避免热点集中与增删函数引发无关缓存雪崩。跨单元仅发射按需外部声明。

### 3.3 验证授权、对象 Action Key 与链接跳过分离

验证授权（`VerifiedModule`）、对象码复用键（action key）与产物内容哈希三者不可互换：

- **对象缓存（`--object-cache DIR`，`zil-obj-cache-v4`）**：先完成当前请求新鲜 `zil-check`，再按包含规范化单元 IR、显式 `OBJECT_CACHE_CODEGEN_ABI`、`LLVM_VERSION`、优化与目标配置的 SHA-256 键查找对象；采用私有 `.stage-<pid>-<seq>` 硬链接快照防跨进程竞态，先发布 `.meta` 再原子发布 `.o/.obj`，读取时核验 SHA-256 与 ELF/COFF 魔数并自动自愈损坏项；默认 `32 MiB` 最佳努力软 LRU 预算（`--object-cache-max-mb`，始终保留当前活跃构建对象）。
- **链接指纹跳过（`zil-link-v2`）**：全部单元缓存命中且无额外 C 源文件时，按编译器版本、`OBJECT_CACHE_CODEGEN_ABI`、`LLVM_VERSION`、目标/优化标志、全部单元键以及工具链二进制元数据身份（`clang`、同级 `ld.lld`、`windows.linker` 与 `import_libraries` 的规范路径、文件大小、mtime 及 Unix `dev`/`ino`）计算链接键；仅在输出文件存在且 SHA-256 与滚动侧车 `.link.<out_hash>.meta` 一致时跳过冗余链接，产物被删除/篡改或工具链原地升级后自动重链。

---

<a id="ZIL-LOWERING"></a>

## 4. 目标表示、代码生成与后端边界

### 4.1 机器表示与 LLVM 目标语义

- **零默认运行时开销**：普通 `Value` 内联布局；`Desc` 表示为原始指针或既定索引，不默认引入 RC、世代或租约表；仅真实管理接口按固定 ABI 传递管理路由；具体闭包内联环境，仅 `dyn` / `OwnedDyn` 保存 vtable 与擦除指针；`Package` 从当前基址解析内部字段关系。
- **主原生路径（LLVM 23）**：采用工具链清单锁定的 LLVM 目标语义。普通 `Desc` 不降级为带活对象保证的 `dereferenceable`/`noalias` 指针，仅在实际读取/强窗口处发射精确范围证据；`Dormant` 描述不比较、哈希或序列化，作为不可解引用的不透明指针处理。
- **后端 Profile 边界**：C、Cranelift 或 Wasm profile 仅接受其目标语义可合法证明的子集，不得把 LLVM 指针模型直接套用到未证明合法的 C 指针搬运上；不支持的 profile 显式返回 `Unsupported`。
- **LIR 保持义务**：LIR 必须显式保留初始化、责任转交、全部出口清理、真实硬件同步与帧固定；优化不得依据普通 `Desc` 擅加排他属性，源程序安全不得依赖优化成功。无堆/无任务程序不链接无关 runtime。必要越界、对齐与 checked 算术检查不因 O2 降级为 UB。

### 4.2 机器策略与源语义解耦

- **多后端一致性**：`LlvmApi`（直接链接 `LLVM-C`）、`Clang` 文本路径及其他受支持后端消费同一 `VerifiedModule` 与显式目标/ABI/优化配置。LLVM IR verifier 仅核验 LLVM 结构约束，不替代 `zil-check` 安全证明。
- **失败关闭发布**：LLVM 核验失败、对象生成失败、链接失败或超时取消时，不得返回可执行新产物，也不得将旧成功文件充当本次构建结果。优化级别（`O0`/`O2`）、CGU 数、并行作业数与是否落盘 IR 不得改变语言语义。
- **大型 Copy 生成 ABI（`.lc1`）**：普通 Copy 聚合按值参数或返回值物理尺寸 `>= 4096` 字节时，统一采用指针传入 + 隐藏 out 指针返回并附 `.lc1` ABI 版本后缀（沿嵌套函数指针签名传播），保持源值语义与清理顺序，避免巨型 SSA 值桥接。
- **LLVM API 边界**：安全核与前端不接触原生 LLVM 指针；LLVM FFI 收敛于最小化、已审计的 `zil-llvm-api` / `zil-backend-llvm` 边界。并行代码生成单元各自管理独立 `Context`/`Module`/`Builder` 生命周期，不跨 Context 共享可变原生句柄。Linux 原生链接自动检测同级 `ld.lld`（`--ld-path`，保留多调用 `argv[0]`），无则回退系统链接器。

---

## 5. 实现取舍与当前配置限制

以下事项属于**当前实现策略或 profile 限额**，而非永久语言语义限制（当前 Alpha 支持边界以 [`docs/guide/supported-language.md`](../guide/supported-language.md) 为准）：

- **Trait 与多态策略**：保留名义 trait 与固定 impl 选择；需要多套算法策略时优先采用显式策略参数/包装类型，不引入作用域隐式搜索。泛型主路径保持单态化；任何按布局去重或存在化实验均不得合并不同 `Drop`、名义身份、错误或调用合同。
- **控制效果与代数效应**：当前 `effects(...)` 仅表示访问/失效/阻塞/挂起等静态效果合同，不包含可捕获/多次恢复的用户代数效应；若未来引入，须与 `ZIL-ASYNC` 的帧、取消、清理与重入规则联合证明。
- **当前 Profile 数值限额（非永久语言上限）**：例如 Alpha 档位下 64 态用户协议域、4 层嵌套字段投影路径、512 槽固定共享地址数组与默认编译工作配额（`validation/language-alpha-resource-profile.json`），均属于当前验证器/实现配置边界，可在不改变语言语义的前提下随实现演进放宽。
