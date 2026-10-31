# 231 既有名义trait的where正规化

依据 _zil_design.md §18.1 profile 231，将普通自由函数直接type binder上的已有名义trait内联约束等价开放到where。先在modules按真实模块解析主体与bound，保持当前函数绑定遮蔽；再用现有trait索引确认真实声明，正规化到既有泛型定义期/实例期检查。

普通无参、无supertrait及无关联项trait可与Copy和其它既有普通trait组合。名义trait重复保持现有内联拒绝，Copy重复沿229幂等。别名/普通类型不能冒充trait；StoreElement/Fn族、关联/投影/复合主体、lifetime/const、方法/impl/extern及泛型trait不开放。不新增schema/Wire、求解器、运行时能力或默认预算。

新增工作先收费；只在首次名义where路径惰性构建现有trait索引。无where与无新增模块解析的纯Copy路径保持原费用；跨模块where实际新解析允许记录旧/新费用及N/N−1/fresh。旧impl229 unknown-bound Marker与review229 separate-invalid-clause按新合同保留原文晋升，并以缺失/泛型trait补最近拒绝；原始历史证据不改。

主写集：expand generic_where.rs、modules.rs、generic_trait_bounds.rs（仅复用既有声明资格入口）及必要generics.rs名字接点；impl231测试、where_traits231消费者与本单证据。需非作者精确提交审查、跨模块公开/私有/缺失、遮蔽、无impl、未调用坏定义、内联等价及LLVM O0/O2对照。

首轮完整CI 41182685056确认review229_joint也含同一旧Marker拒绝边界：按本已冻结合同保留原文晋升，另补最近缺失/泛型trait拒绝；属于遗漏的旧输入迁移，不改变生产、预算或权限范围，原Fail保留。
