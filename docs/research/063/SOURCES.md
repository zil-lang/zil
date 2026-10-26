# 063 原始来源与成熟度

检索日期：2026-10-24。以下是官方文档、实现者的一手说明及原始论文。网页的版本/日期是观察身份；不能把目标日期或nightly公告改写成stable发布事实。所有实验结论另见本切片冻结源码和原始收据。

| 来源 | 直接支持的事实 | 对063的用途与边界 |
|---|---|---|
| [Rust Profiling](https://rustc-dev-guide.rust-lang.org/profiling.html) | self-profile、rustc-perf与LLVM阶段诊断方法 | 文中“大部分LLVM”针对编译rustc自身；不能推导所有Rust程序的瓶颈 |
| [Rust incremental compilation in detail](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html) | 按实际query依赖验证red/green；输入变动不必导致结果变动 | 区分“内容变了”“公开语义结果变了”“下游必须重算” |
| [2026 next-solver目标](https://goals.rust-lang.org/2026/next-solver.html) | coherence自1.84稳定；全局替换是2026目标 | 目标本身不是完成证明 |
| [2026-08-21 next solver nightly公告](https://blog.rust-lang.org/2026/08/21/enabling-next-solver-on-nightly/) | 全局新solver在nightly默认启用、尚待稳定；存在不同性能方向的案例 | 新算法带来结构改进，仍必须看真实负载和退化；未宣称所有Rust代码加速 |
| [next-solver tracking issue](https://github.com/rust-lang/rust/issues/107374) | 新solver的实现里程碑、实验入口和相关跟踪 | 2026-10-24检索未取得全局stable发布证据，报告只声称已确认的nightly进展 |
| [Canonicalization](https://rustc-dev-guide.rust-lang.org/solve/canonicalization.html) | 查询把局部推断上下文规范化，响应携带约束 | ZIL未来摘要共享不能只缓存一个bool；输入身份、约束和局部上下文仍须准确 |
| [Caching](https://rustc-dev-guide.rust-lang.org/solve/caching.html) | 递归深度、溢出和循环上下文影响缓存合法性 | 不能把资源限额失败当全局语义否定，不能任意复用SCC内部暂态结果 |
| [实现者的新solver性能分析](https://donsz.nl/blog/new-solver-performance) | 不必要opaque上下文影响缓存；回滚推断表和目标唤醒可成为隐蔽成本 | 其指标是root crate `cargo check`指令数，不能改称全部依赖的wall-time。可借鉴依赖驱动唤醒、减少上下文和避免整表复制 |
| [2026-08-04 Polonius Alpha公告](https://blog.rust-lang.org/2026/08/04/enabling-polonius-alpha-on-nightly/) | 原始Polonius受性能阻碍；Alpha采用较小NLL结构改动并支持flow-sensitive outlives，在nightly试用 | Alpha和旧Polonius接受集并非严格包含；存在负载退化。语义表达能力和实现性能必须分别验证 |
| [Zig 0.16.0 release notes](https://ziglang.org/download/0.16.0/release-notes.html) §§Reworked Type Resolution/Incremental Compilation/x86/New ELF Linker | 惰性类型字段、减少over-analysis、整理依赖图；LLVM bitcode可增量，LLVM Emit Object没有因此提速；incremental有已知错误且默认关闭 | 前端增量、机器码、链接是不同层；直接后端的编译速度和机器码质量有权衡，不照搬一个“先进”标签 |
| [CMU BDD library](https://www.cs.cmu.edu/~modelcheck/bdd.html) | 关系积、节点管理/回收、变量重排是已有BDD工程方法 | 063 fused与GC不是新发明；对照旨在排除弱基线造成的假突破 |
| [Chauhan等，Using Combinatorial Optimization Methods for Quantification Scheduling, CHARME 2001](https://www.cs.cmu.edu/~pchauhan/acads/charme01.pdf) | 分区迁移关系和提早量化的已有研究 | 059/063最后关联消元属于这类问题，独特收益必须在强基线上另证 |
| [Amarilli/Monet/Senellart, Connecting Width and Structure in Knowledge Compilation, ICDT 2018](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ICDT.2018.6) | 有界treewidth电路到d-SDNNF的构造上界；特定有界arity/degree单调CNF/DNF的width下界 | 不能将其受限公式族下界直接套到所有ZIL图；但应测语义因子图与输入关系宽度，而非只数CFG边 |
| [Deng/Hani/Ma, arXiv:2408.07818](https://arxiv.org/abs/2408.07818) | 原网状关系/逆向切割研究的论文来源；059已核对的桥接前提与限制见059报告 | 保留“唯一位置／释放责任”的结构启发，不把原物理估计定理当任意编译器图的现成算法证明 |

## 本地可核事实

生产源冻结于`70d3a729a6db6f8355ea99942e6f05207eb8815b`。`crates/zil-driver/src/session.rs`和`project.rs`把`&format!("{...:#?}")`传给`ArtifactPlan::dump`，而`artifacts.rs`在函数内部才检查对应输出路径。因此未请求dump也会提前格式化。此因果由本地源码直接给出，性能幅度由063配对实测给出；不是由Rust/Zig网页推断。

原始059的schema8/依赖6ac数据不与schema9新样本合并。063复制的有限语义仍有自己的完整关系合同，Package显式Unsupported。旧数据的来源与阶段不得追改。
