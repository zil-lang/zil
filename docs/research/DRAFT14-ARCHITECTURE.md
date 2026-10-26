# draft14：从物理启发到编译器的可检验机制

2026-10-24。这是研究与工程路线，不增加语言规则或产品门。当前已实现代码与冻结测量分别见 [067](067/RESULTS.md)、[068](068/results.md)；本轮没有建立相对 Rust 的整体编译或运行成本优势。

## 1. 物理类比中可以具体实现的部分

一个责任 token 的出生、转交和退休，可建成有标签的离散流。正常状态中，它只能占据一个拥有位置；闭包或包则可以承载多个不同 token。这个不变量由操作语义决定，不能从“粒子的位置唯一”直接推出。重复 give、重复捕获和重复清理仍须在原始有序事件上检查，不能在编码时消重。

067 已实现这一候选：原先每个 token 对所有 carrier 使用一列布尔关联，改成每个 token 一个有限位置编号。它保留 guard、初始化、Desc、出生/退休和清理次序；正常转移的唯一性有独立检查，违反时使实验 Mismatch。对 raw 重复操作仍保留完整故障前态。编码减少的是关联坐标，不是完整语义，也不是无界动态实例问题的答案。

本轮最有用的反例恰好说明了下一步方向：四 guard 上全部物理位从 272 减到 212，单转移因子支持却从 3 增到 7，完整候选工作从 22,255,624 增到 38,072,796。减少表示位数并不足以减少求解工作。位置编解码、正常保持检查、BDD 产生和回收、完整查询物化都已收费；当前候选不进入生产。

数学上，应把关注点转为联合关系的因子分解和中间边界。布尔半环中，局部约束合取形成转移，存在量化消去旧态。只有保留必要的联合关系，才能准确区分互斥分支，不能把各资源边缘集合重新做笛卡尔积。因子图与 tensor 收缩文献提供了消元次序、结构分割和完整成本规划的方法；它们没有自动给 ZIL 的 BDD 求解器带来相同复杂度上界。[1][2][3]

流体与粗粒化方向的另一个启发是：消去内部自由度以后，剩下变量可能还需要历史信息才能决定下一步。Mori–Zwanzig 的研究在其物理系统中明确保留记忆效应。[4] 对编译器的对应研究问题是“边界信息是否足以决定所有后续观察”：guard 相关性、老 Desc 的来源、责任转交与清理顺序都可能是必要状态。这里是用于发现错误抽象的类比，本轮没有实现物理记忆核，更没有允许概率性地遗漏不安全路径。

## 2. 纵向通道已经落了什么代码

068 沿一次真实 `compile_path` 请求，而非单个阶段查看成本，删除了三种重复：import 发现后再次解析同一字节、Typed IR 的中转深复制、各阶段重复构造相同输入身份。通道只持有本请求不可变的 raw 结果；Verify 仍每次独立执行，原有输入身份和结果摘要不同职责仍保留。

冷路径 N 个模块的解析从 2N 次降为 N 次；未变文件请求仍需 N 次 import 发现解析。Typed 中转复制从 1 次降为 0 次，但 Verify 的拥有输入复制仍保留。成功到 Verify 的完整 query_key 构造从 N+4 降为 N+1。三项确定的工作量下降，尚未转化为普遍稳定的完整请求时间收益；17 模块合成控制的冷 Verify 配对中位比为 0.928，真实 helper_ui 冷 Verify 为 1.006、未变 Build 为 1.065。不能挑选前者而隐藏后两者。

下一步垂直共享应按对象的性质组织：不可变 source/语法/类型身份可以共享；完整关系的计划可以作为未可信提示复用；安全结论必须通过独立检查才能授予权限。若把阶段合成一个巨大的共享可变上下文，虽少传几个参数，也可能扩大失效范围、增加存活内存并破坏复核边界。本轮未测峰值内存，不宣称通道使内存下降。

## 3. Rust、Zig、Polonius 的具体借鉴点

