# 原始语义作用域与实际程序

本合同规定原始泛型、生命周期和语义捕获的完整表示。运输记录是不可信程序操作数，不是来源、能力或安全证明。Source、Types、Analyze、Verify 和 CT 消费者分别承担自己的完整检查；字段存在或能往返编码不代表检查通过。

## 跨阶段语义不变量与按操作组合的判定规则

原始定义及其站点标识源码操作，实例及定位后的站点标识这次执行图上的操作。同一操作可以同时要求类型关系、存储来源、访问权限和退出清理。发布阶段应逐项说明这些事实属于哪个定义或实例、作用于哪个操作数或结果路径、在什么条件下成立。Types 绑定真实发射；Analyze 与独立 Verify 依各自的输入重建并检查这些义务。后端布局、复制与清理消费与当前输入绑定的验证结果。类型相同、布局相同和未出现义务记录，都不能单独提供来源或权限。

| 操作边界 | 应保留并重新核验的关系 |
| --- | --- |
| 调用与返回 | 依操作定义区分静态声明选择、读取值和借用位置。用真实 callee、形参序号、实际操作数及结果路径代入关系。嵌套包装按相同规则再次代入。 |
| 构造、投影与分支 | 用声明、字段或活动分支辨认子位置。保留父来源及其权限；所有可达前驱共同决定汇合点可用的保证。 |
| 闭包创建与调用 | 将创建处的真实捕获绑定到该子实例；薄函数地址绑定声明身份。调用检查实际 callee 和环境。 |
| 移动、替换与退出 | 跟踪实际被占用的位置。泛型结果跨入调用者时按实参解释仍具符号类型的拥有叶，保留已经确定的来源和 Task 根；每条退出边兑现余下的清理义务。 |

解构赋值和模式匹配只求值一次被匹配的表达式。对于 `let (child,) = packet.inputs`，在取值前记录此次投影的绑定和字段路径。取走拥有值后，模式从产生的元组绑定 `child`，原字段在本代保持空缺，直到成功补回。引用模式保留原槽位，借用检查随每条匹配边验证访问模式。有 `?` 的表达式只在成功边提供对应载荷，失败边清理已经初始化的占用者。

可重赋值绑定的逻辑值是当前占用的元素。若实现用单元素数组或单字段 Package 保持清理位置，初始化表达式只求值一次；声明、实际构造输入、StorageLive 和初始化 Store 必须属于同一次绑定。源码读取和转移选择该元素；替换的右侧成功后才交接该位置的责任，右侧失败时按实际初始化状态清理，移出的旧叶子不再欠清理。保留结构的完整类与成员投影；物理形状相同的用户聚合仍由其自己的原声明解释。

拥有闭包可以把上述资源绑定整体移入捕获字段。每次构造的实际移动操作数须对应原自由变量，子函数入口的 BindPackageCapture 将该字段绑定到本次调用的局部视图。该视图的字段借用对外仍指向该捕获；局部视图的初始化、失效与每次替换的责任按实际控制流核对。再次移入子闭包时沿原构造关系逐层核对。

Suspend 函数清理内联聚合时，源路径使用声明字段序号和活动分支。Backend 依同一布局将它映射到实际地址及初始化标志。提前移出的字段将义务交给移动的目的地，兄弟字段保留各自的义务。正常退出、错误退出和取消都从帧中保存的进度继续。子清理返回 Pending 时保留当前占用者，返回 Ready 后推进到下一项。关闭父帧之前，每个仍被拥有的叶子须恰好完成一次清理。

Task 工厂在直接返回真实创建结果时，可以在公开 Context 中以工厂形参序号标识借入来源。创建指令以及 worker 的完整 Context 始终使用 worker 的原始形参序号。每个被持有的输入须由实际创建指令的有序操作数追溯到当前工厂形参的同一代值。仅在这条返回边上，按该对应关系替换完整子路径开头的形参序号，并核对全体端口的顺序、权限、叶类型、类、结果关系与名义发生。遗漏端口、借同型兄弟替换来源或合并独立端口均不能建立该返回关系。后续调用和传递遵守各自实际声明的公开 Context。

逐操作检查所需事实由实际操作和它所引用的声明决定。改变无依赖声明的顺序或新增一个没有依赖边的函数，不能改变合法程序内既有调用的判断。拒绝一项关系时，诊断应指明预期、观察结果及相关发生点。活跃借用冲突在证据可用时给出创建处、冲突操作和仍需该借用的位置。直接输入 IR 时保留函数、块和指令坐标，具备源码映射时同时给出源码位置。

### 新类型与新语法的设计义务

