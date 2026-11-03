# 完整语言还缺什么

2026-11-04本轮共同编译器源码为`14832d5a35630b7002c79e4df47b0cbb007570b6`（B），最终完整受测C与其生产输入逐字节相同，默认4166/0、reference4057/0及双Clippy等门通过，见[验收记录](LANGUAGE-GAPS-20261104.md)。以实际新release CLI重新检查原52份源，得到27 Success、13 Unsupported、12 SafeReject；原25源现在为9/6/10，原正控和保护反例均无回退。五项晋升是记录式变体、两份默认方法源、Copy类型/life组合和具体父闭包内线程。精确差异见[334当前复核](validation/language-gaps-334-final.json)。

[原334的52源及18次native收据](validation/language-gaps-334.json)仍锁定历史40fb基线且字节不改；root以同一旧CLI重放时52项观察和18项机器结果逐项一致。旧89891e CLI、A探索候选及其全量通过均不代表B验收：A后续发现真实primitive Type binder被当作原生类型，未用定义误接受，B修补绑定身份并保留life命名空间和累计预算；这不是源码UB结论。

B release为`beebbe5900ab5e615a33c66e640950e8c71656fa24d9aee7f0c4a1c3b7d0fc73`，实际构建收据记录始末同一clean B源码。新增组合源保持2a8原字节，从View<T>借用经默认协议、记录消息到具体父闭包线程，Clang/API×O0/O2四组均原生打印0、exit0；314两份原CT泛型源另完成相同四配置共8次native。两组共12构建/执行分别计数，不累加历史18次。限定borrow29、generics15、CT37均通过；正式B全量、远端合入/CI及父任务裁决仍由root另行记账。

## 十方面当前矩阵

