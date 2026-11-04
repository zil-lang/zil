# 342 普通泛型借用与终态组合

施工基线：`77eea5bde60b1737b1a7f885ff549ed17308c3de`，派工权威：主树 `.task/impl/342.md`。

## 先行变更提案：定义期真实窗口

普通未实例化模板目前只检查可变 place 的类型资格；现有定义期 loan engine 仅在 owning callable State profile 中开启。最近反例为 `fn bad<T:Copy>(p:&mut T)->T { let a=&mut *p; let b=&mut *p; *a }`（即使从未调用，第二次独占借入也必须使旧 `a` 无法再次使用）。等价有用正例在旧 `a` 最后使用后才建立 `b`，允许原父引用在 child 最后使用后恢复。

实施拟复用现有计费的 loan identity / parent / invalidation / prepared argument / CFG join / backedge 引擎，覆盖所有普通模板；从实际参数引用模式建立独立 referent root，不从 life 名称、同尺寸或具体实例制造权限。普通局部 place、读写、移动、重借和调用窗口统一进入该引擎。type/life binders 的名字空间仍分开；有 life 时返回来源继续用真实参数索引核对声明上界，mutable result 还需真实 exclusive 来源。无 life 的模板同样不得丢弃别名错误。

随后在同一完整符号窗口证明上准入 `T:Copy + life` mutable 输入/结果。Analyze 与 Verify 仍各自重建具体 CFG 来源、返回权限和调用窗口；Core/Wire 现有来源合同不赋予证明位。新增扫描、loan、父链、根集与分支复制在扩张前累计既有请求预算，不增加默认限额。既有正例、最近反例、未调用定义、raw/Wire、实际 O0/O2 和 N/N-1/cache 依适用范围验证。

HRTB/借用 FP、借用存储/lending、显式 dyn 和一般 §5.2 状态后续按实际表示依赖继续实施；此阶段备份不代表该项终态完成。任何公共 schema/AST/Wire 变化先交由集成方发布，不能通过删除 Unsupported 或新增权限位替代证明。

## 首个实际里程碑：未使用普通模板别名核验

移除 State-only lane suppression，同时扩展来源演算：每个实际 Reference 形参由其真实 mutable 模式建立外部 referent root，root 与本地指针槽分离；普通值 local 为实际存储根。普通 local 的值读取、index 读取也参与窗口失效，不能靠缺失访问绕过已有贷款。父子链、同时实参窗口、分支版本化、循环 backedge 与预算复制复用原引擎。

实际限定命令（source `/workspace/.zil-env/env.sh`；独立 `CARGO_TARGET_DIR=/workspace/.zil-env/target342`、jobs1、debug0、incremental0、专用 TMPDIR）：`cargo test -p zil-driver --test generic_regressions --test cross_function_borrows --test impl197_mixed_state --test review184_storage --test impl200_independent --offline -- --nocapture`，先前单项失败按下列来源分账后分别完整重放：generic_regressions33/0、cross_function_borrows37/0、impl197_mixed_state14/0、review184_storage4/0、impl200_independent10/0。不是正式全量。

原 retained `mutable337/known-incomplete-unused-generic.zil` 字节不改；从未调用和有调用均在 Expand/SafeReject/E-GENERIC-ARGUMENT 拒绝。有调用路径原断点为 Verify/E-BORROW-CONFLICT，旧用例仅迁移预期阶段/诊断。首次回归运行保留该预期失败。另一次旧 exact budget 2358→2356 的暂时少费由保留 callable binder 访问收费修正，旧2358边界不变；不以调高限额处理。

331原源Expand N757→779，来自新真实参数贷款建立；其它符号/别名源本轮 N1414/275/1391，各N−1 ResourceLimit/fresh N一致。322原源99/95/714/993、337具体源143/124/1178/4818保持。现有借用目标含真实两核、Wire、原生、缓存回归；本里程碑新增2测试覆盖8份未用近邻反例与6份合法最后使用/分支/循环/独立参数正例。

Rust四目标：未用泛型别名责任增加可重放安全证据，完整语言证明仍待后续；沿既有地址ABI，没有新增隐式分配/RC/锁，未测同语义Rust运行/RSS；普通泛型无需实例化才能发现此类错误，尚无框架维护成本数据；新loan来源与复制实际累计收费，没有编译速度比较。实质增量为真实未使用普通mutable模板的既有漏洞闭合，不代表产品四目标合取或产品净突破；原product-gates/B0/完整语言状态不变。

## 第二里程碑：CopyT+mutable life 与完整返回窗口

规范扩展由集成方 `0feb9324c` 冻结，本分树按授权接为 `0954193ed`。实际 CopyT/life 定义先执行已扩展的普通贷款证明，再准入 mutable 来源；定义期显式返回上界按实际参数索引核对，mutable 结果还检查源形参真实模式。静态 helper 调用按该 callee 自己的 binder 和实际参数位置代换，返回窗口只取允许来源；参数位置是语义身份，未用 Span 或名称作为身份。

架构复查还指出 no-life 局部逃逸和 call-as-place 空来源两个近邻。所有引用返回现核已建立的外部参数根，局部值根不能逃逸；嵌套地址签名和借用在未用定义中仍明确 Unsupported。实际 helper 调用产生的贷款经 place 投影、index/range 端点及赋值 RHS 保留，不能被后续标量表达式清空。Store/ByteBuffer Desc 形参和原语建立真实外部存储根/借用窗口，使旧合法 lending 消费者继续通过；新局部存储不因此变成外部根。

新增9份真实 CopyT/mutable-life 源完成 Clang/API × O0/O2 共36次构建和执行，全部打印42/exit0，覆盖返回/重借/同拼写type-life/slice/记录字段/独立label/共享降权/分支/真实defer更新。6份未用反例、raw/Wire错误上界、跨模块CT及live/restored缓存的当前body/权限/调用方冲突检查通过。新增一般模板近邻包含call直接投影、局部逃逸、store来源、嵌套地址。两份337旧混合mutable源逐字节保留，从Unsupported迁为Success；原局部标注life仍Unsupported。

实际 `cargo test -p zil-driver --test cross_function_borrows --test generic_regressions --test impl197_mixed_state --test review184_storage --test impl200_independent --features llvm-api --offline -- --nocapture` 的生产变更已完整通过42/35/14/4/10（后续两项budget/cache测试在各自既有目标限定重放通过；完整重放交给集成方）。Clippy含两个受影响目标、llvm-api、`-D warnings`通过。mixed select的Expand/Types/Analyze/Verify N=1669/248/2227/7927，每个N−1 ResourceLimit且fresh N一致；331原Expand在完整返回校验后N786（阶段前757），其它符号/别名N1413/282/1398，原322/337具体与legacyState2358保持。默认限额未升。

首次测试的真实失败保留为记录：新raw测试按实例函数名查找失败，改用实际声明上界识别实例；nested `&&T` 被lexer当作&&，改为合法嵌套类型拼写 `& &T` 后准确触及Expand拒绝。它们是测试入口修正，不改语言行为。

下一公开表示提案：`FunctionLifeContract { binder_count:u32, parameter_lives:Vec<Option<u32>>, result_life:Option<u32> }` 与 `Type::BorrowedFunctionPointer { parameters, result, life }`，canonical dense indices、Wire/fingerprint类型tag31；语法 `for<life a> fn(&'a T)->&'a T`。薄指针可复制，签名资格和实际目标/返回根需独立核；每次间接调用重新代换实际根并核全部窗口。共享patch由root统一发布，未完成消费者前不把表示登记算能力。


## 第三里程碑：调用局部借用薄函数指针与无损 CT

