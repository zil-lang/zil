# 原上传1.4 / ZUI语言终态紧凑审计

## 结论与绑定

当前不是完整1.4终态。已经有真实贯通的有限拥有/借入/Store/回调/错误结果路径，但单一高阶helper的可组合性、局部storage捕获、通用拥有形状、完整life/trait/CT/语法扩展及Suspend仍存在实际生产缺口。不能以178–186清单完成替代原设计全部条款。

原上传文件SHA256为 7c29b3505cb4838a7f892c7e70dc52f45b24fc001bdec7c00ef07e719e258e0e。其文件名1.4，正文标识仍1.3、Ante-R1待采纳且实现基线173；这是设计文字状态。后续主线已显式采纳Rule1.4，不能倒称178–181一开始已实现全部最新设计。

实际30个CLI探针绑定组合b9071e00（main14d62b4 + 已授权182修复ebaddd7 +186fc96383d），均保留原源码和JSON。该组合当时是审计候选，非主线最终验收。后续186字段来源修复0fb8aaae有独立144/0/0限定结果；184 lending b1461dc为非作者独审在途候选；187局部ByteBuffer捕获正在实施，未计完成。

表内已有基线是生产源码/既有专项的证据，只有列明probe或本轮冻结执行结果才表示此审计重新运行。未跑全workspace，未改公共规范/工单登记。

## 条款矩阵

