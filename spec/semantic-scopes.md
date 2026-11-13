# 原始语义作用域与实际程序

本合同规定原始泛型、生命周期和语义捕获的完整表示。运输记录是不可信程序操作数，不是来源、能力或安全证明。Source、Types、Analyze、Verify 和 CT 消费者分别承担自己的完整检查；字段存在或能往返编码不代表检查通过。

## 原始定义与实例

定义保留跨模块限定身份、真实词法父链、原始混排的 Type/Const/Life 形参、隐式生命周期的原始发生位置、const 的完整载荷类型、全部 where 条件、语义自由变量和原始签名或名义类型体。未使用形参、phantom 参数和非活动枚举分支不能删除。

实例指向真实 Core 函数、Core 闭包发生点、Typed 函数或词法作用域。原始形参序号与内在类序号不同；完整类型已有的内在类不能因实际生命周期图像相等而合并。缺失语义目录是 Unknown，不能解释为零类。语义自由变量也不能伪装成物理捕获字段。

每个实际 Type/Const 特化保留真实 rigid 代表程序。原始生命周期在该代表中保持独立；具体生命周期分组必须指向同一特化的真实代表，并独立证明完整 CFG、操作数、类型、资源、闭包和清理的替换对应。不能只比较 ABI。`bad<a,b>(Marker<a>)->Marker<b>` 必须在 rigid 检查失败，即使所有当前调用者碰巧选择 `a=b`。

闭包的 `SemanticEnvironmentFormal.source` 在真实词法 parent/creator 的定义域解释；`SemanticEnvironmentInstance.actuals` 在实际 parent/creator 实例的真实闭包创建站点求值，再封闭为 child 的 ghost 实参。这两处的 `Ancestor(0)` 指该 parent 当前作用域。creator 的 Value/Local 序号不能转而选择 child 的同号操作数。必须核对实际闭包、创建指令和 parent 对应，多次创建逐实际对应检查，不能选首个调用者提供证明。环境绑定不回写或替换实例原有的 rigid Life 实参。

## Core 到 Typed 的完整求值

Source 不得猜测 Types 尚未分配的 ClosureId、完整类型内在域、const 载荷或常量位模式。`SemanticTypeValue::Core` 保存精确词法实例和完整原始类型 recipe；推导类型还绑定真实 module/function/expression/occurrence。`SemanticActual::CoreRecipe` 保存待代入的完整 Type/Const/Life 实参表达式。无类型字面量和关联 const 必须先得到真实载荷类型再求值。

该延后表示统一适用于 Type 实参、Const 的载荷类型、实际 Self、构造结果、类型使用和局部声明，并覆盖函数与闭包环境中这些记录的每次出现。实际存在但尚未求值的 Self 不能写成 None，未知完整域不能写成空类表。Self recipe 不能以自身作为解析基准，必须由真实 method/receiver、impl 作用域和父实例实参确定。

Types 消费真实 Core 发射账本，以原表达式身份和发生点解析推导类型，核对 recipe，产生完整 `Resolved { ty, life, classes }`，并重建完整实际 header embedding。Core 阶段尚未发射的空 embedding 不代表 Known0。Types 输出 Typed 前必须解析所有延后记录，连同未使用定义、词法作用域、环境和非活动分支；Analyze、Verify 及可执行 CT 消费者拒绝任何残留 Core 类型、CoreRecipe、Core 目标或未解析语法。

## 实际语义站点

原始站点保留原语法发生路径和作用域。Core 站点绑定真实表达式或声明的发射角色；Typed 站点绑定真实 block/instruction 和前后边界。构造、调用、类型使用、局部声明和闭包环境必须核对实际操作码、SSA、实参和结果，不能用同形状的其他值替代。

局部声明建立之后写入必须满足的类约束，不能读取尚未初始化的 occupant。间接 callable 调用保留原始 callee 类型 recipe，并绑定实际 callee 操作数；抽象 F 不需要也不允许虚构函数或闭包定义。按真实操作码选择调用点 local 的实际 occupant、借入 Address 所指的真实闭包目录或函数指针 SSA 值，不能取同型的另一 local 或同签名的另一 entry。实际 callable 类型、完整签名、where 前提、调用种类、entry 和资源行为仍分别检查。Dyn 成员派发继续要求完整实际 receiver、interface/member 和独立 FullW 证明。

## Dyn 与完整泛型

Dyn 的 Semantic 类型桥使用完整类型 recipe，不另建一个缺少 Closure、Opaque、Task 或关联类型的语法。桥从当前实际 Dyn owner 查找唯一原始语义作用域，核对完整身份、父链、原形参和成员序号。解释器在当前真实 W、Self、声明和方法实参中求值，保留连续 Bound/Invocation 层次，使用共享预算和循环检测。先出现的调用者、相同 Context 图像或无关的字面量不能提供证明。

外层 Address/Desc 的 alpha 对应只重命名完整外层发生域，保留路径、模式、名义声明与原始形参序号；不能改变子类型内在类、BS/Opaque 域或 header binder。两个内在类映射到同一图像仍是两个类。callable 的 Fixed 关系仅可比较同一真实 P/R 外层发生在同一 W 词法域中的合法图像，不能扩散成 TypeEqual、名义类型合并或 Invocation/Bound 实例化。

## 运输与资源

Runtime schema/Wire 和 Typed 身份域为 49，CT artifact 与依赖身份域为 9。完整原始记录进入 Wire、指纹、遍历、CT 类型 DAG、克隆和重定位；不支持的可执行消费者明确拒绝，不得抹掉 Some 目录继续处理。旧版本和未知版本拒绝，历史固定字节保留。

不可信输入在分配、克隆、哈希和缓存前支付完整节点、字节、深度与工作费用。冷、热、失败重试和重新请求均遵守实际请求预算；结构运输与语义解释分别收费，不能把运输费用测试当成 Analyze/Verify 语义预算证明。