公共表示由 root 的 `46e0df677`、`71568b5a8`、`707e38e44` 发布；完整当前规范快照 `5ed669791`。实际消费者接受一层 `for<life a> fn(&'a T)->&'a T`，普通 `fn(&T)->R` 按规范 elision 生成相同的单 binder 关系。标签按实际输入首次出现规范化，alpha 改名、binder 声明重排和未用外层 binder 不改变类型；无借用的 vacuous forall 还原普通函数指针。Analyze 与 Verify 各自拒绝 raw gap/未用尾项、缺失或越界标签及无 exclusive 输入的 mutable 结果。

FunctionRef 的真实函数必须完整模式/参数/结果一致、无环境、非 foreign/teardown。两个核分别重建完整 CFG 返回根并核实际目标返回集合包含于声明上界；指针关系不是证明位。CallIndirect 每次用当前实际输入根重新代换该上界，同时准备所有参数窗口，保留实际 reborrow 父链和结果贷款。代码指针可 Copy 不复制 Address 权限。薄指针 LLVM 表示仍为 ptr，slice 返回保留既有 fat-slice ABI；捕获闭包不能退化为薄指针。普通捕获为空的地址闭包、字段内指针、返回指针和跨模块调用均到达真实机器。

CT Type/Syntax、read view、构造和 Wire 均保留实际 DAG 子 ID 与完整 canonical 关系。构造器借用而非 Copy；实际关系 clone 在扩张前收费。重建成递归运行时类型或源码拼写的每条实际展开边累计工作/临时内存，避免小 DAG 通过重复边无界放大。CT 类型关系只表达结构；生成代码仍经过普通 Types/Analyze/Verify。新增 CT value Wire 精确 roundtrip 及 u32::MAX binder metadata ResourceLimit 控制。

九份真实源 × Clang/API × O0/O2 = 36 次构建/运行全部打印42/exit0；包含独立 label、fresh 调用、mutable slice、无捕获闭包、字段/工厂返回及 CT Type/Syntax 重建。六份最近反例检查错误实际返回根、父写、重复独占参数、跨调用旧结果、shared/mutable 不匹配和捕获逃逸；非函数 forall 与未声明 label 拒绝。raw 与 decoded runtime Wire 独立检查错误目标根、错误关系和实际参数重叠；live/restored semantic cache 重核当前 callee body、caller 权限及接口关系。alpha 改名保持 export/signature/contract/result-origin 接口；源码实现 fingerprint 允许变化。

实际完整限定命令为 `cargo test -p zil-driver --test cross_function_borrows --test generic_regressions --test impl197_mixed_state --test review184_storage --test impl200_independent --test impl033 --test impl260_compiler --features llvm-api --offline -- --nocapture`，最终分别49/35/14/4/10/10/37，零失败；最后 CT clone 收费和 Wire 控制后重放受影响 cross_function_borrows49/0 与 impl260_compiler37/0。记录 `/workspace/.zil-env/terminal-20261104/342-fp-complete-tests.log` 和 `342-fp-ct-final-tests.log`。不是正式全量。Clippy 同七目标、llvm-api、`-D warnings` 通过，单独记录。

新 SELECT 的 Expand/Types/Analyze/Verify N=162/133/1487/5938，N−1 ResourceLimit/fresh N 相同。Signature 中实际标签 clone/canonical remap 的新增工作使原322 Types95→98、337 Types124→129、mixed Types248→258；其它阶段分别保留99/714/993、143/1178/4818、1669/2227/7927，331原Expand786和legacy State2358保持。CT checker 预检现在也累计实际指令扫描；旧 CT 源的 exact 查询测试重新测量并核 N−1/fresh，不增加默认限额。

首轮真实失败保留：测试把闭包误写为 Rust 的竖线语法，改为现行 ZIL `(p:&mut i64)=>p`；该合法源随后暴露 source inference 仍按 Copy scalar 处理地址，已补真实地址约束路径。CT别名初次被旧 alias resolver 的未绑定 life 检查误拒，现用实际 binder scope 独立绑定。跨模块接口最初错误要求 alpha 改名后 implementation fingerprint 也不变，收窄为规范要求的语义接口。完整既有 impl260 编译另暴露独立 harness 的模块路径差异，生产 helper 移入两种布局共同包含的 compiler 模块。两份 impl033 原源字节不变：借用参数薄 FP 从 Unsupported/E-UNSUPPORTED 晋为 Success；其余签名/捕获反例仍拒绝。

Rust四目标：两个独立核的真实 root/window 与 canonical raw/Wire 反例增加有限安全实证，完整语言/异步安全仍未建立；仍为薄 ptr，没有隐式 Box/RC/锁，未测同语义 Rust 运行/RSS；原借用代码指针与 CT 类型组合可直接表达，完整框架维护收益未测；真实新增工作累计收费、默认限额不升，没有 Rust 编译成本优势结论。至少一项有限增量为 borrowed thin pointer 经 Type/Syntax、普通函数/闭包和 native 双后端直接组合；产品四目标合取、B0及现有 product-gates 不因此晋升。

仍待后续：泛型 `F:for<life a> Fn...` 的普遍定义义务、借用存储/一般 lending、显式 dyn、一般 §5.2 状态。本分树未合 TaskYield 实现，集成需重放普通与借用 FunctionRef 到 Suspend 闭包的 source/raw/Wire 反例；不能以当前薄指针合同宣称 Suspend 安全闭合。独立审核等待精确干净候选。

## 下一阶段提案：实际 borrowed owned/resource head

真实345原源 `pub fn byte_len(value:&String)->i64{return bytes_len(value.data)}` 目前在Types拒绝：Package window只允许共享user Record头，Managed(ByteBuffer)未贯通。保留原源，先支持实际ByteBuffer/OwnedStore/有限user资源头的AddressField→DescribeAddress，不复制Managed责任；Task、foreign registration与SABI头不因形状相近获得权限。公开opcode109由root `aa790ccba` 发布，root规范已明确实际读/独占来源及转发/返回/捕获/动态调用义务。

私有权限求证拟从实际Address.mutable和实际descriptor/closure输入建立符号访问限制，分别在Analyze及Verify的完整可达CFG/调用固定点传播；返回与捕获保留实际位置映射，callee实际写效果在当前调用代换，shared restriction不能被类型名、life标签、Desc复制或返回洗掉。描述权限与owner责任/loan/liveness分开：DescribeAddress结果保留父地址来源，实际view借用仍保留父贷款；只读API可访问shared来源，任何真实写/长度变更/独占view必须有实际授权。预算覆盖新CFG状态、符号集、捕获路径、summary复制与固定点；旧无新opcode源保留既有通路。以真实String、最近shared写/helper/returned/captured反例、raw+Wire、机器O0/O2、failure cleanup与适用N/N−1/cache核验，不以有限入口完成一般storage/lending声明。


## 第四里程碑：实际 borrowed resource head 与真实 std.string 消费者

公共 opcode109 `DescribeAddress` 由 root `aa790ccba` 发布；真实 loader/四个普通 std 源库及原345/string源由 root `c4134cd31` 接入。Types 的 Package AddressField 根据实际可用模式投影 Managed(ByteBuffer/既有有限 OwnedStore/Copy 用户资源头)，随后 DescribeAddress 只形成 Desc，不复制 Managed 责任。Task、SABI及外部registration不在本准入内。Lower/LLVM沿真实header地址零偏移指针解释，无新分配。

