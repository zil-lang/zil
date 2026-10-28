# 195 作者交付：同步 helper 的 break / continue

状态：作者限定验证已完成，等待非作者审核、主线合流和 root 的正式组合门。完整语言 NotSatisfied、产品 NotRun、B0 Doing 保持。

## 冻结身份

- 派工基线：`40abb53527aa45c89f6249162c9a69fea5fd80ad`，包含已独审190/192和当时的193候选。
- 第一版符号控制生产：`59683f43a29e26f0f4bc926d99ffefed6e7a4c28`。
- 最终完整生产、测试与消费者冻结：**`1a890dabc4eb04b26570439718b0a9432511fa95`**。最后一处生产变化是给循环入口及退出合流的完整 State 复制补齐计费。
- 当前规范摘要：`48641079ef2ecc84aa50a0a9eafd83ae844f0d3de7924fcc8cb3e2fef3f9db5f`；规范集合摘要：`6488fa9f20121c4268f13153c6daecbfd20da5eb5e8aa86630620413d41edadb`。
- schema24、公共接口、Wire、Types / Analyze / Verify / LLVM 和默认 Limits 均未修改。生产写集只有 Expand 的 `generic_check.rs`、`generic_check_enum.rs`、新私有 `generic_loop_control.rs`。
- 最终 CLI 由 clean 1a890dab 实际构建，SHA256 为 `c6e85adbb018f73709103bda438d1009394bf69e690557aab0fa4a0c76aea4ac`。全部最终作者门绑定这一源码；后面的文档/证据提交不重新冒称执行过。

精确命令、输入、诊断、选定阶段产物及逐文件 hash 见 `evidence/impl/195` 的 `SUMMARY.json`、`SOURCE.json`、`ORIGINAL-FILES.json` 和 `FILES.json`。

## 实际行为与状态义务

泛型正文现在以 `BlockFlow` 分别记录普通 fallthrough 和函数 Return。最近循环帧分别保存 break 出口与 continue 回边的实际 State；两种控制都终止当前路径，但不制造函数返回。If 与穷尽 Match 只合并继续执行的分支，返回路径不污染存活分支，循环控制的状态也不会从分支合流中丢失。

每一条 continue 和正文普通 fallthrough 都检查已有外层非 Copy / carrier / Store 回边不变量。break 不被当作回边，但参与循环后的可能移动、来源、失效与新鲜状态合流。零次入口和已核回边仍保留，不能把一个分支的消费重置为未消费。循环后的名字由外层词法绑定决定，索引及正文影子局部不逃逸；外层旧 id 和 managed field 的责任事实不会因同名局部出现而消失。

循环帧、退出集合、引用表扩展以及完整 State 复制都先支付既有 Budget。State 复制包括可能已被影子名称遮蔽的 moved / Store owner id、来源键和局部名称；没有提高默认额度，也没有以 runtime 迭代次数消耗编译期深度。

真正的控制、清理与借用仍由原有后续阶段处理。While 的 continue 回条件头；For 的 continue 经过独立 step 加一，端点仍左到右各一次。scope 清理生成既有 Drop / StorageDead / ScopeExit 和 Jump，三套 Flow 语义继续独立核实际 CFG。原生输入覆盖最近循环、回调前后退出、混合 Return / Break / Continue、最后使用后的借出、持有旧借出后的冲突以及拥有/借入环境的清理。

仍有环的 FnOnce 维持明确 Unsupported，包括零回调事件的循环；实际无环的“进入后调用一次即 break”保持 Finite 零次或一次合同，零调用环境也会清理。没有改变194的签名、Copy bound 或 nested generic/callable 代入。

## 同一冻结的实际验证

| 验证 | 结果 | 原始记录 |
|---|---|---|
| 新符号状态测试 | 11 / 0 | `joint01`，含未调用模板、每条回边、break 后 may-move、Store 观察/新鲜性、词法作用域与预算 |
| 新源码控制测试 | 12 / 0 | `joint01`，82 次真实 O0 / O2 原生执行 |
| 新 raw / Wire 测试 | 8 / 0 | `joint01`，真实源生成的 cleanup / scope / debt / Flow 候选及变异 |
| 六套相关旧测试 | 73 / 0 | 泛型基础/回归、178 Store、186 拥有成功、190 Flow 源面及 ForRange |
| 联合总计 | **104 / 0，ignored 0** | 9 套，clean 1a890dab；新31与旧73是不同测试，不累加先前重跑 |
| 最终 CLI | Pass | `cli02`，clean 1a890dab |
| 四模块消费者 | **72 次原生执行，含50次实际分配失败** | `consumer03`，O0 / O2；源码、CLI 都为 1a890dab |
| Clippy | Pass，`-D warnings` | `clippy01`，Expand、driver lib、新三套测试 |

