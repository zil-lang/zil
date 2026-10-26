# 0063 编译成本与关系求解结构研究提案

本切片为独立研究，不改变生产接受性、默认预算、ABI或清理规则。依赖draft12/schema9，研究059的冻结历史保持原字节。063复制的研究实现沿用同一有限抽象，新增package/操作明确Unsupported；源基线、算法身份与数据单列，不把新数字重标为059。

## 待验证的假设

1. 059四独立guard失败可能由累计BDD临时节点而非真实活跃关系宽度主导。对阶段、事件、位方程、分配节点、完整持久根可达节点建立有界诊断。诊断开销与算法工作预算分别记录，不把分配节点等同RSS。
2. 固定语义坐标、事件、因子顺序和完整oracle输出，比较独立and/exists与融合relational product；再只调整BDD内部变量rank。无论成功或失败保留完整收据。
3. 图切割的合适边界是语义依赖与观察接口，不等同CFG切口。Rust/Zig的query缓存、惰性解析、具体实例化、增量机器码/链接各有不同成本；用实际阶段诊断选择接口。

## 实施与判定

沿用100M候选/100M oracle、300k BDD节点等原限额，诊断不可默默提高门槛。完整Canonical schema/事件/每原始点与fault/exit联合关系分别由raw oracle核对。只读影子研究不制造Verified权限。计时独占、源与工具链冻结、失败非Pass，默认判定NoGo入核，除非独立同精度完整成本证据支持新裁决。

编译阶段诊断和研究核性能分开报告。Rust样本是可复现的小型对照，不代表Rust生态总体瓶颈或产品级速度比。实验与理论技术保留成熟度，不把2026目标当稳定实现。

## 实验细化：安全边界回收与实际driver对照

保持300,000**物理arena槽位**上限，新增显式mark/sweep，仅在完整事件构造之后、完整worklist迭代发布状态/关系之后运行。存活NodeId不变，回收前清空全部apply缓存，unique表clear后按存活NodeId重建（避免retain遗留随机hash布局相关的tombstone容量）。完整事件根、states和relations共同作为根；plans只引用事件因子，递归及临时结果仍在使用时不回收。扫描、根集合、缓存/unique/free列表重建都计入候选100M共享Work。启用复用后累计created节点可以超过300k；这是明确的表示工程变体，不是假称保持旧累计创建限额。不开GC的原失败另存，不能抹去。

单独冻结生产70d3a729加唯一lazy-dump.patch：只把未请求的IR Debug输出格式化推迟到实际存在输出路径时，既有dump负责取消、冲突和原子发布；不改RawSnapshot::digest所需的缓存身份。真实public driver测cold/unchanged/comment/private-body-edit，Verify和native Build分列；请求emit时所有文本阶段产物核对原字节，实际native行为对照。补丁只存在本研究写集，生产吸收归root另单。

新增反例确认研究违规集合与生产接受集合不等同：条件清理合流的MOVED|DONE并非生产必拒；研究逐状态duplicate-cleanup关系仍保留。此观察不证明机器重复释放，不修改059历史合同。另有GC实际运行且生产确拒的确定重复清理对照。