Analyze 和 Verify 各自执行新的完整可达 CFG/调用固定点权限检查。实际共享Address产生只读限制，实际参数与捕获路径保留位置关系；Desc复制、普通helper返回、闭包捕获/嵌套调用不能清掉限制，也不把独立共享捕获污染到另一个独占捕获。ByteBuffer/Store view保留真实父贷款；Analyze新增实际来源/贷款/父链/同时参数证明，并从callee活跃view与写交互重建外部存储需分离关系，再逐层代换实际caller根。不同外部参数名不证明存储分离。Verify使用自己已有的更细来源/effects/loans并补109父链与实参委托核验，不调用Analyze求证。

原 `tests/language-readiness/terminal/345/string.zil` 字节保持，SHA256 `b665f762f0222a52f0f1e30e46e8bdb9c0294f3f3cb621683a46c716485aa57e`；真实compile_path加载原std.string/utf8/text_edit/store，Clang/API × O0/O2四次原生执行均exit0/stderr空。另五份源涵盖共享/独占投影、Desc捕获、实际共享head参数捕获、独立混合捕获，20次运行均42。实际allocator故障源沿Text的&mut header借入：第一次分配成功、第二次realloc注入失败，错误路径恢复原责任并清理；双后端双优化四次打印10200，即2次尝试/1次release/0存活。总计本里程碑28次实际构建执行。

最近反例含共享原语写/helper写/Desc捕获写/实际head捕获写、父写撞活child/view、移动owner后使用head。raw与decoded runtime Wire在真实Address模式被伪造为shared时独立拒绝（Verify E-VERIFY-HEAD-PERMISSION），真实109合法Wire通过；transitive entry→relay→act中第二输入view活跃而第一输入写，raw caller把两输入改成同一实际存储时两个核/Wire均拒绝。跨模块live/restored cache重新检查当前API写入和caller活跃view。

实际十目标限定命令见 `docs/validation/terminal-342.json`，完整回放分别cross57/generic35/impl19714/review1844/impl20010/impl03310/impl26037/ownedbytes9/ownedbytes-native7/ownedstore18，零失败；最后直接head捕获、state复制去掉重复临时克隆和prepared实参数组收费后完整cross57重放通过。Clippy同十目标、llvm-api、-D warnings通过，首次lint指出新条件可折叠及新增helper位置误挂旧allow，均在生产源码修正。日志342-head-complete-tests.log、342-head-final-tests.log、342-head-clippy.log；不是正式全量。

新READ源Expand/Types/Analyze/Verify N=296/391/15016/19227，每个N−1 ResourceLimit且fresh N相同。Analyze首次15009后实际caller分离固定点增至15013，再预费prepared数组为15016；这是新增实际工作收费，默认限额不升。旧无109模块通过已付instruction扫描得到的私有feature激活条件保留原核费用，该条件只启用证明，不携安全许可。

首次真实失败记录：原345源先被旧全Package-address返回守卫误当Store元素逃逸，缩窄为实际has_store_fields输入，真实String返回view贯通；后续旧Store窗口profile误把当前bytes_view_mut新result本身当作先存竞争贷款，现按真实父链和新result身份区分，现有竞争child由独立贷款检查保留。原源、import和库API不改写成按值所有权调用。

Rust四目标：有限共享限制/实际父链与两核raw/Wire反例增加安全实证；没有完整语言安全结论。地址ABI零额外分配/RC/锁，真实失败计数如上，未测同语义Rust性能/RSS。普通真实std.string可直接使用&String和&mut String提供API，是可组合表达的有限增量，未测框架维护成本。新增工作预费且旧路径边界保留，没有编译成本优势结论。产品四目标合取、B0及现有product-gates不晋升；一般借用字段/存储/lending、HRTB Fn、显式dyn和一般§5.2仍继续施工。


### 独审近邻修正提案：同步scope body的真实调用代换

独审在976c7e7db指出新的Analyze head_loans::invocation尚未包含WithScopedThread/WithForeignCallback，而权限summary已包含两者。body内部重建的Capture0/Capture1需分离关系必须在实际scope调用处代换，不能依赖Verify已覆盖来冒充Analyze证明；MakeClosure弱Desc不能提供实际apart。扩展同一实际调用入口并以原transitive descriptor probe的thread_scope body变体验证distinct输入合法与raw/Wire同源参数拒绝。两种scope的body按实际签名均为零参数，环境是实际body捕获，同样进入该通路；不修改343线程模块。


独审近邻实际复现：新增thread_scope raw alias变体在976原候选Analyze错误通过，保留首次失败日志342-head-scope-first.log。修正后同一个既有测试依次检查普通helper、thread_scope body、c_with_callback body的实际相异输入通过，raw caller把两输入改同源后Analyze/Verify/decodedWire均拒绝。两个scope的body均按真实零参数签名处理，捕获环境独立代换；未虚构callback为body参数。新增thread_scope相异实际buffer源在Clang/API × O0/O2四次打印42，总本里程碑32次native。最终完整cross57/0，十目标Clippy -D warnings通过，N296/391/15016/19227不变；其余九目标前次完整结果仍绑定976，修正不宣称已经重跑它们。独立review8e58 BFP已PassLimited 9e740be05，head当前修正候选重新提交复核。

组合343：真实WithSyncCell body环境须进入两个头权限求证器和Analyze头loan调用代换；密封cell入参无普通资源头来源，捕获描述仍保留实际写/分离义务。原始alias变异曾在Mutex scope被Analyze错误接受；补入实际环境后，Mutex/Atomic的不同buffer正例及同一buffer别名反例分别走Analyze、Verify和Wire，另核共享头模式不能经scope环境洗成可写。


### 集成全仓回归提案：保留symbolic capture的真实Apart

root8981全仓发现impl187原source与raw/Wire反例错误通过。隔离342-capturefix按原源复现，未改运行中root或HRTB WIP。直接f()路径仍有Capture0/Capture1义务；经call(&f)转成ParameterCapture(0,0)/ParameterCapture(0,1)时，新inline优化以coarse均Parameter0且intersects=false丢弃义务。但这是两个外部目标占位，不是同一实际inline存储的相异字段。按340提供的补丁，仅两个实际Root::Window在同origin且区间数值不相交时保留apart豁免，其余symbolic根继续caller代换。原impl187字节和阶段预期保持，并加nested helper原生正例及source/raw/Wire最近同源反例；新增判断仍在既有pair计费内，无新默认额度。


隔离实际结果：按root指令采用340提供的实际Window区间补丁，Verify真实符号义务不再被coarse身份抹掉；临时诊断输出全部移除。原impl187完整24测试在修正后先独立通过，新增nested helper后完整25/0；同次aggregates89/0，含340真实Address Enum/tuple兄弟字段、raw权限反例及reference N18127/N−1/fresh保持。新的nested正例O0/O2真实Clang执行exit0；最近同源source在原Verify阶段E-BORROW-CONFLICT，raw及decodedWire同样拒绝。原反例未迁移预期。双后端新profile、完整语言、HRTB及产品四目标不由本回归修复升级；本条是原有限安全义务恢复，没有运行/RSS/编译或维护收益比较。精简收据terminal-342-capturefix.json，不自行全量；root全仓来源与作者限定回放分账。

同两个既有目标的Clippy offline/-D warnings通过；精确源码/原测试与新增最近邻/harness摘要作为独审输入，未扩大声明范围。
## 下一阶段提案：HRTB Fn 的普遍定义与每次调用关系

