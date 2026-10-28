# 190 提案：运行时循环的同步 callable helper 合同

状态：仅提案与只读预检；没有删除生产门、修改公开 schema 或放宽源码断言。root 决定公开接口并单写；正式派工及依赖候选冻结后再实施私有纵切。

## 身份、依据与真实缺口

- 提案基线：`a07e0129a383fd95964875d85d7a91b383011e7e`，当前 rule `ZIL-LANG-1.4`、schema/wire **23**。
- 设计 SHA256：`8df5f5bb3cb25fdea8670124b09941713529c859ab18864d0fc87090864786d5`；规范摘要：`1068cda3fe79b906f49eeee4bd3f62863f9db4ae85e8a0dbaa3c439d310777b8`。
- 依据：`_zil_design.md` §9.3 明确 Fn 不代表任意重复，重复前后须保持实际入口条件；§9.7 要求捕获与本次显式实参共同核验；§10 要求有限来源节点、循环入口/每条回边及有限前缀核验。
- 188 当前源码 `187a79a55fe7970168b89b9dec165ec1f2e930cd` 提供逐阶段 callee-first DAG 与有限轨迹合成，但 root 独审发现 24 层 helper 实例化导致 Types 默认栈溢出，尚未通过独审。190 依赖其修复后的明确提交，不能把此候选当成已验收主线。
- 189 显式 Copy Address 参数仍由其作者实施；190 不改其来源代换或准入。若合流包含189，重复调用必须保留其当前实参义务；若尚未合流，不以本提案预先开放该形状。

已保留八个最小输入及实际 check 输出：`run-logs/190-preflight/probes472/REPORT.json`。为避免另建缓存，该预检使用精确187源码 `472b3ce052b850ef51fbfa3f8d818c51e080a112` 的 CLI，二进制 SHA256 `ce756a21ab82ff8a06a000b0e7d765c7b608b2657b1674e906426d6d79df7066`；**不是当前 main/188 的测试结果，也没有执行候选机器程序**。

上述八项及下述两项控制出口原输入/原 stdout/stderr 已原字节收入 `evidence/impl/190/preflight/`，包含来源和SHA清单；该目录明确标为 ReadOnlyPreflight，不能作为190生产验收Pass。

| 原输入类别 | 实际结果 | 对190写集的含义 |
|---|---|---|
| 普通具名 main 内 while/for 调用闭包 | Verify Success，各1例 | 常规循环与后端 CFG 已存在 |
| 有限两次调用 helper | Verify Success | 旧有限轨迹必须保持 |
| runtime while 的 Fn、FnMut helper | Types Unsupported，循环须合同 | 不能继续枚举全部调用字 |
| 有环境参数但零次 callback 的 while helper | 同一 Types Unsupported | 零调用与有限退出/潜在不返回仍须区分 |
| runtime for helper | Expand Unsupported，缺符号 statement 核验 | 必须补 generic_check 的 ForRange 检查 |
| FnOnce while | Expand Unsupported，外层消费状态不保持 | 不得把非 Copy 消费每轮重置 |

另保留 `run-logs/190-preflight/control-exits472/` 两个原输入：泛型 while 中的 `break`、`continue` 当前也在 Expand 的 parametric statement 门拒绝。首片默认把它们作为明确拒绝控制；图本身已能表达对应 Jump，但不能据此把符号正文里的出口义务当作普通语句忽略。若 root 将这两种源形状列入190成功范围，需明确批准其符号出口/回边不变式实现，而非只加 match 通配放行。

## 公开类型候选：有限轨迹与有限图的显式分支

推荐改 `CallableParameterContract.traces` 的**类型**，保留字段名与旧 `CallableTrace` 的含义。使用 enum 强迫所有旧消费者处理两个分支，避免把图误当空 vector。此处是接口候选，不是已写入代码的定义。

