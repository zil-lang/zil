# 230 有限生成普通枚举

依据 _zil_design.md §16.6，在228的普通struct/alias生成链上开放无泛型Unit/Tuple enum。动机是让声明生成可以表达已有普通枚举，并使构造器、模式、载荷和同批类型遵守同一名义身份。

使用既有未核AST与普通有限Copy枚举检查，不增加schema/Wire、LLVM布局、拥有权限、默认预算或求解器。每轮预绑定和确定卫生名覆盖enum，变体名称原样保留；载荷与属性迭代替换、类型/值遮蔽、保留名、公开冲突和ConfigReady不回流与228相同。只在实际含enum的新路径新增扫描费用。

主写集：driver ct_selection/materialize.rs、hygiene_types.rs、hygiene.rs；impl230_generation、ct_enums230消费者与本单证据。原impl228的enum拒绝样本保留原字节，按新合同晋升并新增record/generic/非法拥有载荷最近反例；不删除历史失败。

验收绑定精确实现提交，非作者审查；源→TypedIR→独立Verify/Wire→LLVM O0/O2与普通展开源对照，跨轮名义隔离、匹配、绑定遮蔽及N/N−1/fresh。该有限切片不代表完整Type/Syntax/#emit或语言1.4完成。
