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