```rust
pub struct CallableParameterContract {
    pub parameter: u32,
    pub closure: ClosureId,
    pub kind: CallableKind,
    pub hold: Vec<u32>,
    pub call: Vec<u32>,
    pub writes: Vec<u32>,
    pub cleanup: CallableCleanup,
    pub traces: CallableTraces,
}

pub enum CallableTraces {
    Finite(Vec<CallableTrace>),
    Flow(CallableFlow),
}

pub struct CallableFlow {
    pub entry: u32,
    pub nodes: Vec<CallableFlowNode>,
}

pub struct CallableFlowNode {
    pub calls: Vec<CallableKind>,
    pub next: CallableFlowNext,
}

pub enum CallableFlowNext {
    Edges(Vec<u32>),
    Exit(CallableExit), // 已有 Return / Trap
}
```

各类型仅包含平坦索引与有界 vectors，不引入递归 AST、宿主指针或可信证明位。`Finite` 保持现有排序、去重、逐 requested kind 的精确调用字和 Return/Trap 意义；不改 `writes` 为实际效果，也不减少 Hold、Call、Cleanup。`Flow` 表示可达 helper CFG 的调用控制流，允许回边；不是次数上界、任意正则表达式或执行次数计数器。

这里的“精确”沿用有限合同的抽象层：条件分支保留其静态可达路径，直接 callback 是一次已核调用事件，未宣称求解任意数据条件、callback 必然返回或程序终止。callback 正文和实际所有权/借用另外独立检查。

### schema23 无变化的替代是否足够

不够。schema23 的 `Vec<CallableTrace>` 只表达有限个有限调用字；未知 runtime 次数可产生无限集合。空 vector、截断前缀、最大次数哨兵、把 Fn 当可任意调用、用已有 `Trap` 偷渡“重复”，都会改变既有语义。只继续拒绝全部循环，或仅展开已证静态有限上界，才能保持 schema23；两者均未完成这项 runtime 缺口。推荐由 root 发布下一 schema/wire 版本。

### 图的规范化顺序

1. 以真实 TypedCFG 的入口、block id 与 instruction 顺序构造临时片段。每个 block 入口存在一个片段；扫描时把直接实际参数 callback 的调用模式按序加入当前 `calls`。
2. 遇到向188已核 helper 传递**本次实际环境**的 Call，先结束当前片段，再插入下述 callee 子图，随后创建当前 block 的后续片段。每个静态调用点有独立拷贝，不共享不同调用点的 continuation。
3. 普通 CFG Jump 成一条边，Branch 按 then、else 次序成边；Return 和 Trap 分别成 Exit。相同目标边按首次出现去重，不按哈希集合顺序决定分支。
4. 从实际入口按有序边 **BFS** 编号，入口固定为0，首次遇见节点分配连续 id；只保留可达节点。每个节点 `calls` 保持事件顺序。不得做依赖实现细节的图最小化，也不删除空调用节点或空调用回边。
5. `Edges` 必须非空、目标合法、无重复，所有节点须按上述顺序可达；Exit 没有额外后继。图不能是空节点数组。`entry != 0`、孤立节点、非规范编号或伪造路径均拒绝。
6. 若组合后可达图无环，继续发布原 `Finite` 规范形式；若有环，发布 `Flow`。因此同一实际合同没有任选 Finite/Flow 的第二种编码。无环枚举仍受原逻辑预算约束，不因图可表示而偷偷放宽旧有限 profile。

root 可把类型/字段名调整为仓库统一命名；上述规范化与分支语义应同时冻结，三阶段分别实现，不能共享求证函数。

## 188 嵌套组合：首片选择有界展开，不发布图引用

公共候选不含 FunctionId 子图引用。逐阶段先独立核 helper 依赖 DAG；只有 callee 完成核验后才使用其对应合同。保留188的环境来源检查：实际形参位置、具体 ClosureId、实际 Value/Local 来源以及 requested kind 均匹配；同 ClosureId 的另一个环境实例不算同源。递归 helper SCC 继续明确拒绝。

