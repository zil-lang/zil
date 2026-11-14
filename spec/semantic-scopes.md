# 原始语义作用域与实际程序

本合同规定原始泛型、生命周期和语义捕获的完整表示。运输记录是不可信程序操作数，不是来源、能力或安全证明。Source、Types、Analyze、Verify 和 CT 消费者分别承担自己的完整检查；字段存在或能往返编码不代表检查通过。

## 原始定义与实例

定义保留跨模块限定身份、真实词法父链、原始混排的 Type/Const/Life 形参、隐式生命周期的原始发生位置、const 的完整载荷类型、全部 where 条件、语义自由变量和原始签名或名义类型体。未使用形参、phantom 参数和非活动枚举分支不能删除。

普通函数使用 `FunctionWithBody` 保留完整原义务 body，复用 `CoreBlock<SemanticTypeRecipe>` 及同一套 Core 操作，而不是保存求解器声称成立的约束表。全部原分支和未使用定义都要检查；原程序中尚未展开的 iterator 与 spread 操作也必须保留。`External` 必须对应真实外部声明；`Declaration` 只适用于真实抽象 trait 成员。签名和 sites 齐全不能把缺 body 当空 body；仅含这些字段的旧 `Function` 不得成为完整原定义证明或降 tag 的绕行入口。

原有序 value 参数表保留名字、可选注解和 passing mode，其稳定绑定身份是 definition 与 ordinal。`receiver_parameter` 只选择真实原参数，不能伪造编译器添加的闭包隐藏 receiver。`site_nodes` 显式把原 SiteId 绑定到同一原 body 的 Expression/Statement NodeId，要求节点唯一、角色和操作匹配；原 `occurrence_path` 仍是 Source 身份，不能重解释成改写后的 Core 路径。闭包自由名字在真实 parent 的原 Closure 节点环境中封闭，不能从 Typed local 名字、相同序号或任一具体实例猜绑定。

`names` 只记录真实原声明绑定：完整路径、Value/Type/Lifetime 命名空间，以及完整 recipe 或真实 definition 引用。词法父 impl 的形参使用真实 ancestor 引用；原参数和局部名字按 body 的真实遮蔽关系解析。名字表不能增加 where 前提、改变被引用 formal 的种类/carrier，或从字符串猜同号 lifetime；使用中缺失或歧义必须拒绝。它不代替常量原定义或来源证明。

原 Call 的混排 actual vector 保留显式 recipe 或 `Infer`。每个 Infer 的种类和 Const carrier 从真实目标 formal 取得，在 caller 原定义域建立新鲜变量，依据原 callee 签名、原表达式符号类型和真实原结果上下文求解，再证明目标全部要求。目标待证明的 where 不能先成为 caller 假设。无解、歧义、循环、发生检查失败或预算耗尽明确失败，不以具体实例 actuals 或第一个调用者猜解。Types 求解后，Analyze 与 Verify 各自独立核验；Typed actuals 必须全部物化，不能残留 `CoreInferred`。

例如 `same<V>(x:V,y:V)` 的调用者 `outer<T,U>(x:T,y:U){same(x,y)}` 需要在原域证明 T=U，不能因唯一具体调用为 outer<i32,i32> 而通过。Const 与 Life 的偶然同图像也不能补原前提。原未使用函数、特化后被消去的分支和 generic value 的 Copy/trait/清理要求同样受此规则约束。

原函数 signature.call_kind 是固定、尚未核验的调用约定上界。Source 的显式约定原样保留；未显式选择时，候选由完整原 body、真实调用绑定及 Callable/清理合同产生，不能从首个具体实例反推，也不能将未解析的边默认成 Direct。各消费者独立证明原 body 的全称效果不超过该选择，并核实际 Typed ABI 与之相等，不能私自升级 Direct 或降级 Suspend。显式 Direct 的函数执行 task_yield 必须拒绝；显式 Suspend 的空 body 仍保留 Suspend。task_create 引用挂起 worker 不等于创建者执行其挂起正文；IntrinsicCall.effects 同样需要真实封闭 intrinsic 身份和操作证明。封闭递归调用图的效果从真实来源闭包推导，悬而未决的调用边不作零效果。

