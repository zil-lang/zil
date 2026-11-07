# 068：跨编译阶段共享的实作与边界

基线 `400a082444275e1d0e2ad78c4910b9b6fc526be5`；生产候选 `0f59d0ad176aac7b2ff0b9b7a4b1f951dacb7879`。这份实验冻结在 schema/wire10；后续语言主线的 schema11 集成不倒写这组性能身份。产品门、独立 Verify 的权限边界保持原状。

## 1. 整条请求里确实有重复

只对 Parse、Types、Analyze、Verify 分别计时，不能自动揭示阶段之间的重做。我们沿真实 `CompilationSession::compile_path` 追到文件读取、import 发现、查询发布、独立 Verify 和原生后端，找到三个可直接消除的重复来源。

| 原来的行为 | 本片改动 | 保留的核验 |
|---|---|---|
| `load_sources` 完整解析每个文件用于发现 import，随后 `project_pipeline` 再解析同一 `SourceUnit` | 文件适配器保留 `Arc<ParseResult>`，通过请求私有参数交给正常 raw 查询 | 同样的 source/产物预检、查询结果 digest、依赖检查、配额、取消点；没有 Verified 对象流经通道 |
| 项目 `TypedProgram.module` 先深复制到统一 `finish_typed` 的 Arc，再为 Verify 拥有输入复制一次 | `finish_typed` 借用不可变 `&TypedModule`，取消中转复制 | Verify 的独立拥有输入复制和完整验证照常执行 |
| 每一阶段重新组合源内容、规则和 target 的查询身份 | 建立一次请求身份，逐阶段只替换 phase/target 依赖；Analyze 从其 Types 前驱取得相同身份 | 每阶段仍计算自己的真实输出指纹；输入身份不能充当结果指纹 |

这是一个小的纵向数据通道，而不是合并所有编译阶段。source graph、Typed IR、候选计划和已核权限的职责仍然分开。通道中只有已经构造过的不可变原始数据；它不让 Analyze 代替 Verify 作决定。

本片在 `session.rs` / `project.rs` 落地，未引入全局可变上下文、磁盘可信证明缓存、宿主 Rust 借用检查结果、隐式 ZIL 运行时引用计数或额外运行时句柄。

## 2. 可审计的工作量变化

以下是从冻结代码调用路径和实际查询事件推导的调用次数，**不是插桩计时**，也不声称复制成本等于总时差。假设 N 个唯一源模块、未请求磁盘 source cache、成功到 Verify，且整个请求未被取消。

| 操作 | 基线冷请求 | 候选冷请求 | 基线未变连续请求 | 候选未变连续请求 |
|---|---:|---:|---:|---:|
| 完整 syntax parse 调用 | 2N | N | N | N |
| 项目 TypedModule 进入统一 finish 的深复制 | 1 | 0 | 1 | 0 |
| `query_key` 完整输入/规则身份构造 | N+4 | N+1 | N+4 | N+1 |
| 查询身份构造中的 target fingerprint 计算 | 3 | 1 | 3 | 1 |
| raw 查询结果 digest | N+4 | N+4 | 0 | 0 |
| Analyze 求解 | 1 | 1 | 0 | 0 |
| 独立 Verify | 1 | 1 | 1 | 1 |
| 为 Verify 拥有 TypedModule 的复制 | 1 | 1 | 1 | 1 |

请求磁盘 source cache 时，每文件的磁盘 key 仍另外构造一次，与基线一致。单文件内存入口的 `query_key` 构造从5次降为1次；那条入口原本没有 import 发现的重复解析，不能给它记入少解析一次的收益。Build 的 Lower、后端检查、LLVM 输出和 clang/link 都仍执行。

若只有 M 个文件的 Parse 查询失效，完整文件入口的实际解析次数从 N+M 降为 N。未变请求的 M=0，因而这里没有减少解析次数；这也是必须把冷请求和未变连续请求分开报告的原因。

冷请求在这里仅指新建、空的 `CompilationSession`。操作系统文件页缓存不清空。未变连续请求仍从文件入口读取和解析 import 图；它没有实现跨请求 AST 快照读取缓存。候选在文件发现期间保存已解析语法图，特别在未变请求中会与旧 raw 缓存短时共存；本实验没有测峰值内存，不能宣称其下降。

### 配额不是“按少调用一次而少算一次”