每个基础类型须定义其逻辑值和物理位置。结构成员由什么身份选择，哪些成员含借入或拥有值，以及投影如何取得子值、权限与清理责任，都属于类型定义。对和类型分别定义每个分支，对乘积逐成员定义，对引用区分存放引用的槽与被引用的位置。空聚合、零个生命周期形参以及没有物理字段的形参，也须给出各自的语义。

每个语法操作须定义输入和结果的角色、求值顺序、需要成立的条件，以及每条可达出口上的事实。语法糖应给出到既有操作的展开及原发生点与实际发射的对应；有副作用的接收者与实参按语言规定的次数求值。若引入新的基本操作，则在其 IR 定义处完整列出实际值与位置操作数。消费者从该结构读取操作数，并依本操作的前置条件独立检查身份、活跃借用、状态变化和清理。借入视图的替换例如要同时说明旧视图的终止位置、新视图的来源以及仍活跃的父借用。

对每种新增构造，应从一个调用者和一个嵌套使用者检验上述定义能否组合，包括适用的错误出口和最近的非法替换。没有借用或清理效果的标量操作直接沿用相应的空效果规则；在含有未知实参的类型上，空效果需要由原声明约束推出。新增构造的一个操作尚未定义时，编译器应在相应边界明确报告缺失的语义义务。

### 可调用值与失败出口的组合

把持有者中的字段传给泛型形参时，字段表达式求值一次。独占借用须从可变的实际持有者或独占父借用取得，以完整有序字段路径确定新的借用位置。若子字段也是 Package，借用保留其完整类型和声明域，子项保留各自的来源、借用和清理责任。转发已有引用可直接传递该引用。转发引用所指持有者中的子字段时，先对该字段显式再借用。联合字段指令和逐字段指令须逐级对应同一组声明与序号，直到实际局部或实际父地址。将其中一个序号改为同型兄弟字段必须拒绝。

调用可分解为位置投影、接收者取得和签名代入。字段投影按名义定义和物理字段路径选择仍初始化的值。相同的字段类型、函数签名或 ABI 不改变选择的字段。每一次调用都重新检查实际接收者及其活跃权限，非消费 `FnMut` 在取得独占访问后临时调用；发生在 helper 内的再借用仍属于原借用窗口。泛型 helper 对原声明的任意满足约束的实参核验正文。调用者可用已经证明的公开 `impl FnMut` 签名满足约束，实际返回的闭包仍须证明它的参数、全部返回路径和资源行为。闭包返回 Code 时，该返回值保留其完整原签名；调用第一个 Code 的执行约定不足以描述对其结果的后续调用。

函数与闭包的既定 `E!T` 返回类型为每条正常返回和传播出口提供同一个上下文。普通表达式按[错误联合构造](error-union-construction.md)唯一注入；已构造的联合保持标签和载荷。传播错误只结束当前调用及其再借用，并履行该退出边的责任。非消费接收者的环境仍由原持有者持有，直至原持有者转移或退出。替换先形成新值，再清理旧位置并发布新值；右侧失败时，实际未移动的旧值仍有原责任，已转移的值按其实际新责任清理。借用、清理和身份分别从实际程序核验。

| 原操作 | 前提与实际输出 | 保留的关系与出口责任 |
| --- | --- | --- |
| 泛型代入 | 从原形参与实际操作数的完整类型求解，再验证原要求。前端只知道返回签名时，可保留实际调用待 Types 求解。 | 保留原定义、完整签名和实际来源。另一次实例化或相同类型不能替代本次实际操作数。未求解实参不能进入 Typed。 |
| 字段投影 | 从已初始化的父位置按原声明的有序字段选子位置。组合投影逐层复用同一规则。 | 保留根、父权限、字段路径和活动所有者代次。父或同路径替换、取走、清理时检查相交窗口。同型兄弟字段各有自己的来源。 |
| 借用和再借用 | 核对实际位置、有效来源和权限。独占调用必须从实际独占接收者取得窗口。 | 保留父 loan 和有效期。子窗口不得升级权限或活过父窗口，结束时释放该子窗口。 |
| 调用和返回 | 用实际 callee、接收者和有序实参代入完整签名，分别核正常返回和传播错误。 | 返回的可调用值仍可查询自身声明、完整签名和接收者模式。非消费调用保留持有者的环境责任，临时再借用随调用结束。 |
| 分支合流 | 汇合所有可达边的事实。 | 保留每条边仍可能存在的借用和清理义务。只有每条边都成立的单一来源或权限才能作为确定事实。 |
| 替换和错误退出 | 按源顺序准备新值，成功后清旧位置并提交。 | 右侧失败沿实际完成的步骤清理，尚未转移的旧责任保持。成功提交产生新的所有者代次，旧借用不能改指新值。 |