普通 `F:for<life a> Fn(&'a T)->&'a T` bound保留canonical FunctionLifeContract；CoreCallableParameter及实际CallableRole的Option关系由root独占发布，CoreStateBinding不加空字段。定义期从声明bound推导可调用参数/结果与真实贷款，不依据某个实际Closure更强能力；显式返回集合只由本次实参位置代换，环境Hold固定依赖独立。具体现核actual Closure完整签名及每条可达返回来源，捕获根不得洗成调用局部根。声明binder的名字按词法作用域独立，canonical关系按实际输入首次出现，调用每次用当前根/父链准备全部窗口。新增关系构造/复制/固定点按本次请求预费，旧无HRTB的路径不增加费用；真实源/未用定义/raw/Wire/native/cache/边界逐项核验后再提交候选。

HRTB 实现复核还发现既有 Types::lending::populate 在 closure_state.finish 之后以 limits.max_analysis_steps 新建工作量台账。新调用结果来源证明不能取得新额度；本轮把 finish 的实际剩余额度随完成的Closure元数据返回，并在同一剩余台账中扣除后续完整来源固定点。finish的最终签名查询及捕获类型复制先从同一台账付费。真实旧Types边界若因此变化按原源记录迁移，不改默认上限，不以新profile开关重置额度。

## 第五里程碑：HRTB Fn 实际调用关系与独立固定环境

公共 `92b8fc2e8` 的 CoreCallableParameter/CallableRole 保存 canonical `FunctionLifeContract`，Typed/Wire36、CallableContract5。普通 Fn/FnMut/FnOnce 高阶 bound 的每次调用用当前参数位置代换实际根和父贷款；闭包固定捕获 Hold 独立保留，不能作为 invocation-local 返回来源。闭包完整参数/结果与实际环境入口必须一致，Analyze 与 Verify 各自核每条可达返回的实际 Parameter 根及 mutable 来源；同一 ClosureId 用在两个不同 role 位置时逐份证明，不按 ClosureId 去重。允许一个实际返回 arg0 的闭包同时满足上界{0}和{0,1}，拒绝第二份上界{1}。

未用 generic body 从声明 bound 校验参数、结果与贷款，不借用某一具体闭包的强能力。固定环境不能洗成 fresh 调用 life；FnOnce 不能再次调用，活跃 mutable 结果不能被后续相交调用覆盖。实际 FnMut 两次调用不同输入的结果可以共存，调用结果依赖实际参数而非固定环境。普通 slice/fat-slice、CopyT、共享/独占地址与 owning FnOnce 环境沿既有ABI到达机器；没有隐式Box/RC或新分配。

Analyze 旧 head_loans 的粗 Parameter 根不足以表达借入闭包参数的嵌套捕获，独审诊断为合法 distinct-buffer HRTB helper 在 Analyze 拒绝 `head separation has no actual grounded source`。本轮加入私有 Parameter/Capture + 实际 capture path，按真实类型布局建立种子并逐层代换来源，保留 borrowed Desc 与实际 owning capture 的区别。两个核仍分别核实际同时窗口、返回子贷款及父链。所有新路径树容器的 clone/search/insert 预费长度与比较次数乘积，prefix 比较不能以 path 长度和 entry 数之和代替；checked overflow 返回 ResourceLimit。

Types 的 finish→lending 后处理使用实际剩余台账，最终签名与捕获复制先收费，不重新取得 max_analysis_steps。原 322/337/mixed/BFP Types N98/129/258/133 保持；新 STORAGE 源 Expand/Types/Analyze/Verify N=1514/526/11700/12306，N−1 全部 ResourceLimit、fresh N 同值。原 resource-head READ 的 Analyze N15016→15018 是实际 carrier lookup 新增收费，其余296/391/19227保持；默认限额不升。

13份 HRTB 源在 Clang/API × O0/O2 共52次执行均返回42；原 `tests/language-readiness/recovery/pending/life-higher-rank.zil` 字节不变，原 Expand/Unsupported 迁为 Success/Verify，四次机器执行 exit0。实际 owning FnOnce + 末位 error 参数准备在 fault allocator 的失败/成功两组×四配置共8次：失败2次分配尝试/1次释放/0存活/0 body调用，成功2次尝试/2次释放/0存活/1 body调用。该阶段新增机器执行64次。

raw 和 decoded runtime Wire 分别检查同 ClosureId 的不同 role、错误/gap/巨大 binder、固定 capture 返回伪造、两 mutable 实参指向相同实际 local、返回 exclusive child 仍活跃时写 parent；两个独立核均拒绝。nearest distinct-input 与 child-last-use 恢复正例通过。借入Desc/owned捕获与显式view的嵌套helper分离组合通过，真实同源反例拒绝。live/restored cache重核当前callback body与声明relation；JSON输出数值/null关系，alpha改名、binder声明重排和未用binder不改变签名/合同身份，改变实际关系会改变两种语义身份。

完整既有十目标限定回放67/35/10/9/7/18/14/10/37/4全部通过；同十目标 Clippy llvm-api -D warnings通过。日志342-hrtb-complete-tests.log及342-hrtb-clippy-final.log。跨tree共享target的mtime风险按root指令处理：触碰当前tracked生产source/manifests但不改内容，生产聚合SHA前后相同，完整39秒schema36依赖重编日志342-hrtb-schema36-paths.log与342-hrtb-mtime-refresh.json保留；测试显式核schema/Wire36。不是正式全仓。

首轮真实失败保留：原fixture在旧Unsupported列表引起预期迁移断言，源不改；新测试最初误用多binder列表和整数suffix，改为当前chained forall与数组类型标注；owned准备失败源需move环境及give字段初始化，否则原Types正确拒绝Managed/Desc混用。首次Clippy指出重复分支和检查后unwrap，改为等价合并/模式匹配后通过。实际path-aware正例误拒与预算reset已修生产路径，没有改负例来迁就接受。

Rust四目标：本切片提供实际普遍body义务、独立两核/raw/Wire及prepared-call/parent反例的有限安全实证，未证明完整语言安全。沿既有闭包/地址ABI，未测同语义Rust性能或RSS；真实失败释放计数如上。原HRTB源与借用helper/owning环境直接组合是表达力有限增量，未测框架维护收益。累计实际工作、不升默认限额，没有Rust编译成本优势结论。产品四目标合取、B0及既有product-gates不晋升。

后续继续一般存储位置/lending/State，再显式dyn。ForLifetime类型实参在本基线仍明确Unsupported，尚未宣称一般generic BFP/借入字段身份完成；固定存储life需按统一实际位置上界合同接入。当前作者树未包含root后续WithSyncCell实际body修正或343实际Suspend排除，集成时须保留并重放对应正反例；本切片不代替它们的独立证据。

补充限定unit回放保留初次失败：Analyze122通过、两项impl253旧v34冻结Wire在当前36解码按预期E-WIRE-VERSION，历史字节不改，统一codec迁移归root。Types32通过。Verify343通过、shared Product Store精确预算控制失败：新的BorrowLocal closure来源保留真实captured根，原Store控制无间接N15662→15663、间接N18453→18455；本轮保留原module/plan并仅迁移精确已测阈值，仍检查raw/Wire N−1/fresh。不以放松判断处理该失败。

最终unit回放 Types32/0、Verify344/0；两核原shared Product其余N4238/4652/6505保持，Store+1/+2在raw/Wire与N−1/fresh均通过。lint等价修正后完整cross67/0再次通过，最终绑定harness SHA256 59225b1025574c67449b13762b997b33427088e03302009f7202df8f65f3538b，详细精简receipt在docs/validation/terminal-342.json的higher_rank_callbacks。两项Analyze历史Wire版本不匹配继续单列，不记为通过。

## 后续独立提案：实际 mutable 嵌套 Product head

