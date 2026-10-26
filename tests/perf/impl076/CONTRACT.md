# 076 预注册语料与成本

语料完整复用071的16个冻结真实源程序；计时固定tiny、correlated、closure、bundles、
error、three_guards、four_guards七族。没有通过缩小可观察关系来改变问题。
具体路径及八个固定方法见 `tools/research/076/cases.json`。

每次Complete包含相同schema、原始事件标签、所有point/fault/abort/exit/unsupported
关系、完整联合位向量及canonical字节。哈希只用于查找和证据校验，不能替代逐字关系核对。
功能准备保留超限族和原始重复操作反例；不把超限排除后称为全语料成功。

计量以 `changes/IMPLEMENTATION-0076.md` 的预注册合同为准；本目录不重定义产品门。
