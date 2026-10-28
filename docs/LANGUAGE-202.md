# ZIL 1.4 / Ante-R1：202 完整普通 T 模板检查点

2026-10-28。202 已完成普通 `T: StoreElement` 模板的完整类型绑定、非作者审核及整仓回归：**2319 通过、0 失败、0 忽略，241 个结果组**。实际受测 clean 提交为 `b73ba8a764ee605c1ba8e3a651eb2c6baece8197`，命令为 `cargo test --workspace --offline --locked -j2 --no-fail-fast`，实际耗时148.91秒。这是功能回归运行，不是编译速度比较。

设计仍为用户指定的 **ZIL-LANG-1.4 / Ante-R1**。设计摘要 `bb8821ef1c88f2753d12dd668fef0638da6dc849f541c55767051389f6387aa3`，规范集合摘要 `10bd65c22970a3479f436ceab8baf5f58aa014ff94f780ec0896e160fd5c4bb5`。公共 Core/Wire 身份为26；早期运行保留当时摘要。完整1.4仍 **NotSatisfied**，B0 **Doing**，产品、完整draft/product流水线及原生Windows **NotRun**。编译速度支线没有合入或重标成绩。

## 本片已经贯通的能力

原普通泛型函数的实际 T 若依赖 caller 的 callable F，或依赖保留模板自身的 T，Expand 会保留定义和静态 TypeApply。Types 以真实定义身份和完整实际 T 建实例，完整名义族、phantom参数、字段、长度、错误联合及实际ClosureId均参与身份。普通模板未实例化时没有具体FunctionId，不能当函数值或指针调用。原完全具体eager路径保持。

| 能力 | 当前行为 |
|---|---|
| 普通完整T函数 | 六个普通empty/reserve/push/pop/len/capacity可以跨模块保留真实函数边界，支持明确或唯一推得的实际T |
| 词法作用域 | caller F/T 与callee T分别解析；全局名义T不会吞掉局部callable T；不同模块/同布局类型不会合并 |
| 定义与实例 | 每个未调用raw普通模板仍按自己的opaque T检查，不能借别的定义的T、字段、数值或调用能力；实际值参数和结果再次核对 |
| 混合计划 | callable、普通T和子T依赖有界处理；递归/增长或未完成实例明确拒绝；错误和成功载荷分开，问号只取成功侧 |
| 实际资格 | 每个实例T产生canonical OwnedStore资格定义，即使identity没有Store操作，Analyze/Verify仍独立核真实捕获和非零布局 |
| 效果与窗口 | 描述符写效果沿真实参数位置传播；reserve/push/pop及父Store失效不能穿过活动窗口 |
| CT和IDE | runtime CT正规化保留元数据和完整实参；执行新模板/TypeApply在CT中拒绝；IDE与CLI共享判定并正确失效/恢复 |

资格定义不会创建运行时Store值或清理债务，也不授予权限；它可能生成未调用的helper代码，不宣称机器码大小不变。最终仍使用既有Typed指令、布局和实际借入/消费/清理核，没有新造TypedIR opcode。

第一片只开放一个普通StoreElement binder的自由函数；混合ordinary/callable/const参数、receiver/foreign/where、opaque callable返回、递归拥有和新捕获路径仍按未实现边界拒绝。StoreElement不授予Copy或任意字段访问。

## 实际验证与身份

| 范围 | 结果 |
|---|---|
| Types作者最终干净ba4 | driver14/0（source8、raw4、budget2），22次O0/O2 native；Types库18/0 |
| Expand最终限定 | 17/0，含原source、词法遮蔽、返回上下文与真实预算边界 |
| 非作者实际988da100 | 26/0（19语言组、7CT组），44次源码编译中34次实际native；6次Expand拒绝与4次Verify拒绝分别核阶段/原因 |
| Linux五模块消费者 | 作者和非作者各自126/0：每组16普通、110探针，86次真实分配失败，终态live0 |
| Win64/Wine五模块消费者 | 126/0，86次真实分配失败，live0、bad_operations0；原生Windows未运行 |
| root最终 | b73干净整仓2319/0/0；769同生产候选全workspace/all-target Clippy通过 |

这些集合存在重放和共同输入，不能相加成独立覆盖数量。非作者对实际988、CLI769和接受候选b73的相应生产/审核输入逐字核对；b73的正式整仓由root实际执行，没有倒改非作者运行身份。作者早期dirty运行的差异和局限按各自收据保留，最终干净运行单列。

五模块消费者保留两个独立环境，覆盖零次/多次ABBA、continue/break、扩容、pop后的拥有调用、满容量push完整错误载荷清理，以及每一个实际分配点的失败和最后一次之后的控制。期望来自冻结源码、64字节完整元素布局与独立序列，没有按被测LLVM或观察结果回填。

## 发现并修复的实际问题

全局名义T与局部callable T冲突，在原AST仍有词法信息时做有限卫生保护，使用同一剩余Budget。raw未调用模板曾漏掉跨定义T和错误TypeApply目标，现每定义独立检查opaque binder及真实静态目标。普通pop返回的错误联合曾丢失成功载荷计划，现在保留双侧形状，显式注解/局部转交也保持。外层整个错误联合曾误约束内部具体payload调用，现在保留正确的callee结果约束和最终返回检查。

原失败源全部保留。旧201的两条原helper探针仅晋升同字节源码的成功期望；CT fixture和两处旧测试Clippy整改只改变机械写法，非作者确认没有减少断言或改变输入。消费者还保留执行前编译器身份拒绝、旧Types失败、首次遗漏LLVM动态库路径的InfraError；同源环境修复重跑单独登记。

旧若干输入的精确额度保持：Expand1731/2255；作者三旧Types输入4241/11851/11792。新独立完整T输入四阶段N为10837/46169/77259/174758，无Store指令的资格输入Types/Analyze/Verify为6148/10759/13500，分别验证N成功、N−1 ResourceLimit、fresh N恢复。各数字绑定其实际源和提交，不能互换或推成全部旧输入成本不变；默认额度和产品阈值未提高。

历史handoff完整性检查仍报告旧记录/对象问题，没有178–202切片错误。本片不重写这些历史事实，也不把计划检查等同于语言或产品验收。

## 下一阶段与终态义务

203继续不同F/G的持久State、同一实际ClosureId下仍不同的声明调用模式，以及最多两种具体callback的有限Store资格。一个State参数必须只求值、转交一次，不能伪造两个直接callback角色；模式要随源字段和完整T转交保留。泛型定义继续按声明约束检查。

一般trait/关联项/life/HRTB/lending/dyn、卫生Syntax/Type CT、更多拥有组合、长期外部回调、挂起/异常/取消/并发以及产品量化义务继续保留。此检查点没有宣称ZUI全部前置或完整1.4完成。

## 交付与Rust目标

完整主线以标准Git bundle交付，可供用户既有一键脚本本地fetch；新执行物、target缓存和大型证据不入Git，原历史中已跟踪内容随祖先保留。独立证据集合为28.63MB，包含七个原组件ZIP并逐字保留各自哈希。

安全近邻、完整责任和失败清理已在本片真实闭合；完整语言安全仍未完成。六个普通helper提高了库组合能力，但维护成本尚未做产品测量。没有新增隐式Box/RC/锁；行为和分配平衡不代替同语义Rust的时间/峰值内存/尾延迟比较。编译成本专项独立，本片不声明相对Rust性能突破。详见[集成收据](../evidence/integration/202/final/RESULT.json)和[Rust目标](RUST-GOALS.md)。