真实345 ui_input Composition 的普通字段 text:std.string.String，经 &mut Composition 的 `string.clear(&mut session.text)` 被Types既有shared-only子Product门拒绝。保留源/API，不展开String或改成按值借出；当前Types和两个核已接受实际Address<Package>签名及父来源/贷款。本步只把实际mutable父地址投影到已逐层资格核验的有限子Product，Verify独立核子Product完整真实类型，不能升级shared父权限或替换拥有责任。沿既有AddressField/DescribeAddress与实际父窗口，不新增opcode或登记许可。以普通嵌套Text/String完整机器正例、shared伪造raw/Wire、活view撞clear/父替换反例检查。未泛化到存储Address/Desc叶，不替代随后固定life位置合同。

实际mutable嵌套Product补丁验证：新增 `nested_product.zil` 使用真实std.string.String及原API调用形状，双后端/O0/O2四次exit0。现有AddressField父loan/实际resource来源足以覆盖此有限投影，Analyze无需新增许可；Verify对子Package完整实际类型独立资格核验。raw/decodedWire将begin的实际参数/重借改shared后两核拒绝；shared源请求mutable、活view跨begin、活view跨parent替换均拒绝。完整既有十目标68/35/10/9/7/18/14/10/37/4通过，Clippy cross llvm-api -D warnings通过，旧记录phaseN全部不变。当前342没有UInt及新std.ui_input loader，完整原345 ui_input由root合成树重放，本文不冒称已经执行原UI。统一stored Address/Desc life仍后续实现。

### 独审 HRTB 实际签名重复遍历收费修正提案

13677独审重放10/0和64native，但发现每个role的relation递归pointee资格/Copy判断及实际result类型比较未逐次预费完整形状；普通raw preflight只支付自身遍历，不能重复抵扣。两个核各自先从同一剩余台账预费参数/结果完整树，再进行关系/比较；Types validate_bound同样在推断操作前支付本次资格判断。调用固定点只用浅Address-result触发实际完整证明，不反复递归判断pointee；仍按实际声明位置核根和权限。增加deep/wide Copy pointee的实际源、N/N−1/fresh及raw/Wire控制，记录新费用，不升默认上限。

独审费用修正实际完成：每个role和实际callback返回证明先付参数/结果完整形状，再做递归资格与实际签名比较；Types validate_bound同样前置支付，callable_snapshot先做本地深度/工作验证再支付额外relation遍历，尤其覆盖早于state header核验的raw混合入口。调用固定点只用浅shape触发，两个核的普通closure header仍独立付费并限制结果为原有限Copy/slice pointee；新增无role的Package结果raw/Wire在该门拒绝，不扩大profile。

新增32字段+8层pointee正例经两个role重复使用同一ClosureId，Types/Analyze/Verify N15796/31925/72533；raw与Wire N−1 ResourceLimit、freshN同值。新增raw混合header的32层/16深度限制及256字段/128工作额度在递归资格判断前都返回Types ResourceLimit。当前HRTB STORAGE阶段N1514/530/11724/12330（相对13677为Types+4、两核各+24），其它既有控制N保持，默认限额不变。最终既有十目标70/35/10/9/7/18/14/10/37/4零失败；同十目标Clippy-Dwarnings通过，Types32/Verify344单位通过。Analyze限定122通过、2个已记录v34冻结fixture因版本不适用明确排除，原两项E-WIRE-VERSION失败记录仍保留。当前focused HRTB12项包含64native，无新增机器profile重复计数。修正待独立精确源码复核。


## Next public contract proposal: common stored borrows, State and lending


This is the next slice after the corrected HRTB checkpoint. It does not claim general §5.2 protocol execution, dyn, Iterator, Task or registration completion. The producer and each kernel must consume the representation before acceptance. Type tag35 is reserved by root; root owns the actual schema/Wire/CT versions and publication.

## Actual storage, target and declaration relation are separate

A storage position identifies bytes/initialization/epoch. Its current borrowed leaf identifies an actual target and loan ancestry. A declared life class restricts that leaf; it is neither target equality nor separation nor permission. Moving a complete carrier preserves its fixed leaf bounds and actual instance identity. Updating one leaf does not freshen those bounds. Every call creates fresh invocation substitutions; captured fixed environment classes are distinct.

Reuse Field/Variant/ErrorArm from PlaceProjection. Variant/ErrorArm steps require the actual active initialized ancestor; paths never silently dereference stored Address/Desc leaves. 340's BorrowPlaceAddress126/PlaceTagAddress127 can walk real inline storage and stop at those leaves. A dedicated load then resolves the actual stored target.

Proposed common declarations (names can be adjusted together):

```rust
pub enum StoredBorrowMode {
    SharedAddress,
    ExclusiveAddress,
    ReadOnlyDescription,
    WritableDescription,
}
pub struct StorageLifeSlot {
    pub path: Vec<PlaceProjection>,
    pub binder: u32,
    pub mode: StoredBorrowMode,
}
pub struct StorageLifeShape {
    pub binder_count: u32,
    pub slots: Vec<StorageLifeSlot>,
}
// New Type tag35, physical layout exactly storage's existing layout.
Type::BorrowedStorage { storage: Box<Type>, life: StorageLifeShape }

pub enum StorageBase { Parameter(u32), Capture(u32), Result }
pub struct StoragePosition { pub base: StorageBase, pub path: Vec<PlaceProjection> }
pub enum StorageBinder { Fixed(u32), Invocation(u32) }
pub struct StorageLifeBinding { pub position: StoragePosition, pub binder: StorageBinder }
pub struct StorageLifeContract {
    pub fixed_binder_count: u32,
    pub invocation_binder_count: u32,
    pub bindings: Vec<StorageLifeBinding>,
}
```

Modes must match the actual Address leaf and its mutable bit exactly, or describe the permitted Desc access upper bound. A mode cannot upgrade an actual shared/read-only origin. Empty wrappers erase to the existing type; raw empty or direct double wrappers reject. The storage root must be an explicitly supported finite Record/Package/Tuple/Enum/ErrorUnion. It cannot be Address/Desc/Managed/foreign/dynamic storage or an opaque type. Nested finite storage may contain meaningful child borrowed-storage types; their relative maps must compose consistently. Every real Address/Desc storage leaf is covered exactly once, with no missing/extra/duplicate path. Owning children retain their separately checked responsibility and cleanup qualification; the wrapper does not grant Copy, StoreElement, capture, Share or Transfer automatically.

Slot order is deterministic structural declaration order, and labels are dense by first actual occurrence jointly across all stored slots. Reject unused tails, gaps, out-of-range labels, duplicate slots and inconsistent mutable leaf claims. Alpha renaming and binder declaration reordering cannot change the relation; changing which slots share a class changes semantic type identity. Permission remains separate from a class shared by multiple slots.

## Signature binding and source preservation

CoreFunction and TypedFunction receive an optional storage life contract only on actual storage consumers; ClosureDefinition receives the same shape when actual fixed capture or stored-result relations require it. Function parameters/results refer to their actual positions and full projected paths. A closure's Fixed binders refer only to actual captured storage; Invocation binders refer only to that call's actual arguments. An invocation-result relation cannot substitute a fixed capture merely because both use numeric index0. Parameter/Capture/Result rows are checked against actual function/environment layout and complete signatures. A contract with a fixed result source uses the general lending proof; it is not passed through the existing invocation-only FunctionLifeContract shortcut.

For a signature with stored slots, the new contract must include its participating top-level Address leaves as empty paths. It replaces the flat relation for that signature, or root may require an exact checked projection into the old flat field; two independently meaningful, potentially contradictory relations are not allowed. Existing flat FunctionLifeContract encoding remains unchanged on old signatures. Source role/State bound relations need the same stored-slot mapping only when the corresponding implementation consumes them; no placeholder CoreStateBinding field is proposed.

