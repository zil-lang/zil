# 340 基础类型与模式终态候选

权威为主树 `.task/impl/340.md`、当前 `_zil_design.md` §15.2/§16。
本提案不提升 B0 或产品门，不将有限阶段交付记为终态完成。

1. 普通函数指针调用保留已经确定的完整结果类型，使未注解绑定后的
   记录式 Enum match 与同一签名的直接调用一致；局部遮蔽继续按真实绑定。
   原 `tests/language-readiness/batch336/component.zil` 不改写。
2. `T?`/`Option<T>` 使用封闭卫生身份的普通 tagged Enum（None/Some），
   复用既有判别、移动、清理与机器布局；不能与用户同名 Enum 混同。
3. `u64`/`usize` 必须保留独立类型和无符号数值语义；`usize` 以目标宽度
   核验，CT 不借用宿主 usize。公共 Type/Constant/Wire 由 root 发布。
4. 一般模式按真实 place、分支及责任处理。ref 是真实借入，不能复制拥有
   载荷或越过判别失效；失败模式不得先移动父值，通配拥有叶清理一次。

源码正例、最近反例、两核/Wire、机器 O0/O2 和适用额度测试随各阶段提交。
未实现项保留明确未完成状态；不以 Unsupported 当作能力交付。

地址模式接续：107/108 从真实 Address 读取每层 Enum tag/取得 payload 地址，
110 从真实局部 Tuple/Record 的字段地址读写；都不物化父值副本。
两核各自保存有界投影、当前 tag 观察和借入根；相同 CFG 后继必须汇合
真/假两条边，调用、回调和可能的写入使旧观察失效。投影区间可证明
同一实际基址下不同字段分离，完整父值替换仍相交。窗口从最后使用释放，
返回引用保留实际参数根，闭包传递保留父依赖。新 CFG/路径/窗口处理
计入现有请求额度，复制和路径分配前扣费；新增组合源码单独记录边界。
该阶段不改变 FFI、线程 worker、捕获或普通 Copy 的资格。

选择性拥有转移提案（root已分配114–118）：保留结构 Type::Tuple，
MakeOwnedTuple 保留各实际字段的拥有责任，UnpackTuple 原子转交完整元组，
DropTuple 只清理真实剩余初始化字段；不把元组伪装为名义 Package。
TakeTupleField/TakeVariantField 分别从实际 Tuple/Enum 局部选择性转交一个
初始化子树，非空有界路径使用 PlaceProjection::Field(index) 与
PlaceProjection::Variant{variant,index} 保持混合层级。每次访问逐步核验完整
Type、当前活动 tag、字段初始化与真实可变/父借用资格；不跨 Managed、
Address 或外部稳定存储界。跨 Package 字段复用普通拥有字段的实际初始化
与清理规则，不绕过需完整 teardown 的合法前缀限制。

部分转交后，父存储及未转交字段继续存在；父值 Take/完整传参要求所有
字段初始化。重复路径、前缀重叠及相交借入禁止再次转交，真正同级字段
保留分离证明。DropEnum/DropTuple 按当前活动 variant 和剩余初始化状态
各清理一次，退出/早返/循环与反方向分支不能漏清理或重复清理。两核各自
从实际 CFG 重建逐子树初始化，不读取 producer 权限声明；后端从实际
Store/Take/Drop 重建有界局部部分标记并使用真实基址。路径深度、每步
Type/tag 核验、状态复制/汇合、字段表、清理标记和 Wire 步骤在分配前
计入现有额度；不以候选 Plan 字段制造初始化事实。

Copy Tuple 的嵌套模式及条件接续：AddressField 只缩小真实 inline 字段区间，
保留父权限链，显式 ReborrowAddress 才建立子借入。if let/while let 的 ref
条件复用实际 tag/字段检查，条件临时值置于独立词法清理作用域；失败分支、
早返、break/continue 都清理仍拥有的临时父值，借入本身不复制拥有载荷。

本阶段实际完整请求额度：拥有 Tuple fixture N23399；已有 addressed.zil
从90036迁移至90248，反映新元组构造/字段位置处理，N-1及fresh/warm均核验。
unsigned/Option/local-reference 的9050/38035/18127保持。该迁移不修改正式性能门。
组合独审发现旧粗根判定会丢失 ParameterCapture 的 Apart 义务；只允许同一
实际 Address 的明确不相交 Window 区间免除该义务，符号捕获保留调用者代入核验。
impl187原有源码/原始Wire最近反例24/0重放通过。117/118仍为施工拒绝分支，
不将该完整Tuple阶段解释为部分移动或完整模式终态。

