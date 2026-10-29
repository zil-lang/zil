# 204 Analyze：真实叶 Store 拥有捕获

本实现依据设计 §9.1.1 与规范 `92c5dd37cb319f7c81b5ccbdcc51830f872808b485458c3b3a7f35bae65c74b7`。这是一份施工记录，限定编译/测试结果另存真实命令收据，不表示完整 1.4 或产品验收。

## 独立资格与真实输入

已有付费类型遍历仅在实际 Closure 捕获的拥有位置发现 `Managed<OwnedStore<Item>>` 时登记新 profile。地址、Desc、签名和 Store 元素内部不传播拥有位置。新 profile 从实际子 Closure 向拥有父 Closure 传播，有限无环图及所有新 profile 的实际入口/结果独立检查，结果保持 Copy/Unit。

内层 Item 先以有界显式队列核完整树，禁止 Closure、FunctionPointer、ForeignCallback、地址、Desc、Store、未解析类型及嵌套拥有类型，再使用已有 Copy 谓词；要求完整 Package 的直接 Managed 字段及有限 Copy 伴随字段。捕获 Package 的旧 Copy 伴随字段不受内层 Item 特有的 callback 禁止规则影响。每个真实捕获 Store 必须有唯一完整 canonical 资源定义，禁止同名异 payload、别名和用户 teardown。

对新 profile 的 `MakeClosure`，检查真实 SSA 操作数与对应捕获类型逐个匹配。既有资源核继续负责实际值是否可用、移动、不同 Store 的根、窗口、TakeCapture、分支/退出清理；资格分类不产生所有权，也不提供 Verify 可复用的证明。

普通工厂参数与返回环境 cleanup 入口只增加已独立核资格的真实 Managed Store。Desc 不能进入这个拥有分支。

## 费用与验证边界

没有实际新 Store 捕获时，不构造新增捕获图，不扫描全体 MakeClosure，不收费新资格。新根列表、图、队列、名义键、比较、Copy 判断和表扫描先收费，默认限制不变。原无 Store 类型/工厂路径保留原费用。

结构 preflight 同时给原先未展开的 raw `Opaque.hidden` 加上已有的深度/输入界限，避免后续递归类型比较遇到未经界定的隐藏树。正常 TypedIR 已完成 Opaque 擦除；这是 raw 未降低类型的边界修复，不能宣称任意原 malformed raw 输入费用不变。

Analyze 不重新定义 ABI 字节算法。实际非零布局、字段 flags、步长和后端表示仍由独立 Verify/LLVM 检查；任何 raw 候选必须按实际到达阶段报告。

## 真实 Store capture 窗口反例

前端原 `0ead0009` 源在 `40dddcb5` 被独立复现：FnOnce 内从 capture 借元素后 give 父 Store，再读旧元素，Analyze 与 Verify 均错误通过；原负例不执行机器代码。`windows-before01` 保存原输入与完整报告，正确近邻先读取成 Copy 值再 give 的 O0/O2 已实际通过。

Analyze 的私有 Hold 重建已有精确 `Capture`/`Field` 根，但资源 pass 的 Desc liveness 并不代表元素的强 Address 窗口。修复复用 Analyze 自身私有根及真实 CFG 地址 last-use，在新 Store capture 的拥有转交/结构变更处核窗口，不以导出的 CandidatePlan 或 Verify 结果当授权。新 profile 位来自本核已收费类型遍历；仅该新路径构造地址 liveness，旧无 Store 路径不增加扫描或费用，Package 字段根不在检查前粗化成整个 Local。