无捕获的闭包在需要函数指针的位置可形成 `FunctionRef`。转换保留原定义、实际创建函数、源码发生点、完整签名及实际入口，结束临时环境槽的占用。后继闭包可复用该槽。查询较早的函数指针时，沿其创建点和实际 `FunctionRef` 指令找回入口；带捕获的闭包另需按当代环境和入口核对捕获与清理。原工程分别通过结构体字段、数组、分支、元组和 helper 消费这些指针。[函数指针原工程](../examples/function-pointer-pipeline/main.zil)

`c_callback(symbol)` 通过原词法符号选择外部调用约定的函数。发生点是生成真实 `ForeignFunctionRef` 的操作，类型检查及独立核验按该目标核对完整 C 签名。同宽的整数参数也须保留各自的标量身份。[C 互操作源码](../tests/integers/ffi.zil) · [原生及 Raw/Wire 对照](../crates/zil-driver/tests/integer_domains.rs)

阶段摘要须可分别回答“哪个原声明、哪个实际发生点、哪个路径”和“该点的来源、权限及清理是否已独立证明”。定义、实例、发生点和 SSA 生产指令的位置可按不可变输入索引共享查询。SSA 生产位置只回答哪条指令定义了值；支配、借用、清理及替换后的有效性由各核重新证明。动态事实的查询仍须区分程序点、路径和所有者代次。缺失证据的关系为未知。

## 原始构造发生与联合推导

省略混排实参的普通名义构造保留 `RecordConstructor` 的真实声明 ID、原 Construct site、完整原字段及其顺序。无 wanted 的构造也须从全部字段联合推导 Type/Const/Life，并证明声明的全部要求；不可由首字段形状、物理布局或具体调用者猜实参。真正欠定的 phantom 实参或矛盾应报推导失败，不能填充虚构实参。

构造的生命周期发生使用独立保留路径 `Construct.occurrence_path + [u32::MAX - 1, 0, mixed_ordinal]`。`mixed_ordinal` 是原声明完整 Type/Const/Life 混排中 Life 形参的序号。每个真实构造按原声明登记这些发生；原 owner、site、操作种类、声明身份与完整序号必须逐项对应。该目录不属于 Call 的 MAX 后缀，不适用于 pattern、类型资格或 sealed intrinsic；目录存在本身不是借用或相等证明，旧发生 ordinal 不重排。

已有显式、wanted 或内层不变位置的精确解优先。其余 Life 只有在全部相关字段的原 outer σ 本身均为同一原 rigid Input（完整声明身份与序号相同）时才保留该 Input；仅 upper 指向 Input、具体 outside 图像相同或混入 local σ 均不够。其余使用本次构造的独立 region r，分别证明每个实际字段来源到 r 的有向关系，保留全部来源、父 loan 与清理责任。不得据此加入来源之间的 Eq，或改变完整内层 κ、N、phantom、名义及字段身份。两核独立完成这些检查。

## 原始定义与实例

定义保留跨模块限定身份、真实词法父链、原始混排的 Type/Const/Life 形参、隐式生命周期的原始发生位置、const 的完整载荷类型、全部 where 条件、语义自由变量和原始签名或名义类型体。未使用形参、phantom 参数和非活动枚举分支不能删除。

普通函数使用 `FunctionWithBody` 保留完整原义务 body，复用 `CoreBlock<SemanticTypeRecipe>` 及同一套 Core 操作，而不是保存求解器声称成立的约束表。全部原分支和未使用定义都要检查；原程序中尚未展开的 iterator 与 spread 操作也必须保留。`External` 必须对应真实外部声明；`Declaration` 只适用于真实抽象 trait 成员。签名和 sites 齐全不能把缺 body 当空 body；仅含这些字段的旧 `Function` 不得成为完整原定义证明或降 tag 的绕行入口。

原有序 value 参数表保留名字、可选注解和 passing mode，其稳定绑定身份是 definition 与 ordinal。`receiver_parameter` 只选择真实原参数，不能伪造编译器添加的闭包隐藏 receiver。`site_nodes` 显式把原 SiteId 绑定到同一原 body 的 Expression/Statement NodeId，要求节点唯一、角色和操作匹配；原 `occurrence_path` 仍是 Source 身份，不能重解释成改写后的 Core 路径。闭包自由名字在真实 parent 的原 Closure 节点环境中封闭，不能从 Typed local 名字、相同序号或任一具体实例猜绑定。

`names` 只记录真实原声明绑定：完整路径、Value/Type/Lifetime 命名空间，以及完整 recipe 或真实 definition 引用。词法父 impl 的形参使用真实 ancestor 引用；原参数和局部名字按 body 的真实遮蔽关系解析。名字表不能增加 where 前提、改变被引用 formal 的种类/carrier，或从字符串猜同号 lifetime；使用中缺失或歧义必须拒绝。它不代替常量原定义或来源证明。