语言封闭的 trait 使用 `SemanticBuiltinTrait` 原身份：`BuiltinImpl` 保留真实 self recipe 和原 impl 词法域，`Builtin` 要求保留实际被约束类型。不能给 Drop、Transfer、ClosedEnvironment 等捏造用户 nominal declaration，也不能把封闭 impl 改成 inherent impl。名字、原要求或某个 scope 的存在均不授予能力；每个消费者仍须证明该 kind 合法适用、真实 provider/body、清理与跨线程等具体义务。Send/Sync 的原拼写保留，其合法性或别名语义不能仅凭同一枚举族推导。Fn/FnMut/FnOnce 继续使用包含完整签名的 Callable 要求。

实例指向真实 Core 函数、Core 闭包发生点、Typed 函数或词法作用域。原始形参序号与内在类序号不同；完整类型已有的内在类不能因实际生命周期图像相等而合并。缺失语义目录是 Unknown，不能解释为零类。语义自由变量也不能伪装成物理捕获字段。

每个实际 Type/Const 特化保留真实 rigid 代表程序。原始生命周期在该代表中保持独立；具体生命周期分组必须指向同一特化的真实代表，并独立证明完整 CFG、操作数、类型、资源、闭包和清理的替换对应。不能只比较 ABI。`bad<a,b>(Marker<a>)->Marker<b>` 必须在 rigid 检查失败，即使所有当前调用者碰巧选择 `a=b`。

Rigid 使用原生命周期的新鲜身份及完整原声明允许的前提，不是无条件两两不等。定义检查只使用 definition/formal 的原 requirements、真实词法前提、已核环境要求和原签名关系。显式 LifeEqual 可以证明这些身份相等；Outlives 不能当成相等，调用者碰巧同图像不能成为定义前提。原 Callable 前提只能按完整 P/R/Receiver、Fixed、Invocation/Bound 关系推理，不能额外授予 TypeEqual、全类合并或 Bound 实例化。

Type 实参的完整内在域在 callee 自身实例域中封闭。`IntrinsicClass(TypeArgument(Ancestor(0),完整形参序号),[],k)` 是已核完整类型给出的原子内在锚，不能递归读取自己的 classes，也不是实际来源证明。每个真实 Call 在 caller 已成立的上下文求值实参，独立证明全部要求和实际 callee/entry/header/env/资源对应，再建立边界映射；不能使用目标正假设的 where 循环自证，不能把 caller 作用域或 SSA 偷放入 callee，也不能回写 rigid Life 实参。已有 whole T 的 N2 不因两个图像相等降成 N1；新求值的 Nominal recipe 只可使用当前合法前提证明的相等归一。

精确类相等与 callable 角色满足是不同判断。Static、合法缩短、Outlives 和 Fixed 局部适配不能进入全局 LifeEqual，也不能成为 fresh Nominal 归一依据。同一 Static 输出 F 可以满足 `Fn()->&a` 与 `Fn()->&b` 两个允许的角色，但不能据此把 Marker<a> 变成 Marker<b>。只有原显式 LifeEqual 或独立成立的精确相等规则才能用于该归一；已有完整 T 的类数与序号仍不变。

闭包的 `SemanticEnvironmentFormal.source` 在真实词法 parent/creator 的定义域解释；`SemanticEnvironmentInstance.actuals` 在实际 parent/creator 实例的真实闭包创建站点求值，再封闭为 child 的 ghost 实参。这两处的 `Ancestor(0)` 指该 parent 当前作用域。creator 的 Value/Local 序号不能转而选择 child 的同号操作数。必须核对实际闭包、创建指令和 parent 对应，多次创建逐实际对应检查，不能选首个调用者提供证明。环境绑定不回写或替换实例原有的 rigid Life 实参。

`SemanticEnvironmentFormal.kind` 与其 source 使用相同的 parent 定义域，包括 Const 的 carrier 和 Type 的 requirements。复制来自更外层定义或环境的 kind 必须显式重定位到当前 parent 域，不能盲用旧 Ancestor(0)，也不能按 child 的相同序号查找。child 使用的环境槽必须已经在父域核过并封闭。

Typed 类表达式的 Environment 引用只读取沿真实词法路径找到的已核 sealed Life 槽，并检查原 kind 与完整序号。Life 槽没有类型内在域，不能伪装成 `IntrinsicClass(Environment(...),[],0)`；也不能为了可表示而增设物理捕获或 BorrowRoot。

## Core 到 Typed 的完整求值

Source 不得猜测 Types 尚未分配的 ClosureId、完整类型内在域、const 载荷或常量位模式。`SemanticTypeValue::Core` 保存精确词法实例和完整原始类型 recipe；推导类型还绑定真实 module/function/expression/occurrence。`SemanticActual::CoreRecipe` 保存待代入的完整 Type/Const/Life 实参表达式。无类型字面量和关联 const 必须先得到真实载荷类型再求值。

