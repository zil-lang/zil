# Alpha 施工方式与算法路径亲自审核

审核时间：2026-11-15（Asia/Shanghai）。执行者：root 本人。用户要求停止施工，并明确本次不使用子代理；已中断全部实施代理及两名刚启动的审查代理。本结论来自 root 对源码、全部 13 条工程失败记录、提交历史、构建记录的直接核查，不采用被中断代理的审查结论。未重新构建、测试或修改生产代码。

## 结论

实施路径已偏离最快完整 Alpha 交付目标。问题不只是切片细，而是相同语义缺少统一明确的合同、同一内核内部重复的资格门与实际消费者脱节、按编译阶段分工导致跨层缺口长期无人负责。保留独立 Analyze/Verify 是安全要求；让各自独立证成同一公开规则，不等于允许它们分别发明不同的生命周期与模式规则。无需放弃安全检查，也不应继续全面重写编译器或扩张表示体系。

最新完整受测候选为 243c05a737750c0091e7b719ba8003e9e6e027b9，CLI 构建成功，正向工程 1/13，预期 SyncDrop 反例 4/4。此比例是工程验收，不是语言功能完成百分比。标量及 C ABI companion 在 fbbc395c595881b3aaf06d59780d44b5c3a74af5 的原真实测试通过双后端与 O0/O2；不能外推成完整 Alpha。

## 已证实的问题

1. **独立实现变成语义不一致。** `zil-analyze/src/semantic/original/stores.rs:187` 为 Store 借用结果调用 `native_result_occurrence`，该函数包含 initializer 的 Statement/DeclareLocal 发生路径；`zil-types/src/semantics/original_store.rs:105` 则保留真实输入 descriptor 的 lifetime。generic-libraries 的同一完整 `&mut Row` 因此得到 implicit3 与 implicit4，随后被直接比较并拒绝。修复必须明确结果类、声明发生与实际来源的映射，不是按类型相同合并类，也不能在没有合同判断时强改 producer。

2. **只改入口准入，没有完成实际消费者。** 后端 `emit.rs:1039` 已允许经过完整验证的 plain qualified capture，而 `emit.rs:4170` 的 `capture_type` 仍用另一套枚举并在 4192 拒绝相同类型。真实 phantom 原程序最新仍失败于 `LLVM capture field BorrowedStorage`。这是 root 自己刚做的“完整 LLVM 消费”遗漏，不能再归因为正常的下一道门。资格判断、字段布局、ABI、读写投影及清理必须同批闭合。

3. **原事件生产与消费合同不一致。** Source `source_semantic_plan.rs:824` 将 navigation 的 TypeUse 只列为 directory site。Types 已对真实 Assign target 记录 local place，`semantics/lower.rs` 却主要重定位既有 program.sites，额外补点仅针对动态 owner。closures 工程普通 `count += 1` 的原 target recipe61/node148因此没有实际站点。需要统一事件角色的完备关系，而不是遇到每个语法形式就再补一处特殊观察。

4. **猜测先于证据。** root 将任意 Task 参数、结果、创建及 phantom 出现推广成 Suspend，导致合法 Direct 工厂回归，之后整批撤回；root 又在没有实际快照前推测 main 不在 Task worker 库存，后续诊断证明实际 worker=true、non-suspending SABI body=true。还发生 ID 字段、数组常量宽度、私有 API 等机械错误。今后先核公开规则、真实 IR 字段及精确反例，再改变算法；不可用候选编译替代基本接口复核。

5. **优化定位过晚。** ownership 的 protocol_cleanup 原耗 8,517,085；首次 Rc 类型改造仅省 213，完整分账后针对 exact query/category/leave 索引才降至 1,830,049。现在 owning-collections 仍在完整 World 阶段耗尽默认预算。必须先定位请求内真实重复工作，再缓存不可变全域事实，保留活状态、loan、tag、epoch 和责任检查。

6. **流程口头改了，验收单位没改。** 原工作流已明确按整链交付，但实际仍按 Source/Types/A/V/Task 模块派工，父钩子和 helper 分离等待，root 负责大量交接、推测与候选轮转。自 UTC07:06 起当前 feature 历史有 254 个 main 之外提交，完整工程仍 1/13。近期成功 CLI 构建通常约 26–62 秒，不能把主要停滞归因于宿主构建。未建立整链完成率的硬约束，导致“首个错误往后移动”长期替代可用性成果。

