# 097 — Types 摘要读取前沿与数据库有效闭包

状态：先行实施提案。派工提交 `1eeb474a33695d86f042caf72d6f74a54c312582`，
生产代码基线 `28dbeba242578bac0178c1b2a97ce82c01f78018`，冻结 schema17。
并行099规范准备不改变此研究的源代码快照或旧成绩。父任务B0、默认预算、
独立Verify及产品门保持；A/B分别控制，不把研究描述当准入证明。

## A：Types 的实际读取依赖

080已使Analyze/Verify按依赖传播，Types::resource_effects::solve仍全轮扫描。
每个函数摘要是有限Desc参数下标集合；保留现有单函数扫描/保守未知调用、
import/receiver解析、词法shadowing和局部迭代。所有函数先排队一次；扫描
记录真实读取的被调摘要，注册callee→consumer；摘要严格增长时只通知消费者。
尚未注册的消费者仍在首次队列里，会读取最新摘要。有限单调方程组的公平
工作队列因此得到原全扫最小固定点。没有将Types结果作为Verify可信证明。

新增表/队列/读取集合/通知按确定性实际工作规模计费；删除的无关全函数重扫
不再收费。默认max_analysis_steps不增加。A与基线的ResourceLimit阈值允许
不同，分别保留完整请求N/N−1与失败阶段，不把较低费用假装成预算相同。
足额预算的摘要、成功/安全拒绝、完整产物与诊断顺序必须一致。

## B：已维护的查询图闭包

Database的公开变更API维护以下不变量：每个resident依赖端点存在、记录的
kind指纹相等、resident图无环。初始为空；publish先校验输入、再按变化kind
删除传递消费者后插入；invalidate只删除且闭合；失败发布不修改快照；
begin/abort只改变active集合。QuerySnapshot为无内部可变性的Arc。

read先照旧拒绝目标本身active。若全部active key都不在resident map中，
这些key不可能在任何resident闭包内，直接读取目标与原DFS等价。存在resident
active key时保留旧DFS及其诊断。B不改变配额、指纹、查询事件、失效算法、
Verify次数或磁盘内容可信性。

最近反例：begin(absent_key); read(absent_key)必须报E-QUERY-CYCLE而非None；
active祖先、同键相同/不同指纹替换、改依赖、重复边、失败publish、clear、
外部invalidate、删除重发均与冻结旧算法对拍。依赖闭包证明不得扩展为
整个编译器的安全证明，也不允许绕过独立Verify。

## 固定实验与停止

同一冻结生产源码构造 baseline/A/B/AB；A、B、组合分别归因。功能先比完整
输出/接口/阶段文件、目标/取消/错误与逻辑额度。单独计数版直接数实际
Types body solve及DB遍历；不对计数版取性能。限定固定真实消费者、正反向
调用链/递归和模块扩展，完整compile_path的cold/unchanged/private/public，
包含文件发现、解析、身份、Analyze、Verify及Build链接。cold为空session，
不清OS缓存；父VmHWM不声称包括clang总峰值。完整进程另计源记录/析构。

先冻结全部单元与规则，再在root独占窗口执行1轮预热+7轮保留配对、交替顺序，
全部样本与失败保留；统计是有限探索，无多重比较校正。缺Verify、产物不一致、
错误/安全观察不一致即停止该候选。代表性完整成本无益或有可重复退化则
不宣称通用提速；准入由非作者审核与root裁决，不能拿大链倍数掩盖小例。

## 来源与归因

原论文为Deng/Hani/Ma, Long time derivation of the Boltzmann equation from
hard sphere dynamics, arXiv:2408.07818v3 (2025-07-18)。free/fixed切口保留
原身份，UP依赖方向不变量；本片取其显式边界/局部传播启发，编译器正确性
分别证明。A是既有单调工作队列在尚未优化阶段的应用，B是维护不变量
消除重复闭包验证；均不冒称原物理UP算法或原创普遍数学突破。

https://arxiv.org/abs/2408.07818v3
https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html
