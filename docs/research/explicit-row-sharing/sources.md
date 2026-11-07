# 一手来源与本轮适用范围

核对日期2026-10-24。071的Petri网缩域、PINS读写依赖和饱和算法来源继续保留在
`docs/research/071/SOURCES.md`；076不重新解释或重标071测量。

1. Alfons Laarman、Jaco van de Pol、Michael Weber，*Parallel Recursive State
   Compression for Free*，SPIN2011，38–56。
   https://research.utwente.nl/en/publications/parallel-recursive-state-compression-for-free-2/
   DOI https://doi.org/10.1007/978-3-642-22306-8_4
   一手摘要明确提出共享状态子向量的树压缩。076只实现完整行驻留和点关系索引，尚未
   实现子向量树压缩、多核并发或该论文的内存指标；论文数字不能搬成ZIL结果。

2. Alfons Laarman，*Optimal compression of combinatorial state spaces*，
   Innovations in Systems and Software Engineering 15，235–251，2019。
   https://link.springer.com/article/10.1007/s11334-019-00341-7
   论文在局部状态变动的信息模型下分析无损压缩，并以树和紧凑哈希表实现共享。076的
   直接联系是“局部事件使完整行出现重复”；完整行能共享多少由实际unique_rows与
   relation_rows给出，不能从论文模型推出本语料达到信息论界限。若整行共享不足，
   子向量共享才是有根据的下一结构，而不是先假定任何物理类比都会降低复杂度。

3. Freark I. van der Berg，*Recursive Variable-Length State Compression for
   Multi-Core Software Model Checking*，arXiv:2007.12174v2，2021修订。
   https://arxiv.org/abs/2007.12174
   论文的dtree支持可变长状态、部分重建和不先展开完整状态的增量更新。076当前仍
   每步通过既有direct内核产生完整后态，最后恢复全部关系，因此没有采用dtree。
   此差异提示后续可把事件读写支持用于持续共享子树，再把完整观察费用单列；是否
   值得做必须由本轮强显式基线和构造/恢复占比决定，不能借“增量”二字省略费用。

## 从物理类比到可证伪结构

带身份的粒子位置启发资源身份/载体的表示；守恒启发不复制、不丢失真实清理责任。
局部相互作用启发只触达事件读写支持。076检验的是第三步中最简单的精确共享版本：
相同完整状态只存一次，并保持每个位置对该状态的独立成员关系。

这不允许以流体平均密度取代完整联合状态：清理次序、描述根、存储活性和路径相关性
均可影响后续故障。保持这些观察的压缩可被oracle反例推翻；仅守恒计数不能建立安全。
当前完整输出合同也造成可测的恢复下限，未来垂直共享通道必须先定义真实消费者需要
哪些关系，再同时与旧完整合同和新合同分别比较，不能悄悄删掉观察来制造收益。
