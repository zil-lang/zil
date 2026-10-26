# 资源交接：公共 helper、保存回调与 ZUI 前置

本页记录当前draft16/schema13的资源能力及历史接续；语言规则仍以`_zil_design.md`第2/7/8/9/10/12/14/15章为准。正式全门身份和数字只读[evidence/final-summary.json](../evidence/final-summary.json)，切片受测提交分别保留在各自收据中；能力切片完成不替代B0或完整语言验收。

## draft16 当前接续：组件由可保存环境拥有

077–079完成完整有限Package的显式move捕获。工厂可以接收完整Package，返回拥有它的Fn/FnMut/FnOnce；调用者通过既有同步helper借用或消费环境。每个捕获唯一的词法view直接引用真实环境数据、shell和字段责任flag，不创建第二份拥有者，也不在每次调用时重置状态。Fn禁止直接/间接写；FnMut在全部普通及错误返回恢复完整；FnOnce部分消费后由caller按捕获顺序、字段声明顺序清余项。Trap仍abort。

[owned_component](../examples/mainline/owned_component/README.md)是当前首选消费者：五模块Window事件环、三模块生命周期和真实fd入口共20固定输入，ZIL/Rust各O0/O2共80次行为对照。它将draft15的字段替换/移出/补回与draft14的工厂返回组合起来；完整Package普通值ABI保持，环境布局随schema13携带持久字段责任。源级模式推断、Analyze、raw/wire Verify、CT和机器输出边界分别有精确审核收据。

当前Package至少含一个直接Managed<Record>字段，其余字段有限Copy；不含嵌套拥有Package、拥有闭包字段、Desc/Address或整体自定义Drop。callback显式参数与结果仍有限Copy；一般闭包嵌套组合、动态擦除、借用逃逸、拥有错误载荷尚未开放。默认借入Package捕获不以隐式拥有替代，源码必须显式move。

下一片的可并行前置是：字符串/动态容器所需的元素拥有和分配合同；平台指针FFI的来源、长度及外部调用边界；更一般的拥有闭包/Package组合和回调关闭合同。各自先冻结具体接口，再拆前端、独立Verify与机器/库消费者；有共同表示依赖的部分顺序整合。已有同步组件可继续作为回归，不等待图研究改核。

当前Subscription没有保存会在关闭后访问Model的OS回调；MSU只证明这个应用的清理顺序。后续若Subscription持有访问Model的回调，必须在销毁Model前取消并排空，或证明后续Call已关闭；不能直接照搬当前顺序。窗口、GPU、文本/字体、OS注册/注销、异步取消排空及完整ZUI仍未完成。

076在冻结schema10上补足dense-direct控制并否定当前整态共享的普遍净收益；下一研究应将当前schema13字段/环境事件先做完整影子迁移，再测事件支持下的子态更新及可证切口关系。旧实验不改标成新语言覆盖，当前生产NoGo。以下章节按原版本保留，涉及“下一片”时以上述当前接续为准。


## draft14 历史接续

069/070已经实现普通工厂返回自包含Copy/Managed环境，调用者在同步事件间保存、移动和Fn/FnMut/FnOnce调用；原生五模块20输入、四变体80次行为对照。工厂输入同样仅Copy/Managed，返回身份/精确kind/字段cleanup由独立Verify重建。新片不保留已退出工厂的栈借用。

后续优先并行推进：拥有字段替换/部分移动的独立状态、必要字符串/容器、平台FFI的真实指针/回调合同。现有公开非逃逸helper与新工厂可组合；一般借用逃逸、嵌套环境、OS回调注册及取消排空仍未完成。以下历史建议不再把这个有限自包含工厂返回片列成待从零开始。

## draft12 新增：完整拥有包

060–062已用独立公共表示/机器路径、前端Analyze和Verify实现有限原子拥有包；当前首选运行入口是[package_ui](../examples/mainline/package_ui/README.md)。完整包普通Value移动/参数/返回，字段Desc有精确来源；准备失败按真实临时逆序，完整包按声明序清理。禁止成品部分移动和替换的范围内用一份whole责任状态，未公开部分完成对象，没有预先添加通用字段标记。

非逃逸helper已在draft13贯通，面向ZUI的后续前置是一般拥有组合与可保存事件处理器。逃逸、嵌套包、部分替换、拥有错误载荷及平台指针FFI仍按依赖推进。有界文本事件环已跑通，不能称为完整图形框架。

## draft13 新增：具体环境公共 helper

