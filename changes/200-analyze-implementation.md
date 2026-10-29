# 200 Analyze：独立核验两个拥有输入的有限 State 构造边界

本组件生产提交为 `0b7b2e51fcd661e8f8420ad9206e68b579ab7bc6`，只增加 `zil-analyze/src/helpers.rs` 中的有限入口识别。作者最终限定测试绑定 `65e82c62de59149078313ff772920c0f667009df`，含前端/Types/Verify 组合 `8247f3bfcbdd3842aab57c25d31cbeef7e51849f`。该结果等待非作者审核，不是200整体验收或完整语言终验。

设计摘要为 `13ea0962a2bf73893a0b03eac133456216856f3748152392c925e95028059c7d`，规范集合摘要为 `4402e04fd45551cbdd8dff92c4ac51af8d8ddd6f87296c17ce3690f55359bff3`，均从实际文件重算。schema/Wire 25、默认额度和公共表示不变。

## 实际变化

旧的单输入拥有构造例外及其原计费逐字保留。只有无 public helper 合同、没有外部或环境入口的函数，才考虑新增的双输入边界：两个不同实际参数 local 必须直接按值承载同一 ClosureId；返回完整 Package 必须含一或两个直接、同 id 的 Closure 字段。其余实际参数逐个限制为有限 Copy 或直接 Managed。函数名称不参与判断。

Analyze 从模块的真实局部类型、返回字段和捕获图重建这些条件，没有读取 Types 的分类布尔值或调用其求证函数。相同 ClosureId 仅代表类型相同，不合并两个 local、SSA 值、字段位置或清理责任。自含捕获检查沿既有规则允许已经核验的 FnMut lending，不套用196普通双角色同步 helper 的 Copy 结果限制。

字段扫描、参数归属查找、递归类型工作及自含捕获图遍历先计费。通过有限入口后，实际 TakeLocal、MakePackage、初始化、完整转交和清理仍由原资源检查继续核验。普通两个角色的同步 helper 必须保有其真实 public 合同；本片没有为空调用制造合同，也没有改动资源核或 Verify。

## 作者限定运行及证据等级

| 实际命令组 | 测试组 | 结果 | 证据等级 |
|---|---:|---|---|
| `zil-analyze --test impl200_constructor` | 8 | 8/0 | 手工 TypedIR；不能替代源码或机器执行 |
| `zil-driver --test impl200_analyze_constructor` | 8 | 8/0 | 真实源解析、展开、类型与Analyze生产；以其模块为基础的 raw/Wire 直接攻击 |
| `zil-analyze --test budgets --test impl196_roles --test package_budgets` | 27 | 27/0 | 既有有界工作量、联合角色合同和Package责任回归 |

手工 TypedIR 共记录20个普通断言输入：9个通过控制、11个正确拒绝，另有3组独立 N/N−1/fresh 探测。控制包含一个输入/一个字段、两个输入/一个字段、两个输入/两个字段、名称无关、旧单输入保留地址额外参数、新双输入接受 Copy 和直接 Managed、普通 helper 的真实合同。反例覆盖不同类型或借入参数、重复 formal 导致未初始化、零/三字段、同一非 Copy SSA 重复装配、缺字段、第二输入责任未付、自含捕获伪造和缺失 helper 合同。

source派生 raw/Wire 每路线实际20个输入：8个控制通过、8个 SafeReject、4个 Unsupported。8个控制中7个直接来自实际源码生产，另1个控制从实际模块克隆构造函数并追加有限 Copy 参数，属于 TypedIR 派生。这个未被调用的新函数用于单独变异额外参数为 Address、Desc 或 State，排除调用者签名不匹配先拒造成的混淆。Analyze 在新增入口处拒绝；raw/Wire 准确报告有限构造签名 Unsupported。这3例只证明有限入口关闭，不算借用或清理安全通过。

原00、原01和函数改名实际进入 Analyze、raw Verify 与 Wire Verify。非 Copy SSA 重复装配分别触发Analyze的已消费责任检查及Verify的 `E-VERIFY-MOVE`；漏第二清理触发真实资源责任拒绝。普通 helper 删除合同后由Analyze及Verify的独立合同检查拒绝。实际工厂捕获改成Desc会更早在返回工厂的自含检查处被拒，未将它标为新构造门的责任证明。

Copy 环境原源码正确经过独立暂存和 TakeLocal 发布。仅将真实参数生产者改成 Load 后，Analyze能够产生候选；raw/Wire 仍分别以 `E-VERIFY-MOVE` 的访问视图不可发布诊断拒绝。这里是既有Verify发布边界的证据，不能称作Analyze拒绝，也没有给无资源 Copy 环境制造虚假清理义务。

本组件未运行LLVM/native、Wine或原生Windows。200消费者、故障注入和整体平台结果由相应作者/独审收据负责，不能移植到此组件的运行身份。

## 预算与保留失败

| 实际输入 | Analyze最小工作量N | N−1 | 新状态重试N |
|---|---:|---|---|
| 手工一个Closure输入/一个Closure字段，另有Managed参数/字段 | 1241 | ResourceLimit | Pass、完整候选相同 |
| 手工两个Closure输入/一个Closure字段，另有Managed参数/字段 | 1466 | ResourceLimit | Pass、完整候选相同 |
| 手工两个Closure输入/两个Closure字段，另有Managed参数/字段 | 1626 | ResourceLimit | Pass、完整候选相同 |
| 原00单回调State源码 | 11099 | ResourceLimit | Pass、完整候选相同 |
| 原01同F双回调State源码 | 15920 | ResourceLimit | Pass、完整候选相同 |

最初raw01在 `d3da2046` 为0/8：手工Package缺当前既有Managed资源资格，普通helper控制的清理顺序也不符合逆初始化顺序。补齐有效夹具后raw02为7/1、source01为5/1，余下两项只是期待了错误的更晚诊断；实际分别先报未初始化和返回工厂捕获外部描述。失败命令、输出和对应历史源码全部保留，没有用修改生产核来使这些断言通过。

最终实际命令、source、TypedIR、候选和Wire字节独立存于 `200-analyze-evidence.zip`；Git内只收 `evidence/impl/200/analyze/SUMMARY.json` 与 `MANIFEST.json`。压缩包不包含执行产物。

## Rust四目标及当前边界

| 目标 | 本组件实证和限制 |
|---|---|
| Rust同类安全 | 新有限入口与实际来源、Move和清理核分离，原规则保留，raw/Wire分别拒绝近邻攻击；不证明完整语言安全 |
| 运行与内存同级 | 本组件没有机器执行或同合同Rust运行/RSS比较 |
| 表达与维护更简单 | 原双输入State源码可达独立核验；没有维护工作量比较 |
| 编译成本更低 | 原额度内工作量与fresh失败重试有实测；没有编译速度比较 |
| 至少一项突破 | 补齐有限普通组件构造缺口；不将工程能力增量称为产品净突破 |

F/G State、callback Store、State加replacement F和两个State输入等扩展未由本组件开放。完整语言 NotSatisfied、产品 NotRun、B0 Doing，产品门字节与默认预算保持。
