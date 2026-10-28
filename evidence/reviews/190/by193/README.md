# 190 非作者独立审核（by193）

## 裁决与精确身份

**Pass，仅限 190 的这份冻结候选及本次实际检查范围。** 未发现需要退回作者的生产缺陷。此审核没有修改生产、规范、公共 schema、默认预算、Cargo.lock 或产品门，也没有执行全仓正式验收。

- 候选 source：`7491c212d392b12bb90f735b90f535066fa00a47`。
- 候选 production：`edd858b631c6020053ae107bdec59331925a419d`。至 source 的变化是测试、示例、consumer 工具和 191 审查证据，`crates/*/src` 无变化。
- 独审测试冻结 source：`12527d47bcb836231a38344bcefaf1ad47d1ed6c`。相对候选仅新增 `crates/zil-driver/tests/review190_by193.rs`，生产逐字相同。
- 独审者：`/root/resume188_acceptance`，193 作者，**未参与 190 实现**；193 另由 root 独审。
- 本冻结树设计 SHA-256：`8df5f5bb3cb25fdea8670124b09941713529c859ab18864d0fc87090864786d5`；规范登记 SHA-256：`333893800628a27c068534323c4782f2fcf3138a0e4e0317149af205c8803fda`。

主线工单后来已经加入 193 条款，本次候选并不包含 193 显式 slice 参数实现，不把更新后的主线规范摘要归给这个受测二进制。

## 实际执行

`records/frozen01/command/command.json` 的 cwd、source、环境、命令及原始日志摘要完整保留，执行时 Git clean。五套合计 **50 passed / 0 failed**：

| 套件 | 通过 | 性质 |
|---|---:|---|
| impl190_flow_raw | 14 | 独审环境实际重跑作者的 raw/Wire 攻击及预算用例 |
| impl190_flow_source | 17 | 独审环境实际重跑作者的源面及原生用例 |
| impl190_for_ranges | 10 | 整数范围、端点一次求值和回边状态用例复跑 |
| impl191_flow_transport | 2 | 原伪 Flow 仍在 Analyze、raw Verify、Wire Verify 被 SafeReject；图边界先于旧身份处理 |
| review190_by193 | 7 | 此审核新增的独立预期与攻击组合 |

`fmt01` 的 `cargo fmt --all --check` 与 `clippy01` 的 `cargo clippy --offline -p zil-driver --test review190_by193 -- -D warnings` 均 Pass，同样绑定冻结 source。作者报告的 Linux consumer **84 次 / 62 次真实分配失败**属于作者结果，**本次没有重跑该 consumer，不计为审核者的新运行**。Wine 由 root 另行安排，此处 Wine 和原生 Windows 均 NotRun。

## 新独审预期与观察

### 前缀、循环体、返回后缀不能互换

`exact_four_node_loop_distinguishes_prefix_body_and_return_suffix` 使用一段回调前缀、一个 while 循环体及一个返回后缀。其四个规范节点由源代码控制流手工写定：节点 0 有一次调用并到节点 1；节点 1 分到 2/3；节点 2 有一次调用并回到 1；节点 3 有一次调用后 Return。期望图没有调用任何生产归一函数。

在总共三个调用标签数不变的情况下，分别把前缀移到循环体、循环体移到返回、返回移到前缀。每份变造只更新模块 fingerprint，沿 raw 和 Wire 两路均实际得到 `E-VERIFY-CALLABLE-CONTRACT`。仅有总调用数或可传输身份不能授权错误的调用位置。

### 没有 Return 的子 helper

原源面有两个 helper 和实际资源 Desc。审核将子 helper 的循环头改成无条件回边，保留其循环前实际回调；断开的旧返回块仍保留合法 SSA 常量定义，移除不再活跃的旧 scope 退出。这是明确标识的手工 raw 输入，**没有运行无限循环**。

子图的三个节点及外层展开后的四个节点均手工列出。Analyze、raw Verify 和 Wire Verify 接受这个正控制。子图没有 Return，因此外层在调用之后写着的第二次回调与 Return 不可达，不能进入合同。删去子/父图的可达前缀、或伪造一条通向外层调用后缀的边，raw/Wire 均精确拒绝。

### 无返回前缀仍有实际资源效果

同一个合法 raw 输入中，回调通过 Desc 修改 origin，另一资源 other 的共享视图跨 helper 调用活跃，异源控制通过。审核仅把实际 `DescribeLocal(origin)` 改成 `DescribeLocal(other)`，保持合同和候选行不变，仅重绑 fingerprint。raw/Wire 均实际 `E-BORROW-CONFLICT`。这检查的是真实资源来源关系以及没有 Return 的可达前缀效果，不是只改一个抽象调用标签。

### Finite 与 FnOnce

有限 `for` 在第一轮无条件返回时，合同仍精确为零次 Return 与一次 FnOnce Return 两条 Finite 路径，没有升级为 Flow。即使实参为零，真正存在回边但没有回调事件的 FnOnce helper 仍在 Types 报 Unsupported；运行时值不取消静态消费约束。两者原输入均完整保留。

### 深度和工作预算

24 个静态 helper 构成直链，末端运行时迭代实参为 20000。Types、Analyze、raw Verify、Wire Verify 在 `max_call_depth=24` 均通过，在 23 均 ResourceLimit。Types 精确诊断为 `callable helper nesting exceeds selected bound`；另三路为 `helper dependency call depth exhausted`。没有把运行时迭代次数作为静态 helper 深度，也没有调高默认值。

