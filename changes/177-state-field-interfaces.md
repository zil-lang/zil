# 177：长期状态字段与动态共享元素窗口的公共接口

## 为什么

173 已能在局部组合固定拥有字段、动态 Store 和保存回调，但普通长期 State 尚不能同时持有 Store 与具体回调，动态元素也不能在不移出的情况下读取。本批按 `_zil_design.md` 的 ZIL-MODES、ZIL-ACCESS、ZIL-PACKAGE、ZIL-CAPTURE 规则补齐有限纵切；不改变责任不可复制、真实地址窗口、准备后提交或固定清理顺序。

## 公共表示

发布 schema / raw Wire 21，typed 指纹域 `ZIL-TYPED-MODULE-V21`。现有类型标签与 opcode 0–85 的载荷格式保持；旧 Wire/缓存明确拒绝，不把原20证据改标为21。新增：

- 86 `OwnedStoreBorrow { store: ValueId, index: ValueId }`：i64 索引，结果为 shared `Address<Package Item>`。源入口 `store_at(store, index)`；按实际 len 检查负值/越界并 trap，返回实际初始化前缀中元素的位置，不读取/复制整元素。
- 87 `BorrowPackageField { local: LocalId, index: u32, mutable: bool }`：只用于实际 Package 中已初始化的自含具体 Closure 字段，结果为相应 `Address<ClosureId>`；真实字段位与调用能力由 Types/Analyze/独立 Verify 检查。

`Type::is_shared_package_address` 只识别结构形状，加入现有 scalar-or-address 分类；不扩大 `is_address_pointee`。名义元素定义、来源、贷款、初始化和许可均需独立核验。结构分类、Wire 解码和源属性不产生安全权限。

## 三片合同及最近反例

174 只扩大 Package 直接 `manages Store<Item>` 字段准入，沿用 Describe/Take/Store/DropPackageField。Store 元素保持172有限形状；完整父壳普通参数、返回和错误出口须保持全部责任。替换先完成新值准备；失败清理实际获得的临时值。清理必须命中 Store 专用前缀 drop，不能把密封定义的 `teardown=None` 当作无需清理。拒绝缺字段整传、重复give、旧字段Desc复活、Store捕获与嵌套Store元素。

175 的共享元素窗只开放有限 Item 的 Copy 字段投影/读取及同步普通参数。活动窗及其派生 Copy 地址保护实际容器来源，阻止相交 reserve/pop/push/give/drop、174字段替换和父壳移动；不同参数名不证明分离。首片禁止可变视图、整Package/Managed读取、直接视图捕获与返回；函数内出现该窗时地址结果的保守拒绝只作用于相关函数，保留无关旧合法函数。索引检查不替代静态贷款。禁止把动态容量展开成编译器槽。最近反例含未知参数别名、跨helper写效果、越界、扩容后旧窗、字段/父壳移动与当前调用捕获的活动子地址。

176 使用既有泛型 Package 与 `F: Fn/FnMut/FnOnce`，不增加字段位置 `impl Fn` 语法。字段保留实际 ClosureId 和 source opaque identity；普通不可Copy字段移动、整取/补回/替换及递归清理均沿现有责任规则。借入调用必须访问原环境，FnMut 多次调用不能加载快照；FnOnce消费结算未给出的捕获。普通State<F>参数/返回需要完整具体化与独立核验，不能仅删除旧helper限制。拒绝不同opaque工厂混赋、已消费FnOnce重复调用、借入环境逃逸、缺字段整传及未支持的capture组合。

## 生产者、消费者和迁移

177 单写公共 IR/Wire/指纹/版本及保留名；174、175、176 分别持有 Store 字段叶、Store 元素窗、Closure 字段叶语义，独立 worktree 实施后顺序集成相交分类。公开接口初始阶段对尚未实现的新操作精确返回 Unsupported，不能先放行 raw 输入。完整语义由每片 Types、Analyze、Verify、Lower 和 LLVM 实现与原生消费者证明。

所有新增 operand/local/metadata 都进入现有确定预算和身份；默认额度、产品门和性能规则不变。元数据路径、raw/wire变造、精确N/N−1、实际Drop及分配失败由定向测试和非作者审核覆盖。旧Store字段原反例在174生效后迁移其接受性断言，原源码及先前失败保留。整仓与产品状态分别报告，本提案不宣称阶段已经验收。
