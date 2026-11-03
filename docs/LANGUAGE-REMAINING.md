# 完整语言还缺什么

2026-11-03按用户十方面重核，源码基线为已合入main的`40fbcca6ef506f3cb8e1d40a4f8995c788627da0`。当前结论见[334精简结果](validation/language-gaps-334.json)：52份真实源码检查为22 Success、18 Unsupported、12 SafeReject，9份正例经LLVM API O0/O2完成18次原生构建/执行，均无fallback。它们是有限能力观察，不是完整语言覆盖率。

复用了316的原25份源，字节全部相同，当前为8 Success、7 Unsupported、10 SafeReject。life参数返回、具体泛型trait实例已成功；原三份未声明life源现以`E-LIFE-DECLARATION`拒绝，旧“标注被擦除”的结果只保留为[历史事实](validation/recovery-capabilities.json)。`pending`是历史目录名，不能据此判定当前缺口，也不能把合法目标的当前失败永久注册为应拒。

本轮release CLI的实际SHA256与[原构建/消费者收据](validation/mainline-consumer208-20261103.json)一致；其观察源码`f2122be4`至本基线无编译器生产源码、Cargo或原生适配变化。上批共同编译器`022303df8`已有本地默认4097/0、reference3988/0，末批profile及19项补测见[主线有限验收](LANGUAGE-MAINLINE-20261103.md)。这些成绩分别记账；GitHub CI曾因账号支付/额度未启动，不据合入main声称CI通过。本页不复述全部旧阶段成绩。

## 十方面当前矩阵

