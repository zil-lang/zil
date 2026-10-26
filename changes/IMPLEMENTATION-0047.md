# 047 有限资源前端与逐出口清理

按规范第2/3/6/7/8章和049冻结接口贯通有限Copy payload资源。普通资源表达式产生Desc，give只消费Managed；result(create/give)进入Core和Typed。内置Drop实现绑定hidden teardown，普通调用不授予清理资格。

局部按实际声明成功序逆序清理，普通块、return、break、continue均显式DropManaged再StorageDead；工厂临时值在完整表达式末清理。条件转交依赖真实live flag，不把静态可能初始化误当运行时责任。

首片拒绝Desc返回/捕获/聚合、嵌套资源字段、资源替换、Address<Resource>和一般错误展开；受测源例与最近拒绝反例同批提交。类型/来源/清理仍为未核输入，独立Verify不依赖本实现的求证结论。

## 实跑与修复

`crates/zil-driver/tests/impl047.rs` 从真实源码经过CT、Types、Analyze、独立Verify和LLVM，以O0/O2观察C ABI `putchar`清理轨迹。覆盖普通退出、early return、continue、break、条件give、if/while/match条件临时值、短路、条件资源结果、管道、range端点、跨模块私有资源和默认字段清理。源例 `main.zil` 输出 `CB`，`lifecycle.zil` 输出 `BACDFAGHJJKL`，exit均为0。

新局部承接Managed结果前先结束其初始化表达式的临时责任；完成结果暂持于SSA，再Store到局部，避免把较老临时值放在较新局部之后清理。嵌套if表达式只结束自己的条件临时值，保留调用中先前实参的资源到完整调用结束。管道内部保存的既有Managed值可以继续转交；普通用户绑定不会因此隐式give。

跨模块接口真实保留result_mode，signature指纹有独立模式字节和Managed/Desc结构编码。root的CT集成修复6f8810a保持这些模式不被重建覆盖；047不修改CT实现。

最近反例证明：旧Desc在give之后使用、incoming Managed伪装result(create)，均能经过Analyze而被独立Verify拒绝。无效Drop签名、给出描述而非载体、缺give、读已移出局部在相应前段拒绝。资源与需要闭包推导的同一函数组合仍Unsupported；资源捕获/返回/嵌套资源尚未实现，不把这轮有限首片宣称为全语言。

独审补遗：资源构造的卫生Sequence曾将Managed外壳直接交给记录字段查找，丢失字段期望类型，导致`resource R{x:i64}; R{x:1}`和`[i64;2]`字段的`[3,4]`误推为i32并拒绝。修复仅在生成字段提示时解开一层Managed payload；不改数值后缀或转换规则。新增第14/15项真实源回归分别观察i64值（含5000000000）与i64数组的读回和Drop，O0/O2输出`BA`/`C`且exit 0；原13项收据仍保留原受测hash，增量收据另记失败到修复。

## Rust目标核对

安全目标有实际进展：描述与唯一责任分开，正常出口准确清理，且不依赖Rust借用检查器授信；一般错误展开、资源闭包、并发与完整安全证明仍未完成。运行与内存仅证实这组程序内联资源头、无隐式malloc/RC，未做同语义Rust性能配对，不能宣称更快。表达层已把工厂、别名读写和局部give组成跨模块程序，维护优势尚未证明。编译成本未做配对测量，本轮保持有限工作预算。至少一项相对Rust的实质突破仍未证明；root的资源Rust对照属于整合证据，不由047测试数代替。
