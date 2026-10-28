# 203 Expand：完整 State 错误侧返回

在原规范 `1355ae0d2351199391d65ca4faa4a84ce768ece5c8aaaca465b8de5b20aad65a` 下，独审原输入 SHA `ea1b01a8e990dbbb850b6beb090cc0733a1b4ff0ff9461c6af91d8488f879514` 经过 `push -> State!void -> error_route<T> -> ?` 返回 `State!i32`，先被旧直接 State 结果分类拒绝。

修正仅用于已经具有一个拥有 State 参数的双声明路径：错误联合任一侧可保持完整 State 叶。两个 direct callable 输入构造器的结果仍须完整 State；不扩展 tuple/capture/opaque 逃逸。整函数定义检查及实际字段 selectors 不变。

该输入随后到达整数 payload 的旧上下文门：完整 `State!i32` 含 F/G，字面量 0 被误当成对泛型作数值运算。新双 State 分支只为错误联合里唯一的具体整数侧提供字面量宽度；整返回仍由原 `same_return` 比较两个实际侧并拒绝歧义，不能为 T:Copy 推出整数能力。

作者 `union01` 保留第二处真实失败；`union02` 实际新14/0、旧20217/0，旧和新3753/4120 N不变；`union03` 补运行负字面量正例和错误 bool/显式 i64 宽度反例。此前 dirty 运行不标成冻结 commit。原05文件复制独审字节，未改成功预期；完整消费者的 main 值推断为另一个规范增量，不由本修复绕过。

`union03` 实际在作者的 `0i64` 非法词法拼写反例失败，未到 Expand。已将这一新增邻居改为语言已有的 `i64(0)`，原独审输入仍逐字不变；首次失败日志保留。
