# 277–280 结构化回调内的具体闭包组合

基线9c6a64ee；前片273–275冻结验收，本片隔离并行，合入依赖前片。

两个真实原源阻碍普通UI处理：泛型scoped wrapper中建立具体Event→Reply handler在Expand被拒；普通非泛型工厂返回拥有body后传给With在Verify被误作必须是wrapper形参角色。相同代码的非泛型最近控制通过。依据设计§9.3.1、§9.4及§12.4.1补组合；新profile不开放新的签名、捕获类型、opaque拥有签名、F逃逸、异步或跨线程。

具体本地闭包按其真实参数/结果/捕获逐项检查；不把With存在性、具体实例的更强kind或同ClosureId视为符号F的新权限。允许封闭Event→Reply handler在普通同步callable helper中定义、合法转交及调用，拥有参数/返回/失败清理沿原核。无With对照必须适用同一原则。符号F被捕获、声明FnOnce复用、借入延寿及其它原拒绝仍独立成立。

With的本地body也可来自已有合法具体工厂调用及局部转交。只消费已核公共返回合同下的本次实际值，不能用同型值、私有callee正文或任意无来源Call替代实际事实。角色Flow只描述wrapper真实callable形参；局部具体body沿实际资源/借用/清理核验，不捏造形参角色。错误返回合同、同型不同值、已移走body、重复非Copy消费、实际F别名或捕获仍拒绝。独立raw与Wire先保留准确结构/Flow正前缀，再变异最近反例。

复用现有IR/schema/Wire32；新增遍历及分配预付共享预算，不提高默认额度。记录新路径N/N−1/fresh及旧精确费用控制。机器验收复用既有功能target和按功能命名输入：源码→Verify→生产Lower→LLVM API及参考后端O0/O2，登记成功/null、准备分配失败、剩余环境清理和F返回后仍可用；独立oracle，非法源不native，不永久保留原始产物。

277负责Expand及必要Types通用组合，278负责独立Verify工厂body来源，280负责消费者与独立预期；相交文件先交root协调。常规完整门由root，作者不合入、不跑全量。

独审发现并复现了旧With-body来源16的继承漏审：将声明FnOnce但实际Fn的B装入本地body，内部调用一或两次、外层省掉B事件，旧Verify raw/Wire错误接受。Types和Verify现在对16/32统一核实际callable捕获来源，不能隐藏形参角色、恢复更强模式或择取混合来源；Analyze已有逐位置证明保持拒绝。单/双调用、合法Parameter转交和同ClosureId不同值的反例持久保留，合法独立借入body不变。

必要审计工作如实预付：Types单捕获With-body增加3 work、双捕获增加4，普通32、无With与节点数保持；具体源码控制2423→2426、5247→5250、6829→6832、6585→6589、借入body1489→1492。Verify本地body1690→1692、Managed2459→2462、Package3624→3627、captured handler4141→4145，direct1101、borrowed2057、wrapper1441、factory2861/4491及旧419/4990/2923保持；所有相关N/N−1/fresh重新核验，默认限额不增。两个helper-local alias raw因角色隐藏更早返回E-VERIFY-CALLABLE-CONTRACT，直接same-F及准备窗口反例仍独立要求E-BORROW-CONFLICT，不放宽为任意错误。

后续独审补出真实Managed+callback Package→普通Fresh工厂→body路径：
仅检查闭包构造会漏掉先进入聚合的supplied B。Types与Verify现在按收敛后的
实际输入审聚合构造/更新与普通Call，拒绝角色、未知和混合来源变成Fresh；
同ClosureId的独立Fresh值仍通过。State独立入口和已核单槽Parameter桥保留，
后者不能转作任意普通聚合调用通道。Analyze原实际值证明已拒绝此路径，
新增准确Managed正前缀及raw/Wire变异回归，不伪造结构错误充当安全证据。

最终Types工作为本地准备5250→5253、body-local6832→6834、
capture6589→6591、无With5364→5366；owned参数2426、scalar248、
普通factory1208、借入body1492与三项State/Parameter原控制保持，
Expand及节点费用不变。Verify原控制费用保持，新合法包装工厂N4552，
均继续核N/N−1/fresh；没有提高默认限额。

291移除了空聚合及单一基础scalar输入的无用审计队列：它们不能携带
callable角色，常量时间分类复用既有指令预费。旧Copy enum的610因初版
额外收2而回归，现恢复610，scalar420和flat418保持；原预算断言未改。
上述四个新路径从初版5257/6836/6593/5370降到最终精确值，均重新验证
N/N−1/fresh。多字段、Closure、Address、单槽array及未知/混合来源仍完整审计。
