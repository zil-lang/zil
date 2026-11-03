# 完整语言还缺什么

2026-11-03核查，本地恢复候选`a57a668b68044e37e870d058684f9364307e04b1`。本地全量默认4055/0、reference3947/0（各422组，零忽略/过滤），fmt/clippy、text64/0及migration9/0通过。远端main仍为`7f039d16`，候选备份分支为`task/language-recovery-20261103`；这些本地结果不代表远端合入或CI通过。按现行`_zil_design.md`及具名附件、当前源码和最小源检查归类；这是能力清单，不是以测试数量计算的完成率。当前同步资源语言已经可运行，但完整语言尚未完成。

## 已经打通的主干

文件模块与可见性、普通直接/互递归、有限泛型和静态trait、有限拥有Product/Sum/Store、自动清理与同步defer、具体Fn/FnMut/FnOnce、拥有Event→公共dispatch→拥有Reply、F原值转交、共享Product参数捕获、Type/Syntax生成及同线程结构化外部回调均已有实现与相应验收。这里的“已有”限于各自公布范围，不代表任意组合都支持。

相关阶段：[拥有回调](LANGUAGE-OWNED-CALLBACK-20261102.md)、[原值转交](LANGUAGE-CALLABLE-FORWARD-20261102.md)、[同步核心](LANGUAGE-CORE-20261102.md)、[共享参数](LANGUAGE-SHARED-PARAMETER.md)、[生成链](LANGUAGE-TYPE-SYNTAX.md)、[外部回调](LANGUAGE-SCOPED-CALLBACK.md)。历史状态页中的旧“未实现”不覆盖这些新验收。

此前PR27已补泛型显式绑定、普通函数实参、已知具体闭包实参的错误联合注入，以及独立约束、实际回调返回和具体实例的歧义检查；该历史阶段的独立审核、本地完整工作区和两套最终CI通过。详见[阶段结果](LANGUAGE-GENERIC-INJECTION.md)，不将其CI成绩归给本地恢复候选。

本地恢复候选已完成以下有限范围的实现与验证：313的普通有限Copy tuple绑定及321的泛型符号消费者；314的Type/Syntax普通Copy泛型函数与具体纯记录组合及320的Struct Copy消费者；315的普通完整Product局部`&mut`同步helper；317的实际多模块组合消费者及O0/O2成功、错误和关闭清理。范围与边界见[313](../changes/313-copy-tuple-binding.md)、[321](../changes/321-generic-tuple-binding.md)、[314](../changes/314-ct-generics.md)、[320](../changes/320-copy-record-bound.md)、[315](../changes/315-local-product-borrow.md)、[317](../changes/317-recovery-consumer.md)。316仅完成[25源能力核查](validation/recovery-capabilities.json)，没有实现life、trait或异步/并发缺项。

## 实质缺口

