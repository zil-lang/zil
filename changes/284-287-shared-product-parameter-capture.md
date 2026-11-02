# 284–287 同步共享 Product 参数捕获

基线6e3b6e42；前片283验收与本片隔离并行，合入依赖283。原始缺口：
`fn render(panel:&Panel)->i32{let body=()->i32=>panel.count;return body()}`
可通过Types，Analyze及独立Verify仍只接受普通Local根。

依据设计§8.1.2与§9.1.4，增加callee内局部、同步、非逃逸的既有共享
`&Product`参数捕获，结果限原有限Copy/Unit。两核独立恢复并保留
Parameter(i)、完整Product资格及父来源链；调用处按真实实参映射Hold、
投影与窗口，不把Parameter重标Local，也不凭Address类型授信。

实际参数可来自合法普通局部或Store元素；本片明确开放后者经参数的
同步窗口，保留整次调用的may效果核验：相交reserve、pop、give、drop、
替换须拒绝；调用返回且无未来使用后可操作，不同Store应通过。
不承诺callee内最后一次读取后的失效顺序精化。直接动态Store窗口捕获、
外层capture来源、返回/长期保存、可变Product与异步不随本片开放。
空、未知或丢失父链的来源继续拒绝。

不新增公共IR/schema、运行时机制或借用模式，不删Local-only门后直接
放行。新增工作预费且默认额度保持，原普通Local及拥有参数Local费用
单独复验。合法原源保持字节，最近反例须真正到达相应核验边界；独立
raw/Wire、N/N−1/fresh和生产源到两后端O0/O2分别记账。

284由root管理契约/集成；285负责Analyze；286负责独立Verify；287负责
已有共享Product功能target中的源码/机器消费者，不增加工单名Cargo目标。
无需新Types能力，发现前置缺口先给最小实源，不暗改签名或扩大范围。
原始产物仅临时保留，入库只有有用输入、命令与精简实际结果。

285 分析核先恢复实际 Parameter 根及调用点私有完整 Product/Store 父位置；仅该
新增路径预付调用 may 摘要。普通 Call 的传递 Store 操作与拥有 Store 或含 Store
的 Product 参数按整次调用检查。Store 来源未知、或被调用闭包持有 Store 而实际
父来源尚不能恢复时保守 Unsupported；不以名义闭包类型或不同参数名称证明不相交。
源、raw/Wire、预算、独立 Verify 和机器验证仍须各自完成，本说明不代表通过。