新源码82次与消费者72次合计154次实际原生执行。相关旧测试中的原生次数没有混入这个新增计数；早期候选的重复执行也没有累加。作者机器范围是 Linux x86_64，没有本片 Win64 / Wine 作者结论。

四模块消费者由 State 拥有独立 observed Buffer 和具体回调；回调拥有另一 Buffer 与可见 Ticket。每次进入循环另建 scratch Buffer 和可见 Iteration 资源，index 1 continue、index 4 break，普通迭代通过 Finite leaf 调用 FnMut。stdin 的0 / 1 / 2 / 3 / 7决定实际路径，另有非法输入9。每一个实际分配调用点都做失败注入，逐次核对输出、退出码、alloc_calls、allocated = frees 和 live = 0。这样同时检验跳过、提前退出、普通清理与失败时的真实责任，而不是只观察一张 Flow 声明。

最终精确工作前沿如下；均实际检查 N 成功、N−1 ResourceLimit、之后 fresh 默认请求成功，没有调高产品阈值。

| 层次 | 最小额度 |
|---|---:|
| Expand 节点 | 769 |
| Expand 工作 | 2,242 |
| Verify raw 工作 | 44,926 |
| Verify Wire 工作 | 44,926 |

raw 反例删除真实 scope exit、遗漏 StorageDead、关闭错误 scope、遗漏拥有资源 Drop、把 Jump 声称为 Return cleanup、改变 break / continue 的实际目标但保留旧 Flow、用有限空轨迹冒充实际循环。Return 标签与 Jump 不符由 Verify 独立拒绝；Analyze 的 scope 检查不解释这个标签，该特例的 Analyze 成功被准确保留，没有冒称三个阶段都拒绝。无安全差异的 Break / Continue 标签互换没有被强行当作负例。

## 原始失败与边界

`symbolic01` 是原190范围10 / 0、新符号9 / 1。唯一失败是新测试误把“先把无 Copy 约束的 current 移到 temporary，再向已移动 current 赋值”当作支持；既有 generic place 检查在 continue 之前拒绝。原输入逐字保留为该既有边界的独立负例，没有修改 `generic_places.rs` 或扩大重新初始化能力。

`source01` 的10 / 2来自新最大整数夹具用了不支持的后缀拼写，以及静态程序不读 stdin、提前结束导致新测试管道 BrokenPipe。修正为已有明确 i64 参数上下文中的无后缀整数，并为静态程序使用明确无输入执行；原失败和输入保留，生产没有为这两个问题放宽。

`consumer01` 的 Python 多余右括号在任何 ZIL 命令之前失败，语言程序执行数为0。`consumer02` 已在早期计费版本通过72 / 50；最终生产补齐计费后重新构建 CLI 并以 `consumer03` 完整执行，正式作者结论仅归于最后冻结。

共同磁盘曾在 `joint01` 完整 Pass 后耗尽；没有把这次观察记成未发生的测试失败。root 只清理关闭的构建缓存后继续最终 CLI、消费者和 Clippy，未删除本片源码、原始输入、历史失败或证据。

190 的旧控制测试只迁移原两个未调用模板的接受断言，两个输入仍逐字相同，其它原断言不动。预检14 / 15 的原持有借出输入也逐字保留：新增 continue 语句获准后，两者都实际到达 Verify 并以 `E-BORROW-CONFLICT` 拒绝；不再用前端 Unsupported 代替安全结论。

## Rust 四目标与仍未完成项

| 目标 | 本片实际推进 | 尚未证明 |
|---|---|---|
| 同类安全 | 每条循环退出的符号义务，以及真实 scope / Drop / 借用 / raw / Wire 反例 | 完整语言安全、通用重新初始化、一般逃逸/并发能力 |
| 运行与内存 | 实际 O0 / O2 控制行为，50个分配失败观察，全部 allocated = frees / live 0 | 相对 Rust 的时间、峰值内存或尾延迟 |
| 表达与维护 | 普通同步 helper 能组合 break / continue、拥有环境和每轮资源，四模块消费者可运行 | 完整库组合或维护成本优势 |
| 编译成本 | 新状态复制/集合工作受原额度约束，精确 N / N−1 与 fresh 请求有实证 | 任何编译提速或与 Rust 的比较 |
| 至少一项实质突破 | 真实库组合能力增加，循环控制不再依赖仅有的有限调用轨迹 | 产品级可重复净突破仍未证明 |

标签/value break、循环表达式、一般 iterator、新借入逃逸、泛型已移动局部重新初始化以及其它未派工能力不由本片宣称完成。独立审核和主线正式组合由 root 接续；本片没有自行标记父任务完成，也没有运行正式全仓门。

交付收据排除可执行文件、目标缓存和大部分重复阶段文本；原文件的全 SHA 索引解释所有排除项。源代码交付包与证据附件继续按用户要求分离，不将执行产物混入源码包。
