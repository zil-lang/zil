# 029 枚举与穷尽匹配

沿既有 enum、构造与 Pattern 文法贯通有限 Copy 枚举。机器表示为真实 tag 加最大 payload 的对齐 union；每个 payload 读取都检查 tag，未匹配读取陷阱，不把未初始化数据交给 LLVM。普通函数参数、返回和嵌套 tuple/record/array 使用同一内联值表示，不装箱。

match 仅计算一次输入，按源码顺序匹配并绑定 payload；类型阶段必须证明穷尽并拒绝不可达重复分支。typed IR 的构造、tag 和 payload 投影由独立 verify 重新校验类型/下标；原始 IR、接口或指纹不能自行授权读 payload。CT 源走访覆盖 match，enum 执行仍须独立白名单，不自动授权。

本片首先覆盖 unit/tuple 变体与递归 Copy payload；未实施的 record pattern、资源 payload 和泛型枚举保持显式 Unsupported。Option 拼写不在本片新增。

新增显式 Terminator::Trap（raw wire tag 4），表示可达的检查失败；Unreachable 的可达拒绝保持。原生执行 llvm.trap，CT VM 返回 E-CT-TRAP，不使用宿主 panic。枚举三操作 tag 18/19/20，Type::Enum tag 15；接口类型摘要使用独立 tag 9，不改借入来源合同摘要。最终 schema 版本仍由集成方统一更新。