原 Call 的混排 actual vector 保留显式 recipe 或 `Infer`。每个 Infer 的种类和 Const carrier 从真实目标 formal 取得，在 caller 原定义域建立新鲜变量，依据原 callee 签名、原表达式符号类型和真实原结果上下文求解，再证明目标全部要求。目标待证明的 where 不能先成为 caller 假设。无解、歧义、循环、发生检查失败或预算耗尽明确失败，不以具体实例 actuals 或第一个调用者猜解。Types 求解后，Analyze 与 Verify 各自独立核验；Typed actuals 必须全部物化，不能残留 `CoreInferred`。

例如 `same<V>(x:V,y:V)` 的调用者 `outer<T,U>(x:T,y:U){same(x,y)}` 需要在原域证明 T=U，不能因唯一具体调用为 outer<i32,i32> 而通过。Const 与 Life 的偶然同图像也不能补原前提。原未使用函数、特化后被消去的分支和 generic value 的 Copy/trait/清理要求同样受此规则约束。

原函数 signature.call_kind 是固定、尚未核验的调用约定上界。Source 的显式约定原样保留；未显式选择时，候选由完整原 body、真实调用绑定及 Callable/清理合同产生，不能从首个具体实例反推，也不能将未解析的边默认成 Direct。各消费者独立证明原 body 的全称效果不超过该选择，并核实际 Typed ABI 与之相等，不能私自升级 Direct 或降级 Suspend。显式 Direct 的函数执行 task_yield 必须拒绝；显式 Suspend 的空 body 仍保留 Suspend。task_create 引用挂起 worker 不等于创建者执行其挂起正文；IntrinsicCall.effects 同样需要真实封闭 intrinsic 身份和操作证明。封闭递归调用图的效果从真实来源闭包推导，悬而未决的调用边不作零效果。

缺省 `FnOnce` 角色的调用约定包含本体及剩余环境的完整消费。若源码没有显式选择该角色的执行约定，Source 只可在发布原 Γ 前，遍历该角色的全部真实消费、别名及转发端点选择候选：必须至少有一个真实消费见证，追踪全部别名和转发关系后，所有实际消费端点均有独立成立的 Suspend 执行依据，且该完整关系中不存在已声明的 Direct 角色限制，才能选择 Suspend；否则保持 Direct。真实 `task_yield`、外部已固定的 Suspend 合同及原 Γ 已知 Task 的必要剩余清理可以提供独立依据；待提升角色自身的调用、无种子的转发环、未解析调用和抽象 F 缺少 SyncDrop 都不能提供此依据。未消费的 F 仍须独立满足原清理义务，不能由候选上界替它获得 SyncDrop。

候选完成后，角色的执行上界固定进入原 Callable signature、公开接口及其依赖/cache 身份；各消费者再分别全称核验原定义及实际 Typed 调用与完整清理。调用者不能把已固定 Direct 角色升级成 Suspend，外层 Suspend 也不能豁免内部 Direct 合同。正文改变推导出的公开上界属于接口变化；薄 Code、FFI 和显式 Direct 的约定不参与此推导。该规则仅推导已有 FnOnce 角色的完整操作上界，不授予其他效果泛型、同步 Drop、借用来源或挂起执行根。

语言封闭的 trait 使用 `SemanticBuiltinTrait` 原身份：`BuiltinImpl` 保留真实 self recipe 和原 impl 词法域，`Builtin` 要求保留实际被约束类型。不能给 Drop、Transfer、ClosedEnvironment 等捏造用户 nominal declaration，也不能把封闭 impl 改成 inherent impl。名字、原要求或某个 scope 的存在均不授予能力；每个消费者仍须证明该 kind 合法适用、真实 provider/body、清理与跨线程等具体义务。Send/Sync 的原拼写保留，其合法性或别名语义不能仅凭同一枚举族推导。Fn/FnMut/FnOnce 继续使用包含完整签名的 Callable 要求。

实例指向真实 Core 函数、Core 闭包发生点、Typed 函数或词法作用域。原始形参序号与内在类序号不同；完整类型已有的内在类不能因实际生命周期图像相等而合并。缺失语义目录是 Unknown，不能解释为零类。语义自由变量也不能伪装成物理捕获字段。

每个实际 Type/Const 特化保留真实 rigid 代表程序。原始生命周期在该代表中保持独立；具体生命周期分组必须指向同一特化的真实代表，并独立证明完整 CFG、操作数、类型、资源、闭包和清理的替换对应。不能只比较 ABI。`bad<a,b>(Marker<a>)->Marker<b>` 必须在 rigid 检查失败，即使所有当前调用者碰巧选择 `a=b`。