- 子合同为 `Finite`：插入一个空调用分支节点，后继按现有规范 trace 次序排列；每个 trace 对应一个含完整 `calls` 的叶节点，出口保留 Return/Trap。
- 子合同为 `Flow`：按已核规范顺序复制其平坦节点与边，重定位节点 id。
- 仅子图的 **Return** 出口接父调用点的后续片段；子图 **Trap** 保持终止，不拼父后缀。子图没有 Return 时，不凭空添加返回边。调用前已发生的前缀和子图中的有限前缀仍存在。
- 子图回边保持回边；父 CFG 自身无环但调用含环 helper 时，父导出也可能是 Flow。父循环中的有限子 helper 则在每轮执行其真实有限序列。
- 逐副本预先检查总节点、边、调用事件和计费；有界展开超出既有逻辑配额就 ResourceLimit。不能先递归深拷贝再检查，也不能合并不同 continuation 来“节省”节点。未来若要增加 DAG 图引用以降低体积，应另定引用身份/出口/预算合同，本片不偷加。

这保留了有限 nested helper 的精确顺序与 Trap 停止行为，也让首片 Wire、raw、fingerprint 和独立核验保持平坦且可实现。

## 退出、零调用与 FnOnce 的义务

| 情况 | 合同与核验要求 |
|---|---|
| runtime 条件允许0轮 | 图必须存在真实零调用退出路径；拥有 F 的 helper 即使零调用，普通返回仍恰好清理其环境；借入 F 不取得清理责任 |
| 仅有无调用循环 | 保留空事件回边；不能伪造必然 Return。是否可能调用从图的所有可达节点决定，不能从出口列表推断 |
| 调用后无限循环或重复调用且不返回 | 包含有限可达前缀；即使没有 Return，前缀访问、存储与责任仍须合法。无需证明终止 |
| 正常 Return | 当前回调环境及各普通局部责任按真实 CFG/清理计划结算；保留 Helper Cleanup 的 Owned/Borrowed 区别 |
| Copy `E!T` 成功或错误返回 | 二者仍是 CallableExit::Return；没有新增 tag 相关合同。`?` 的错误边按正常返回核所有清理与可重复入口。不得把 error 当 Trap 跳过责任 |
| Trap | 保留 Trap 前全部调用与访问；不声明正常返回清理，不向父后缀续接；不增加 unwind 语义 |
| FnOnce | 首片 Flow 只准入 requested Fn/FnMut；reachable cyclic FnOnce helper（含零调用循环）继续 Unsupported，有限 FnOnce 仍至多一次。消费发生在回边路径的输入同时由 Expand 消费不变式、Analyze 与 Verify 真实 affine/资源状态独立挡住；把 metadata 改成 FnOnce 也不能过 raw/Wire |

首片对 cyclic FnOnce 的范围限制明确保留，不把它描述为“所有零调用循环均不安全”。同一 consuming callback 在无回边的有限路径零次或一次仍由原合同支持。

## 可重复性的证明不来自 Fn 标签

公开图只描述真实调用控制流。每个具体实例还必须通过独立 Verify 的资源、初始化、来源、借用及实际效果检查；任何 public claim 或 Analyze 结果均不授予权限。

1. **环境责任。** 现有 Analyze resources 的普通返回保存检查与 Verify `resources::Transfer::block` 的 `capture_debt`/`retained_debt == LIVE` 检查是必要条件。共享/独占入口每个普通出口都恢复完整拥有字段；FnOnce 的剩余责任由消费调用方结算。不得每轮重置 capture flag、复制 Managed 或把新 fresh 冒充旧实例。
2. **循环入口与回边。** 现有两套资源状态 worklist 独立从真实 CFG 合流；每个消费、替换、构造、错误返回和 Drop 都在每条可达回边后的可能状态上检查。190 要用真实 source/raw 反例确认这些检查对 helper 循环生效，不能仅确认“有一个环”便跳过实际 transfer。
3. **外部目标状态。** 实际 Desc/Address 的 read/write/invalidation 必须从每个具体 callback 及普通 helper 重建，保留182/187的实际来源、逐捕获 effects 和 Apart 代换。当前有限原语的状态保持、完整 Store 前缀、ByteBuffer 失败保持等须由其既有真实核验建立；不能因环境是 Fn 就假定外部目标仍 Live。若遇到未表达的 Desc Live→Done、跨轮责任路由或其它后态不能回到下一入口的能力，继续明确拒绝，不能用图补造不变式。
4. **调用窗口。** 循环中每次调用共同纳入 captures 和本次显式 arguments；继承189已核入口时，保留其 Address 权限与重借。循环体结束不是自动释放所有 loan 的依据；跨回边仍被用到的窗口由有界 liveness 保持。新一轮重分配、push/reserve/truncate/clear/give/drop 不能恢复旧 view。
5. **退出与后继。** 最后一次调用之后的合法 owner 使用、give/drop 必须继续通过；反之回边后仍需调用时不得提前失效。错误出口同样恢复可重复环境。仅 Trap/真实无返回路径无普通返回承诺。