064–066已将上一片列出的五类验收需求接入真实源码、独立核验和机器执行，当前主入口是[helper_ui](../examples/mainline/helper_ui/README.md)。一个F:Fn/FnMut/FnOnce模板参数在实际ClosureId已知后实例化；实际环境和定义模块身份保留，既不擦成薄函数指针，也不靠在caller内联私有正文绕过接口。

Core保留CoreCallableParameter；TypedFunction携带CallableParameterContract；Verify从CFG及捕获类型独立重建；driver才导出公开合同和指纹。Hold为捕获的地址窗口，Call保留Address/Desc目标依赖，writes是保守能力上界，Cleanup区分Owned/Borrowed，有限轨迹记录0/1/2次请求模式及Return/Trap。Return包括Copy E!T任一标签，不声称接口区分错误标签。

借入拥有Fn两次后原环境仍可调用；&mut F更新同一环境；实际Fn可适配FnMut或FnOnce，实际FnMut可适配FnOnce。按值未调用环境清理全部字段，消费后清理剩余字段。后续实参?失败时，已准备callback及give参数由实际临时按逆序清理，callee未进入。机器层仍保留内联环境和真实指针；非拥有只读Fn保留原按值ABI，不能据此复制拥有环境。

UI字段Desc的view经跨模块helper两次使用，最后使用后允许整包移动；未来仍有使用时拒绝。独立raw/wire还拒绝伪造次数、缺Hold、错误调用能力、借入环境Drop/Move和隐藏捕获地址冲突。25个输入×ZIL/Rust O0/O2的100次行为执行与源码/raw测试分开计数，不作为性能或完整安全结论。

### 当前边界与下一片

当前一个具体环境参数、每路径最多两次callback，FnOnce最多一次；callback显式参数和结果有限Copy，额外helper参数只能是有限Copy、Managed give或完整Package。FnMut helper接&mut F，参数本身仍不可写。无helper循环/递归/转发、环境返回或保存、一般trait/dyn/opaque。未使用模板仍检查合法性；队列、缓存、深快照和原始合同均有实际配额，限额失败不记为安全接受。

下一片优先让环境成为真正可拥有、可保存和可替换的值，并在公共接口传递关闭与后续调用条件；与此无前置的有限容器/IO包装、应用模块提取及平台标量/指针FFI可按各自真实能力并行。具体源码边界与机器布局仍需先行提案，同批前端、独立核验和失败清理，不用隐式RC/Box掩盖存储资格。

保存回调另有真实前置。当前view只在一帧内使用，Subscription没有保存会继续访问Model的回调，所以MSU只证明当前应用。如果将来Subscription持有Model描述或地址，关闭前须取消并排空或证明后续Call已关闭，才能销毁Model；不可直接复用当前次序。保存/替换合同必须覆盖旧Cleanup、新Hold和正在执行的调用状态。

图研究063继续与主线独立。BDD节点回收、变量顺序、融合提前消元和公共区域摘要各自给出同精度基线、完整成本和原始反例；只有通过生产反例与净收益门的候选才能进入安全核。当前driver的lazy dump是实际成本定位后的工程修复，不是图理论突破，也没有减少Verify。

## 已经贯通的链路

| 阶段 | 已实现的输入与输出 |
|---|---|
| 源码与展开 | `resource R`、隐藏Drop入口、`give`及`result(create/give)`保留原合同；普通资源捕获为Desc，拥有捕获在创建时转交责任。新增真实`E!T`与表达式后缀`?`贯通，普通`return expr`在期望联合类型下按唯一精确匹配注入。两侧同时匹配拒绝歧义，传播要求相同错误类型。 |
| CT与类型 | 错误联合两侧递归限制为有限Copy，支持已有Copy聚合、枚举及嵌套联合；拥有错误/成功载荷尚未开放。原Fn/FnMut/FnOnce、Desc写效果和Copy快照合同不变。纯CT允许已核验的scope标记并计入真实step，运行期错误联合/传播仍不进入CT可执行子集。 |
| TypedCFG与分析 | `MakeErrorUnion`、`ErrorTag`、`ReadErrorUnion`与`ScopeEnter/ScopeExit`表达实际控制路径。`?`错误边先取错误载荷，再用清理快照退出；成功边的待清理列表保留。拥有实参及Consume callee在全部参数成功前存入真实临时，后续实参失败仍清理。Analyze另行重建scope、存储和责任。 |
| 独立Verify | 从原始CFG重建scope栈、StorageLive归属、初始化/责任、Desc根和写效果；核对错误指令类型及SSA支配。拒绝漏清、重复清、乱序、跨scope伪造、未完成对象完整teardown和已转交结果重复清理；不读取Analyze证明。原始wire同样重新核验。 |
| Lower与LLVM | payload/环境内联并保留真实责任标志。拥有Fn借实际位置，Consume结束清理剩余字段，未调用环境也清理；局部按逆成功初始化顺序，环境按捕获首出现顺序，清标志先于用户Drop。错误联合内联`{tag,E,T}`，非活动载荷置零；错误投影先检查tag，包括Unit。 |
| 原生入口与错误返回 | 同步错误沿普通Return返回，native `main -> E!i32`成功使用结果退出码，`E!void`成功为0，错误为1且不隐式打印。退出前完成当前合同内的合法清理；checked trap仍为进程abort，不具有错误传播的清理保证。 |

