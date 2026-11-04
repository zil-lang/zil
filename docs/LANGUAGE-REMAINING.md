# 完整语言还缺什么

## 当前终态施工候选（root37校正合流，正式全量重放中）

这张表对应当前实际合流源码。下面336–339的旧表属于历史接受范围，不能继续当成当前缺口；作者未来公共表示和未合入源码也不能当成已完成能力。远端接受main仍为bcc5714f2，进度源码另有逐SHA核对的GitHub备份。

| 方面 | 当前合流增量 | 继续实施 |
| --- | --- | --- |
| 基础类型与模式 | Option、usize/u64原生和CT链、记录式枚举、拥有Tuple、混合Tuple/Enum部分移动与局部ref模式、实际清理顺序。 | 一般地址/Package/ErrorUnion源模式及拥有位置替换；126/127消费者仍在作者树，不计入本候选。 |
| 泛型与trait | const、关联类型/常量、条件名义impl、supertrait、默认及泛型接收者方法，模块/原声明身份保留。 | 一般life/lending方法、静态关联函数、约束蕴含与动态接口；不能把有限方法覆盖当完整trait。 |
| 借用与拥有 | 普通及嵌套Product可变helper、混合mutable life、借入FP、HRTB每次调用来源与固定环境隔离；实际两核/raw/Wire和缓存。 | 普通借入字段、存储位置不变上界、lending迭代器、一般合法捕获和状态转换。 |
| Type/Syntax | 普通泛型Type、具体纯记录、整数/浮点及匿名Sum的既有类型桥接。 | CT3匿名Sum值、完整phantom/type/const/life元数据和所有可序列化值组合；新CT3公共草案未作为生产CT2能力。 |
| 动态抽象 | 既有具体callable/opaque与显式拥有环境。 | 真正dyn接口/静态vtable、异构调用、完整associated/finite generic slot及共同借入存储。 |
| 状态协议 | 既有资源U/K/D、完整性/部分移动与callable State。 | 一般逐结果状态/角色/别名、真实投影及导出上界。 |
| 外部登记 | 返回式Registration稳定拥有实际闭包环境、显式关闭、真实安装/分配失败清理。 | 更一般借入/异步/跨线程环境、与waker晚通知和no-future/drain的组合。 |
| 异步与并发 | 实际结构化线程、MutexI32/AtomicI32、TaskI32稳定帧、显式Suspend静态child与一次初始化/Ready。 | 拥有Task<T>、通用PendingExit/取消排空/waker、Mutex<T>/Guard/CAS及一般Transfer/Share。 |
| 标准库与ZUI | 普通字符串/Unicode、Store插入移除和回调迭代、整数布局、RGBA合成、输入预编辑/提交/取消及失败保留。 | lending迭代、完整集合和平台窗口、OS IME、字体/布局/渲染闭环。 |
| 可靠性与产品 | 原大数组O0/普通大型按值ABI及默认2MiB API已修；默认栈深层调用正常通过或返回ResourceLimit。 | 更广特殊helper/FFI组合、完整浮点/匿名union，实际Windows及Rust同语义运行内存/维护/编译成本和突破实证。 |

泛型方法错派发原例已修：e5b3e9ae6以实际callee位置保持替换前后绑定，原exit4变为0，Astra精确复核见d556e0ece。root组合五组73/60/23/59/15通过；旧0cb/261及其失败收据保持，上一全量4354/14与逐项修正见docs/validation/terminal-root37-corrections.json。当前完整重放仍待通过。

精确限定回放：aggregates112/cross73/generic59/impl18725/UI16，generics23/trait58；IR61/Vec13及重建历史Wire消费者通过。源码与原失败、精确预算、独审分见docs/validation/terminal-340.json、terminal-341.json、terminal-342.json及各review收据。正式全仓、B0/完整语言/ZUI和product-gates尚未由这些数字升级。

## 历史336–339接受范围


2026-11-04当前共同受测源码`874f071bb53e85727b5467a2769ada47b51edd98`：默认4196/0、参考4086/0，各422组零忽略/过滤及双Clippy等门通过。336–339有限增量、真实机器结果、原失败与边界见[本批验收](LANGUAGE-BATCH336-20261104.md)和[精确记录](validation/language-batch336-integration-20261104.json)。此前B/C的4166/4057仍属于[上一批](LANGUAGE-GAPS-20261104.md)，不重标为当前结果。