另一段四层原输入逐阶段独立搜索并重测如下最小工作预算，N−1 均为 ResourceLimit，N 与全新默认预算请求均通过：

| 通道 | N 通过 | N−1 拒绝 |
|---|---:|---:|
| Types | 2148 | 2147 |
| Analyze | 3942 | 3941 |
| Verify raw | 6223 | 6222 |
| Verify Wire | 6223 | 6222 |

这些数值是本输入的实际工作边界，不是编译性能比较，不能外推为任意输入的最低成本。

### 新原生组合

`piece` 是接受显式 i32 值的 FnMut 范围 helper。`route` 的每一轮以不同长度、不同字符起点两次调用同一个 `piece`，中间直接调用回调；回调拥有资源并累计环境计数。独立 Rust oracle 按输入生成预期字节序列，并以闭式三角和核验累计返回值。

O0/O2 分别执行输入 0、1、2、3、4，总共 **10 次有限原生 Linux 运行**，均在超时界内 exit 0，stdout 精确匹配、stderr 为空。末尾直接调用产生 Z，拥有资源只产生一次析构 H。两次静态 child 调用的实参顺序、可变环境和正常清理实际贯通。没有额外的分配失败注入，也没有据此声称分配计数平衡或 Rust 性能同级。

## 源代码审核结论

| 关注点 | 实际实现路径与审核判断 |
|---|---|
| Types 独立构造 | `zil-types/src/helper_flow.rs` 从实际 CFG 和实际参数来源建立 fragments，预先检查 nodes/calls/edges 与工作预算；每个静态 child 调用分配独立偏移和 continuation；仅 Return 接回，Trap 保留终结；BFS 规范编号，Kahn 检测回边。 |
| Analyze 独立构造 | `zil-analyze/src/helper_flow.rs` 自建计数 rows，独立来源与 callee 顺序检查，独立 BFS/拓扑分类并逐项对比公开图。它不以 Types 提交的公开标签当作生产证明。 |
| Verify 独立重建 | `zil-verify/src/helper_flow.rs` 使用 `Body(block,from)` 与 `Child(block,call,node)` 虚拟位置，经自身 BFS 建图和迭代 DFS 分类。静态 call 位置进入键，child 的 Return 只能到该位置的 continuation。`zil-verify/Cargo.toml` 生产依赖仅 base/ir，没有 Types/Analyze 证明调用。 |
| 预算在复制之前 | 三模块的 Size/Counts/Allowance 对节点、调用、边及复制成本用 checked 运算；child 元数据遍历先扣费，再为拥有副本和重定位预留；队列/映射、规范化、循环分类、Finite 集合传播及对比亦有预算。独立 work/depth 边界与作者 pre-copy 变造均实际复跑。 |
| 公共声明不能授权 | Verify 的 claim shape 先做输入界限，所有 helper 独立重建并完整对比后，才运行 borrows/resources。原 191 伪 Flow 即使 Wire roundtrip/fingerprint 匹配也被拒绝。 |
| 前缀效果与来源 | 两个 resources 模块在已检查的整个可达 Flow 中查找调用，包含无返回前缀；forwarded helper 必须是本次实际参数来源，不能只凭 ClosureId 相等。作者不同环境同类型攻击和审核者实际 Desc 变造分别有执行结果。 |
| 范围前端 | `zil-expand/src/generic_check.rs` 和 `generics.rs` 的 range hint 只读类型信息；实际端点仍按顺序各展开/求值一次，回边核非 Copy 状态，索引仅在循环体作用域。原十项范围用例实际复跑通过。 |

## 早期失败与记录范围

`author31-01` 在原 7491c212 上实际三套 41/0，命令日志保留，最终 clean 冻结复跑另列。`independent01` 为 6/1：正确的 Types ResourceLimit 文案用 nesting，而审核断言错误地要求包含 depth。随后只把断言收窄为各通道精确消息，没有改源输入、生产或预算；`independent02` 7/0，最终 clean `frozen01` 7/0。草稿的未跟踪测试不冒充冻结受测源。

启动 handoff 检查为 Fail，原 JSON 保留；authority 指向主树，错误为继承的历史记录/稀疏 checkout 缺失 evidence 等登记问题，没有把它写成本轮语言编译通过或失败。

`collect.py` 保留原始命令、输入、诊断、Wire、关键 typed IR、真实输出和预期输出。`MANIFEST.json` 对每份复制文件记录原路径、字节数和 SHA-256，并逐字复核。可执行文件、对象和重复阶段展开不复制，只有摘要登记；原始日志末尾空行不做归一化。完整原始运行目录仍在 run-logs/190-review。

## Rust 四目标与尚未完成项

| 目标 | 本次实证及边界 |
|---|---|
| Rust 同类安全 | 当前有限语言片中的实际来源、调用前缀、FnOnce、独立 Verify 与预算界限有新增实证；不是全语言安全证明。 |
| 运行与内存成本同级 | 新组合有真实机器执行和一次析构顺序；没有同语义 Rust 时间、RSS、分配或尾延迟比较。 |
| 表达与维护更简单 | 泛型整数循环与可变回调已经跨两层 helper、两个静态调用位置组合；维护成本优势未测。 |
| 编译成本更低 | 本轮只检查实际工作预算与恢复性，没有编译速度或 Rust 对比采样。 |

工程增量是有界的静态 Flow 合同可以描述运行时循环，而不按运行时迭代次数展开；新增源面组合与独立重建已验证。产品级四目标合取及至少一项可重复净突破仍未证明。一般回调逃逸、并发/取消及完整 ZUI 前置闭合不在此审核结论内；完整语言仍未完成。