同步错误历史主入口[error_cleanup/main.zil](../examples/mainline/error_cleanup/main.zil)由四个真实ZIL模块组成；[样例说明](../examples/mainline/error_cleanup/README.md)列出15个输入、输出与退出码，`main.rs`为独立手写Rust对照。056在ZIL/Rust O0/O2四个二进制上共执行60次，涵盖创建前后失败、待提交give实参、待调用FnOnce环境、Copy字段准备失败、Consume正文错误及循环退出。另有5次源码入口/聚合/函数指针原生执行。干净受测提交为`452d13c39df482943e09f3d6c969a2c376742bbc`，见[056收据](../evidence/impl/056/acceptance.json)。这些是行为与清理顺序对照，不是运行/内存性能结论。

[056独立审核](../evidence/reviews/056/review.json)另保留在`b340d45675588f2ed13c86b7083bf7a23fadf945`上的12次原生近邻执行及4/4源码suite：包括嵌套宽载荷、未使用的非活动Unit投影仍trap、较早实参/Consume callee与较晚局部的清理顺序、部分移出捕获后的剩余清理。最终452d只补测试与说明，相关生产源码逐字相同；独立实测身份没有改写成新SHA。058的非作者审核由另一方完成，见[058审核](../evidence/reviews/058/review.json)。

旧[resource_closures/main.zil](../examples/mainline/resource_closures/main.zil)继续保留三模块12输入×ZIL/Rust O0/O2的48次行为对照，见[053收据](../evidence/impl/053/acceptance.json)。它使用用户`Outcome`枚举和普通Return，是原资源/闭包基线；本轮`E!T/?`的证据来自056真实源码，不能倒记到旧枚举路径。

051最终生产受测提交`c53fbcbe508202f642d8daa034de377ae3ce39cc`，交付`289734aedfd8dbbd51a7718b8cfa59d173814be1`仅追加证据；[051收据](../evidence/impl/051/acceptance.json)记录新16项及包含旧030/047的45项限定回归。旧047初始`c5b8a04`的13项/27项记录及后续字段上下文修复新增2项各保留原身份；当前047共15项，其中两处旧断言按已实现能力及Analyze提前拒绝迁移，没有删除错误源反例。

054 `resource-closure-native`已进入现有Rust hunt/issue通道：4项限定测试与无注入8 cases/32 actions通过，尚无真实生产缺陷发现。省略调用注入验证检测/重放链；051 Copy快照误拒是手工边界发现。详情见[054收据](../evidence/impl/054/acceptance.json)，不要把手工回归或受控变异记为长期探索自然发现。

## 当前有限边界

资源自身的payload字段仍限有限Copy；完整package已可同时拥有多个这样的资源和Copy字段。闭包显式参数/返回仍限有限Copy。资源可转交给普通give参数函数，但资源型闭包参数/结果、Desc返回/聚合保存/逃逸、嵌套资源/package、资源替换和一般拥有聚合、Address<Resource>尚未开放。带借用的逃逸闭包、嵌套拥有环境和已有闭包环境作为捕获项的一般组合仍未实现。

**作用域与责任证明分开。** 本轮scope合同覆盖实际StorageLive局部及根scope参数：合流的scope栈与活动存储一致，本scope退出前所属存储均结束。它不证明每个拥有SSA值的词法归属；合法块结果可以跨内层ScopeExit转交，拥有SSA的全函数守恒由既有独立资源检查负责，Error/Return不能遗留待提交参数或环境。普通Return也可携带错误联合，不能把Return标记解释成成功tag。[058实现说明](../changes/IMPLEMENTATION-0058.md)保留这些边界及跨scope结果/漏清的近邻反例。

源码前端统一输出scope；含scope、错误类型/操作或package类型/操作的raw IR进入完整scope检查。无错误且无scope的旧raw夹具继续经过原责任、StorageDead和Return核验，不声称获得细分词法scope证明。未核wire或前端提供的scope标记本身不制造权限。

