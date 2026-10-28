# 170：Package 内完整拥有数组字段

基线：d9f8f9469cf58b4d7dd4a1d2ac966bd1dc0ee3e0；父任务 B0。
规则依据：ZIL-MODES、ZIL-CLEANUP、ZIL-PACKAGE、ZIL-CAPTURE。

## 接纳增量与保持边界

Package 继续至少含一个直接 manages 字段；普通字段新增非空固定数组，元素为当前直接 Managed + Copy 字段组成的完整 Package。数组字段本身是不可 Copy 的 Value，不用 give。使用已有声明与成员语法，整个字段表达式消费移动到普通数组局部；后续静态下标操作复用 167。父壳保持字段完整/缺失两态，不新增嵌套部分位置或字段内数组下标借用。

构造按源字段求值顺序准备全部参数；全部成功才一次组成完整 SSA Package。整个数组字段取走/补回/替换复用 TakePackageField/StorePackageField/DropPackageField，并核对准确字段类型。ReadPackageField 仍 Copy-only，DescribePackageField 仍仅直接 Managed。替换先准备完整 RHS，失败保留旧字段；成功先清旧字段再发布新字段。父壳清理位置保持。

父字段责任位只表示完整数组存在与否；字段内数组按下标递增清理，其 Package 元素按声明字段顺序清理。SSA、普通参数与结果均要求完整父字段。数组元素、错误联合有效载荷内的此类完整 Package 同样必须递归清理，不能因类型准入扩大而漏掉子字段。

带拥有数组字段的 Package 捕获暂时在 Types 和独立 Verify 明确拒绝；现有 Package 环境视图仅维护直接 Managed 字段，不能借 supported() 变宽暗开保存。普通字段不新增 Closure 类型语法、直接 Managed 外的借用、动态数组、直接嵌套 Package 或更深的拥有数组字段。

## 公共接口和实现影响

Type::Package 和现有指令物理编码不增字段；Take/DropPackageField 准入扩大到指定的非 Copy Value 字段，完整字段责任与 `MakePackage` 的输入消费同步扩展。173 负责版本/指纹和接口登记，本单不写公共 schema。

Types 构造及投影；Analyze 独立字段责任形状；Verify 独立类型、完整性、责任与 raw/wire 消费；LLVM 完整载体递归清理；Lower 准入同步。布局/元数据按实际嵌套数组乘数计费，默认预算不变。

## 验证计划

- 真实源码：四字段顺序、数组整取/补回/替换/完整返回；局部提取后静态槽消费；跨模块普通传递。
- 原生 O0/O2：字段和元素 Drop 次序、部分父壳早退、后续数组元素准备失败、字段替换失败保持旧责任；字节分配失效注入与释放平衡。
- 最近反例：重复取字段、缺字段返回、同 RHS 两次补回、部分子数组冒充完整字段、临时父壳/嵌套部分下标、捕获父包。
- raw 与 wire：Read/Extract 伪装复制、取错字段/缺清理即 Store、重复 MakePackage SSA 输入、字段越界/大长度元数据预算。
- 旧 060/072/099/166/167 接纳与清理受影响闭包限定回归；完整仓由173运行。实际结果与失败单独留证，不预记 Pass。