119/120混合模式落地保持全部测试先于借入/转交，真实剩余叶按§7声明顺序
清理，解构目标按既定逆序登记以保持字段清理次序。新长共同前缀的BTree
比较、join、clone及insert分别按路径长度与有界搜索因子预付。实际可写
Copy Enum endpoint及后续调用/写入使tag知识失效，但不抹掉拥有叶来源或
已移走子树标记；保留fresh-tag正例、过期tag/重复take/前缀/父take反例。

真实原源发现准备临时值跨嵌套语句的清理边界缺陷：match临时父仍被arm
局部依赖时，arm中的let不能清掉外层准备队列。每条语句保存已有临时值
下界，仅结束本语句新建的准备临时；Return/?等真实退出仍按词法作用域
清理所有活责任。命名父留到其词法边界，rvalue match父在arm局部清理后
于本完整表达式结束；失败模式只测试，不提前移动。该实现不增加运行时
分配、隐藏父副本或预付之外的新容器。后续126/127与121–124不计本阶段完成。

119/120本阶段验证：terminal340 28/0；IR60/0；Analyze Sum26/0；Verify
Sum16/0及borrow23/0；既有impl187原24/0；相关五个库Clippy通过。mixed fixture
与20个内联源码均执行Clang/API O0/O2（84次原生配置）；长共同前缀fixture仅
源码/两核预算，不宣称该大例原生执行。请求实测N2094103；addressed90248→90264，
完整Tuple23399→20265，后者直接逆序登记原子解构目标，消除再次发布绑定的
临时目标；unsigned9050/Option38035/local-ref18127保持。全部N/N−1及fresh/warm
重试成立，默认额度与正式性能门不变。

本阶段仍非完整模式终态：Product-root选择性转移、嵌套消费Package member、
直接ErrorUnion的TypeName(payload)源码解构、126/127真实地址混合路径和121–124
实际拥有字段replace继续施工。当前ErrorUnion两侧只证明真实剩余清理，不能
冒充直接解构已完成。精确阶段独审另行绑定；与主树Wire37/HRTB/Task/float组合
的验收由root执行。Rust四目标与突破的实证/未测区分见terminal-340收据。

126/127接续提案：实际Address作为SSA基址，使用与119/120完全一致的有界
Field/Variant/ErrorArm路径。126非空，端点真实父资格及mutable权限来自该
Address而非保存它的局部槽；127允许空路径只读最终头，ErrorUnion归一为
0 success/1 error。每个所经子树/判别必须仍活动，不能隐式跨Managed、
Address、Desc或外部稳定存储界。两核独立重建当前真实tag观察与全部祖先
标签、混合字段区间及父借入；Parameter/Capture/Result继续沿实际来源。
后端使用原始真实指针和每一步GEP/tag读取，不生成拥有父副本或包装。
新pass只在实际使用126/127时运行，路径/集合比较/状态分配继续先付费；
最近反例覆盖同级允许/重叠拒绝、过期guard、同后继假证明和共享输入伪造
mutable。公共编码由root发布；此提案不把121–124拥有replace提前记为完成。

实际双Package mutable sibling原源还暴露复合赋值的重复窗口误拒：a.n+=b.n
已经准备真实LHS地址后，不能再为同一个稳定a.n创建一个相交shared子借入。
对于有界、相同名字/字段/已稳定索引的整型旧值读，直接经已准备地址读取
再算RHS/写入；effectful位置不做此替换，原求值次序和一次索引求值保持。
这只消除重复投影，不给RHS其他alias或拥有replace授予两阶段权限。

126/127精确branch阶段：terminal340 34/0；IR60；Analyze Sum26；Verify Sum16、
borrow23；impl187原24；五库Clippy及impl030/042原复合位置求值各1/0。新增实际
地址源码28个Clang/API O0/O2配置；source N103790，fresh/warm保持；raw64观察
新lane三核N134446/247974/247974，旧lane41628/41013/41013，N−1与独立fresh重试
成立。旧地址pass同因预付/有序集合费用一并修复；branch local-ref迁为17540，
addressed迁为92292，其他9050/38035/20265/2094103保持。root261的组合边界另记，
不混同身份。实际Return ref及只捕获返回标量引用已运行；拥有基址捕获资格
按root明确派工在下一有限profile实施，原captured-owning-base-open.zil保留。
这不提升FFI、worker、Transfer/Share或源拥有值捕获资格；121–124公共补丁
仍仅提案。终态剩余及Rust四目标实证/未测界限见更新收据。
