# Store 的按需增长追加

`std.store.push_growing<T:StoreElement>(items:Store<T>,value:T)->T!void` 在普通库源码中组合现有 `store_len`、`store_capacity`、`store_reserve` 和 `store_push`。原 `std.store.push` 保持固定容量行为；新接口不增加编译器原语、元素资格或借用权限。

## 容量足够时直接追加

底层 `reserve` 接受绝对容量。有效请求不超过现有容量时直接成功且不缩容；真正增长成功后，容量才更新为请求值。每次追加都只请求 `length+1` 会反复分配，因此增长接口只在当前容量满时准备空间：零容量增长到一，此后通常翻倍；超过 `i64::MAX/2` 时选择 `i64::MAX`，达到最大长度时返还输入。乘法只在结果可表示的分支执行，已有空位时不走增长分支。

这里的上界约束元素个数的算术。实际可申请数量仍由原 `store_reserve` 根据元素布局检查：请求数不得超过 `i64::MAX / stride`，再计算字节数和调用分配器。选择了可表示的增长容量，不代表该元素布局或分配器一定能满足它；失败也不自动降为较小请求。

## 准备失败返还原输入

调用方的实参按通常规则求值一次。需要增长时先完成容量准备，失败返回实际传入的 `value`，保留实参求值完成时 Store 的长度、容量和已初始化前缀。对拥有值，这个错误载荷仍承担原来的唯一资源责任，调用方可以检查它、重试或清理；成功才调用原 `store_push` 追加一次。Copy 值沿用原 Copy 行为。准备失败不会回滚实参构造中已经发生的输出、计数或其他外部副作用。

增长可能改变底层地址，因此仍受[真实 Store 元素借用](store-element-field-borrow.md)及[普通 Store 所有权](ordinary-store-ownership.md)规则约束。增长前已经结束使用的视图不阻止调用；调用后还要使用的旧视图不能凭地址碰巧未变而继续有效。实际捕获、父借用与 owner 转移由 Analyze 和 Verify 各自检查。

## 维护入口

实现位于 [std.store](../lib/std/store.zil)，[消费者](../tests/language-readiness/owning-trait-hosts/store_push_growing/consumers.rs)覆盖直接 owning 实参、Copy 元素、实际 FnMut 捕获、最后使用点、消费后复用和活借用反例。反例要求与原固定容量 API 的相邻误用具有相同的诊断阶段、类别、错误码和消息。

原生控制应核对完整返回值、准备失败后的旧前缀与容量、实际返回 owner 的重试，以及分配和释放。四次 owning 追加只需要三次容量增长，第四项利用既有空位；构造输出与计数同时约束实参的一次求值。逆序输出来自显式 `pop` 后移交 `dispose`，不作为非空 Store 自动析构顺序的证明。实际执行范围及工具链由[限定验收收据](../validation/ordinary-store-growth.json)绑定。