| 来源 | 可借鉴机制 | ZIL 本轮状态 | 下一步需要的证据 |
|---|---|---|---|
| Rust 查询与 red/green 增量 [5] | 输入变化后按实际依赖重算；结果未变则阻止失效继续扩散 | 已有 raw 查询缓存；068复用请求身份，不等于实现按函数的全新增量器 | 私有正文、公开签名、捕获布局分别改动，验证失效闭包与冷编译一致 |
| Rust next trait solver [6][7] | canonical goal、结果缓存、循环上下文与剩余深度影响复用资格 | 尚无完整 trait solver；当前有限泛型与 callable 工厂不能改名当作 next solver | 先定义 ZIL 自身 obligation/环境/周期语义，再核对求解与缓存；溢出不能作为正常否定结果缓存 |
| Polonius Alpha [8] | 流敏感的 outlives 关系可接受原 NLL 拒绝的条件路径 | 继续用于借用关系与联合 guard 的研究参照，未将 Rust 实现作为 ZIL 安全核 | 与既有独立 Verify 比较完整关系和接受集合；精度收益、查询成本分别记录 |
| Zig 0.16 类型解析与增量 [9] | 减少过度分析、细化依赖；惰性字段解析；优化 LLVM bitcode 构造与 Emit Object 是不同问题 | 已有按需 dump；068继续减少重复准备。没有实现新的增量链接器或自有调试后端 | 在完整 Build 路径衡量前端、代码生成、链接及缓存维护的净成本 |
| tensor / factor 图 [1][2][3] | 优化真正的语义因子支持和消元中间态；不同结构选择不同精确计划 | 067完整交叉比较 dense/location × mono/fused，当前 location NoGo | token 局部 carrier 域、直接有限域搬移、计划选择开销和最坏退化 |

截至上述官方来源，next solver 和 Polonius Alpha 的公告都明确针对 nightly。Polonius Alpha 为更强精度付出等量或更多工作；它不是“启用就加速”的开关。next solver 的性能成果也有具体工作负载和负向样本，不能迁移为 ZIL 的速度结论。Zig 0.16 的增量改进仍带实验边界，其 LLVM bitcode 构造增量也不表示 LLVM Emit Object 自动变快。[6][8][9]

## 4. 下一批可并行工作

研究优先验证两个不同机制。其一，为每个 token 构造保守可达 carrier 集合，只编码它实际可能到达的局部域；“保守”要求不能剪掉坏状态，域构造费用也要计入。其二，把单一责任的转交表示成前置条件加局部搬移，直接求有限域关系像，避免先生成 dense 关系再展开成宽的二进制比较。两者尚未实现；独立 raw oracle、故障前态、完整边界和严格预算应继续使用现有实验合同。

纵向工程优先细分可复用依赖单元，测量共享对象存活时间和字节数，再决定是否引入不可变 arena 与稳定身份。遇到 LLVM/链接占主导的实际输入，应另开低延迟后端或增量产物实验；不能用进一步删减 Analyze 的几微秒假装解决了全部 Build 成本。

语言主线已由069/070补上工厂返回拥有环境、跨同步事件保存、调用与释放。下一批可分别推进明确布局下的拥有字段替换/部分移动、必要容器与字节/字符串值、平台 FFI 合同。平台回调注册涉及保存、重入、取消和排空，应依据真实接口建立责任状态；当前同步闭包返回不证明这些能力已经具备。

## 一手来源

1. Kschischang/Frey/Loeliger, *Factor Graphs and the Sum-Product Algorithm*: https://www.isiweb.ee.ethz.ch/papers/arch/aloe-2001-1.pdf
2. Markov/Shi, *Simulating Quantum Computation by Contracting Tensor Networks*: https://arxiv.org/abs/quant-ph/0511069
3. Gray/Kourtis, *Hyper-optimized tensor network contraction*: https://quantum-journal.org/papers/q-2021-03-15-410/
4. Li/Bian/Li/Karniadakis, *Incorporation of memory effects in coarse-grained modeling via the Mori-Zwanzig formalism*: https://doi.org/10.1063/1.4935490
5. Rust Compiler Development Guide, incremental compilation: https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html
6. Rust official next-solver announcement, 2026-08-21: https://blog.rust-lang.org/2026/08/21/enabling-next-solver-on-nightly/
7. Rust Compiler Development Guide, solver caching: https://rustc-dev-guide.rust-lang.org/solve/caching.html
8. Rust official Polonius Alpha announcement, 2026-08-04: https://blog.rust-lang.org/2026/08/04/enabling-polonius-alpha-on-nightly/
9. Zig 0.16.0 official release notes: https://ziglang.org/download/0.16.0/release-notes.html