The existing AST GenericArg supports only Type/Const. Add root-owned GenericArg::Lifetime(String) so ordinary Iter<'s,T> applications can express the actual relationship; lifetime declaration kinds already exist. Private parsing/substitution/key work must preserve argument kind and ordinal, lexical binder scope and canonical joint maps. No lexical-name-only key, no dropping life arguments as if they were omitted types, and no classifying Address type arguments as Copy. The actual source target is an ordinary declaration such as SliceIter<life s,T:Copy>{slice:&'s[T],index:i64}, with Option<&'s T> results and ordinary trait methods, without an Iterator builtin.

Address slots have explicit source life labels. Descriptor slots must acquire a fixed upper from a real declared/inferred Hold relation; a bare resource type or empty trait does not invent one. Any source syntax needed for explicitly named descriptor Hold must be settled with root rather than guessed in lowering.

## Actual leaf operations

Propose two untrusted runtime operations, using an actual Address to qualified storage as base (root assigns append-only opcodes):

```rust
pub enum StoredBorrowLoad {
    CopyShared,
    CopyDescription,
    Reborrow { mutable: bool },
    Take,
}
InstructionKind::LoadStoredBorrow {
    storage: ValueId,
    path: Vec<PlaceProjection>,
    mode: StoredBorrowLoad,
}
InstructionKind::StoreStoredBorrow {
    storage: ValueId,
    path: Vec<PlaceProjection>,
    value: ValueId,
}
```

CopyShared is only a real shared Address leaf. It returns the actual external target loan, preserving its fixed bound; a shared SliceIter result can outlive the short borrow of the iterator itself. CopyDescription preserves actual target, read/write restriction and parent view windows. Reborrow creates a fresh child of the actual stored target permission and storage slot epoch; exclusive reborrow requires both an exclusive storage base and an exclusive leaf. Take requires exclusive initialized storage, moves that leaf permission and marks that exact position unavailable. It cannot duplicate an exclusive loan. Store requires exclusive storage, exact actual leaf type/mode and invariant fixed upper, checks the new Hold before discarding old permission, and rejects any overlapping live call/result loan. Owning field replacement continues the separate responsibility/cleanup operations; this operation writes only an Address/Desc leaf.

These operations must not materialize a freely usable Address<Address<T>> pointer slot or turn ReadAddress/WriteAddress into unchecked pointer copying. If root prefers an existing opcode with an equally explicit storage-only protocol, the same independent checks and nonescaping cut point remain required.

## Independent proofs and evidence

Analyze and Verify each reconstruct storage initialization/version, actual target roots, loan parents, live uses and call/CFG joins. Paths and actual instance identities are private proof state, not public permission flags. Calls substitute parameter/capture paths into actual targets and propagate post-call slot Holds; result summaries refer to actual source positions, including stored leaves, rather than only coarse Parameter numbers. Signature life is checked as an upper bound. Local targets, stale slot versions, missing origins and cleanup after an escaped borrow reject. A loop/backedge or recursive call does not reset the fixed upper or the analysis ledger.

This same mechanism covers ordinary borrowed records, borrowed State fields, shared SliceIter, exclusive lending next, and later owning-pointee lending. Active tags, partial initialization and responsibility remain separate. Replacement must validate the prepared new value before old Cleanup; a new value borrowing the old environment cannot survive destruction of that old environment. No independent iterator/state special case, hidden Box/RC, runtime borrow checker or implicit pinning is introduced.

CT Type DAG/read view/constructor/Wire must preserve storage child node IDs and the complete numeric shape. CT reconstruction or source materialization may not print free lexical labels, erase the wrapper or turn it into ordinary Copy storage. Proposed CT node BorrowedStorage{storage:CtTypeNodeId,life:StorageLifeShape} has one actual DAG edge; constructor takes the storage type operand plus complete shape, with all clones/edges/path comparisons prepaid. Core/Typed/module/type/interface/JSON/CT/cache identities include the actual maps. Root chooses the joint versions; candidate decoding reruns current kernels rather than accepting an old proof.

Bounded metering includes every repeated shape/path walk, ordered common-prefix comparison, root set join/copy, slot epoch state, active-loan substitution and fixed point. A shared remaining ledger is retained throughout each phase. Validate size/depth before recursive classification and before clone/growth. Default limits do not increase.

Retain original SliceIter/lending/State source, unused definition obligations, nearest short-lived field overwrite, same-root mutable alias, stale epoch after overwrite, result crossing parent replacement, active-tag forgery, raw+decoded Wire and current cache/body mutation controls. Native Clang/API O0/O2 must include successful iteration and owned/error/argument-preparation cleanup. General §5.2 protocol transitions and explicit dyn follow this substrate and remain separately reported until real consumers exist.

## Clarifications requested by root before publication

CopyShared snapshots the actual external target permission, not a loan of the pointer slot. Its result retains the target's real root/owner and fixed life upper but is detached from that slot's epoch. A later valid write of that slot must not invalidate or freeze this copied shared result. Replacing/destroying the real target remains blocked. Reborrow keeps the slot's parent/epoch dependency. A real Take transfers the actual exclusive permission, marks the old slot unavailable and detaches the transferred value from the old slot epoch; its fixed upper and actual target/owner/parent ancestry remain. Neither operation is implemented as CopyShared of an exclusive pointer.

Representative ordinary source for the positive/nearest negative (new generic lifetime argument syntax is required):

```zil
struct Saved<life s> { pointer:&'s i64 }
fn replace<life s>(saved:&mut Saved<'s>, next:&'s i64)->i64 {
    let old=saved.pointer
    saved.pointer=next       // allowed: same fixed upper; old refers to old target
    return *old
}
```

An actual caller may use two distinct targets that both satisfy the fixed upper. An old shared result copied out before replacement continues to refer to the first target; writing that first target while the result remains live rejects. This is distinct from writing the pointer slot. An exclusive child reborrowed from a stored exclusive leaf keeps the storage epoch and blocks overwriting that leaf until the child's last use.

Owned-field description and stored-description load are separate operations. DescribeAddress/DescribePackageField derives a description from a real owning Managed field and its actual carrier permission. CopyDescription can only load a real Desc leaf already stored in qualified storage; it intersects (a) that stored value's actual access fact, (b) the declared description access upper, and (c) the current storage base permission. A shared base produces only readonly access absent an independently supported interior-mutation protocol. A mutable carrier cannot upgrade a readonly description stored earlier. The public description mode is an access upper, not a promise that every current value actually has write authority; independent actual permission facts must remain attached.

Representative normal source using an inferred fixed Hold for a descriptor field:

```zil
struct Described { bytes:ByteBuffer }
package Owner { manages bytes:ByteBuffer }
fn reserve(saved:&mut Described) { bytes_reserve(saved.bytes,8) }
fn observe(saved:&Described)->i64 { return bytes_len(saved.bytes) }
```

Construction from an actual live writable descriptor, followed by reserve(&mut saved), is positive; observe(&saved) is positive. Loading through &Described and attempting bytes_reserve rejects. A second positive constructs the stored descriptor through an exclusive Owner head. The nearest negative constructs it from a shared Owner head, then passes the resulting carrier by &mut to reserve: the actual readonly permission survives storage, so reserve rejects. Destroying/moving the original owner before a future stored-description use also rejects. These are intended source/CFG controls for implementation, not acceptance claims for the current compiler.

## Consuming transfer and call-state boundary

