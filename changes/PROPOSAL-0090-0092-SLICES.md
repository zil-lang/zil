# 090–092：有限借入切片作为 ZIL 通用语言能力

2026-10-25（Asia/Shanghai）。父任务 B0；依据用户最新指示，本轮以语言为主体，ZUI 仅作为发现缺口的消费者。现有 §15.2 已有切片语法，但编译器尚无真实 Slice 表示。此提案先于实现提交，内部 schema/raw wire 升至16，不改变完整语言与产品验收门。

## 接受范围与语义

`&array[lo..hi]` / `&mut array[lo..hi]` 从实际固定数组借入窗口；已有切片也可再切片或通过 `&*s` / `&mut *s` 整体重借。对象、显式 lo、显式 hi 从左至右各求值一次，缺省端点为0/当前长度。先检查 `0 <= lo <= hi <= len`，随后才计算偏移和长度；空区间合法，空区间索引依然越界。索引沿用当前整数类型及 checked trap 规则。trap 是进程终止，不增加 unwind 清理承诺。

元素须是现有可表示的有限 Copy 值；裸无尺寸 `[T]` 不能成为按值对象，嵌套地址、资源和拥有元素仍关闭。只有一层 Address 的内层可为 Slice；该 Address 不是普通 Copy aggregate 字段。共享窗口复制、可变窗口转交/重借使用已有地址规则。来源保守覆盖整个后备对象，当前不证明不同区间互不相交。Unit、空 tuple 等零尺寸元素保持现有机器布局规则，不能将 void 当 GEP 元素类型。

普通函数和跨模块函数可接收、返回切片；返回只可由已有参数来源与活性规则建立，不能返回局部数组的窗口。`.len()` 仅在接收者被解析为切片地址且零实参时为内建操作，返回 i64；不新增全局保留字，不把其它接收者的同名调用改义。长度读取也要求窗口活性与合法访问资格，不提供绕过暂停借用的元数据通道。先定位基底再求端点，端点不得通过相交写破坏该窗口。

本片限已支持的 x64 little-endian checked LLVM Linux/Win64 profiles；不开放切片的 CT 执行、FFI 胖指针 ABI、闭包捕获或显式 callable/helper 参数结果、聚合/错误载荷保存、动态分配/扩容、原始指针制造、OS 留存、隐式 RC 或对象全局表。它是未来容器和字符串的借入接口前置，不等于这些库或分配生命周期已完成。

## 公共接口与机器表示

- `Type::Slice { element: Box<Type> }`：仅 `Address { mutable, inner: Slice }` 位置合法；`Slice::is_copy_value()` 必须为 false。
- `CoreExprKind::Slice { object, start: Option<Box<CoreExpr>>, end: Option<Box<CoreExpr>> }`：仍为未核源码表达式；所有遍历计入孩子与深度。`.len()` 保留普通 Call/Member 形状至类型阶段识别。
- `AddressSlice { address, start: Option<ValueId>, end: Option<ValueId> }`：源为固定 Array 或 Slice 的地址，结果为元素和 mutable 完全相同的 Slice 地址。只投影已有资格；与 AddressField/Index 一样不复制独占资格。显式 `ReborrowAddress` 才产生 parent/child。最多三个 operand 均参与支配、活性与额度遍历。
- `SliceLength { address }`：源必须是 Slice 地址，结果必须是 i64。
- `AddressIndex`：支持 Array 与 Slice，产生元素的普通地址；本身仍为投影。
- wire 类型 tag23、操作 tag61/62，所有可选端点有规范布尔/ValueId 编码；Typed 指纹域 V16。旧 wire/schema0–15与未知新版本拒绝。
- LLVM 切片地址为 `{ ptr, i64 }`，按值传递该窗口，不分配；普通指向元素/数组的 Address 仍是 ptr。源数组/子切片负责实际存储活性，传递胖指针不授予额外权限。

Types/Analyze 和 Verify 分别建立其来源、活性及 loan 事实。Verify 不能调用前两者的证明函数；lower/backend 继续绑定不可变 VerifiedModule。源码布局、raw 类型、预算或 provenance 的手工声明都不是可信凭证。

## 分工与可独立验证的边界

090：Expand、Types、Analyze、Core/CT 消费遍历与拒绝、真实源前端正反例。091：独立 Verify 的类型/CFG/operand/来源/loan/预算与恶意 raw/wire 近邻。092：root 持有公共 schema/wire、Lower/LLVM、源到原生语义消费者及 O0/O2；跨片审核由非作者绑定精确提交，root 独占最终集成全量。

至少核对不同数组长度共用同一个普通函数、动态子范围修改、多层切片、空区间、共享和可变参数来源返回、最后使用后恢复基底访问；拒绝共享写、重叠独占、活动 child 期间父访问、移动后使用、局部逃逸、错误元素/权限、直接加载 Slice、FFI/CT/闭包藏入窗口。运行反例覆盖负数、反向范围、越界、索引等于长度与端点副作用顺序，O0/O2 必须执行实际机器产物。

编译图089仍固定 schema15 强基线；其研究、计数和计时不贴为16。语言新增能力不被旧研究结果授予性能结论。
