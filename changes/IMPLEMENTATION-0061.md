# 061 有限拥有 package 源码与独立 Analyze

2026-10-24（Asia/Shanghai）。源码基线 `6acda08c72dbd3e387893b4260770f0834dcdbe0`；共享接口提交 `6322b9c27b71f7f15019647e555268385a999d6a`。本提案在本切片消费者代码修改前写入。语言依据 `_zil_design.md` §§2、7、8、15.5；精确公共表示遵循 `changes/IMPLEMENTATION-0060.md`。

## 施工范围与固定边界

- 接通已有 `package`、`manages` 语法到 `Type::Package`。至少一个资源责任字段，其他字段为有限 Copy；只支持 Copy 字段默认值。包整体属于普通非 Copy Value，参数、结果为 Infer，普通读取用于整体移动，不写 give、不自动 Desc 化。
- 构造的显式字段按书写顺序、Copy 默认值按声明顺序求值一次。现有 Sequence 的真实作用域局部守护每项成功准备；随后无失败 Take + MakePackage 按声明序提交完整包。提交前失败按独立准备局部的逆成功初始化序清理；提交后 DropPackage 按声明字段序清理。不存在公开的部分 package 成品。
- Copy 字段用 ReadPackageField；资源字段用 DescribePackageField，投影保留包局部和字段索引。投影不先移动整个包。活跃字段 Desc、别名和已捕获 Desc 阻止整包移动、覆盖、清理和存储结束。
- 独立 Analyze 保留完整 carrier 生命周期并记录精确字段来源；MakePackage 消费每个资源 SSA 一次。所有 package 路径遵守显式根/词法 scope 与 StorageLive/StorageDead，守护临时和错误分支均实际记账。Verify 由另一作者重新核验，不使用 Analyze 的求证实现。
- 零 manages、非资源 manages、包内 Desc/Address/Closure、嵌套 package、直接 package 捕获、字段 give/赋值/spread、replace/decompose、自定义包 Drop、拥有错误载荷与间接 package 签名均明确拒绝。已有包字段 Desc 的本地闭包捕获继续组合；不开放一般逃逸。

## 预先确定的验证

真实源码正例覆盖 create/give 字段、乱序书写与固定销毁顺序、Copy 默认值、普通移动/参数/返回/分支结果、临时投影、字段 Desc 写入及捕获、循环各类出口、构造早中晚失败和较晚参数错误。最近反例覆盖重复责任、移后使用、存活描述跨移动、隐私、字段类型与未开放范围。原始 IR 变异仅作为独立 Analyze 诊断与预算反例，不能替代源码贯通。

作者只运行独立低 debug target 的限定 crate/source 测试，记录实际命令、退出码、失败修订及精确 SHA；062 的独立 Verify 和 060 的 native/Rust 对照分别记录，正式全门仅由 root 执行。公共 schema/生产写集不越界。

## 预算回归修订（先于修复）

首轮限定旧 Analyze 回归发现，scope admission 新增了一个与 preflight 重复的整函数类型扫描，改变了三个旧非 package 原始 IR 用例的精确预算。修复在已经逐项收费的函数结果、局部和值类型 preflight 中同时记录该函数是否包含 Package，顺序保存每函数一个布尔标记并传给 scope checker；不再新增类型遍历。每个标记在对应函数 preflight 费用预付后写入，数量受 max_functions 约束；既有 scope 指令扫描继续识别 package 操作。保留旧测试的预算值和全部断言，另由真实 package 源码固定检查 N−1/N/N+1 的失败边界，不放宽上限。

## Rust 四目标

本切片增加拥有组合及失败清理的源码能力和安全证据。运行/内存成本、表达维护成本、完整编译成本尚无新比较结果；不以测试数量或语法简短宣称整体同级或净突破。ZUI 平台、GPU、异步与取消闭环不由本切片完成。