| 条款 | 状态 | 实际能力 | 尚缺或永久边界 |
|---|---|---|---|
| §0–1 基础四分类及四项目标 | 有限实现 | 现有Value/Managed/Desc/Address、实际生命周期与失败关闭；178–186仅验证有限形状。 | Rust成本/组合性/维护性与真实ZUI产品未完成，不能以任务数或测试数替代。 |
| §2.1–2.2 值分类、资源头、移动与显式所有模式 | 已有+本轮细化 | 178定义期StoreElement，183拥有成功载荷，186泛型成功载荷；0fb8候选修复var内部Package包装丢创建来源。 | 默认Desc不得升级；通用递归拥有模式仍未全部开放。 |
| §2.3–2.4 赋值准备、数值与解构 | 有限实现 | 现有先准备后替换、字段部分移动、检查数值转换；144邻接包含Package/Store失败清理。 | 任意自定义Drop的decompose及全部结构形状不能由有限字段例外外推。 |
| §3.1–3.3 来源集合、One/Many、有限CFG | 有限实现 | 真实参数/局部/字段/环境来源与固定点存在；新旧实例仍按实际路径检查。 | 任意递归来源家族及所有非局部投影未统一完成；必须保持ResourceLimit边界。 |
| §3.4 / §4.1 投影与失效依赖 | 本轮落实有限片 | 179真实local+field roots分开父壳存活与字段足迹；180真实Store独占窗口；182逐捕获实际效果。 | 未知Drop/未知索引不假apart；任意私有抽象投影与重入/挂起/外部保留尚不能声称完成。 |
| §4.2 Store元素窗口 | 本轮落实有限片 | 180共享/独占Copy字段投影；181真实index/len/Copy操作；Store/Callback/Copy sibling组合。 | 视图逃逸、被闭包捕获、任意元素/任意动态不相交和更精细append仍关闭。 |
| §4.3 当前调用窗口 | 本轮落实有限片 | 182捕获与实际实参同一调用期检查，非仅最后使用后的活性。 | 闭包显式Address实参仍被finite Copy前置拒绝，不能宣称完整当前调用组合。 |
| §5.1–5.2 有序效果与有限protocol | 有限实现 | 现有资源责任和Desc实际读写摘要、有限Drop/Store状态与独立重建。 | 通用角色/关联alias-case/组件产品协议及跨挂起状态未见与完整设计同构的生产承载，不能由Store限定实现替代。 |
| §5.3 高阶Hold/Call/Cleanup合同 | 有限实现，188候选目标 | 单一F、单一参数、有限无环直接调用序列及分支已经可验证。 | 嵌套helper、循环trace、多F/多参数尚关闭；Fn本身不证明可重复无害。 |
| §6 实际管理路由与选择关联 | 有限实现 | Managed/Package显式来源和实际拥有责任闭合；普通union失败0成功1。 | 任意返回/存储的选取联合路由、显式存在包及高阶逃逸不应从本轮结果外推。 |
| §7 清理、部分初始化与析构 | 已有+本轮补齐片 | 字段顺序、逆序局部清理、失败准备/Store前缀清理和真分配失败已有实际运行。 | 通用异步/panic/cancel PendingExit、可恢复close协议及任意Drop结构未完成。 |
| §8 显式manages与实例替换 | 有限实现 | 有限Package及资源字段/数组/Store/callback形状的take/refill/drop已有独立检查。 | 直接嵌套Package字段仍Unsupported；通用Product形状尚未统一。 |
| §8.1 拥有形状代数 | 部分形状落实 | Package、固定Package/Closure数组、Store活动前缀、具体Closure及有限拥有ErrorUnion均有生产实现。 | 任意Product/Sum递归组合、普通Option、直接资源数组类别需明确后贯通，不能靠形状白名单宣布终态。 |
| 当前补充§8.2 StoreElement符号约束 | 178本轮落实有限片 | 未实例化定义检查，有限Package元素，两种真实名义形状和清理都验收。 | Closure元素/任意拥有元素未在StoreElement有限集合内；不能猜create上下文。 |
| 当前补充§8.3 E!Managed成功载荷 | 183+186本轮落实有限片 | Copy E + 直接支持R/ByteBuffer/Store；give参数作用成功载荷、外层普通Value移动；?与显式create/give。 | 两侧拥有资源、任意嵌套Sum、union捕获/替换及默认Desc升级不在当前片。 |
| §9.1 捕获规则与精确来源 | 有限实现 | Copy/Managed/Package有限捕获及182混合Desc/Address实际来源；owned ByteBuffer对照通过。 | 局部借入ByteBuffer正在187；slice/Store/带Store或拥有数组Package捕获仍明确关闭。 |
| §9.2–9.3 Fn/FnMut/FnOnce与动作选择 | 有限实现 | 持有/可变借入/消费分离；普通分支直接helper调用通过，重复拥有环境责任检查保留。 | 循环helper要求可独立验证循环合同；泛型多回调与不同模式序列未普遍支持。 |
| §9.4 opaque callable与环境lending | 184候选有限实现 | b1461dc实际inline Copy/owned ByteBuffer共享&Copy或&[Copy]；Fn/FnMut、借入helper、State字段及真实native证据。 | 尚在独审，不能算主线完成；通用life/HRTB、外部Address/Desc、嵌套Package/Closure、独占结果、消费/临时env均未开放。 |
| §9.5 延后执行、容器与借入边界 | 部分实现 | 自包含callback字段及固定数组、Store资源容器有限支持；具体身份未按布局合并。 | 一般持有借用的动态handler/iterator、Active槽重入、存在环境和dyn容器缺生产闭环。 |
| §9.6 ByteBuffer及真实视图 | 已有+187/184候选 | 已有owned ByteBuffer/Package字段/实际reserve失败和视图借期；184 owned-env lending候选。 | 普通局部Desc捕获probe明确Unsupported，187正实施；slice捕获仍另缺。 |
| §9.7 混合捕获与当前调用 | 182本轮落实有限片 | 无关head不再统一拒绝；隐藏helper Desc实际读写、不同来源、当前调用别名维持检查。 | 显式Address参数、动态storage捕获、完整高阶嵌套合同仍需独立施工。 |
| §10 分支、循环、递归与资源上限 | 有限实现 | 普通语言CFG、资源替换循环、分支must/may和默认预算拒绝已运行。 | 不能把普通循环等同高阶helper循环：后者实际Unsupported；真正递归helper继续明确拒绝。 |
| §11 并发与Transfer/Share | 未形成设计终态 | 同步源、真实Drop路径具备基础；并发资格不能只按数据布局推导。 | 没有本轮线程/fork/join/Mutex/Atomic/弱内存、Cleanup/Resume资格的完整生产与执行证明。 |
| §12.1–12.3 Suspend/SABI/取消/关闭 | 未实现完整机制 | 当前执行链以同步Native函数和Trap出口为主。 | 稳定frame、poll/request_cancel/poll_close/finish_destroy、PendingExit与no-future收敛缺完整语言和ABI闭环。 |
| §12.4 C ABI | 已有有限同步片 | x64 Linux/Win64、显式noescape地址、静态无环境callback/handle/wstr及本轮真实PE。 | 外部保留userdata、异步callback/waker、跨边界借入环境仍未开放。 |
| §13–13.1 后端表示与机器等价 | 语言平台有限片；独立编译线 | 181实际x64 Heap ABI、平台配置/失败注入；语言slice仍走VerifiedLIR门。 | LLVM API替代文本IR、机器缓存、共享target配置/性能属于独立LLVM/编译线，不在本代理重复改。 |
| §14 / §18.1 名义抽象与泛型 | 有限实现 | 普通静态trait/泛型record/alias、StoreElement与单F bounded helper有实现。 | 关联类型/常量完整solver、<life a>/HRTB、dyn固定ABI、一般多泛型高阶组合未完成。 |
| §15.1–15.3 语法、模式、求值 | 已有有限基线 | Unicode/token/module、局部绑定/范围/receiver/有限slice与错误联合已实施。 | Optional类型参数真实Unsupported；其余未测语法不能由parser接受当语义支持。 |
| §15.4–15.6 方法、默认值、管道与spread | 已有有限基线 | 现有methods/defaults/pipe/tuple spread生产与专项测试存在。 | 只覆盖既有有限类型和求值模式；涉及未支持的拥有/借入形状仍不能外推。 |
| §15.7 Native与Managed profile | Native是当前目标 | 当前Native四分类/显式资源责任，无隐含GC。 | Managed/GC profile本就设计为后续，不把它列作同步Native ZUI必须先完成的语言缺口。 |
| §16 / §16.1 CT值、Type与Syntax | 部分实现 | 受预算CT标量/函数求值通过到Verify（square对照）。 | Type/Syntax数据生成、反射、#if/#for/#emit物化未达设计；合法#if声明/语句探针实际Unsupported。 |
| §16.2 阶段DAG与封存impl | 依赖架构未完整核验 | 现有CT/Expand/Types阶段及配额存在。 | 生成器/trait循环、冻结impl批次的完整语义仍有前置语言缺口；查询调度/增量性能归独立编译线。 |
| §17 属性与生成产物边界 | 表面识别与拒绝边界已有 | 属性词法/绑定与未执行请求的明确诊断存在；未知或未执行属性不构成安全授权。 | 完整derive/属性扩展协议和卫生生成未达设计，与Type/Syntax生成同缺。 |
| §18.2 卫生语法扩展 | 未形成设计终态 | 固定parser/Core输入和普通编译检查存在。 | syntax use /锁定扩展/源码映射/预览/formatter与受限生成器的完整路径尚缺；ui DSL前置应单独列，不能用手写ZUI消费者替代。 |
| §19.1–19.6 编译架构与缓存 | 独立施工线 | 语言slice遵守独立Verify、固定budget与源码接口边界。 | per-item/SCC query、immutable Snapshot、CGU/LLVM API/cache authority/canonicalization属独立编译速度任务；不重复改、不替其验收。 |
| §20 / V01–V22 完成定义与验证 | 未达全1.4终态 | 本轮多个有限slice有源码/raw/Wire/native/失败注入，并保留实际失败。 | 需逐条闭合正例和最近反例；Unsupported、ResourceLimit、安全拒绝与环境失败不能混计Pass。任务勾选本身不构成设计完成。 |

