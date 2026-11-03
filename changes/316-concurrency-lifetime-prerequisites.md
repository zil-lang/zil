# 316：恢复后的并发、life 与 trait 实施前置

本片从现存源码重新取证。旧301–312交付对象和旧worktree没有恢复；旧311所述公共121/122及消费者123不在当前IR/Wire中，不能把历史描述、预留数字或模型当成生产实现。本片只新增最小源码、来源检查结果和后续合同，不修改接受性、ABI、预算、生产代码或公共schema。

规范依据为`_zil_design.md` §11、§12、§14、§18.1及具名附件；状态以实际源码和本片结果为准。`spec/interfaces.json`自身标明ContractToImplement，公共结构登记不是能力验收。完整1.4、ZUI、B0及产品门状态保持。

## 取证范围

`tests/language-readiness/recovery/`保存普通借用/静态trait/同线程scope正控、最近安全反例及life/trait/挂起/并发需求探针。`check.py`对每个原源执行生产`zil check --json`，记录源SHA、二进制SHA、规范输入SHA、实际诊断、独立核验次数和退出码。`pending`表示规范要求仍待实施，不注册成必须永久拒绝的测试。名称或普通record正控不代表Mutex、Atomic或SABI已实现。源码检查不等于原生执行、weak-memory证明、完整规范覆盖或性能结论。

本轮实际25源：6个既有正控Success且各有1次Verify；5个安全反例SafeReject，其中局部返回与相交借用由Verify直接拒绝；11个待实施源为9个Unsupported、2个SafeReject；3个life语义探针Success。后三个Success不是语言能力正控，也没有证明safe UB。完整结果逐源记账，初次缺LLVM动态库与collector空列表错误单列为基础设施/取证脚本失败，均已修正后重跑，不归因于语言。

当前runtime instruction decoder实际覆盖0–89、92–94及255（Unsupported），没有121/122/123。不能据编号空洞推导曾恢复过某个公共表示；§12状态机、线程、同步、稳定帧及其消费者均不能由该数字表补出。

## life：先核真实来源，再正规化表面名字

已有Address、参数来源返回及有限共享lending不能因补life语法而绕过。第一实际切片可以限制普通Direct函数的一层已有地址参数/结果，并将命名life约束映射到真实参数索引集合；life名称、Self接收者继承、fn指针签名和省略规则必须先写明。不得仅删除`<life ...>`或擦除`&'a`后宣称支持。

实际观察是：`type_ref`在降Reference时忽略lifetime字段，故未声明`'missing`、返回`'left`却实际返回`'right`输入、Self接收者上的未声明life均能到Verify；`<life a>`定义则被Expand的泛型守门拒绝。既有核仍按真正返回的地址来源检查，当前结果不能解释为标签已受检查或安全反例漏判。函数指针的一层地址参数另在Types被拒，HRTB在Expand被拒；不能靠擦除名字顺便开放间接调用。

- Expand：检查binder声明、重复/未绑定名字、词法遮蔽、返回life可依赖的输入集合；明确不支持的HRTB、where偏序及存储位置，完整访问未调用定义、别名、方法、函数指针及生成代码。life信息的擦除只能发生在约束已保留到核验入口之后。
- Types：构造候选来源合同时使用实际参数索引、完整类型和返回位置；不能把同一life名当成对象同一性或独占证明。多输入同life返回应覆盖所有可能来源，不同life不能被无条件合并。
- Analyze与独立Verify：各自从实际CFG重建来源、活性、重借父子、当前调用窗口和清理；检查返回来源属于声明允许集合。局部逃逸、错误输入来源、独占别名、销毁或替换后的旧窗口仍拒绝。只转交来源合同而不独立检查不够。
- Lower/backend：有限life可以无运行时字节，但布局/ABI不变不等于语义不用检查。原生正控须覆盖跨模块传返和最后使用后重借，反例只检查不运行；后续存储/可写字段必须保持不变型，不能缩短表面借期后写入短命对象。

最小下一施工分两段：先保留现有无life源码的正文来源推导，对普通Direct、显式life binder的共享地址输入/结果建立“binder→实际参数索引集合→允许的返回根”检查；匹配输入和多输入同binder的合法选择须接通，错binder来源和局部来源拒绝。若现有公共数据不足以独立保留/核对所声明上界，先向集成方提最小字段合同，不假装已有字段，也不在前端暗藏外部调用所需保证。之后接Self与fn指针/HRTB：Self正规化前后保留真实receiver索引和life关系；fn指针必须另有间接调用的借用/结果来源合同，不能复用只允许纯Copy的旧指针假设。