| 方面 | 当前已支持的有限范围 | 仍缺的完整范围与证据 | 消费者与承接 |
| --- | --- | --- | --- |
| 1. 基础类型与模式 | 普通枚举、穷尽match、有限Copy tuple普通/泛型绑定及通配部分解构已有；329补完整有限Copy记录式enum变体和显式具名match模式，原声明源及组合消息已实际运行。 | Option、`let ref`、更一般非Copy/借入模式及`usize/u64`完整原生链仍未实现；当前原源仍在Types未支持。记录式变体的字段名字、求值顺序和未用定义资格继续检查，不等于全部模式完成。 | 节点/集合的可选数据、长度索引及更一般解构。 |
| 2. 泛型与trait | 既有Copy/StoreElement/F/State、静态名义trait、具体泛型trait及life-only保留；330补有限默认方法，331补一个普通`T:Copy`与life的普通自由Direct函数，type/life次序任意且普通实参只计T。 | const、泛型/条件impl、关联类型/常量与投影、supertrait及泛型方法仍缺；默认体直接return全局函数名作为thin fnptr仍为KnownIncomplete（Expand unknown value），不是永久负控。已支持的fnptr参数forward不能据此说成缺失。 | 继续普通组件/迭代协议与关联投影；不称所有普通default body闭合。 |
| 3. 借用与拥有 | 普通完整Product局部`&mut`helper及322共享life返回上界保留；331有限Copy类型/life组合、跨callee实际参数映射和模块/CT/cache合同已复核，新的组合View<T>字段借用已原生成功。 | mutable life、HRTB/借用FP、借入存储及一般lending/拥有组合仍缺；必要来源经过未标注callee转发、循环改变外部引用来源仍暂未支持。局部逃逸和相交窗口仍独立拒绝。 | 更一般视图、容器与回调；不把Copy授予数值/字段权限，不把同拼写life当成同一对象。 |
| 4. Type/Syntax | 普通Copy泛型返回Type、具体泛型纯记录参与CT两旧缺口已关闭；B以原源Verify和Clang/API×O0/O2共8次native复核，37项既有CT源认证/Wire/名义/预算控制通过。 | 一般拥有/借入/动态/异步形状的CT组合和全部生成产物覆盖仍不能由两个成功源推出；Syntax产物始终进入普通核验。 | 普通配置与组件生成复用现有能力；旧pending路径和字节保留。 |
| 5. 动态抽象 | 具体Fn/FnMut/FnOnce、opaque拥有工厂、有限具体环境存储已有。 | 借入/拥有`dyn`、运行期异构调用及显式存储/管理route的完整生产链仍缺，`pending/dyn-trait.zil`在Types未支持。不能用隐式Box/RC补位。 | 异构handler/组件集合；需先冻结真实环境、调用、清理及ABI表示。 |
| 6. 一般状态协议 | 资源U/K/D、初始化/完整性、Store及有限callable/State合同已有；`state_bindings203/main.zil`此次真实多模块检查通过。 | 这不是§5.2的一般有限状态/角色/真实投影、逐结果转换、别名情形和导出上界完整生产闭环。当前无通用协议声明/求证链的完整消费者收据，本次不臆造语法探针冒充证明。 | 扩展有状态库时按真实协议推进，不能从`State`名字或布尔Ready字段制造权限。 |
| 7. 返回后的外部登记 | 静态无环境C回调、返回静态guard及同线程结构化scope已有；同步scope可承载长时间事件环。 | 安装函数返回后继续保留捕获环境的登记对象、明确稳定存储及关闭/晚通知责任仍缺。原捕获handler转`c_callback`源在Types以`E-FFI-CALLBACK`拒绝，不能删除此保护来宣称完成。 | 分离组件安装与后续外部通知；先有具体消费者和完整存储/关闭合同。 |
| 8. 挂起、取消与并发 | 真实Linux x86_64 Direct pthread、静态纯标量i32 worker、父线程FnOnce body、创建失败清理及join保留；332有限具体父闭包入口已支持，组合源父body读取借入视图且仅i32跨线程，四配置native成功。 | Suspend/SABI、稳定帧、PendingExit、waker/晚通知、取消排空、脱离handle、子共享/借入载荷、一般Transfer/Share、Mutex/Atomic及弱内存论证仍缺；未核callable/helper合同入口不随具体闭包开放。 | 同步事件环与已核线程组合可用；异步I/O、锁与原子分别持完整责任合同。 |
| 9. 标准库与ZUI | ByteBuffer、Store、UTF处理和真实I/O/FFI已有；普通同步组件和事件消费者可运行。 | 完整String、集合/迭代库、ZUI布局/IME/渲染尚未形成完整库/平台闭环。`std.io`此次为`E-STD-IMPORT`，当前仅供应普通`std.meta/std.syntax`导入；缺库不等于缺全部I/O能力。 | 先复用已核语言编写普通库和真实组件；区分库代码、编译器原语与平台责任。 |
| 10. 待定语义与验证 | 324原a18已修；333进一步贯通大型Array/Tuple/Record内部按值调用/返回和fnptr，并按完整signature/carrier依赖隔离`.lc1`机器符号。此前版本传播阻断和修补均保留。 | 一般大型Enum/ErrorUnion按值可靠性仍未验收；大型closure接口/环境和未贯通特殊helper等有明确Codegen边界，不能说所有大Copy均完成。浮点/匿名union合同、原生Windows与Rust同语义安全/运行内存/维护/编译成本仍未完成。 | 继续剩余真实ABI形状及平台/产品门，不给完成百分比或Rust胜出结论。 |

规范依据为`_zil_design.md`：基础源面§15.2和`spec/grammar.ebnf`；trait/life§18.1；Type/Syntax§16；借用/动态§8–9；一般协议§5；外部/SABI§12；并发§11。规范与当前机器证据分开；规范未完成定界的项目不按其它语言习惯自行承诺。

## 当前施工与下一步

329–333的本轮有限源码已进入B，原阻断、分支独审、共同有限复验与正式全量分列。334的历史40fb JSON不追改；当前矩阵以B实际投影和有限机器结果更新，不把新profile登记、A全量或旧CLI结果当成B通过。B正式全量及工单状态由root最后裁决。

优先级与不相交消费者见[下一步](LANGUAGE-NEXT-PRIORITIES.md)。不用返回局部借用、消费后悬空、隐式装箱、重叠impl、CT泄出地址/权限等规范禁止行为填充缺口；普通递归已经可用。完整1.4、B0/ZUI和Rust安全/运行内存/维护/编译成本四目标仍未完成，本片没有新增产品比较，也没有宣称一般异步或线程安全。