## 最紧迫且可独立施工的下一批

1. **187（已派）局部ByteBuffer Desc捕获**：`fn main()->i32{let b=bytes_new();let f=()=>bytes_len(b);return i32(f())}` 当前明确Unsupported；移动拥有捕获对照Pass。必须补真实动态storage效果与当前调用别名，不可删准入门后依赖已有Fn标签。
2. **188建议：有限无递归helper组合**：`inner<F:Fn(i32)->i32>(f:&F,x:i32)=>f(x)` 再由 `outer` 调 `inner(f,x)`，Expand明确Unsupported；184 State→helper也有独立实际失败。现有有限Trace可表达，优先给单一真实参数、同一bound kind、无环有界调用依赖DAG，展开顺序与Return/Trap出口。
3. **独立循环helper合同片**：while内调用借入F实际Unsupported，而if分支调用对照Pass。不能因Fn而认定无限次调用安全；需独立可验证的循环不变式/重复前置，或明确有限模式。188继续拒真实递归和无限循环，不顺带放开。
4. **独立显式Address回调参数片**：`let f=(p:&i32)=>*p;f(&x)` 和可变版本均Unsupported finite Copy type。对ZUI事件回调的状态借入有直接价值；需当前实参/捕获alias组合、重借、结果来源和raw/Wire，不与184环境结果lending混为一项。
5. **后续组合片**：`T:Copy + F:Fn(T)->T`、两个callable类型参数或同一F两个参数、泛型返回F均有各自具体Unsupported。需扩合同参数索引和来源域；不能只放松泛型数量限制。
6. **形状与UI表面层片**：普通Option、直接嵌套Package、CT声明/语句#if与Type/Syntax生成均有真实代码或探针缺口。直接资源数组的源类别先明确，不把Desc静默升级。完整ui DSL需要§18.2真实扩展路径；同步手写ZUI消费者并未验证该路径。

## 永久安全拒绝与暂未实现分开

父壳move/drop、相同字段替换使旧实例失效、真实同源写与活动读/独占窗冲突、重复unique/责任、旧资源包装假create、未知索引假apart、借入环境逃逸、FnOnce返回其即将销毁存储的视图应继续安全拒绝。它们不是待放开的“缺口”。未知或暂不支持的安全形式则应保持Unsupported并列工单；上述ByteBuffer局部只读、嵌套helper等属于后者。

## 不计作能力缺口证明的试探

probes01的ct-emit使用了不成立的生成表达式，store closure element缺明确Store上下文，首个nested Package缺manages字段，option直接注入猜测语法，probes02 dyn方法声明少分号。这些原始输出保留但不拿来证明设计禁止；nested Package/Option/#if已用有效更小输入复核。async探针只说明给定拼写不在当前parser，Suspend的缺口判断还依据生产IR/Lower未形成完整SABI，而非仅这一parse失败。

## 证据入口

机器记录：terminal-matrix.json；实际源码和JSON：probes01与probes02。每个条款的生产文件定位保存在JSON source_evidence字段。184的实际运行收据由其作者/独审保管；本报告不复制计数为自己的执行。独立LLVM/编译速度线的§13.1、§19查询/缓存/API事项只列接口依赖，不重复实现或代其宣布完成。