原334的52份输入及历史40fb收据保持原字节；新release CLI实际重放仍为27 Success、13 Unsupported、12 SafeReject，原25源9/6/10，旧正控/保护反例无回退。新特性另有具体源/两核/机器证据，不从该固定样本统计推导完成率。[当前投影](validation/batch336-current52.json)不替代全部语言或机器验证。

| 方面 | 当前已支持的有限范围 | 尚未补齐及下一消费者 |
| --- | --- | --- |
| 基础类型与模式 | 普通枚举、穷尽match、有限Copy tuple与通配部分解构；329有限Copy记录式变体及完整显式具名模式。 | Option、usize/u64完整原生链、ref及一般非Copy/借入模式。fnptr返回的记录式枚举参与match目前需要显式结果类型，原未标注组合源保留。 |
| 泛型与trait | Copy/StoreElement/F/State、静态名义trait、具体泛型trait、有限默认体；336默认体可返回实际普通薄fnptr，局部遮蔽、完整签名与模块私有身份保留。 | const、关联类型/常量及投影、泛型/条件impl、supertrait、泛型方法。336限明确结果标注的普通具体Copy签名；一般限定函数路径及更大默认体组合仍缺。 |
| 借用与拥有 | 普通完整Product局部可变helper、有限共享life及CopyT/life、337 life-only具体Direct/inherent可变返回；真实来源、mutable输入权限和独占窗口分别检查。 | CopyT+mutable、HRTB/借用FP、借入存储与一般lending；未用无life可变泛型存在定义期别名检查缺口，实例化时拒绝。部分未标注callee转发及循环改变引用来源仍有限。 |
| Type/Syntax | 普通Copy泛型返回Type、具体泛型纯记录参与CT；两份原源本批Clang/API O0/O2共8次native通过，生成产物继续普通核验。 | 一般拥有/借入/动态/异步CT组合和全部产物覆盖，不能从有限成功源推出全面完成。 |
| 动态抽象 | 具体Fn/FnMut/FnOnce、opaque拥有工厂与有限具体环境存储。 | 完整dyn、运行期异构调用及显式存储/管理route；不能用隐式Box/RC补位。 |
| 一般状态协议 | 资源U/K/D、初始化/完整性、Store及有限callable/State合同。 | §5.2一般有限状态/角色/真实投影、逐结果转换、别名与导出上界的完整生产闭环。 |
| 返回后的外部登记 | 静态无环境C回调、返回静态guard、同线程结构化scope和同步事件环。 | 安装返回后保留捕获环境的登记对象，稳定存储、关闭/晚通知责任；原捕获c_callback保护仍保留。 |
| 挂起与并发 | Linux x64真实结构化pthread、静态纯标量worker、具体父闭包入口；339另有共享i32指针载荷、父侧合法并行窗口和join归还。 | worker仍是封闭只读图、标量结果；未核局部Store/lending投影Unsupported，已核调用/捕获可承接其保活责任。Mutex/Atomic、一般Transfer/Share、Suspend/SABI、稳定帧、取消排空、waker/晚通知、脱离handle仍缺。 |
| 标准库与ZUI | ByteBuffer、Store、UTF和真实I/O/FFI，可构造有限同步组件。 | 完整String、集合/迭代库，ZUI布局、IME、渲染和平台闭环；缺std.io导入不等于没有I/O能力。 |
| 机器可靠性与产品验证 | 原a18、普通大型Array/Tuple/Record及338普通Copy Enum/ErrorUnion内部按值/返回/fnptr/入口已核，`.lc1`贯通符号与机器缓存。 | 大型closure/特殊helper/foreign接口仍有限；合法大型借入Enum闭包在普通CLI成功但2MiB Rust API仍崩溃，内部阶段未知。浮点/匿名union、原生Windows与Rust同语义安全/运行内存/维护/编译成本仍未完成。 |

339的普通地址参数/固定捕获槽需要真实调用边界保活，不按原始对象类名统一拒绝，也不凭同名life、不同参数号或仅有Address类型授予权限。实际同根Store/拥有环境转移和父body冲突有独立拒绝；纯Copy闭包的合法复制保留原存储，不误记为悬空。

小栈合法`var f`样本与有FnMut调用错误的原`let f`样本分别保留；后者正常CLI的Types拒绝不是安全反例或合法机器失败。完整矩阵的语义依据仍为`_zil_design.md`及具名附件；派工/登记、测试数、CLI成功均不替代B0、完整语言/ZUI和产品门。具体下一步见[优先级](LANGUAGE-NEXT-PRIORITIES.md)。旧301–312未恢复源码与历史Unavailable/Unreviewed、legacy Fail继续分账。