原来的加载器与项目解析器分别维护自己的 `remaining` 限额副本，每份副本按唯一源文件累计 token/node。两次解析从来不是共享一个计数器扣两遍。本片保持这两个逻辑检查以及原有扣费和错误位置：加载器读取时扣总字节，按同样剩余 token/node/depth 解析；项目入口再次验证总 source 额度，逐文件按结果 token 数扣项目 front-end 额度。复用原始结果只减少实现层面的重复执行，不扩大接受范围。

所有原有取消 checkpoint 及 query event 的顺序保持。import 发现失败、预先取消或输出覆盖依赖文件，不得提前发布 raw 查询。Parse-only 只解析根文件，不读取依赖。公开的显式 `ProjectSources` 接口继续自己解析，不接受调用者提供的 ParseResult。

## 3. 现在仍存在的纵向成本

| 位置 | 当前成本及为什么没有直接删除 | 可进一步验证的方向 |
|---|---|---|
| `RawSnapshot::digest` | 除单模块 Typed 外，大部分 raw 结果仍先 Debug 序列化再 hash；它覆盖真实结果，不能用源码 hash 代替 | 为每类 IR 编写有版本的结构指纹；需对字段遗漏、顺序、碰撞域和独立结果变更做负向验证 |
| 项目 CT → Types | 模块、导出信息、Core 的若干结构重建；有的变化属于真实阶段产物 | 让 source / symbol / type 等稳定部分驻留在只读公共上下文，变更部分独立拥有 |
| Verify 输入 | 独立 Verify 接收拥有的 TypedModule 与 CandidatePlan | 若确有热点，可让已核包装器内部持有不可变 Arc，验证器自己绑定对象身份；不能用 Analyze 的证明缓存跳过 Verify |
| Lower / Backend | Lower 为 LIR 复制 functions/closures/resources；Backend 对 LIR 与 Verified 的绑定再核验 | 分离“同一不可变语义体”和目标相关布局/指令；共享不变体，同时核对精确目标/版本/身份 |
| `Database::read` | 每次读取走依赖闭包；当前 raw 查询粒度较粗 | 在 revision 内记已检查状态，配合稳定项目/函数身份；避免把旧图已检查状态带到新输入 |
| `Database::invalidate` | 查找依赖者时重复扫描 metadata | 反向依赖索引、局部 dirty 传播；先验证更新/撤销/取消时索引一致，再测大量查询规模 |
| 外部后端 | LLVM 文本物化、clang 进程启动、链接具有与前端独立的成本 | 测清 Build 中占比后才选择对象级增量或持久后端；前端局部提速不能承诺等比例缩短 Build |

不宜直接把全编译器压成单个求解器：它会扩大每次变更的依赖面，也可能让错误定位、取消、目标依赖和权限验证纠缠。更有价值的是共享**不可变表示和准确依赖**，同时保留**独立决策与检查**。同理，图切割/分量划分若产生共同 CFG、SCC、索引，可以作为 raw 数据复用；候选求解器的“该分量安全”结论必须由 Verify 自己检查，不能顺着通道变成权威。

## 4. 一手研究参照

2026-10-24 读取下列项目自身文档。本片并不声称复制其整个框架。

1. [Rust Compiler Development Guide: Memory management](https://rustc-dev-guide.rust-lang.org/memory.html)：rustc 的共享 `TyCtxt`、arena 与 interning 说明编译期间数据可以分配一次、由引用流经多处。本片借鉴“共享稳定的表示”，没有引入整套 arena 或把宿主生命周期当成 ZIL 证明。
2. [Rust Compiler Development Guide: Incremental compilation in detail](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html)：red-green 区分输入变动与结果实际变动，降低保守失效级联；跨会话还须解决身份稳定和序列化成本。当前 ZIL 查询仍保守观察完整 implementation，本片未实现函数粒度 red-green。
3. [Salsa overview](https://salsa-rs.github.io/salsa/overview.html)：外部输入更新与纯确定性计算分开；数据库中的不可变结果可以借用而不深复制。借用返回与身份复用可以独立采用，并不需要先接入全部 Salsa。该文档里的“verified”是增量缓存状态概念，不能等同 ZIL 的安全验证权威。

对应测试、冻结二进制身份、配对完整请求结果见 `evidence/impl/068`。性能结论以受测输入、请求模式和完整区间为单位，不把几项复制删除直接外推成普遍编译提速倍数。
