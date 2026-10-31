# 233 跨模块普通trait实现

基线为0a3d0021fa788a27c87df20454c34c338039f1c5，父任务B0，继续231及_zil_design.md §18.1。修复trait定义与本地Self实现分属真实模块时，接收者和静态方法索引仅查询本模块导致的有限实现缺口；不改变完整语言的名义所属或impl唯一性规则。

仅支持实际导入的公开普通无参trait和impl模块本地具体非泛型Struct/既有资源Self。真实签名继承及显式等价接收者、公开导出、名义身份、重复impl、字段/inherent碰撞和签名一致性必须贯通；资源保持共享接收者。泛型impl、关联项、方法泛型/where/默认体、私有trait及外来Self不由本片开放。模块改名后的Send/Sync/Drop/Fn族保留排除覆盖不经过泛型bound的静态trait路径，并保留合法近名。

复用现有Core、静态调用和泛型bound证据，不新增字典、权限、schema/Wire或默认预算；新工作在增长前收费。旧review231三模块trait/impl分离原字节输入按新合同晋升，并补最近私有、同名不同身份、外来Self、错误receiver、重复impl和未调用坏定义。必须核inline/where等价、N/N−1/fresh及O0/O2真实运行；无新模块工作的旧路径预算需保持。