**构造失败只覆盖已实现形状。** 当前资源字段均为Copy，所有字段成功才建立完整Managed；失败时没有完整对象的Drop责任，先前已完成的资源/临时仍清理。完整package在全部字段准备成功后才提交，其失败清理对象是已完成的准备临时。一般公开部分成品、带teardown的分解、拥有错误载荷、一般escape、panic unwind、cancel、异步frame、并发和SABI仍未完成。外部fd取得/关闭继续由显式unsafe wrapper合同保证，整数复制不证明外部所有权。

**布局与预算按实际工作记录。** `{tag,E,T}`同时保留两侧payload槽，并非`max(sizeof(E),sizeof(T))`的紧凑联合，也不表示与Rust的Result ABI或内存成本相等；本组源例没有隐式malloc、Box、RC或由Desc推导的noalias。纯CT访问新增scope指令照常扣step，`#(zero())`单查询12步通过，双查询12/23步ResourceLimit、24步通过；没有提高生产默认限额或放宽产品门。最初旧预算夹具失败、非法`3i64`被Lex提前拒绝及其合法类型注解修复均保留在056收据中。

## draft11 分包建议（完整package部分已由060–062接续）

沿已定`package`、非Copy Value和callable合同推进，不重新派发已经实现的`E!T/?`或本地Fn/FnMut/FnOnce。先固定一个有界、同步、非逃逸切片：有限拥有载体保存已支持的资源/环境，静态已知callable通过普通helper调用；错误载荷继续有限Copy。整体转交与准确清理先贯通，字段部分移出、替换和初始化失败必须有独立状态表示后才开放。表中是后续派工建议，不把尚未建立的公共表示视为已实现。

| 包与写集 | 具体工作与可运行完成标准 |
|---|---|
| 公共表示与机器路径：ir/schema/lower/backend | 集成方先冻结拥有字段的身份、完成状态、整体转交、待提交值和清理顺序，以及静态callable的Hold/Call/Cleanup接口。实际内联布局与逐字段责任标志同批落地；未完成载体不能执行完整teardown。先贯通一个完整载体，再扩部分移动/替换；不预先承诺拥有ErrorUnion或动态擦除ABI。 |
| 前端与分析：syntax/expand/types/analyze | 消费同一表示，按已定package/Value规则输出真实持有、移动和清理CFG；在静态类型已知、非逃逸helper中承载Fn/FnMut/FnOnce调用及有限Copy错误返回。逐项覆盖创建、调用、移出、未调用销毁和后续参数失败；既有Desc/Hold不能因存入环境而丢失，Analyze保持独立诊断。 |
| 独立核验：verify | 从原始CFG与类型另行重建每字段完成/责任、callee环境依赖和结果转交，覆盖正常、错误与循环出口。加入raw/wire最近反例：复制拥有字段、漏清/重复清、未完成字段读、部分移出后完整teardown、过期Desc和结果转交后重复清理；保留现有storage scope与全函数SSA责任分工。 |
| 可复用库与源程序：独立库模块/examples/driver tests | 可先用现有签名抽出资源工厂、有限Copy校验/错误传播helper及显式外部Drop wrapper，至少让两个真实调用方复用。新的回调/拥有载体API只在上三包贯通后接入；保留四模块15输入基线，再增加每个新构造/转交点的成功与错误轨迹，比较ZIL/Rust O0/O2的真实资源效果。 |
| 现有质量工具与集成：roam/driver | 沿用resource-closure-native和现有原生测试域，按真正开放的源语义扩生成、重放与最近反例。作者限定验证、非作者审精确提交后集成；未实现输入明确拒绝。控制注入、人工反例和自然hunt发现分别记账，不新建质量平台。 |

一般闭包返回/保存/逃逸、递归拥有结构、动态容器/擦除、拥有错误载荷和panic/取消分别依赖更完整的Hold、来源、逐字段责任或退出协议；不能由上述有限高阶切片外推完成。可复用库先交付现有合同能支持的实物，不等待全部高阶能力，也不通过额外Box/RC掩盖表示缺口。

## 独立并行研究：有限完整联合关系与切割

059在冻结schema8真实TypedIR上保留每个原始点的有限完整联合状态，直接raw枚举oracle与候选不共享提取器、转移函数或BDD。mono连接全部事件位方程后消元，cut按最后关联提早消元，wire增加满足新鲜唯一输出等前置的恒等/取反代换；全函数仍为联合固定点，未实现跨CFG区域摘要。43个函数的903次正式全关系核对通过；两个限额入口明确非Pass。相关分支保留了生产合流丢失的关联，当前候选的完整成本仍无稳定优势，NoGo入核。固定结果与独立审核见[059结果](research/059/RESULTS.md)和[审核](../evidence/reviews/059/review.json)。

