# 178：同一泛型 Store 库的真实消费者

`collection.zil` 使用编译器保留的 `T: StoreElement` 约束，定义构造、扩容、
插入、弹出、计数、完整给出及清空。`model.zil` 提供两个不同的名义元素：
Item 拥有 Ticket 和 Copy 值；Other 另外拥有 ByteBuffer。
`state.zil` 的具体 callback State 在泛型 F 正文中调用同一容器库，保留真实
FnMut 环境并连续调用两次。入口使用四个模块，不用函数名特判或手写容器。

```sh
zil build examples/mainline/generic_store178/main.zil --opt O2 -o program
printf 3 | ./program
```

输入 0–3 决定 Item 数量，成功输出相应 A/B/C 前缀后接 HZ，退出 0。
H 是 callback 捕获的 Ticket 清理；Z 是 Other 的 Ticket 清理。
非法输入 9 退出 9。运行 `tools/language/178/consumer.py` 可在 O0/O2 下逐个
真实分配点注入失败，并核对成功分配、释放和存活数。

能力边界：StoreElement 是有限完整 Package 形状，不授予 Copy、字段访问或
Managed 权限。普通 Store 参数仍是 Desc，给出需真实载体；引用/嵌套 Store/
拥有闭包元素仍未开放。循环只能保持外部非 Copy 消费状态和载体来源；一般
循环携带责任、多个 callable 类型参数和借入逃逸不由本例开放。