首片不增加逃逸环境、异步/重入、保存借片、参数借入返回或新的动态类型形状。已有 finite lanes 的可用能力保持；无法闭合的 loop lane 用明确拒绝说明具体前置。

## 全部 traces 消费者与间接边界清单

下表以提案基线全 `crates/**/*.rs` 搜索和188候选对应源码为准，不含测试中的故意伪造字段。

| 位置 | 当前实际用途 | 190 必须处理 |
|---|---|---|
| `zil-types/src/helpers.rs` `traces`，188 `forwarded_traces` | 生成有限轨迹，真实输入环境来源，nested trace 接续 | 生成规范 Finite/Flow；保留每次调用来源与模式；有界前置计费 |
| `zil-analyze/src/helpers.rs` `check`，188 `forwarded`/`dependency_order` | 独立比较有限计数/出口与 public trace | 真实 CFG 独立重建两分支，callee-first，错误图不当空 |
| `zil-verify/src/helpers.rs` `check_claim_shape`/`traces`/`check_module`，188 `forwarded` | raw 指纹前界限；独立 CFG/来源/合同重建与完全比较 | 完整 shape 计费在 hash 前；迭代图重建/规范化/环检查；不消费 Types/Analyze 证明 |
| `zil-analyze/src/resources.rs` Call 分支 | `traces.any(nonempty calls)` 与 writes 上界共同决定 Desc 写责任 | 两分支都检查**所有可达调用节点**，包括不返回前缀；不得用空 vector 表示循环 |
| `zil-verify/src/resources.rs` Call 分支 | 同上，且独立计费 public trace/writes | 独立得到 may-invoke；继续核真实 CFG 资源状态。旧 finite 语义不变 |
| `zil-analyze/src/budget.rs` | 预收费整个合同及每个 trace.call | 预收费图的节点/边/调用字，所有算术 checked；默认预算数值不变 |
| `zil-ir/src/lib.rs` `encode_callable_contract` | TypedModule 指纹与专用 callable 指纹 | 编码分支 tag、entry、每节点有序 calls/edges/exit；建议 callable 指纹 domain 升版 |
| `zil-ir/src/wire.rs` Writer/Reader `callable_contract` | 公开 wire 往返，长度先界限后分配 | 两分支独立合法 tag、预计费、索引与长度界限；root 升 wire/schema；raw 不绕过同一 shape 检查 |
| `zil-driver/src/contracts.rs` `publish`/`callable_json` | 仅 Verify 后 clone/export、合同 fingerprint 与 JSON 展示 | JSON 明确 Finite/Flow，不丢 graph；导出绑定全合同，建议发布 domain 同步升版 |
| `zil-ir/src/program.rs` `ScalarInterface`/`FunctionCallableContract`；`zil-driver/src/lib.rs` 输出 | 整体携带和展示公开合同，未单独读 trace | 保留 None（未核）与已核空导出区别；与上行 exporter 连通 |
| `zil-verify/src/lib.rs` `VerifiedModule::callable_contract` | 仅从已核不可变 module 暴露合同 | 不新增 raw graph “已核”入口；Graph 的未核 claim 仍不能取得 token |
| `zil-analyze/src/retained.rs`、`zil-verify/src/retained.rs` | **不读取 traces**；factory 与 helper contract 互斥，检查 CallableReturnContract/lending | 保持 helper 不变成 factory，不能用图延长外部捕获寿命或把借入环境存入 owning 返回；本片无必需格式变更 |
| `zil-verify/src/borrows/loans.rs` | **不读取 traces**；用 callable parameter 取得本次环境 Hold，辨别 owning publication | 两分支都保持实际环境/参数的当前调用窗口，不依据 Flow 省略 Hold |
| `zil-verify/src/native_values.rs` | **不读取 traces**；约束普通 native 函数不得冒充 helper | 不改合同类别边界 |
| `zil-lower/src/lib.rs`、LLVM emitter | 生产路径不读 traces；Lower/后端绑定精确 Verified/LIR CFG，循环已有机器表示 | 不增 ABI/布局或伪展开机器调用；真实 runtime 测试证明常规 lowering 生效；发现具体问题再授权写集 |
| `spec/interfaces.json` callable wire/contract/scope/interface 条目 | 公开语义说明目前仍含旧有限限制 | root 在新接口更新 Finite/Flow、188组合、187/189已核范围、错误返回与导出绑定，刷新规范摘要 |