下一步应先解释四个独立条件触及BDD节点上限的具体阶段，再验证责任表示、变量排序或构造方法。将token编码为唯一owner位置可能减少组合，但必须保留重复责任的非法态或独立检测，不能靠表示静默抹去反例。跨区域联合摘要和可独立核验的局部证书尚未实现；TypedIR语义覆盖、关系等价、资源唯一性/出口结账/清理顺序仍是不同证明责任。Desc查询目前保守不区分guard，动态同址重叠Unsupported，不外推一般借入模型或内存管理突破。

### 保留的055历史研究

055已从真实源码的TypedIR只读抽取控制/资源/闭包事件，在有界、签名受支持的函数上进行两个不同层次的shadow核对：每个原点的完整Desc/借入闭包may-liveness，以及保留身份、分支标签、调用、清理与出口的同一带标记事件图。它没有实现一般联合资源状态关系，也没有把050人工精确关系求解器接入生产。相同活跃关系不保证资源间分支关联相同，事件图核对必须另行保留；研究答案不制造Verified权限、不改变生产接受集合。

055主计时冻结在draft10/schema7；schema8的`?`、scope及错误返回另交[功能收据](../evidence/impl/055/schema8-integration.json)，实际四模块19函数、853图点和20错误退出全部核对，不能重标旧采样。保守隐含failure sink是抽象扩张，不代表实际错误次数或已经覆盖schema8错误清理。[055正式结果](research/055/RESULTS.md)固定3d4375a源码/驱动：19源3192样本的cut/block总成本配对中位比均大于1（1.007–1.147，多数波动跨1），当前恒等点候选暂不入核。后续应先隔离生产责任/newer等耦合热点，再研究真正语义边界关系，并计入抽取、计划、求解/恢复与独立核对的完整成本；不能只比较一个逐指令单体基线。

历史收益与退化保留各自身份：

- [046生产优化](PROOF-CUT-PERFORMANCE.md)：旧ZIL基线`493edcb0`、最终候选`a0532bda`。反向256链Analyze/Verify约45.8×/45.1×，完整check为1.615×；小例长跑完整check仍慢9.1%–21.6%，不能用局部阶段收益覆盖退化。
- [050固定研究结果](research/050/RESULTS.md)：计时源码`bef7d9f3`、冻结证据`bdb1576`。窄链16约217.7×只相对研究单体基线，dense_12约慢5.34倍，小例/闭包/owned也退化；宽观察边界的差异不能算切割收益。050-R1修复`e18fb242`未重采性能，旧1512条样本不归到修复或055。

059已补有限抽象内的完整联合语义抽取与核对；一般联合语言语义、可替换生产核的端到端净收益、冷/增量与同能力Rust对比仍未完成。有限may-liveness实测和059局部消元均不能替代这些结论。

每包保持独立worktree/target/output，作者限定测试、非作者审精确提交、集成方跑必要正式门。原始日志和机器产物放唯一out/runs，仓库只留小摘要、反例和哈希；沿用既有派单与交接，不增加审批流程。Rust安全目标增加有限同步错误清理实证；运行/内存、表达维护、完整编译成本的整体同级与至少一项净突破仍待同语义证据，见[Rust目标判断](RUST-GOALS.md)。

## draft14 研究交接

067用原始有序事件保留重复give/capture故障前态，再做可逆位置编码；完整有限关系核对成立，性能当前NoGo。下一方向为token局部carrier域与直接有限域搬移。068小型纵向raw共享已落生产，独立Verify保持；没有确定的普遍整请求速度提升。精确代码/原始测量/预算更正见067和068收据，不重写旧055/059/063结论。

## draft15接续

072–074实现无整体Drop包的直接字段取走、条件旧清理、补入/替换和Copy字段写入。准确的局部字段债务、旧Desc实例、RHS准备失败与完整SSA/ABI边界由Types/Analyze/独立Verify/LLVM同批消费，schema12。更新组件的20输入/80次native与真实fd仅是当前有限语义证据。下一包继续拥有环境与组件组合、容器/字符串及平台接口；不能把同步保存环境直接当OS注册/注销排空。

071局部域+直接同步转移在冻结schema10完整关系接口上取得明确总成本收益；候选仍保留完整联合态，未做无界分配/MDD/区域摘要。先强化dense-explicit基线，再按真实行数/相互作用宽度决定表示与切割，不直接替换生产核或改写067负结果。详见071报告和DRAFT15-ARCHITECTURE。
