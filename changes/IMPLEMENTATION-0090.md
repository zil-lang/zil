# 090：有限借入切片的源面与分析消费者

父任务 B0；公共契约为 `PROPOSAL-0090-0092-SLICES.md`（schema/wire16）。
本说明先于实现提交；接受性、预算、错误及 ABI 只依当前规范。

## 实施范围

从现有范围语法构造 `CoreExprKind::Slice`，将 `&[T]` / `&mut [T]`
解析为一层 Address/Slice。Types 定位真实基底，按对象、lo、hi 的源码顺序
求值，再产生不新增权限的 AddressSlice 与显式 ReborrowAddress。
`.len()` 仅对切片地址接收者选择内建 i64 长度操作，保留普通同名方法。
固定数组及切片索引沿用来源和整基底借用模型；返回沿用现有参数来源合同。

更新所有 Core/type 遍历、跨模块身份、CT collector/preflight/materialize
节点与深度收费、Analyze 的操作数与 Hold 传播。不开放 CT 执行、外部胖指针
ABI、闭包捕获/显式 callable/helper 切片接口、拥有元素或无尺寸按值存储。
不修改 Verify、Lower、LLVM 或公共 IR，不使用前端结果作为独立核的证明。

闭包限制在边界：捕获切片地址、显式 callable/helper 切片参数与结果不开放；
闭包正文可以从本地数组（包括合法 Copy 数组捕获环境）构造并消费局部窗口，
只返回 Copy 结果。可变捕获的写效果遍历范围基底，不能误判成只读闭包。

未使用 alias/template 的固定非法元素形状也不能随消除而消失：沿既有
类型预算遍历保留 Slice 元素上下文，拒绝其中的 Reference 和嵌套无尺寸
Array；alias 替换后、丢弃前再核其完整类型树。Named、泛型实参和未知 T
仍由实际解析/实例化决定，不增加一套 Copy 能力求解器或默认额度。

## 验证计划与事实边界

限定检查覆盖真实源的类型/Analyze 正例和最近反例、普通同名 len 方法、
动态范围及省略端点、再切片、参数返回、空区间、Copy 聚合元素和零尺寸元素。
CT 与闭包拒绝必须穿过真实源与对应公共入口；raw 预算近邻计入全部新节点。
与 091/092 的实际依赖合流后才做 source→Verify/LLVM/native 对照。
所有失败、修复、精确提交、命令和执行结果按实际范围另存 evidence/out，
不得把限定前端通过表述为机器或完整语言验收。

## 有限验证记录

依赖合流使用 root 的 09fba1fa（091 初始核、092 后端）及 091 的 9983
元数据读取资格修正。2026-10-25，最终代码 f19512a0 的 `impl090_slices`
13 项全部通过真实 source→Types→Analyze→Verify，含显式 helper 边界、
端点相交写和借捕获/移动捕获 Copy 数组的局部窗口。CT materialize 3 项
通过，其中新用例分别令 Slice 的 object、lo、hi 达到 NodeId 上界，确认
身份扫描没有丢失任何子节点。形成检查的 3 项与旧 generic 2 项在前一提交
f049737e 通过；两提交之间仅修正普通反例的既有 Types 阶段预期，未改产品。
受影响 closure 前端 22 项、CT collector 6 项、CT preflight 12 项也通过，
合计 61 项限定测试函数；五个受影响 crate 的库 clippy、所有本单 Rust 文件
格式与 diff 检查通过。具体命令、输入摘要与全部失败沿革在本单 evidence。

普通 receiver 方法与闭包在同一函数的组合存在原有 source-closure 推断缺口：
没有 Slice 的 `q.len()+i64(f())` 也报 `E-UNKNOWN-FIELD`，baseline
`inference.rs` 同一路径只做字段投影。此次分别验证普通同名方法与 Slice
并存、普通闭包与 Slice 并存，不声称已修复这个独立组合缺口。

## 工程价值与未作出的结论

同一个普通函数现在可借入任意有限后备数组的窗口，动态长度、索引与可变
子范围不再要求把固定长度写入函数类型。它可供序列/文本/对象状态代码
复用，未引入框架接口、拥有容器或隐式分配。安全检查继续由独立 Verify
负责；机器 ABI 与运行时范围陷阱归 092 的生产源原生证据负责。本单没有
编译速度比较、Rust 产品对照或完整语言完成声明，图算法研究证据另立。
