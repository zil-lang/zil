# 173：整仓回归中的精确旧预期迁移

76f87af的整仓03实际为1522 Pass / 4 Fail，原失败和全部后续测试均保留。四项均是已声明新行为或更准确的原有拒绝消息与旧断言不一致；本次不改任何生产实现。

1. `impl147_static_traits` 的共享资源`&Self`原声明，正是159/163/171已经保留并验证的同一源码。原串不变，从不支持列表移到Success且独立Verify一次。其他泛型trait、默认方法及重名trait方法限制原样保留。
2. `impl166_owned_results` 的拥有错误联合capture仍在Types以Unsupported/E-UNSUPPORTED拒绝。普通错误ABI推导漏项修好后到达实际capture检查，旧消息`finite Copy type`变为`capture requires a finite Copy value...`。保留输入、阶段、分类、诊断码，只更新这一条消息片段；不放开错误联合捕获。
3. Verify两处原始/wire非法Package字段测试仍为SafeReject/E-VERIFY-PACKAGE：170已经允许拥有数组字段，因此拒绝文案从`manages field`准确改为`owning field`。只更新两处消息断言，非法Copy字段消费/非法拥有字段复制依然拒绝。

047/163原资源头探针的相同迁移另见173接口澄清提案。源码数组/资源模式、闭包捕获和ByteBuffer头的未实现边界不在迁移范围。最终全量重新执行，不能把四个失败从计数中删除后改写原运行。