`CallableReturnContract` 本身没有 traces，不需加入循环字段；其当前 lending/cleanup vectors 和指纹不得被本片覆盖。测试夹具中的 `CallableParameterContract { traces: ... }` 需要 root 的机械 Finite 包装迁移，并保留原输入、原拒绝意义及预算失败记录。

## 资源界限与独立实现约束

建议复用已有 `Limits`：图节点受 `max_nodes` 限制，调用事件总数受 `max_instructions` 限制，边及全部扫描/复制/队列/比较计费进入既有 `max_analysis_steps`；逐函数和全模块的累积长度都用 checked 加法。精确额度由 root 接口定义统一，不能各阶段各猜一套。Wire 读 vector 长度后先检查和收费再分配；raw Verify 在计算模块指纹前完成同等界限。

SCC/可达性/BFS 使用显式队列或栈；不对 graph cycle 递归，不按 runtime 迭代次数追加节点。源面的 helper 实例依赖深度继续遵守188修复后的显式计划；不提高宿主栈或默认逻辑预算掩盖深度问题。重复子图展开的全部真实工作必须计费，精确旧预算断言如因必要工作变化需要迁移，由 root 逐项批准并留存原失败。

**复制前的数量和 depth 检查是强制步骤。** 对每个静态子 helper 调用点，先用 checked 加法计算完整子图副本的节点数、边数、调用事件数、Return 接续边及重定位/队列辅助项，检查本函数与本模块余量，并为实际扫描、所有 owned clone/拷贝、重定位和结果比较分别完整收费；之后才建立副本。禁止先 clone `nodes`/`calls` 再发现超限，也不只给最后压缩后的图计费。实际 helper 依赖 DAG 的最大活动调用路径深度须先独立计算并限制于现有 `max_call_depth`，包含有限/循环 helper 的任意组合；引用深度通过不能免除复制体积界限，复制体积通过也不能免除路径深度界限。这里的 depth 是静态 helper 调用嵌套，不是 runtime 回边反复经过的次数。Flow 的平坦索引数组无需递归 Wire/raw 解析，图遍历自身用 visited/queue 与已核节点总量有界。

## 建议写集与实施顺序

**root 单写公共接口：** `zil-base/src/lib.rs` schema、`zil-ir/src/lib.rs`/`wire.rs`、必要导出 JSON/fingerprint 与接口文件/规范摘要；机械 Finite 构造器迁移；新公共类型的两个分支都应显式处理，循环生产可暂显式 Unsupported。

