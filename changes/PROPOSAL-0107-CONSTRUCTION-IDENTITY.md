# 107：未核会话快照的构造身份

## 冻结范围

本提案先于107实现提交。权威工单为主树 `.task/impl/107.md`；派工基线
`b9cce1d1b7301850a3e65b81323caea8ea88593c`，生产源码基线
`24ca9583d8789cfdfe3ccdbfe3eeff614bf56429`。语言/规范摘要与工单一致，schema17
不变。101阶段诊断指出raw Debug/digest是实际成本，本片检验消除该重复工作的
完整请求净效果；093同Debug字节流式SHA路线已经NoGo，不再作为本片候选。

仅修改 `CompilationSession` 私有raw查询的结果身份。`QueryKey`、其原规则串、
源/项目指纹、`TypedModule::fingerprint`、对外接口、证书、独立Verify、磁盘候选
格式/键、`Database`通用合同和全部默认/硬预算保持原行为。没有公开CLI开关，
没有把新值描述为IR内容hash，也没有用它授予Verified能力。

## 精确定义

在一次已完成的成功构造之后，用以下字节序列作SHA256：

1. 独立固定域 `ZIL-DRIVER-RAW-CONSTRUCTION-1\0`。
2. 八种raw快照的显式、互异种类标签。
3. 完整QueryKey：module、phase、input、rules、target。
4. 有序实际依赖数量，然后逐项编码完整依赖key、FingerprintKind和observed值。

定长整数用小端；变长phase标签加长度，集合/依赖显式计数。依赖顺序保留，
不排序、不去重、不用revision替代已观察值。`Dependency`与`QueryKey`字段采用
无`..`的穷尽解构，使增加字段不能静默遗漏。结果封装为私有
`ConstructionIdentity`，只在写入当前会话的查询元数据时取其Fingerprint表示。

`semantic/private_implementation`在这个私有raw查询子系统内都继续保守观察整个
构造。`abi=key.target`、`source_positions=key.input`和`build_inputs`保持原值。
`QuerySnapshot.bytes`仍为32个元数据字节，不增加raw反序列化或跨会话恢复入口。

不把指纹相同当作无条件数学等价。闭包证明的前提是固定编译器可执行文件内，
完整输入的规范序列不发生SHA256碰撞，并且调用的是下表审计的成功确定性构造。
身份只供一个CompilationSession的未核memo/依赖账本使用；不同编译器构建或
会话之间没有导入raw快照的协议。即使不同构建碰巧产生同一个数值，也不能据此
转移对象、声称内容相同或获得证书。未来改变输入、生产者或非成功缓存策略时，
必须重新审计闭包并更改这个内部域。

## 九个生产闭包

完整逐项证明见 [INPUT-CLOSURE.md](../docs/research/107/INPUT-CLOSURE.md)。冻结入口为：

| 调用点 | 成功raw种类 | 真正读取的构造输入 |
|---|---|---|
| single Parse | Parse | SourceUnit全部内容、Limits、固定默认属性上下文 |
| single Expand | Expanded | SourceUnit、已解析module、Limits |
| single CT | Core | expanded.core、target、Limits、纯CT编译器 |
| single Types | Typed | CT Core、target、Limits |
| shared Analyze | Candidate | TypedModule、Limits；项目路径从ProjectTyped提取同一module |
| project Parse | Parse | 当前SourceUnit、按有序前缀扣费后的parse_limits，或同源私有loaded_syntax |
| project Expand | Project | root、有序sources、AST、已解析imports、完整项目指纹、Limits |
| project CT | ProjectCore | 完整expanded CoreProgram、target、Limits、逐模块剩余额度 |
| project Types | ProjectTyped | CoreProgram、target、Limits |

源路径/文本、FileId/ModuleId、项目root、模块顺序、paths映射均有现存完整键绑定；
11项Limits、build_inputs、目标完整字段的绑定逐项检查。特殊loaded_syntax渠道
是从同一不可变源向量按同一token/node/depth预算得到的私有Arc，源字节预算在加载
及validate_sources分别检查；两个渠道成功时不因仅用于拒绝的字节上限不同而改变
AST。没有从文件系统另读语义配置、可变随机表、时钟或FFI执行结果作为raw值输入。

错误/取消继续在原位置abort；新身份只在成功构造且取消检查通过后产生。成功前缀
阶段可保留自己的完整raw条目，当前失败阶段不能发布条目；这不声称整个请求事务化。
所有到Verify的成功请求，包括热复用和磁盘源候选命中，仍各执行独立Verify一次。

## 对照与消融

保留原`RawSnapshot::digest`的逐字行为：Typed使用原公共fingerprint，其余使用原
Debug字符串的Fingerprint。研究构建只通过一个私有编译时布尔常量选择新构造身份
或原digest。工具将精确替换该常量的一处匹配并记录源码差异；不使用环境变量读取
来改变查询语义，不增加Cargo feature/public CLI。关闭常量的完整实现是消融基线，
原24ca生产基线也保留，不借消融开关抹掉历史NoGo。

source、plain、count三种证据分开。count构建只记录实际工作，不能作为计时binary。
最终比较必须核对阶段产物、公共fingerprint、接口、诊断、LLVM、原生输出和每请求
Verify次数；raw内部依赖身份按新合同单列，不要求它伪装成原IR内容hash。

## 冻结的功能反例

测试需要触发真正query/编译，不止比较一组手拼字节。最小集合：

- 同一源码/项目的cold、hot、真实private body edit、public edit、恢复原文；所有
  公共结果与新会话逐项相同，热请求仍Verify一次。
- 根选择、模块顺序、FileId、ModuleId、source.path、source.text和完整paths映射的
  独立改动；含同名模块别名转指另一模块的可见结果变化。
- Limits十一字段、build_inputs及TargetProfile完整字段的独立敏感性。即使该小例
  产物恰好相同，也允许保守重新构造，不能漏掉限额收紧导致的失败。
- 明确源图与文件加载渠道的阶段输出相等；精确累计字节/token/node/depth边界及
  失败/取消不能发布当前阶段的半成品。
- 依赖key每字段、kind、observed、数量和顺序的近邻反例；八种snapshot标签互异。
- 在测试内保留相同查询身份但篡改缓存Typed内容，仍应被原Verify拒绝。这是受控
  负例，不登记为自然生产缺陷；它证明新身份不能充当Verified权限。

本片不借口快照未核而容忍语义输入闭包缺项。发现缺项先保留可运行最小原件、
最近反例和实际阶段，交root明确处理；不能用“旧缓存也这样”作为正确性证明。

## 成本试验及准入

任何候选性能采样前，另行冻结107自己的精确输入/工具/二进制/交替顺序/样本数
与停止规则。复用101完整矩阵并补实际private body edit，采用root协调的A/A校准
结果确定可分辨口径。现在不自行计时，不从微观digest减少推导完整收益。

原1%准入界、真实消费者退化条件和product-gates不变。若校准不足或真实消费者
退步，保留结果并NoGo；不能增加样本直到过线，也不能用modules512热点抵消
其他完整请求退步。生产准入只由非作者审核和root固定正式窗口决定。

本片预期贡献是减少重复构造身份工作；不是新的全图复杂性定理，也不宣称四项
Rust目标已经同时完成。最终结果分别报告实证、未测和剩余缺口。
