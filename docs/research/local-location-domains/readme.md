# 071：局部位置域与直接精确联合态

本目录是独立研究，不是生产安全核。当前实验基线为 schema10 的
`400a082444275e1d0e2ad78c4910b9b6fc526be5`，067 核心为
`bf5876bd8b5e9936ad52e8f24adaa673884232dc`；当前语言树的新 schema 不自动进入实验。
实现和原始证据身份以 `evidence/impl/071` 及冻结 `identity.json` 为准。

## 五组实验

| 方法 | token 域 | 联合关系引擎 | 用途 |
|---|---|---|---|
| dense:fused:owner:gc | 完整 incidence 矩阵 | BDD 融合消元 | 保守强基线 |
| location:fused:owner:gc | 所有 owner | BDD 融合消元 | 067 位置表示控制 |
| local:fused:owner:gc | 静态可达 owner 闭包 | BDD 融合消元 | 单独测缩域 |
| location:direct:owner:gc | 所有 owner | 完整行的同步转移 | 单独测转移引擎 |
| local:direct:owner:gc | 静态可达 owner 闭包 | 完整行的同步转移 | 联合候选 |

direct 使用前态 Boolean 表达式核，但不构造 post 等式或关系乘积。它保留完整联合行，
逐行求所有 RHS/自由位，按精确集合去重并计算 CFG 固定点；没有拆成独立 token/guard
边缘集。当前没有实现 MDD、饱和调度、连续流体求解或一般区域消元。

## 为什么这值得测

067 四独立条件的完整结果只有1440行，而位置 BDD 创建约179万节点。
这提示构造临时关系的工作可能明显大于观察结果；输出敏感的精确行引擎可检验这个差距。
行集合也会随独立条件指数增长，不能据此宣称消除了状态爆炸。

局部域是另一个因素。每个资源身份通常只经过少数具体 SSA/local/环境位置。制造点和
参数/环境为种子，Store/Take/拥有捕获/give 返回为搬移边，静态闭包删去不可能位置。
guard、存活和错误仍由完整状态决定；域分析只过近似可达位置，不自行决定合法性。

## 不丢观察的论证

设每个 token 的闭包为 D(t)。初始位置属于种子；每个正常更新只有保持、退休、制造
和沿原始搬移边转交，故归纳可知可达位置始终属于 D(t)。位置编码在每个 token 至多
一个 owner 的状态上与 incidence 可逆，多个 token 可以同时属于同一个拥有闭包。
因此在这些前提下，缩域不会删掉可达状态。所有非位置坐标和事件标签保持不变。

这是一份与实现逐条对应的有限语义证明义务，不是对全部 ZIL、任意内存别名或无界动态
分配的形式化证明。正常像中的域外赋值/多位置会返回 Mismatch；初始和 oracle 行编码
也拒绝域外身份。故障、abort 和 unsupported 记录完整原始前态，仍保留 raw 重复实参。

## 独立比较与费用

`oracle.rs` 与 `extract.rs` 从067冻结研究复制，oracle 原样保留，直接从 raw TypedIR
独立建坐标/事件语义/固定点；它不调用候选 `domains/encoding/symbolic/direct`。
候选 BDD 与 direct 共用候选事件语义，归因只比较表示/执行机制；独立 oracle 防止二者
共用候选语义时一起遗漏错误。每个 complete 查询都比较 schema、标签、状态、全部
关系的计数/成员和完整 canonical 字节；摘要不作为语义 oracle。

每函数上限沿用100M抽象工作、300k BDD槽、600k缓存、100k完整关系行、4096语义位和
20k点。BDD 与 direct 的抽象收费单位不同，不用工作计数比当CPU加速比。候选时间包括
域闭包、所有构建、执行、核对、完整输出和释放；前端/oracle 求解单列，进程墙钟也记录。
Linux `VmHWM` 为整个进程高水位，包含前端、oracle 和候选，不能解释为候选独占内存；
不可用时为 null。功能准备计时不作正式性能证据。正式采样需主集成者协调空闲窗口。

## 可重放入口

先使用交付工具链并定位源码树，再从冻结候选运行：

```sh
python3 tools/research/071/run.py prepare --repo . --candidate CANDIDATE --out NEW_OUTPUT
python3 tools/research/071/run.py sample --out NEW_OUTPUT --window ROOT_COORDINATED_WINDOW
python3 tools/research/071/run.py verify --out NEW_OUTPUT
```

prepare 导出固定生产树、固定067目录与071覆盖，记录每个文件/二进制/库/工具链身份。
采样前验证全部冻结内容；输出目录不得覆盖旧尝试。正式结果见 RESULTS.md，研究依据
和后续候选见 SOURCES.md。研究结果不改变生产接受集合或产品门。
