# 067 一手资料与可移植机制

检索日：2026-10-24。以下只取作者论文；源码实验的正确性由本仓有限模型的等价合同、独立解释器和反例承担。物理系统的结论不自动成为编译器定理。

| 一手来源 | 论文涉及的机制 | 对本实验的具体启发 | 当前落地程度与边界 |
|---|---|---|---|
| Kschischang, Frey, Loeliger, **Factor Graphs and the Sum-Product Algorithm**, IEEE TIT 47(2), 2001. [作者机构 PDF](https://www.isiweb.ee.ethz.ch/papers/arch/aloe-2001-1.pdf) | 全局函数由局部因子表示；分配律和中间结果复用；无环与有环图的精确性条件不同。论文允许一般半环。 | 本有限模型使用布尔半环：合取组成转移，存在量化消去旧态。观察边界必须保留完整联合关系；不能独立传播各资源的边缘结果再做笛卡尔积。 | 复用063的精确BDD关系像、融合合取/量化；067只换可逆坐标。没有采用有环信念传播近似，也没有把程序CFG直接当因子图。 |
| Markov & Shi, **Simulating Quantum Computation by Contracting Tensor Networks**, SIAM J. Comput. 38(3), 2008. [作者论文](https://arxiv.org/abs/quant-ph/0511069)；[DOI](https://doi.org/10.1137/050644756) | 特定量子线路模拟复杂度与图treewidth有关；收缩中间边界影响计算规模。 | 图切割应优化关系中间表示、变量消元和语义分隔边界，而不只最小化CFG边数。location编码减少“同一token在各carrier”的冗余坐标，但可能增加因子支持或耦合。 | 记录物理位数、因子支持、BDD节点、工作量和完整输出成本；没有声称BDD复杂度继承量子线路的treewidth上界。 |
| Gray & Kourtis, **Hyper-optimized tensor network contraction**, Quantum 5, 410, 2021. [期刊](https://quantum-journal.org/papers/q-2021-03-15-410/)；[作者版本](https://arxiv.org/abs/2002.01935) | 对完整收缩树及总成本优化；不同图结构适合不同策略；分割、聚合和预处理可组合。 | 不把“切割一定更快”作为预设。本轮交叉比较dense/location和mono/fused；全部编码、构造、求解、检查、输出和析构均计费。 | 实施可复核的交叉对照；尚未实现自适应收缩树搜索。未来启发式只能选精确等价的计划，规划费用也必须计入。 |
| Li, Bian, Li, Karniadakis, **Incorporation of memory effects in coarse-grained modeling via the Mori–Zwanzig formalism**, J. Chem. Phys. 143, 243128, 2015. [作者全文](https://pmc.ncbi.nlm.nih.gov/articles/PMC4644152/)；[DOI](https://doi.org/10.1063/1.4935490) | 粗粒化通常引入记忆；尺度没有明显分离时，忽略记忆的Markov近似可能不准确。 | 编译中的“删去中间变量/历史”也必须说明哪些信息足够决定后续观察。局部owner集合不足以表达相关guard、清理顺序或旧Desc的身份。 | 本轮编码可逆，保留全部其他状态；没有做历史截断、概率近似或用物理记忆核求解。这里是方法上的警示，不声称两类动力系统同构。 |
| Gray & Chan, **Hyper-optimized approximate contraction of tensor networks with arbitrary geometry**, Phys. Rev. X 14, 011009, 2024. [作者论文](https://arxiv.org/abs/2206.07044)；[DOI](https://doi.org/10.1103/PhysRevX.14.011009) | 结合bond压缩与收缩策略，在误差和成本之间优化。 | 近似可用于候选计划排序、成本预测或找到反例的提示；最终安全接受需要精确检查。 | 本轮不使用该近似算法。它说明前沿数值方法不能不加条件地移植为所有权判定器。 |

## 本轮独立推导的守恒类比

“同一责任token沿carrier移动”来自本仓操作语义，可视作有出生、转交、退出和退休事件的离散带标签流。这里的token不是物理粒子，责任也不等于总能量；制造和退休明确改变活跃集合。可实施部分是：每个正常可达态中，同一token至多有一个位置，于是可用有限枚举位置替代布尔关联列。

闭包可以同时携带多个不同token。重复捕获同一token两次、同一实参重复give、重复清理，都必须保留原始有序事件的错误；位置编码不能替它们消重。实际实现对正常转移保持性额外做BDD检查，出现反例时使实验失败。

## 有价值的后续方向

1. 对保持性证明成立的事件生成小型可核验证书，用证书免去反复构造全owner对的保持检查；证书生成和核验成本需同时计入。
2. 以当前有限关系的实际支持集构建因子超图，优化消元顺序和联合边界；只用CFG切口大小并不足够。
3. 用结构摘要预测dense/location/其他精确表示的成本，在共享计划层选择；保留完整调试输出与生产查询输出的不同成本边界。

这三项未在本轮实现，不能计作已有编译性能收益。
