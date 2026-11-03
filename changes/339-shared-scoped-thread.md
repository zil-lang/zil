# 339：joined child 的共享 i32 借入

权威合同为主树 `_zil_design.md` §11.2/profile339，root 冻结提交 `c4da43441`。
施工基线 `e5a4d51d844e29c4f4f5184eaff5704b1b27dd24`；独立339树及工单写集。

原始真实输入 `shared339/shared_child.zil` 对 B CLI 的结果是 Types
E-THREAD-WORKER（fn(i32)->i32 门），Verify 未执行。同址父body读取、普通静态
共享helper及join后写入必须贯通；标量thread和顺序shared调用分别为Verify正控。

不增保留名称、公共字段、opcode或Wire版本。现有WithScopedThread的实际argument
类型决定i32或共享Address<I32> lane，所有raw输入仍经当前独立两核。新机器context
为{ptr,i32}，Linux x86_64大小16/对齐8；旧{i32,i32}大小8/对齐4保持。child实际
载入指针并解引用原父对象，不把引用改为快照。helper/context私有且按实际指令生成，
既有CGU依赖、完整Typed指纹和绑定compiler身份的object action涵盖此变化。

共享窗口覆盖payload准备、完整body执行/清理直至成功join；创建失败不执行worker/
body而准确清理，join不能证明静止时abort。两核各自重建实际worker图、共享读取权限/
来源与实际payload/body捕获的同时窗口；不能借最后使用、相同ClosureId或FnMut整位
替代实际逐捕获根。嵌套writer、可变ancestor重借、拥有body清理失活和相交移动必须拒。
父共享读和不相交写应成功。worker仅增加一层shared&i32参数/局部传递和读取，静态
helper结果仍为标量，不允许地址返回/留存、可变地址、FFI/环境/资源/未知间接调用。

新增算法在现有请求余额内累计预付，记录Types/Analyze/Verify的N/N−1/fresh及旧标量
控制实际delta，不提高默认限额。不创建Cargo目标、不跑正式全量。机器观察C仅控制
真实pthread入口延迟和核对身份/清理，不充当语言线程实现。

本文件先记录合同；实际结果与范围/原失败将在完成后补记。无Mutex/Atomic、一般
Transfer/Share、异步/SABI、Rust运行/内存或编译性能结论；B0/完整语言/ZUI保持未完成。

## 首个实际checkpoint

`cargo check -p zil-driver --features llvm-api`通过。既有generic_regressions目标
过滤thread339的首次源码/原生两项通过（非正式全量）。新disjoint源码实际
Analyze N=10829、Verify N=46749，N/N−1/fresh均实测；尚未比较旧控制或完整Types。
原失败保留：Rust临时borrow编译失败；nested move捕获被既有retained环境门Unsupported，
非move后未声明var又被E-IMMUTABLE-ASSIGNMENT拒绝；改为真实可变writer后才进入
实际两核/原生。首轮Clippy发现新重复分支，已改为同一资格匹配；最终lint待重跑。

机器新profile实际两backend×O0/O2，16个带观察器成功/业务错误/create失败/join失败
场景；另5个源控制×4配置=20个无观察器运行。observer比较body实际&input与pthread
context所载指针，延迟production start到body Drop之后，核生成worker读出11；未把
C观察器当语言实现。API O2多CGU，全部实际backend且无fallback。重放不重复计数。

## 独立来源核的修正

独审发现首版Verify的额外producer句法DFS把合法父侧`same(&local)`结果误拒为
Unsupported；该观察保留。root授权私有Verify Budget共享thread成员表，付费创建和
读取，只对真正共享载荷caller启用原有完整loan重建。独立完成Facts的实际根现在
用于stable storage资格，payload和body进入同一活动窗口，不复用Analyze候选来源。
首版DFS已删除，不新增源句法白名单；worker的地址返回限制不因此放开。普通父helper
返回合法根的parent_return源晋升成功，raw aggregate projection仍在Analyze与Verify
分别Unsupported、Lower未执行。标量路径无成员表、保持旧收费。

实际新共享basic Types N=532、Analyze N=8345、Verify N=29372；disjoint为
656/10829/46218；parent_capture Types N=423。均保留N/N−1/fresh；共享basic的
Verify费用上升来自此前会早退的非affine路径现在执行完整loan核。旧标量控制的
Types/Analyze/Verify为526/7787/15329，原parent332/kinds为1587/26216/92685，
同一个独立API探针绑定B既有release rlib与339实际debug rlib，各自全部边界逐字相同。
探针源码shared339/budget_probe.rs保留；不是新Cargo测试目标，也不提供安全判定替代。

## 非作者反例与当前实测

Astra保留的helper_writer原源在首版Analyze成功、Verify E-BORROW-CONFLICT；
新增exclusive表只处理特殊producer遗漏了普通Call返回的&mut及可变投影。
修补为每个实际非environment可变Address结果从独立Hold实际根补齐，&mutClosure
仍只取得environment位置及逐捕获真实exclusive根，不能升级Retained* shared根。
原反例和独立raw helper_return seed改变thread实际argument均由两核拒绝；原disjoint
仍成功，最近nested_disjoint把shared x与mutable y同时放在被借入writer内。

更正：最早报告owner_cleanup为SafeReject是不准确的，早期该fixture未进入格式化后
的测试negative表；现实际为Analyze Unsupported，和lending_pending同属直接未核
动态来源。此两源已进入精确表。336另交拥有Ticket环境的retained helper正控及
owned_lender_cleanup移动/销毁反例；后者真实Verify E-BORROW-CONFLICT，不把它
记作Analyze拒绝。标量Copy lender的parent复制为合法Load，原environment仍活到
join之后，copied_lender作为正控，不用保守禁止合法复制。

当前339六项进入既有generic_regressions；完整既有目标实际32/0、零过滤/忽略，
不是workspace正式全量。旧325两核N=6873/10587，旧332两核26216/92685及Types
全部既有精确边界保持。最终本地disjoint Analyze N为10837（新增实际mutable-root
工作），Verify46218；basic532/8345/29372，two_scopes632/4175/11774，nested_shared
477/3769/7724；各项Types/Analyze/Verify均实测N/N−1/fresh。parent_capture Types423。

backend既有scoped_thread三项模型/ABI/分区测试另3/0，不当成新增源码覆盖。
共享cache测试同目录先i32、再pointer、再warm pointer，三次均重新Verify、实际API
无fallback，warm明确object-cache命中，三个生成程序均0；不同payload类型和机器ABI
不会复用旧对象。后续root contract同步后的结果另记，不倒记本次为整合树结果。
