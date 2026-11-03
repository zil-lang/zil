# 314：Type/Syntax 与普通 Copy 泛型

依据 `_zil_design.md` §16.1、§16.7、§18 的普通 Copy 与 where Copy 规则，本片在现有 CT artifact 源归因器内接通普通自由函数的有序类型参数及具体纯 struct 实例。源入口仍用普通调用、`::<...>`、构造及 Type/Syntax 结果；不修改 313 的 Expand/driver 入口或公共 schema。

定义期按每个真实 binder 的独立不透明 Copy 类型检查未调用 CT 泛型正文。该内部检查不产生可执行 artifact，不把某个具体 i32 代入来推定泛型能力；Copy 不授予字段、算术、比较、callable 或运行期资源权限。内联 Copy 与当前函数直接 binder 的 `where T:Copy` 幂等归并；其它 bound、const/lifetime、泛型 enum 暂仍明确拒绝。实际实例再次核查普通 Copy 结构；Type/Syntax 本身以及含它们的记录不能充当普通 Copy 类型实参。

函数实例键包含真实声明及完整有序规范化类型实参；记录实例键包含定义模块、原声明和全部实参，包含 phantom 参数。同布局不同名义类型保留独立身份，透明别名归一。CT raw 沿用既有独立函数/记录 schema ID；definition 保留原词法声明以维持反射可见性，源码重新归因和现有独立核继续认证实际 artifact。

新增约束扫描、绑定、类型复制/比较、实例查找、符号定义检查及队列增长先从同一请求余额收费。缓存命中仍扣比较工作；定义检查消耗保留，内部检查产物丢弃；不增默认额度。无泛型旧路径保留原费用；必要的类型正确性补检不另收额外预算。

限定验证使用已有 `impl260_compiler` 及 Type/Syntax 相关目标，保留 pending 两原源，覆盖推导/显式实参、完整身份、未调用反例、跨 binder、资源/地址拒绝、独立 raw/Wire/VM 和 N/N−1/fresh。正式全量、原生 O0/O2 和产品四目标由集成/消费者单独验收，本片未运行时不登记通过。