A real consuming Take transfers that leaf's actual permission and target loan ancestry to the result, marks the original slot unavailable, and detaches the transferred result from the old slot epoch. Refill with a different valid target does not invalidate the transferred result. The fixed upper and actual target/owner/parent dependencies remain. Reborrow alone retains the original storage borrow and epoch dependency.

A concrete ordinary positive prepares the new permission before consuming the old leaf:

```zil
struct Exclusive<life s> { pointer:&'s mut i64 }
fn replace<life s>(saved:&mut Exclusive<'s>, next:&'s mut i64)->&'s mut i64 {
    let old=give saved.pointer
    saved.pointer=next
    return old
}
```

The proposed existing give-place surface here lowers to the real consuming stored-leaf operation. Caller targets must actually be disjoint, even though both have the same allowed life class. A second `give saved.pointer` before refill rejects. Returning after only the first Take rejects the incomplete borrowed carrier. Moving/calling with that whole incomplete carrier rejects. An aliased actual replacement target must reject while `old` remains an exclusive result; two distinct live targets are positive. Raw/Wire controls must test these actual target aliases independently of source's earlier rejection.

StoreStoredBorrow accepts a vacant borrowed-leaf slot for refill, still requiring its invariant bound/type/mode and all active ancestors. A positive prepares a new target first, Takes the old exclusive permission, refills the slot with the new target, and returns/uses the old permission. The intended ordinary consuming field expression lowers to Take (the existing explicit `give` place surface can be extended for this actual affine leaf; root owns the final syntax norm). Nearest negatives are a second Take before refill, moving/calling with the whole carrier while incomplete, and refilling from the same actual target while the transferred exclusive permission remains live. Different target names or life labels do not establish separation.

A function borrowing an external carrier by &mut must restore its declared legal complete state on every normal returning path, including error returns, or really transition it to a valid empty Enum branch. A temporary vacant slot cannot silently persist in a caller still treating that carrier as complete. Cross-call missing/refill states require a separately proved real state protocol. Cleanup of a partly moved local remains checked independently against its remaining responsibility.

For signature StoragePosition traversal, an empty parameter path denotes that actual outer Address leaf if present. A nonempty path may unwrap the entry Address once and then traverse only inline storage/metadata-transparent borrowed-storage wrappers. It cannot silently dereference an Address/Desc encountered inside the path. Capture bases name actual current environment fields; any nested capture traversal is checked against the real layout and is not ordinary pointer-place authority.

An Invocation binder meeting a stored class is bound to that existing class; it cannot narrow or freshen the stored class's invariant fixed upper. Fresh call substitution never grants write permission or changes a carrier's fixed bounds. CopyShared's independence from the pointer slot does not remove a real target owner/parent dependency, including when a separately supported target is actually owned by the same carrier.

### HRTB export identity work proposal

343审查联合program finish余额时发现新增interface_fingerprint的HRTB export/role扫描和数值关系序列化未从同一余额逐次收费；signature快照只支付自身遍历/复制。下一修正把该余额传到identity构建，预付新扫描与新增关系字节的growth/hash工作，保持字节顺序和语义身份不变，overflow/不足返回Types ResourceLimit；默认限额不变。以真实跨模块普通HRTB源的Types project N/N−1/fresh和既有live/restored接口缓存控制核验，记录必要旧预算迁移。

实际export identity修正通过：同一真实双模块源的Types project N662→784，N−1 ResourceLimit/fresh N相同；新增guard不改任何已序列化字节及顺序。完整既有十目标71/35/10/9/7/18/14/10/37/4通过，同十目标Clippy llvm-api -D warnings通过；原HRTB STORAGE和deep阶段N均保持530/11724/12330及15796/31925/72533。现有alpha/changed-relation、live/restored cache与原生清理控制仍通过。原ebcc独审PassLimited254de49db及nested Product独审8808a1fa6保留；root已集成相同fee补丁及两项相邻field/ClosedEnvironment source控制，root73/0另记，不作为作者树直接执行证据。
### 集成当前Wire37回归迁移

真实root组合HRTB/Task37与generic receiver methods通过cross73、generic_regressions59、generics23、trait58。独立Vec当前oracle补完整CallKind、role.life及SuspendCall，保留V31的95操作原固定样本；IR61及oracle13通过。历史v32/v34字节全部保持，只以固定80bec5278的结构decoder离线转码，显式加入Direct/None life；current核重验，旧plan不授予权限。初次迁移漏更新六个driver模块的helper路径，fmt暴露后统一覆盖所有17个历史输入；Analyze2及driver16/7/1/1/1/2实际通过，tools/migrate-impl253-wire37.py可复现。不会只改版本头冒充新记录。

## Root publication: common stored-borrow data contract

# Proposed normative increment for joint Typed/Wire38 (root publication only)

Ordinary finite borrowed storage uses BorrowedStorage{storage,life}, Type tag35. The wrapper preserves the underlying machine layout and carries a canonical relative stored-leaf relation, not permission or target identity. The first concrete consumers cover actual finite Record/Package/Tuple/Enum/ErrorUnion storage with real Address/Desc leaves. Owning subcomponents retain independent responsibility/cleanup obligations. Copy, StoreElement, capture, thread qualification and raw pointee eligibility do not follow from the wrapper tag.

StorageLifeShape has dense binder_count and declaration-ordered StorageLifeSlot{path,binder,mode}. Paths reuse only physical PlaceProjection Field/Variant/ErrorArm, retaining active tags and initialization. All actual borrowed leaves are covered exactly once; unknown, duplicate, missing and inconsistent paths/modes reject. SharedAddress/ExclusiveAddress match actual Address mutability; ReadOnlyDescription/WritableDescription are declared access uppers and never actual authority. New traversal, cloning, ordered path comparison, state and fixed point work is paid before use from the existing shared request ledger. Validate depth/size before recursive classification or allocation.

StorageLifeContract separates persistent Fixed and fresh Invocation binder namespaces, with actual Parameter/Capture/Result positions. A Fixed class must be independently grounded in a real retained carrier class (capture, stored carrier or dyn port); it is not restricted syntactically to Capture, and bare new call inputs/results cannot invent one. A call meeting an existing fixed class cannot narrow/freshen it or choose a new intersection. Fresh invocation binders are independent across calls. Equal labels do not establish aliases, separation or mutable permission.

For a parameter path, empty selects an actual outer Address when present. A nonempty path may unwrap that entry Address once and then use inline physical projections. It does not implicitly dereference a stored Address/Desc along the path. ContractProjection is distinct: Place wraps a physical projection, while terminal DynPort selects a canonical abstract fixed environment port only at the actual Address<Dyn(interface)> boundary. DynPort never supplies GEP, LoadStoredBorrow or ordinary field access. Interface ports {leaf,mode,fixed_binder} and actual vtable port→concrete Self StoragePosition bindings are checked by both kernels. Actual dyn values retain receiver and mapped target roots; permissions and actual aliases remain separate from interface or trait spelling. Type::Dyn is an unsized pointee; Address<Dyn> is the env/vtable fat address and an ordinary stored Address leaf.

LoadStoredBorrow129 takes an actual storage Address and physical path. CopyShared copies a shared target permission and fixed upper, detaching the result from the pointer-slot epoch/short carrier borrow; replacing that pointer slot with another value satisfying its unchanged upper may proceed while the old copied result remains live. The actual target owner and parent loans remain protected, including a target independently known to be owned by the carrier. Reborrow retains storage/slot/epoch dependence and creates a new real child loan. Take consumes/transfers the leaf permission, marks that slot unavailable and detaches the transferred result from its old slot epoch; actual target ancestry and fixed upper remain. A second Take before refill rejects. A later refill from a distinct legitimate target does not invalidate the transferred permission, while overlapping simultaneous exclusive targets reject by actual roots.

