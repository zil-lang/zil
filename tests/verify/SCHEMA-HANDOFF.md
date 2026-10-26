# B0 / T05 公共 schema 与可信边界交接

本文件记录已实现的结构和仍未冻结的合同，不替代 `_zil_design.md`、`spec/interfaces.json` 或任务验收裁决。

## 当前结论

**完整 `schema.v1` 尚未交齐，B0 不应因 scalar 路径可执行而被判定为完成。T05 也未完成。**

已实现的生产路径是无地址、无资源、无捕获环境的 scalar CFG 核验，以及闭合、无分支的 scalar CTIR 核验。来源、窗口、资源、闭包、异步和 SABI 的结构或核验缺口均不能通过 `Unsupported` 算作相关验收成功。

## 接口逐项对照

| `spec/interfaces.json` 合同 | 实际实现 | 尚缺的结构或生产连接 |
|---|---|---|
| SourceUnit | FileId/ModuleId、UTF-8、Span、诊断、独立 SourceMap 旁车 | SourceMap 已提供实际校验和追踪；调用方仍须将 target 对应到真实 AST 节点，表本身不证明 AST 存在或语义正确 |
| CoreModule | NodeId、普通声明、显式 receiver shape、normalized tree、Sequence 单次求值临时、move closure 标记、CompileTime 节点 | 尚无绑定到每个名字的 scope/hygiene identity；`ScopeId` 类型单独存在不等于已实现卫生绑定。未支持声明通过明确 Unsupported 保留 |
| TypedCFG | dense Function/Local/Value/Block ID、scalar 类型、指令、普通控制边、SSA、初始化 | 无 typed Place 表；无 receiver dispatch/capture env 表；无 normal/error/panic/cleanup/resume 分类边；无显式责任转交、窗口或清理 effects |
| CallableShape | 函数指针与调用种类枚举、参数/结果、捕获类别和模式 | `CallableContracts` 仍为字符串集合，不能作为已冻结 Hold/Call/Cleanup/Resume 语言；缺精确捕获路径、storage policy、动态 ABI、opaque identity 的完整出口约束 |
| ExportedInterface | 已有 ScalarInterface：真实模块/声明身份、公开标量签名、签名与实现分开的指纹；跨模块调用已生产贯通 | 完整资源接口仍缺布局、receiver/参数/结果模式、OriginFlow/BindingRoute、effects、泛型 bounds/witness，以及完整 semantic/ABI/private/source/build fingerprints；标量接口不能代替这些字段 |
| CandidatePlan | 每函数/块的 definite initialization 集合、版本和模块/目标身份 | OriginClaim/ResponsibilityClaim 当前只是简化 raw 候选记录；缺有限来源图、分配历史、窗口转移、按出口状态、ordered prefix、selector、cleanup 和 certificate opcode/version |
| VerifiedModule | 私有只读包装；独立 scalar 核验；原始字节恢复后完整重核验 | 未覆盖上列高级语义；nonempty resource 候选明确 Unsupported，未获资源安全背书 |
| VerifiedCTIR | 私有只读包装；允许操作、类型、定义顺序、所选目标、执行/值/核验预算；闭合 raw wire 解码 | 不接受 CT dependency capability。raw `completed: true` 不代表已完成查询权；缺可验证 completed-query snapshot 和有限外部输入能力 |
| LIRTarget | scalar CFG 快照、显式 TargetProfile、scalar layout；后端匹配已核快照 | 尚无资源/闭包环境/协程帧布局、SABI、全部结果清理和同步证据。精确工具链输入当前由构建证据记录，未进入完整 LIR 合同 |
| QueryResult | zil-db 分开的结果指纹、依赖、query snapshot；driver 会话复用 raw 阶段快照，多模块输入/路径映射改变时保守失效相关整图；Verify 每次重跑 | 资源合同、ABI、源位置等细粒度依赖和完整 Typed/CT 持久缓存仍待贯通；缓存摘要不创建已核能力 |
| ExecutionEvidence | runner 实际命令/退出码/输出/执行及断言数；本目录边界脚本记录实际编译命令和拒绝码 | 正式任务验收由集成者执行；本目录 hand-built IR 和 compiler-boundary 证据不能替代 ZIL 源码或 native gate |

## 已冻结并可调用的新接口

### SourceMap

`zil_base::SourceMap { module_id, entries }` 的目标可为 `Node(NodeId)`、`Parameter { function, index }` 或 `Span(Span)`。每条记录保存 origin、ExpansionRole、related spans 和 parent。