**190 私有生产建议写集：** `zil-expand/src/generic_check.rs`（真实 for 符号检查）；`zil-types/src/helpers.rs` 与新的私有 flow 子模块；`zil-analyze/src/helpers.rs`、`budget.rs`、`resources.rs` 与其私有 flow 子模块；`zil-verify/src/helpers.rs`、`resources.rs` 与其独立私有 flow 子模块。若循环反例暴露实际资源/借用不变式漏拒，先保留最小输入和定位，再申请具体 `borrows/**` 或资源模块写集；不提前重写182/187/189。

源码 for 检查必须只准入当前已支持的整数半开 range，按现有普通 for 的类型规则检查 lo/hi，两个端点各求值一次，循环 index 不泄漏出作用域，逐轮保留外层非 Copy/Store/Package 的真实不变式；空 range 不把未检查正文隐藏掉。不新增通用 iterator 协议。

**验收与证据写集：** 本片 driver source/native/raw/Wire tests、四模块消费者、限定 runner、changes/evidence；涉及旧 `impl126_helper_sequences` 的循环拒绝条目仅在本片实际安全正例通过后由 root 授权同源迁移。Lower/LLVM、默认预算、正式门与工单均不由作者擅改。

顺序：188栈修复独审并合流 → root 公共接口冻结 → 新190作者基线绑定并提交私有提案补充 → Types/Analyze/Verify纵切与 raw/Wire → runtime消费者/失败清理 → 冻结源码交独审。189按精确块串行吸收，不整文件覆盖；不将本提案的推理计为测试Pass。

## 必须闭合的限定验收族

1. 输入决定0/1/2/多轮的 while/for；Fn只读、Fn带外部Desc写、FnMut计数捕获；O0/O2机器结果；range端点带可观察副作用且只执行一次。
2. 零调用 helper（仍可能普通 helper 写实参）、空循环、提前 Return/Copy错误返回、Trap 与纯不返回图；现未准入的泛型 break/continue 保留明确拒绝控制，若另获批准则补其独立出口不变式和机器正例。不返回仅做有界编译/核验，不能把无限机器运行当验收。
3. 循环→188有限helper→callback、有限helper→循环helper、两层组合、子Trap不执行父后缀、零次分支；Forwarded实际环境同实例/伪同ClosureId反例。
4. 每轮局部资源、拥有环境最终Drop、借入环境无Drop、错误返回完整责任、分配/增长真实失败后的状态与唯一清理。每条 backedge 的已消费外层Managed和未恢复环境必须拒绝。
5. 同源/异源ByteBuffer与普通Desc当前窗口；Capture/Capture及Capture/显式argument；隐藏普通helper写；跨回边旧view在重新分配后失效；最后一次调用后允许合法owner动作。
6. FnOnce回边消耗保持拒绝，旧有限0/1次仍正例；raw更改call kind/环境实例/责任状态不可绕过源面拒绝。
7. raw与Wire分别篡改删回边、删调用、改出口、错索引、孤立/非规范节点、伪空图、Flow/FnOnce、错误Hold/Cleanup；分别正确拒绝。长度/节点/边/调用预算的精确边界和热冷一致；不以溢出/宿主栈错误代替ResourceLimit。
8. public interface 的 None/verified区分、任一graph事件/边/出口改变即改变合同指纹；真实源构建后机器层仍由普通CFG执行，无隐藏堆盒/RC/锁或编译期展开运行次数。

四个 Rust 目标反思应在实际交付时按受测范围填写。本提案预期改善普通库表达和可解释拒绝；尚无190的安全、机器性能或开发体验实证，完整语言与产品门状态不变。

## 2026-10-28 接续：ForRange具体展开的整数上下文

符号正文已接受 `0..identity(bounds.high)`（high:i64），但具体AST展开仍把Range交普通Binary路径，先把左0猜i32后把错误的i32期望交右generic调用，真实源测试9/10保留。按root 3d10918派工，只在For语句增加范围专用入口：先以有界只读元数据查询端点已知类型（未定无后缀整数不先定宽），再左→右各调用一次正常expr_expected。元数据查询不实例化函数、不改AST、不消费资源、不运行端点，实际表达式与后续Types仍重检真实类型/权限。普通Binary行为、188具体helper调度、默认额度和loop责任不变式不变。