Typed 的实际 SSA、local、签名、捕获、字段和容器元素必须一致保留 canonical 完整 Type。聚合的非零持久域包含 phantom 类，必须进入实际 BorrowedStorage qualification。例如 Marker<a>{i32} 使用 slots 为空、binder_count=1、nominal_arguments 保留原 a→class0 的 BorrowedStorage<Record>，不能用 plain Record 加 Some 旁表代替。Resolved.ty 必须匹配被绑定的完整实际 Type，life 从该 Type 与真实 nominal/opaque 目录独立重建，不能作为矛盾的第二合同。类型比较、实例键、缓存与谓词不能先剥除 qualification；跨请求缓存还须解析真实声明身份。

qualification 不增加物理内存，不生成 BorrowRoot、地址、loan 或资源权限。根 Address/Desc 保留原表示，由外部 life 描述其外层发生；内部 BS 完整保留，不额外制造 BS<Address> 或直接双 wrapper，已有 nominal BS 即使 N0 也不能随意剥除。

完整 nominal 目录同样覆盖未包 BS 的普通 Record/Package/Enum；plain 实例仅在消费者从真实声明、完整混排 actuals、全部字段与枚举分支及依赖目录独立重建持久域并证明 N=0 时合法。目录缺失、循环或未支持不能当空域，slots/ports 为空也不能证明 N0。Type 实参和 phantom 参数仍完整参与；任一非零持久域必须保留 BS。同一 exact Type 只能对应一个完整 declaration/actual vector；同布局不同声明或 phantom 实参不能覆盖同一项。plain N0 与已有 BS0 不自动互换，根地址外层发生也不因 referent 为 N0 而消失。


例如根共享地址 `&κ i32` 的 `Resolved.ty` 仍是原 Address；外部 `life` 有一个 binder 和位于空路径的 `SharedAddress` slot，`classes` 保存 κ。标量 referent 的内在零域不能抹掉该外层发生，也不能将外层发生与 referent 内在类合并。

根 BS<N> 是完整内在域边界，header.classes 只列该 base、空路径下的 N 个 class，不重复列其 child paths；BS 自身完整 slots、retained 和 nominal_arguments 仍独立核对 child→wholeN 关系，header.bindings 另列真实物理借入发生。根 Address/Desc<BS<N>> 不增加 BS 边界，外层借入单独 binding，pointee 的各内在类使用 `[Pointee]` 的 class 行。

`OriginalType` 引用正确原定义中的 expression、binding、parameter 或 result 类型义务。`OriginalExpression` 分别保留原义务节点与真实 Core 发射发生点，Source 不必预先执行 typeof。缺注解的局部值或闭包参数依原 body 和合法上下文求唯一符号解，不从运行时 occupant、child 同号 SSA 或具体特化反推原类型。

Opaque 的 hidden recipe 保留 callee 定义域，显式 arguments 才按实际调用代入；callee 局部结果角色不能按 caller 的同号 Implicit 解释。原调用求值须在原 Γ 中建立结果角色边界，以真实 caller 定义域、原 Call site、callee 定义、已核完整符号实参和结果完整内在类槽标识，随后与实际 caller/callee 实例及 Call 结果 SSA 对应。Source 可用既有 OriginalType/OriginalExpression 延后此求值，不预猜结果 N 或伪造 implicit 行。不同调用的角色锚各自保留依赖，但不因此证明来源不等，也不改变每个 opaque 定义及 Type/Const 实例的名义身份。

结果边界先从已核定义的完整结果 N、全部 return 和局部角色的全部输入/static/role 依赖证明导出关系，再由真实调用建立结果内在类发生；不能读取待核结果 classes 自证。局部指针可分别取 a、b 时，返回闭包的局部 κ 必须保留两者对 κ 的保活关系，不能选一个输入替代 κ。phantom 与非活动分支类仍参与，两个图像相同不能压缩完整 N。局部声明的原 expected 仍由真实 original binding/initializer 独立求解后比较，具体 initializer 的已发射类型只提供实际对应，不能反过来补原 Γ 的前提。公开调用者使用已证明的导出关系，不取得依赖 callee 私有实现的额外能力。