Rigid 使用原生命周期的新鲜身份及完整原声明允许的前提，不是无条件两两不等。定义检查只使用 definition/formal 的原 requirements、真实词法前提、已核环境要求和原签名关系。显式 LifeEqual 可以证明这些身份相等；Outlives 不能当成相等，调用者碰巧同图像不能成为定义前提。原 Callable 前提只能按完整 P/R/Receiver、Fixed、Invocation/Bound 关系推理，不能额外授予 TypeEqual、全类合并或 Bound 实例化。

`Callable{ty, kind, signature}` 内的 Receiver 发生属于该 `ty` 的本次真实接收者访问窗口，不是包围该要求的工厂函数或方法接收者。沿用[闭包接收者借用合同](closure-receiver-borrows.md)，只有非消费的 Direct `Fn`/`FnMut` 入口或绑定该已独立核验入口的角色可以使用此窗口；没有显式地址输入时，省略生命周期的根 shared Address 结果可以要求此窗口，唯一显式地址输入沿用原输入省略规则，多输入歧义仍须显式关系。这个签名局部发生不增加捕获 Type 的类，不生成无来源的 Fixed 或 Invocation，不改变参数、结果内部类及实际捕获的完整域；薄 Code、Suspend 签名和 `FnOnce` 不因此获得环境借用结果。

原定义必须分别证明每个实际返回的来源满足该接收者上界。实际签名用已有 `StorageBase::Receiver` 与 Result 的显式关系连接真实接收者访问域；调用须绑定真实 callee 的存储、借入和发生身份，并保留其父 loan 与代次。`make()()` 的接收者来自一次真实调用表达式，不得在原声明中伪造局部或重复求值；实际临时存储存活到完整表达式的最后使用。原要求和签名关系只建立待证义务，不能代替真实捕获、每条返回路径、替换失效或退出清理的证明；独立 Static 来源仍按自身规则证明。

真实闭包入口若从共享捕获返回独立的 Fixed(a) 来源，实际 Result 绑定及捕获完整域仍保留 Fixed(a)。在原定义全部返回和实际捕获来源均已独立证明该来源可覆盖本次接收者窗口时，Direct、非消费入口可以另列空路径的 Receiver Invocation 访问头；这两个绑定不同，也不建立 LifeEqual。满足较短 Receiver 结果角色时，调用端须先完整验证原 Fixed 结果、真实 callee 及其父 loan，再形成本次调用窗口内的输出视图，同时保留原来源映射。只有 Receiver 行、相同 Type 或候选角色不能触发此适配；没有完整来源证明的 Fixed 返回仍按其原合同处理。

Type 实参的完整内在域在 callee 自身实例域中封闭。`IntrinsicClass(TypeArgument(Ancestor(0),完整形参序号),[],k)` 是已核完整类型给出的原子内在锚，不能递归读取自己的 classes，也不是实际来源证明。每个真实 Call 在 caller 已成立的上下文求值实参，独立证明全部要求和实际 callee/entry/header/env/资源对应，再建立边界映射；不能使用目标正假设的 where 循环自证，不能把 caller 作用域或 SSA 偷放入 callee，也不能回写 rigid Life 实参。已有 whole T 的 N2 不因两个图像相等降成 N1；新求值的 Nominal recipe 只可使用当前合法前提证明的相等归一。

精确类相等与 callable 角色满足是不同判断。Static、合法缩短、Outlives 和 Fixed 局部适配不能进入全局 LifeEqual，也不能成为 fresh Nominal 归一依据。同一 Static 输出 F 可以满足 `Fn()->&a` 与 `Fn()->&b` 两个允许的角色，但不能据此把 Marker<a> 变成 Marker<b>。只有原显式 LifeEqual 或独立成立的精确相等规则才能用于该归一；已有完整 T 的类数与序号仍不变。

闭包的 `SemanticEnvironmentFormal.source` 在真实词法 parent/creator 的定义域解释；`SemanticEnvironmentInstance.actuals` 在实际 parent/creator 实例的真实闭包创建站点求值，再封闭为 child 的 ghost 实参。这两处的 `Ancestor(0)` 指该 parent 当前作用域。creator 的 Value/Local 序号不能转而选择 child 的同号操作数。必须核对实际闭包、创建指令和 parent 对应，多次创建逐实际对应检查，不能选首个调用者提供证明。环境绑定不回写或替换实例原有的 rigid Life 实参。

若闭包自己的原 body 直接使用自由名字，其隐式借用发生点取该绑定的首次直接读取。若该绑定只经内层闭包转发，外层在构造实际内层环境时使用它一次。此时 `SemanticImplicitLifetime.occurrence_path` 为 `[42, nested_constructor_node, environment_formal]`。节点属于当前定义，序号选择当前定义的词法环境形参。父创建点的真实输入、内层创建点所见绑定和相应 `MakeClosure` 的实际操作数必须逐层对应。转发保留原借用权限和有效期，清理仍由各自实际拥有的叶子承担。

