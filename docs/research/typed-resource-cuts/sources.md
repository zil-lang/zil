# 055 主源、实现根据与迁移边界

核对日期：2026-10-23。下面分别列论文/官方资料能支持的事实，以及本研究自身的推论。

## 原始切割启发

Yu Deng、Zaher Hani、Xiao Ma，[*Long time derivation of the Boltzmann equation from hard sphere dynamics*，v3](https://arxiv.org/abs/2408.07818v3)，2025-07-18，192 页。作者摘要明确把碰撞历史的 cumulant 小量控制归结到特定 molecule 图的组合性质，并提出切割算法；研究对象是稀薄硬球气体的动力学方程推导。

本轮迁移推论：真实程序要先定义可观察身份和边界组合律，再研究哪些透明区可去除。原文的 molecule 条件和小量估计没有在 TypedCFG 上成立，不能由该论文推出编译复杂度保证。055 没有把论文的物理估计或专用分型照搬为编译器算法。050/SOURCES 中保留了原文 §11 玩具模型及后续章节的更细阅读定位。

旧 `compiler-proof-cut.md` 中 gen/lease 等语言机制已经被现行规范替代。这里只继承保留切口身份、明确语义等价、独立检查、按结构选择和回退的研究要求，不恢复旧语法或权限机制。

## 消元与宽度

Rina Dechter，[*Bucket Elimination: A Unifying Framework for Reasoning*，作者稿](https://ics.uci.edu/~csp/r48b.pdf)。摘要及 §1 给出统一消元框架，复杂度受交互图的 induced width 指数影响；高宽度时需要考虑 conditioning 等时空折中。论文支持的是相应有限因子/关系框架的结构结论。

055 的资源交互图只做宽度/密度普查，min-fill 是已有启发式。候选算法本身是 gen/kill 活跃方程中的恒等点消去，不是050完整联合关系表的消元，也没有发明 min-fill。宽捕获形成共同团，说明从“CFG 很稀疏”不能推出“所有资源联合状态问题的边界很窄”。后者若要进入核，必须另有语义依赖图、关系表示与实测证据。

## 独立语义参考

[Polonius 官方 loan 规则](https://rust-lang.github.io/polonius/rules/loans.html)保留 Origin、Loan 和 CFG Point，传播依赖 subset、liveness、kill；官方同时说明不同实现服务于可读性、验证与性能，并保留计算完整关系的 naive 版本。

工程推论：先冻结清楚的关系语义，再用独立表示/调度核对优化实现；不能把资源根、程序点或共享控制条件压成一个布尔值。055 raw-TypedIR/BTreeSet oracle 遵循此验证方式，但没有实现 Polonius loan analysis，也不比较 Rust 编译性能。

## 本仓的实际语义依据

- `_zil_design.md` §§3/5/9/10/18/19：IR、验证边界、预算及现行语言义务。
- `crates/zil-verify/src/resources.rs`：Desc/借入闭包活跃槽位、逐指令用途与块级固定点，是055窄查询的生产对应位置；资源状态、newer 关系和权限核仍由生产负责。
- `crates/zil-ir/src/lib.rs`：本轮冻结 draft10/schema7 指令、分支和终结身份。
- `tools/research/055/oracle.rs`：从原 TypedIR 直接解释活跃语义，另审计抽取；不调用候选操作数访问器或消元器。
- `docs/research/055/PROOF.md`：055自身限定代数的证明与未证明事项。

以上引用不能替代研究程序的正确性收据，也不构成生产入核授权。