## 全部工程当前断点

| 工程族 | 阶段 | 真实首败 |
| --- | --- | --- |
| scalars | 完成 check | Success；C ABI/native 另有精确版本收据 |
| patterns | Analyze | signature value needs its ownership consumer |
| generic-libraries | Analyze | 原/实际类值 implicit3/4 不一致 |
| closures | Analyze | 原 recipe61/node148 缺实际操作 |
| ownership | Types | 完整 nominal 子域未知；真实 closure capture 遗留 Named(F) 需核 producer |
| dynamic-abstraction | Analyze | 接口未完成资格；消费者为 complete zero nominal dependency |
| async-ownership | Analyze | nominal value 缺确切原全域实例 |
| threads | Verify | 原 body 的 i32 与 ErrorUnion 关系不一致 |
| foreign-lifecycle | Analyze | plain nominal dependency 缺完整零域证明 |
| owning-collections | Types | 请求预算在 actual World/metadata snapshot 耗尽 |
| buffered-io | Analyze | borrow 缺确切声明发生 |
| iteration | Analyze | trait premise 缺 lexical frame |
| project-workflow | Analyze | borrow SSA 缺完整原发生值 |

CT、项目命令、独立 raw/Wire、双后端实际原生、取消/分配失败/一次清理仍须同版本验收，13 个入口不能单独取代完整 Alpha 的功能边界。

## 恢复施工时的执行方式

- 保留单功能分支；停止继续扩大 IR、协议、泛化目录或通用证明框架。已有实现不盲目推倒。先固定现有语义中的完整原域、外层发生、绑定、来源、实际权限以及固定 ABI 的区别与映射规则；共享公开语法与规则，不共享两核的证明结论。
- 以最多三条完整功能链编排：普通值/泛型/闭包/借用与工程组合；资源/Task/Dyn/线程及所有退出；请求规模/CT/正式联合验收。每条链有贯穿所有阶段的责任人，独立两核按同一公开规则各自证成。停止以单个 helper、一个 opcode、一个字段作为交付边界。
- 每次完整改动前列出形成、物化、资格、表示、ABI、实际访问、退出清理和后端入口的所有相关消费者；存在某个同类消费者未接通时不称 READY 整链。先一次改通逻辑，再集中测试真实源及最近反例；Rust 编译失败先统一收齐 API 错误，不运行旧 CLI 冒充新证据。
- 第一完整成果须是保持原复杂度的普通多模块工程走通到两后端运行及失败清理，而不是增加组件计数。随后沿共用核心将剩余工程整族推进；不改输入、抬预算、关闭 Some、删未用定义或减安全反例。13 正向工程、CT/CLI、raw/Wire 与失败清理必须在最终同一候选汇合。
- 对同一功能链重复出现“入口已放行、下游又按另一白名单拒绝”，立即审核完整路由；不得再作为独立小任务排队。对完整工程没有进展的连续改动窗口，重新审核实现假设与消费者完备性，禁止靠提交数说明进展。
- 报告只分已运行成功、实现待验、尚未实现。阶段前移和性能节省单列，不能宣称功能完成。完成目标不缩减，也不再无依据承诺具体小时数。

## 停工现场

源码 HEAD fd028ceb3807b986eb3600664f32266a837c8139。存在 Verify Task 端口/结果等未提交 coherent WIP，未编译、未验收。原始日志留在 immutable candidate 的 out/runs。所有实施/审查代理均停止，无 cargo/rustc/clang/zil 测试或构建进程。本次仅更新 goal 的审计和执行约束、保全既有源码现场，不恢复生产施工。

停工现场已保全：远端 task/language-alpha=fd028ceb3807b986eb3600664f32266a837c8139；固定 archive/source-history=90d7881baf28e97375cb2caec1e48d6b9bd1c1a9，source_base 同上，10 个修改/未跟踪路径、12 个恢复产物，manifest 与 patch 已从 GitHub 读回相等。备份不是验收。
