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

285 后继把 Store 依赖判断扩展到闭包实际捕获的嵌套 Product/数组/和式及
闭包字段；代码指针的签名不是其持有的数据。闭包入口及普通调用的有限
may 写入图在当前请求内预付收敛：只读 Store 不被误记为写，可能写且无法
恢复实际 Store 父位置仍保守 Unsupported。拥有 Store 或含 Store Product 的
移动位置保留当前槽及原始父来源；不把 Parameter 改写成 Local。

Analyze 本地测量（target-285/jobs1，TypedModule raw/Wire；N−1 为
ResourceLimit，重新请求 N 与首次结果相同）：原参数普通 Local 源 N=8271，
参数来自 Store 的最后使用源 N=13063；不同 Store N=13334、同父不同 Store
字段 N=22232、不同 owner Product N=38853、give 不同 Store N=14076。旧普通
Local 捕获 `examples/mainline/shared_product_capture226/local.zil` 的冻结基线
与新路径均 N=17355。独立source-derived raw/Wire 复核同Store reserve/pop、
调用内部先读后失效、同父同字段、普通嵌套调用和先move再give/整Product
move及inline构造等9个反例均 E-BORROW-CONFLICT；伪造空参数来源与断裂Store父来源均拒绝。
这是 Analyze 限定证据，不替代 Verify、native 或整合验收。

独立审查另发现组装 Product 后提取字段丢失原 Store 父来源；逐字段传播
构造、整体移动及字段替换来源，未知来源保守拒绝。同字段两个反例 raw/Wire
拒绝，不同 Store 和先组装后借用的相邻字段正控通过（N=33921/25059，
N−1 ResourceLimit，fresh N 一致）。后续仍须独立审查及替换近邻验证。

此前把先借 left、再 give left 组装 Product、最后消费 right 的源误记为
相邻字段正控；窗口内 give 借入 Store 本身已违反合同，应拒，8880 原接受。
后继按实际 Store 元素借入或地址返回触发活窗口检查；该源现 raw/Wire
均 E-BORROW-CONFLICT。替代正控先组装 Product 再借 left/消费 right。

合法直线交换字段后才开始借用的源此前误拒；单直线无循环内，先由
活窗口检查逐操作确认，再按当前拥有字段位置判断，并核实所有拥有字段
写入的 Store 来源。交换正控 raw/Wire N=39384、N−1 ResourceLimit、fresh
一致；交换后借用同一字段、先借后移动和同源重填近邻均 E-BORROW-CONFLICT。
分支/循环及未知写入来源不通过此快捷路径，仍须独立审查。
