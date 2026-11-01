# ZUI语言前置：本批之后的最短路径

只读判断，依据主树`22203555`时点的`_zil_design.md`§7.1、§8.1.3–8.1.4、§9.5、§12.3–12.4、§16/18，以及227/235已独审候选、237/238工单和236原源码。本次没有构建或新增功能探针；历史Unsupported仅按仍有效的有限合同解释，不冒充本日重测。

**先完成237、238及236原样联合验收。随后优先拥有Sum，再补拥有参数/结果的高阶组合；不重派动态集合、固定回调家族或ID关系。**

| 需求与最小源码/API形状 | 当前能表达的部分与真实剩余义务 |
| --- | --- |
| 完整同步会话：`Session{subscriptions:SubscriptionSet,nodes:NodeSet}`，两child各拥有`Store<Leaf>`；`current=grow(current,n)`；`defer{shutdown()}` | 235提供普通完整拥有、child take/refill/replace；227提供同步退出义务；238仍需补整值赋值事务，保持shell及清理年龄。236首候选实际被此处阻塞，不能先称联合完成。已有ByteBuffer、Store前缀、静态trait、固定F/G State、同步窗口、文本处理与ID关系不重做。 |
| 普通容器抽取：`Bag<T>{Store<T>}`、`Root<T>{Bag<T>}`、`wrap<T:StoreElement>(bag:Bag<T>)->Root<T>` | 这是237已派的具体缺口：直接/延后实例、owner、原family、phantom与完整实参身份。它不是“一般泛型尚未实现”，也不自动开放`Store<Root<T>>`、新Product捕获或共享资格。 |
| 可选拥有状态与tag切换：`enum MaybeSubscription{Empty,Some(Subscription)}`；`fn create()->InitError!Session` | **下一横向能力1：拥有Sum。** Copy枚举和有限`T!Unit`/`Unit!T`壳已有；真实拥有variant、两侧拥有错误载荷、新Product载荷仍受限。应核真实tag、每variant初始化、切换前准备、活动分支析构及来源保持。最近反例是旧variant借用跨切换、双取载荷、失败准备提前清旧值。空Store/ID可表达部分“没有订阅”，不能替代公开拥有Option API。 |
| 拥有事件处理：`fn dispatch<F:FnOnce(Event)->Reply>(f:F,event:Event)->Reply`，Event/Reply含ByteBuffer或Product；普通转发`keep<F>(f:F)->F` | **下一横向能力2：同步高阶拥有参数/结果与原样转发。** 保存/调用既有Copy签名回调、单F包装成新闭包已有，不应重派。当前不能据此推出callback任意拥有实参/结果；须保留实参准备失败、callee先准备、声明FnOnce限制、结果责任及环境残余清理。先用具体Event/Reply与跨模块公共helper冻结合同，避免同时扩dyn/lending/异步。 |
| 组件抽象：`trait View{type Model;fn read(self:&Self)->i32;}`；`fn measure<T:Measure>(x:&T)->i32` | 后者有限普通静态trait已有。关联项、泛型/default trait方法、通用投影与lending仍缺；237只修一个StoreElement族的身份。`&dyn Measure`乃至“任意用户handler统一拥有注册”还需独立固定ABI/效果/管理合同；有限F/G表或已知家族分表+ID已经可用。关联类型是后续独立切片，不把整个trait求解器放进237或下一高阶片。 |
| 声明式入口：`syntax use zui.syntax as ui; let panel=ui!{column{text("Ready")}}` | 受限`#if/#for`及生成普通struct/alias/Copy enum已有。Type/Syntax值、生成trait/impl、完整扩展链、卫生绑定、映射/预览/格式化及生成/手写同核仍是真缺口；手写Session通过不能替代声明式入口。此方向可独立规划，不作为普通同步Store操作的重复前置。 |
| 外部长期回调：API形状`register(stable_userdata,handler)->Registration`，`close(registration)`保证停止准入、注销排空、no-future后清理 | 这里只表示所需API，不是已有ZIL签名或新可信FFI注解。当前noescape地址与static无环境callback不能保存ZIL闭包userdata。须闭合稳定存储、注册失败、实际重入、已进入/仍可开始回调、停止准入与登记线性化、no-future及最终执行者清理；`defer{unregister()}`本身不证明排空。retained对该平台API是真阻塞，不能用同步Q标记冒充；不因此提前要求完整并发/async才能写同步库。 |

236通过后，足以支持**本批有限同步核心组合**的结论：五模块完整拥有、运行时0–3项集合、普通helper、整值赋值、失败准备/替换、fallthrough/return/`?`/continue/break及`LQabAB`关闭顺序；原生与逐分配点失败必须实际通过后才成立。它还没有验证237的泛型族身份、保存callback与新形状交互、任意规模图、陈旧ID协议、retained或声明式生成。因此237须保留自己的原样泛型正反例；既有217/State/callback回归须独立复验，不能让236取代。

上述两个下一片都应从真实源、最近反例、独立raw/Wire、布局/失败清理和原预算贯通；不引入隐式Box/RC/锁/全局表。它们提高普通API覆盖面，仍没有Rust运行/内存、维护或编译成本配对结论，也不能宣布完整ZUI、1.4或四目标合取完成。
