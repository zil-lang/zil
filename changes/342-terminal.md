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
