# 完整语言还缺什么

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