`SemanticEnvironmentFormal.kind` 与其 source 使用相同的 parent 定义域，包括 Const 的 carrier 和 Type 的 requirements。复制来自更外层定义或环境的 kind 必须显式重定位到当前 parent 域，不能盲用旧 Ancestor(0)，也不能按 child 的相同序号查找。child 使用的环境槽必须已经在父域核过并封闭。

Typed 类表达式的 Environment 引用只读取沿真实词法路径找到的已核 sealed Life 槽，并检查原 kind 与完整序号。Life 槽没有类型内在域，不能伪装成 `IntrinsicClass(Environment(...),[],0)`；也不能为了可表示而增设物理捕获或 BorrowRoot。

## Core 到 Typed 的完整求值

Source 不得猜测 Types 尚未分配的 ClosureId、完整类型内在域、const 载荷或常量位模式。`SemanticTypeValue::Core` 保存精确词法实例和完整原始类型 recipe；推导类型还绑定真实 module/function/expression/occurrence。`SemanticActual::CoreRecipe` 保存待代入的完整 Type/Const/Life 实参表达式。无类型字面量和关联 const 必须先得到真实载荷类型再求值。

Typed 的实际 SSA、local、签名、捕获、字段和容器元素必须一致保留 canonical 完整 Type。聚合的非零持久域包含 phantom 类，必须进入实际 BorrowedStorage qualification。例如 Marker<a>{i32} 使用 slots 为空、binder_count=1、nominal_arguments 保留原 a→class0 的 BorrowedStorage<Record>，不能用 plain Record 加 Some 旁表代替。Resolved.ty 必须匹配被绑定的完整实际 Type，life 从该 Type 与真实 nominal/opaque 目录独立重建，不能作为矛盾的第二合同。类型比较、实例键、缓存与谓词不能先剥除 qualification；跨请求缓存还须解析真实声明身份。

qualification 不增加物理内存，不生成 BorrowRoot、地址、loan 或资源权限。根 Address/Desc 保留原表示，由外部 life 描述其外层发生；内部 BS 完整保留，不额外制造 BS<Address> 或直接双 wrapper，已有 nominal BS 即使 N0 也不能随意剥除。

逻辑类域与物理借入叶是两份不同的事实。例如包含共享静态字节片和整数的 Tuple 可以没有可变来源类，但实际 Address 字段仍须完整保留。完整值的结构解释递归保留每个字段的类型及来源义务；N0、空 slots 或相同布局本身不证明 Static。原定义、真实构造与捕获、后续写入、投影及返回分别核验该物理叶的来源，不得借结构域形成跳过这些检查，也不得把这种按发生核验的值退化为裸 Type 的通用零域或 Copy 证明。

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

普通 `Value` 形参从 opaque callable 实参推导时，以实际值发生形成完整类型实参。可见的 Fn/FnMut/FnOnce 签名只约束调用，不能作为实例键，不能抹去工厂声明的身份、混排实参或捕获域。所有约束该形参的实参与返回位置仍须一致。放入 Package 后该字段承担完整环境的责任，调用者沿声明字段序号和真实借用根读取该环境。Value 约束不授予 StoreElement、Copy 或 SyncDrop。

显式借入 callable 后立即调用时，借用表达式只求值一次。调用需核同一借用的完整路径、实际 occupant 与共享或独占权限。投影和再借用保留父窗口及所有者代次，只有相交的移动、替换或清理使该证明失效。构造或参数准备失败时，按实际已转交的责任结算。已确认的旧字段在新值准备成功前仍承担原责任，所有正常和失败出口都核对实际活跃的字段与环境。

真实调用的默认资源描述参数使用原 Call path 加 `[u32::MAX, 5, value_ordinal]` 标识本次调用的资源描述发生，ordinal 按完整原值参数顺序计算，包含已声明 receiver。临时资源实参仍以原拥有表达式的完整类型求解。实际调用只可用为该参数准备的拥有临时位置生成描述，并核对同一路径上的初始化、实际描述操作和消费操作。获得的外层短借用只覆盖本次临时位置的存续期，内在类及清理责任各自保留。普通非移动捕获在真实拥有位置获得描述时同样依实际来源核验，移动捕获保留拥有类别。

同一签名的 Invocation 类按原声明的完整形参顺序、再按结果中的首次发生编号。正规化得到的隐式资源描述和显式切片都参与这个顺序。原来重复使用同一类的发生仍共用该类。两端在实例化时按这一对应核对实际参数。

