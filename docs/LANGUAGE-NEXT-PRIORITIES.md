# 当前语言主线优先级

基线为已合入main的`40fbcca6ef506f3cb8e1d40a4f8995c788627da0`。334按用户十方面复核，结果为52份源检查、9份正例的18次LLVM API O0/O2原生构建/执行，见[剩余矩阵](LANGUAGE-REMAINING.md)及[精简结果](validation/language-gaps-334.json)。它们不是全量或完整语言验收。旧25源保持原字节，life-only和具体泛型trait已从旧失败晋升；未声明life现在明确拒绝。

普通Copy tuple绑定及通配部分解构、普通Product局部可变helper、CT普通Copy泛型返回Type/具体纯记录组合均已有实证；322共享life来源合同、323具体泛型trait、325/328真实Direct结构化线程也已有有限实现。324原a18的O0崩溃已经修复。后续工作不重复开发这些旧缺口，也不把有限支持概括成整个life/trait/并发已完成。

| 顺序 | 实际消费者 | 当前并行施工与保留边界 |
| --- | --- | --- |
| 1 | 普通消息/节点数据、共享模型视图、组件协议的自然源码 | 329有限Copy记录式enum变体、331`T:Copy + life`共享视图、330有限默认trait方法并行。它们在本基线仍未支持，须保留未用定义符号检查、完整实例身份、来源和累计预算；泛型impl/关联项/可变life等终态继续保留。 |
| 2 | 普通父闭包组合与大型值的真实机器可靠性 | 332将已有thread_scope接到有限具体父闭包入口；333先运行未执行的巨型by-value原源，再按真实调用/返回/FP链修复。不能只移除两核门或改一侧ABI。原a18已通过，by-value未测不预报崩溃。 |
| 3 | 常规可选数据、长度索引和容器接口 | Option、usize/u64完整目标数值/布局/转换/ABI及剩余模式尚缺。用节点/集合API固定实际需求和正反例，再给不相交切片；有限Copy tuple部分解构已支持，不重做。 |
| 4 | 可复用模型/迭代协议与运行期异构handler | 在真实普通库需要时推进关联类型/常量与符号投影、一般impl/方法、借用FP/HRTB及显式dyn存储。先补缺少的公共来源/环境/清理/ABI合同；不靠具体i32特化或隐式Box/RC绕过。 |
| 5 | 安装后继续通知的外部组件、异步I/O与同步库 | 返回后保留捕获环境需要稳定存储和关闭/晚通知合同；SABI/稳定帧/取消排空及Mutex/Atomic/弱内存分别推进。普通同步事件环无需等待全套异步；真实pthread不替代这些义务。 |
| 持续 | 标准库、ZUI及平台/产品门 | 复用ByteBuffer/Store/UTF/FFI与已核Type/Syntax建设String、集合/迭代和实际组件，再接布局/IME/渲染。浮点/匿名union先明确合同。原生Windows与Rust同语义安全、运行/RSS、维护和编译成本逐项验证，不把Wine或编译成功代替它们。 |

329–333均为施工，不因本页排序、源预期或公共合同登记变成Pass。施工与独审仍须绑定精确提交；root管理规范、公共表示、主树状态和正式全量。共享泛型入口等相交写集由root串行整合，不能因并行编号自动认为互不影响。

历史pending四源`runtime-tuple-binding.zil`、`ct-type-generic-function.zil`、`ct-type-generic-record.zil`、`mutable-product-helper-gap.zil`保留原路径与字节，本次均已原生成功。`life-generic.zil`、`generic-trait.zil`及recovery中的具体life/trait源也不能再按目录名判为失败。旧泛型错误联合pending及其最初误拒继续引用[历史验收](LANGUAGE-GENERIC-INJECTION.md)，不重记成本轮新增覆盖。

每批按新增可表达的普通API、最近反例、真实来源/清理、N/N−1/fresh及必要机器结果验收。已有能力的组合回退先修，未支持的合法目标不改成永久应拒。完整语言/B0/ZUI及Rust四目标仍未完成；不重启CP研究、Skill改写、测试目录大重组或raw evidence归档。