| 方面 | 当前已支持的有限范围 | 仍缺的完整范围与证据 | 消费者与承接 |
| --- | --- | --- | --- |
| 1. 基础类型与模式 | 普通枚举、穷尽match、有限Copy tuple普通/泛型绑定已有；`let (left,_)=(3,4)`此次已到O0/O2原生，不能把所有部分解构都列为缺失。 | `T?`、record式enum variant、`let ref`目前在Types返回Unsupported；`usize/u64`显式参数/返回链仍为未支持的Named类型。更一般非Copy/借入模式未闭合。 | 消息/节点数据与索引接口。329正施工有限Copy记录式变体；Option、整数链和其余模式另有剩余责任。 |
| 2. 泛型与trait | Copy/StoreElement/F/State等既有有限泛型、静态名义trait及323的具体泛型trait实例已有；泛型trait声明与`Read<i32>`实际方法调用均通过，后者此次原生执行。322的life-only binder亦已支持。 | const参数、普通类型与life混合参数、泛型/条件impl、关联类型/常量与投影、supertrait、默认/泛型方法仍未完整支持；原关联项、泛型impl、默认体和const源仍失败。 | 普通组件协议。330施工有限默认方法，331施工`T:Copy + life`；其它trait/const范围没有随此完成。 |
| 3. 借用与拥有 | 普通完整Product局部`&mut`同步helper已由315贯通，本次原源再次到O0/O2；共享/独占窗口、Store元素、有限lending和真实来源推导已有。322支持普通Direct及具体inherent方法的共享life输入/结果上界；未标注多输入返回仍按真实正文来源推导。 | mixed T/life、可变life、HRTB/借用函数指针、借入存储、一般lending/拥有组合仍缺；新`type-life-copy.zil`在Expand未支持，原FP/HRTB源仍未支持。局部逃逸与相交窗口仍由Verify拒绝。 | 共享模型视图、容器和回调API；331只承接有限混合参数，不缩减一般life终态。 |
| 4. Type/Syntax | 314/320已贯通普通Copy泛型返回Type和具体泛型纯记录参与CT计算；两份原pending源此次均通过Verify及O0/O2。已有卫生Syntax、生成、反射及锁定provider链。 | 这两个旧缺口应关闭。混合拥有/借入/动态/异步形状进入CT、一般语言组合及任意生成产物的完整覆盖不能由这两源推得；生成结果仍须普通两核核验。 | 普通配置与组件生成可复用现有能力，优先实际库消费者，不重复重写原两份源。 |
| 5. 动态抽象 | 具体Fn/FnMut/FnOnce、opaque拥有工厂、有限具体环境存储已有。 | 借入/拥有`dyn`、运行期异构调用及显式存储/管理route的完整生产链仍缺，`pending/dyn-trait.zil`在Types未支持。不能用隐式Box/RC补位。 | 异构handler/组件集合；需先冻结真实环境、调用、清理及ABI表示。 |
| 6. 一般状态协议 | 资源U/K/D、初始化/完整性、Store及有限callable/State合同已有；`state_bindings203/main.zil`此次真实多模块检查通过。 | 这不是§5.2的一般有限状态/角色/真实投影、逐结果转换、别名情形和导出上界完整生产闭环。当前无通用协议声明/求证链的完整消费者收据，本次不臆造语法探针冒充证明。 | 扩展有状态库时按真实协议推进，不能从`State`名字或布尔Ready字段制造权限。 |
| 7. 返回后的外部登记 | 静态无环境C回调、返回静态guard及同线程结构化scope已有；同步scope可承载长时间事件环。 | 安装函数返回后继续保留捕获环境的登记对象、明确稳定存储及关闭/晚通知责任仍缺。原捕获handler转`c_callback`源在Types以`E-FFI-CALLBACK`拒绝，不能删除此保护来宣称完成。 | 分离组件安装与后续外部通知；先有具体消费者和完整存储/关闭合同。 |
| 8. 挂起、取消与并发 | 325/328已有真实Linux x86_64 Direct结构化pthread：静态`fn(i32)->i32` worker、父线程FnOnce body、创建失败清理及成功join。life/trait/父借用组合此次原生成功；既有create/join失败机器收据见325/328阶段记录。 | Suspend/SABI、稳定帧、PendingExit、waker/晚通知、取消/排空、可脱离handle、子线程共享/借入载荷、一般Transfer/Share、Mutex/Atomic及弱内存论证仍缺。父闭包内部调用现为Unsupported，332仅施工此位置；pthread测试适配里使用mutex不等于ZIL支持Mutex。 | 保持同步事件环可用；332扩普通组合。异步I/O、锁/原子各依自己的公共责任合同，不从已支持thread_scope推导完成。 |
| 9. 标准库与ZUI | ByteBuffer、Store、UTF处理和真实I/O/FFI已有；普通同步组件和事件消费者可运行。 | 完整String、集合/迭代库、ZUI布局/IME/渲染尚未形成完整库/平台闭环。`std.io`此次为`E-STD-IMPORT`，当前仅供应普通`std.meta/std.syntax`导入；缺库不等于缺全部I/O能力。 | 先复用已核语言编写普通库和真实组件；区分库代码、编译器原语与平台责任。 |
| 10. 待定语义与验证 | 整数、当前内联布局及已有Win64/Wine链保留各自有限实证。324已修复原262144字节a18，两后端O0/O2实测成功。 | 浮点数值/转换/ABI及匿名union完整合同仍需定界；原生Windows与Rust同语义成本是未完成验证，不能简单当作编译器功能全缺。一般巨型按值ABI原源本片未执行，由333先真实复现再修；不得继续称原a18未修或预报by-value崩溃。 | 独立平台/同语义产品门；333施工一般内部按值ABI。没有可信完成百分比或Rust胜出结论。 |

规范依据为`_zil_design.md`：基础源面§15.2和`spec/grammar.ebnf`；trait/life§18.1；Type/Syntax§16；借用/动态§8–9；一般协议§5；外部/SABI§12；并发§11。规范与当前机器证据分开；规范未完成定界的项目不按其它语言习惯自行承诺。

## 当前施工与下一步

329记录式变体、330默认方法、331类型/life组合、332具体父闭包内线程、333一般大值ABI正按各工单施工。334冻结在40fb基线，不将它们的提案、WIP或未来测试计为通过；各自须原源、近邻、独立核验、必要预算和机器消费者闭合后另行晋升。

优先级与不相交消费者见[下一步](LANGUAGE-NEXT-PRIORITIES.md)。不用返回局部借用、消费后悬空、隐式装箱、重叠impl、CT泄出地址/权限等规范禁止行为填充缺口；普通递归已经可用。完整1.4、B0/ZUI和Rust安全/运行内存/维护/编译成本四目标仍未完成，本片没有新增产品比较，也没有宣称一般异步或线程安全。
