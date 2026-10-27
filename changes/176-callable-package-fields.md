# 176：具体自含回调进入普通拥有状态

## 为什么

现有固定回调数组能够保存具体环境，但 `package State<F> { manages ticket: Ticket; callback: F }` 尚不能经普通泛型接口构造、转交和更新。仅扩大 Package 类型谓词会遗漏三件事：源 opaque 工厂身份、字段的真实调用位置、递归责任清理。因此本片必须贯通源模板、完整值边界和机器字段。

## 有限合同

沿用已有一个 `F: Fn/FnMut/FnOnce` 绑定及普通泛型 Package 语法。首片使下面的完整 API 可执行：

```zil
package State<F> { manages ticket: Ticket; callback: F; count: i32 }
fn pack<F: FnMut() -> i32>(callback: F, code: i32) -> State<F> {
    return State::<F>{ticket: Ticket{code: code}, callback: callback, count: 0}
}
fn step<F: FnMut() -> i32>(state: State<F>) -> State<F> {
    var next = state
    let callback = &mut next.callback
    next.count = callback()
    return next
}
```

F 在一个被拥有的参数中出现；参数可以是直接 F，也可以是包含 F 的完整 Package。具体化得到一个确定 ClosureId 和实际内联环境，源约束同时保留不透明工厂身份；不同源工厂即使转发同一机器环境也不能混赋。已有不含拥有状态的借入/Desc helper 保留原有限合同。本片新的完整拥有 State 参数/返回只接受递归自含环境：有限 Copy、直接 Managed、已支持完整 Package、已支持自含子闭包；借入 Address/Desc/Slice、Store 捕获及未实现的拥有组合仍拒绝。

新 profile 的 Typed 函数使用完整拥有值的普通参数/返回。Verify 从实际 ClosureDefinition 与函数签名独立检查自含性和责任转交，不接受前端给出的空 Hold/Call envelope 作为证明。普通返回 F 不在本片范围，普通完整 State<F> 返回是本片目标。借入 State 地址、任意 dyn、异构字段和递归内联布局不随本片开放。

## 字段操作与机器表示

- 完整 Closure 字段是唯一槽位，即使环境没有资源，也不得通过重复 Load/SSA 操作把一次槽责任复制。
- 构造按源码顺序准备字段，失败只清已完成的准备值；提交后由父字段旗标承接责任。
- `let old = state.callback` 消费整字段并撤销父旗标。字段缺失时只能局部存活，不能完整传参/返回；补回必须是同一具体源类型。
- 替换先将 RHS 放入实际临时责任槽；RHS 失败不清旧字段，成功则先清旧字段再提交新环境。
- `&state.callback` / `&mut state.callback` 降低为公共 opcode 87 `BorrowPackageField {local,index,mutable}`，结果仅精确 `Address<Closure>`。地址指向当前内联字段，FnMut 多次调用访问原环境，不能取值快照后丢弃状态。
- 活动字段窗口阻挡相交字段取走、替换和父壳转交/销毁；未知来源不视为分离。FnOnce 只在完整取出后消费。
- 父壳销毁按声明序；Closure 字段递归清理具体环境。先撤销对应旗标，防止双重清理；不得以 `teardown=None` 静默跳过环境。

## 公开接口与预算

公共 schema/Wire/指纹由 177 同批登记。既有 Take/Drop/StorePackageField 复用，唯一新增指令是 BorrowPackageField。源模板/F 的替换、具体 Package 类型、Closure 环境展开、字段责任槽及借用来源遍历都必须先收费再克隆/生成，沿用现有默认额度。不同阶段独立实现检查，Verify 不调用 Types/Analyze 的证明函数。

## 必须交付的验证

真实多模块消费者覆盖：带 Managed/ByteBuffer 的具体环境，保存多次 FnMut 调用、完整 State 参数/返回、字段取出/补回/替换、FnOnce 消费及提前/错误清理。源码最近反例覆盖不同 opaque 工厂混赋、重复 take、缺字段整传、活动借用期间替换/转交、借入环境进入长期字段、无权限 mutable 借用；raw/Wire 覆盖伪造字段类型/索引/权限、重复环境值、缺责任/清理、深宽元数据及预算边界。机器运行检查输出、退出码、析构顺序，真实分配失败与剩余责任另有观察。

本片不改产品条件、默认预算、B0 状态，不把测试数量当成 ZUI 或全语言验收。
