# 203 双根 Store 资格与原201断言迁移

§8.2.2明确把总计两个直接callback字段扩展到两个实际自含类型，结果均Copy。root在`f250342b695f8b5f5d7d8a6b38f22375c7ee01b1`先实际运行原201 `one_type_binding_keeps_both_fields_and_rejects_extra_or_heterogeneous_fields`，保留真实Fail：原测试第149行的`[0,1]`拒绝断言不再成立。收据`run-logs/203-root/old201-layout01`在交付证据中单列，未修改旧收据或伪造旧版本成功。

这次仅晋升该完全相同raw输入为接受并校准测试名；同一原测试的单字段、同根双字段与三字段拒绝控制保留。新增测试独立覆盖不同实际大小/对齐/live flags、共享子布局但各根保留实际存储、坏第二根/环/缺失id/Store capture、两个结果Copy限制、原单根lending资格和新N/N−1/fresh。它们属于原始资格/布局层测试，不冒称源级F/G声明模式或原生机器行为；后者由driver/consumer/非作者补齐。
