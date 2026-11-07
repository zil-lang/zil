# 097：两个局部正确性论证

这些论证只说明生产编译器中两处重算的等价替代，不构成整个语言的安全证明。

## A：有限单调摘要的实际读取工作队列

每个函数摘要是有限Desc参数下标集合；单函数扫描只随被调用摘要增加而增加写
效果，旧实现以集合并累积。把所有函数初始入队，执行单函数转移时记录所读取
的callee摘要，包含尚不存在或为空的摘要。callee严格增长后通知已登记consumer。

对于某次callee增长，其consumer只有两种情况：已经扫描的consumer已登记依赖，
会被通知；尚未扫描的consumer仍在初始队列中，会读取新摘要。队列去重仅合并
尚未执行的同一consumer，不丢失未来的重算。依赖表保守累积、不删除，因而
不会因为后续转移路径变化漏掉此前观察。未知调用的保守写效果保持不变。

每次摘要改变都增加至少一个有限参数事实，故总变化次数有限。通知只由严格
增加触发，因此队列终止。终止时没有遗漏被更改输入影响的consumer，摘要是
原转移方程组的不动点。初始为空、只以单调转移和集合并增加，迭代始终位于
任何不动点之下，故它与旧全轮扫描得到同一个最小不动点。

该结论依赖单函数转移保持不变、真实callee身份、读空摘要也登记、全函数首次
排队、公平处理和单调增加，不能移植到会删除事实的动态分析而不重新证明。
当前预算对新增索引/队列/读取登记/通知计费；更改的N/N−1不属于接受语义等价
之外的隐藏事项，而是已先行登记的资源限制变化。有限控制包括全部4096个
三节点有向图与写种子，生产控制仍经过实际源码前端和独立Verify。

## B：公开更新维护的闭合查询图

维护不变量：每个resident snapshot的每条依赖均指向resident endpoint，所记录
kind的指纹相等，resident图无环。空数据库满足；QuerySnapshot以无内部可变性
的Arc发布，外部持有Arc不能直接改动内部图。

`publish`首先检查全部依赖存在且指纹一致。待发布key处于active状态：如果
某条依赖可达旧key，原DFS在该active key拒绝，因此替换不会制造环；如果key
尚未resident，闭合不变量保证任何resident路径都不能提前引用它。检查失败
仅清除active标记，不改旧snapshot。检查成功后，`invalidate`删除改变kind的
消费者及其完整传递消费者；剩余边仍闭合且指纹一致，再插入新snapshot。
外部单独`invalidate`只删除这种传递闭合集合，也保持不变量。`clear`回到空图；
`begin/abort`只改变active集合。

读请求先保留目标key本身active的错误优先级。如果所有active key均不resident，
它们都不在任意resident依赖闭包中，因此旧DFS不会遇到active节点；闭合、指纹
一致和无环三项已由更新不变量保证，DFS只能返回目标snapshot或在目标缺失时
返回None。这正是一次直接查表的结果。只要存在resident active key，候选保留
旧DFS和它的错误顺序；不会凭“可能无关”跳过检查。

最近反例是 `begin(absent); read(absent)`：即使active key不resident，也必须
报cycle，不能先查表返回None。另保留active祖先/无关resident、同指纹换依赖、
不同kind改值、失败发布、重复边、删除重发、clear等控制。公开更新后的闭合
不变量不适用于任意外部反序列化图；本片不以缓存字节或前端证明制造Verify许可。

## 研究归因

Deng/Hani/Ma论文的identity-preserving cuts和有方向条件的局部处理可启发明确
“哪些边界信息必须保留”，但其molecule图和cumulant估计不证明以上两个编译器
命题。A是既有单调工作队列的定点应用，B是利用维护不变量消除重复验证。
Rust增量编译也记录真实query reads及其顺序；这里不宣称发明一般图算法。

- [Deng/Hani/Ma, arXiv:2408.07818v3](https://arxiv.org/abs/2408.07818v3)
- [Rust Compiler Development Guide：Incremental compilation](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html)
