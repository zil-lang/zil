# 203 Expand：真实 State 值来源保留普通静态调用

最终规范 `a3557b7a000baed3d381b2f8f53eafd24f03c65aa8a94a3fdbf31708d69a9e52`，设计 `c83b9c0dd726d3d2740fb02b2e46d7b51f3891e031d9994c309a6734b421d489`。本补片实现集成方 `203-value-inferred-store-calls.md` 的 Expand 分工。

只在真实双 callable 调用生产者返回一个完整 generated nominal State 时，记录其完整名义来源。私有表使用已建立的 generated type 身份，实际家族来自原声明、普通参数及字段形状；不是全局 F/G 名字资格表。此事实只说明具体环境待 Types 值推导，不能证明 StoreElement、创建资源或许可调用。

隐式 ordinary 静态调用的真实实参经过 T/Store<T>/ErrorUnion 位置带来该完整来源时，保留原始 `T:StoreElement` 定义并输出既有静态 Call。原实参只保留一次，不增加运行时求值。不用残余 F/G 制造 TypeApply；返回的完整源名义 hint 继续通过 identity、seed、push、pop 和成功侧传递，实际每个 T 的完整唯一性、普通字段/phantom/双 Closure identity 以及责任由 Types 独立核。

无实际约束的 empty、函数值、动态调用不因此打开；源显式 TypeApply 仍走原完整类型约束。旧单 F/无新生产者路径不增加类型扫描，旧 202 17 项及其五个精确预算继续相同。

真实 `inferred01`：新五模块 Expand 1/0，旧202 17/0；新State测试13/1，唯一失败为作者此前要求隐式identity一定输出TypeApply的元数据断言。按照正式新增裸Call合同具体迁移该断言，并增加完全相同来源的显式TypeApply对照；失败保留。`inferred02` 实际新State15/0、多模块1/0、旧20217/0。新203原01/04的N由早期规范下3753/4120增加到3814/4181，来源登记先计费，早期收据不改写。

完整消费者已通过Expand，但其Types/Analyze/Verify/native/fault尚须组合候选实际执行，不能据此报告成功。源目录和runner继续使用同一失败后修正give的消费者字节；未以内联或删除普通T helper绕开问题。
