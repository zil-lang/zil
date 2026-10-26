# 0070：自包含闭包工厂的公开返回合同

2026-10-24（Asia/Shanghai），基于 draft13。按规范 §9.4–9.5 实现第一条真实保存链：普通工厂返回一个隐藏的具体环境，调用者保存它、按已声明模式调用并最终结算责任。此提案先于实现提交；不更改语言设计正文或产品门。

## 表示与信任

- CoreFunction.callable_return: Option<CoreCallableReturn { kind, parameters, result }> 保留返回位置 impl Fn/FnMut/FnOnce 的原始声明；Core 可使用既有 Opaque/FunctionPointer 作为待解签名，Typed 不带未解 Opaque。
- TypedFunction.callable_return_contract: Option<CallableReturnContract { closure, kind, hold, call, writes, cleanup }>，真实 return_type 为 Closure(id)。hold/call/writes 是外部依赖字段索引；本片全部为空。cleanup 按字段序准确列出直接 Managed 捕获，不是可省略的提示。
- 环境捕获只允许有限 Copy 快照和已有 Managed 值；地址、Desc、嵌套环境/Package、借入局部/参数、非 Copy 回调结果均拒绝。回调形状固定；本片声明模式必须等于实际模式，不把更强能力透出，也不回改工厂内局部闭包调用模式。
- 一个工厂对应一个隐藏闭包身份；所有正常返回必须为同一实际 Closure(id)。工厂递归、工厂返回转发、不同闭包分支合并和一般 trait opaque 尚不接纳。工厂本身可使用现有普通函数和有限资源操作。
- 工厂输入同样只允许有限 Copy 或拥有 Managed；独立 Verify 自行检查 raw 参数表，不能让 Address/Desc 实参借空的外部 envelope 隐藏作用。原生对照覆盖拥有输入源片；此限制不改普通函数的已有借入参数规则。
- 独立 Verify 从真实定义、签名、捕获和 CFG 检查返回合同；拥有 SSA 返回沿现有责任核验消费，caller 的 Call 结果取得新载体责任。不能用公开字段替代定义核验。
- ScalarInterface.callable_returns 在 Types 为 None，Verify 后发布 FunctionCallableReturn { name, contract }；签名绑定实际布局和模式，公开合同指纹域升级 v3。

## 机器与失败

真实环境按现有内联 LLVM struct 按值返回，含 Managed 字段的责任位；caller Store 接管，借入调用使用同一环境位置。工厂不能在返回后清理已转交字段，caller 必须准确一次清理。FnOnce 调用使用已有固定 scratch 和剩余字段清理。无隐式堆、RC、锁或全局注册表。

工厂失败/检查 trap 不变；普通同步错误及晚实参失败使用现有守护临时。回调返回既有有限 Copy 错误联合已有机器证据；工厂本身返回错误联合包裹的闭包、取消、外部 retained callback、并发重入和注销排空尚未完成。调用者可跨多次同步事件迭代保留自包含环境，不保持任何已退出工厂的栈地址。

## 发布与反例

公共 schema/raw wire 同批升为11，Typed fingerprint域V11；旧1–10拒绝。wire字段长度在分配前计费，raw合同在指纹前限额。最近反例包含缺合同、错误模式/布局/cleanup、伪外部Hold、Address/Desc捕获逃逸、不同隐藏类型、未转交/双消费、调用后重复清理以及公共接口与定义错配。各阶段独立测试和原生来源完整记录，非作者审核后集成。
