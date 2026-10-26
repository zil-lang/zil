# 060 有限拥有包的公共表示、机器清理与 ZUI 前置

2026-10-24（Asia/Shanghai）。基线 draft11 `6acda08c72dbd3e387893b4260770f0834dcdbe0`；依据 `_zil_design.md` §§2、7、8，B0 下独立实施切片。此提案在本切片代码修改前写入。

## 为什么

现有 Managed 只能独立存放或进入局部闭包，不能以普通非 Copy Value 把多个资源组成一个名义载体。ZUI 的上下文、窗口与组件生命周期需要可构造、移动、传参、返回和准确销毁的资源组合。先接通完整有限包，避免把一般部分移动、逃逸或取消隐含到这个接口。

## 冻结合同

- 新 `Type::Package { name: String, fields: Vec<(String, Type)> }` 是有限内联非 Copy Value。至少一个字段为 `Managed<Record>`（由源 `manages field: R` 产生）；其他字段只允许现有有限 Copy。资源定义仍由 module.resources 独立核对。零 manages、非 manages 的 Desc/Address、嵌套 package、Closure 字段、本包用户 Drop 均暂不开放。普通 Value 的全规范结构 Copy 规则没有因此改写；这里只限制当前准入。
- 源面已有 package/manages，整体按普通 Value 移动。复用 `TakeLocal`、`Store`、普通 `Call` 与 `Return`；传参/结果均 `PassingMode::Infer`。`give package` 拒绝。Package 不能经过 Load、Copy aggregate、ErrorUnion、indirect signature 或 closure capture 暗复制/逃逸。
- `MakePackage { fields: Vec<ValueId> }` 结果必须是对应 Package，字段以声明顺序排列，完全类型匹配；每个拥有 SSA 输入各消费一次。源码先按规定求值次序准备字段，每个已成功资源由真实 owning local 守护；全部准备成功后无失败 Take + MakePackage 提交。这里没有公开未完成 package 对象：准备失败清理守护临时，按逆成功初始化序；完整包清理则按声明字段序。这两种轨迹分别验证，不混称逐字段部分成品。
- `ReadPackageField { local, index }` 仅读取完整活动 Package 的 Copy 字段；不先加载/移动整个包。`DescribePackageField { local, index }` 仅给其 Managed 字段实际头地址的 Desc；身份包含包局部及字段索引。不会转成拥有值、管理权限或稳定 payload 地址。
- `DropPackage { local }` 条件清理完整包。先将本地责任转入清理激活并清除标志，再按声明顺序执行每个 Managed 字段已有的 teardown。当前所有字段一同完成/转交/清理，单个本地 live flag 足够，无隐式堆盒/RC/锁/全局表。一般逐字段完成标志留待真实 partial/replace 合同。
- 参数在函数入口完整持责；返回/实参暂存/普通分支结果保持既有全函数 SSA 责任与存储 scope 分工。活跃字段 Desc（包括其别名或捕获）阻止源包 whole move、覆盖、StorageDead 和 Drop；证明不能把所有字段根折成互不相关的匿名值。
- 字段 `give`、赋值、spread、replace/decompose、部分成品的 whole move、拥有错误联合暂不开放；相关源码明确拒绝。不把 Unsupported 项移出完整语言完成条件。
- schema/wire 升至9，Typed fingerprint V9；Package type tag17，新操作 tags49–52 依次为 MakePackage、ReadPackageField、DescribePackageField、DropPackage。所有原有 tags 不变。

## 分工与验收

060单写base/ir/wire/lower/backend/CT入口与真实native总成；061独立源码与Analyze；062独立Verify与raw/wire。所有 Package 类型/指令均要求严格scope。Verify不调用Analyze/Types的证明函数，原始类型树/字段/操作/预算必须重检。

必要正例包括 create/give 字段、按值参数/返回、Copy字段访问、Desc字段写/捕获、分支与循环清理、乱序书写字段的声明序销毁，以及较晚字段/参数 `?` 失败。最近反例包括重复拥有SSA、漏清/重复清、活Desc跨wholemove、伪造字段类型、Load复制、错误模式及未开放嵌套。原生 ZIL/Rust 对照和实际资源关闭按实际执行记录；不增加或放宽产品门。

拥有包不能证明窗口/GPU/并发能力已完成；ZUI 应用闭环及未实现平台边界另记。Rust四目标和图研究的收益也分别举证。