`validate(&[SourceUnit], &Limits)` 检查：

- entry ID 必须等于索引，parent 必须指向更早的记录；
- target 唯一；引用文件存在且 FileId 无歧义；
- origin/related/generated span 的范围和 UTF-8 边界合法；
- 节点和工作预算满足限额。

`trace(&SourceTarget, &Limits)` 返回 target 到根的链，独立检查所经 ID、parent 顺序和查找/深度预算。未先 validate 的恶意映射也不会因 parent 越界或循环而获得无限追踪。

该数据只提供诊断来源，没有权限含义。

### 原始 wire v1

`zil_ir::wire` 提供：

```rust
encode_ct(&CtModule, &Limits) -> CompileResult<Vec<u8>>
decode_ct(&[u8], &Limits) -> CompileResult<CtModule>
encode_module_candidate(&TypedModule, &CandidatePlan, &Limits)
    -> CompileResult<Vec<u8>>
decode_module_candidate(&[u8], &Limits)
    -> CompileResult<(TypedModule, CandidatePlan)>
```

格式使用不同 magic、显式 wire version、固定宽度小端整数和带长度的 UTF-8 字符串。未知 tag/opcode、缺字段、非法 bool、反向 span、尾随数据和配额违反均显式返回诊断。集合长度在分配之前与剩余字节、节点数和分类限额对照；递归类型具有深度上限。

CT wire 的 `CERT` 段是每条操作的 `(ValueId, Type)` 原始类型声明，正是现有 `CtInstruction.ty` 的运输形式。缺段、覆盖不足或 result 身份错配会拒绝；声明存在并不证明它正确，解码也不会修复错误声明。`verify_ct` 仍逐条独立核验。此段没有声称实现资源证明、完整 CT capability 或窗口证书。

没有任何格式编码 `VerifiedModule` 或 `VerifiedCTIR`。

### 唯一的恢复信任路径

```rust
zil_verify::verify_module_bytes(bytes, selected_target, limits)
zil_verify::verify_ct_bytes(bytes, selected_target, limits)
```

这两个函数先恢复 raw 候选，再调用与新编译相同的 `verify_module` 或 `verify_ct`。原接口保持不变。正确的 wire/hash/模块身份不能替代内部事实校验；测试实际写入、读回有正确摘要但错误初始化事实的缓存，仍得到 `E-VERIFY-INITIALIZATION-CLAIM`。

## T05 登记样本的状态与边界

### PLAN-T05-003

`tests/verify/check-boundary.py ct` 实际构建 zil-base、zil-ir、zil-verify，编译并运行下游边界程序。观察项包括：允许 CT 的成功对照；明确禁操作；错误类型声明和未来/不存在 operand；缺 typed claims；未知 opcode/version；截断/尾随数据；字节、执行和集合配额。

配额反例验证的对象是配额机制本身，不把 ResourceLimit 源程序编译当作通过。证据级别为 `RawCandidateBoundary`。

### PLAN-T05-004

`tests/verify/check-boundary.py isolation` 实际运行磁盘 raw candidate 往返与重核验，再编译六个下游反例，分别要求确切的 E0451（私有构造）、E0596（只读写入）和 E0277（raw 数据不能无核验转换为 Verified）。同一过程包含合法 wrapper 返回和只读使用的成功对照。

脚本使用新的临时 target，确认真实构建产物仅为 zil-base、zil-ir、zil-verify；不以静态依赖策略文件代替实际隔离。证据级别为 `CompilerBoundary/RawCandidateBoundary`。

### PLAN-T05-001 / PLAN-T05-002

仍未实现，未创建 fixture。以下最小反例要求新增结构并实际核验后才能推进：

- 当前来源候选集合只含一个名字，却代表循环中多个历史分配；单凭集合大小不能把 Many 改成 One 并强更新。
- 两个描述来自相同真实目标，但引用不同候选节点；单凭 node ID 不同不能证明不交。
- 引用仍存在时发生 reserve/realloc；重新创建相同名字的映射不能恢复旧窗口。
- 某条 error/panic 出口未清理责任，普通 Return 路径均清理也不能通过。
- carrier 被复制后两个出口各自 cleanup；记录一条最终 Done 状态不足以证明唯一责任。

这些结构必须与实际 TypedCFG 操作和所有控制出口对应，不能让候选自行声明事实后由 verifier 接受同一字符串。来源、窗口、责任和清理边的 schema 扩展需集成者统一定案；当前 blocker 保持显式。