从 Package 字段借出字节视图时，`AddressField` 选择原字段，`DescribeAddress` 保留该位置，`ByteBufferBorrow` 从实际描述建立对已初始化前缀的共享或独占借用。返回视图的期限须落在该输入所有者及父借用的有效期内。共享输入只支持共享视图。字段所属的 Package 在成功和失败出口都继续承担该字段的清理责任。Analyze 和独立 Verify 分别核对实际形参根、字段序号和访问模式，同型字段各有自己的绑定。

## Dyn 与完整泛型

Dyn 的 Semantic 类型桥使用完整类型 recipe，不另建一个缺少 Closure、Opaque、Task 或关联类型的语法。桥从当前实际 Dyn owner 查找唯一原始语义作用域，核对完整身份、父链、原形参和成员序号。解释器在当前真实 W、Self、声明和方法实参中求值，保留连续 Bound/Invocation 层次，使用共享预算和循环检测。先出现的调用者、相同 Context 图像或无关的字面量不能提供证明。

Bound depth 只计算实际非零量词 frame：ForAll.lifetimes 或 Signature.bound_lifetimes 大于零才引入。ForAll1 中的零 bound Callable signature 不增加深度，外层 binder 为 depth0。零量词节点仍保留精确运输身份；Invocation 域仍按每个真实 Code/call 作用域独立，不能借跳过空 Bound frame 串入外部 Invocation 或抹掉 signature 身份。

外层 Address/Desc 的 alpha 对应只重命名完整外层发生域，保留路径、模式、名义声明与原始形参序号；不能改变子类型内在类、BS/Opaque 域或 header binder。两个内在类映射到同一图像仍是两个类。callable 的 Fixed 关系仅可比较同一真实 P/R 外层发生在同一 W 词法域中的合法图像，不能扩散成 TypeEqual、名义类型合并或 Invocation/Bound 实例化。

## 运输与资源

Runtime schema/Wire 和 Typed 身份域为 50，CT artifact 与依赖身份域为 10。完整原始记录进入 Wire、指纹、遍历、CT 类型 DAG、克隆和重定位；不支持的可执行消费者明确拒绝，不得抹掉 Some 目录继续处理。旧版本和未知版本拒绝，历史固定字节保留。

FunctionWithBody.protocol_contracts 直接保留原函数声明的完整有限协议关系，复用 FunctionProtocolContract。parameter 是该定义原有序 value parameters 的序号，包含真实 receiver，不包含之后增加的隐藏 ABI 参数。原入口状态是条件证明的假设；每个原调用须先证明真实对应位置满足入口，再使用已从所有原 body 出口证明的关系。缺表不提供 Closing/Closed，也不证明调用没有状态影响。原表从变换前的源声明独立生产，实际 Typed 表只能用于完整桥接比较及实际 CFG 核验，不能补原前提。原/实际参数通过真实 ABI 对应，不能按名字、相同类型或首个 owner 猜测。所有实际根、代次、Poll 来源、状态失效和清理仍独立检查。

同一原声明头的 foreign 完整保留原 ABI、符号、noescape、静态 callback、scoped provider role 与 driver transfer 承诺；resume_bound 保留原线程恢复选择。它们均从原声明独立产生并进入内容身份，不能由实际 Typed 表反补。External 与原 foreign 声明必须对应，实际外部入口及恢复选择须通过真实定义/ABI 映射核对完整原头；这些未信任字段本身不证明 ABI、借用、ThreadCallable、provider 行为或清理安全。

原资源类别在创建 signature 和 implicit 目录之前正规化。对真实声明或封闭内建身份确认的资源 payload R，默认裸 R 是 Desc<R>；合法 give 参数、create/give 结果与显式 Managed<R> 选择一次 Managed<R>。显式 &R / &mut R 借实际 Managed 头；显式 &Ref<R> / &mut Ref<R> 借描述槽，保留内层 Desc 发生。普通完整 Type formal T 的 &T 始终是 Address<T>：若实际 T 已是 Desc<R>，不得在语法代换后把它重新解释为裸 &R；Managed actual 也不能被降成描述。重复 Managed 或 Managed<Ref<R>> 拒绝，Package 不因拥有字段变成资源。错误联合的 create/give 只作用于规则指定的成功载荷。

Self 是原名义 Self，按其发生位置和真实 impl scope 解释；不能套用普通完整 Type formal 的替换规则。已证为资源主体时，直接 Address<Self> 使用 Managed 头，显式 Desc<Self> 或 Managed<Self> 内部使用 raw payload；裸 Self 的默认描述与 give/create 模式分别处理。未确定的 trait Self 保持符号形式；provider、非 receiver 参数、default body 与 where/Callable 签名采用同一解释，不把所有 Self 全局改成 Managed。正规化不得从实际 hint、布局相同或首个同型实例取得资源身份。