| 类别 | 现在还缺什么 | 对普通语言/ZUI的意义 |
| --- | --- | --- |
| 基础类型和模式 | `T?`、record形式enum variant、ref绑定及超出已核Copy范围的更一般模式尚未贯通；规范示例中的`usize/u64`尚无完整原生类型链。有限Copy tuple普通绑定及泛型符号消费者已本地验证，已有普通枚举和穷尽match。 | 可选节点、消息数据和更一般解构；整数索引与长度接口。 |
| 完整泛型与trait | 已有有限Copy/StoreElement/F/State及静态名义trait；const/life参数、关联类型/常量与符号投影、泛型trait/impl、supertrait、默认/泛型方法尚缺。 | 常规组件、模型和迭代协议能以普通库表达，减少按具体类型复制代码。 |
| 借用与拥有组合 | 共享/独占地址、Store元素窗口、有限共享lending已有；普通完整Product局部`&mut`同步helper现已本地验证。一般可证明来源的借用返回、life/HRTB和更多合规拥有形状组合尚未闭合。 | 已核普通局部路径保留来源、完整性、窗口及清理检查；更一般视图、容器、可变捕获和回调组合仍逐项推进。 |
| 动态抽象 | 具体闭包和opaque工厂已有；借入/拥有`dyn`、异构调用与显式存储合同尚无完整生产链。 | 运行期异构组件和handler集合；存储策略必须显式，不能暗加Box/RC。 |
| 一般状态协议 | 现有资源责任、Store和有限callable合同可用；规范§5的一般状态/角色/投影、逐结果转换、别名与导出上界还没有完整生产闭环。 | 把更多有状态库协议交给同一规则检查，超出少数内建场景。 |
| 返回后的外部登记 | 同线程结构化scope已可覆盖长时间事件循环；仍缺安装函数返回后继续保留捕获环境的外部登记对象及稳定存储/关闭合同。 | 分离组件安装和后续外部通知。静态无环境C回调与返回handle本身不能证明捕获环境的生命周期。 |
| 挂起、取消和并发 | Suspend/SABI、稳定帧、PendingExit、waker/晚通知、取消与最终排空尚未闭合；线程join、Transfer/Share、Mutex/Atomic及弱内存证明也是独立缺项。 | 异步I/O和跨线程平台接口。同步事件环不必等待整套异步系统。 |

规范锚点：基础源面§15.2及`spec/grammar.ebnf`；泛型/trait§14、§16、§18.1；借用与动态抽象§8–9；协议§5；外部/挂起§12；并发§11；Type/Syntax§16.7。

[保存的最小源码](../tests/language-readiness/README.md)和[历史基线观察](validation/language-readiness-baseline.json)覆盖代表性缺口与13个既有能力正控；该历史观察是源到Verify检查，不是当前候选的原生或完整规范覆盖。`pending/runtime-tuple-binding.zil`、`ct-type-generic-function.zil`、`ct-type-generic-record.zil`、`mutable-product-helper-gap.zil`四源保留原路径和字节，现已作为正控运行，不再按目录名判为缺口；其余pending按实际能力区分，不能把合法入口的当前失败永久登记成“应拒”。

## 另外两类工作

- **标准库与平台消费者**：已有ByteBuffer、Store、UTF处理及真实I/O/FFI，不等于完整String、集合/迭代库、布局/IME/渲染和ZUI。库缺项与编译器原语缺项分别判断；`std.io`等标准模块当前也未供应。已有Wine/首窗结果不等于原生Windows验收。
- **规范定界与产品验证**：浮点规则/转换/ABI、匿名union的完整语义和若干语法表面仍需先明确合同，不能按其它语言习惯自行承诺。Rust安全、运行/内存、维护和编译成本四目标仍需同语义配对；当前不能给可信的“语言完成百分比”或宣称产品胜出。
- **大数组原生可靠性**：a18在当前工具链的O0仍会崩溃，生产修复尚未实施；精简复现见`docs/validation/recovery-large-array.json`。本轮有限组合和全量通过不关闭该问题。

不把返回局部借用、消费环境后返回悬空引用、defer中向外return/`?`、隐式装箱、重叠impl、CT泄出地址/权限等规范禁止行为列成缺口。普通模块内递归已验证；模块import环未明确承诺，不据当前拒绝强行列为必修。

## 下一步集中方向

普通Copy tuple、Product局部可变helper与Type/Syntax普通Copy泛型的本轮有限组合已本地验证，不重复开发旧pending四源。Option/基础模式、一般life/trait、关联类型和动态抽象按真实组件API接续；大数组a18的生产修复独立保留。需要返回登记对象的实际消费者出现后再扩外部生命周期；不为“长期”二字把同步主线改造成异步项目，异步/并发与Mutex义务也不因此删除。

每批以新增可表达的真实API、近邻拒绝边界和原生结果验收。保留独立审核与必要整仓检查；不重启CP研究、Skill改写、测试目录大重组或原始产物归档。
