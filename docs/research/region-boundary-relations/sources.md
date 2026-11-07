# 050 主源与迁移边界

查询日期：2026-10-23。以下链接为作者论文或 Rust/Polonius 官方开发资料。算法解释与本仓实验结论分开，不把网页说明当作本原型正确性的证明。

## Deng / Hani / Ma

[IMU 2026 官方获奖页](https://www.mathunion.org/imu-awards/fields-medal/fields-medals-2026) 列有 Yu Deng。表彰包含多方面 PDE 工作，不能把下列文章称为唯一“获奖论文”。

相关具体文章为 Yu Deng、Zaher Hani、Xiao Ma，[*Long time derivation of the Boltzmann equation from hard sphere dynamics*](https://arxiv.org/abs/2408.07818)，本轮读 v3（2025-07-18；初稿 2024-08-14）。原文是 192 页，不是一般图分隔器复杂度论文。[PDF](https://arxiv.org/pdf/2408.07818)。

阅读定位与提要：§2.2.4 将非局部长替换写成局部积分因子；§2.3/定义8.3 将跨边分成 free/fixed 端，仍要求端点变量相等，得到算子组合式。§11 的简化情形要求层内森林；玩具 I 在上层自底选择节点，并在下层贪心处理特定度数结构；I+/II/III 按跨层连接和分量分类，分别引入 MAINUD、3COMPUP、2CONNUP/DN。3COMPUP 的 connected/no-bottom 选择维持后继切割条件，其收益由特殊基本分子的计数支付。通用情形另处理退化和环。对应 printed pp.19–20、99–102、115–116，以及 §§12–13。

对 ZIL 的迁移判断是本研究的推论：必须保留切口身份，定义局部组合等价律，并根据残余交互结构选择次序；不能删除跨区相关性。原文的范数小量、节点度数分类和森林条件没有在任意 CFG、闭包或 trait 图上建立，因而其效率保证不适用。050 使用自己的有限关系等价证明，没有移植论文的具体分类算法或声称得到同一数学定理。

## 已有消元框架

Rina Dechter，[*Bucket Elimination: A Unifying Framework for Reasoning*，作者原稿](https://ics.uci.edu/~csp/r48b.pdf)。论文将约束推理等不同任务统一到消元框架，并讨论 induced width 对时间/空间的指数影响。§§2–3 的约束与命题推理、§4 的局部因子移项/消元推导可与本原型对应。

050 的 existential projection + join、基于 primal graph 的 min-fill 属于这一已有技术家族。潜在 ZIL 差异应来自语义图构造、低边界接口、与安全核分离且完整的检查、按实际密度选区域和失效协议，而非宣称发明变量消元。

## Rust next solver

| 官方页面 | 本轮读取的事实 | 对研究的约束 |
|---|---|---|
| [Canonicalization](https://rustc-dev-guide.rust-lang.org/solve/canonicalization.html) | 隔离查询上下文，保存原变量映射；响应返回约束和外部区域义务，再实例化回调用方 | 不能只存 bool，不能丢失共享替换与区域边界 |
| [Caching](https://rustc-dev-guide.rust-lang.org/solve/caching.html) | 递归剩余深度可影响 ambiguity；循环参与者的暂定结果依赖栈，不能任意放入全局缓存；依赖图也影响复用 | 区域输入相同还不足以证明 trait 缓存可复用；本模型的四个 context 数字不是完整解答 |
| [Coinduction](https://rustc-dev-guide.rust-lang.org/solve/coinduction.html) | canonical 循环、归纳/共归纳和暂定响应的固定点有特殊语义 | 不把循环目标当普通无环约束图；不把有限展开当完整递归证明 |
| [Significant changes and quirks](https://rustc-dev-guide.rust-lang.org/solve/significant-changes.html) | 嵌套目标也 canonicalize；候选响应带约束；嵌套目标循环至固定点；规范化与候选规则仍有细节 | 可以吸收响应接口和失效思路，不能用一个通用 SAT 布尔结果替换整个类型系统 |

这些官方页面自身标有简化与未完细节，描述的是开发模型，不是“所有 Rust 程序当前都已使用新求解器”的发布状态声明。本片没有测 Rust 编译器，也没有在 ZIL 实现一般 trait solver。

## Polonius

[官方 loan 规则](https://rust-lang.github.io/polonius/rules/loans.html) 同时保留 Origin、Loan 和 CFG Point 的关系；传播受 subset、liveness 和 kill 影响；illegal access 由 live loan 与 invalidation 共同决定。官方还保留易读的 naive 关系实现，用于对照不同算法版本。

迁移推论：对象、程序点和替换/来源关系都是边界信息，不能只抽取“某处借用过”的单值摘要。固定 kill 关系时集合转移仍可单调；改变输入事实后必须重新求解受影响的关系。050 的独立枚举 oracle 延续的是“保留明确语义参考，再验证更快执行”这一工程方式，并未实现 Polonius，也不比较其性能。

## 本仓事实

- `_zil_design.md` §§11/18：闭包创建、调用、重复调用义务和 trait 固定边界；impl specialization 仍非默认语言功能。
- `crates/zil-verify/src/resources.rs`：真实的状态、Desc 根与 newer 顺序边合并。本文讨论机制成本，未声称已经采得其生产热点占比。
- `evidence/impl/046/benchmarks.json`：046 相对旧 ZIL 调度的收益和小例退化，继续保留。
- `evidence/impl/050/input-manifest.json`、`paired-raw.jsonl`、`paired-summary.json`：本研究模型的来源身份和实际测量，不能当成生产编译器结果。
