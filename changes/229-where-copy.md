# 229 普通泛型where Copy正规化

父任务B0，接续main 6ba0b04ee8b9cac3ead187b2aa9f03c91c217e49；合同为_zil_design.md §18.1的229段。语法已解析where但Expand直接拒绝，本片把普通自由函数的直接类型参数Copy约束正规化到已有内联bound，复用现有定义期与实例期核验。

仅普通类型泛型、空或Copy内联bound；where主体是当前函数直接单段参数且仅无实参Copy。重复Copy幂等，其他约束不丢弃。未调用定义、非Copy实参、跨模块及别名仍按普通规则检查。新工作从同一Budget预费，并传递剩余余额给alias与Engine；旧无where路径保持。

作者/root/mainline_recovery；非作者/root/scope_audit；root管理主规范和公共索引，第三代理提供消费者独立对照。写集为generics.rs、generic_where.rs、generic_alias.rs、generic_budget.rs及本片测试证据；与225–228实现分离。完整trait/lending、方法/impl/callable/StoreElement where不在本片，但终态义务不删除。

正例比较内联/where/重复where及多模块实际机器结果；反例覆盖未知主体、别名/投影/组合主体、const/lifetime、非Copy、多bound、未用正文越权与真实非Copy实参。N/N−1/fresh及旧费用需分别测量；不以语法简化冒充Rust产品突破。
