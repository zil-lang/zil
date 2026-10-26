# 一手依据、物理类比和后续候选

资料核对日期2026-10-24。以下分别标明已有结果、ZIL 推论和未实现假设；不把论文性能
数字迁移到本实验。067历史实测只引用其冻结 RESULTS/acceptance。

## 直接支持本轮实现

1. Alexander Bilgram 等，*Methods for Efficient Unfolding of Colored Petri Nets*，
   Fundamenta Informaticae 189(3–4)，2023扩展版。
   https://arxiv.org/html/2204.07039v3
   §4通过单调颜色扩展过近似每个place的可达颜色，Lemma 4.2给出覆盖归纳，Theorem 4.4
   给出缩域后可达LTS同构。071借鉴“先删不可能的身份/位置组合”，自行处理ZIL的出生、
   搬移、退休和完整观察；论文定理不能替代这些实现义务。其颜色等价类合并未采用，因为
   资源身份、Desc根、释放类别和清理顺序都是可观察坐标。

2. Jeroen Meijer、Gijs Kant、Stefan Blom、Jaco van de Pol，*Read, Write and Copy
   Dependencies for Symbolic Model Checking*，HVC2014，204–219。
   https://research.utwente.nl/en/publications/read-write-and-copy-dependencies-for-symbolic-model-checking/
   DOI https://doi.org/10.1007/978-3-319-13338-6_16
   论文区分read/may-write/must-write/copy依赖，并让符号算法利用局部性。071直接同步
   更新仅求改变的坐标，未改坐标随完整行复制；这与局部依赖思想一致。当前没有实现论文
   所有动态转换学习、数组模型或专门变量排序算法，也不声称其性能结果已在ZIL复现。

## 下一批结构候选

3. Gianfranco Ciardo、Gerald Lüttgen、Radu Siminiceanu，*Saturation: An Efficient
   Iteration Strategy for Symbolic State-Space Generation*，NASA/CR-2001-210663，2001。
   https://ntrs.nasa.gov/citations/20010022506
   多值决策图与局部事件饱和是直接有限域路线的重要先例。若显式联合行达到瓶颈，下一步
   可用共享前缀的MDD/LDD保存同一完整关系，再比较局部饱和调度。须包含建图、唯一表、
   缓存/GC、输出展开和独立核对费用；当前只列候选，未实现或测量。

4. Stefan Blom、Jaco van de Pol、Michael Weber，*LTSmin: Distributed and Symbolic
   Reachability*，CAV2010。
   https://cadp.inria.fr/ftp/publications/others/Blom-Pol-Weber-10.pdf
   PINS把语言前端和可达性引擎通过局部next-state接口连接。ZIL可研究一次构建的未可信
   事件表、read/write支持和多引擎消费，减少重复物化；独立Verify必须保留自己的合法性
   检查，不能把共享表的存在当作证明。当前候选内部共享公式不等于生产纵向通道。

## 官方编译器现状与适用边界

5. Rust官方，*Enabling the next iteration of the borrow checker on nightly*，2026-08-04。
   https://blog.rust-lang.org/2026/08/04/enabling-polonius-alpha-on-nightly/
   官方说明Polonius Alpha增加outlives关系的流敏感性，同时明确它目前至少做与NLL相同
   的工作，仍需关注回归。因此“更精确的关系”不能直接推导“编译更快”。071保留路径联合
   状态的动机与之相容，但语义、数据结构和计量对象均不同，未实现Rust Polonius。

6. Zig官方开发日志，*Incremental compilation with LLVM*，2026-04-08；
   *Type resolution redesign, with language changes to taste*，2026-03-10。
   https://ziglang.org/devlog/2026/
   https://ziglang.org/download/0.16.0/release-notes.html
   官方区分Zig自身增量工作和LLVM Emit Object耗时，并说明惰性类型字段分析可减少不必要
   工作。071只优化有限关系分析实验，未测整个ZIL流水线、增量编辑、LLVM或链接；未来
   垂直共享/惰性分析必须按实际热点测量，不能用求解器局部收益替代整体结论。

## 物理学启发如何变成可证伪算法

“有身份粒子在载体之间搬移”提示保持实体身份和位置局部性；“守恒”提示转交时不丢失也
不复制责任；“局部相互作用”提示依赖支持、事件分区与共享前缀。071落实的是前两项有限
数据结构/不变量；守恒计数本身不足以证明没有泄漏、陈旧Desc、错误释放类别或错误清理序。

更大胆的粗粒化需要观察等价：只有被合并的微观状态对全部后续point/fault/abort/exit观察
可重建，才可能精确缩小状态空间。一般均值、密度或独立边缘分布会丢掉相关性，最多用于
排序/预算预测，不能直接作安全裁决。区域消元也必须保留完整边界联合关系与内部故障观察；
小CFG切口不保证小资源相互作用宽度。目标驱动切片若只保留某一出口，会违背当前全部观察
合同。增量缓存须区分新增事实和删除事实、失效支撑与循环再求解，不能复用过期证明。

优先次序由本轮剖面决定：若直接行转移显著减少临时成本而行数仍小，先巩固输出敏感路线；
若行爆炸主导，比较MDD/LDD与事件局部饱和；若边界窄而内部密集，再做精确区域摘要。
这些都是可执行的下一实验条件，尚不是突破声明。
