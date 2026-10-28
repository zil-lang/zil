# 172：同质拥有 Package 动态前缀存储提案

本记录先于实现。当前基线 d9f8f9469cf58b4d7dd4a1d2ac966bd1dc0ee3e0；规范正文不变；公共 schema/Wire 由 173 统一发布。

## 需求和第一片

ZUI 可变子项集合需要运行时数量的真实拥有元素。167 的固定数组只提供静态槽，不能把未知 cap 展开成槽或把元素家族伪记为 One。本片以密封资源头保存同质完整 Package 的初始化前缀 `[0,len)`；尾部 `[len,cap)` 没有元素责任。只暴露下列已核原语，不提供原始指针、元素借片或可写 len/cap。

元素限原直接 Managed 与 Copy 字段的完整 Package，且布局非零；至少一个明确 manages 字段。支持用户有限 Copy resource 和 ByteBuffer 字段。暂不支持 170 的嵌套拥有数组字段、Store 递归元素、Closure 元素、异构元素和借入字段。本片不自动增加容量。

## 源面和模式

保留名 `Store<T>` 是密封资源族，默认 Desc；create/give 与现有资源规则完全相同。保留六个裸原语名 `store_new/store_reserve/store_push/store_pop/store_len/store_capacity`；不能以局部或普通函数遮蔽。`store_new()` 必须有具体 Store<T> 上下文；可写普通工厂如下，也允许同型的局部注解。没有上下文不能猜测 T；显式 `store_new::<T>()` 不属于首片。

```zil
fn empty() -> Store<Item> contract { result(create) } {
    return store_new()
}
fn relay(give s: Store<Item>) -> Store<Item> contract { result(give) } {
    return give s
}
```

| 原语 | 参数 | 结果与责任 |
|---|---|---|
| store_new | 无，必须已知 T | 新 Managed<Store<T>>，null/0/0，不分配 |
| store_reserve | Desc<Store<T>>, i64 绝对容量 | bool；负数、尺寸溢出、分配失败为 false，原状态完全保持；已有容量满足为 true |
| store_push | Desc<Store<T>>, 完整 T 按值 | T!void；有容量则成功并消费进前缀，无容量则错误载荷归还完整 T；不分配、不调用用户代码 |
| store_pop | Desc<Store<T>> | void!T；非空先撤销尾项责任再转交完整 T，空返回 Unit 错误 |
| store_len | Desc<Store<T>> | i64 初始化前缀长度 |
| store_capacity | Desc<Store<T>> | i64 已分配元素容量 |

可恢复失败不用 bool 伪称已传值仍归调用方所有。push 的返回值与参数都是完整责任载体，必须沿真实 tag 清理；pop 也不发布未初始化项。普通签名、错误传播、give 和现有自动清理可组合。元素从前缀整体取出后才可使用其现有字段/Desc/部分移动能力；Store 内不外借元素地址，避免先承诺迁移稳定性。

## 公共候选表示和独立核验

请求 `Type::OwnedStore { element: Box<Type> }`，仅允许位于 Managed/Desc 内以及其精确密封 ResourceDefinition。资源名 `@builtin:OwnedStore<包的完整名义名>`，不接受普通构造、用户 teardown 或外来 raw payload。其原生头是 `{ ptr, i64, i64 }`，但不等同 ByteBuffer；普通 source/raw/Wire 均不能把两种类型互换。

请求以下 Typed/LIR 指令；编号由 173 配发：

- `OwnedStoreNew { element: Type }`：类型明确在候选指令中，Verify 不从声称的结果类型反推授权。
- `OwnedStoreReserve { store: ValueId, capacity: ValueId }`。
- `OwnedStorePush { store: ValueId, value: ValueId }`。
- `OwnedStorePop { store: ValueId }`。
- `OwnedStoreLen { store: ValueId }`。
- `OwnedStoreCapacity { store: ValueId }`。

Drop 复用已有 DropManaged，但按精确新 payload 派发清理。TakeLocal/give 只转交一个真实 Store 头及其整个动态前缀，原槽不能再次释放。同一个 Store 的不同 Desc 名字不是不交；描述资格与已存值的责任由 Analyze 和 Verify 各自从 CFG 重建。Push 消费一次完整 T 并产生拥有错误结果；Pop 产生受运行时空检查保护的拥有结果。密封 helper 保持动态前缀不变量，不为每个运行时元素制造静态节点。

首片不准入 Store 捕获、Store 的 Package 字段或资源头地址，以免 existing supported() 扩展隐式开放未经核验的环境/字段清理协议。普通参数、返回、局部和给出可用；后续组合另以明确最小增量核验。

所有 Type 子树在克隆/比较/布局展开前按现有预算计费；运行时 len/cap 不改变编译预算。无预算默认值提升。新型纯结构谓词不能替代真实定义、操作数、责任和来源核验。

## 原生操作和失败保持

Linux x64 起步，复用既有可信 allocator ABI 的声明与分配/释放入口。元素 stride 使用受核的自然布局，reserve 拒绝负容量与 `requested > isize::MAX / stride`，成功 realloc 后一次提交 pointer/cap；失败不写旧头。非零布局避免零字节 allocation 的平台语义分歧。reserve 不运行元素 Drop，也不复制独立清理旗标。

Push 在完整 RHS 准备成功后检查容量；成功按 element 型写入尾槽，再提交 len；失败错误结果携带原值。Pop 先检查非空，将 len 减一，再把原尾槽的完整值载入返回；旧槽不再在前缀中。期间没有回调/挂起/可恢复失败。销毁先由词法激活清除头责任，按索引升序对前缀中每个完整 Package 执行字段声明序清理，最后释放块；释放后不再读写块中记录。runtime helper 不导入手写 C 容器。

## 近邻反例与验收

保留旧编译器对目标源的拒绝；新增真实源码运行时数量循环、多模块普通参数/返回/give、Push 满时归还、Pop 空时错误传播、已有两项后扩容、构造后续项失败与整个容器早退。每个 Ticket Drop 一次，ByteBuffer payload 分配/释放平衡；O0/O2 分开实际执行。

源码负例包括无上下文 new、非 Package/零尺寸/嵌套 Store 元素、遮蔽内建、重复 move、give Desc、Store 捕获/字段、任意头投影和地址逃逸。raw 与 Wire 篡改包括新建 element/result 不一致、伪造密封定义/teardown、MakeManaged 伪造 Store、错误 push/pop 类型、重复消费/漏清理、假名义布局，以及N/N−1元数据预算边界。工具失败或任意拒绝不当成指定反例成功。

本片不宣称通用动态容器、异构回调集合、外部回调保持/取消、并发、Windows 原生行为或 Rust 成本目标已完成。
