# 完整联合关系的无损因子消费器

`factor_relations.py` 从已经物化的 canonical 行集构造候选因子，提供按需投影、membership 和完整重建。它不消除产生输入的旧求解成本；与直接在 transfer 上推进因子的 Rust `graph/factored.rs` 是两个不同实验。

实际事件支持先形成字段连通分量。常量字段存成一个单行常量向量；剩余分量只有满足完整基数证书才分开：每个因子都由原关系投影而来，故原关系包含于因子的笛卡尔积；再检查两者有限基数相等。这里绝不以两两独立代替完整独立。若候选不满足，进行有预算的完整二分划分搜索，预算用完则保守合并，保留联合信息。

查询按字段直接定位因子，避免单字段投影扫描所有因子。每个小关系的所有原始成员、完整重建、单字段和支持集合投影均与原 canonical 核对；不超过八字段时枚举全部字段子集。修改负例显式抽样至最多十六行、三十二字段，报告实际数量。`--oracle` 还要求候选输入的字段及全部关系与独立 oracle 文件一致。

```sh
python3 tools/research/incremental-compilation/stage6_algorithms/factor_relations.py \
  input.json output.json --oracle input.oracle.json --supports support.json
python3 tools/research/incremental-compilation/stage6_algorithms/factor_relations.py --self-test
```

支持文件接受 `component_fields` 或 `supports` 数组，元素为字段名列表。没有支持文件时明确标记为 singleton 字段候选，不能称为已应用事件支持。

## 当前控制结果

所有 256 个三位关系分别使用 singleton 和粗支持划分，共 512 例，逐个核对全部成员、全部投影和完整重建。另覆盖三变量 parity、parity 加独立第四变量、相关分支、独立相关组、空关系、常量、重复行、跨 u64 字段和非零未声明位拒绝。

对当前源码导出的十四函数 canonical 与独立 oracle，singleton 候选完成 779 关系、1367 原行、876728 次投影对照以及 779 次完整重建。52 个关系耗尽搜索预算后合并，全部仍精确。总 field-cell 数为 950745→874499；原 packed u64 只有 17803 cells，两种单元不等价，因此这不是内存压缩结论。`diamond3/f1` 的 field-cell 为 69440→13747，而 `sequence16/f1` 的 327 个单行关系没有这一收益。

实际 Transfer 支持来自 `stage6-factor-final-v2` 元数据，严格检查每个函数的 canonical 字段全集与组件覆盖。十个支持函数共 464 关系、464 原行、847178 次投影及 464 次完整重建全部一致。该子集所有关系都是单行，field-cell 为 826298→826298，没有分解压缩收益。`bundles` 的两个多块函数、`loop` 主函数及 `diamond3` 的工作函数为 Unsupported，未伪造其事件支持，也未套用 singleton 实验中的收益。

工作区结果在 `/workspace/.zil-env/research/stage6-algorithms/` 下的 `factored-consumer-singletons-v2/`、`factored-consumer-event-support/`、`factor-self-test.json`。每份输出记录工具、输入、独立 oracle 与支持文件 SHA，构造/查询/oracle 操作计数、因子和搜索缓存规模、原行/因子载荷的 compact JSON 字节数。这里没有墙钟或 RSS 结论。

首次真实控制在 2497 字段样本发现消费端投影及负例生成的多余二次工作，作者中止该子进程，改为字段索引、常量向量和有记录的负例抽样，再完整重跑。旧十二份 partial 与 `factored-consumer-development.json` 保留，旧结果不重绑新工具身份。