Head 地址和描述槽地址都必须来自真正被借 place；描述槽不能借类型注解升级为 owner 位置。统一类别不开放所有操作 profile，未完成的组合仍明确拒绝。Head 地址不授整头 Copy、任意写入、移动、销毁或 Thread 能力；其存续期、父 loan、完整内在 N、phantom、retained、状态和实际清理责任保持独立检查。

仅局部初始化绑定的根注解允许以真实 raw 资源 recipe 保留未选模式的资源族约束，供 direct placement 使用；它不是无 wrapper 的运行时资源值。原 initializer 的已解完整类型与声明模式决定该绑定为 Managed 或 Desc，不能从实际 Typed hint、首个分支或仅“是一次 Call”决定。显式 Ref/Desc 保留描述类别，显式 Managed 必须得到真实完整拥有结果；普通完整 Type formal T 不重新选类别。只透明处理该根的 Group，不递归剥除 Tuple/ErrorUnion 等内部成员的 Desc；无 initializer 不以将来赋值猜模式。确定后绑定类别固定，后续赋值不切换。发布的 binding type、query 和 Typed 均须已消解为明确类型，并保持原 initializer 的完整生命域、来源及绑定身份。显式 ascription 先核完整类别，不能像 Group 一样穿透它重新取得内部工厂的拥有模式；显式描述借自临时工厂且无合法保活位置时拒绝。

不可信输入在分配、克隆、哈希和缓存前支付完整节点、字节、深度与工作费用。冷、热、失败重试和重新请求均遵守实际请求预算；结构运输与语义解释分别收费，不能把运输费用测试当成 Analyze/Verify 语义预算证明。

显式 dyn `Context`/`Ctx` 使用 `SemanticDynContextRecipe::Shape` 保存完整原 retained shape；`Closed` 保留省略形式的空输入上界，`Type` 只从真实 whole 类型提取 Context。Shape 不伪造成一个拥有值或空 BS<Unit>，也不授予物理 port、根、loan 或能力。类域、全部 ports/retained/nominal 项及其 scope/kind 由各消费者独立检查；未知不等于空 Context。

CoreInferred 的 scope/call 始终选择真实 caller 实例及其原调用点。普通调用 actuals 的 formal 按目标方法自身完整混排顺序解释；仅当 actual 所在实例的 definition 被独立核为该原目标的真实词法 parent Scope 时，才按该 holder 的原混排顺序解释。后者从原 receiver 与 impl self_type 的新鲜统一求解，不得用方法同号 formal 代替。每次调用的 parent 要求、实际 provider/receiver 和实例 parent 关系分别核对；目标 where 不成为 caller 假设，其他容器或不相干 holder 不得触发这种解释。Typed 发布前所有请求仍须完全物化。

选定 CT 程序分离完整冻结背景 S 与可执行图 E。CtProgram.frozen_context 只运输阶段及原 query 发生的未信任绑定请求；验证必须另外收到实际不可变 CoreProgram 视图 S 和当前 expected binding。摘要或请求本身不代替背景、类型、来源或完成证明。S 保留所有用户原定义、Names、真实 caller Function(Core) 身份及完整参数/要求；未用定义与非活动分支继续全称检查。S 的函数和外部声明只供只读原域/ABI检查，不成为 E 的执行目标。E 的完整调用闭包、全部分支与操作仍须独立满足 CT 能力集合；VM 仅以 E 分发表解析函数。编译器新增 query wrapper 保留真实生成原体及唯一原 query/词法上下文关系，不能用当前具体输入替换用户原体或洗掉原义务。

E 可携带 S 原定义的完整只读副本；重合身份仅在与实际冻结 S 的完整原记录结构相等时接受，解释仍以 S 为准，不得替换或遮蔽 S，单一输入目录中的重复身份仍须拒绝。此类副本不迁入 S 的实例、站点或执行权限；执行目标仍须来自独立核验的 E 调用图，新增 query wrapper 必须具有 S 中不存在的定义身份及精确入口绑定。选入 E 的合法 helper 可引用 S 原定义，但不得因此直接调用 S 分发表。

Core 冻结视图由真实调用者建立不可变 context 和自身阶段 identity，不冒充 AST 冻结阶段的 epoch 或 generation。fresh freeze 的 epoch 由受检单调分配产生；内容/声明/生成摘要均绑定实际 S，并在预付完整节点、深度、字节与工作额度后计算。编译器版本限定的结构内容摘要不能用于 Type 或名义等价。普通无 S 的 CT 验证入口必须拒绝携带 frozen_context 的请求。选定 CT 包使用独立 ZILCTP2 标记；普通 Runtime Wire50 与 CT artifact10 身份域不因此变成执行授权。