省略life仍沿当前有正文函数的实际来源推导，不能引入Rust式“多输入必须拒绝”的未定规则而误拒`borrow-selected-parameter.zil`。where life偏序、可写存储不变型、HRTB每次调用绑定及动态存储是后续明确义务；首片未开入口保持Unsupported，不能把合法程序永久登记为SafeReject。单独拒绝未声明life只是可报告的小诊断修复，不能替代本段真实binder/来源实现。

## trait：最小可实施扩展仍须完整名义身份

当前普通无参、无关联项的名义trait与具体本地Self实现为正控。后续可先做“有限具体trait实参 + 具体Self impl + 普通泛型bound消费”，再接一般泛型impl、关联投影、默认/泛型方法和supertrait；这是实施顺序，不缩减§18.1终态。

Expand/模块解析需保留trait定义模块、完整有序实际类型实参、真实Self和impl前提；同布局或同末段名不能合并。候选选择必须在全部前提成立后发布，关联等式不得证明自身前提；无调用定义同样做符号检查。coherence、所属、重复与方法歧义必须按完整impl身份核对；不得通过删除守门分支把`Vec<String>`的旧无参bound索引冒作求解器。Types仍检查代入后的完整签名、接收者、实际静态目标和约束；Analyze/Verify沿实际调用与责任路径独立核验。Copy/Transfer/Share结构资格、清理与同步权限不能由空trait或impl制造。原生验收要含跨模块、多实际类型、同布局不同名义、unused坏定义及预算N/N−1/fresh。

## Mutex/线程：真实资格和happens-before是同一切片的必要部分

当前没有生产线程join、Transfer/Share、Mutex/Atomic或弱内存同步核。`effects(spawn)`或自定义名为Mutex的record均不能补出这些能力。首片可以是结构化线程或作用域Mutex，但必须同时冻结并贯通以下合同：

1. 实际共享对象和环境中哪些字节被操作，执行资格放在哪里、何时交出/取得/归还；父线程仅保活而不被子线程操作/清理的外部对象不得虚构成子载荷。
2. Transfer/Share由实际数据、捕获环境、Cleanup、Resume、allocator、TLS和driver依赖共同建立，Pinned不授予线程安全；跨线程丢弃也在合同内。
3. Mutex lock获得不变量和独占访问，unlock归还；锁内借用不得跨unlock逃逸。真实acquire/release、失败/毒化策略、重入、非正常退出、已进入用户与最终清理必须固定。
4. 公共IR需由集成方同时登记producer、两核、wire/fingerprint、Lower、API/reference后端、目标可信适配和CT拒绝；未知tag保持fail closed。真实机器同步和弱内存论证必须与源面/独立raw反例一起交付，SC模型仅单列。

首片不必实现整套异步，但不能把同步FFI名字或仅声明公共opcode叫作并发完成。普通Atomic操作也不授予包围的非原子payload访问权。

## Suspend/SABI：稳定存储和最终静止不能由句法代替

§12终态保留init/poll/request_cancel/poll_close/finish_destroy、稳定帧、一次结果转交和PendingExit。后续首片须明确可挂起源形式、Direct/Suspend调用模式传播和显式执行根；静态无环子帧按真实并存区间布局，挂起递归需显式存储边界，不能暗加Box、RC或阻塞。

公共帧状态、结果槽、取消和清理边同时进入实际IR与独立核验。return RHS在源位置已经合法后，才可存入PendingExit；仍被子任务借用的可移动头必须先join。poll/poll_close不可同帧并发重入；Ready结果只移出一次，借自帧的结果阻止销毁。失败/取消/二次panic按现有规范收口，不用提前free换时限。

外部使用者必须包括submit、poll、poll_close、进入中的callback、可新开始的raw入口、timer、保留waker、DMA、OS登记及返回借用。准入与active登记必须线性化；active=0、取消请求或旧通知都不是释放证明。先获得后端no-future/稳定登记根保证，再排空所有实际用户和借用，最后处理结果责任并回收。弱内存、FFI/DMA及驱动承诺分别验证；不得用可能已释放的裸frame指针实现“稍后检查过期”。

## ZUI前置和验证边界

当前同线程结构化`c_with_callback`已能覆盖持续同步事件循环；返回静态无环境C入口的handle也不证明外部捕获环境存活。安装函数返回后保留捕获环境需要真实稳定存储、关闭准入、注销排空与最终清理；不能放宽`c_callback`接受closure来绕过。同步组件组合可继续推进，不等待完整SABI；返回登记、跨线程平台入口和异步I/O分别由真实消费者提出上述义务。

本片检查命令、结果和已知不足在`docs/validation/recovery-capabilities.json`。Rust四目标：本片只提供可重放缺口和安全反例，没有新增运行机制、运行/内存测量、完整框架维护比较或编译速度结果；至少一项产品突破仍未证明。