`BindingProjection` 精确选择原 pattern 的名字叶：Match 首项选择 arm，其余项按原 pattern 的子项顺序下降，不能把一个 tuple 声明的整体类型当每个名字的类型。`NameAt` 只读取已核原词法点的类型环境，不授初始化或 occupant 读取权限。捕获使用真实 parent Closure 点的环境；child 的同号节点不能替代。原Closure节点属于parent，内层body和参数属于该Closure site对应child；两份原body按同源节点/类型域对应核对，父索引不得进入child后碰号取值。

该延后表示统一适用于 Type 实参、Const 的载荷类型、实际 Self、构造结果、类型使用和局部声明，并覆盖函数与闭包环境中这些记录的每次出现。实际存在但尚未求值的 Self 不能写成 None，未知完整域不能写成空类表。Self recipe 不能以自身作为解析基准，必须由真实 method/receiver、impl 作用域和父实例实参确定。

Types 消费真实 Core 发射账本，以原表达式身份和发生点解析推导类型，核对 recipe，产生完整 `Resolved { ty, life, classes }`，并重建完整实际 header embedding。Core 阶段尚未发射的空 embedding 不代表 Known0。Types 输出 Typed 前必须解析所有延后记录，连同未使用定义、词法作用域、环境和非活动分支；Analyze、Verify 及可执行 CT 消费者拒绝任何残留 Core 类型、CoreRecipe、Core 目标或未解析语法。

普通函数的真实结果内在类发生可用 Invocation header 行映到已验证的原生命周期输入；闭包内部已捕获或封闭的类使用其真实 Fixed 关系。仅在完整语义程序、原声明输入、header embedding 和每个实际调用均独立成立时，才可识别没有物理参数叶的 ghost 生命周期输入；Some 或 Some0 本身不够，不能为未使用的原 lifetime 伪造 Fixed 行、Parameter 或 BorrowRoot。它只建立语义类关系，不能生成 Address/Desc 的实际内存来源，不能减免父 loan、epoch 或资源义务。

## 实际语义站点

原始站点保留原语法发生路径和作用域。Core 站点绑定真实表达式或声明的发射角色；Typed 站点绑定真实 block/instruction 和前后边界。构造、调用、类型使用、局部声明和闭包环境必须核对实际操作码、SSA、实参和结果，不能用同形状的其他值替代。

Source 在泛型变换前记录原始语义事件，并通过真实发射账本绑定 Core 节点，不能从 Span 或改写后路径回猜。请求内标记仅是账本重定位索引；原始输入和 CTAST 不能提供已记录标记作为证明。按真实实例、生成路径、owner 与发射角色核对并线性消费事件，克隆一个标记不产生第二份语义授权。

局部声明建立之后写入必须满足的类约束，不能读取尚未初始化的 occupant。间接 callable 调用保留原始 callee 类型 recipe，并绑定实际 callee 操作数；抽象 F 不需要也不允许虚构函数或闭包定义。按真实操作码选择调用点 local 的实际 occupant、借入 Address 所指的真实闭包目录或函数指针 SSA 值，不能取同型的另一 local 或同签名的另一 entry。实际 callable 类型、完整签名、where 前提、调用种类、entry 和资源行为仍分别检查。Dyn 成员派发继续要求完整实际 receiver、interface/member 和独立 FullW 证明。

## Dyn 与完整泛型

Dyn 的 Semantic 类型桥使用完整类型 recipe，不另建一个缺少 Closure、Opaque、Task 或关联类型的语法。桥从当前实际 Dyn owner 查找唯一原始语义作用域，核对完整身份、父链、原形参和成员序号。解释器在当前真实 W、Self、声明和方法实参中求值，保留连续 Bound/Invocation 层次，使用共享预算和循环检测。先出现的调用者、相同 Context 图像或无关的字面量不能提供证明。

Bound depth 只计算实际非零量词 frame：ForAll.lifetimes 或 Signature.bound_lifetimes 大于零才引入。ForAll1 中的零 bound Callable signature 不增加深度，外层 binder 为 depth0。零量词节点仍保留精确运输身份；Invocation 域仍按每个真实 Code/call 作用域独立，不能借跳过空 Bound frame 串入外部 Invocation 或抹掉 signature 身份。

外层 Address/Desc 的 alpha 对应只重命名完整外层发生域，保留路径、模式、名义声明与原始形参序号；不能改变子类型内在类、BS/Opaque 域或 header binder。两个内在类映射到同一图像仍是两个类。callable 的 Fixed 关系仅可比较同一真实 P/R 外层发生在同一 W 词法域中的合法图像，不能扩散成 TypeEqual、名义类型合并或 Invocation/Bound 实例化。