CopyDescription loads an actual stored Desc leaf; its effective permission is the intersection of the actual stored fact, declared upper and actual base permission. A shared carrier cannot upgrade a readonly owning-head path to writable without a separately implemented interior-mutation protocol; borrowing the carrier mutably does not upgrade a descriptor stored readonly earlier. Describing a real owning Managed field remains DescribeAddress/DescribePackageField and is distinct from loading a stored description.

StoreStoredBorrow130 requires actual exclusive storage, active/live ancestors, exact leaf type/mode and invariant fixed upper. It handles initialized replacement and vacant refill. The prepared new Hold is checked before old permission/cleanup ends, and all overlapping call/result loans are respected. A borrowed external &mut carrier must return in its declared legal complete state on every ordinary/error return, or really transition to a valid empty enum branch. Temporary missing fields cannot silently escape to a caller still treating the value as complete. Cross-call partial states require a separately proved real state protocol. Whole moves/calls while incomplete reject; partial local cleanup checks remaining actual responsibility.

Core/Typed function, closure and scalar signature data preserve the complete stored relation. A storage signature replaces the flat relation, or any retained flat projection must be exactly checked; contradictory independent contracts reject. Interfaces, JSON, type/module/CT identities and caches include actual maps. Candidate restoration rechecks current independent kernels and actual body/target facts. Static layout erases the relation only for machine representation; semantic type identity does not erase it. No hidden Box/RC/pinning/runtime borrow checker is introduced.

Explicit nominal lifetime actuals use AST GenericArg::Lifetime and preserve argument kind, declaration ordinal, lexical scope and joint canonical relation. Source `SliceIter<life s,T:Copy>{slice:&'s[T],index:i64}` and ordinary Option<&'s T>/trait methods are the intended real consumer, not an Iterator builtin. All no-call definitions have the same symbolic obligations. General §5.2 protocols, full State/lending and dyn capabilities require their actual consumers and are not granted by registering these records.

CT BorrowedStorage node16 retains one actual storage child ID and complete StorageLifeShape; constructor12 takes that storage Type and raw shape. Its codecs bound class counts, path length, allocation, growth and DAG edges. Reflection/Wire cannot erase the wrapper or relation. Existing CT encoding bytes/domains can remain when unchanged; root owns the joint version choice.

## Explicit remaining CT lifetime/phantom interface

This model patch does not add a free-name CtTypeArgument::Lifetime. CtTypeArgument currently has Type/Scalar only. A future lifetime actual requires an explicit closed numeric scope owner/reference compatible with Fixed/Invocation domains, preserving the full nominal argument position including phantom lifetimes; validation must check binder scope/range/argument kind against the frozen declaration and reject a scope escape. A lexical string or omitted argument is not a substitute. The precise scoped-reference representation is still under joint design and is not claimed implemented here.

Until that representation and materialization consumer exist, CT nominal Type/Syntax reconstruction involving lifetime/phantom actuals must explicitly reject Unsupported. The source consumer must also keep unsupported phantom-only lifetime types explicit where this first stored-leaf shape cannot yet retain the required semantic arguments; it must not normalize them to an ordinary type or discard lifetime actuals. Empty wrapper erasure is permitted only when no actual lifetime/nominal relationship is being discarded. This is a remaining terminal obligation, not a permanent language rejection or acceptance waiver.

Proposed schema changes in this external candidate: SCHEMA_VERSION38, runtime Wire38, typed-module fingerprint domainV38 (old function/closure bytes gain optional storage metadata). TypeIdentityV35 and existing callable contract domains remain when their old encodings do not change. Root will compose Task34/128 and Dyn36/131/132 alongside this patch and publish the final unified registry. Data registration/codec roundtrip and standalone IR compilation are not language/kernel acceptance.
## Stored-borrow baseline and actual Store resource-head continuation

The shared Wire38 records are staged before their consumers. Private adapters keep unimplemented stored-borrow operations and CT reconstruction explicitly unsupported while preserving every existing operand and ClosedEnvironment traversal; this is not a capability-completion claim. The complete retained-class/nominal-argument supplement is delivered externally for root publication.

The unchanged impl180 source `let v=store_at_mut(s,0);v.owner.code=90;return 0` currently loses the actual Store origin in resource flow, then rejects a real exclusive resource-head Copy-field projection. Repair both independent resource flows by forwarding the actual Store descriptor origins to its borrowed Package result. Permit a Managed payload field projection only from an independently admitted real head Address, preserving the actual base mutable bit, resource responsibility and Store/view loan windows. This does not permit whole owner replacement, copying Managed responsibility, shared writes or stale view use after Store mutation. Retain the exact original source as a positive with native cleanup observation and nearest shared/stale-window negatives, plus raw and decoded Wire provenance/permission forgeries in both kernels.

Store-head validation on staged38: unchanged b377 impl180 constructed runtime source is byte-equal to the retained `store_resource_head.zil`. The actual exclusive resource field write succeeds with normal cleanup stdout `ZB`; its nearest last-use then reserve case also succeeds in Clang/API O0/O2 (8 new profiles). Four new focused tests retain shared/stale/destroyed/unknown-index source negatives, both-kernel raw+decoded-Wire permission/owner/reserve/caller-alias negatives, live/restored current-body cache checks, Analyze N22441 and Verify N38508 with N−1 ResourceLimit and fresh N success. Existing cross_function_borrows77 and impl180_store_mut19 pass in full; same two-target Clippy with llvm-api and `-D warnings` passes. The historical37 schema assertion alone follows root's published38 version; earlier source and fee receipts remain unchanged. General stored-borrow types/operations/CT stay unimplemented and fail closed at the staged boundary; this checkpoint does not complete SliceIter, lending, dyn or State. Rust goals: actual resource bytes and deterministic cleanup remain explicit; the source API now composes an ordinary Store view with its real owned head's Copy field. No heap wrapper, extra allocation, synthetic permission or index-based separation was introduced. General reusable borrowed storage remains the next obligation.

## Ordinary stored-carrier source continuation

The next actual source is an ordinary `SliceIter<life s,T:Copy>` containing `slice:&'s [T]` and `index:i64`, with ordinary factory/method calls and `Option<&'s T>` results. There is no Iterator builtin route. Nominal actual syntax must preserve Lifetime/Type/Const kind and full ordinal before specialization; parsing a lifetime actual does not qualify storage or authorize an otherwise unresolved key. The factory may bind its result's stored class to the actual slice argument, while `next(&mut self)` uses that already fixed class and an independent short receiver invocation. Reading the stored shared slice detaches that pointer-slot epoch while retaining the actual slice target, permitting index updates and the next call with a prior element still live. Mutable reborrow/Take/refill follow the distinct common rules already recorded above. The runtime and each kernel must reconstruct the actual stored target separately from the carrier's byte position; generic definition obligations remain before specialization.

Stored-source WIP checkpoint before root39 refresh: the ordinary fixture now parses and its Life/Type/Const actual-order control passes. Analyze and Verify have separate physical path/type/mode checks for129/130; neither replaces actual tag, initialization, upper-bound, loan-parent or epoch reconstruction. The full BorrowedStorage type/signature admission guards remain Unsupported, so this is not a stored-borrow consumer pass. Driver check and cross-target Clippy pass; existing terminal34241 controls pass (37 unrelated cross tests explicitly filtered). Source/native SliceIter and the general storage proof remain unfinished. No public schema or other author's BFP key implementation was changed.
