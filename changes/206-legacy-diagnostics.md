# 206 旧反例诊断迁移

正式workspace01在clean d078f9e2cc5902167e78a5cb815638f9d05a2095、规范d71ea98d执行，2612通过、2失败、0忽略，265组。原完整Fail不改。此次只登记以下两个原始raw子例的准确新诊断，构造、输入类型和值、SSA、源码控制和其他近邻均保留，非法输入不执行机器程序。

1. Analyze impl201_store::fields_require_direct_finite_shape_and_complete_resources 的 hidden=Nested{Managed ByteBuffer,Closure}：类别仍Analyze Unsupported。206普通Product与Store旧资格分离后，新的Store父界先报“ordinary Package Product is not a Store element profile”；旧断言期待后续callback Store的“one or two direct self-contained callbacks”。只这一项改为准确新消息；旧address、array、nested Store及其它缺根/缺环境反例不改。
2. Verify package_tests::package_fields_reject_noncopy_and_nested_owners 的 extra=package()：原输入只把local字段改成新的合法nested Product形状，构造的SSA仍是原flat值，另有旧Copy投影。206不再从类型资格处拒绝，但在实际Store准确Verify SafeReject E-VERIFY-TYPE / “stored value does not match local type”。原raw和Wire必须继续拒绝，不能补构造让它成功。只此子例强化断言到准确phase/category/code/message；其余Desc/Address/Tuple<Managed>/ErrorUnion<Managed>的旧non-Copy拒绝保持。原120B真实source及独立新Product正例另有通过记录，不与这个伪造raw混淆。

旧接受205的测试源码可从2db3c090及完整原workspace02恢复；本轮原始失败的具体值与日志另存在run-logs/206-root/workspace01。这不是运行时生产行为更改，也不改默认预算、schema27或产品门。后续另发现的局部capture资格疑点不是这两项迁移的理由，须另行真实复现、修复、独审。