## 运输与资源

Runtime schema/Wire 和 Typed 身份域为 50，CT artifact 与依赖身份域为 10。完整原始记录进入 Wire、指纹、遍历、CT 类型 DAG、克隆和重定位；不支持的可执行消费者明确拒绝，不得抹掉 Some 目录继续处理。旧版本和未知版本拒绝，历史固定字节保留。

FunctionWithBody.protocol_contracts 直接保留原函数声明的完整有限协议关系，复用 FunctionProtocolContract。parameter 是该定义原有序 value parameters 的序号，包含真实 receiver，不包含之后增加的隐藏 ABI 参数。原入口状态是条件证明的假设；每个原调用须先证明真实对应位置满足入口，再使用已从所有原 body 出口证明的关系。缺表不提供 Closing/Closed，也不证明调用没有状态影响。原表从变换前的源声明独立生产，实际 Typed 表只能用于完整桥接比较及实际 CFG 核验，不能补原前提。原/实际参数通过真实 ABI 对应，不能按名字、相同类型或首个 owner 猜测。所有实际根、代次、Poll 来源、状态失效和清理仍独立检查。

同一原声明头的 foreign 完整保留原 ABI、符号、noescape、静态 callback、scoped provider role 与 driver transfer 承诺；resume_bound 保留原线程恢复选择。它们均从原声明独立产生并进入内容身份，不能由实际 Typed 表反补。External 与原 foreign 声明必须对应，实际外部入口及恢复选择须通过真实定义/ABI 映射核对完整原头；这些未信任字段本身不证明 ABI、借用、ThreadCallable、provider 行为或清理安全。

不可信输入在分配、克隆、哈希和缓存前支付完整节点、字节、深度与工作费用。冷、热、失败重试和重新请求均遵守实际请求预算；结构运输与语义解释分别收费，不能把运输费用测试当成 Analyze/Verify 语义预算证明。

显式 dyn `Context`/`Ctx` 使用 `SemanticDynContextRecipe::Shape` 保存完整原 retained shape；`Closed` 保留省略形式的空输入上界，`Type` 只从真实 whole 类型提取 Context。Shape 不伪造成一个拥有值或空 BS<Unit>，也不授予物理 port、根、loan 或能力。类域、全部 ports/retained/nominal 项及其 scope/kind 由各消费者独立检查；未知不等于空 Context。

CoreInferred 的 scope/call 始终选择真实 caller 实例及其原调用点。普通调用 actuals 的 formal 按目标方法自身完整混排顺序解释；仅当 actual 所在实例的 definition 被独立核为该原目标的真实词法 parent Scope 时，才按该 holder 的原混排顺序解释。后者从原 receiver 与 impl self_type 的新鲜统一求解，不得用方法同号 formal 代替。每次调用的 parent 要求、实际 provider/receiver 和实例 parent 关系分别核对；目标 where 不成为 caller 假设，其他容器或不相干 holder 不得触发这种解释。Typed 发布前所有请求仍须完全物化。

选定 CT 程序分离完整冻结背景 S 与可执行图 E。CtProgram.frozen_context 只运输阶段及原 query 发生的未信任绑定请求；验证必须另外收到实际不可变 CoreProgram 视图 S 和当前 expected binding。摘要或请求本身不代替背景、类型、来源或完成证明。S 保留所有用户原定义、Names、真实 caller Function(Core) 身份及完整参数/要求；未用定义与非活动分支继续全称检查。S 的函数和外部声明只供只读原域/ABI检查，不成为 E 的执行目标。E 的完整调用闭包、全部分支与操作仍须独立满足 CT 能力集合；VM 仅以 E 分发表解析函数。编译器新增 query wrapper 保留真实生成原体及唯一原 query/词法上下文关系，不能用当前具体输入替换用户原体或洗掉原义务。

Core 冻结视图由真实调用者建立不可变 context 和自身阶段 identity，不冒充 AST 冻结阶段的 epoch 或 generation。fresh freeze 的 epoch 由受检单调分配产生；内容/声明/生成摘要均绑定实际 S，并在预付完整节点、深度、字节与工作额度后计算。编译器版本限定的结构内容摘要不能用于 Type 或名义等价。普通无 S 的 CT 验证入口必须拒绝携带 frozen_context 的请求。选定 CT 包使用独立 ZILCTP2 标记；普通 Runtime Wire49 与 CT artifact9 身份域不因此变成执行授权。
